// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! Removing a backup deletes its run state and history from the state
//! store, which cosmic-config itself has no way to do (see
//! `paths::remove_state_key`). Its own binary, with `XDG_STATE_HOME` pointed
//! at a fresh temporary directory, for the same reasons as
//! `tests/settings_export_history.rs`: the variable is process-wide, and the
//! developer's real state store must never be touched.

use std::path::{Path, PathBuf};

use stellarshot::event_log::{self, EventKind, Source};
use stellarshot::run_state::{self, RunState};

/// Every file under `dir` named `name`, however deep.
fn find(dir: &Path, name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(find(&path, name));
        } else if path.file_name().is_some_and(|file| file == name) {
            found.push(path);
        }
    }
    found
}

#[test]
fn a_removed_backups_state_and_history_are_deleted() {
    let state = tempfile::TempDir::new().unwrap().keep();
    // SAFETY: this is the only test in this binary, and nothing here spawns
    // another thread that could read `XDG_STATE_HOME` mid-change.
    unsafe {
        std::env::set_var("XDG_STATE_HOME", &state);
    }
    let id = uuid::Uuid::new_v4().to_string();
    let other = uuid::Uuid::new_v4().to_string();
    for profile in [&id, &other] {
        run_state::save(
            profile,
            &RunState {
                last_success: Some(1),
                ..RunState::default()
            },
        )
        .unwrap();
        event_log::record(profile, 1, EventKind::BackedUp, Source::Desktop);
    }
    let run_key = format!("run-{id}");
    let log_key = format!("event-log-{id}");
    // The test must actually be looking at the store, or its "gone" below
    // would pass for nothing.
    assert_eq!(find(&state, &run_key).len(), 1, "run state saved");
    assert_eq!(find(&state, &log_key).len(), 1, "history saved");

    run_state::remove(&id).unwrap();
    event_log::remove(&id).unwrap();

    assert!(find(&state, &run_key).is_empty(), "run state deleted");
    assert!(find(&state, &log_key).is_empty(), "history deleted");
    assert_eq!(run_state::load(&id), RunState::default());
    assert!(event_log::load(&id).is_empty());
    // Only that backup's.
    assert_eq!(run_state::load(&other).last_success, Some(1));
    assert_eq!(event_log::load(&other).len(), 1);
    // Removing what is already gone is not an error.
    run_state::remove(&id).unwrap();
    let _ = std::fs::remove_dir_all(&state);
}
