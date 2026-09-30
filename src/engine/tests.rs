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
    // Linux file names are bytes, not necessarily UTF-8 (see REL-6):
    // `\xe9` alone is not a valid UTF-8 sequence.
    fs::write(non_utf8_name(root), b"cafe, sort of").unwrap();
}

/// `root` joined with a name that is not valid UTF-8: `caf\xe9.txt`.
fn non_utf8_name(root: &Path) -> PathBuf {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    root.join(OsStr::from_bytes(b"caf\xe9.txt"))
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
fn excluded_folders_with_glob_metacharacters_in_their_name_are_not_backed_up() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::create_dir_all(fixture.source.join("a [b]")).unwrap();
    fs::write(fixture.source.join("a [b]/data"), b"skip me").unwrap();
    fs::create_dir_all(fixture.source.join("x*y")).unwrap();
    fs::write(fixture.source.join("x*y/data"), b"skip me too").unwrap();

    let request = BackupRequest {
        excludes: vec![fixture.source.join("a [b]"), fixture.source.join("x*y")],
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
        !restored.join("a [b]").exists(),
        "a name that looks like a character class must still be excluded literally"
    );
    assert!(
        !restored.join("x*y").exists(),
        "a name containing a glob wildcard must still be excluded literally"
    );
}

#[test]
fn a_non_utf8_exclude_fails_the_backup_rather_than_including_it() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fixture = fixture();
    awkward_tree(&fixture.source);
    let bad_name = OsStr::from_bytes(b"\xff");
    fs::create_dir_all(fixture.source.join(bad_name)).unwrap();

    let request = BackupRequest {
        excludes: vec![fixture.source.join(bad_name)],
        ..sources(&fixture.source)
    };
    let err = open(&fixture.repo, &secret())
        .unwrap()
        .backup(&request, Arc::new(NoProgress))
        .unwrap_err();

    assert_eq!(err.kind, ErrorKind::Io);
}

#[test]
fn a_repository_inside_a_source_with_a_bracket_in_its_path_is_still_excluded() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let repo_like = fixture.source.join("[repo]");
    fs::create_dir_all(&repo_like).unwrap();
    fs::write(repo_like.join("config"), b"pretend repository").unwrap();

    let request = BackupRequest {
        excludes: vec![repo_like.clone()],
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
        !restored.join("[repo]").exists(),
        "a repository-shaped exclude with a bracket in its path must not back up into itself"
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
fn a_folder_marked_as_a_cache_is_excluded() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::create_dir_all(fixture.source.join("build-cache")).unwrap();
    fs::write(
        fixture.source.join("build-cache/CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n",
    )
    .unwrap();
    fs::write(fixture.source.join("build-cache/object.o"), b"x").unwrap();

    let request = BackupRequest {
        exclude_caches: true,
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("plain.txt").exists());
    assert!(
        !restored.join("build-cache").exists(),
        "a folder tagged as a cache must be left out"
    );
}

#[test]
fn a_projects_own_gitignore_is_honored_without_needing_a_git_repository() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    fs::write(fixture.source.join(".gitignore"), "*.log\nbuild/\n").unwrap();
    fs::create_dir_all(fixture.source.join("build")).unwrap();
    fs::write(fixture.source.join("build/output.bin"), b"x").unwrap();
    fs::write(fixture.source.join("debug.log"), b"x").unwrap();

    let request = BackupRequest {
        git_ignore: true,
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("plain.txt").exists());
    assert!(!restored.join("build").exists());
    assert!(!restored.join("debug.log").exists());
    // The .gitignore's own rules apply to everything but itself.
    assert!(restored.join(".gitignore").exists());
}

#[test]
fn files_larger_than_the_limit_are_excluded() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("small.bin"), vec![0u8; 100]).unwrap();
    fs::write(fixture.source.join("big.bin"), vec![0u8; 10_000]).unwrap();

    let request = BackupRequest {
        exclude_larger_than: Some(1_000),
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("small.bin").exists());
    assert!(!restored.join("big.bin").exists());
}

#[test]
fn case_insensitive_patterns_match_either_case() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("Cache.TMP"), b"x").unwrap();
    fs::write(fixture.source.join("keep.txt"), b"x").unwrap();

    let request = BackupRequest {
        exclude_patterns_ignoring_case: vec!["*.tmp".into()],
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("keep.txt").exists());
    assert!(
        !restored.join("Cache.TMP").exists(),
        "a case-insensitive pattern must match regardless of case"
    );
}

#[test]
fn patterns_kept_in_a_file_are_applied() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("keep.txt"), b"x").unwrap();
    fs::write(fixture.source.join("scratch.tmp"), b"x").unwrap();
    let pattern_file = fixture.work.join("patterns.txt");
    fs::write(&pattern_file, "*.tmp\n").unwrap();

    let request = BackupRequest {
        exclude_pattern_files: vec![pattern_file],
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored = restored(&destination, &fixture.source);
    assert!(restored.join("keep.txt").exists());
    assert!(!restored.join("scratch.tmp").exists());
}

#[test]
fn a_missing_pattern_file_fails_the_backup_rather_than_including_everything() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("secret.txt"), b"x").unwrap();
    let request = BackupRequest {
        exclude_pattern_files: vec![fixture.work.join("does-not-exist.txt")],
        ..sources(&fixture.source)
    };

    let err = open(&fixture.repo, &secret())
        .unwrap()
        .backup(&request, Arc::new(NoProgress))
        .unwrap_err();

    assert_eq!(err.kind, ErrorKind::Io);
    assert_eq!(
        open(&fixture.repo, &secret())
            .unwrap()
            .snapshots()
            .unwrap()
            .len(),
        0,
        "nothing must have been backed up with the exclusion silently missing"
    );
}

#[test]
fn an_unchanged_backup_is_skipped_when_asked() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let request = BackupRequest {
        skip_if_unchanged: true,
        ..sources(&fixture.source)
    };
    back_up(&fixture, &request);
    back_up(&fixture, &request);

    let snapshots = open(&fixture.repo, &secret()).unwrap().snapshots().unwrap();
    assert_eq!(
        snapshots.len(),
        1,
        "an unchanged backup must not add a second snapshot"
    );

    fs::write(fixture.source.join("plain.txt"), b"changed").unwrap();
    back_up(&fixture, &request);
    let snapshots = open(&fixture.repo, &secret()).unwrap().snapshots().unwrap();
    assert_eq!(snapshots.len(), 2, "a real change must still be recorded");
}

