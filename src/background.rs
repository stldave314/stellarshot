// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot --background`: what runs scheduled backups inside a Flatpak
//! sandbox, where there are no systemd timers to do it (see
//! [`crate::timers`] for the native way).
//!
//! One long-running process per session: once a minute it runs, one at a
//! time and through the same `--scheduled` path a timer would, every timed
//! backup whose period has passed since it last ran, and every backup set
//! to run when its drive connects whose drive has just appeared. It starts
//! at login, asked for through the Background portal or, where the desktop
//! has none (COSMIC), an autostart entry written directly; and it ends by
//! itself once no backup is scheduled.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use crate::constants::{APP_ID, BACKGROUND_TICK, PORTAL_REQUEST_TIMEOUT};
use crate::core::config::StellarshotConfig;
use crate::debug::SCHED;
use crate::profile::{Destination, Profile, Schedule};
use crate::run_state::{self, RunState};
use crate::{debug_log, error_log, fl};

/// The argument that starts this mode.
pub const ARG: &str = "--background";

/// How long a timed schedule waits between runs, in seconds.
fn period(schedule: Schedule) -> Option<i64> {
    match schedule {
        Schedule::Manual | Schedule::OnConnect => None,
        Schedule::Hourly => Some(3_600),
        Schedule::Daily => Some(86_400),
        Schedule::Weekly => Some(7 * 86_400),
    }
}

/// When a backup last ran, as far as the run state knows: its last success
/// or its last failure, whichever is newer.
fn last_run(state: &RunState) -> Option<i64> {
    let failed = state.failure.as_ref().map(|failure| failure.time);
    state.last_success.max(failed)
}

/// Whether a backup on `schedule` is due at `now`, having last run (or
/// last been started by this process) at `last`. A backup that never ran
/// is due at once; one whose period passed while the computer was off runs
/// at the first chance, as a persistent systemd timer catches up.
fn due(schedule: Schedule, last: Option<i64>, now: i64) -> bool {
    match (period(schedule), last) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(period), Some(last)) => now.saturating_sub(last) >= period,
    }
}

/// When a backup on `schedule` that last ran at `last` runs next, at the
/// earliest: for the window to show, as it shows a timer's next run.
fn next_after(schedule: Schedule, last: Option<i64>, now: i64) -> Option<i64> {
    let period = period(schedule)?;
    Some(last.map_or(now, |last| last.saturating_add(period).max(now)))
}

/// When `id` runs next on its own, for the window. `None` for a backup that
/// is not on a timed schedule, or that cannot be found.
pub fn next_run(id: &str) -> Option<i64> {
    let (config, _) = StellarshotConfig::load();
    let profile = config.profile(id)?;
    let now = crate::core::format::now();
    next_after(profile.schedule, last_run(&run_state::load(id)), now)
}

/// The drives a backup is set to run on: removable drives' filesystem
/// UUIDs, for backups scheduled "when the drive connects".
fn on_connect(profiles: &[Profile]) -> Vec<(&str, &str)> {
    profiles
        .iter()
        .filter(|profile| profile.schedule == Schedule::OnConnect)
        .filter_map(|profile| match &profile.destination {
            Destination::Removable { uuid, .. } => Some((profile.id.as_str(), uuid.as_str())),
            _ => None,
        })
        .collect()
}

/// Run `id`'s scheduled backup the way a timer would, and wait for it.
fn run_scheduled(id: &str) {
    let program = match crate::exe::installed_path() {
        Ok(program) => program,
        Err(err) => {
            error_log!(
                SCHED,
                "background: cannot find this program to run {id}: {err}"
            );
            return;
        }
    };
    debug_log!(SCHED, "background: running {id}");
    match Command::new(program)
        .args(["--scheduled", id])
        .stdin(Stdio::null())
        .status()
    {
        Ok(status) => debug_log!(SCHED, "background: {id} ended: {status}"),
        Err(err) => error_log!(SCHED, "background: could not run {id}: {err}"),
    }
}

