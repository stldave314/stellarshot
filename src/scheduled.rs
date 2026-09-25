// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot --scheduled <backup-id>`: what a backup's systemd timer runs.
//!
//! It backs up through [`runner::run`], the same function the window's
//! `--run backup` child uses, so a scheduled backup and **Back Up Now** are
//! the same code. After a successful backup it forgets snapshots under the
//! retention policy, checks the repository if a check is due, and prunes if
//! that is enabled and the repository is not known to be damaged.
//!
//! A destination that cannot be reached (a drive not plugged in, no
//! network) or a repository another process is writing to is skipped
//! quietly; the next slot tries again. Any other failure is recorded for the
//! window to show and raises a desktop notification.

use std::process::ExitCode;
use std::sync::Arc;

use crate::app::config::StellarshotConfig;
use crate::app::errors;
use crate::constants::CHECK_INTERVAL;
use crate::debug::SCHED;
use crate::engine::{EngineError, ErrorKind, KeepRules, Location, Secret};
use crate::event_log;
use crate::profile::{Profile, Schedule};
use crate::run_state::{self, Failure, RunState, Stage};
use crate::runner::{self, Job, Operation, Output};
use crate::{debug_log, error_log, fl, notify};

/// What runs after a successful backup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Forget by these rules; `None` keeps every snapshot.
    pub forget: Option<KeepRules>,
    /// A check is due.
    pub check: bool,
    /// Prune is enabled for this backup.
    pub prune: bool,
}

impl Plan {
    pub fn new(profile: &Profile, state: &RunState, now: i64) -> Self {
        let interval = CHECK_INTERVAL.as_secs() as i64;
        Self {
            forget: profile.retention.keep_rules(),
            check: state.last_check.is_none_or(|last| now - last >= interval),
            prune: profile.prune_enabled(),
        }
    }

    /// Whether to prune, once forgetting has removed `removed` snapshots and
    /// the repository is known to be `damaged` or not. Pruning only frees
    /// something after snapshots were forgotten, and pruning a damaged
    /// repository can lose more than the damage already has.
    pub fn prune_now(&self, removed: u64, damaged: bool) -> bool {
        self.prune && removed > 0 && !damaged
    }
}

/// Failures that are not worth a notification: the next slot will try
/// again, and the backup's page shows how long ago the last one was.
fn is_quiet(error: &EngineError) -> bool {
    matches!(
        error.kind,
        ErrorKind::DestinationUnavailable | ErrorKind::Locked
    )
}

fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

/// A skipped slot is quiet on its own — the next one tries again — but a
/// destination that stays unreachable for a long time is a problem the
/// window's sidebar shows and nothing else says out loud until it is
/// looked at. Notify once per overdue streak, not at every skipped slot.
fn notify_if_overdue(profile: &Profile, runtime: &tokio::runtime::Runtime) {
    let state = run_state::load(&profile.id);
    if !overdue_notification_due(profile, &state, now()) {
        return;
    }
    record(profile, |state| state.overdue_notified = true);
    let summary = fl!("notify-overdue", name = profile.name.clone());
    let body = fl!(
        "notify-overdue-body",
        schedule = crate::app::pages::profile::schedule_summary(profile.schedule)
    );
    runtime.block_on(notify::failure(
        &summary,
        &body,
        &fl!("notify-open"),
        &profile.id,
    ));
}

/// Whether a persistently unreachable destination has earned a
/// notification: overdue, and not already notified about since the last
/// success. Kept separate from [`notify_if_overdue`] so it can be tested on
/// its own: that function's other half sends a real desktop notification,
/// which no automated test may risk triggering for real (see
/// `tests/scheduled.rs`'s doc comment on why it never runs a scenario that
/// reaches `notify::failure`).
fn overdue_notification_due(profile: &Profile, state: &RunState, now: i64) -> bool {
    !state.overdue_notified && run_state::is_overdue(profile, state, now)
}

fn record(profile: &Profile, change: impl FnOnce(&mut RunState)) {
    if let Err(err) = run_state::update(&profile.id, change) {
        error_log!(SCHED, "could not record the run of {}: {err}", profile.id);
    }
}

