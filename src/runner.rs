// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot --run <operation>`: one write operation in its own process.
//!
//! Backups, restores, checks and clean-ups run here rather than in the window's process
//! because rustic cannot be interrupted once an operation starts. A child
//! process can be: killing it is safe, because the snapshot file is written
//! last and an interrupted backup leaves only unreferenced data behind.
//!
//! Protocol: the job arrives as one JSON object on **stdin** (a password never
//! goes in argv or the environment, both of which other processes of the same
//! user can read from `/proc`). Progress and the outcome leave as JSON lines on
//! stdout. The exit status is 0 on success and 1 when an error was reported.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::debug::ENGINE;
use crate::engine::{
    self, BackupReport, BackupRequest, EngineError, ErrorKind, ForgetReport, KeepRules, Location,
    ProgressEvent, ProgressSink, PruneReport, RestorePreview, RestoreRequest, Secret,
    SnapshotSummary, lock,
};
use crate::hooks::{self, HookResult};
use crate::profile::Hook;
use crate::{debug_log, error_log};

/// The operation a child process performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Backup,
    Restore,
    Check,
    DeleteSnapshots,
    /// Pin or unpin a snapshot: see [`crate::engine::Repo::set_pinned`].
    SetPinned,
    /// Forget snapshots under the retention rules, then prune if asked.
    Maintain,
    /// Add a key for the new password, then remove the one the repository
    /// was opened with: see [`crate::engine::Repo::change_password`].
    ChangePassword,
}

impl Operation {
    /// Every variant, once — [`Self::from_arg`] searches it rather than
    /// repeating the list a second time, which a new variant could add to
    /// one list and forget in the other.
    const ALL: [Self; 7] = [
        Self::Backup,
        Self::Restore,
        Self::Check,
        Self::DeleteSnapshots,
        Self::SetPinned,
        Self::Maintain,
        Self::ChangePassword,
    ];

    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Check => "check",
            Self::DeleteSnapshots => "delete-snapshots",
            Self::SetPinned => "set-pinned",
            Self::Maintain => "maintain",
            Self::ChangePassword => "change-password",
        }
    }

    fn from_arg(arg: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|op| op.as_arg() == arg)
    }
}

/// Everything an operation needs, sent on stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub repository: Location,
    pub password: Secret,
    /// For `backup`.
    #[serde(default)]
    pub request: Option<BackupRequest>,
    /// For `backup`: run before and after it.
    #[serde(default)]
    pub hooks: Vec<Hook>,
    /// For `restore`: what to restore, where, and what to do about files
    /// that already exist.
    #[serde(default)]
    pub restore: Option<RestoreRequest>,
    /// For a whole-snapshot `restore`: an ID, a unique prefix, or `latest`.
    #[serde(default)]
    pub snapshot: Option<String>,
    /// For `restore`.
    #[serde(default)]
    pub destination: Option<PathBuf>,
    /// For `delete-snapshots`, and the one snapshot for `set-pinned`.
    #[serde(default)]
    pub ids: Vec<String>,
    /// For `set-pinned`: the new pinned state.
    #[serde(default)]
    pub pinned: Option<bool>,
    /// For `maintain`: the retention rules to forget by, if any.
    #[serde(default)]
    pub keep: Option<KeepRules>,
    /// For `maintain`: prune after forgetting.
    #[serde(default)]
    pub prune: bool,
    /// For `maintain`: this profile's own tag and canonical sources, so
    /// `forget` only ever touches this profile's own snapshots in a
    /// repository shared with another (see [`engine::profile_tag`] and
    /// [`crate::engine::Repo::forget`]'s own doc comment). Empty for every
    /// other operation, which reads neither.
    #[serde(default)]
    pub profile_tag: String,
    #[serde(default)]
    pub profile_sources: Vec<PathBuf>,
    /// For `change-password`.
    #[serde(default)]
    pub new_password: Option<Secret>,
}

impl Job {
    /// A job for `repository` with nothing else set; each operation fills in
    /// the fields it reads.
    pub fn new(repository: Location, password: Secret) -> Self {
        Self {
            repository,
            password,
            request: None,
            hooks: Vec::new(),
            restore: None,
            snapshot: None,
            destination: None,
            ids: Vec::new(),
            pinned: None,
            keep: None,
            prune: false,
            profile_tag: String::new(),
            profile_sources: Vec::new(),
            new_password: None,
        }
    }
}

