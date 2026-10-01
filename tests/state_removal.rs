// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! Removing a backup deletes its run state and history from the state
//! store, which cosmic-config itself has no way to do (see
//! `paths::remove_state_key`), and a status that cannot be read can be
//! reset. Its own binary, with `XDG_STATE_HOME` pointed at one fresh
//! temporary directory for every test here, for the same reasons as
//! `tests/settings_export_history.rs`: the variable is process-wide, and
//! the developer's real state store must never be touched.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

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

/// The state folder every test here shares, set once before any of them
/// reads it.
fn state_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::TempDir::new().unwrap().keep();
        // SAFETY: set once, inside `get_or_init`, before any test in this
        // binary reads it; nothing else here reads the environment.
        unsafe {
            std::env::set_var("XDG_STATE_HOME", &dir);
        }
        dir
    })
}

#[test]
fn a_removed_backups_state_and_history_are_deleted() {
    let state = state_dir();
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
    assert_eq!(find(state, &run_key).len(), 1, "run state saved");
    assert_eq!(find(state, &log_key).len(), 1, "history saved");

    run_state::remove(&id).unwrap();
    event_log::remove(&id).unwrap();

    assert!(find(state, &run_key).is_empty(), "run state deleted");
    assert!(find(state, &log_key).is_empty(), "history deleted");
    assert_eq!(run_state::load(&id), RunState::default());
    assert!(event_log::load(&id).is_empty());
    // Only that backup's.
    assert_eq!(run_state::load(&other).last_success, Some(1));
    assert_eq!(event_log::load(&other).len(), 1);
    // Removing what is already gone is not an error.
    run_state::remove(&id).unwrap();
}

#[test]
fn an_unreadable_status_can_be_reset() {
    let state = state_dir();
    let id = uuid::Uuid::new_v4().to_string();
    let key = format!("run-{id}");
    run_state::save(&id, &RunState::default()).unwrap();
    let [file] = find(state, &key).try_into().unwrap();
    // What a newer version might write: a field value this one cannot read.
    std::fs::write(&file, "(failure: Some(NotAThing))").unwrap();

    let stand_in = run_state::load(&id);
    assert!(stand_in.unreadable && stand_in.damaged, "{stand_in:?}");
    // Nothing can update it in place, so a passing check alone could never
    // clear it.
    assert!(run_state::update(&id, |run| run.damaged = false).is_err());

    run_state::reset(&id).unwrap();
    assert_eq!(run_state::load(&id), RunState::default());
    // The original is kept beside it, for a newer version or a person.
    let [aside] = find(state, &format!("{key}.unreadable"))
        .try_into()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(aside).unwrap(),
        "(failure: Some(NotAThing))"
    );
    run_state::update(&id, |run| run.last_check = Some(5)).unwrap();
    assert_eq!(run_state::load(&id).last_check, Some(5));
}

#[test]
fn an_unreadable_history_can_be_set_aside_and_started_again() {
    let state = state_dir();
    let id = uuid::Uuid::new_v4().to_string();
    let key = format!("event-log-{id}");
    event_log::record(&id, 1, EventKind::BackedUp, Source::Desktop);
    let [file] = find(state, &key).try_into().unwrap();
    std::fs::write(&file, "([(time: 1, kind: NotAThing)])").unwrap();

    assert!(event_log::is_unreadable(&id));
    // Nothing new reaches it while it cannot be read.
    event_log::record(&id, 2, EventKind::BackedUp, Source::Desktop);
    assert!(event_log::is_unreadable(&id));

    event_log::reset_if_unreadable(&id).unwrap();
    assert!(!event_log::is_unreadable(&id));
    assert_eq!(find(state, &format!("{key}.unreadable")).len(), 1);
    event_log::record(&id, 3, EventKind::BackedUp, Source::Desktop);
    assert_eq!(event_log::load(&id).len(), 1);

    // A readable one is left alone.
    event_log::reset_if_unreadable(&id).unwrap();
    assert_eq!(event_log::load(&id).len(), 1);
}
