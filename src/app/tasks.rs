// SPDX-License-Identifier: GPL-3.0-only

//! The work behind the window: blocking engine calls moved off the UI thread,
//! the file chooser, and the size estimate as a stream.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cosmic::dialog::file_chooser;
use cosmic::iced::futures::{SinkExt, Stream, channel::mpsc};

use crate::app::pages::profile::SizeEstimateEvent;
use crate::app::portal::url_to_path;
use crate::app::wizard::browse;
use crate::app::wizard::{EstimateEvent, Mode};
use crate::debug::UI;
use crate::debug_log;
use crate::engine::{self, BackupRequest, EngineError, ErrorKind, Probe, Secret};
use crate::profile::{Destination, Profile};

/// Run blocking engine work off the UI thread.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, EngineError> + Send + 'static,
) -> Result<T, EngineError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|err| Err(EngineError::new(ErrorKind::Internal, err.to_string())))
}

/// Ask for one or more folders. Canceling returns an empty list.
pub async fn pick_folders(title: String) -> Vec<PathBuf> {
    match file_chooser::open::Dialog::new()
        .title(title)
        .open_folders()
        .await
    {
        Ok(response) => response.urls().iter().filter_map(url_to_path).collect(),
        Err(err) => {
            debug_log!(UI, "folder chooser: {err}");
            Vec::new()
        }
    }
}

/// Ask for a single folder.
pub async fn pick_folder(title: String) -> Option<PathBuf> {
    match file_chooser::open::Dialog::new()
        .title(title)
        .open_folder()
        .await
    {
        Ok(response) => url_to_path(response.url()),
        Err(err) => {
            debug_log!(UI, "folder chooser: {err}");
            None
        }
    }
}

/// Ask for a single existing file.
pub async fn pick_file(title: String) -> Option<PathBuf> {
    match file_chooser::open::Dialog::new()
        .title(title)
        .open_file()
        .await
    {
        Ok(response) => url_to_path(response.url()),
        Err(err) => {
            debug_log!(UI, "file chooser: {err}");
            None
        }
    }
}

pub async fn probe(destination: Destination) -> Result<Probe, EngineError> {
    blocking(move || engine::probe(&destination.location()?)).await
}

/// Open a profile's repository and list its snapshots. Remembering the
/// password, if asked, is the caller's own separate task: a keyring failure
/// is not a reason to fail the unlock, and is reported on its own, not
/// folded into whether opening the repository succeeded.
pub async fn open(
    profile: Profile,
    secret: Secret,
) -> Result<Vec<engine::SnapshotSummary>, EngineError> {
    let location = profile.location()?;
    blocking(move || engine::open(&location, &secret)?.snapshots()).await
}

/// Read every index file and list the destination: worth asking for, not
/// fetching on its own.
pub async fn statistics(
    profile: Profile,
    secret: Secret,
) -> Result<engine::Statistics, EngineError> {
    let location = profile.location()?;
    blocking(move || engine::open(&location, &secret)?.statistics()).await
}

/// A backup's history, off the UI thread: it is a config read like any
/// other, but every other one already goes through here.
pub async fn history(profile_id: String) -> Vec<crate::event_log::Event> {
    blocking(move || Ok::<_, EngineError>(crate::event_log::load(&profile_id)))
        .await
        .unwrap_or_default()
}

/// Every backup's settings and history, as text ready to write out.
pub async fn export_settings(profiles: Vec<Profile>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        crate::settings_export::Export::collect(&profiles).to_text()
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Ask where to save a file named `file_name`, with `title` for the dialog.
/// `None` if the user canceled.
pub async fn choose_save_path(title: String, file_name: String) -> Option<PathBuf> {
    match file_chooser::save::Dialog::new()
        .title(title)
        .file_name(file_name)
        .save_file()
        .await
    {
        Ok(response) => response.url().and_then(url_to_path),
        Err(err) => {
            debug_log!(UI, "save-file chooser: {err}");
            None
        }
    }
}

/// Ask for a file to import, with `title` for the dialog. `Ok(None)` if the
/// user canceled.
pub async fn choose_import_path(title: String) -> Option<PathBuf> {
    match file_chooser::open::Dialog::new()
        .title(title)
        .open_file()
        .await
    {
        Ok(response) => url_to_path(response.url()),
        Err(err) => {
            debug_log!(UI, "open-file chooser: {err}");
            None
        }
    }
}

/// Write `text` to `path`, off the UI thread. Atomic and fsynced (see
/// `timers::write_if_changed`'s own doc comment for why a plain
/// `fs::write` is not enough), so an interrupted export leaves either the
/// old file or the new one, never an empty one.
///
/// Used only for a settings export today, which can carry a hook's command
/// line (and so, indirectly, whatever credentials that command needs, such
/// as `mysqldump -pX`), so the file is owner-only from the moment it
/// exists, whatever mode a shared umask would otherwise have given it.
/// The mode goes on the temporary file at creation (`write_with_options`,
/// as for any file holding a secret), not as a
/// `set_permissions` after the rename: that would leave a window, between
/// the temporary file being written and the mode being fixed up, in which
/// another user could read those commands — and a failed fix-up would
/// leave the file at the umask's mode for good.
pub async fn write_file(path: PathBuf, text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true).mode(0o600);
        atomicwrites::AtomicFile::new(&path, atomicwrites::AllowOverwrite)
            .write_with_options(|file| file.write_all(text.as_bytes()), options)
            .map_err(|err| std::io::Error::from(err).to_string())
    })
    .await
    .map_err(|err| err.to_string())?
}

