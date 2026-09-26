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
use crate::app::{errors, format};
use crate::debug::CONFIG;
use crate::debug_log;
use crate::engine::{EngineError, ErrorKind};
use crate::fl;
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
    Skipped {
        kind: ErrorKind,
    },
    /// An integrity check ran to the end.
    Checked {
        damaged: bool,
    },
    /// Old snapshots were forgotten, data no longer needed was freed, or
    /// both.
    CleanedUp {
        forgotten: u64,
        freed: u64,
    },
    /// Files were restored from a snapshot.
    Restored {
        files: u64,
        bytes: u64,
    },
    /// A snapshot was deleted outright, not merely forgotten by a keep
    /// policy: see [`EventKind::CleanedUp`] for that.
    SnapshotDeleted {
        snapshot: String,
    },
    /// A snapshot's pinned state changed.
    Pinned {
        snapshot: String,
        pinned: bool,
    },
    /// The repository's password was changed.
    PasswordChanged,
    /// A snapshot was mounted as a read-only folder.
    Mounted {
        snapshot: String,
    },
    Unmounted {
        snapshot: String,
    },
}

/// Where an action that produced an event came from: distinguishes an
/// action taken through the desktop window (or a scheduled run) from one
/// taken through the web interface, which is otherwise indistinguishable
/// from the same action done locally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Source {
    #[default]
    Desktop,
    Web,
}

/// One entry in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub time: i64,
    pub kind: EventKind,
    /// Missing in a log entry written before this field existed, which was
    /// always a desktop or scheduled action: never a wrong guess, since the
    /// web interface did not exist yet either.
    #[serde(default)]
    pub source: Source,
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

/// Every one of `profile_ids`' histories, merged and newest first: for a
/// view across every backup at once, not one profile's own page.
pub fn load_all(profile_ids: &[String]) -> Vec<(String, Event)> {
    let logs = profile_ids
        .iter()
        .map(|id| (id.clone(), load(id)))
        .collect();
    merge_sorted(logs)
}