/// One line of the child's output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    Progress {
        progress: ProgressEvent,
    },
    Done {
        report: Option<BackupReport>,
        #[serde(default)]
        restored: Option<RestorePreview>,
        #[serde(default)]
        forgotten: Option<ForgetReport>,
        #[serde(default)]
        pruned: Option<PruneReport>,
        // Boxed: an unboxed `SnapshotSummary` here, alongside the other
        // report fields already in this variant, pushes `Event` (and the
        // `ChildEvent`/`Message` types that wrap it) over clippy's
        // large-enum-variant threshold.
        #[serde(default)]
        pinned: Option<Box<SnapshotSummary>>,
    },
    Error {
        error: EngineError,
    },
}

/// Writes events to stdout, one JSON object per line, and mirrors progress to
/// the repository's progress file for any window that did not start this run.
pub(crate) struct Output {
    /// `None` for a scheduled run, whose stdout is the journal: progress
    /// many times a second does not belong there.
    stdout: Option<Mutex<std::io::Stdout>>,
    progress_file: PathBuf,
}

impl Output {
    /// Progress to the repository's progress file only.
    pub(crate) fn progress_file_only(location: &Location) -> Self {
        Self {
            stdout: None,
            progress_file: lock::progress_path(location),
        }
    }

    fn emit(&self, event: &Event) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        if let Some(Ok(mut stdout)) = self.stdout.as_ref().map(Mutex::lock) {
            // A closed stdout (the window went away) must not stop the job.
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
        if let Event::Progress { .. } = event {
            let temporary = self.progress_file.with_extension("progress.tmp");
            if write_progress_temp(&temporary, &line).is_ok() {
                let _ = std::fs::rename(&temporary, &self.progress_file);
            }
        }
    }
}

/// Writes `line` to `path`, refusing to follow a symlink already there
/// rather than `std::fs::write`'s plain open-and-truncate (see SEC-5 in the
/// review plan). `path`'s own directory is already verified private
/// (`lock::create_private_dir`), so only this same user's own other
/// processes could ever plant such a symlink — a narrow residual, but
/// refusing it costs one open flag.
fn write_progress_temp(path: &PathBuf, line: &str) -> std::io::Result<()> {
    let fd = rustix::fs::open(
        path,
        rustix::fs::OFlags::CREATE
            | rustix::fs::OFlags::WRONLY
            | rustix::fs::OFlags::TRUNC
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )?;
    std::fs::File::from(fd).write_all(line.as_bytes())
}

impl ProgressSink for Output {
    fn update(&self, event: &ProgressEvent) {
        self.emit(&Event::Progress {
            progress: event.clone(),
        });
    }

    /// See `proc_signal`: the run was told to stop and has run its After
    /// hooks; this is the outcome the window sees for it.
    fn canceled(&self) {
        self.emit(&Event::Error {
            error: EngineError::new(ErrorKind::Canceled, String::new()),
        });
    }
}

/// Removes a file when dropped.
struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn missing(what: &str) -> EngineError {
    EngineError::new(ErrorKind::Internal, format!("the job has no {what}"))
}

/// The failed `Before` hook, as the backup's own failure: `results` is
/// never empty when this is called, since [`hooks::run_before`] only
/// returns `Err` after pushing the hook that failed.
fn hook_failure(results: &[HookResult]) -> EngineError {
    let Some(failed) = results.last() else {
        return EngineError::new(ErrorKind::HookFailed, "a hook failed");
    };
    let detail = if failed.detail.is_empty() {
        failed.name.clone()
    } else {
        format!("{}: {}", failed.name, failed.detail)
    };
    EngineError::new(ErrorKind::HookFailed, detail)
}

/// `After` hooks are best-effort: a failure is logged, not reported as the
/// backup's own failure, since the backup has already succeeded or failed
/// on its own terms by the time these run.
fn log_after_hooks(results: &[HookResult]) {
    for result in results {
        if !result.ok {
            error_log!(ENGINE, "hook \"{}\" failed: {}", result.name, result.detail);
        }
    }
}

/// Guarantees `After` hooks run once `Before` hooks have succeeded, even if
/// something between here and the explicit call on the success path returns
/// early through `?` — a wrong password or an unreachable destination, say.
/// A `Before` hook's documented use is to stop a database and its matching
/// `After` hook to start it again; skipping the second because the backup
/// never got as far as running would leave the service stopped.
///
/// Dropped while still armed, it runs the hooks as a failure. The success
/// path calls [`Self::run`] itself, which disarms it so they do not run
/// twice.
///
/// A signal is the one path neither of those covers: see `proc_signal`,
/// which runs the same hooks on SIGTERM. `claim_after_hooks` is what keeps
/// the two from both running them.
struct AfterHookGuard<'a> {
    hooks: &'a [Hook],
    armed: bool,
    /// This run's own entry in `proc_signal`, so the claim below answers
    /// for this run and no other.
    ticket: crate::proc_signal::Ticket,
}