/// Read and parse a settings export, off the UI thread. Reads at most
/// `MAX_EXPORT_BYTES` (plus one, to tell "exactly at the limit" from
/// "past it") rather than the whole file: the path came from a file
/// chooser, and a wrong pick could be anything at all.
pub async fn read_export(path: PathBuf) -> Result<crate::settings_export::Export, String> {
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let file = std::fs::File::open(&path).map_err(|err| err.to_string())?;
        let mut text = String::new();
        file.take(crate::constants::MAX_EXPORT_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|err| err.to_string())?;
        if text.len() as u64 > crate::constants::MAX_EXPORT_BYTES {
            return Err(format!(
                "{} is larger than {} bytes, which no settings export is",
                path.display(),
                crate::constants::MAX_EXPORT_BYTES
            ));
        }
        crate::settings_export::Export::from_text(&text)
    })
    .await
    .map_err(|err| err.to_string())?
}

/// What finishing the wizard produced.
#[derive(Debug, Clone)]
pub struct Finished {
    pub mode: Mode,
    pub profile: Profile,
    pub secret: Option<Secret>,
    pub snapshots: Vec<engine::SnapshotSummary>,
}

/// Create or open the repository behind a finished wizard. When opening,
/// the profile's sources come from the latest snapshot, so an existing
/// backup carries on as it was. Remembering the password, if asked, is the
/// caller's own separate task; see [`open`].
pub async fn finish(
    mode: Mode,
    mut profile: Profile,
    secret: Option<Secret>,
) -> Result<Finished, EngineError> {
    let snapshots = match (&mode, &secret) {
        (Mode::Create, Some(secret)) => {
            let location = profile.location()?;
            let key = secret.clone();
            let append_only = profile.append_only;
            let compression = profile.compression.level();
            blocking(move || {
                engine::init_with(&location, &key, append_only, compression).map(drop)
            })
            .await?;
            Vec::new()
        }
        (Mode::Open, Some(secret)) => {
            let location = profile.location()?;
            let key = secret.clone();
            // Read back whether the repository is append-only rather than
            // trusting the wizard's own toggle, which only exists for
            // `Mode::Create` — opening one made append-only by another
            // Stellarshot, or by `rustic`/`restic` directly, must still be
            // recognized as such, or Clean Up Now and pinning would be
            // offered and then refused by rustic itself, and a schedule
            // would keep attempting a forget that can never succeed.
            let (append_only, snapshots) = blocking(move || {
                let repo = engine::open(&location, &key)?;
                Ok((repo.is_append_only(), repo.snapshots()?))
            })
            .await?;
            profile.append_only = append_only;
            if let Some(latest) = snapshots.first() {
                profile.sources = latest.paths.iter().map(PathBuf::from).collect();
                profile.last_success = Some(latest.time);
            }
            snapshots
        }
        _ => Vec::new(),
    };
    Ok(Finished {
        mode,
        profile,
        secret,
        snapshots,
    })
}

/// Run `work` on a blocking thread and hand what it produces to the UI as a
/// stream. `work` gets two senders: `interim`, for progress that may be
/// dropped when the UI is behind, and `deliver`, for results that may not, so
/// it waits for room in the channel.
fn stream_blocking<E: Send + 'static>(
    work: impl FnOnce(&mut dyn FnMut(E), &mut dyn FnMut(E)) + Send + 'static,
) -> impl Stream<Item = E> {
    cosmic::iced::stream::channel(16, move |mut out: mpsc::Sender<E>| async move {
        let (tx, mut rx) = mpsc::channel::<E>(16);
        let worker = tokio::task::spawn_blocking(move || {
            let mut interim_tx = tx.clone();
            let mut interim = move |event| {
                let _ = interim_tx.try_send(event);
            };
            let mut tx = tx;
            let mut deliver = move |event| {
                let _ = cosmic::iced::futures::executor::block_on(tx.send(event));
            };
            work(&mut interim, &mut deliver);
        });
        use cosmic::iced::futures::StreamExt;
        while let Some(event) = rx.next().await {
            let _ = out.send(event).await;
        }
        let _ = worker.await;
    })
}