#[test]
fn a_dry_run_reports_size_without_writing_anything() {
    let fixture = fixture();
    awkward_tree(&fixture.source);

    let request = BackupRequest {
        dry_run: true,
        ..sources(&fixture.source)
    };
    let report = back_up(&fixture, &request);
    assert!(
        report.snapshot.data_added > 0,
        "a dry run must still report the size it would add"
    );

    let snapshots = open(&fixture.repo, &secret()).unwrap().snapshots().unwrap();
    assert!(snapshots.is_empty(), "a dry run must not write a snapshot");

    // The same backup for real must add just as much data as the dry run
    // estimated, since nothing was actually written in between.
    let real = back_up(&fixture, &sources(&fixture.source));
    assert_eq!(real.snapshot.data_added, report.snapshot.data_added);
}

#[test]
fn extended_attributes_are_saved_and_restored() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    let file = fixture.source.join("tagged.txt");
    fs::write(&file, b"x").unwrap();
    xattr::set(&file, "user.stellarshot.test", b"hello").unwrap();

    back_up(&fixture, &sources(&fixture.source));
    let destination = fixture.work.join("restore");
    open(&fixture.repo, &secret())
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();

    let restored_file = restored(&destination, &fixture.source).join("tagged.txt");
    let value = xattr::get(&restored_file, "user.stellarshot.test").unwrap();
    assert_eq!(value, Some(b"hello".to_vec()));
}

#[test]
fn a_new_repository_verifies_data_after_compression_by_default() {
    // rustic's own `extra_verify` already defaults to on; this only proves
    // Stellarshot's `init` does not accidentally turn it off, not that a
    // real corruption is caught (rustic exposes no public hook to induce
    // one from outside).
    let fixture = fixture();
    let repo = open(&fixture.repo, &secret()).unwrap();
    assert!(repo.inner.config().extra_verify());
}

#[test]
fn a_chosen_compression_level_is_stored_in_the_repository() {
    let dir = TempDir::new().unwrap();
    let location = Location::local(dir.path().join("repo"));

    let repo = init_with(&location, &secret(), false, Some(19)).unwrap();
    assert_eq!(repo.compression_level(), Some(19));

    let reopened = open(&location, &secret()).unwrap();
    assert_eq!(
        reopened.compression_level(),
        Some(19),
        "read back from the repository itself, not just the handle that created it"
    );
}

#[test]
fn no_chosen_compression_leaves_rustics_own_default_in_place() {
    let dir = TempDir::new().unwrap();
    let location = Location::local(dir.path().join("repo"));

    let repo = init_with(&location, &secret(), false, None).unwrap();

    assert_eq!(
        repo.compression_level(),
        None,
        "nothing was overridden, so nothing should be pinned to today's default"
    );
}

#[test]
fn an_append_only_repository_refuses_to_forget_a_snapshot() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("file.txt"), b"content").unwrap();
    let location = Location::local(dir.path().join("repo"));
    init_with(&location, &secret(), true, None).unwrap();

    assert!(open(&location, &secret()).unwrap().is_append_only());
    let report = open(&location, &secret())
        .unwrap()
        .backup(&sources(&source), Arc::new(NoProgress))
        .unwrap();

    let err = open(&location, &secret())
        .unwrap()
        .delete_snapshots(&[report.snapshot.id])
        .unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::Internal,
        "rustic itself must refuse the deletion, not Stellarshot silently skipping it: {err:?}"
    );
    assert_eq!(
        open(&location, &secret())
            .unwrap()
            .snapshots()
            .unwrap()
            .len(),
        1,
        "the snapshot must still be there after the refused deletion"
    );
}

#[test]
fn an_append_only_repository_refuses_to_prune() {
    // Unlike `delete_snapshots`/`forget` above, `Repo::prune` never checks
    // `is_append_only()` itself — it relies entirely on rustic_core
    // refusing `prune_plan`/`prune` on its own. This proves that reliance
    // is justified, rather than assuming it because the *other* operation
    // does refuse.
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("source");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("file.txt"), b"content").unwrap();
    let location = Location::local(dir.path().join("repo"));
    init_with(&location, &secret(), true, None).unwrap();
    open(&location, &secret())
        .unwrap()
        .backup(&sources(&source), Arc::new(NoProgress))
        .unwrap();

    let err = open(&location, &secret()).unwrap().prune().unwrap_err();
    assert_eq!(
        err.kind,
        ErrorKind::Internal,
        "rustic itself must refuse to prune an append-only repository: {err:?}"
    );
}

#[test]
fn a_second_key_opens_the_same_repository_as_the_first() {
    let fixture = fixture();
    let repo = open(&fixture.repo, &secret()).unwrap();
    repo.add_key("a second password").unwrap();

    // Both passwords must now open it, and see the same key listed twice.
    open(&fixture.repo, &secret()).unwrap();
    let second = open(&fixture.repo, &Secret::new("a second password")).unwrap();

    let keys = second.keys().unwrap();
    assert_eq!(keys.len(), 2, "both keys must be listed: {keys:?}");
}

#[test]
fn changing_the_password_replaces_the_key_it_was_opened_with() {
    let fixture = fixture();
    let repo = open(&fixture.repo, &secret()).unwrap();
    repo.change_password("a new password").unwrap();

    assert_eq!(
        open(&fixture.repo, &Secret::new("a new password"))
            .unwrap()
            .keys()
            .unwrap()
            .len(),
        1,
        "the old key must be gone, not just a new one added"
    );
    let err = open(&fixture.repo, &secret()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::WrongPassword);
}

#[test]
fn a_key_that_is_not_the_current_one_can_be_deleted() {
    let fixture = fixture();
    let repo = open(&fixture.repo, &secret()).unwrap();
    let added = repo.add_key("a second password").unwrap();

    repo.delete_key(&added).unwrap();

    assert_eq!(repo.keys().unwrap().len(), 1);
    let err = open(&fixture.repo, &Secret::new("a second password")).unwrap_err();
    assert_eq!(err.kind, ErrorKind::WrongPassword);
}

