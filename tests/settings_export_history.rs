// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! `settings_export::merge` writes imported history into the real
//! `event_log` state store — there is no test-only namespace for it, only
//! whatever `cosmic_config::Config::new_state` resolves from
//! `XDG_STATE_HOME`. Redirected here, in its own test binary, to a fresh
//! temporary directory before anything touches it, so this can never leave
//! a stray key behind in the developer's own real state store the way
//! running it as a unit test inside `settings_export.rs` itself would.
//!
//! Its own binary for the same reason `tests/rclone_credentials.rs` is:
//! `XDG_STATE_HOME` is process-wide, and `cargo test` runs every `#[test]`
//! in one binary concurrently by default, so any other test that also
//! touches `event_log` or `run_state` running at the same time would race
//! on it.

use stellarshot::event_log::{self, Event, EventKind, Source};
use stellarshot::profile::{Destination, Profile};
use stellarshot::settings_export::Export;

fn profile(id: &str) -> Profile {
    let mut profile = Profile::new(
        "Home".into(),
        Destination::Local {
            path: "/backup".into(),
        },
        vec!["/home/alex".into()],
    );
    profile.id = id.to_owned();
    profile
}

#[test]
fn history_is_merged_for_new_and_existing_backups_alike() {
    // SAFETY: this is the only test in this binary, and nothing here spawns
    // another thread that could read `XDG_STATE_HOME` mid-change.
    unsafe {
        std::env::set_var("XDG_STATE_HOME", tempfile::TempDir::new().unwrap().keep());
    }

    let id = uuid::Uuid::new_v4().to_string();
    let existing = [profile(&id)];
    let mut export = Export::collect(&[profile(&id)]);
    export.history = vec![(
        id.clone(),
        vec![Event {
            time: 1,
            kind: EventKind::BackedUp,
            source: Source::Desktop,
        }],
    )];

    let merged = stellarshot::settings_export::merge(&existing, &export);
    stellarshot::settings_export::store_history(&merged.history);

    assert_eq!(event_log::load(&id).len(), 1);
}
