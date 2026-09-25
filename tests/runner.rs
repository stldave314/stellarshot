// SPDX-License-Identifier: GPL-3.0-only

//! The `stellarshot --run` child process, driven exactly as the window drives
//! it: a job on stdin, JSON events on stdout.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};

use stellarshot::engine::{
    self, BackupRequest, ConflictPolicy, ErrorKind, KeepRules, Location, Phase, RestoreRequest,
    Secret, Target, lock,
};
use stellarshot::runner::{Event, Job};
use tempfile::TempDir;

const PASSWORD: &str = "correct horse battery staple";

struct Fixture {
    dir: TempDir,
    location: Location,
    source: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("a file.txt"), b"hello").unwrap();
        let location = Location::local(dir.path().join("repo"));
        engine::init(&location, &Secret::new(PASSWORD)).unwrap();
        Self {
            dir,
            location,
            source,
        }
    }

    /// Lock and progress files for children started by this test, kept apart
    /// from the user's real session and from other tests.
    fn runtime_dir(&self) -> PathBuf {
        self.dir.path().join("runtime")
    }

    fn backup_job(&self, password: &str) -> Job {
        Job {
            request: Some(BackupRequest {
                sources: vec![self.source.clone()],
                ..BackupRequest::default()
            }),
            ..Job::new(self.location.clone(), Secret::new(password))
        }
    }

    fn spawn(&self, operation: &str, job: &Job) -> Child {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stellarshot"))
            .args(["--run", operation])
            .env("XDG_RUNTIME_DIR", self.runtime_dir())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(&serde_json::to_vec(job).unwrap()).unwrap();
        drop(stdin);
        child
    }

    fn run(&self, operation: &str, job: &Job) -> (Vec<Event>, ExitStatus) {
        let output = self.spawn(operation, job).wait_with_output().unwrap();
        let events = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        (events, output.status)
    }

    fn snapshot_count(&self) -> usize {
        engine::open(&self.location, &Secret::new(PASSWORD))
            .unwrap()
            .snapshots()
            .unwrap()
            .len()
    }
}

#[test]
fn runner_backs_up_and_reports_done() {
    let fixture = Fixture::new();

    let (events, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));

    assert!(status.success(), "events: {events:?}");
    match events.last() {
        Some(Event::Done {
            report: Some(report),
            ..
        }) => {
            assert!(report.snapshot.files_new >= 1);
        }
        other => panic!("expected a done event with a report, got {other:?}"),
    }
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Progress { .. })),
        "progress must be reported"
    );
    assert_eq!(fixture.snapshot_count(), 1);
}

#[test]
fn runner_reports_wrong_password() {
    let fixture = Fixture::new();

    let (events, status) = fixture.run("backup", &fixture.backup_job("wrong"));

    assert_eq!(status.code(), Some(1));
    match events.last() {
        Some(Event::Error { error }) => assert_eq!(error.kind, ErrorKind::WrongPassword),
        other => panic!("expected an error event, got {other:?}"),
    }
    assert_eq!(fixture.snapshot_count(), 0);
}

#[test]
fn runner_rejects_an_unknown_operation() {
    let status = Command::new(env!("CARGO_BIN_EXE_stellarshot"))
        .args(["--run", "format-disk"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(2));
}

#[test]
fn second_writer_reports_locked() {
    let fixture = Fixture::new();
    // Hold the lock the way a running backup would.
    let _held = lock::acquire_in(
        &fixture.runtime_dir().join("stellarshot"),
        &fixture.location,
    )
    .unwrap();

    let (events, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));

    assert_eq!(status.code(), Some(1));
    match events.last() {
        Some(Event::Error { error }) => assert_eq!(error.kind, ErrorKind::Locked),
        other => panic!("expected a locked error, got {other:?}"),
    }
    assert_eq!(fixture.snapshot_count(), 0, "nothing may be written");
}

/// Write `count` files of incompressible data, 1 MiB each.
fn bulky_source(source: &Path, count: usize) {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    for index in 0..count {
        let block: Vec<u8> = (0..1024 * 1024)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state >> 24) as u8
            })
            .collect();
        std::fs::write(source.join(format!("bulk-{index:03}.bin")), block).unwrap();
    }
}

#[test]
fn killed_backup_leaves_a_sound_repository() {
    use std::os::unix::process::ExitStatusExt;

    let fixture = Fixture::new();
    bulky_source(&fixture.source, 48);

    let mut child = fixture.spawn("backup", &fixture.backup_job(PASSWORD));
    let stdout = BufReader::new(child.stdout.take().unwrap());
    // Kill as soon as data is being stored: the worst moment, with packs
    // half-written and no snapshot yet.
    for line in stdout.lines() {
        let event: Event = serde_json::from_str(&line.unwrap()).unwrap();
        match event {
            Event::Progress { progress }
                if progress.phase == Phase::BackingUp && progress.done > 0 =>
            {
                child.kill().unwrap();
                break;
            }
            Event::Done { .. } => panic!(
                "the backup finished before it could be interrupted; the fixture is too small to test this"
            ),
            _ => {}
        }
    }
    let status = child.wait().unwrap();
    assert!(
        status.signal().is_some(),
        "the child must have been killed, got {status}"
    );

    // No snapshot was recorded, the repository is still consistent, and the
    // lock died with the process, so the next backup goes ahead.
    assert_eq!(fixture.snapshot_count(), 0);
    engine::open(&fixture.location, &Secret::new(PASSWORD))
        .unwrap()
        .check()
        .expect("an interrupted backup must not damage the repository");
    let (events, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));
    assert!(status.success(), "events: {events:?}");
    assert_eq!(fixture.snapshot_count(), 1);
}

