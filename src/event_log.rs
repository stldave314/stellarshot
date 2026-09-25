// SPDX-License-Identifier: GPL-3.0-only

//! A history of what has happened to each backup: every run, failure, check
//! and clean-up, whichever started it.
//!
//! Kept in cosmic-config's state store, one key per profile, for the same
//! reason as [`crate::run_state`]: a scheduled run and the window can both
//! add to it, and neither must ever overwrite the other's entry. It is
//! included in the settings export (`app::settings_export`) even though it
//! lives apart from the settings themselves.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};

use crate::app::APP_ID;
use crate::app::config::CONFIG_VERSION;
use crate::debug::CONFIG;
use crate::debug_log;
use crate::engine::ErrorKind;
use crate::run_state::Stage;

/// Entries kept per backup. The oldest are dropped as new ones arrive, so a
/// backup that has run for years does not grow its log without bound.
pub const CAPACITY: usize = 200;

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    /// A snapshot was taken.
    BackedUp,
    /// A run failed and was not simply skipped: see [`EventKind::Skipped`]
    /// for the difference.
    Failed {
        stage: Stage,
        kind: ErrorKind,
        detail: String,
    },
    /// A scheduled backup found the destination unreachable or another
    /// process already writing, and left the next slot to try again rather
    /// than treating it as a failure.
    Skipped { kind: ErrorKind },
    /// An integrity check ran to the end.
    Checked { damaged: bool },
    /// Old snapshots were forgotten, data no longer needed was freed, or
    /// both.
    CleanedUp { forgotten: u64, freed: u64 },
}

/// One entry in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub time: i64,
    pub kind: EventKind,
}

/// The stored form: a plain `Vec` under one config key, newest last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Log(Vec<Event>);

fn store() -> Option<Config> {
    Config::new_state(APP_ID, CONFIG_VERSION)
        .inspect_err(|err| debug_log!(CONFIG, "no state store: {err}"))
        .ok()
}

fn key(profile_id: &str) -> String {
    format!("event-log-{profile_id}")
}

/// `profile_id`'s history, oldest first.
pub fn load(profile_id: &str) -> Vec<Event> {
    store()
        .and_then(|store| store.get::<Log>(&key(profile_id)).ok())
        .unwrap_or_default()
        .0
}

/// Add `event` to `log`, dropping the oldest entry once it holds more than
/// [`CAPACITY`]. Separate from [`record`] so the trimming can be tested
/// without touching the real config store: like [`crate::run_state`], this
/// module's own tests never call `store()`, which would read and write the
/// machine's actual state directory.
fn push(log: &mut Vec<Event>, event: Event) {
    log.push(event);
    if log.len() > CAPACITY {
        let excess = log.len() - CAPACITY;
        log.drain(0..excess);
    }
}

/// Add `kind` at `time` to `profile_id`'s log.
pub fn record(profile_id: &str, time: i64, kind: EventKind) {
    let Some(store) = store() else {
        return;
    };
    let mut log: Log = store.get(&key(profile_id)).unwrap_or_default();
    push(&mut log.0, Event { time, kind });
    if let Err(err) = store.set(&key(profile_id), &log) {
        debug_log!(CONFIG, "could not log an event for {profile_id}: {err}");
    }
}

/// Add every one of `incoming` that is not already present (a settings
/// import, which may bring events this installation already has if it is
/// run more than once), oldest first, capped the same as [`record`].
pub fn merge(profile_id: &str, incoming: &[Event]) {
    let Some(store) = store() else {
        return;
    };
    let mut log: Log = store.get(&key(profile_id)).unwrap_or_default();
    for event in incoming {
        if !log.0.contains(event) {
            push(&mut log.0, event.clone());
        }
    }
    log.0.sort_by_key(|event| event.time);
    if let Err(err) = store.set(&key(profile_id), &log) {
        debug_log!(CONFIG, "could not merge history for {profile_id}: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_kept_oldest_first() {
        let mut log = Vec::new();
        push(
            &mut log,
            Event {
                time: 1,
                kind: EventKind::BackedUp,
            },
        );
        push(
            &mut log,
            Event {
                time: 2,
                kind: EventKind::Checked { damaged: false },
            },
        );

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].time, 1);
        assert_eq!(log[1].time, 2);
    }

    #[test]
    fn the_oldest_entries_are_dropped_past_capacity() {
        let mut log = Vec::new();
        for time in 0..(CAPACITY as i64 + 5) {
            push(
                &mut log,
                Event {
                    time,
                    kind: EventKind::BackedUp,
                },
            );
        }

        assert_eq!(log.len(), CAPACITY);
        assert_eq!(log.first().unwrap().time, 5, "the first 5 were dropped");
        assert_eq!(log.last().unwrap().time, CAPACITY as i64 + 4);
    }
}
