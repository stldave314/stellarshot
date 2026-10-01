// SPDX-License-Identifier: GPL-3.0-only

//! Running a write operation in a `stellarshot --run` child process and
//! turning its output into a stream the UI can consume.

use std::fmt;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use cosmic::iced::futures::{SinkExt, Stream, channel::mpsc};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};

use crate::constants::{
    CHILD_STDERR_DETAIL, CHILD_STDERR_TAIL, DRAIN_AFTER_EXIT, PROCESS_POLL_INTERVAL, TERM_GRACE,
};
use crate::debug::ENGINE;
use crate::engine::{EngineError, ErrorKind};
use crate::runner::{Event, Job, Operation};
use crate::{debug_log, error_log};

/// A running child, shared so the UI can cancel it.
#[derive(Clone)]
pub struct ChildHandle {
    child: Arc<Mutex<Child>>,
    /// The child's own process group (see `drive`'s `process_group(0)`).
    group: Option<rustix::process::Pid>,
    /// When `cancel` was called, if it was: `wait` escalates to killing
    /// the group once `TERM_GRACE` has passed since, and `drive` reports a
    /// child that died by signal as canceled only if this is set.
    canceled: Arc<Mutex<Option<Instant>>>,
    /// Whether `wait` has already escalated, so it does so once.
    escalated: Arc<AtomicBool>,
}

impl fmt::Debug for ChildHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChildHandle")
    }
}

impl ChildHandle {
    /// Stop the operation. Safe for a backup: the snapshot file is written
    /// last, so nothing half-written is ever referenced.
    ///
    /// SIGTERM to the child alone, not SIGKILL to its whole group as this
    /// used to be: the child's own `proc_signal` thread then runs the
    /// backup's `After` hooks (a `Before` hook that stopped a database
    /// would otherwise leave it stopped), reports the run canceled over its
    /// stdout, and stops its own group itself — rustic's `rclone serve`
    /// child included, which an rclone left behind would otherwise keep
    /// the child's stdout open with, so the operation was never seen to
    /// end. `wait` kills the group outright if the child is still there
    /// after `TERM_GRACE`, and `drive` does so once more after the child
    /// exits, in case anything was left.
    pub fn cancel(&self) {
        let Ok(child) = self.child.lock() else {
            return;
        };
        // `id()` is `None` once the child has been reaped, after which its
        // PID could in time belong to someone else.
        let Some(pid) = child
            .id()
            .and_then(|id| i32::try_from(id).ok())
            .and_then(rustix::process::Pid::from_raw)
        else {
            return;
        };
        let mut canceled = self.canceled.lock().unwrap_or_else(PoisonError::into_inner);
        if canceled.is_some() {
            return;
        }
        debug_log!(ENGINE, "canceling child {pid:?}");
        *canceled = Some(Instant::now());
        if let Err(err) = rustix::process::kill_process(pid, rustix::process::Signal::TERM) {
            debug_log!(ENGINE, "sending SIGTERM to {pid:?}: {err}");
        }
    }