#[test]
fn the_key_a_repository_was_opened_with_cannot_be_deleted() {
    let fixture = fixture();
    let repo = open(&fixture.repo, &secret()).unwrap();
    let current = repo.keys().unwrap();
    let current_id = current
        .into_iter()
        .find(|key| key.current)
        .expect("the key just used to open it must be marked current")
        .id;

    let err = repo.delete_key(&current_id).unwrap_err();

    assert_eq!(err.kind, ErrorKind::Internal);
    // Still there and still works, not silently removed anyway.
    open(&fixture.repo, &secret()).unwrap();
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
    for entry in fs::read_dir(fixture.repo.local_path().unwrap().join("index")).unwrap() {
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

// ---------------------------------------------------------------------------
// Browsing
// ---------------------------------------------------------------------------

fn browser(fixture: &Fixture) -> Browser {
    open(&fixture.repo, &secret()).unwrap().browse().unwrap()
}

#[test]
fn lists_a_folder_in_a_snapshot() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let entries = browser(&fixture).list("latest", &fixture.source).unwrap();

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"nested") && names.contains(&"plain.txt"));
    let first_file = entries
        .iter()
        .position(|e| e.kind != EntryKind::Directory)
        .unwrap();
    assert!(
        entries[..first_file]
            .iter()
            .all(|e| e.kind == EntryKind::Directory),
        "folders come first"
    );
    let plain = entries.iter().find(|e| e.name == "plain.txt").unwrap();
    assert_eq!(plain.path, fixture.source.join("plain.txt"));
    assert_eq!(plain.size, 5);
}

#[test]
fn search_finds_names_anywhere() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let found = browser(&fixture).search("latest", "DATA", 10).unwrap();

    assert_eq!(found.len(), 1, "case is ignored");
    assert_eq!(found[0].path, fixture.source.join("nested/deeper/data.bin"));
}

#[test]
fn search_all_finds_every_snapshot_a_name_appears_in() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("keepme.txt"), b"first").unwrap();
    back_up(&fixture, &sources(&fixture.source));
    fs::write(fixture.source.join("keepme.txt"), b"second").unwrap();
    back_up(&fixture, &sources(&fixture.source));
    fs::remove_file(fixture.source.join("keepme.txt")).unwrap();
    fs::write(fixture.source.join("unrelated.txt"), b"noise").unwrap();
    back_up(&fixture, &sources(&fixture.source));

    let found = browser(&fixture).search_all("keepme", 10).unwrap();

    assert_eq!(
        found.len(),
        1,
        "one distinct path, not one row per snapshot"
    );
    assert_eq!(found[0].path, fixture.source.join("keepme.txt"));
    assert_eq!(
        found[0].snapshots.len(),
        2,
        "found only in the two snapshots that actually had it"
    );
}

#[test]
fn search_all_matches_every_distinct_path() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("report-jan.txt"), b"a").unwrap();
    fs::write(fixture.source.join("report-feb.txt"), b"b").unwrap();
    fs::write(fixture.source.join("other.txt"), b"c").unwrap();
    back_up(&fixture, &sources(&fixture.source));

    let found = browser(&fixture).search_all("report", 10).unwrap();

    assert_eq!(found.len(), 2);
    assert!(found.iter().any(|m| m.path.ends_with("report-jan.txt")));
    assert!(found.iter().any(|m| m.path.ends_with("report-feb.txt")));
}

#[test]
fn search_all_ignores_case_and_an_empty_query_finds_nothing() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    fs::write(fixture.source.join("Report.txt"), b"a").unwrap();
    back_up(&fixture, &sources(&fixture.source));

    assert_eq!(browser(&fixture).search_all("REPORT", 10).unwrap().len(), 1);
    assert_eq!(browser(&fixture).search_all("", 10).unwrap().len(), 0);
}

#[test]
fn versions_collapse_identical_content() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    fs::write(fixture.source.join("plain.txt"), b"second").unwrap();
    back_up(&fixture, &sources(&fixture.source));
    back_up(&fixture, &sources(&fixture.source));

    let versions = browser(&fixture)
        .versions(&fixture.source.join("plain.txt"))
        .unwrap();

    assert_eq!(versions.len(), 3);
    assert!(!versions[0].same_as_newer, "the newest is always shown");
    assert!(
        versions[1].same_as_newer,
        "unchanged between the last two backups"
    );
    assert!(
        !versions[2].same_as_newer,
        "the first backup had other content"
    );
    assert_eq!(versions[2].size, 5);
}

#[test]
fn diff_reports_added_removed_modified() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let before = back_up(&fixture, &sources(&fixture.source)).snapshot.id;
    fs::write(fixture.source.join("plain.txt"), b"changed").unwrap();
    fs::remove_file(fixture.source.join("with space.txt")).unwrap();
    fs::write(fixture.source.join("new.txt"), b"new").unwrap();
    let after = back_up(&fixture, &sources(&fixture.source)).snapshot.id;

    let diff = browser(&fixture).diff(&before, &after).unwrap();

    let change = |name: &str| {
        diff.iter()
            .find(|d| d.path == fixture.source.join(name))
            .map(|d| d.change)
    };
    assert_eq!(change("plain.txt"), Some(Change::Modified));
    assert_eq!(change("with space.txt"), Some(Change::Removed));
    assert_eq!(change("new.txt"), Some(Change::Added));
    assert_eq!(
        change("100% done.txt"),
        None,
        "unchanged files are not listed"
    );
}

#[test]
fn diff_does_not_drop_names_that_collide_once_lossily_converted() {
    // REL-6: `bad\xfe.txt` and `bad\xff.txt` both become `bad<REPLACEMENT
    // CHARACTER>.txt` under `to_string_lossy`. A map keyed by that lossy
    // string would keep only one of them. Distinct bytes from
    // `awkward_tree`'s own non-UTF-8 file, so this does not also exercise
    // (and get confused by) that one being modified.
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let fixture = fixture();
    awkward_tree(&fixture.source);
    let before = back_up(&fixture, &sources(&fixture.source)).snapshot.id;
    fs::write(fixture.source.join(OsStr::from_bytes(b"bad\xfe.txt")), b"a").unwrap();
    fs::write(fixture.source.join(OsStr::from_bytes(b"bad\xff.txt")), b"b").unwrap();
    let after = back_up(&fixture, &sources(&fixture.source)).snapshot.id;

    let diff = browser(&fixture).diff(&before, &after).unwrap();

    assert_eq!(
        diff.len(),
        2,
        "both differently-invalid names must be reported, not just one: {diff:?}"
    );
    let added: std::collections::BTreeSet<&Path> = diff
        .iter()
        .filter(|d| d.change == Change::Added)
        .map(|d| d.path.as_path())
        .collect();
    assert!(
        added.contains(
            fixture
                .source
                .join(OsStr::from_bytes(b"bad\xfe.txt"))
                .as_path()
        )
    );
    assert!(
        added.contains(
            fixture
                .source
                .join(OsStr::from_bytes(b"bad\xff.txt"))
                .as_path()
        )
    );
}

#[test]
fn a_snapshot_compared_with_itself_has_no_changes() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let id = back_up(&fixture, &sources(&fixture.source)).snapshot.id;

    assert!(browser(&fixture).diff(&id, &id).unwrap().is_empty());
}