/// Size what `request` covers, then what its exclusions take out, including
/// each folder in `exclude_folders`, as a stream of events. Stops early when
/// `cancel` is set.
pub fn estimate(
    request: BackupRequest,
    exclude_folders: Vec<PathBuf>,
    cancel: Arc<AtomicBool>,
) -> impl Stream<Item = EstimateEvent> {
    stream_blocking(move |interim, deliver| {
        let result = engine::estimate(&request, &cancel, &mut |total| {
            interim(EstimateEvent::Progress(total));
        });
        match result {
            Ok(Some(total)) => {
                deliver(EstimateEvent::Done(total));
                match engine::exclusion_breakdown(&request, &exclude_folders, &cancel) {
                    Ok(Some(breakdown)) => {
                        deliver(EstimateEvent::Breakdown(exclude_folders, breakdown));
                    }
                    Ok(None) => {}
                    Err(err) => deliver(EstimateEvent::Failed(err)),
                }
            }
            Ok(None) => {}
            Err(err) => deliver(EstimateEvent::Failed(err)),
        }
    })
}

/// Size what `request` covers, as a stream of events: the same local walk
/// [`estimate`] does, without the exclusion-arithmetic pass, for an
/// already-configured backup that has no wizard-style exclude folder list to
/// size separately.
pub fn estimate_size(
    request: BackupRequest,
    cancel: Arc<AtomicBool>,
) -> impl Stream<Item = SizeEstimateEvent> {
    stream_blocking(move |interim, deliver| {
        let result = engine::estimate(&request, &cancel, &mut |total| {
            interim(SizeEstimateEvent::Progress(total));
        });
        match result {
            Ok(Some(total)) => deliver(SizeEstimateEvent::Done(total)),
            Ok(None) => {}
            Err(err) => deliver(SizeEstimateEvent::Failed(err)),
        }
    })
}

/// List `dir`'s immediate children with their sizes, as a stream of wizard
/// browse events. `cancel` stops the walk early: the wizard sets it when the
/// browser is closed or reopened, or the wizard is discarded.
pub fn browse_folder(dir: PathBuf, cancel: Arc<AtomicBool>) -> impl Stream<Item = browse::Message> {
    stream_blocking(move |interim, deliver| {
        let mut scanned = 0usize;
        let result = engine::list_with_sizes(&dir, &cancel, &mut |_entry| {
            scanned += 1;
            interim(browse::Message::Progress(dir.clone(), scanned));
        });
        match result {
            Ok(Some(entries)) => deliver(browse::Message::Listed(dir, entries)),
            Ok(None) => {}
            Err(err) => deliver(browse::Message::Failed(dir, err)),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Location, NoProgress};
    use crate::profile::Destination;
    use tempfile::TempDir;

    fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    fn profile(repo: &std::path::Path, sources: Vec<PathBuf>) -> Profile {
        Profile::new(
            "Test".into(),
            Destination::Local {
                path: repo.to_path_buf(),
            },
            sources,
        )
    }

    #[test]
    fn an_exported_settings_file_is_written_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("export.ron");

        block_on(write_file(path.clone(), "(profiles: [])".into())).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "an export can carry a hook's own credentials");
    }

    #[test]
    fn finishing_create_makes_the_repository() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");

        let finished = block_on(finish(
            Mode::Create,
            profile(&repo, vec![dir.path().to_path_buf()]),
            Some(Secret::new("pw")),
        ))
        .unwrap();

        assert!(crate::engine::location::is_repository(&repo));
        assert!(finished.snapshots.is_empty());
    }

    #[test]
    fn finishing_open_takes_the_folders_from_the_latest_snapshot() {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("documents");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a.txt"), b"a").unwrap();
        let repo = dir.path().join("repo");
        let secret = Secret::new("pw");
        engine::init(&Location::local(&repo), &secret).unwrap();
        engine::open(&Location::local(&repo), &secret)
            .unwrap()
            .backup(
                &BackupRequest {
                    sources: vec![source.clone()],
                    ..BackupRequest::default()
                },
                Arc::new(NoProgress),
            )
            .unwrap();

        // Opening knows nothing about what was backed up; it finds out.
        let finished =
            block_on(finish(Mode::Open, profile(&repo, Vec::new()), Some(secret))).unwrap();

        assert_eq!(finished.profile.sources, vec![source]);
        assert!(finished.profile.last_success.is_some());
        assert_eq!(finished.snapshots.len(), 1);
    }

    #[test]
    fn finishing_open_with_the_wrong_password_fails() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        engine::init(&Location::local(&repo), &Secret::new("pw")).unwrap();

        let result = block_on(finish(
            Mode::Open,
            profile(&repo, Vec::new()),
            Some(Secret::new("wrong")),
        ));

        assert_eq!(result.unwrap_err().kind, ErrorKind::WrongPassword);
    }
}
