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
        [
            Self::Backup,
            Self::Restore,
            Self::Check,
            Self::DeleteSnapshots,
            Self::SetPinned,
            Self::Maintain,
            Self::ChangePassword,
        ]
        .into_iter()
        .find(|op| op.as_arg() == arg)
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
            restore: None,
            snapshot: None,
            destination: None,
            ids: Vec::new(),
            pinned: None,
            keep: None,
            prune: false,
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
            if std::fs::write(&temporary, &line).is_ok() {
                let _ = std::fs::rename(&temporary, &self.progress_file);
            }
        }
    }
}

impl ProgressSink for Output {
    fn update(&self, event: &ProgressEvent) {
        self.emit(&Event::Progress {
            progress: event.clone(),
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
    let repo = engine::open(&job.repository, &job.password)?;
    match operation {
        Operation::Backup => {
            let request = job.request.ok_or_else(|| missing("backup request"))?;
            repo.backup(&request, sink).map(|report| Outcome {
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
                .map(|rules| repo.forget(&rules, &engine::hostname()))
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
    // Every real backup, restore or clean-up runs here, never in the
    // window's own process, so this is what actually needs rustic's and
    // rclone's own diagnostics to reach the log, not just the window seeing
    // them for in-process reads.
    crate::app::settings::set_logger_for_child();
    // Applies the cache location preference to every repository this
    // process opens; the rest of the app's settings go unused here.
    let _ = crate::app::config::StellarshotConfig::config();

    let Some(operation) = args.first().and_then(|arg| Operation::from_arg(arg)) else {
        eprintln!(
            "usage: stellarshot --run <backup|restore|check|delete-snapshots|set-pinned|maintain|change-password> < job.json"
        );
        return ExitCode::from(2);
    };

    let mut input = String::new();
    let job = std::io::stdin()
        .read_to_string(&mut input)
        .map_err(EngineError::from)
        .and_then(|_| {
            serde_json::from_str::<Job>(&input)
                .map_err(|err| EngineError::new(ErrorKind::Internal, format!("invalid job: {err}")))
        });
    // The job holds the password; drop the raw text as soon as it is parsed.
    drop(input);

    let output = Arc::new(Output {
        stdout: Some(Mutex::new(std::io::stdout())),
        progress_file: job
            .as_ref()
            .map(|job| lock::progress_path(&job.repository))
            .unwrap_or_default(),
    });

    let result = job.and_then(|job| run(operation, job, output.clone()));
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