#[test]
fn missing_finds_deleted_files_with_their_last_snapshot() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let latest = back_up(&fixture, &sources(&fixture.source)).snapshot.id;
    fs::remove_file(fixture.source.join("with space.txt")).unwrap();

    let missing = browser(&fixture).missing(&fixture.source, 0, 100).unwrap();

    assert_eq!(missing.len(), 1, "only the deleted file: {missing:?}");
    assert_eq!(missing[0].path, fixture.source.join("with space.txt"));
    assert_eq!(
        missing[0].last_seen.id, latest,
        "restored from the newest snapshot"
    );
}

#[test]
fn dump_file_writes_exactly_that_files_bytes() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let destination = fixture.work.join("plain.txt");
    browser(&fixture)
        .dump_file("latest", &fixture.source.join("plain.txt"), &destination)
        .unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"plain");
}

#[test]
fn dump_file_refuses_a_folder() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let destination = fixture.work.join("nested.txt");
    let err = browser(&fixture)
        .dump_file("latest", &fixture.source.join("nested"), &destination)
        .unwrap_err();

    // Not `Internal`: a wrong-type path is reported the same way a path
    // that plain doesn't exist is, so the web API (WEB-6) can answer 404
    // for both rather than a bare 500.
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(!destination.exists(), "nothing is written on failure");
}

#[test]
fn archive_folder_produces_a_tar_gz_with_the_same_tree() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let destination = fixture.work.join("nested.tar.gz");
    browser(&fixture)
        .archive_folder("latest", &fixture.source.join("nested"), &destination)
        .unwrap();

    let extracted = fixture.work.join("extracted");
    fs::create_dir_all(&extracted).unwrap();
    let gzip = flate2::read::GzDecoder::new(fs::File::open(&destination).unwrap());
    tar::Archive::new(gzip).unpack(&extracted).unwrap();

    assert_eq!(
        snapshot_of(&extracted),
        snapshot_of(&fixture.source.join("nested")),
    );
}

#[test]
fn archive_folder_refuses_a_file() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let destination = fixture.work.join("plain.tar.gz");
    let err = browser(&fixture)
        .archive_folder("latest", &fixture.source.join("plain.txt"), &destination)
        .unwrap_err();

    // See dump_file_refuses_a_folder's own comment on NotFound vs Internal.
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(!destination.exists(), "nothing is written on failure");
}

/// Overwrites every file directly under `dir` with garbage: used to make a
/// repository's own pack data unreadable without touching its index or
/// config, the same failure mode a damaged remote or a bit-rotted disk
/// would cause partway through reading a blob that opened and indexed
/// just fine.
fn corrupt_every_file(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if fs::metadata(&path).unwrap().is_dir() {
            corrupt_every_file(&path);
        } else {
            fs::write(&path, b"corrupted").unwrap();
        }
    }
}

/// TST-5: `write_atomically`'s all-or-nothing guarantee must hold even for
/// a failure that happens well after writing has started, not only for an
/// upfront rejection like `archive_folder_refuses_a_file` above.
#[test]
fn archive_folder_leaves_nothing_behind_when_a_blob_read_fails_partway_through() {
    let fixture = fixture();
    fs::create_dir_all(fixture.source.join("nested")).unwrap();
    fs::write(fixture.source.join("nested/a.txt"), b"first file").unwrap();
    fs::write(fixture.source.join("nested/b.txt"), b"second file").unwrap();
    back_up(&fixture, &sources(&fixture.source));
    corrupt_every_file(&fixture.repo.local_path().unwrap().join("data"));

    let destination = fixture.work.join("nested.tar.gz");
    let err = browser(&fixture)
        .archive_folder("latest", &fixture.source.join("nested"), &destination)
        .unwrap_err();

    // Whether the corruption is caught while opening the blob (Internal, a
    // RusticError) or while streaming it through BlobReader (Io, a plain
    // io::Error `?`-converted with no richer classification) depends on
    // which internal rustic_core path handles it. Asserting only Internal
    // passed every time run alone, but failed with Io instead under the
    // full suite's parallel load (reproduced locally and in CI). Either is
    // a genuine read failure; the two assertions below are what this test
    // actually exists to prove.
    assert!(
        matches!(err.kind, ErrorKind::Internal | ErrorKind::Io),
        "expected a blob read failure, got {err:?}"
    );
    assert!(
        !destination.exists(),
        "nothing is written when a blob fails to read partway through archiving"
    );
    assert_eq!(
        fs::read_dir(&fixture.work).unwrap().count(),
        0,
        "no stray temp file is left behind in the destination's own folder either"
    );
}

// ---------------------------------------------------------------------------
// Mounting a snapshot through FUSE
// ---------------------------------------------------------------------------

/// `read_dir` on `path`, retrying briefly: the mount point is registered
/// with the kernel before `mount::mount` returns, but the background
/// thread that actually answers FUSE requests may not have serviced its
/// first one yet.
fn read_mounted_dir(path: &Path) -> Vec<fs::DirEntry> {
    let mut last = None;
    for _ in 0..50 {
        match fs::read_dir(path) {
            Ok(entries) => return entries.collect::<Result<Vec<_>, _>>().unwrap(),
            Err(err) => {
                last = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
        }
    }
    panic!("{} never became readable: {last:?}", path.display());
}

#[test]
fn a_mounted_snapshot_can_be_read_with_plain_filesystem_calls() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let mount_point = fixture.work.join("mnt");
    fs::create_dir_all(&mount_point).unwrap();
    let browser = Arc::new(browser(&fixture));
    let _mount = mount::mount(browser, "latest".to_owned(), &mount_point).unwrap();

    let root = restored(&mount_point, &fixture.source);
    let names: Vec<String> = read_mounted_dir(&root)
        .iter()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.contains(&"plain.txt".to_owned()));
    assert!(names.contains(&"nested".to_owned()));

    assert_eq!(fs::read(root.join("plain.txt")).unwrap(), b"plain");
    assert_eq!(
        fs::symlink_metadata(root.join("private.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600,
        "the file's own recorded mode, not a default"
    );
    assert_eq!(
        fs::read_link(root.join("link-to-plain")).unwrap(),
        PathBuf::from("plain.txt")
    );

    let data = fs::read(root.join("nested/deeper/data.bin")).unwrap();
    assert_eq!(data, pseudo_random(64 * 1024, 1));

    // REL-6: a non-UTF-8 name must be listed by its exact bytes (not a
    // lossy, mangled one `lookup` could never match back) and readable.
    let non_utf8 = non_utf8_name(&root);
    let mounted_names: Vec<_> = read_mounted_dir(&root)
        .iter()
        .map(std::fs::DirEntry::file_name)
        .collect();
    assert!(
        mounted_names.contains(&non_utf8.file_name().unwrap().to_owned()),
        "the non-UTF-8 name must appear in the listing by its exact bytes"
    );
    assert_eq!(fs::read(&non_utf8).unwrap(), b"cafe, sort of");
}

#[test]
fn a_mounted_file_cannot_be_written_to() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));

    let mount_point = fixture.work.join("mnt");
    fs::create_dir_all(&mount_point).unwrap();
    let browser = Arc::new(browser(&fixture));
    let _mount = mount::mount(browser, "latest".to_owned(), &mount_point).unwrap();

    let root = restored(&mount_point, &fixture.source);
    read_mounted_dir(&root);
    assert!(fs::write(root.join("plain.txt"), b"tampered").is_err());
}

