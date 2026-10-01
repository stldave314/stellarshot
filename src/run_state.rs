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
use crate::constants::OVERDUE_FACTOR;
use crate::debug::CONFIG;
use crate::engine::{EngineError, ErrorKind};
use crate::profile::Profile;
use crate::{debug_log, error_log};

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
            // A shield rather than a plain drive: "up to date" is a claim
            // about safety, not just where the data happens to sit.
            // Confirmed to actually render as a shield, not assumed from
            // its freedesktop name alone.
            Self::UpToDate => "security-high-symbolic",
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
    if run.current_failure(profile.last_success, now).is_some() {
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
    // A success stamped in the future (a wrong clock) counts as none.
    match run
        .last_success
        .max(profile.last_success)
        .filter(|&last| last <= now)
    {
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
    /// When a check last started but did not run to the end (an error other
    /// than damage), so a check that keeps failing is retried after a while
    /// rather than at every slot.
    #[serde(default)]
    pub last_check_attempt: Option<i64>,
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
    ///
    /// A clean-up or check failure comes after the backup's own success in
    /// the same run, often within the same second, so it shows when it is at
    /// or after that success; a backup failure only when strictly after. A
    /// success stamped later than `now` (the clock was wrong when it was
    /// recorded) is ignored rather than hiding every failure until the clock
    /// catches up.
    pub fn current_failure(&self, other: Option<i64>, now: i64) -> Option<&Failure> {
        let last_success = self.last_success.max(other).filter(|&time| time <= now);
        self.failure.as_ref().filter(|failure| {
            last_success.is_none_or(|success| match failure.stage {
                Stage::Backup => failure.time > success,
                Stage::Cleanup | Stage::Check => failure.time >= success,
            })
        })
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

/// [`load`], distinguishing "nothing saved yet" from "something is there
/// but this process cannot read it right now" (an `ErrorKind` variant a
/// newer version added, or plain corruption). The difference matters: a
/// caller must never save a fresh default over data it simply could not
/// parse — that is how one bad read would otherwise erase this backup's
/// whole run history and reset `damaged` back to `false`, letting
/// automatic pruning resume against a repository a check had found
/// damaged.
fn load_checked(profile_id: &str) -> Result<RunState, ()> {
    let Some(store) = store() else {
        return Ok(RunState::default());
    };
    match store.get(&key(profile_id)) {
        Ok(state) => Ok(state),
        Err(err) if is_missing(&err) => Ok(RunState::default()),
        Err(err) => {
            error_log!(
                CONFIG,
                "run state for {profile_id} could not be read, leaving it alone: {err}"
            );
            Err(())
        }
    }
}

/// Whether `err` means "nothing saved here yet" — safe to treat as a fresh
/// default — as opposed to "something is there, but this process could not
/// read it": a parse error, an unknown enum variant a newer version wrote,
/// or a real I/O failure. Kept as its own pure function, tested on its own,
/// since it is the entire boundary [`load_checked`]'s safety depends on:
/// get it backwards and an unreadable value is silently treated as absent
/// again.
pub(crate) fn is_missing(err: &cosmic::cosmic_config::Error) -> bool {
    matches!(
        err,
        cosmic::cosmic_config::Error::NotFound | cosmic::cosmic_config::Error::NoConfigDirectory
    )
}

/// `profile_id`'s run state, or a default if none has ever been saved.
/// Unreadable (not merely absent) state comes back marked `damaged`, even
/// though nothing here actually confirmed that, so automatic pruning stays
/// paused rather than resuming against state this process cannot vouch
/// for — see [`load_checked`]'s own doc comment.
pub fn load(profile_id: &str) -> RunState {
    load_checked(profile_id).unwrap_or_else(|()| RunState {
        damaged: true,
        ..RunState::default()
    })
}

/// Forget a removed backup's state entirely.
pub fn remove(profile_id: &str) -> Result<(), String> {
    crate::paths::with_state_lock(|| crate::paths::remove_state_key(&key(profile_id)))
}

pub fn save(profile_id: &str, state: &RunState) -> Result<(), String> {
    let store = store().ok_or("no state directory")?;
    store
        .set(&key(profile_id), state)
        .map_err(|err| err.to_string())
}

/// Change one profile's state in place. Fails, rather than saving a fresh
/// default over data this process could not read (see [`load_checked`]):
/// there is nothing sensible to change without first knowing what the
/// value actually was. It is an error, not a silent no-op, because the
/// caller's own change genuinely did not happen — a passing check that
/// could not clear `damaged` this way would otherwise report success while
/// automatic pruning stayed paused indefinitely, with nothing in the log
/// or the window saying why.
pub fn update(profile_id: &str, change: impl FnOnce(&mut RunState)) -> Result<(), String> {
    crate::paths::with_state_lock(|| update_unlocked(profile_id, change))
}

fn update_unlocked(profile_id: &str, change: impl FnOnce(&mut RunState)) -> Result<(), String> {
    let Ok(mut state) = load_checked(profile_id) else {
        return Err(format!(
            "the run state for {profile_id} could not be read, so it was left as it was"
        ));
    };
    change(&mut state);
    save(profile_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Destination, Profile, Schedule};

    #[test]
    fn only_not_found_counts_as_genuinely_missing() {
        assert!(is_missing(&cosmic::cosmic_config::Error::NotFound));
        assert!(is_missing(&cosmic::cosmic_config::Error::NoConfigDirectory));
        assert!(!is_missing(&cosmic::cosmic_config::Error::GetKey(
            "run-x".into(),
            std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        )));
        assert!(!is_missing(&cosmic::cosmic_config::Error::Io(
            std::io::Error::other("disk error")
        )));
    }

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
        assert!(state.current_failure(None, i64::MAX).is_some());
        assert!(
            state.current_failure(Some(300), i64::MAX).is_none(),
            "a later backup from the window clears it"
        );
        state.last_success = Some(300);
        assert!(state.current_failure(None, i64::MAX).is_none());
    }

    /// A run state written by a newer Stellarshot, with an `ErrorKind` this
    /// one has never heard of. Without `#[serde(other)]` on the kind, the
    /// whole file failed to parse; `load_checked` then (correctly) refused
    /// to overwrite it, and `update` (correctly) refused to change it — so
    /// a passing check could never clear `damaged`, and pruning stayed
    /// paused with no way out short of a hand edit.
    #[test]
    fn a_failure_kind_from_a_newer_version_loads_as_unknown_not_as_unreadable() {
        // A bare identifier, the way RON writes a unit variant (`kind: io`,
        // `kind: canceled` in a real event log), not a quoted string.
        let failure: Failure = ron::from_str(
            r#"(time: 1, stage: Backup, kind: brand_new_kind, detail: "something newer")"#,
        )
        .expect("an unknown kind must not fail the whole value");

        assert_eq!(failure.kind, ErrorKind::Unknown);
        assert_eq!(failure.detail, "something newer", "the rest still loads");
    }

    #[test]
    fn a_failure_with_no_success_ever_shows() {
        let state = RunState {
            failure: failure(5),
            ..RunState::default()
        };
        assert!(state.current_failure(None, i64::MAX).is_some());
    }

    #[test]
    fn a_clean_up_failure_in_the_same_second_as_the_backup_still_shows() {
        let state = RunState {
            last_success: Some(500),
            failure: Some(Failure {
                time: 500,
                stage: Stage::Cleanup,
                kind: ErrorKind::Io,
                detail: String::new(),
            }),
            ..RunState::default()
        };
        assert!(state.current_failure(None, 1000).is_some());
        let backup_same_second = RunState {
            failure: failure(500),
            ..state
        };
        assert!(backup_same_second.current_failure(None, 1000).is_none());
    }

    #[test]
    fn a_success_stamped_in_the_future_hides_nothing() {
        let state = RunState {
            last_success: Some(9_999_999),
            failure: failure(200),
            ..RunState::default()
        };
        assert!(
            state.current_failure(None, 1000).is_some(),
            "a wrong clock must not hide a real failure"
        );
    }
}
