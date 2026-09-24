// SPDX-License-Identifier: GPL-3.0-only

//! Repositories reached through rclone, exercised with rclone's own `:local:`
//! backend: the same code path as SFTP and cloud storage (rustic starting
//! `rclone serve restic`, the probe, the delete) without a server or an
//! account.
//!
//! These need rclone installed and fail, rather than skip, without it. CI
//! installs it.

use std::path::Path;
use std::sync::Arc;

use stellarshot::engine::{self, BackupRequest, Location, NoProgress, Probe, Secret, rclone};
use tempfile::TempDir;

const PASSWORD: &str = "correct horse battery staple";

fn require_rclone() {
    assert!(
        rclone::available(),
        "rclone must be installed for these tests (sudo apt install rclone)"
    );
}

/// A repository location at `dir` through rclone, with a private, empty
/// rclone configuration so nothing of the user's is read.
fn through_rclone(scratch: &Path, dir: &Path) -> Location {
    Location::Rclone {
        remote: ":local".to_owned(),
        path: dir.display().to_string(),
        config: scratch.join("rclone.conf"),
    }
}

#[test]
fn backup_through_rclone_round_trips() {
    require_rclone();
    let scratch = TempDir::new().unwrap();
    let source = scratch.path().join("source");
    std::fs::create_dir_all(source.join("nested")).unwrap();
    std::fs::write(source.join("nested/file with space.txt"), b"through rclone").unwrap();
    let location = through_rclone(scratch.path(), &scratch.path().join("repo"));
    let secret = Secret::new(PASSWORD);

    engine::init(&location, &secret).unwrap();
    let report = engine::open(&location, &secret)
        .unwrap()
        .backup(
            &BackupRequest {
                sources: vec![source.clone()],
                ..BackupRequest::default()
            },
            Arc::new(NoProgress),
        )
        .unwrap();
    assert_eq!(report.snapshot.files_new, 1);

    let destination = scratch.path().join("restore");
    engine::open(&location, &secret)
        .unwrap()
        .restore_all("latest", &destination, Arc::new(NoProgress))
        .unwrap();
    let restored = destination
        .join(source.strip_prefix("/").unwrap())
        .join("nested/file with space.txt");
    assert_eq!(std::fs::read(restored).unwrap(), b"through rclone");
}

#[test]
fn rclone_probe_classifies_like_a_folder() {
    require_rclone();
    let scratch = TempDir::new().unwrap();

    let missing = through_rclone(scratch.path(), &scratch.path().join("not-yet"));
    assert_eq!(engine::probe(&missing).unwrap(), Probe::Empty);

    let busy = scratch.path().join("busy");
    std::fs::create_dir_all(&busy).unwrap();
    std::fs::write(busy.join("notes.txt"), b"mine").unwrap();
    assert_eq!(
        engine::probe(&through_rclone(scratch.path(), &busy)).unwrap(),
        Probe::NotEmpty
    );

    let repo = through_rclone(scratch.path(), &scratch.path().join("repo"));
    engine::init(&repo, &Secret::new(PASSWORD)).unwrap();
    assert_eq!(engine::probe(&repo).unwrap(), Probe::Repository);
}

#[test]
fn rclone_delete_leaves_foreign_files() {
    require_rclone();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("shared");
    let location = through_rclone(scratch.path(), &dir);
    engine::init(&location, &Secret::new(PASSWORD)).unwrap();
    std::fs::write(dir.join("report.odt"), b"precious").unwrap();

    engine::delete_repository(&location).unwrap();

    for entry in ["config", "keys", "data", "index", "snapshots"] {
        assert!(!dir.join(entry).exists(), "{entry} should be gone");
    }
    assert_eq!(std::fs::read(dir.join("report.odt")).unwrap(), b"precious");
}

#[test]
fn rclone_delete_refuses_a_folder_that_is_not_a_repository() {
    require_rclone();
    let scratch = TempDir::new().unwrap();
    let dir = scratch.path().join("mine");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("notes.txt"), b"mine").unwrap();

    let err = engine::delete_repository(&through_rclone(scratch.path(), &dir)).unwrap_err();

    assert_eq!(err.kind, engine::ErrorKind::NotARepository);
    assert!(dir.join("notes.txt").exists());
}

#[test]
fn an_unreachable_remote_is_unavailable() {
    require_rclone();
    let scratch = TempDir::new().unwrap();
    // A remote name that is not defined in the (empty) configuration.
    let location = Location::Rclone {
        remote: "no-such-remote".to_owned(),
        path: "backups".to_owned(),
        config: scratch.path().join("rclone.conf"),
    };

    let err = engine::probe(&location).unwrap_err();

    assert_eq!(err.kind, engine::ErrorKind::DestinationUnavailable);
}

#[test]
fn signing_in_never_writes_into_a_readable_configuration() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::TempDir::new().unwrap();
    let config = dir.path().join("rclone.conf");
    std::fs::write(&config, "[old]\ntype = local\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

    // `local` needs no browser; a cloud sign-in writes its token the same way.
    stellarshot::engine::rclone::sign_in(&config, "probe", "local", &[]).unwrap();

    let mode = std::fs::metadata(&config).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "rclone keeps an existing file's mode");
    let text = std::fs::read_to_string(&config).unwrap();
    assert!(text.contains("[probe]") && text.contains("[old]"));
}
