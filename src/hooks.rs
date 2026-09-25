// SPDX-License-Identifier: GPL-3.0-only

//! Commands run before and after a backup: see [`crate::profile::Hook`].
//!
//! Split and run the same way `password_command` is: without invoking a real
//! shell, so a hook's own command line is never subject to shell injection.
//!
//! Synchronous, like the rest of `runner.rs`, which this is called from: a
//! backup runs in its own process precisely so it can be interrupted, and a
//! hook that never returns must not defeat that by hanging a fully
//! synchronous call forever — [`wait_with_timeout`] kills one that overruns
//! [`HOOK_TIMEOUT`] rather than blocking on it.

use std::io::Read;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::constants::HOOK_TIMEOUT;
use crate::profile::{Hook, HookTiming};

/// What happened running one hook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookResult {
    pub name: String,
    pub ok: bool,
    /// Empty on success.
    pub detail: String,
}

/// Run every enabled `Before` hook, in order, stopping at the first
/// failure: a `Before` hook exists to make the backup safe to take, so one
/// that fails must stop the backup rather than let it proceed regardless.
/// `Err` carries every result so far, including the one that failed.
pub fn run_before(hooks: &[Hook]) -> Result<Vec<HookResult>, Vec<HookResult>> {
    let mut results = Vec::new();
    for hook in hooks
        .iter()
        .filter(|hook| hook.enabled && hook.timing == HookTiming::Before)
    {
        let result = run_one(hook);
        let failed = !result.ok;
        results.push(result);
        if failed {
            return Err(results);
        }
    }
    Ok(results)
}

/// Run every enabled hook for the backup's outcome: `AfterSuccess` or
/// `AfterFailure`, plus `After` either way. Every one runs regardless of
/// another's failure, since the backup itself has already finished.
pub fn run_after(hooks: &[Hook], succeeded: bool) -> Vec<HookResult> {
    let outcome = if succeeded {
        HookTiming::AfterSuccess
    } else {
        HookTiming::AfterFailure
    };
    hooks
        .iter()
        .filter(|hook| hook.enabled && (hook.timing == outcome || hook.timing == HookTiming::After))
        .map(run_one)
        .collect()
}

fn run_one(hook: &Hook) -> HookResult {
    match run_command(&hook.command) {
        Ok(()) => HookResult {
            name: hook.name.clone(),
            ok: true,
            detail: String::new(),
        },
        Err(detail) => HookResult {
            name: hook.name.clone(),
            ok: false,
            detail,
        },
    }
}

fn run_command(command: &str) -> Result<(), String> {
    let args = shell_words::split(command).map_err(|err| format!("hook: {err}"))?;
    let Some((program, args)) = args.split_first() else {
        return Err("the hook command is empty".to_owned());
    };
    let child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("hook: {err}"))?;
    let (status, stderr) = wait_with_timeout(child, HOOK_TIMEOUT)?;
    if !status.success() {
        let detail = stderr.trim().to_owned();
        return Err(if detail.is_empty() {
            format!("exited with {status}")
        } else {
            detail
        });
    }
    Ok(())
}

/// Waits for `child`, killing it if it runs longer than `timeout`. Reads
/// stderr on its own thread while waiting, so a hook that writes more than
/// fits in the pipe's buffer cannot deadlock against a poll loop that never
/// drains it.
fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<(ExitStatus, String), String> {
    let mut stderr = child.stderr.take();
    let stderr_thread = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.read_to_string(&mut buffer);
        }
        buffer
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(format!("timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => break Err(format!("hook: {err}")),
        }
    }?;
    let stderr = stderr_thread.join().unwrap_or_default();
    Ok((status, stderr))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(name: &str, command: &str, timing: HookTiming) -> Hook {
        Hook {
            name: name.to_owned(),
            command: command.to_owned(),
            timing,
            enabled: true,
        }
    }

    #[test]
    fn a_before_hook_that_fails_stops_the_rest() {
        let hooks = vec![
            hook("first", "true", HookTiming::Before),
            hook("second", "false", HookTiming::Before),
            hook("third", "true", HookTiming::Before),
        ];
        let results = run_before(&hooks).unwrap_err();
        assert_eq!(
            results.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["first", "second"],
            "the third hook never ran"
        );
        assert!(results[0].ok);
        assert!(!results[1].ok);
    }

    #[test]
    fn every_before_hook_running_cleanly_reports_ok() {
        let hooks = vec![
            hook("first", "true", HookTiming::Before),
            hook("second", "true", HookTiming::Before),
        ];
        let results = run_before(&hooks).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|r| r.ok));
    }

    #[test]
    fn a_disabled_or_differently_timed_hook_does_not_run() {
        let hooks = vec![
            Hook {
                enabled: false,
                ..hook("off", "false", HookTiming::Before)
            },
            hook("after", "false", HookTiming::AfterSuccess),
        ];
        assert_eq!(run_before(&hooks).unwrap(), Vec::new());
    }

    #[test]
    fn after_hooks_run_for_the_matching_outcome_and_always() {
        let hooks = vec![
            hook("on-success", "true", HookTiming::AfterSuccess),
            hook("on-failure", "true", HookTiming::AfterFailure),
            hook("always", "true", HookTiming::After),
        ];
        let names: Vec<String> = run_after(&hooks, true)
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["on-success", "always"]);

        let names: Vec<String> = run_after(&hooks, false)
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["on-failure", "always"]);
    }

    #[test]
    fn one_after_hook_failing_does_not_stop_the_rest() {
        let hooks = vec![
            hook("fails", "false", HookTiming::After),
            hook("still-runs", "true", HookTiming::After),
        ];
        let results = run_after(&hooks, true);
        assert!(!results[0].ok);
        assert!(results[1].ok);
    }

    #[test]
    fn a_failing_hook_reports_its_stderr() {
        let hooks = vec![hook(
            "fails",
            "sh -c 'echo nope 1>&2; exit 1'",
            HookTiming::Before,
        )];
        let err = run_before(&hooks).unwrap_err();
        assert_eq!(err[0].detail, "nope");
    }

    #[test]
    fn unmatched_quoting_is_reported_rather_than_run_incorrectly() {
        let hooks = vec![hook("bad", "echo '", HookTiming::Before)];
        let err = run_before(&hooks).unwrap_err();
        assert!(err[0].detail.contains("hook"));
    }

    #[test]
    fn a_hook_that_never_finishes_is_killed_rather_than_hanging_forever() {
        let child = Command::new("sleep")
            .arg("120")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let err = wait_with_timeout(child, Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }
}