/// The lock that keeps this process the only one of its kind in the
/// session, held for as long as it runs. `None` if another one holds it.
fn single_instance() -> Option<std::fs::File> {
    let dir = crate::engine::lock::runtime_dir();
    if let Err(err) = crate::engine::lock::create_private_dir(&dir) {
        error_log!(SCHED, "background: {}: {err}", dir.display());
        return None;
    }
    let path = dir.join("background.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .inspect_err(|err| error_log!(SCHED, "background: {}: {err}", path.display()))
        .ok()?;
    file.try_lock().ok().map(|()| file)
}

/// Entry point for `stellarshot --background`.
pub fn main() -> ExitCode {
    crate::debug::init(crate::debug::Role::Background);
    crate::paths::tighten_app_dirs();
    crate::core::localization::init();
    let Some(_lock) = single_instance() else {
        debug_log!(SCHED, "background: already running");
        return ExitCode::SUCCESS;
    };
    debug_log!(SCHED, "background: started");
    // When this process last started each timed backup: a run that was
    // skipped (its conditions were not met, its destination was away)
    // records nothing, and must still wait a full period, as a timer's
    // skipped slot does, rather than be retried every minute.
    let mut started: HashMap<String, i64> = HashMap::new();
    // The drives present at the last look, so a backup runs once per
    // connection rather than for as long as its drive stays plugged in.
    let mut connected: HashSet<String> = HashSet::new();
    loop {
        let (config, unreadable) = StellarshotConfig::load();
        if !unreadable {
            if !config
                .profiles
                .iter()
                .any(|profile| profile.schedule != Schedule::Manual)
            {
                debug_log!(SCHED, "background: nothing is scheduled; ending");
                return ExitCode::SUCCESS;
            }
            for profile in &config.profiles {
                let now = crate::core::format::now();
                let last =
                    last_run(&run_state::load(&profile.id)).max(started.get(&profile.id).copied());
                if due(profile.schedule, last, now) {
                    started.insert(profile.id.clone(), now);
                    run_scheduled(&profile.id);
                }
            }
            let drives = on_connect(&config.profiles);
            if !drives.is_empty() {
                let present: HashSet<String> = crate::drives::mounted_drives()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|drive| drive.uuid)
                    .collect();
                for (id, uuid) in drives {
                    if present.contains(uuid) && !connected.contains(uuid) {
                        run_scheduled(id);
                    }
                }
                connected = present;
            }
        }
        std::thread::sleep(BACKGROUND_TICK);
    }
}

/// Start the background process now if it is not already running. A
/// second one finds the first's lock and ends at once.
fn start() {
    let Ok(program) = crate::exe::installed_path() else {
        return;
    };
    let spawned = {
        use std::os::unix::process::CommandExt;
        Command::new(program)
            .arg(ARG)
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    match spawned {
        // Reaped on a thread of its own, so it never lingers as a zombie
        // while this process runs.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(err) => error_log!(SCHED, "could not start the background process: {err}"),
    }
}

/// The autostart entry the Background portal itself would write, and this
/// writes where there is no portal. One name, so the two never both exist.
fn autostart_path() -> Option<PathBuf> {
    crate::paths::home_dir().map(|home| {
        home.join(".config/autostart")
            .join(format!("{APP_ID}.desktop"))
    })
}

/// What the autostart entry says.
fn autostart_text() -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment={comment}\n\
         Exec=flatpak run --command=stellarshot {APP_ID} {ARG}\n\
         X-Flatpak={APP_ID}\n\
         NoDisplay=true\n\
         X-GNOME-Autostart-enabled=true\n",
        name = fl!("stellarshot"),
        comment = fl!("background-reason"),
    )
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Background",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait Background {
    fn request_background(
        &self,
        parent_window: &str,
        options: HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.portal.Request",
    default_service = "org.freedesktop.portal.Desktop"
)]
trait Request {
    #[zbus(signal)]
    fn response(
        &self,
        response: u32,
        results: HashMap<String, zbus::zvariant::OwnedValue>,
    ) -> zbus::Result<()>;
}

/// Ask the Background portal to start this program at login (`autostart`)
/// or to stop doing so. `Ok(true)` when the portal agreed, `Ok(false)` when
/// the user said no, `Err` when there is no such portal to ask.
async fn ask_portal(autostart: bool) -> zbus::Result<bool> {
    use futures_util::StreamExt;
    let connection = zbus::Connection::session().await?;
    let token = format!("stellarshot{}", uuid::Uuid::new_v4().simple());
    // The request's object path is known before asking, so its answer
    // cannot arrive before anything is listening for it.
    let sender = connection
        .unique_name()
        .map(|name| name.trim_start_matches(':').replace('.', "_"))
        .unwrap_or_default();
    let path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = RequestProxy::builder(&connection)
        .path(path)?
        .build()
        .await?;
    let mut responses = request.receive_response().await?;
    let portal = BackgroundProxy::new(&connection).await?;
    let reason = fl!("background-reason");
    let commandline = vec!["stellarshot", ARG];
    let options = HashMap::from([
        ("handle_token", zbus::zvariant::Value::from(token.as_str())),
        ("reason", zbus::zvariant::Value::from(reason.as_str())),
        ("autostart", zbus::zvariant::Value::from(autostart)),
        ("commandline", zbus::zvariant::Value::from(commandline)),
    ]);
    portal.request_background("", options).await?;
    let answer = tokio::time::timeout(PORTAL_REQUEST_TIMEOUT, responses.next())
        .await
        .map_err(|_| zbus::Error::Failure("no answer from the Background portal".into()))?
        .ok_or_else(|| zbus::Error::Failure("the Background portal went away".into()))?;
    let args = answer.args()?;
    let granted = args.response == 0
        && args
            .results
            .get("autostart")
            .and_then(|value| bool::try_from(value).ok())
            .unwrap_or(false)
            == autostart;
    Ok(granted || (args.response == 0 && !autostart))
}