#[test]
fn backup_finishes_after_the_window_goes_away() {
    let fixture = Fixture::new();
    bulky_source(&fixture.source, 16);

    let mut child = fixture.spawn("backup", &fixture.backup_job(PASSWORD));
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut first = String::new();
    stdout.read_line(&mut first).unwrap();
    assert!(
        !first.is_empty(),
        "the child must report before the reader leaves"
    );
    // The window quits: its end of the pipe closes while the backup runs.
    drop(stdout);

    let status = child.wait().unwrap();
    assert!(
        status.success(),
        "a closed stdout must not stop the backup: {status}"
    );
    assert_eq!(fixture.snapshot_count(), 1);
}

#[test]
fn runner_restores_a_selection_keeping_both() {
    let fixture = Fixture::new();
    let (_, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));
    assert!(status.success());
    let file = fixture.source.join("a file.txt");
    std::fs::write(&file, b"changed since").unwrap();

    let job = Job {
        request: None,
        restore: Some(RestoreRequest {
            snapshot: "latest".into(),
            paths: vec![file.clone()],
            target: Target::Original,
            policy: ConflictPolicy::KeepBoth,
            ..RestoreRequest::default()
        }),
        ..fixture.backup_job(PASSWORD)
    };
    let (events, status) = fixture.run("restore", &job);

    assert!(status.success(), "events: {events:?}");
    match events.last() {
        Some(Event::Done {
            restored: Some(restored),
            ..
        }) => {
            assert_eq!(restored.files, 1);
            assert_eq!(restored.conflicts, 1);
        }
        other => panic!("expected a done event with what was restored, got {other:?}"),
    }
    assert_eq!(
        std::fs::read(&file).unwrap(),
        b"changed since",
        "Keep both never touches the existing file"
    );
    let copies: Vec<_> = std::fs::read_dir(&fixture.source)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("a file (restored "))
        .collect();
    assert_eq!(copies.len(), 1, "one restored copy: {copies:?}");
    assert_eq!(
        std::fs::read(fixture.source.join(&copies[0])).unwrap(),
        b"hello"
    );
}

#[test]
fn runner_maintains_by_forgetting_then_pruning() {
    let fixture = Fixture::new();
    for _ in 0..3 {
        let (_, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));
        assert!(status.success());
    }

    let job = Job {
        request: None,
        keep: Some(KeepRules {
            last: Some(1),
            ..KeepRules::default()
        }),
        prune: true,
        ..fixture.backup_job(PASSWORD)
    };
    let (events, status) = fixture.run("maintain", &job);

    assert!(status.success(), "events: {events:?}");
    match events.last() {
        Some(Event::Done {
            forgotten: Some(forgotten),
            pruned: Some(_),
            ..
        }) => assert_eq!((forgotten.removed, forgotten.kept), (2, 1)),
        other => panic!("expected a done event with both reports, got {other:?}"),
    }
    assert_eq!(fixture.snapshot_count(), 1);
}

#[test]
fn runner_pins_a_snapshot_and_protects_it_from_maintain() {
    let fixture = Fixture::new();
    let (events, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));
    assert!(status.success());
    let id = match events.last() {
        Some(Event::Done {
            report: Some(report),
            ..
        }) => report.snapshot.id.clone(),
        other => panic!("expected a done event with a report, got {other:?}"),
    };
    for _ in 0..2 {
        let (_, status) = fixture.run("backup", &fixture.backup_job(PASSWORD));
        assert!(status.success());
    }
    assert_eq!(fixture.snapshot_count(), 3);

    let job = Job {
        request: None,
        ids: vec![id],
        pinned: Some(true),
        ..fixture.backup_job(PASSWORD)
    };
    let (events, status) = fixture.run("set-pinned", &job);
    assert!(status.success(), "events: {events:?}");
    match events.last() {
        Some(Event::Done {
            pinned: Some(summary),
            ..
        }) => assert!(summary.pinned),
        other => panic!("expected a done event with the pinned snapshot, got {other:?}"),
    }

    let job = Job {
        request: None,
        keep: Some(KeepRules {
            last: Some(1),
            ..KeepRules::default()
        }),
        prune: true,
        ..fixture.backup_job(PASSWORD)
    };
    let (_, status) = fixture.run("maintain", &job);
    assert!(status.success());

    // The pin protects the oldest snapshot beyond what `last: 1` alone would
    // keep, so 2 remain: the newest and the pinned one.
    assert_eq!(fixture.snapshot_count(), 2);
}