// ---------------------------------------------------------------------------
// Selective restore
// ---------------------------------------------------------------------------

fn restore_request(paths: Vec<PathBuf>, target: Target, policy: ConflictPolicy) -> RestoreRequest {
    RestoreRequest {
        snapshot: "latest".to_owned(),
        paths,
        target,
        policy,
        ..RestoreRequest::default()
    }
}

fn run_restore(fixture: &Fixture, request: &RestoreRequest) -> RestorePreview {
    open(&fixture.repo, &secret())
        .unwrap()
        .restore(request, Arc::new(NoProgress))
        .unwrap()
}

fn preview(fixture: &Fixture, request: &RestoreRequest) -> RestorePreview {
    open(&fixture.repo, &secret())
        .unwrap()
        .preview_restore(request)
        .unwrap()
}

/// The copy Keep both makes next to `path`.
fn kept_copy(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap();
    let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
    fs::read_dir(parent)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|candidate| {
            let name = candidate
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            name.starts_with(&format!("{stem} (restored "))
        })
        .unwrap_or_else(|| panic!("no kept copy next to {}", path.display()))
}

#[test]
fn verify_existing_catches_content_that_looks_unchanged() {
    let fixture = fixture();
    fs::create_dir_all(&fixture.source).unwrap();
    let file = fixture.source.join("data.bin");
    fs::write(&file, b"original content").unwrap();
    back_up(&fixture, &sources(&fixture.source));

    let destination = fixture.work.join("restore");
    let target_file = restored(&destination, &fixture.source).join("data.bin");
    fs::create_dir_all(target_file.parent().unwrap()).unwrap();
    // Same length as "original content", so a size-and-date check alone
    // cannot tell the two apart.
    fs::write(&target_file, b"corrupted-------").unwrap();
    let original_mtime = fs::metadata(&file).unwrap().modified().unwrap();
    filetime::set_file_mtime(
        &target_file,
        filetime::FileTime::from_system_time(original_mtime),
    )
    .unwrap();

    let request = restore_request(
        vec![PathBuf::from("/")],
        Target::Folder(destination.clone()),
        ConflictPolicy::Overwrite,
    );
    run_restore(&fixture, &request);
    assert_eq!(
        fs::read(&target_file).unwrap(),
        b"corrupted-------",
        "trusting size and date must leave the file untouched"
    );

    let request = RestoreRequest {
        verify_existing: true,
        ..request
    };
    run_restore(&fixture, &request);
    assert_eq!(
        fs::read(&target_file).unwrap(),
        b"original content",
        "verifying by content must catch the mismatch and rewrite it"
    );
}

#[test]
fn restoring_with_numeric_or_no_ownership_does_not_fail() {
    // Actually changing an owner needs root, which tests must not assume;
    // this only proves the option is wired through without breaking a
    // restore for everyone else. See VALIDATION.md for how ownership itself
    // was checked.
    for ownership in [Ownership::Numeric, Ownership::None, Ownership::Preserve] {
        let fixture = fixture();
        awkward_tree(&fixture.source);
        back_up(&fixture, &sources(&fixture.source));
        let destination = fixture.work.join(format!("restore-{ownership:?}"));
        let request = RestoreRequest {
            ownership,
            ..restore_request(
                vec![PathBuf::from("/")],
                Target::Folder(destination.clone()),
                ConflictPolicy::Overwrite,
            )
        };
        run_restore(&fixture, &request);
        assert!(
            restored(&destination, &fixture.source)
                .join("plain.txt")
                .exists()
        );
    }
}

#[test]
fn restore_to_a_folder_keeps_names() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let target = fixture.work.join("elsewhere");

    run_restore(
        &fixture,
        &restore_request(
            vec![
                fixture.source.join("nested"),
                fixture.source.join("plain.txt"),
            ],
            Target::Folder(target.clone()),
            ConflictPolicy::Overwrite,
        ),
    );

    assert_eq!(
        fs::read(target.join("nested/deeper/data.bin")).unwrap(),
        pseudo_random(64 * 1024, 1)
    );
    assert_eq!(fs::read(target.join("plain.txt")).unwrap(), b"plain");
}

#[test]
fn a_non_utf8_named_file_can_be_restored_by_itself() {
    // REL-6: restoring just this one file needs `node_from_snapshot_and_path`
    // (or its replacement) to resolve its exact path, not a lossy
    // approximation of it.
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let non_utf8 = non_utf8_name(&fixture.source);
    let target = fixture.work.join("elsewhere");

    run_restore(
        &fixture,
        &restore_request(
            vec![non_utf8.clone()],
            Target::Folder(target.clone()),
            ConflictPolicy::Overwrite,
        ),
    );

    assert_eq!(
        fs::read(target.join(non_utf8.file_name().unwrap())).unwrap(),
        b"cafe, sort of"
    );
}

#[test]
fn keep_both_preserves_a_non_utf8_stem() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let non_utf8 = non_utf8_name(&fixture.source);
    fs::write(&non_utf8, b"edited since").unwrap();

    run_restore(
        &fixture,
        &restore_request(
            vec![non_utf8.clone()],
            Target::Original,
            ConflictPolicy::KeepBoth,
        ),
    );

    assert_eq!(
        fs::read(&non_utf8).unwrap(),
        b"edited since",
        "the existing file must not be touched"
    );
    let copy = kept_copy(&non_utf8);
    assert_eq!(fs::read(&copy).unwrap(), b"cafe, sort of");
    assert!(
        copy.file_name()
            .unwrap()
            .as_encoded_bytes()
            .starts_with(b"caf\xe9"),
        "the restored copy's name must keep the original's exact bytes: {copy:?}"
    );
}

