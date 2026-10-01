// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! The window's side of the `--run` protocol: spawning the child, streaming
//! its events, and canceling it.

use std::path::PathBuf;

use cosmic::iced::futures::StreamExt;
use stellarshot::app::child::{self, ChildEvent};
use stellarshot::engine::{self, BackupRequest, ErrorKind, Location, Phase, Secret};
use stellarshot::runner::{Event, Job, Operation};
use tempfile::TempDir;

const PASSWORD: &str = "correct horse battery staple";

fn exe() -> std::io::Result<PathBuf> {
    Ok(PathBuf::from(env!("CARGO_BIN_EXE_stellarshot")))
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn setup(files: usize) -> (TempDir, Job) {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    for index in 0..files {
        let block: Vec<u8> = (0..1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        std::fs::write(source.join(format!("{index}.bin")), block).unwrap();
    }
    let location = Location::local(dir.path().join("repo"));
    engine::init(&location, &Secret::new(PASSWORD)).unwrap();
    let job = Job {
        request: Some(BackupRequest {
            sources: vec![source],
            ..BackupRequest::default()
        }),
        ..Job::new(location, Secret::new(PASSWORD))
    };
    (dir, job)
}

/// `count` unreadable subdirectories under the source: each one makes
/// rustic_core log a warning to the child's stderr, which is how REL-3's
/// regression test reproduces a pipe full enough to block a `write(2)` that
/// never gets drained.
fn setup_with_unreadable_dirs(count: usize) -> (TempDir, Job) {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir_all(&source).unwrap();
    for index in 0..count {
        let sub = source.join(format!("locked-{index}"));
        std::fs::create_dir(&sub).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let location = Location::local(dir.path().join("repo"));
    engine::init(&location, &Secret::new(PASSWORD)).unwrap();
    let job = Job {
        request: Some(BackupRequest {
            sources: vec![source],
            ..BackupRequest::default()
        }),
        ..Job::new(location, Secret::new(PASSWORD))
    };
    (dir, job)
}

#[test]
fn a_backup_does_not_hang_on_a_full_stderr_pipe() {
    let (_dir, job) = setup_with_unreadable_dirs(2000);

    let result = runtime().block_on(async {
        tokio::time::timeout(
            std::time::Duration::from_secs(60),
            child::run_with(exe(), Operation::Backup, job).collect::<Vec<_>>(),
        )
        .await
    });

    let events = result
        .expect("the backup must finish inside the timeout, not deadlock on a full stderr pipe");
    assert!(
        matches!(events.last(), Some(ChildEvent::Event(Event::Done { .. }))),
        "last event: {:?}",
        events.last()
    );
}

#[test]
fn a_backup_streams_started_progress_and_done() {
    let (_dir, job) = setup(2);
    let events: Vec<ChildEvent> =
        runtime().block_on(child::run_with(exe(), Operation::Backup, job).collect());

    assert!(matches!(events.first(), Some(ChildEvent::Started(_))));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, ChildEvent::Event(Event::Progress { .. })))
    );
    assert!(
        matches!(
            events.last(),
            Some(ChildEvent::Event(Event::Done {
                report: Some(_),
                ..
            }))
        ),
        "last event: {:?}",
        events.last()
    );
}

#[test]
fn canceling_a_backup_ends_it_as_canceled_without_a_snapshot() {
    let (_dir, job) = setup(48);
    let location = job.repository.clone();

    let events = runtime().block_on(async {
        let mut stream = std::pin::pin!(child::run_with(exe(), Operation::Backup, job));
        let mut handle = None;
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            match &event {
                ChildEvent::Started(started) => handle = Some(started.clone()),
                ChildEvent::Event(Event::Progress { progress })
                    if progress.phase == Phase::BackingUp && progress.done > 0 =>
                {
                    handle.as_ref().expect("started before progress").cancel();
                }
                _ => {}
            }
            events.push(event);
        }
        events
    });

    // Either shape is a canceled backup to the window (see the profile
    // page's `Backup` handling): `Event::Error` when the child got the
    // chance to say so itself after SIGTERM, `Ended` if it had to be
    // killed outright instead.
    match events.last() {
        Some(ChildEvent::Ended(error) | ChildEvent::Event(Event::Error { error })) => {
            assert_eq!(error.kind, ErrorKind::Canceled);
        }
        other => panic!("expected the backup to end canceled, got {other:?}"),
    }
    let snapshots = engine::open(&location, &Secret::new(PASSWORD))
        .unwrap()
        .snapshots()
        .unwrap();
    assert!(
        snapshots.is_empty(),
        "a canceled backup must not leave a snapshot"
    );
}

