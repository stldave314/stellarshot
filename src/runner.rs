// SPDX-License-Identifier: GPL-3.0-only

//! `stellarshot --run <operation>`: one write operation in its own process.
//!
//! Backups, restores and checks run here rather than in the window's process
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
    self, BackupReport, BackupRequest, EngineError, ErrorKind, Location, ProgressEvent,
    ProgressSink, Secret, lock,
};
use crate::{debug_log, error_log};

/// The operation a child process performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Backup,
    Restore,
    Check,
    DeleteSnapshots,
}

impl Operation {
    pub fn as_arg(self) -> &'static str {
        match self {
            Self::Backup => "backup",
            Self::Restore => "restore",
            Self::Check => "check",
            Self::DeleteSnapshots => "delete-snapshots",
        }
    }

    fn from_arg(arg: &str) -> Option<Self> {
        [
            Self::Backup,
            Self::Restore,
            Self::Check,
            Self::DeleteSnapshots,
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
    /// For `restore`: an ID, a unique prefix, or `latest`.
    #[serde(default)]
    pub snapshot: Option<String>,
    /// For `restore`.
    #[serde(default)]
    pub destination: Option<PathBuf>,
    /// For `delete-snapshots`.
    #[serde(default)]
    pub ids: Vec<String>,
}

/// One line of the child's output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum Event {
    Progress { progress: ProgressEvent },
    Done { report: Option<BackupReport> },
    Error { error: EngineError },
}

/// Writes events to stdout, one JSON object per line, and mirrors progress to
/// the repository's progress file for any window that did not start this run.
struct Output {
    stdout: Mutex<std::io::Stdout>,
    progress_file: PathBuf,
}

impl Output {
    fn emit(&self, event: &Event) {
        let Ok(line) = serde_json::to_string(event) else {
            return;
        };
        if let Ok(mut stdout) = self.stdout.lock() {
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

/// Run `operation` for `job`, holding the repository's write lock throughout.
pub fn run(
    operation: Operation,
    job: Job,
    sink: Arc<dyn ProgressSink>,
) -> Result<Option<BackupReport>, EngineError> {
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
            repo.backup(&request, sink).map(Some)
        }
        Operation::Restore => {
            let snapshot = job.snapshot.ok_or_else(|| missing("snapshot"))?;
            let destination = job.destination.ok_or_else(|| missing("destination"))?;
            repo.restore_all(&snapshot, &destination, sink)
                .map(|()| None)
        }
        Operation::Check => repo.check().map(|()| None),
        Operation::DeleteSnapshots => repo.delete_snapshots(&job.ids).map(|()| None),
    }
}

/// Entry point for `stellarshot --run <operation>`. `args` are the arguments
/// after `--run`.
pub fn main(args: &[String]) -> ExitCode {
    let Some(operation) = args.first().and_then(|arg| Operation::from_arg(arg)) else {
        eprintln!("usage: stellarshot --run <backup|restore|check|delete-snapshots> < job.json");
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
        stdout: Mutex::new(std::io::stdout()),
        progress_file: job
            .as_ref()
            .map(|job| lock::progress_path(&job.repository))
            .unwrap_or_default(),
    });

    let result = job.and_then(|job| run(operation, job, output.clone()));
    match result {
        Ok(report) => {
            output.emit(&Event::Done { report });
            ExitCode::SUCCESS
        }
        Err(error) => {
            error_log!(ENGINE, "--run {} failed: {error}", operation.as_arg());
            output.emit(&Event::Error { error });
            ExitCode::FAILURE
        }
    }
}