    /// Whether `cancel` has been called.
    fn canceled_at(&self) -> Option<Instant> {
        *self.canceled.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Kill every process in the child's group.
    fn kill_group(&self) {
        if let Some(group) = self.group
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
///
/// Spawns [`crate::exe::running_image`] (`/proc/self/exe`), not
/// [`crate::exe::installed_path`]: the child's stdin/stdout JSON protocol
/// must match this exact process's own, and `/proc/self/exe` keeps
/// resolving to that exact binary even once a package upgrade has unlinked
/// the path it was launched from — confirmed directly, not assumed (see
/// that function's own doc comment). `AppUpdated` is reachable now only if
/// spawning genuinely fails, not merely because the window has been
/// running since before an upgrade.
pub fn run(operation: Operation, job: Job) -> impl Stream<Item = ChildEvent> {
    run_with(Ok(crate::exe::running_image()), operation, job)
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
/// it was canceled before it could.
async fn drive(
    exe: std::io::Result<std::path::PathBuf>,
    operation: Operation,
    job: Job,
    out: &mut mpsc::Sender<ChildEvent>,
) -> Result<bool, EngineError> {
    let exe = exe?;
    let mut child = Command::new(&exe)
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
        .spawn()
        .map_err(|err| spawn_error(&exe, err))?;

    // `Zeroizing` wipes this buffer when it drops: `job.password` (a
    // `Secret`, already zeroized on its own drop) is serialized into a
    // second, temporary copy here that `secrecy` has no reach into, purely
    // to get it onto the wire to the child.
    let job = zeroize::Zeroizing::new(
        serde_json::to_vec(&job)
            .map_err(|err| EngineError::new(ErrorKind::Internal, err.to_string()))?,
    );
    if let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(&job).await
    {
        // A child that died at once (a crash before it read anything)
        // closes its end and this fails with EPIPE. Not worth returning
        // for on its own: what the child left on stderr, read below, says
        // what actually happened, where a bare "broken pipe" would not.
        debug_log!(ENGINE, "writing the job to the child: {err}");
    }
    // Dropping stdin closes it, which tells the child the job is complete.
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
    let handle = ChildHandle {
        child: Arc::new(Mutex::new(child)),
        group,
        canceled: Arc::new(Mutex::new(None)),
        escalated: Arc::new(AtomicBool::new(false)),
    };
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
    let canceled = handle.canceled_at().is_some();
    if canceled {
        // The child stops its own group on SIGTERM (see `proc_signal`), and
        // `wait` already killed it if it never answered; this is for
        // anything either of those left. The child was reaped only just
        // now, so its group ID has had no time to be given out again —
        // and a group with nothing left in it is a harmless ESRCH.
        handle.kill_group();
    }
    if reported {
        return Ok(true);
    }
    let diagnostics = match stderr_tail {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    let diagnostics = crate::bounded::tail_str(&diagnostics, CHILD_STDERR_DETAIL);
    use std::os::unix::process::ExitStatusExt;
    // A child told to stop exits 143 (128 + SIGTERM) itself once it has run
    // its After hooks (see `proc_signal`), rather than dying of the signal.
    if canceled && status.code() == Some(128 + libc::SIGTERM) {
        debug_log!(ENGINE, "canceled child stopped itself");
        return Ok(false);
    }
    if let Some(signal) = status.signal() {
        if canceled {
            debug_log!(ENGINE, "canceled child ended by signal {signal}");
            return Ok(false);
        }
        // Not ours: the OOM killer, a segfault, an abort. Reporting that
        // as "canceled" hid every such crash from the History page and
        // the log, since a canceled backup is deliberately not recorded.
        error_log!(
            ENGINE,
            "child was stopped by signal {signal} with no outcome: {diagnostics}"
        );
        return Err(EngineError::new(
            ErrorKind::Internal,
            format!("stopped by signal {signal}: {}", diagnostics.trim()),
        ));
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

/// `err` from failing to spawn `exe`, kept a plain [`ErrorKind::Io`] unless
/// `exe` is [`crate::exe::running_image`] itself — a failure to spawn
/// `/proc/self/exe` specifically has no ordinary explanation left once that
/// function's own confirmed guarantee is trusted, short of something
/// upgrade-related it did not anticipate; reported the same way an upgrade
/// already is, rather than a bare, unhelpful `Io`. A path a test injects
/// here instead (to exercise spawn failure in general, unrelated to
/// upgrades) is reported plainly.
fn spawn_error(exe: &std::path::Path, err: std::io::Error) -> EngineError {
    if exe == crate::exe::running_image() {
        EngineError::new(ErrorKind::AppUpdated, err.to_string())
    } else {
        EngineError::from(err)
    }
}

/// Reads `stderr` to the end, keeping only the last `CHILD_STDERR_TAIL`
/// bytes: enough for diagnostics without buffering an unreadable-file
/// warning per file in a large home folder without bound.
async fn drain_stderr_tail(stderr: tokio::process::ChildStderr) -> String {
    let tail = crate::bounded::read_tail_async(stderr, CHILD_STDERR_TAIL).await;
    String::from_utf8_lossy(&tail).into_owned()
}

/// Sleep until `deadline`, or forever without one.
async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Wait for the child without holding its lock, so it can still be
/// canceled. A canceled child that has not gone within `TERM_GRACE` of the
/// SIGTERM — a hook of its own that will not finish, or a child from
/// before `proc_signal` existed — has its whole group killed, once.
async fn wait(handle: &ChildHandle) -> Result<std::process::ExitStatus, EngineError> {
    loop {
        let finished = handle
            .child
            .lock()
            .map_err(|_| EngineError::new(ErrorKind::Internal, "child lock poisoned"))?
            .try_wait()?;
        if let Some(status) = finished {
            return Ok(status);
        }
        if let Some(at) = handle.canceled_at()
            && at.elapsed() >= TERM_GRACE
            && !handle.escalated.swap(true, Ordering::SeqCst)
        {
            debug_log!(
                ENGINE,
                "child did not stop within {}s of SIGTERM; killing its group",
                TERM_GRACE.as_secs()
            );
            handle.kill_group();
        }
        tokio::time::sleep(PROCESS_POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn not_found() -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::NotFound, "gone")
    }

    #[test]
    fn a_failure_to_spawn_running_image_itself_is_reported_as_an_update() {
        let error = spawn_error(&crate::exe::running_image(), not_found());
        assert_eq!(error.kind, ErrorKind::AppUpdated);
    }

    #[test]
    fn a_failure_to_spawn_anything_else_is_reported_plainly() {
        let error = spawn_error(
            std::path::Path::new("/nonexistent/stellarshot"),
            not_found(),
        );
        assert_eq!(error.kind, ErrorKind::Io);
    }
}