/// A `Before` hook that stopped a service must have its `After` hook run
/// when the backup is canceled, not only when it finishes. Cancel used to
/// be SIGKILL to the whole group, which no hook can run after; now it is
/// SIGTERM to the child, which runs them itself (see `proc_signal`) and
/// only then goes.
#[test]
fn canceling_a_backup_still_runs_its_after_hooks() {
    use stellarshot::profile::{Hook, HookTiming};

    let (dir, mut job) = setup(48);
    let before = dir.path().join("before-ran");
    let after = dir.path().join("after-ran");
    let hook = |name: &str, marker: &std::path::Path, timing: HookTiming| Hook {
        name: name.to_owned(),
        command: format!("touch {}", marker.display()),
        timing,
        enabled: true,
    };
    job.hooks = vec![
        hook("before", &before, HookTiming::Before),
        hook("after", &after, HookTiming::After),
    ];

    let events = runtime().block_on(async {
        // Well inside `TERM_GRACE`: a child that had to be *killed* after
        // ignoring SIGTERM would only end once that grace had run out,
        // which is exactly the failure this test exists to catch.
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let mut stream = std::pin::pin!(child::run_with(exe(), Operation::Backup, job));
            let mut handle = None;
            let mut events = Vec::new();
            while let Some(event) = stream.next().await {
                match &event {
                    ChildEvent::Started(started) => handle = Some(started.clone()),
                    ChildEvent::Event(Event::Progress { progress })
                        if progress.phase == Phase::BackingUp && progress.done > 0 =>
                    {
                        assert!(before.exists(), "the Before hook runs before any progress");
                        handle.as_ref().expect("started before progress").cancel();
                    }
                    _ => {}
                }
                events.push(event);
            }
            events
        })
        .await
        .expect("a canceled backup must end on its own, not wait to be killed")
    });

    assert!(
        after.exists(),
        "the After hook must run on cancel; last event: {:?}",
        events.last()
    );
    match events.last() {
        Some(ChildEvent::Ended(error) | ChildEvent::Event(Event::Error { error })) => {
            assert_eq!(error.kind, ErrorKind::Canceled);
        }
        other => panic!("expected the backup to end canceled, got {other:?}"),
    }
}