#[test]
fn keep_both_never_touches_the_existing_file() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let plain = fixture.source.join("plain.txt");
    fs::write(&plain, b"edited since").unwrap();

    let done = run_restore(
        &fixture,
        &restore_request(
            vec![fixture.source.clone()],
            Target::Original,
            ConflictPolicy::KeepBoth,
        ),
    );

    assert_eq!(
        fs::read(&plain).unwrap(),
        b"edited since",
        "the user's file is untouched"
    );
    assert_eq!(
        fs::read(kept_copy(&plain)).unwrap(),
        b"plain",
        "the backed-up copy is beside it"
    );
    assert_eq!(done.conflicts, 1);
}

/// REL-14: `looks_identical` used to compare modification times to the
/// second, but rustic's own `add_file` compares at full precision
/// (confirmed by reading `rustic_core::commands::restore`'s own source, not
/// assumed: `meta.len() == file.meta.size && mtime == file.meta.mtime`,
/// where `mtime` is a full `jiff::Timestamp`). A file whose mtime landed in
/// the same second as the snapshot's, but at a different nanosecond, used
/// to look identical here — so **Keep Both** never renamed the existing
/// file aside — while rustic, underneath, still saw two different files
/// and restored over it anyway.
#[test]
fn keep_both_does_not_overwrite_a_file_within_the_same_second() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let plain = fixture.source.join("plain.txt");
    // A clean second boundary, not whatever the filesystem's own clock
    // happens to be right now, so perturbing it by 500ms below is
    // guaranteed to stay within the same whole second rather than risk
    // rolling over into the next one.
    let clean_second =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    filetime::set_file_mtime(&plain, filetime::FileTime::from_system_time(clean_second)).unwrap();

    back_up(&fixture, &sources(&fixture.source));

    // Same size as the backed-up content ("plain", 5 bytes), so the size
    // check alone cannot tell the two apart — only the nanosecond both
    // rustic and the fixed `looks_identical` now compare at can.
    fs::write(&plain, b"xlain").unwrap();
    let same_second_different_nanosecond = clean_second + std::time::Duration::from_millis(500);
    filetime::set_file_mtime(
        &plain,
        filetime::FileTime::from_system_time(same_second_different_nanosecond),
    )
    .unwrap();

    run_restore(
        &fixture,
        &restore_request(
            vec![plain.clone()],
            Target::Original,
            ConflictPolicy::KeepBoth,
        ),
    );

    assert_eq!(
        fs::read(&plain).unwrap(),
        b"xlain",
        "the existing file must not be touched, even though its mtime \
         matches the snapshot's to the second"
    );
    assert_eq!(fs::read(kept_copy(&plain)).unwrap(), b"plain");
}

#[test]
fn keep_both_for_a_single_file() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let plain = fixture.source.join("plain.txt");
    fs::write(&plain, b"edited since").unwrap();

    run_restore(
        &fixture,
        &restore_request(
            vec![plain.clone()],
            Target::Original,
            ConflictPolicy::KeepBoth,
        ),
    );

    assert_eq!(fs::read(&plain).unwrap(), b"edited since");
    assert_eq!(fs::read(kept_copy(&plain)).unwrap(), b"plain");
}

#[test]
fn skip_restores_only_what_is_missing() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    fs::write(fixture.source.join("plain.txt"), b"edited since").unwrap();
    fs::remove_file(fixture.source.join("with space.txt")).unwrap();

    let done = run_restore(
        &fixture,
        &restore_request(
            vec![fixture.source.clone()],
            Target::Original,
            ConflictPolicy::Skip,
        ),
    );

    assert_eq!(
        fs::read(fixture.source.join("with space.txt")).unwrap(),
        b"space"
    );
    assert_eq!(
        fs::read(fixture.source.join("plain.txt")).unwrap(),
        b"edited since"
    );
    assert_eq!(done.conflicts, 1, "the edited file was skipped");
}

/// Builds a fixture where the snapshot has a directory ("nested", holding
/// one file) at a path that, at the destination, is already occupied by a
/// plain file — the file-vs-directory type conflict TST-5 in the review
/// plan asked to cover. Returns the fixture and the destination folder;
/// `target.join("nested")` is the blocking file.
fn fixture_with_a_directory_blocked_by_an_existing_file() -> (Fixture, PathBuf) {
    let fixture = fixture();
    fs::create_dir_all(fixture.source.join("nested")).unwrap();
    fs::write(fixture.source.join("nested/inner.txt"), b"content").unwrap();
    back_up(&fixture, &sources(&fixture.source));
    let target = fixture.work.join("elsewhere");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("nested"), b"in the way").unwrap();
    (fixture, target)
}

/// Before this fix, a snapshot directory whose own path was blocked by an
/// existing file (or, as here, a file whose *parent* path was blocked by
/// one) was never recognized as a conflict at all: `on_disk.symlink_metadata()`
/// failing was always read as "does not exist yet, safe to create," which
/// does not distinguish that from "cannot exist because something is in
/// the way higher up." The restore silently reported zero conflicts and
/// handed rustic an unrestorable path.
#[test]
fn a_file_vs_directory_conflict_is_reported_not_silently_ignored() {
    let (fixture, target) = fixture_with_a_directory_blocked_by_an_existing_file();

    let preview = open(&fixture.repo, &secret())
        .unwrap()
        .restore(
            &restore_request(
                vec![fixture.source.join("nested")],
                Target::Folder(target),
                ConflictPolicy::Overwrite,
            ),
            Arc::new(NoProgress),
        )
        .unwrap();

    assert_eq!(preview.conflicts, 1);
}

#[test]
fn skip_leaves_a_file_blocking_a_directory_completely_untouched() {
    let (fixture, target) = fixture_with_a_directory_blocked_by_an_existing_file();

    open(&fixture.repo, &secret())
        .unwrap()
        .restore(
            &restore_request(
                vec![fixture.source.join("nested")],
                Target::Folder(target.clone()),
                ConflictPolicy::Skip,
            ),
            Arc::new(NoProgress),
        )
        .unwrap();

    assert_eq!(fs::read(target.join("nested")).unwrap(), b"in the way");
}

/// Keep Both cannot rename a conflicting directory aside the way it does a
/// file: every item is shaped independently against the same, fixed
/// destination, with no way to carry a rename down to a directory's own
/// descendants. Rather than attempt a rename that cannot actually work,
/// this case is treated the same as Skip.
#[test]
fn keep_both_also_leaves_a_file_blocking_a_directory_untouched() {
    let (fixture, target) = fixture_with_a_directory_blocked_by_an_existing_file();

    open(&fixture.repo, &secret())
        .unwrap()
        .restore(
            &restore_request(
                vec![fixture.source.join("nested")],
                Target::Folder(target.clone()),
                ConflictPolicy::KeepBoth,
            ),
            Arc::new(NoProgress),
        )
        .unwrap();

    assert_eq!(fs::read(target.join("nested")).unwrap(), b"in the way");
}