struct Failed(Stage, EngineError);

/// Run one of the runner's operations, reporting progress only to the
/// progress file a window follows.
fn operation(operation: Operation, job: Job) -> Result<runner::Outcome, EngineError> {
    let output = Arc::new(Output::progress_file_only(&job.repository));
    runner::run(operation, job, output)
}

/// Back up, then forget, check and prune as the plan says.
fn run(
    profile: &Profile,
    global_exclude_patterns: &[String],
    location: Location,
    secret: Secret,
) -> Result<(), Failed> {
    let job = || Job::new(location.clone(), secret.clone());
    operation(
        Operation::Backup,
        Job {
            request: Some(profile.backup_request(global_exclude_patterns)),
            ..job()
        },
    )
    .map_err(|err| Failed(Stage::Backup, err))?;
    let finished = now();
    record(profile, |state| {
        state.last_success = Some(finished);
        state.failure = None;
        state.overdue_notified = false;
    });
    event_log::record(&profile.id, finished, event_log::EventKind::BackedUp);
    debug_log!(SCHED, "backed up {}", profile.id);

    let plan = Plan::new(profile, &run_state::load(&profile.id), finished);
    let removed = match plan.forget {
        Some(rules) => operation(
            Operation::Maintain,
            Job {
                keep: Some(rules),
                ..job()
            },
        )
        .map_err(|err| Failed(Stage::Cleanup, err))?
        .forgotten
        .map_or(0, |report| report.removed),
        None => 0,
    };

    if plan.check {
        let result = operation(Operation::Check, job());
        let damaged = matches!(&result, Err(err) if err.kind == ErrorKind::RepositoryDamaged);
        if result.is_ok() || damaged {
            let checked = now();
            record(profile, |state| {
                state.last_check = Some(checked);
                state.damaged = damaged;
            });
            event_log::record(
                &profile.id,
                checked,
                event_log::EventKind::Checked { damaged },
            );
        }
        result.map_err(|err| Failed(Stage::Check, err))?;
    }

    let damaged = run_state::load(&profile.id).damaged;
    let mut freed = 0;
    if plan.prune_now(removed, damaged) {
        freed = operation(
            Operation::Maintain,
            Job {
                prune: true,
                ..job()
            },
        )
        .map_err(|err| Failed(Stage::Cleanup, err))?
        .pruned
        .map_or(0, |report| report.bytes);
        record(profile, |state| state.total_freed += freed);
    }
    if removed > 0 || freed > 0 {
        event_log::record(
            &profile.id,
            now(),
            event_log::EventKind::CleanedUp {
                forgotten: removed,
                freed,
            },
        );
    }
    Ok(())
}

