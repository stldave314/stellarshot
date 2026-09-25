// SPDX-License-Identifier: GPL-3.0-only

//! The work behind the window: blocking engine calls moved off the UI thread,
//! the file chooser, and the size estimate as a stream.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cosmic::dialog::file_chooser;
use cosmic::iced::futures::{SinkExt, Stream, channel::mpsc};

use crate::app::portal::url_to_path;
use crate::app::wizard::{EstimateEvent, Mode};
use crate::debug::UI;
use crate::debug_log;
use crate::engine::{self, BackupRequest, EngineError, ErrorKind, Probe, Secret};
use crate::keyring;
use crate::profile::{Destination, Profile};

/// Run blocking engine work off the UI thread.
pub async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, EngineError> + Send + 'static,
) -> Result<T, EngineError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or_else(|err| Err(EngineError::new(ErrorKind::Internal, err.to_string())))
}

/// Ask for one or more folders. Cancelling returns an empty list.
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

pub async fn probe(destination: Destination) -> Result<Probe, EngineError> {
    blocking(move || engine::probe(&destination.location()?)).await
}

/// Open a profile's repository and list its snapshots, remembering the
/// password if asked and it worked.
pub async fn open(
    profile: Profile,
    secret: Secret,
    remember: bool,
) -> Result<Vec<engine::SnapshotSummary>, EngineError> {
    let location = profile.location()?;
    let key = secret.clone();
    let snapshots = blocking(move || engine::open(&location, &key)?.snapshots()).await?;
    if remember {
        // A keyring that refuses is not a reason to fail the unlock; the
        // password simply is not remembered.
        let _ = keyring::store(&profile.id, &profile.name, &secret).await;
    }
    Ok(snapshots)
}

pub async fn snapshots(
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

/// Ask where to save an export, with `title` for the dialog. `Ok(None)` if
/// the user cancelled.
pub async fn choose_export_path(title: String) -> Option<PathBuf> {
    match file_chooser::save::Dialog::new()
        .title(title)
        .file_name("stellarshot-settings.ron".to_owned())
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
/// user cancelled.
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

/// Write `text` to `path`, off the UI thread.
pub async fn write_file(path: PathBuf, text: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || std::fs::write(&path, text).map_err(|err| err.to_string()))
        .await
        .map_err(|err| err.to_string())?
}

/// Read and parse a settings export, off the UI thread.
pub async fn read_export(path: PathBuf) -> Result<crate::settings_export::Export, String> {
    tokio::task::spawn_blocking(move || {
        let text = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
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

/// Create or open the repository behind a finished wizard, and remember the
/// password if asked. When opening, the profile's sources come from the
/// latest snapshot, so an existing backup carries on as it was.
pub async fn finish(
    mode: Mode,
    mut profile: Profile,
    secret: Option<Secret>,
    remember: bool,
) -> Result<Finished, EngineError> {
    let snapshots = match (&mode, &secret) {
        (Mode::Create, Some(secret)) => {
            let location = profile.location()?;
            let key = secret.clone();
            blocking(move || engine::init(&location, &key).map(drop)).await?;
            Vec::new()
        }
        (Mode::Open, Some(secret)) => {
            let location = profile.location()?;
            let key = secret.clone();
            let snapshots = blocking(move || engine::open(&location, &key)?.snapshots()).await?;
            if let Some(latest) = snapshots.first() {
                profile.sources = latest.paths.iter().map(PathBuf::from).collect();
                profile.last_success = Some(latest.time);
            }
            snapshots
        }
        _ => Vec::new(),
    };
    if let (true, Some(secret)) = (remember, &secret) {
        let _ = keyring::store(&profile.id, &profile.name, secret).await;
    }
    Ok(Finished {
        mode,
        profile,
        secret,
        snapshots,
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
    cosmic::iced::stream::channel(16, move |mut out: mpsc::Sender<EstimateEvent>| async move {
        let (progress_tx, mut progress_rx) = mpsc::channel::<EstimateEvent>(16);
        let worker = tokio::task::spawn_blocking(move || {
            let mut tx = progress_tx.clone();
            let result = engine::estimate(&request, &cancel, &mut |total| {
                let _ = tx.try_send(EstimateEvent::Progress(total));
            });
            // Interim totals may be dropped when the UI is behind; these may
            // not, so they wait for room in the channel.
            let mut tx = progress_tx;
            let mut deliver = |event| {
                let _ = cosmic::iced::futures::executor::block_on(tx.send(event));
            };
            match result {
                Ok(Some(total)) => {
                    deliver(EstimateEvent::Done(total));
                    match engine::exclusion_breakdown(&request, &exclude_folders, &cancel) {
                        Ok(Some(breakdown)) => {
                            deliver(EstimateEvent::Breakdown(exclude_folders, breakdown));
                        }
                        Ok(None) => {}
                        Err(err) => deliver(EstimateEvent::Failed(err.detail)),
                    }
                }
                Ok(None) => {}
                Err(err) => deliver(EstimateEvent::Failed(err.detail)),
            }
        });
        use cosmic::iced::futures::StreamExt;
        while let Some(event) = progress_rx.next().await {
            let _ = out.send(event).await;
        }
        let _ = worker.await;
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
    fn finishing_create_makes_the_repository() {
        let dir = TempDir::new().unwrap();
        let repo = dir.path().join("repo");

        let finished = block_on(finish(
            Mode::Create,
            profile(&repo, vec![dir.path().to_path_buf()]),
            Some(Secret::new("pw")),
            false,
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
        let finished = block_on(finish(
            Mode::Open,
            profile(&repo, Vec::new()),
            Some(secret),
            false,
        ))
        .unwrap();

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
            false,
        ));

        assert_eq!(result.unwrap_err().kind, ErrorKind::WrongPassword);
    }
}
