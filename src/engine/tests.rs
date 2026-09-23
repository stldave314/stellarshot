// SPDX-License-Identifier: GPL-3.0-only

//! Engine tests against real repositories in temporary folders.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tempfile::TempDir;

use super::*;

const PASSWORD: &str = "correct horse battery staple";

fn secret() -> Secret {
    Secret::new(PASSWORD)
}

/// Deterministic, incompressible-looking bytes.
fn pseudo_random(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect()
}

/// A tree with the names and file types that tend to break backup tools.
fn awkward_tree(root: &Path) {
    fs::create_dir_all(root.join("nested/deeper")).unwrap();
    fs::create_dir_all(root.join("empty dir")).unwrap();
    fs::write(root.join("plain.txt"), b"plain").unwrap();
    fs::write(root.join("with space.txt"), b"space").unwrap();
    fs::write(root.join("ünïcödé ✓.txt"), "unicode".as_bytes()).unwrap();
    fs::write(root.join("100% done.txt"), b"percent").unwrap();
    fs::write(
        root.join("nested/deeper/data.bin"),
        pseudo_random(64 * 1024, 1),
    )
    .unwrap();
    fs::write(root.join("private.txt"), b"secret").unwrap();
    fs::set_permissions(root.join("private.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    symlink("plain.txt", root.join("link-to-plain")).unwrap();
}

#[derive(Debug, PartialEq, Eq)]
enum Entry {
    File { content: Vec<u8>, mode: u32 },
    Dir,
    Link(PathBuf),
}

/// Every entry under `root`, keyed by relative path.
fn snapshot_of(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Entry>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let relative = path.strip_prefix(root).unwrap().to_path_buf();
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                out.insert(relative, Entry::Link(fs::read_link(&path).unwrap()));
            } else if meta.is_dir() {
                out.insert(relative, Entry::Dir);
                walk(root, &path, out);
            } else {
                out.insert(
                    relative,
                    Entry::File {
                        content: fs::read(&path).unwrap(),
                        mode: meta.permissions().mode() & 0o777,
                    },
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// Where a restored copy of `source` ends up under `destination`: snapshots
/// record absolute paths, so a restore recreates them.
fn restored(destination: &Path, source: &Path) -> PathBuf {
    destination.join(source.strip_prefix("/").unwrap())
}

struct Fixture {
    _dir: TempDir,
    repo: Location,
    source: PathBuf,
    work: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    let work = dir.path().join("work");
    fs::create_dir_all(&work).unwrap();
    let repo = Location::local(dir.path().join("repo"));
    init(&repo, &secret()).unwrap();
    Fixture {
        repo,
        source,
        work,
        _dir: dir,
    }
}

fn back_up(fixture: &Fixture, request: &BackupRequest) -> BackupReport {
    open(&fixture.repo, &secret())
        .unwrap()
        .backup(request, Arc::new(NoProgress))
        .unwrap()
}

fn sources(path: &Path) -> BackupRequest {
    BackupRequest {
        sources: vec![path.to_path_buf()],
        ..BackupRequest::default()
    }
}

#[test]
fn round_trip_preserves_tree() {
    let fixture = fixture();
    awkward_tree(&fixture.source);

    back_up(&fixture, &sources(&fixture.source));
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let original = snapshot_of(&fixture.source);
    assert!(original.len() >= 10, "the fixture itself must not be empty");
    assert_eq!(
        snapshot_of(&restored(&destination, &fixture.source)),
        original
    );
}

#[test]
fn excluded_folder_is_not_in_the_snapshot() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::create_dir_all(fixture.source.join("cache")).unwrap();
    fs::write(fixture.source.join("cache/big.tmp"), b"skip me").unwrap();

    let request = BackupRequest {
        excludes: vec![fixture.source.join("cache")],
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(
        restored.join("plain.txt").exists(),
        "the rest must be backed up"
    );
    assert!(
        !restored.join("cache").exists(),
        "the excluded folder must be left out"
    );
}

#[test]
fn exclude_pattern_applies_at_any_depth() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::create_dir_all(fixture.source.join("nested/deeper/node_modules/pkg")).unwrap();
    fs::write(
        fixture
            .source
            .join("nested/deeper/node_modules/pkg/index.js"),
        b"x",
    )
    .unwrap();
    fs::write(fixture.source.join("nested/scratch.tmp"), b"x").unwrap();

    let request = BackupRequest {
        exclude_patterns: vec!["node_modules".into(), "*.tmp".into()],
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("nested/deeper/data.bin").exists());
    assert!(!restored.join("nested/deeper/node_modules").exists());
    assert!(!restored.join("nested/scratch.tmp").exists());
}

#[test]
fn wrong_password_is_reported_as_such() {
    let fixture = fixture();

    let err = open(&fixture.repo, &Secret::new("wrong")).unwrap_err();

    assert_eq!(err.kind, ErrorKind::WrongPassword);
}

#[test]
fn open_non_repository_is_not_a_repository() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("notes.txt"), b"mine").unwrap();

    let err = open(&Location::local(dir.path()), &secret()).unwrap_err();

    assert_eq!(err.kind, ErrorKind::NotARepository);
}

#[test]
fn unreachable_location_is_reported_as_unavailable() {
    let err = open(
        &Location::local("/nonexistent-stellarshot-mount/drive/repo"),
        &secret(),
    )
    .unwrap_err();

    assert_eq!(err.kind, ErrorKind::DestinationUnavailable);
}

#[test]
fn init_refuses_existing_and_non_empty_locations() {
    let fixture = fixture();
    assert_eq!(
        init(&fixture.repo, &secret()).unwrap_err().kind,
        ErrorKind::AlreadyExists
    );

    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("notes.txt"), b"mine").unwrap();
    assert_eq!(
        init(&Location::local(dir.path()), &secret())
            .unwrap_err()
            .kind,
        ErrorKind::LocationNotEmpty
    );
    assert!(!dir.path().join("config").exists());
}

