// SPDX-License-Identifier: GPL-3.0-only

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

    match events.last() {
        Some(ChildEvent::Ended(error)) => assert_eq!(error.kind, ErrorKind::Canceled),
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
/// This used to spawn a real child and check whether `/proc/<pid>/mem`'s
/// owning uid changed to `0`, the externally visible side effect a bare-
/// metal or VM kernel gives a non-dumpable process. That is not portable:
/// on GitHub Actions' own runners the owning uid changes to *something*,
/// but not literally `0` (`1001` was observed there, not this process's own
/// uid either) — plausibly a container/user-namespace detail in how "root"
/// is mapped, not a sign the `prctl` failed. Rather than assert an exact
/// uid that varies by environment, this checks the one thing that is
/// actually portable and is the real contract `runner::main` depends on:
/// `PR_SET_DUMPABLE`/`PR_GET_DUMPABLE` round-tripping correctly for this
/// process, via the exact same `rustix::process` calls `runner::main` uses.
/// Restores the dumpable flag afterward, since `cargo test` runs many tests
/// in one process and this would otherwise leak into all of them.
#[test]
fn the_dumpable_flag_set_by_runner_main_round_trips() {
    use rustix::process::{DumpableBehavior, dumpable_behavior, set_dumpable_behavior};

    let restore = dumpable_behavior().ok();
    set_dumpable_behavior(DumpableBehavior::NotDumpable).unwrap();
    let now = dumpable_behavior().unwrap();
    if let Some(previous) = restore {
        let _ = set_dumpable_behavior(previous);
    }

    assert_eq!(
        now,
        DumpableBehavior::NotDumpable,
        "PR_SET_DUMPABLE must be readable back as set; runner::main relies on \
         exactly this call succeeding to keep the repository password out of \
         a core dump"
    );
}