/// Entry point for `stellarshot --scheduled <backup-id>`.
pub fn main(args: &[String]) -> ExitCode {
    let Some(id) = args.first() else {
        eprintln!("usage: stellarshot --scheduled <backup-id>");
        return ExitCode::from(2);
    };
    crate::core::localization::init();
    let config = StellarshotConfig::config();
    let Some(profile) = config.profile(id).cloned() else {
        error_log!(SCHED, "--scheduled: no backup with the ID {id}");
        return ExitCode::from(2);
    };
    if profile.schedule == Schedule::Manual {
        // A timer left from before the schedule was turned off; the window
        // removes it the next time it starts.
        debug_log!(SCHED, "{id} is no longer scheduled");
        return ExitCode::SUCCESS;
    }
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            error_log!(SCHED, "--scheduled: {err}");
            return ExitCode::FAILURE;
        }
    };

    let result = profile
        .location()
        .map_err(|err| Failed(Stage::Backup, err))
        .and_then(|location| {
            let secret = runtime.block_on(profile.password()).ok_or_else(|| {
                Failed(
                    Stage::Backup,
                    EngineError::new(ErrorKind::PasswordNotRemembered, ""),
                )
            })?;
            run(&profile, &config.global_exclude_patterns, location, secret)
        });
    let Err(Failed(stage, error)) = result else {
        return ExitCode::SUCCESS;
    };
    if stage == Stage::Backup && is_quiet(&error) {
        debug_log!(SCHED, "{id} skipped: {error}");
        event_log::record(
            id,
            now(),
            event_log::EventKind::Skipped { kind: error.kind },
        );
        notify_if_overdue(&profile, &runtime);
        return ExitCode::SUCCESS;
    }

    error_log!(
        SCHED,
        "the scheduled run of {id} failed ({stage:?}): {error}"
    );
    let time = now();
    record(&profile, |state| {
        state.failure = Some(Failure {
            time,
            stage,
            kind: error.kind,
            detail: error.detail.clone(),
        });
    });
    event_log::record(
        id,
        time,
        event_log::EventKind::Failed {
            stage,
            kind: error.kind,
            detail: error.detail.clone(),
        },
    );
    let summary = match stage {
        Stage::Backup => fl!("notify-backup-failed", name = profile.name.clone()),
        Stage::Cleanup => fl!("notify-cleanup-failed", name = profile.name.clone()),
        Stage::Check => fl!("notify-check-failed", name = profile.name.clone()),
    };
    let body = errors::explain(&error);
    runtime.block_on(notify::failure(&summary, &body, &fl!("notify-open"), id));
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::profile::{Destination, Retention};

    fn profile() -> Profile {
        let mut profile = Profile::new(
            "Home".into(),
            Destination::Local {
                path: PathBuf::from("/mnt/backup"),
            },
            vec![PathBuf::from("/home/alex")],
        );
        profile.schedule = Schedule::Daily;
        profile.retention = Retention::Smart;
        profile
    }

    const DAY: i64 = 86_400;

    #[test]
    fn a_check_is_due_every_thirty_days() {
        let now = 1_000 * DAY;
        let never = RunState::default();
        assert!(Plan::new(&profile(), &never, now).check, "never checked");
        let recent = RunState {
            last_check: Some(now - 29 * DAY),
            ..RunState::default()
        };
        assert!(!Plan::new(&profile(), &recent, now).check);
        let old = RunState {
            last_check: Some(now - 30 * DAY),
            ..RunState::default()
        };
        assert!(Plan::new(&profile(), &old, now).check);
    }

    #[test]
    fn prune_waits_for_a_clean_check() {
        let plan = Plan::new(&profile(), &RunState::default(), 0);
        assert!(plan.prune, "a local folder prunes by default");
        assert!(plan.prune_now(3, false));
        assert!(!plan.prune_now(3, true), "not while damage is known");
        assert!(!plan.prune_now(0, false), "nothing was forgotten");
    }

    #[test]
    fn an_overdue_destination_notifies_once_per_streak() {
        let now = 1_000 * DAY;
        let overdue = RunState {
            last_success: Some(now - 3 * DAY),
            ..RunState::default()
        };
        assert!(
            overdue_notification_due(&profile(), &overdue, now),
            "3 days late on a daily schedule is well past due"
        );

        let already_notified = RunState {
            overdue_notified: true,
            ..overdue.clone()
        };
        assert!(
            !overdue_notification_due(&profile(), &already_notified, now),
            "the same streak does not notify twice"
        );

        let current = RunState {
            last_success: Some(now - DAY / 2),
            ..RunState::default()
        };
        assert!(!overdue_notification_due(&profile(), &current, now));
    }

    #[test]
    fn keep_forever_forgets_and_prunes_nothing() {
        let mut profile = profile();
        profile.retention = Retention::KeepForever;
        let plan = Plan::new(&profile, &RunState::default(), 0);
        assert_eq!(plan.forget, None);
        assert!(!plan.prune_now(0, false));
    }

    #[test]
    fn only_unreachable_and_busy_repositories_are_skipped_quietly() {
        let error = |kind| EngineError::new(kind, "");
        assert!(is_quiet(&error(ErrorKind::DestinationUnavailable)));
        assert!(is_quiet(&error(ErrorKind::Locked)));
        for kind in [
            ErrorKind::WrongPassword,
            ErrorKind::PasswordNotRemembered,
            ErrorKind::RcloneMissing,
            ErrorKind::Io,
            ErrorKind::Internal,
        ] {
            assert!(!is_quiet(&error(kind)), "{kind:?} must be reported");
        }
    }
}
