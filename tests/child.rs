// SPDX-License-Identifier: GPL-3.0-only

//! The window's side of the `--run` protocol: spawning the child, streaming
//! its events, and cancelling it.

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
        repository: location,
        password: Secret::new(PASSWORD),
        request: Some(BackupRequest {
            sources: vec![source],
            ..BackupRequest::default()
        }),
        snapshot: None,
        destination: None,
        ids: Vec::new(),
    };
    (dir, job)
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
            Some(ChildEvent::Event(Event::Done { report: Some(_) }))
        ),
        "last event: {:?}",
        events.last()
    );
}

#[test]
fn cancelling_a_backup_ends_it_as_cancelled_without_a_snapshot() {
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
        Some(ChildEvent::Ended(error)) => assert_eq!(error.kind, ErrorKind::Cancelled),
        other => panic!("expected the backup to end cancelled, got {other:?}"),
    }
    let snapshots = engine::open(&location, &Secret::new(PASSWORD))
        .unwrap()
        .snapshots()
        .unwrap();
    assert!(
        snapshots.is_empty(),
        "a cancelled backup must not leave a snapshot"
    );
}

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
