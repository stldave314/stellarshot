// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot --scheduled <id>`, run the way its systemd timer runs it:
//! settings read from cosmic-config, the password from the keyring, the
//! outcome recorded in cosmic-config's state store.
//!
//! Settings, state and runtime files live in a temporary directory. The
//! password goes into the real Secret Service under a throwaway profile ID
//! and is removed again, so this needs a running, unlocked keyring (as
//! `tests/keyring.rs` does) and fails, rather than skips, without one.
//!
//! **No test here may reach a real failure or overdue notification.** The
//! keyring lookup and the notification both go over the same D-Bus session
//! bus; overriding `XDG_RUNTIME_DIR` alone does not stop a notification
//! from reaching a real desktop's notification daemon if the tests happen
//! to run in a real session (as they do outside CI), and removing
//! `DBUS_SESSION_BUS_ADDRESS` to stop that also breaks the keyring lookup
//! every test here depends on. A test was tried once with a scenario that
//! reached `notify::failure`: it sent a real notification and then waited
//! on it for several minutes before being killed by hand. Test the
//! decision to notify as a pure function instead (`scheduled.rs`'s own
//! `overdue_notification_due`), and leave the desktop notification itself
//! to manual verification, alongside the failure notification's own entry
//! in `VALIDATION.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

use stellarshot::engine::{self, Location, Secret};
use stellarshot::keyring;
use tempfile::TempDir;

const APP_ID: &str = "io.github.stldave314.Stellarshot";
const PASSWORD: &str = "correct horse battery staple";

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// A profile ID no real profile will have, with its password in the
/// keyring until dropped.
struct Remembered(String);

impl Remembered {
    fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        runtime()
            .block_on(keyring::store(
                &id,
                "Scheduled test",
                &Secret::new(PASSWORD),
            ))
            .expect("a Secret Service must be running and unlocked for this test");
        Self(id)
    }
}

impl Drop for Remembered {
    fn drop(&mut self) {
        let _ = runtime().block_on(keyring::forget(&self.0));
    }
}

struct Home {
    dir: TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        for sub in ["config", "state", "runtime"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        Self { dir }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// Save one daily profile backing up `source` to `repository`.
    fn save_profile(&self, id: &str, source: &Path, repository: &Path) {
        let settings = self.path("config").join(format!("cosmic/{APP_ID}/v2"));
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(
            settings.join("profiles"),
            format!(
                r#"[
    (
        id: "{id}",
        name: "Scheduled test",
        destination: Local(path: "{}"),
        sources: ["{}"],
        schedule: Daily,
        retention: Smart,
    ),
]"#,
                repository.display(),
                source.display()
            ),
        )
        .unwrap();
    }

    fn run(&self, id: &str) -> std::process::Output {
        Command::new(env!("CARGO_BIN_EXE_stellarshot"))
            .args(["--scheduled", id])
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .output()
            .unwrap()
    }

    /// The recorded run state, as text.
    fn run_state(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(
            self.path("state")
                .join(format!("cosmic/{APP_ID}/v2/run-{id}")),
        )
        .ok()
    }

    /// The recorded event log, as text.
    fn event_log(&self, id: &str) -> Option<String> {
        std::fs::read_to_string(
            self.path("state")
                .join(format!("cosmic/{APP_ID}/v2/event-log-{id}")),
        )
        .ok()
    }
}

#[test]
fn a_scheduled_backup_runs_checks_and_is_recorded() {
    let home = Home::new();
    let source = home.path("source");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("notes.txt"), b"hello").unwrap();
    let repository = home.path("repo");
    let location = Location::local(&repository);
    engine::init(&location, &Secret::new(PASSWORD)).unwrap();
    let profile = Remembered::new();
    home.save_profile(&profile.0, &source, &repository);

    let output = home.run(&profile.0);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let snapshots = engine::open(&location, &Secret::new(PASSWORD))
        .unwrap()
        .snapshots()
        .unwrap();
    assert_eq!(snapshots.len(), 1, "one snapshot from the scheduled run");
    let state = home.run_state(&profile.0).expect("the run is recorded");
    assert!(state.contains("last_success: Some("), "{state}");
    assert!(
        state.contains("last_check: Some("),
        "the first run checks the repository: {state}"
    );
    assert!(state.contains("failure: None"), "{state}");
    assert!(state.contains("damaged: false"), "{state}");

    let events = home.event_log(&profile.0).expect("the run is logged");
    assert!(
        events.contains("BackedUp"),
        "the backup is in the history: {events}"
    );
    assert!(
        events.contains("Checked"),
        "the first run's check is in the history: {events}"
    );
}

#[test]
fn an_unplugged_destination_is_skipped_quietly() {
    let home = Home::new();
    let source = home.path("source");
    std::fs::create_dir_all(&source).unwrap();
    let profile = Remembered::new();
    // A drive that is not there: the repository's parent does not exist.
    home.save_profile(&profile.0, &source, &home.path("unplugged/drive/repo"));

    let output = home.run(&profile.0);

    assert!(
        output.status.success(),
        "a missing drive is not a failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let recorded_failure = home
        .run_state(&profile.0)
        .is_some_and(|state| state.contains("failure: Some("));
    assert!(
        !recorded_failure,
        "nothing to report; the next slot retries"
    );
    let events = home.event_log(&profile.0).expect("the skip is logged");
    assert!(
        events.contains("Skipped"),
        "quiet does not mean invisible: {events}"
    );
}