/// A child that dies of something other than Cancel — a segfault here, the
/// OOM killer in real life — is a failure with a reason, not "canceled".
/// Reporting it as canceled kept every such crash out of the History page
/// and the log, since a canceled backup is deliberately not recorded.
#[test]
fn a_child_killed_by_a_signal_is_a_failure_not_a_cancel() {
    use std::os::unix::fs::PermissionsExt;

    let (dir, job) = setup(0);
    let script = dir.path().join("crash.sh");
    std::fs::write(&script, "#!/bin/sh\nkill -SEGV $$\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let events: Vec<ChildEvent> =
        runtime().block_on(child::run_with(Ok(script), Operation::Backup, job).collect());

    match events.last() {
        Some(ChildEvent::Ended(error)) => {
            assert_eq!(error.kind, ErrorKind::Internal, "{error:?}");
            assert!(error.detail.contains("signal"), "{error:?}");
        }
        other => panic!("expected a failure with a reason, got {other:?}"),
    }
}

// REL-11: `run` now spawns `crate::exe::running_image` (`/proc/self/exe`)
// rather than a raw `current_exe()`, and the lossy `" (deleted)"` string
// check this test used to exercise is gone along with it — that path stays
// executable even after the file it once named is unlinked (confirmed
// directly: a running process whose own backing file was deleted still
// re-executed itself successfully through this exact link; see
// `crate::exe::running_image`'s own doc comment and its test). What
// remains reachable of `AppUpdated` — `running_image` itself somehow
// failing to spawn — is covered by `src/app/child.rs`'s own
// `spawn_error` unit tests instead, which do not need a real subprocess.

#[test]
fn a_missing_executable_is_reported_not_hung() {
    let (_dir, job) = setup(0);
    let events: Vec<ChildEvent> = runtime().block_on(
        child::run_with(
            Ok(PathBuf::from("/nonexistent/stellarshot")),
            Operation::Backup,
            job,
        )
        .collect(),
    );

    match events.as_slice() {
        [ChildEvent::Ended(error)] => assert_eq!(error.kind, ErrorKind::Io),
        other => panic!("expected a single error, got {other:?}"),
    }
}

/// SEC-8's own regression test: the `--run` child holds the repository
/// password in memory, so it disables core dumps for itself
/// (`rustix::process::set_dumpable_behavior`) before doing anything else —
/// `runner::main`'s very first line.
///
/// Against a real child, not this test's own process: an earlier version
/// set and read back the dumpable flag *here*, via the same `rustix` calls
/// `runner::main` uses, which proved those calls work and nothing about
/// `runner::main` — deleting its `prctl` left that test green. This one
/// spawns the actual binary with `--run backup` and its stdin held open,
/// so it runs the `prctl` and then blocks reading its job, and looks at
/// what the kernel then shows another process of the same user.
///
/// Two things are checked, both portable across a bare-metal kernel and
/// GitHub Actions' own runners (where `/proc/<pid>/mem` of a non-dumpable
/// process was observed to become owned by `1001`, not `0` — a detail of
/// how root is mapped there, not a sign the `prctl` failed): `/proc/<pid>/mem`
/// is no longer owned by this user, and `/proc/<pid>/environ` — the
/// process's own memory, in the way a core dump would also expose it — can
/// no longer be read. (`/proc/<pid>` itself, the directory, keeps the
/// user's ownership on at least one current kernel; it is the per-file
/// entries that change hands.) A `sleep` child of this same user is the
/// control for both: readable and owned by us, or this test could not tell
/// a working `prctl` from a `/proc` that hides everything.
#[test]
fn the_run_child_hides_its_memory_from_other_processes_of_the_same_user() {
    use std::os::unix::fs::MetadataExt;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    fn proc_owner(pid: u32) -> Option<u32> {
        std::fs::metadata(format!("/proc/{pid}/mem"))
            .ok()
            .map(|m| m.uid())
    }
    fn environ(pid: u32) -> std::io::Result<Vec<u8>> {
        std::fs::read(format!("/proc/{pid}/environ"))
    }

    // The control: an ordinary, dumpable child of this user.
    let mut control = Command::new("sleep")
        .arg("30")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let control_owner = proc_owner(control.id());
    let control_environ = environ(control.id());
    let _ = control.kill();
    let _ = control.wait();
    let our_uid = rustix::process::getuid().as_raw();
    assert_eq!(
        control_owner,
        Some(our_uid),
        "a dumpable child must show up as ours, or nothing below proves anything"
    );
    control_environ.expect("a dumpable child's environ must be readable by its own user");

    // The real thing. `--run backup` with stdin piped but never written:
    // `runner::main` sets the flag, then blocks in `read_to_end` on its job
    // — while `child.stdin` stays open below, which it does until `child`
    // is dropped.
    let mut child = Command::new(env!("CARGO_BIN_EXE_stellarshot"))
        .args(["--run", "backup"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The `prctl` is the child's first act, but "first" is still after
    // exec; poll rather than race it.
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut owner = proc_owner(child.id());
    while owner == Some(our_uid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
        owner = proc_owner(child.id());
    }
    let child_environ = environ(child.id());
    let _ = child.kill();
    let _ = child.wait();

    assert_ne!(
        owner,
        Some(our_uid),
        "/proc/<pid>/mem of the --run child must not stay owned by this user: the prctl in \
         runner::main did not take effect (owner was {owner:?})"
    );
    assert_eq!(
        child_environ.map(|_| ()).unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied,
        "the --run child's memory must not be readable by another process of the same user"
    );
}

/// Cancel reaches more than a backup: a restore (and a check, a clean-up, a
/// snapshot delete) told to stop must end canceled, not as an "internal
/// error: exit status 143". Canceled right at the start, so the SIGTERM can
/// land before the child has even taken its lock.
#[test]
fn canceling_a_restore_ends_it_as_canceled_not_as_a_failure() {
    use stellarshot::engine::{ConflictPolicy, NoProgress, RestoreRequest, Target};

    let (dir, mut job) = setup(24);
    let request = job.request.take().unwrap();
    engine::open(&job.repository, &Secret::new(PASSWORD))
        .unwrap()
        .backup(&request, std::sync::Arc::new(NoProgress))
        .unwrap();
    job.restore = Some(RestoreRequest {
        snapshot: "latest".to_owned(),
        paths: request.sources.clone(),
        target: Target::Folder(dir.path().join("out")),
        policy: ConflictPolicy::Overwrite,
        ..RestoreRequest::default()
    });

    let events = runtime().block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let mut stream = std::pin::pin!(child::run_with(exe(), Operation::Restore, job));
            let mut events = Vec::new();
            while let Some(event) = stream.next().await {
                if let ChildEvent::Started(handle) = &event {
                    handle.cancel();
                }
                events.push(event);
            }
            events
        })
        .await
        .expect("a canceled restore must end on its own")
    });

    match events.last() {
        Some(ChildEvent::Ended(error) | ChildEvent::Event(Event::Error { error })) => {
            assert_eq!(error.kind, ErrorKind::Canceled, "{error:?}");
        }
        // Finishing before the signal landed is acceptable too; anything
        // else is the bug.
        Some(ChildEvent::Event(Event::Done { .. })) => {}
        other => panic!("expected a canceled restore, got {other:?}"),
    }
}