impl<'a> AfterHookGuard<'a> {
    fn new(hooks: &'a [Hook], ticket: crate::proc_signal::Ticket) -> Self {
        Self {
            hooks,
            armed: true,
            ticket,
        }
    }

    /// Run the hooks for the backup's real outcome, and disarm the guard so
    /// `Drop` does not run them a second time.
    fn run(mut self, succeeded: bool) -> Vec<HookResult> {
        self.armed = false;
        match crate::proc_signal::claim_after_hooks(self.ticket) {
            Some(_running) => hooks::run_after(self.hooks, succeeded),
            // A SIGTERM got here first and is running them itself (see
            // `proc_signal`); wait for it to end the process rather than
            // carry on (or exit) under those hooks.
            None => {
                crate::proc_signal::wait_if_terminating();
                Vec::new()
            }
        }
    }
}

impl Drop for AfterHookGuard<'_> {
    fn drop(&mut self) {
        if self.armed
            && let Some(_running) = crate::proc_signal::claim_after_hooks(self.ticket)
        {
            log_after_hooks(&hooks::run_after(self.hooks, false));
        }
    }
}

/// What a finished operation reports.
#[derive(Debug, Default)]
pub struct Outcome {
    pub report: Option<BackupReport>,
    pub restored: Option<RestorePreview>,
    pub forgotten: Option<ForgetReport>,
    pub pruned: Option<PruneReport>,
    pub pinned: Option<SnapshotSummary>,
}

/// Run `operation` for `job`, holding the repository's write lock throughout.
pub fn run(
    operation: Operation,
    job: Job,
    sink: Arc<dyn ProgressSink>,
) -> Result<Outcome, EngineError> {
    let _lock = lock::acquire(&job.repository)?;
    // Declared after the lock, so it is dropped (and the file removed) while
    // the lock is still held. A process that failed to get the lock never
    // reaches this line, and so never removes the holder's progress file.
    let _progress_file = RemoveOnDrop(lock::progress_path(&job.repository));
    debug_log!(ENGINE, "--run {} holds the lock", operation.as_arg());
    // Checked before `Before` hooks run, so a malformed job (never produced
    // by the window itself) cannot leave them run with no matching `After`:
    // every other way this function can fail before reaching the explicit
    // `run_after` call is instead covered by `AfterHookGuard` below.
    if operation == Operation::Backup && job.request.is_none() {
        return Err(missing("backup request"));
    }
    // Armed before the first Before hook, and for every operation, not only
    // a backup: a SIGTERM (Cancel, `systemctl --user stop`, logout) then
    // always reports the run canceled, and a backup's After hooks run
    // whatever stage it is stopped at. A non-backup has no hooks to run.
    let backup_hooks: &[Hook] = if operation == Operation::Backup {
        &job.hooks
    } else {
        &[]
    };
    let report_sink = sink.clone();
    let ticket = crate::proc_signal::arm(crate::proc_signal::Armed {
        hooks: backup_hooks.to_vec(),
        report: Box::new(move || report_sink.canceled()),
    });
    // Dropped without `run` (any early return below, including a Before hook
    // failing), it runs the After hooks as a failure: a Before hook that
    // already stopped a service must see it started again.
    let after_hooks = AfterHookGuard::new(backup_hooks, ticket);
    if operation == Operation::Backup {
        hooks::run_before(&job.hooks).map_err(|results| hook_failure(&results))?;
    }
    let repo = engine::open(&job.repository, &job.password)?;
    match operation {
        Operation::Backup => {
            let request = job.request.ok_or_else(|| missing("backup request"))?;
            let result = repo.backup(&request, sink);
            log_after_hooks(&after_hooks.run(result.is_ok()));
            result.map(|report| Outcome {
                report: Some(report),
                ..Outcome::default()
            })
        }
        Operation::Restore => match job.restore {
            Some(request) => repo.restore(&request, sink).map(|restored| Outcome {
                restored: Some(restored),
                ..Outcome::default()
            }),
            None => {
                let snapshot = job.snapshot.ok_or_else(|| missing("snapshot"))?;
                let destination = job.destination.ok_or_else(|| missing("destination"))?;
                repo.restore_all(&snapshot, &destination, sink)
                    .map(|()| Outcome::default())
            }
        },
        Operation::Check => repo.check().map(|()| Outcome::default()),
        Operation::DeleteSnapshots => repo.delete_snapshots(&job.ids).map(|()| Outcome::default()),
        Operation::SetPinned => {
            let id = job.ids.first().ok_or_else(|| missing("snapshot ID"))?;
            let pinned = job.pinned.ok_or_else(|| missing("pinned state"))?;
            repo.set_pinned(id, pinned).map(|summary| Outcome {
                pinned: Some(summary),
                ..Outcome::default()
            })
        }
        Operation::Maintain => {
            let forgotten = job
                .keep
                .map(|rules| {
                    repo.forget(
                        &rules,
                        &engine::hostname(),
                        &job.profile_tag,
                        &job.profile_sources,
                    )
                })
                .transpose()?;
            let pruned = job.prune.then(|| repo.prune()).transpose()?;
            Ok(Outcome {
                forgotten,
                pruned,
                ..Outcome::default()
            })
        }
        Operation::ChangePassword => {
            let new_password = job.new_password.ok_or_else(|| missing("new password"))?;
            repo.change_password(new_password.expose())
                .map(|()| Outcome::default())
        }
    }
}

