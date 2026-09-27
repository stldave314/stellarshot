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
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::constants::{DRAIN_AFTER_EXIT, HOOK_TIMEOUT};
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
        // Its own process group, so a background process the hook starts
        // (`sh -c 'mydaemon &'`) can be reached too: see `kill_group` and
        // `wait_with_timeout`'s own doc comment.
        .process_group(0)
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

/// Kill every process in `child`'s own group (see `run_command`'s
/// `process_group(0)`), not only `child` itself: a hook like
/// `sh -c 'mydaemon &'` returns at once, but a background process it
/// started keeps running, and keeps stderr's write end open, in the same
/// group.
fn kill_group(child: &Child) {
    if let Some(pid) = i32::try_from(child.id())
        .ok()
        .and_then(rustix::process::Pid::from_raw)
    {
        let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
    }
}

/// Waits for `child`, killing its whole process group if it runs longer
/// than `timeout`. Reads stderr on its own thread the whole time, so a hook
/// that writes more than fits in the pipe's buffer cannot deadlock against
/// a poll loop that never drains it; once `child` itself has exited, that
/// read is bounded to at most `DRAIN_AFTER_EXIT` more, since what is
/// holding the pipe open past that point is something the hook left
/// running behind it, not the hook itself finishing up. If even killing the
/// group does not free the pipe (it always should), the read still returns
/// once the kernel actually closes the last write end, rather than being
/// abandoned outright.
fn wait_with_timeout(mut child: Child, timeout: Duration) -> Result<(ExitStatus, String), String> {
    let mut stderr = child.stderr.take();
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.read_to_end(&mut buffer);
        }
        let _ = sender.send(String::from_utf8_lossy(&buffer).into_owned());
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {
                if start.elapsed() >= timeout {
                    kill_group(&child);
                    let _ = child.wait();
                    break Err(format!("timed out after {}s", timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(err) => break Err(format!("hook: {err}")),
        }
    }?;
    let stderr = match receiver.recv_timeout(DRAIN_AFTER_EXIT) {
        Ok(text) => text,
        Err(_) => {
            kill_group(&child);
            receiver.recv().unwrap_or_default()
        }
    };
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
    fn a_backgrounded_process_does_not_block_on_a_full_stderr_pipe() {
        // The hook itself (`sh`) exits at once; `yes` keeps writing to
        // stderr in the background, in the same process group. Before
        // REL-4, joining the stderr-reading thread waited for every holder
        // of the pipe to close it, which `yes` never does on its own.
        let hooks = vec![hook(
            "backgrounds a talkative child",
            "sh -c '(yes 1>&2 &); true'",
            HookTiming::Before,
        )];
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(run_before(&hooks));
        });
        let result = receiver.recv_timeout(Duration::from_secs(10));
        assert!(
            result.is_ok(),
            "must not hang waiting for a backgrounded child's stderr"
        );
        assert!(result.unwrap().unwrap()[0].ok);
    }

    #[test]
    fn a_timeout_kills_the_whole_group_not_just_the_direct_child() {
        // A UUID, not the thread ID, so the path is both unique (this test
        // shares a process, and even a `std::process::id()` alone, with
        // every other test in the crate) and shell-safe: `ThreadId`'s own
        // `Debug` form contains parentheses, which break unquoted use in
        // the shell command below.
        let marker = std::env::temp_dir().join(format!(
            "stellarshot-hook-group-test-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_file(&marker);
        let child = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "sleep 300 & echo $! > {}; sleep 300",
                marker.display()
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .unwrap();
        // Give the background grandchild a moment to start and record its
        // own PID before the direct child (the outer `sh`) is timed out.
        for _ in 0..50 {
            if marker.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let grandchild_pid = std::fs::read_to_string(&marker).unwrap().trim().to_owned();

        let _ = wait_with_timeout(child, Duration::from_millis(200));

        let still_alive = Command::new("kill")
            .args(["-0", &grandchild_pid])
            .status()
            .unwrap()
            .success();
        let _ = std::fs::remove_file(&marker);
        assert!(
            !still_alive,
            "the timeout must kill the whole group, including a backgrounded \
             grandchild, not just the direct child"
        );
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