/// [`ask_portal`] from a plain thread.
fn portal(autostart: bool) -> zbus::Result<bool> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| zbus::Error::Failure(err.to_string()))?
        .block_on(ask_portal(autostart))
}

/// Make sure scheduled backups run: at login from now on, and from now.
/// Through the Background portal where there is one; where there is none,
/// by writing the autostart entry it would have written.
pub fn schedule_changed() -> Result<(), String> {
    match portal(true) {
        Ok(true) => {}
        Ok(false) => return Err(fl!("background-refused")),
        Err(err) => {
            debug_log!(
                SCHED,
                "no Background portal ({err}); writing the autostart entry"
            );
            let path = autostart_path().ok_or("no home directory")?;
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
            }
            let text = autostart_text();
            if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                atomicwrites::AtomicFile::new(&path, atomicwrites::AllowOverwrite)
                    .write(|file| std::io::Write::write_all(file, text.as_bytes()))
                    .map_err(|err| format!("{}: {err}", path.display()))?;
            }
        }
    }
    start();
    Ok(())
}

/// No backup is scheduled any more: stop starting at login. The running
/// process notices by itself and ends.
pub fn nothing_scheduled() -> Result<(), String> {
    if let Err(err) = portal(false) {
        debug_log!(
            SCHED,
            "no Background portal ({err}); removing the autostart entry"
        );
    }
    let Some(path) = autostart_path() else {
        return Ok(());
    };
    // Only an entry that is this program's own.
    match std::fs::read_to_string(&path) {
        Ok(text) if text.contains(&format!("X-Flatpak={APP_ID}\n")) => {
            std::fs::remove_file(&path).map_err(|err| format!("{}: {err}", path.display()))
        }
        _ => Ok(()),
    }
}

/// Start `id`'s scheduled run now, as the background process would.
pub fn start_now(id: &str) -> Result<(), String> {
    if !crate::profile::valid_id(id) {
        return Err(format!("not a backup ID: {id:?}"));
    }
    let program = crate::exe::installed_path().map_err(|err| err.to_string())?;
    let mut child = {
        use std::os::unix::process::CommandExt;
        Command::new(program)
            .args(["--scheduled", id])
            .process_group(0)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|err| err.to_string())?
    };
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_state::Failure;

    #[test]
    fn a_timed_backup_is_due_once_its_period_has_passed() {
        let now = 1_000_000;
        assert!(due(Schedule::Daily, None, now), "never ran: due at once");
        assert!(!due(Schedule::Daily, Some(now - 3_600), now));
        assert!(due(Schedule::Daily, Some(now - 86_400), now));
        assert!(due(Schedule::Hourly, Some(now - 3_600), now));
        assert!(!due(Schedule::Weekly, Some(now - 6 * 86_400), now));
        assert!(
            due(Schedule::Weekly, Some(now - 30 * 86_400), now),
            "caught up after time off"
        );
        assert!(!due(Schedule::Manual, None, now));
        assert!(!due(Schedule::OnConnect, None, now));
    }

    #[test]
    fn the_last_run_is_the_newer_of_success_and_failure() {
        let mut state = RunState {
            last_success: Some(100),
            ..RunState::default()
        };
        assert_eq!(last_run(&state), Some(100));
        state.failure = Some(Failure {
            time: 250,
            stage: crate::run_state::Stage::default(),
            kind: crate::engine::ErrorKind::Internal,
            detail: String::new(),
        });
        assert_eq!(last_run(&state), Some(250));
        assert_eq!(last_run(&RunState::default()), None);
    }

    #[test]
    fn the_next_run_is_a_period_after_the_last_and_never_in_the_past() {
        let now = 1_000_000;
        assert_eq!(
            next_after(Schedule::Daily, Some(now - 3_600), now),
            Some(now - 3_600 + 86_400)
        );
        assert_eq!(
            next_after(Schedule::Daily, Some(now - 90_000), now),
            Some(now)
        );
        assert_eq!(next_after(Schedule::Hourly, None, now), Some(now));
        assert_eq!(next_after(Schedule::Manual, Some(now), now), None);
    }

    #[test]
    fn the_autostart_entry_runs_this_flatpak_in_the_background() {
        let text = autostart_text();
        assert!(text.contains(&format!(
            "Exec=flatpak run --command=stellarshot {APP_ID} {ARG}\n"
        )));
        assert!(text.contains(&format!("X-Flatpak={APP_ID}\n")));
    }
}