/// A symlink conflicting in type with something already at its own path —
/// unlike the directory case above, already handled correctly by
/// `looks_identical` (a symlink is compared like a file: `meta.is_file()`
/// or `meta.file_type().is_symlink()` fails to match, so it is never
/// mistaken for "identical, nothing to do"), just never proven by a test
/// until now (TST-5 in the review plan).
#[test]
fn a_symlink_vs_existing_directory_conflict_is_reported() {
    let fixture = fixture();
    fs::create_dir_all(fixture.source.join("nested")).unwrap();
    fs::write(fixture.source.join("nested/target.txt"), b"target").unwrap();
    symlink("target.txt", fixture.source.join("nested/link")).unwrap();
    back_up(&fixture, &sources(&fixture.source));
    let target = fixture.work.join("elsewhere");
    fs::create_dir_all(target.join("nested/link")).unwrap();

    let preview = open(&fixture.repo, &secret())
        .unwrap()
        .restore(
            &restore_request(
                vec![fixture.source.join("nested")],
                Target::Folder(target.clone()),
                ConflictPolicy::Skip,
            ),
            Arc::new(NoProgress),
        )
        .unwrap();

    assert_eq!(
        preview.conflicts, 1,
        "the symlink's own name is blocked by an existing directory"
    );
    assert!(
        target.join("nested/link").is_dir(),
        "Skip must leave the existing directory alone"
    );
}

#[test]
fn overwrite_replaces_changed_files() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    fs::write(fixture.source.join("plain.txt"), b"edited since").unwrap();

    run_restore(
        &fixture,
        &restore_request(
            vec![fixture.source.clone()],
            Target::Original,
            ConflictPolicy::Overwrite,
        ),
    );

    assert_eq!(
        fs::read(fixture.source.join("plain.txt")).unwrap(),
        b"plain"
    );
}

#[test]
fn preview_counts_match_the_restore() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    fs::write(fixture.source.join("plain.txt"), b"edited since").unwrap();
    fs::remove_file(fixture.source.join("with space.txt")).unwrap();
    let request = restore_request(
        vec![fixture.source.clone()],
        Target::Original,
        ConflictPolicy::Overwrite,
    );

    let before = preview(&fixture, &request);
    // The preview writes nothing.
    assert!(!fixture.source.join("with space.txt").exists());
    assert_eq!(
        fs::read(fixture.source.join("plain.txt")).unwrap(),
        b"edited since"
    );

    let done = run_restore(&fixture, &request);

    assert_eq!(before, done);
    assert_eq!(
        before.files, 2,
        "one missing and one changed file: {before:?}"
    );
    assert!(before.unchanged > 0);
}

#[test]
fn preview_creates_nothing() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let target = fixture.work.join("not-yet");

    let planned = preview(
        &fixture,
        &restore_request(
            vec![fixture.source.join("nested")],
            Target::Folder(target.clone()),
            ConflictPolicy::Overwrite,
        ),
    );

    assert!(planned.files > 0);
    assert!(!target.exists(), "a preview must not create folders");
}

#[test]
fn restores_into_a_deleted_folder() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    fs::remove_dir_all(fixture.source.join("nested")).unwrap();

    run_restore(
        &fixture,
        &restore_request(
            vec![fixture.source.join("nested")],
            Target::Original,
            ConflictPolicy::KeepBoth,
        ),
    );

    assert_eq!(
        fs::read(fixture.source.join("nested/deeper/data.bin")).unwrap(),
        pseudo_random(64 * 1024, 1)
    );
}

// ---------------------------------------------------------------------------
// Retention
// ---------------------------------------------------------------------------

/// Four snapshots, a day apart, the newest a minute old.
fn daily_history(fixture: &Fixture) -> Vec<String> {
    awkward_tree(&fixture.source);
    let now = jiff::Timestamp::now().as_second();
    (0..4)
        .rev()
        .map(|days_ago| {
            let request = BackupRequest {
                time: Some(now - 60 - days_ago * 86_400),
                ..sources(&fixture.source)
            };
            back_up(fixture, &request).snapshot.id
        })
        .collect()
}

fn snapshot_ids(fixture: &Fixture) -> Vec<String> {
    open(&fixture.repo, &secret())
        .unwrap()
        .snapshots()
        .unwrap()
        .into_iter()
        .map(|snapshot| snapshot.id)
        .collect()
}

#[test]
fn forget_applies_the_rules() {
    let fixture = fixture();
    let taken = daily_history(&fixture);
    let rules = KeepRules {
        daily: Some(2),
        ..KeepRules::default()
    };

    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            &hostname(),
            "",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();

    assert_eq!((report.removed, report.kept), (2, 2));
    let mut left = snapshot_ids(&fixture);
    left.sort();
    let mut newest = taken[2..].to_vec();
    newest.sort();
    assert_eq!(left, newest, "the two newest days are kept");
}

#[test]
fn keep_last_zero_removes_every_unpinned_snapshot() {
    // `Some(0)` ("keep the 0 most recent") is a real, reachable value from
    // the retention settings page (a spinner with no built-in minimum other
    // than 0), distinct from `None` ("this rule is off"): `count()` in
    // `KeepRules::options` maps both to *some* `i32`, so it would be easy
    // for `Some(0)` to accidentally behave like "unlimited" if a future
    // change swapped `Option::map` for `Option::unwrap_or(i32::MAX)` or
    // similar. Every other field stays `None` (off), so nothing else keeps
    // a snapshot alive either.
    let fixture = fixture();
    daily_history(&fixture);
    let rules = KeepRules {
        last: Some(0),
        ..KeepRules::default()
    };

    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            &hostname(),
            "",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();

    assert_eq!((report.removed, report.kept), (4, 0));
    assert!(snapshot_ids(&fixture).is_empty());
}

#[test]
fn a_pinned_snapshot_survives_forget_that_would_otherwise_remove_it() {
    let fixture = fixture();
    let taken = daily_history(&fixture);
    let repo = open(&fixture.repo, &secret()).unwrap();
    let pinned = repo.set_pinned(&taken[0], true).unwrap();
    assert!(pinned.pinned);

    let rules = KeepRules {
        daily: Some(2),
        ..KeepRules::default()
    };
    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            &hostname(),
            "",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();

    let left = open(&fixture.repo, &secret()).unwrap().snapshots().unwrap();
    assert!(
        left.iter().any(|s| s.id == pinned.id),
        "the pinned snapshot must survive under its new ID"
    );
    // Without the pin, only the two newest days would remain (see
    // `forget_applies_the_rules`); the pin keeps one extra.
    assert_eq!(left.len(), 3);
    assert_eq!(report.kept, 3);

    let unpinned = open(&fixture.repo, &secret())
        .unwrap()
        .set_pinned(&pinned.id, false)
        .unwrap();
    assert!(!unpinned.pinned);
}

