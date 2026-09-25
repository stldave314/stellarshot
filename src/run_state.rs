// SPDX-License-Identifier: GPL-3.0-only

//! What happened when each backup last ran: facts, not settings.
//!
//! They are kept in cosmic-config's *state* store, one key per profile,
//! rather than in the profile list. A scheduled run and the window both
//! write them, and neither ever rewrites the other's settings: had they
//! lived in the profiles, a scheduled run finishing while the user edited a
//! backup could have saved the old profile list over the edit.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};

use crate::app::APP_ID;
use crate::app::config::CONFIG_VERSION;
use crate::debug::CONFIG;
use crate::debug_log;
use crate::engine::{EngineError, ErrorKind};
use crate::profile::Profile;

/// How much later than its own schedule a backup may run before it is shown
/// as overdue: enough slack for the timer's own jitter, not so much that a
/// truly stuck schedule (an unplugged drive, a laptop closed for days) goes
/// unnoticed.
const OVERDUE_FACTOR: i64 = 2;

/// A backup's state, for the sidebar icon and its legend. See [`status`] for
/// how the fields it is drawn from combine into one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupStatus {
    /// A check found the repository damaged. Automatic pruning is paused.
    Damaged,
    /// The last scheduled run failed and no later run has succeeded.
    Failed,
    /// Backs up automatically, but no success is recent enough for its
    /// schedule (see [`Schedule::period`] and [`OVERDUE_FACTOR`]).
    Overdue,
    /// A backup, check or clean-up is running right now.
    Running,
    /// Nothing wrong: manual with no failure, or automatic and current.
    UpToDate,
}

impl BackupStatus {
    /// The symbolic icon name for the sidebar.
    pub fn icon(self) -> &'static str {
        match self {
            Self::UpToDate => "drive-harddisk-symbolic",
            Self::Running => "emblem-synchronizing-symbolic",
            Self::Overdue => "appointment-missed-symbolic",
            Self::Failed => "dialog-error-symbolic",
            Self::Damaged => "dialog-warning-symbolic",
        }
    }

    /// A short label for the legend and the icon's tooltip.
    pub fn label(self) -> String {
        match self {
            Self::UpToDate => crate::fl!("status-up-to-date"),
            Self::Running => crate::fl!("status-running"),
            Self::Overdue => crate::fl!("status-overdue"),
            Self::Failed => crate::fl!("status-failed"),
            Self::Damaged => crate::fl!("status-damaged"),
        }
    }

    /// Every status, worst first, for the legend.
    pub fn legend() -> [Self; 5] {
        [
            Self::Damaged,
            Self::Failed,
            Self::Overdue,
            Self::Running,
            Self::UpToDate,
        ]
    }
}

/// `profile`'s status, from what is known without opening its repository:
/// its own record of the last success, this computer's run facts, and
/// whether the window has work running for it right now.
pub fn status(profile: &Profile, run: &RunState, running: bool) -> BackupStatus {
    status_at(profile, run, running, crate::app::format::now())
}

fn status_at(profile: &Profile, run: &RunState, running: bool, now: i64) -> BackupStatus {
    // What is happening right now outranks history that this very run may
    // be about to change (a retry, or the check a damaged repository asked
    // for).
    if running {
        return BackupStatus::Running;
    }
    if run.damaged {
        return BackupStatus::Damaged;
    }
    if run.current_failure(profile.last_success).is_some() {
        return BackupStatus::Failed;
    }
    if is_overdue(profile, run, now) {
        return BackupStatus::Overdue;
    }
    BackupStatus::UpToDate
}

/// Whether `profile` is significantly late for an automatic backup. A
/// schedule that has never had a success is not yet called overdue: it may
/// simply not have reached its first slot, and a real failure to run at all
/// is reported as [`BackupStatus::Failed`] instead. Public so a scheduled
/// run can decide from it whether persistent unavailability has earned a
/// notification, not only so the sidebar can choose an icon.
pub fn is_overdue(profile: &Profile, run: &RunState, now: i64) -> bool {
    let Some(period) = profile.schedule.period() else {
        return false;
    };
    match run.last_success.max(profile.last_success) {
        Some(last) => now - last > period * OVERDUE_FACTOR,
        None => false,
    }
}

/// Which part of a scheduled run failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    #[default]
    Backup,
    /// Forgetting old snapshots or pruning, after the backup succeeded.
    Cleanup,
    /// The integrity check, after the backup succeeded.
    Check,
}

/// A run that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// Unix seconds.
    pub time: i64,
    #[serde(default)]
    pub stage: Stage,
    pub kind: ErrorKind,
    pub detail: String,
}

