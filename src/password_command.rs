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

use crate::engine::{EngineError, ErrorKind, Secret};

/// Run `command` and use its standard output, with one trailing newline
/// trimmed if there is one (what nearly every password-manager CLI prints),
/// as the password. Never logs the command's output, only whether it
/// succeeded.
pub async fn run(command: &str) -> Result<Secret, EngineError> {
    let args = shell_words::split(command)
        .map_err(|err| EngineError::new(ErrorKind::Internal, format!("password command: {err}")))?;
    let Some((program, args)) = args.split_first() else {
        return Err(EngineError::new(
            ErrorKind::Internal,
            "the password command is empty",
        ));
    };
    let output = tokio::process::Command::new(program)
        .args(args)
        .output()
        .await
        .map_err(|err| EngineError::new(ErrorKind::Internal, format!("password command: {err}")))?;
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
}