fn merge_sorted(logs: Vec<(String, Vec<Event>)>) -> Vec<(String, Event)> {
    let mut all: Vec<(String, Event)> = logs
        .into_iter()
        .flat_map(|(id, events)| events.into_iter().map(move |event| (id.clone(), event)))
        .collect();
    all.sort_by_key(|(_, event)| std::cmp::Reverse(event.time));
    all
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

/// Add `kind` at `time` to `profile_id`'s log, from `source`.
pub fn record(profile_id: &str, time: i64, kind: EventKind, source: Source) {
    let Some(store) = store() else {
        return;
    };
    let mut log: Log = store.get(&key(profile_id)).unwrap_or_default();
    push(&mut log.0, Event { time, kind, source });
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

/// A snapshot ID, shortened the same way `SnapshotSummary::short_id` does:
/// this module never has a `SnapshotSummary` on hand, only the plain ID a
/// caller already had when it recorded the event.
fn short(snapshot: &str) -> &str {
    snapshot.get(..8).unwrap_or(snapshot)
}

/// One entry, as a sentence.
pub fn describe(kind: &EventKind) -> String {
    match kind {
        EventKind::BackedUp => fl!("event-backed-up"),
        EventKind::Failed {
            stage,
            kind,
            detail,
        } => {
            let stage = match stage {
                Stage::Backup => fl!("event-stage-backup"),
                Stage::Check => fl!("event-stage-check"),
                Stage::Cleanup => fl!("event-stage-cleanup"),
            };
            let error = EngineError::new(*kind, detail.clone());
            fl!(
                "event-failed",
                stage = stage,
                reason = errors::explain(&error)
            )
        }
        EventKind::Skipped { kind } => {
            let error = EngineError::new(*kind, String::new());
            fl!("event-skipped", reason = errors::explain(&error))
        }
        EventKind::Checked { damaged: false } => fl!("event-checked-sound"),
        EventKind::Checked { damaged: true } => fl!("event-checked-damaged"),
        EventKind::CleanedUp { forgotten, freed } => fl!(
            "event-cleaned-up",
            count = (*forgotten as i64),
            size = format::bytes(*freed)
        ),
        EventKind::Restored { files, bytes } => fl!(
            "event-restored",
            count = (*files as i64),
            size = format::bytes(*bytes)
        ),
        EventKind::SnapshotDeleted { snapshot } => {
            fl!("event-snapshot-deleted", snapshot = short(snapshot))
        }
        EventKind::Pinned {
            snapshot,
            pinned: true,
        } => fl!("event-pinned", snapshot = short(snapshot)),
        EventKind::Pinned {
            snapshot,
            pinned: false,
        } => fl!("event-unpinned", snapshot = short(snapshot)),
        EventKind::PasswordChanged => fl!("event-password-changed"),
        EventKind::Mounted { snapshot } => fl!("event-mounted", snapshot = short(snapshot)),
        EventKind::Unmounted { snapshot } => fl!("event-unmounted", snapshot = short(snapshot)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(time: i64, kind: EventKind) -> Event {
        Event {
            time,
            kind,
            source: Source::Desktop,
        }
    }

    #[test]
    fn merge_sorted_interleaves_every_profiles_events_newest_first() {
        let logs = vec![
            (
                "a".to_owned(),
                vec![event(1, EventKind::BackedUp), event(4, EventKind::BackedUp)],
            ),
            ("b".to_owned(), vec![event(3, EventKind::PasswordChanged)]),
        ];

        let merged = merge_sorted(logs);

        let times: Vec<i64> = merged.iter().map(|(_, event)| event.time).collect();
        assert_eq!(times, vec![4, 3, 1]);
        assert_eq!(merged[0].0, "a", "the event at time 4 belongs to profile a");
        assert_eq!(merged[1].0, "b");
    }

    #[test]
    fn events_are_kept_oldest_first() {
        let mut log = Vec::new();
        push(&mut log, event(1, EventKind::BackedUp));
        push(&mut log, event(2, EventKind::Checked { damaged: false }));

        assert_eq!(log.len(), 2);
        assert_eq!(log[0].time, 1);
        assert_eq!(log[1].time, 2);
    }

    #[test]
    fn the_oldest_entries_are_dropped_past_capacity() {
        let mut log = Vec::new();
        for time in 0..(CAPACITY as i64 + 5) {
            push(&mut log, event(time, EventKind::BackedUp));
        }

        assert_eq!(log.len(), CAPACITY);
        assert_eq!(log.first().unwrap().time, 5, "the first 5 were dropped");
        assert_eq!(log.last().unwrap().time, CAPACITY as i64 + 4);
    }

    #[test]
    fn every_new_event_kind_describes_itself_with_the_snapshots_short_id() {
        let long_id = "abcdef0123456789";
        for kind in [
            EventKind::SnapshotDeleted {
                snapshot: long_id.to_owned(),
            },
            EventKind::Pinned {
                snapshot: long_id.to_owned(),
                pinned: true,
            },
            EventKind::Pinned {
                snapshot: long_id.to_owned(),
                pinned: false,
            },
            EventKind::Mounted {
                snapshot: long_id.to_owned(),
            },
            EventKind::Unmounted {
                snapshot: long_id.to_owned(),
            },
        ] {
            assert!(
                describe(&kind).contains("abcdef01"),
                "{kind:?} should mention the snapshot's short ID, not its full one"
            );
        }
        assert!(!describe(&EventKind::PasswordChanged).is_empty());
        assert!(
            describe(&EventKind::Restored {
                files: 3,
                bytes: 1024
            })
            .contains('3')
        );
    }

    #[test]
    fn an_event_logged_before_source_existed_is_read_back_as_desktop() {
        // What was actually on disk before this field was added: no
        // `source` key at all, not a null or a default placeholder.
        let stored = r#"(time:1700000000,kind:BackedUp)"#;
        let loaded: Event = ron::from_str(stored).unwrap();
        assert_eq!(loaded.source, Source::Desktop);
    }
}
