// SPDX-License-Identifier: GPL-3.0-only

//! A history of what has happened to each backup: every run, failure, check
//! and clean-up, whichever started it.
//!
//! Kept in cosmic-config's state store, one key per profile, for the same
//! reason as [`crate::run_state`]: a scheduled run and the window can both
//! add to it, and neither must ever overwrite the other's entry. It is
//! included in the settings export (`settings_export`) even though it
//! lives apart from the settings themselves.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};

use crate::constants::EVENT_LOG_CAPACITY;
use crate::core::{errors, format};
use crate::debug::CONFIG;
use crate::engine::{EngineError, ErrorKind, short_id};
use crate::fl;
use crate::run_state::Stage;
use crate::error_log;

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

/// Where an action that produced an event came from: the desktop window (or
/// a scheduled run), or some other program recording into the same history.
///
/// `Other` also catches any source a later or earlier version wrote that
/// this one does not know, so an unfamiliar value never makes a whole log
/// unreadable — which would lose the history, or leave an unreadable file
/// that nothing is allowed to write over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Source {
    #[default]
    Desktop,
    #[serde(other)]
    Other,
}

/// One entry in the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Unix seconds.
    pub time: i64,
    pub kind: EventKind,
    /// Missing in a log entry written before this field existed, which was
    /// always a desktop or scheduled action: never a wrong guess.
    #[serde(default)]
    pub source: Source,
}

/// The stored form: a plain `Vec` under one config key, newest last.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Log(Vec<Event>);

fn store() -> Option<Config> {
    crate::paths::state_store()
}

fn key(profile_id: &str) -> String {
    format!("event-log-{profile_id}")
}

/// [`load`]'s log, distinguishing "nothing recorded yet" (`Ok` with an
/// empty log) from "something is there but this process cannot read it
/// right now" (`Err`) — a parse error, or an entry this version does not
/// know how to read. The difference matters to [`record`] and [`merge`],
/// which must never save a fresh, empty log over history they simply could
/// not read: losing the one new event either was about to add is far
/// better than losing every old one.
fn load_checked(store: &Config, profile_id: &str) -> Result<Log, ()> {
    match store.get(&key(profile_id)) {
        Ok(log) => Ok(log),
        Err(err) if crate::run_state::is_missing(&err) => Ok(Log::default()),
        Err(err) => {
            error_log!(
                CONFIG,
                "history for {profile_id} could not be read, leaving it alone: {err}"
            );
            Err(())
        }
    }
}

