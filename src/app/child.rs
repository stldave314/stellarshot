// SPDX-License-Identifier: GPL-3.0-only

//! Running a write operation in a `stellarshot --run` child process and
//! turning its output into a stream the UI can consume.

use std::fmt;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cosmic::iced::futures::{SinkExt, Stream, channel::mpsc};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::debug::ENGINE;
use crate::engine::{EngineError, ErrorKind};
use crate::runner::{Event, Job, Operation};
use crate::{debug_log, error_log};

/// A running child, shared so the UI can cancel it.
#[derive(Clone)]
pub struct ChildHandle(Arc<Mutex<Child>>);

impl fmt::Debug for ChildHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildHandle")
    }
}

impl ChildHandle {
    /// Stop the operation. Safe for a backup: the snapshot file is written
    /// last, so nothing half-written is ever referenced.
    pub fn cancel(&self) {
        if let Ok(mut child) = self.0.lock() {
            debug_log!(ENGINE, "cancelling child {:?}", child.id());
            let _ = child.start_kill();
        }
    }
}

/// What the stream reports about a child process.
#[derive(Debug, Clone)]
pub enum ChildEvent {
    /// The child is running; keep the handle to cancel it.
    Started(ChildHandle),
    /// A line the child reported.
    Event(Event),
    /// The child ended without reporting an outcome: killed, crashed, or
    /// unable to start.
    Ended(EngineError),
}

/// Spawn `stellarshot --run <operation>` with `job` on stdin, and stream
/// everything it reports. The last item is always a `Done` or `Error` event,
/// or `Ended`.
pub fn run(operation: Operation, job: Job) -> impl Stream<Item = ChildEvent> {
    run_with(std::env::current_exe(), operation, job)
}

/// [`run`], with the executable given explicitly. Tests use this to run the
/// real binary from a test harness whose own executable is not Stellarshot.
pub fn run_with(
    exe: std::io::Result<std::path::PathBuf>,
    operation: Operation,
    job: Job,
) -> impl Stream<Item = ChildEvent> {
    cosmic::iced::stream::channel(32, move |mut out: mpsc::Sender<ChildEvent>| async move {
        let ended = match drive(exe, operation, job, &mut out).await {
            Ok(true) => return,
            Ok(false) => EngineError::new(ErrorKind::Cancelled, String::new()),
            Err(err) => err,
        };
        let _ = out.send(ChildEvent::Ended(ended)).await;
    })
}

/// Returns `Ok(true)` when the child reported its own outcome, `Ok(false)` when
/// it was killed before it could.
async fn drive(
    exe: std::io::Result<std::path::PathBuf>,
    operation: Operation,
    job: Job,
    out: &mut mpsc::Sender<ChildEvent>,
) -> Result<bool, EngineError> {
    let exe = exe?;
    let mut child = Command::new(exe)
        .arg("--run")
        .arg(operation.as_arg())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Closing the window must not abandon a backup halfway: the child
        // finishes on its own.
        .kill_on_drop(false)
        .spawn()?;

    let job = serde_json::to_vec(&job)
        .map_err(|err| EngineError::new(ErrorKind::Internal, err.to_string()))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&job).await?;
        // Dropping stdin closes it, which tells the child the job is complete.
    }
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let handle = ChildHandle(Arc::new(Mutex::new(child)));
    let _ = out.send(ChildEvent::Started(handle.clone())).await;

    let mut reported = false;
    if let Some(stdout) = stdout {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            match serde_json::from_str::<Event>(&line) {
                Ok(event) => {
                    reported |= !matches!(event, Event::Progress { .. });
                    let _ = out.send(ChildEvent::Event(event)).await;
                }
                Err(err) => debug_log!(ENGINE, "unreadable child output {line:?}: {err}"),
            }
        }
    }

    let status = wait(&handle).await?;
    if reported {
        return Ok(true);
    }
    let mut diagnostics = String::new();
    if let Some(mut stderr) = stderr {
        let _ = stderr.read_to_string(&mut diagnostics).await;
    }
    use std::os::unix::process::ExitStatusExt;
    if status.signal().is_some() {
        debug_log!(ENGINE, "child ended by signal {:?}", status.signal());
        return Ok(false);
    }
    error_log!(
        ENGINE,
        "child exited with {status} and no outcome: {diagnostics}"
    );
    Err(EngineError::new(
        ErrorKind::Internal,
        format!("{status}: {}", diagnostics.trim()),
    ))
}

/// Wait for the child without holding its lock, so it can still be cancelled.
async fn wait(handle: &ChildHandle) -> Result<std::process::ExitStatus, EngineError> {
    loop {
        let finished = handle
            .0
            .lock()
            .map_err(|_| EngineError::new(ErrorKind::Internal, "child lock poisoned"))?
            .try_wait()?;
        if let Some(status) = finished {
            return Ok(status);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
