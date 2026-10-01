// SPDX-License-Identifier: GPL-3.0-only

//! A repository password from running a command instead of the keyring or
//! typing it, for a password manager with a command-line client (the
//! Bitwarden CLI, `pass`, a Vaultwarden client).
//!
//! The command is split into an argument list the same way rustic_core
//! splits its own `stdin_command` and rclone-command strings: without
//! invoking a real shell, so it is never subject to shell injection, at the
//! cost of not supporting pipes or other shell operators directly. A small
//! wrapper script covers that if it is ever needed.

use zeroize::Zeroizing;

use crate::constants::{
    CHILD_STDERR_DETAIL, DRAIN_AFTER_EXIT, PASSWORD_COMMAND_MAX_OUTPUT, PASSWORD_COMMAND_TIMEOUT,
};
use crate::engine::{EngineError, ErrorKind, Secret};

/// Run `command` and use its standard output, with one trailing newline
/// trimmed if there is one (what nearly every password-manager CLI prints),
/// as the password. Never logs the command's output, only whether it
/// succeeded.
pub async fn run(command: &str) -> Result<Secret, EngineError> {
    run_with_timeout(command, PASSWORD_COMMAND_TIMEOUT).await
}

/// [`run`], with the timeout given explicitly so a test can use one far
/// shorter than [`PASSWORD_COMMAND_TIMEOUT`] against a command that never
/// finishes, rather than actually waiting out the real one.
async fn run_with_timeout(
    command: &str,
    timeout: std::time::Duration,
) -> Result<Secret, EngineError> {
    let args = shell_words::split(command)
        .map_err(|err| EngineError::new(ErrorKind::Internal, format!("password command: {err}")))?;
    let Some((program, args)) = args.split_first() else {
        return Err(EngineError::new(
            ErrorKind::Internal,
            "the password command is empty",
        ));
    };
    // `kill_on_drop` plus wrapping the whole run in a timeout: a command
    // stuck waiting on a prompt nobody can see (a GUI pinentry, a hardware
    // key never plugged in) fails cleanly instead of hanging whatever
    // called this forever — a `--scheduled` run especially, which would
    // otherwise leave its systemd unit "active" and silently skip every
    // later timer fire rather than ever trying again.
    //
    // Its own process group, so a timeout can reach a child the command
    // itself started, not only the command's own direct process:
    // `kill_on_drop` alone (like a plain `child.kill()`) only ever reaches
    // that direct process.
    //
    // No stdin: launched from a terminal, a command that prompts (`bw`,
    // pinentry-curses) would otherwise wait on a tty nobody is watching
    // until the timeout.
    let mut child = tokio::process::Command::new(program)
        .args(args)
        .process_group(0)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| EngineError::new(ErrorKind::Internal, format!("password command: {err}")))?;
    let group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw);
    // Read on tasks, into buffers this function can still take from if the
    // pipe never closes: waiting for end-of-file would let a background
    // process the command left behind (one that `setsid`s away and keeps
    // stdout open) turn a command that printed the password and exited into
    // a timeout.
    let stdout = Capture::start(child.stdout.take(), PASSWORD_COMMAND_MAX_OUTPUT, true);
    let stderr = Capture::start(child.stderr.take(), CHILD_STDERR_DETAIL, false);
    let status = match tokio::time::timeout(timeout, child.wait()).await {
        Ok(result) => result.map_err(|err| {
            EngineError::new(ErrorKind::Internal, format!("password command: {err}"))
        })?,
        Err(_) => {
            if let Some(group) = group {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            return Err(EngineError::new(
                ErrorKind::TimedOut,
                timeout.as_secs().to_string(),
            ));
        }
    };
    // Its own output is already in the pipes; give the readers a moment to
    // move it across, then take what there is.
    let (stdout, truncated) = stdout.finish().await;
    let (stderr, _) = stderr.finish().await;
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr).trim().to_owned();
        return Err(EngineError::new(
            ErrorKind::Internal,
            if detail.is_empty() {
                format!("the password command exited with {status}")
            } else {
                detail
            },
        ));
    }
    if truncated {
        return Err(EngineError::new(
            ErrorKind::Internal,
            "the password command printed far more than a password",
        ));
    }
    // Borrowed, then copied once into the `Secret`: `stdout` itself is
    // zeroized when it goes out of scope, on every path below, so no stray
    // copy of the password outlives this call.
    let text = std::str::from_utf8(&stdout).map_err(|_| {
        EngineError::new(
            ErrorKind::Internal,
            "the password command's output was not valid UTF-8",
        )
    })?;
    let text = text
        .strip_suffix('\n')
        .map_or(text, |rest| rest.strip_suffix('\r').unwrap_or(rest));
    if text.is_empty() {
        return Err(EngineError::new(
            ErrorKind::Internal,
            "the password command printed nothing",
        ));
    }
    Ok(Secret::new(text.to_owned()))
}