impl Failure {
    pub fn error(&self) -> EngineError {
        EngineError::new(self.kind, self.detail.clone())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    /// When a scheduled backup last finished, in Unix seconds.
    #[serde(default)]
    pub last_success: Option<i64>,
    /// When an integrity check last ran to the end, damaged or not.
    #[serde(default)]
    pub last_check: Option<i64>,
    /// The last scheduled run that failed. It stays until a later run
    /// succeeds, and is shown while it is newer than the last success.
    #[serde(default)]
    pub failure: Option<Failure>,
    /// The last check found damage. Automatic pruning waits until a check
    /// passes, because pruning a damaged repository can lose more.
    #[serde(default)]
    pub damaged: bool,
    /// Bytes freed by every clean-up this backup has ever run, from the
    /// window or on its own schedule.
    #[serde(default)]
    pub total_freed: u64,
    /// A notification has already been sent for the current overdue streak,
    /// so a scheduled run that keeps finding the destination unreachable
    /// notifies once, not at every skipped slot. Cleared on the next
    /// success.
    #[serde(default)]
    pub overdue_notified: bool,
}

impl RunState {
    /// The failure to show, if it happened after the last success. `other`
    /// is a success recorded elsewhere, such as a backup from the window.
    pub fn current_failure(&self, other: Option<i64>) -> Option<&Failure> {
        let last_success = self.last_success.max(other);
        self.failure
            .as_ref()
            .filter(|failure| last_success.is_none_or(|success| failure.time > success))
    }
}

fn store() -> Option<Config> {
    Config::new_state(APP_ID, CONFIG_VERSION)
        .inspect_err(|err| debug_log!(CONFIG, "no state store: {err}"))
        .ok()
}

fn key(profile_id: &str) -> String {
    format!("run-{profile_id}")
}

pub fn load(profile_id: &str) -> RunState {
    store()
        .and_then(|store| store.get(&key(profile_id)).ok())
        .unwrap_or_default()
}

pub fn save(profile_id: &str, state: &RunState) -> Result<(), String> {
    let store = store().ok_or("no state directory")?;
    store
        .set(&key(profile_id), state)
        .map_err(|err| err.to_string())
}

/// Change one profile's state in place.
pub fn update(profile_id: &str, change: impl FnOnce(&mut RunState)) -> Result<(), String> {
    let mut state = load(profile_id);
    change(&mut state);
    save(profile_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Destination, Profile, Schedule};

    fn profile(schedule: Schedule, last_success: Option<i64>) -> Profile {
        let mut profile = Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/backup".into(),
            },
            vec!["/home/alex".into()],
        );
        profile.schedule = schedule;
        profile.last_success = last_success;
        profile
    }

    #[test]
    fn running_wins_over_every_other_status() {
        let profile = profile(Schedule::Manual, None);
        let run = RunState {
            damaged: true,
            ..RunState::default()
        };
        assert_eq!(
            status_at(&profile, &run, true, 1_000),
            BackupStatus::Running
        );
    }

    #[test]
    fn damage_outranks_a_failure() {
        let profile = profile(Schedule::Manual, Some(500));
        let run = RunState {
            damaged: true,
            failure: failure(600),
            ..RunState::default()
        };
        assert_eq!(
            status_at(&profile, &run, false, 1_000),
            BackupStatus::Damaged
        );
    }

    #[test]
    fn a_manual_backup_that_never_ran_is_up_to_date_not_overdue() {
        let profile = profile(Schedule::Manual, None);
        assert_eq!(
            status_at(&profile, &RunState::default(), false, 1_000_000),
            BackupStatus::UpToDate
        );
    }

    #[test]
    fn a_scheduled_backup_becomes_overdue_after_twice_its_period() {
        const DAY: i64 = 86_400;
        let profile = profile(Schedule::Daily, Some(0));
        assert_eq!(
            status_at(&profile, &RunState::default(), false, DAY + 1),
            BackupStatus::UpToDate,
            "a bit over one day is still within slack"
        );
        assert_eq!(
            status_at(&profile, &RunState::default(), false, 2 * DAY + 1),
            BackupStatus::Overdue
        );
    }

    #[test]
    fn a_scheduled_backup_with_no_success_yet_is_not_overdue() {
        let profile = profile(Schedule::Hourly, None);
        assert_eq!(
            status_at(&profile, &RunState::default(), false, 1_000_000),
            BackupStatus::UpToDate,
            "it may simply not have reached its first slot yet"
        );
    }

    #[test]
    fn a_scheduled_success_from_the_window_counts_against_overdue() {
        const DAY: i64 = 86_400;
        // The scheduled run itself never succeeded, but a manual backup from
        // the window is just as real a success.
        let profile = profile(Schedule::Daily, Some(0));
        let run = RunState::default();
        assert_eq!(
            status_at(&profile, &run, false, DAY - 1),
            BackupStatus::UpToDate
        );
    }

    fn failure(time: i64) -> Option<Failure> {
        Some(Failure {
            time,
            stage: Stage::Backup,
            kind: ErrorKind::WrongPassword,
            detail: String::new(),
        })
    }

    #[test]
    fn a_failure_shows_until_a_later_success() {
        let mut state = RunState {
            last_success: Some(100),
            failure: failure(200),
            ..RunState::default()
        };
        assert!(state.current_failure(None).is_some());
        assert!(
            state.current_failure(Some(300)).is_none(),
            "a later backup from the window clears it"
        );
        state.last_success = Some(300);
        assert!(state.current_failure(None).is_none());
    }

    #[test]
    fn a_failure_with_no_success_ever_shows() {
        let state = RunState {
            failure: failure(5),
            ..RunState::default()
        };
        assert!(state.current_failure(None).is_some());
    }
}
