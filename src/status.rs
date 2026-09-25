// SPDX-License-Identifier: GPL-3.0-only

//! Each backup's status, read from whatever any process can see on disk:
//! its run history, and whether something currently holds its
//! repository's write lock. Used by the applet, which is never the one
//! doing the writing, and by the window for a run it did not itself start
//! (a scheduled backup that began while the window was closed, or was
//! started by another window).

use crate::engine::lock;
use crate::profile::Profile;
use crate::run_state::{self, RunState};

/// One backup's status right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub profile_id: String,
    pub name: String,
    /// Something holds this backup's repository lock: a scheduled run, or
    /// another window's own backup, not necessarily this process.
    pub running: bool,
    pub last_success: Option<i64>,
    /// A scheduled run failed since the last success.
    pub failed: bool,
    /// Significantly late for its schedule; see [`run_state::is_overdue`].
    pub overdue: bool,
}

/// `profile`'s status right now.
pub fn of(profile: &Profile, run: &RunState, now: i64) -> Status {
    let running = profile
        .location()
        .is_ok_and(|location| lock::is_running(&location));
    Status {
        profile_id: profile.id.clone(),
        name: profile.name.clone(),
        running,
        last_success: run.last_success.max(profile.last_success),
        failed: run.current_failure(profile.last_success).is_some(),
        overdue: run_state::is_overdue(profile, run, now),
    }
}

/// Every profile's status, in the order given.
pub fn all(profiles: &[Profile], now: i64) -> Vec<Status> {
    profiles
        .iter()
        .map(|profile| of(profile, &run_state::load(&profile.id), now))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::profile::{Destination, Schedule};

    fn profile() -> Profile {
        Profile::new(
            "Home".into(),
            Destination::Local {
                path: PathBuf::from("/backups/home"),
            },
            vec![PathBuf::from("/home/alex")],
        )
    }

    #[test]
    fn a_never_run_backup_is_not_overdue_and_not_failed() {
        let status = of(&profile(), &RunState::default(), 1_000_000);
        assert!(!status.running);
        assert_eq!(status.last_success, None);
        assert!(!status.failed);
        assert!(!status.overdue);
    }

    #[test]
    fn a_failure_after_the_last_success_shows_until_a_later_success() {
        let run = RunState {
            last_success: Some(100),
            failure: Some(crate::run_state::Failure {
                time: 200,
                stage: crate::run_state::Stage::Backup,
                kind: crate::engine::ErrorKind::Internal,
                detail: String::new(),
            }),
            ..RunState::default()
        };
        assert!(of(&profile(), &run, 300).failed);

        let recovered = RunState {
            last_success: Some(250),
            ..run
        };
        assert!(!of(&profile(), &recovered, 300).failed);
    }

    #[test]
    fn an_overdue_daily_backup_is_reported_as_such() {
        let mut backup = profile();
        backup.schedule = Schedule::Daily;
        let day = 86_400;
        let run = RunState {
            last_success: Some(0),
            ..RunState::default()
        };
        assert!(of(&backup, &run, 30 * day).overdue, "30 days late");
        assert!(!of(&backup, &run, day).overdue, "1 day is on schedule");
    }
}