#[test]
fn second_backup_is_incremental() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let first = back_up(&fixture, &sources(&fixture.source));

    fs::write(fixture.source.join("plain.txt"), b"changed").unwrap();
    let second = back_up(&fixture, &sources(&fixture.source));

    assert!(first.snapshot.files_new > 0);
    assert!(
        first.snapshot.data_added > 64 * 1024,
        "the first backup stores everything"
    );
    assert_eq!(second.snapshot.files_changed, 1);
    assert!(second.snapshot.files_unmodified > 0);
    // Changed directory metadata is rewritten too, so a few kilobytes are
    // expected; the unchanged 64 KiB file must not be stored again.
    assert!(
        second.snapshot.data_added < 16 * 1024,
        "the unchanged data was stored again: {} bytes added",
        second.snapshot.data_added
    );
}

#[test]
fn snapshots_are_listed_newest_first_and_can_be_deleted() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    back_up(&fixture, &sources(&fixture.source));

    let repo = open(&fixture.repo, &secret()).unwrap();
    let listed = repo.snapshots().unwrap();
    assert_eq!(listed.len(), 2);
    assert!(listed[0].time >= listed[1].time);
    assert_eq!(listed[0].short_id().len(), 8);

    repo.delete_snapshots(&[listed[0].id.clone()]).unwrap();
    let remaining = repo.snapshots().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, listed[1].id);
}

#[test]
fn check_passes_on_a_sound_repository() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    open(&fixture.repo, &secret()).unwrap().check().unwrap();
}

#[test]
fn check_reports_a_damaged_repository() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    // Remove every index file: snapshots now refer to data the repository can
    // no longer find.
    for entry in fs::read_dir(fixture.repo.path().join("index")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }

    let result = open(&fixture.repo, &secret()).unwrap().check();

    assert_eq!(result.unwrap_err().kind, ErrorKind::RepositoryDamaged);
}

#[derive(Default)]
struct Recorder(Mutex<Vec<ProgressEvent>>);

impl ProgressSink for Recorder {
    fn update(&self, event: &ProgressEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

#[test]
fn backup_reports_progress_ending_at_the_total() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let recorder = Arc::new(Recorder::default());

    open(&fixture.repo, &secret())
        .unwrap()
        .backup(&sources(&fixture.source), recorder.clone())
        .unwrap();

    let events = recorder.0.lock().unwrap();
    let last = events
        .iter()
        .rev()
        .find(|event| event.phase == Phase::BackingUp)
        .expect("a backing-up phase must be reported");
    assert!(last.bytes);
    // Every byte of every regular file is reported. The scanned total also
    // counts symlinks, whose contents are not file data, so it can be a
    // little larger.
    let file_bytes: u64 = snapshot_of(&fixture.source)
        .values()
        .map(|entry| match entry {
            Entry::File { content, .. } => content.len() as u64,
            _ => 0,
        })
        .sum();
    assert_eq!(last.done, file_bytes);
    assert!(last.total.is_some_and(|total| total >= last.done));
}

#[test]
fn exclude_through_a_symlinked_path_still_applies() {
    // Where /home is a symlink (e.g. to /var/home), the backup walks canonical
    // paths; an exclude written through the symlink must still match.
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::create_dir_all(fixture.source.join("cache")).unwrap();
    fs::write(fixture.source.join("cache/big.tmp"), b"skip me").unwrap();
    let linked = fixture.work.join("home-link");
    symlink(&fixture.source, &linked).unwrap();

    let request = BackupRequest {
        sources: vec![linked.clone()],
        excludes: vec![linked.join("cache")],
        ..BackupRequest::default()
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fs::canonicalize(&fixture.source).unwrap());
    assert!(
        restored.join("plain.txt").exists(),
        "the canonical source is backed up"
    );
    assert!(
        !restored.join("cache").exists(),
        "the exclude must follow the symlink"
    );
}
