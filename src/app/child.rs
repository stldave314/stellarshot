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

use crate::constants::{CHILD_STDERR_DETAIL, CHILD_STDERR_TAIL, DRAIN_AFTER_EXIT};
use crate::debug::ENGINE;
use crate::engine::{EngineError, ErrorKind};
use crate::runner::{Event, Job, Operation};
use crate::{debug_log, error_log};

/// A running child, shared so the UI can cancel it.
#[derive(Clone)]
pub struct ChildHandle(Arc<Mutex<Child>>, Option<rustix::process::Pid>);

impl fmt::Debug for ChildHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildHandle")
    }
}

impl ChildHandle {
    /// Stop the operation. Safe for a backup: the snapshot file is written
    /// last, so nothing half-written is ever referenced.
    ///
    /// The whole process group is killed, not only the child: for SFTP and
    /// cloud storage, rustic runs `rclone serve restic` under it, and an
    /// rclone left behind keeps the child's stdout open, so the operation
    /// would never be seen to end.
    pub fn cancel(&self) {
        if let Ok(mut child) = self.0.lock() {
            debug_log!(ENGINE, "canceling child {:?}", child.id());
            // `id()` is `None` once the child has been reaped, after which
            // its group ID could in time belong to someone else.
            if child.id().is_some() {
                self.kill_group();
            }
            let _ = child.start_kill();
        }
    }

    /// Kill every process in the child's group.
    fn kill_group(&self) {
        if let Some(group) = self.1
            && let Err(err) =
                rustix::process::kill_process_group(group, rustix::process::Signal::KILL)
        {
            debug_log!(ENGINE, "killing process group {group:?}: {err}");
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
            Ok(false) => EngineError::new(ErrorKind::Canceled, String::new()),
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
    if exe.to_string_lossy().ends_with(" (deleted)") {
        // The kernel appends this to `/proc/self/exe`'s target once the file
        // it named has been unlinked, which is what happens to a running
        // process's own binary during a package upgrade. The path is no
        // longer valid to spawn: it would fail with a bare ENOENT below.
        return Err(EngineError::new(
            ErrorKind::AppUpdated,
            exe.display().to_string(),
        ));
    }
    let mut child = Command::new(exe)
        .arg("--run")
        .arg(operation.as_arg())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Closing the window must not abandon a backup halfway: the child
        // finishes on its own.
        .kill_on_drop(false)
        // Its own process group, so canceling reaches everything it
        // started (see `ChildHandle::cancel`).
        .process_group(0)
        .spawn()?;

    // `Zeroizing` wipes this buffer when it drops: `job.password` (a
    // `Secret`, already zeroized on its own drop) is serialized into a
    // second, temporary copy here that `secrecy` has no reach into, purely
    // to get it onto the wire to the child.
    let job = zeroize::Zeroizing::new(
        serde_json::to_vec(&job)
            .map_err(|err| EngineError::new(ErrorKind::Internal, err.to_string()))?,
    );
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&job).await?;
        // Dropping stdin closes it, which tells the child the job is complete.
    }
    let stdout = child.stdout.take();
    // Read concurrently with stdout, on its own task, for as long as the
    // child runs: its tracing can write more than a pipe's buffer holds
    // (see `CHILD_STDERR_TAIL`'s own doc comment), and reading it only
    // after the child exits, as stdout's own loop below used to, would let
    // that write block forever while the child still holds the repository
    // lock.
    let stderr_tail = child
        .stderr
        .take()
        .map(|stderr| tokio::spawn(drain_stderr_tail(stderr)));

    let group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw);
    let handle = ChildHandle(Arc::new(Mutex::new(child)), group);
    let _ = out.send(ChildEvent::Started(handle.clone())).await;

    let mut reported = false;
    if let Some(stdout) = stdout {
        let mut lines = BufReader::new(stdout).lines();
        let mut exited = std::pin::pin!(wait(&handle));
        // Once the child has exited, what it wrote is already in the pipe;
        // anything still holding the pipe open is an orphan it started.
        let mut deadline = None;
        loop {
            let line = tokio::select! {
                line = lines.next_line() => line,
                _ = &mut exited, if deadline.is_none() => {
                    deadline = Some(tokio::time::Instant::now() + DRAIN_AFTER_EXIT);
                    continue;
                }
                () = sleep_until(deadline) => {
                    // Still holding the pipe, so still in the group: the
                    // group ID cannot have been reused.
                    debug_log!(ENGINE, "child exited but its output stayed open");
                    handle.kill_group();
                    break;
                }
            };
            let Ok(Some(line)) = line else {
                break;
            };
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
    let diagnostics = match stderr_tail {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    let diagnostics = tail_for_detail(&diagnostics);
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

/// Reads `stderr` to the end, keeping only the last `CHILD_STDERR_TAIL`
/// bytes: enough for diagnostics without buffering an unreadable-file
/// warning per file in a large home folder without bound.
async fn drain_stderr_tail(mut stderr: tokio::process::ChildStderr) -> String {
    let mut tail: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tail.extend_from_slice(&buffer[..n]);
                if tail.len() > CHILD_STDERR_TAIL {
                    let excess = tail.len() - CHILD_STDERR_TAIL;
                    tail.drain(..excess);
                }
            }
        }
    }
    String::from_utf8_lossy(&tail).into_owned()
}

/// The most recent `CHILD_STDERR_DETAIL` bytes of `text`, landing on a
/// `char` boundary, for whatever goes into an error message or the log.
fn tail_for_detail(text: &str) -> &str {
    if text.len() <= CHILD_STDERR_DETAIL {
        return text;
    }
    let start = text.len() - CHILD_STDERR_DETAIL;
    let boundary = (start..=text.len())
        .find(|&i| text.is_char_boundary(i))
        .unwrap_or(text.len());
    &text[boundary..]
}

/// Sleep until `deadline`, or forever without one.
async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Wait for the child without holding its lock, so it can still be canceled.
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