#[test]
fn pinning_an_already_pinned_snapshot_is_a_harmless_no_op() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    let taken = back_up(&fixture, &sources(&fixture.source)).snapshot.id;
    let repo = open(&fixture.repo, &secret()).unwrap();
    let first = repo.set_pinned(&taken, true).unwrap();
    let second = open(&fixture.repo, &secret())
        .unwrap()
        .set_pinned(&first.id, true)
        .unwrap();
    assert_eq!(
        first.id, second.id,
        "pinning twice must not change the ID again"
    );
}

#[test]
fn forget_leaves_other_computers_alone() {
    let fixture = fixture();
    daily_history(&fixture);
    let rules = KeepRules {
        last: Some(1),
        ..KeepRules::default()
    };

    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            "another-computer",
            "",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();

    assert_eq!((report.removed, report.kept), (0, 0));
    assert_eq!(
        snapshot_ids(&fixture).len(),
        4,
        "every snapshot here was taken by this computer, not that one"
    );
}

#[test]
fn forget_only_touches_this_profiles_own_tagged_snapshots() {
    // Two profiles sharing one repository (the wizard's "open existing"
    // mode allows it): REL-1's own scenario. Each backs up its own folder,
    // three times, tagged with its own profile ID.
    let fixture = fixture();
    let source_a = fixture.source.join("a");
    let source_b = fixture.source.join("b");
    fs::create_dir_all(&source_a).unwrap();
    fs::create_dir_all(&source_b).unwrap();
    fs::write(source_a.join("file.txt"), b"a").unwrap();
    fs::write(source_b.join("file.txt"), b"b").unwrap();
    for _ in 0..3 {
        back_up(
            &fixture,
            &BackupRequest {
                profile_tag: "stellarshot-profile:a".into(),
                ..sources(&source_a)
            },
        );
        back_up(
            &fixture,
            &BackupRequest {
                profile_tag: "stellarshot-profile:b".into(),
                ..sources(&source_b)
            },
        );
    }
    assert_eq!(snapshot_ids(&fixture).len(), 6);

    let rules = KeepRules {
        last: Some(1),
        ..KeepRules::default()
    };
    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            &hostname(),
            "stellarshot-profile:a",
            std::slice::from_ref(&source_a),
        )
        .unwrap();

    assert_eq!(
        (report.removed, report.kept),
        (2, 1),
        "only profile A's own 2 older snapshots are forgotten"
    );
    assert_eq!(
        snapshot_ids(&fixture).len(),
        4,
        "profile B's 3 snapshots must all survive A's clean-up"
    );
    let b_kept = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &KeepRules::default(),
            &hostname(),
            "stellarshot-profile:b",
            std::slice::from_ref(&source_b),
        )
        .unwrap()
        .kept;
    assert_eq!(b_kept, 3, "every one of B's own snapshots is still there");
}

#[test]
fn an_untagged_snapshot_is_claimed_only_by_the_profile_whose_sources_match() {
    // Simulates a snapshot made before per-profile tagging existed: no tag
    // at all, so `forget` falls back to matching sources exactly.
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    let rules = KeepRules {
        last: Some(1),
        ..KeepRules::default()
    };

    let unrelated_sources = fixture.work.join("unrelated");
    let unclaimed = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &rules,
            &hostname(),
            "stellarshot-profile:other",
            std::slice::from_ref(&unrelated_sources),
        )
        .unwrap();
    assert_eq!(
        unclaimed.kept, 0,
        "a profile whose sources do not match must never see it as its own"
    );
    assert_eq!(
        snapshot_ids(&fixture).len(),
        1,
        "and so must not have removed it either"
    );

    let claimed = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &KeepRules::default(),
            &hostname(),
            "stellarshot-profile:mine",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();
    assert_eq!(
        claimed.kept, 1,
        "the profile whose sources actually match may still manage it, tag or no tag"
    );
}

#[test]
fn forget_keeps_everything_without_rules() {
    let fixture = fixture();
    daily_history(&fixture);

    let report = open(&fixture.repo, &secret())
        .unwrap()
        .forget(
            &KeepRules::default(),
            &hostname(),
            "",
            std::slice::from_ref(&fixture.source),
        )
        .unwrap();

    assert_eq!((report.removed, report.kept), (0, 4));
    assert_eq!(snapshot_ids(&fixture).len(), 4);
}

#[test]
fn prune_reclaims_forgotten_data() {
    let fixture = fixture();
    awkward_tree(&fixture.source);
    back_up(&fixture, &sources(&fixture.source));
    // A big file that exists for one backup only, so its data sits in packs
    // of its own. (Unused data sharing a pack with data still in use may be
    // left where it is: rustic limits how much it rewrites per prune.)
    let big = fixture.source.join("big.bin");
    fs::write(&big, pseudo_random(512 * 1024, 7)).unwrap();
    back_up(&fixture, &sources(&fixture.source));
    fs::remove_file(&big).unwrap();
    back_up(&fixture, &sources(&fixture.source));
    let repo = open(&fixture.repo, &secret()).unwrap();
    let rules = KeepRules {
        last: Some(1),
        ..KeepRules::default()
    };
    assert_eq!(
        repo.forget(
            &rules,
            &hostname(),
            "",
            std::slice::from_ref(&fixture.source)
        )
        .unwrap()
        .removed,
        2
    );

    let pruned = repo.prune().unwrap();

    assert!(
        pruned.bytes >= 512 * 1024,
        "the forgotten file's data is no longer needed: {pruned:?}"
    );
    let repo = open(&fixture.repo, &secret()).unwrap();
    repo.check().unwrap();
    let target = fixture.work.join("restored");
    repo.restore_all("latest", &target, Arc::new(NoProgress))
        .unwrap();
    assert_eq!(
        fs::read(restored(&target, &fixture.source).join("plain.txt")).unwrap(),
        b"plain",
        "the kept snapshot is whole"
    );
}

#[test]
fn lock_keys_are_stable_across_versions() {
    // Every process writing to a repository must name its lock the same way,
    // including an older Stellarshot still running during an upgrade. The
    // value is the first 8 bytes of `printf /mnt/backup/home | sha256sum`.
    assert_eq!(
        Location::local("/mnt/backup/home").key(),
        "556651669569762a"
    );
}