/// `profile_id`'s history, oldest first.
pub fn load(profile_id: &str) -> Vec<Event> {
    let Some(store) = store() else {
        return Vec::new();
    };
    load_checked(&store, profile_id).unwrap_or_default().0
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
/// [`EVENT_LOG_CAPACITY`]. Separate from [`record`] so the trimming can be tested
/// without touching the real config store: like [`crate::run_state`], this
/// module's own tests never call `store()`, which would read and write the
/// machine's actual state directory.
fn push(log: &mut Vec<Event>, event: Event) {
    // A destination that stays unplugged is skipped at every slot: an
    // hourly backup would otherwise fill the log with identical "skipped"
    // entries within days and push every real one out. A run of the same
    // skip is one entry, with the latest time.
    if let EventKind::Skipped { .. } = event.kind
        && let Some(last) = log.last_mut()
        && last.kind == event.kind
        && last.source == event.source
    {
        last.time = event.time;
        return;
    }
    log.push(event);
    if log.len() > EVENT_LOG_CAPACITY {
        let excess = log.len() - EVENT_LOG_CAPACITY;
        log.drain(0..excess);
    }
}

/// Forget a removed backup's history entirely.
pub fn remove(profile_id: &str) -> Result<(), String> {
    crate::paths::with_state_lock(|| crate::paths::remove_state_key(&key(profile_id)))
}

/// Whether `profile_id`'s history is there but cannot be read, so nothing
/// new can be added to it (see [`load_checked`]) until [`reset_if_unreadable`].
pub fn is_unreadable(profile_id: &str) -> bool {
    store().is_some_and(|store| load_checked(&store, profile_id).is_err())
}

/// Move an unreadable history aside (see [`crate::paths::set_aside_state_key`])
/// so the backup starts a new one. Leaves a readable one alone.
pub fn reset_if_unreadable(profile_id: &str) -> Result<(), String> {
    crate::paths::with_state_lock(|| {
        if is_unreadable(profile_id) {
            crate::paths::set_aside_state_key(&key(profile_id))?;
        }
        Ok(())
    })
}

/// Add `kind` at `time` to `profile_id`'s log, from `source`.
pub fn record(profile_id: &str, time: i64, kind: EventKind, source: Source) {
    push_event(profile_id, Event { time, kind, source });
}

fn push_event(profile_id: &str, event: Event) {
    crate::paths::with_state_lock(|| push_event_unlocked(profile_id, event));
}

fn push_event_unlocked(profile_id: &str, event: Event) {
    let Some(store) = store() else {
        return;
    };
    let Ok(mut log) = load_checked(&store, profile_id) else {
        return;
    };
    push(&mut log.0, event);
    if let Err(err) = store.set(&key(profile_id), &log) {
        error_log!(CONFIG, "could not log an event for {profile_id}: {err}");
    }
}

/// Fold `incoming` into `log`: every event not already there, oldest first,
/// trimmed to [`EVENT_LOG_CAPACITY`] from the old end only after everything is
/// in and sorted. Trimming as each one arrives would let a file full of old
/// events push this installation's real recent ones out first. Pure, so it
/// can be tested without touching the state store.
///
/// Imported events are recorded as [`Source::Other`]: the file is not this
/// installation's own record of what it did, so none of it is presented as
/// something the desktop or a scheduled run did. "Already there" therefore
/// compares the time and kind only, so importing an export this installation
/// made itself does not double every entry.
fn merge_into(log: &mut Vec<Event>, incoming: &[Event]) {
    // Only the newest `EVENT_LOG_CAPACITY` of them could survive the trim;
    // bounding the input also bounds the quadratic scan below.
    let mut incoming: Vec<&Event> = incoming.iter().collect();
    incoming.sort_by_key(|event| event.time);
    let skip = incoming.len().saturating_sub(EVENT_LOG_CAPACITY);
    for event in incoming.into_iter().skip(skip) {
        let present = log
            .iter()
            .any(|have| have.time == event.time && have.kind == event.kind);
        if !present {
            log.push(Event {
                source: Source::Other,
                ..event.clone()
            });
        }
    }
    log.sort_by_key(|event| event.time);
    if log.len() > EVENT_LOG_CAPACITY {
        let excess = log.len() - EVENT_LOG_CAPACITY;
        log.drain(0..excess);
    }
}

/// Add every one of `incoming` that is not already present (a settings
/// import, which may bring events this installation already has if it is
/// run more than once): see [`merge_into`]. Does disk I/O; call it off the
/// UI thread.
pub fn merge(profile_id: &str, incoming: &[Event]) {
    crate::paths::with_state_lock(|| merge_unlocked(profile_id, incoming));
}

fn merge_unlocked(profile_id: &str, incoming: &[Event]) {
    let Some(store) = store() else {
        return;
    };
    let Ok(mut log) = load_checked(&store, profile_id) else {
        return;
    };
    merge_into(&mut log.0, incoming);
    if let Err(err) = store.set(&key(profile_id), &log) {
        error_log!(CONFIG, "could not merge history for {profile_id}: {err}");
    }
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
            fl!("event-snapshot-deleted", snapshot = short_id(snapshot))
        }
        EventKind::Pinned {
            snapshot,
            pinned: true,
        } => fl!("event-pinned", snapshot = short_id(snapshot)),
        EventKind::Pinned {
            snapshot,
            pinned: false,
        } => fl!("event-unpinned", snapshot = short_id(snapshot)),
        EventKind::PasswordChanged => fl!("event-password-changed"),
        EventKind::Mounted { snapshot } => fl!("event-mounted", snapshot = short_id(snapshot)),
        EventKind::Unmounted { snapshot } => fl!("event-unmounted", snapshot = short_id(snapshot)),
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
    fn a_run_of_the_same_skip_is_one_entry_with_the_latest_time() {
        let skipped = || EventKind::Skipped {
            kind: ErrorKind::DestinationUnavailable,
        };
        let mut log = Vec::new();
        push(&mut log, event(1, EventKind::BackedUp));
        for time in 2..500 {
            push(&mut log, event(time, skipped()));
        }
        assert_eq!(log.len(), 2, "the real history survives");
        assert_eq!(log[1].time, 499);

        push(&mut log, event(500, EventKind::BackedUp));
        push(&mut log, event(501, skipped()));
        assert_eq!(log.len(), 4, "a skip after something else is new again");
    }

    #[test]
    fn the_oldest_entries_are_dropped_past_capacity() {
        let mut log = Vec::new();
        for time in 0..(EVENT_LOG_CAPACITY as i64 + 5) {
            push(&mut log, event(time, EventKind::BackedUp));
        }

        assert_eq!(log.len(), EVENT_LOG_CAPACITY);
        assert_eq!(log.first().unwrap().time, 5, "the first 5 were dropped");
        assert_eq!(log.last().unwrap().time, EVENT_LOG_CAPACITY as i64 + 4);
    }

    #[test]
    fn importing_old_history_never_evicts_the_newest_local_events() {
        let cap = EVENT_LOG_CAPACITY as i64;
        let mut log: Vec<Event> = (1000..1000 + cap)
            .map(|time| event(time, EventKind::BackedUp))
            .collect();
        let incoming: Vec<Event> = (0..cap)
            .map(|time| event(time, EventKind::BackedUp))
            .collect();

        merge_into(&mut log, &incoming);

        assert_eq!(log.len(), EVENT_LOG_CAPACITY);
        assert_eq!(
            log.first().unwrap().time,
            1000,
            "every local event survived"
        );
        assert_eq!(log.last().unwrap().time, 1000 + cap - 1);
    }

    #[test]
    fn importing_twice_does_not_duplicate_and_marks_events_as_other() {
        let mut log = vec![event(5, EventKind::BackedUp)];
        let incoming = [
            event(5, EventKind::BackedUp),
            event(7, EventKind::PasswordChanged),
        ];

        merge_into(&mut log, &incoming);
        merge_into(&mut log, &incoming);

        let times: Vec<i64> = log.iter().map(|e| e.time).collect();
        assert_eq!(times, vec![5, 7]);
        assert_eq!(log[0].source, Source::Desktop, "the local one is untouched");
        assert_eq!(
            log[1].source,
            Source::Other,
            "an imported one is not claimed"
        );
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
        let stored = r"(time:1700000000,kind:BackedUp)";
        let loaded: Event = ron::from_str(stored).unwrap();
        assert_eq!(loaded.source, Source::Desktop);
    }

    /// Entries an earlier version recorded for a front end that no longer
    /// exists in this program: a `Web` source, and a `web` context naming a
    /// peer. Neither is a field or a variant any more, and neither may make
    /// the entry — or the whole log it is in — unreadable, or nothing would
    /// be allowed to write over it and that backup's history would freeze.
    #[test]
    fn an_entry_from_a_source_this_version_does_not_know_still_loads() {
        let stored = r#"(time:1700000000,kind:BackedUp,source:Web,web:Some((addr:"10.0.0.2:5555",method:Password)))"#;

        let loaded: Event = ron::from_str(stored).unwrap();

        assert_eq!(loaded.source, Source::Other);
        assert_eq!(loaded.kind, EventKind::BackedUp);
    }

    #[test]
    fn a_source_written_by_a_later_version_loads_as_other_too() {
        let stored = r"(time:1700000000,kind:BackedUp,source:SomethingNew)";
        let loaded: Event = ron::from_str(stored).unwrap();
        assert_eq!(loaded.source, Source::Other);
    }
}