/// Entry point for `stellarshot --run <operation>`. `args` are the arguments
/// after `--run`.
pub fn main(args: &[String]) -> ExitCode {
    // This process holds the repository password in memory (in the parsed
    // `Job`, and briefly in the stdin buffer before it is parsed and
    // zeroized): a core dump triggered by a crash here would otherwise be
    // readable by anyone who can read this user's files, unlike the memory
    // itself.
    crate::harden_process();
    // Before anything else can start a thread: see `proc_signal`. Cancel
    // in the window sends this process SIGTERM, and this is what turns
    // that into "run the After hooks, say so, then go" instead of just
    // "go".
    crate::proc_signal::install();

    crate::debug::init(crate::debug::Role::Run);
    crate::paths::tighten_app_dirs();
    // Every real backup, restore or clean-up runs here, never in the
    // window's own process, so this is what actually needs rustic's and
    // rclone's own diagnostics to reach the log, not just the window seeing
    // them for in-process reads.
    crate::app::startup::set_logger_for_child();
    // Applies the cache location preference to every repository this
    // process opens; the rest of the app's settings go unused here.
    let _ = crate::app::config::StellarshotConfig::config();

    let Some(operation) = args.first().and_then(|arg| Operation::from_arg(arg)) else {
        eprintln!(
            "usage: stellarshot --run <backup|restore|check|delete-snapshots|set-pinned|maintain|change-password> < job.json"
        );
        return ExitCode::from(2);
    };

    let mut input = zeroize::Zeroizing::new(Vec::new());
    let job = std::io::stdin()
        .read_to_end(&mut input)
        .map_err(EngineError::from)
        .and_then(|_| {
            serde_json::from_slice::<Job>(&input)
                .map_err(|err| EngineError::new(ErrorKind::Internal, format!("invalid job: {err}")))
        });
    // The job holds the password; `Zeroizing` wipes this buffer when it
    // drops, rather than a plain deallocation that leaves the password's
    // bytes in freed memory as they were.
    drop(input);

    let output = Arc::new(Output {
        stdout: Some(Mutex::new(std::io::stdout())),
        progress_file: job
            .as_ref()
            .map(|job| lock::progress_path(&job.repository))
            .unwrap_or_default(),
    });

    let result = job.and_then(|job| run(operation, job, output.clone()));
    // A SIGTERM is being handled: that thread runs the After hooks, reports
    // the run canceled and ends the process. Reporting this thread's own
    // result now (often a hook the handler just stopped, as a "failure")
    // would race it, and returning would end the process under its hooks.
    crate::proc_signal::wait_if_terminating();
    match result {
        Ok(outcome) => {
            output.emit(&Event::Done {
                report: outcome.report,
                restored: outcome.restored,
                forgotten: outcome.forgotten,
                pruned: outcome.pruned,
                pinned: outcome.pinned.map(Box::new),
            });
            ExitCode::SUCCESS
        }
        Err(error) => {
            error_log!(ENGINE, "--run {} failed: {error}", operation.as_arg());
            output.emit(&Event::Error { error });
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_operation_round_trips_through_its_own_arg() {
        for operation in Operation::ALL {
            assert_eq!(
                Operation::from_arg(operation.as_arg()),
                Some(operation),
                "{operation:?} must parse back from its own `as_arg()`"
            );
        }
    }

    #[test]
    fn an_unrecognized_arg_is_not_an_operation() {
        assert_eq!(Operation::from_arg("not-a-real-operation"), None);
    }

    #[test]
    fn write_progress_temp_writes_an_ordinary_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("job.progress.tmp");

        write_progress_temp(&path, "hello").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
    }

    #[test]
    fn write_progress_temp_refuses_a_symlink_and_does_not_write_through_it() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::TempDir::new().unwrap();
        let target = dir.path().join("elsewhere");
        std::fs::write(&target, "untouched").unwrap();
        let path = dir.path().join("job.progress.tmp");
        symlink(&target, &path).unwrap();

        let result = write_progress_temp(&path, "hostile");

        assert!(result.is_err(), "a symlink must not be written through");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "untouched",
            "the symlink's real target must be untouched"
        );
    }
}