/// A Before hook that fails after an earlier one succeeded: the earlier one
/// may have stopped a service, so the After hooks still run.
#[test]
fn a_failing_before_hook_still_runs_the_after_hooks() {
    use stellarshot::profile::{Hook, HookTiming};

    let (dir, mut job) = setup(1);
    let after = dir.path().join("after-ran");
    let hook = |name: &str, command: String, timing: HookTiming| Hook {
        name: name.to_owned(),
        command,
        timing,
        enabled: true,
    };
    job.hooks = vec![
        hook("stop the service", "true".to_owned(), HookTiming::Before),
        hook("fails", "false".to_owned(), HookTiming::Before),
        hook(
            "start the service",
            format!("touch {}", after.display()),
            HookTiming::After,
        ),
    ];

    let events: Vec<ChildEvent> =
        runtime().block_on(child::run_with(exe(), Operation::Backup, job).collect());

    match events.last() {
        Some(ChildEvent::Event(Event::Error { error })) => {
            assert_eq!(error.kind, ErrorKind::HookFailed, "{error:?}");
        }
        other => panic!("expected the hook failure, got {other:?}"),
    }
    assert!(
        after.exists(),
        "the After hook must run when a Before hook fails"
    );
}

/// Cancel while a Before hook is still running: the hook is stopped (it is
/// in its own process group, which the child's own SIGTERM to its group
/// does not reach), the After hooks run, and the run ends canceled.
#[test]
fn canceling_during_a_before_hook_stops_it_and_runs_the_after_hooks() {
    use stellarshot::profile::{Hook, HookTiming};

    let (dir, mut job) = setup(1);
    let pid_file = dir.path().join("hook-pid");
    let after = dir.path().join("after-ran");
    job.hooks = vec![
        Hook {
            name: "slow".to_owned(),
            command: format!("sh -c 'echo $$ > {}; exec sleep 60'", pid_file.display()),
            timing: HookTiming::Before,
            enabled: true,
        },
        Hook {
            name: "after".to_owned(),
            command: format!("touch {}", after.display()),
            timing: HookTiming::After,
            enabled: true,
        },
    ];

    let events = runtime().block_on(async {
        tokio::time::timeout(std::time::Duration::from_secs(60), async {
            let mut stream = std::pin::pin!(child::run_with(exe(), Operation::Backup, job));
            let mut events = Vec::new();
            while let Some(event) = stream.next().await {
                if let ChildEvent::Started(handle) = &event {
                    let handle = handle.clone();
                    let pid_file = pid_file.clone();
                    tokio::spawn(async move {
                        while !pid_file.exists() {
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                        handle.cancel();
                    });
                }
                events.push(event);
            }
            events
        })
        .await
        .expect("a run canceled during a Before hook must end on its own")
    });

    match events.last() {
        Some(ChildEvent::Ended(error) | ChildEvent::Event(Event::Error { error })) => {
            assert_eq!(error.kind, ErrorKind::Canceled, "{error:?}");
        }
        other => panic!("expected a canceled run, got {other:?}"),
    }
    assert!(after.exists(), "the After hook must run");
    let pid = std::fs::read_to_string(&pid_file).unwrap();
    let mut alive = true;
    for _ in 0..50 {
        alive = std::path::Path::new(&format!("/proc/{}", pid.trim())).exists()
            && !std::fs::read_to_string(format!("/proc/{}/stat", pid.trim()))
                .unwrap_or_default()
                .contains(") Z ");
        if !alive {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(!alive, "the Before hook's process must be stopped");
}
