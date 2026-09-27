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

use crate::constants::PASSWORD_COMMAND_TIMEOUT;
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
    let child = tokio::process::Command::new(program)
        .args(args)
        .process_group(0)
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|err| EngineError::new(ErrorKind::Internal, format!("password command: {err}")))?;
    let group = child
        .id()
        .and_then(|id| i32::try_from(id).ok())
        .and_then(rustix::process::Pid::from_raw);
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
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
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(EngineError::new(
            ErrorKind::Internal,
            if detail.is_empty() {
                format!("the password command exited with {}", output.status)
            } else {
                detail
            },
        ));
    }
    let mut password = String::from_utf8(output.stdout).map_err(|_| {
        EngineError::new(
            ErrorKind::Internal,
            "the password command's output was not valid UTF-8",
        )
    })?;
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    if password.is_empty() {
        return Err(EngineError::new(
            ErrorKind::Internal,
            "the password command printed nothing",
        ));
    }
    Ok(Secret::new(password))
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

        let err = run_with_timeout(&command, std::time::Duration::from_millis(200))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::TimedOut);

        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        let grandchild_pid = std::fs::read_to_string(&marker).unwrap().trim().to_owned();
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