/// What a child wrote to one pipe, collected on a task into a buffer that can
/// be taken at any time, even while the pipe is still open.
struct Capture {
    buffer: std::sync::Arc<std::sync::Mutex<Zeroizing<Vec<u8>>>>,
    truncated: std::sync::Arc<std::sync::atomic::AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl Capture {
    /// Keep the first `limit` bytes (`head`), or the last `limit` (not).
    fn start<R>(reader: Option<R>, limit: usize, head: bool) -> Self
    where
        R: tokio::io::AsyncRead + Unpin + Send + 'static,
    {
        use std::sync::atomic::Ordering;
        use tokio::io::AsyncReadExt;
        // Allocated once at its final size: growing would leave copies of
        // what was read behind in freed memory.
        let buffer = std::sync::Arc::new(std::sync::Mutex::new(Zeroizing::new(
            Vec::with_capacity(limit + 1),
        )));
        let truncated = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task = reader.map(|mut reader| {
            let (buffer, truncated) = (buffer.clone(), truncated.clone());
            tokio::spawn(async move {
                let mut chunk = Zeroizing::new([0u8; 4096]);
                loop {
                    let n = match reader.read(&mut chunk[..]).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let Ok(mut kept) = buffer.lock() else { break };
                    if head {
                        let room = limit.saturating_sub(kept.len());
                        kept.extend_from_slice(&chunk[..room.min(n)]);
                        if n > room {
                            truncated.store(true, Ordering::Relaxed);
                        }
                    } else {
                        kept.extend_from_slice(&chunk[..n]);
                        if kept.len() > limit {
                            let excess = kept.len() - limit;
                            kept.drain(..excess);
                        }
                    }
                }
            })
        });
        Self {
            buffer,
            truncated,
            task,
        }
    }

    /// Wait up to [`DRAIN_AFTER_EXIT`] for the pipe to close, then stop
    /// reading and take what was collected, and whether more than the limit
    /// was written (for a head only).
    async fn finish(mut self) -> (Zeroizing<Vec<u8>>, bool) {
        if let Some(task) = self.task.take() {
            let mut task = task;
            if tokio::time::timeout(DRAIN_AFTER_EXIT, &mut task)
                .await
                .is_err()
            {
                task.abort();
            }
        }
        let taken = self
            .buffer
            .lock()
            .map(|mut kept| Zeroizing::new(std::mem::take(&mut **kept)))
            .unwrap_or_default();
        (
            taken,
            self.truncated.load(std::sync::atomic::Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_commands_output_becomes_the_password() {
        let secret = run("printf hunter2").await.unwrap();
        assert_eq!(secret.expose(), "hunter2");
    }

    #[tokio::test]
    async fn a_trailing_newline_is_trimmed_but_no_more_than_one() {
        let secret = run("printf 'hunter2\\n\\n'").await.unwrap();
        assert_eq!(secret.expose(), "hunter2\n");
    }

    #[tokio::test]
    async fn a_background_process_holding_stdout_does_not_turn_success_into_a_timeout() {
        let started = std::time::Instant::now();

        let secret = run("sh -c 'setsid sleep 30 & printf pw'").await.unwrap();

        assert_eq!(secret.expose(), "pw");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "took {:?}: waited for the orphan's pipe instead of the command",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn output_far_beyond_a_password_is_refused_not_buffered() {
        let err = run("sh -c 'head -c 50000000 /dev/zero | tr \"\\0\" x'")
            .await
            .unwrap_err();
        assert!(err.detail.contains("far more than a password"), "{err:?}");
    }

    #[tokio::test]
    async fn a_failing_command_reports_its_stderr() {
        let err = run("sh -c 'echo nope 1>&2; exit 1'").await.unwrap_err();
        assert_eq!(err.detail, "nope");
    }

    #[tokio::test]
    async fn empty_output_is_refused_rather_than_an_empty_password() {
        let err = run("true").await.unwrap_err();
        assert!(err.detail.contains("printed nothing"));
    }

    #[tokio::test]
    async fn unmatched_quoting_is_reported_rather_than_run_incorrectly() {
        let err = run("echo '").await.unwrap_err();
        assert!(err.detail.contains("password command"));
    }

    #[tokio::test]
    async fn a_command_that_never_finishes_times_out_rather_than_hanging_forever() {
        let err = run_with_timeout("sleep 120", std::time::Duration::from_millis(200))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn a_timeout_kills_a_backgrounded_grandchild_too() {
        let marker = std::env::temp_dir().join(format!(
            "stellarshot-password-command-group-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&marker);
        let command = format!(
            "sh -c 'sleep 300 & echo $! > {}; sleep 300'",
            marker.display()
        );

        // Two seconds, not 200 ms: the shell has to fork the grandchild and
        // write its PID to the marker *before* the timeout kills the whole
        // group, and under a loaded `cargo test` run 200 ms was not always
        // enough for that — the group was killed with the marker never
        // written, and the read below failed on a file that did not exist
        // rather than on anything this test is about.
        let err = run_with_timeout(&command, std::time::Duration::from_secs(2))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TimedOut);

        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let grandchild_pid = std::fs::read_to_string(&marker)
            .expect("the marker was never written: the shell was killed before it forked")
            .trim()
            .to_owned();
        // The SIGKILL lands immediately, but the grandchild stays visible to
        // `kill -0` as a zombie until whatever it reparents to reaps it —
        // under a busy `cargo test` run with hundreds of tests contending for
        // CPU, that can lag well past the instant the signal was sent. Poll
        // with the same patience as the marker wait above instead of
        // checking once right away.
        let mut still_alive = true;
        for _ in 0..50 {
            still_alive = std::process::Command::new("kill")
                .args(["-0", &grandchild_pid])
                .status()
                .unwrap()
                .success();
            if !still_alive {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let _ = std::fs::remove_file(&marker);
        assert!(
            !still_alive,
            "the timeout must kill the whole group, not just the direct process"
        );
    }
}
