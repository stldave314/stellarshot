// SPDX-License-Identifier: GPL-3.0-only

//! Where a repository lives on disk, and how to create and remove one safely.
//!
//! A repository is a handful of entries at the root of a folder. Upstream
//! Stellarshot let a user pick any folder — including their home directory —
//! initialised the repository *into* it, and then deleted a repository with
//! `remove_dir_all` on that folder. Picking `~` and pressing Delete would have
//! erased the home directory. Everything here exists to make that impossible:
//! creation refuses a folder that already holds other things, and deletion only
//! ever touches the entries the repository format itself creates.

use std::io;
use std::path::{Path, PathBuf};

/// Entries rustic creates at the root of a repository. Nothing else is ever
/// deleted.
pub const REPOSITORY_ENTRIES: &[&str] = &["config", "keys", "data", "index", "snapshots", "locks"];

/// What a folder holds, from the point of view of creating a repository in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitCheck {
    /// Missing or empty: safe to create a repository here.
    Empty,
    /// Already a repository: open it rather than creating one.
    ExistingRepository,
    /// Holds other files: a repository must not be created on top of them.
    NotEmpty,
}

/// Whether `path` is the root of a repository: it has the `config` file and the
/// `keys` folder every repository starts with.
pub fn is_repository(path: &Path) -> bool {
    path.join("config").is_file() && path.join("keys").is_dir()
}

/// Classify a folder before a repository is created in it.
pub fn check_init_location(path: &Path) -> io::Result<InitCheck> {
    if !path.exists() {
        return Ok(InitCheck::Empty);
    }
    if is_repository(path) {
        return Ok(InitCheck::ExistingRepository);
    }
    if std::fs::read_dir(path)?.next().is_none() {
        Ok(InitCheck::Empty)
    } else {
        Ok(InitCheck::NotEmpty)
    }
}

/// Remove only the repository's own entries, then the folder if it is left
/// empty.
///
/// Returns the entries that were left in place because they are not part of
/// the repository. Fails without touching anything if `path` is not a
/// repository.
pub fn delete_repository(path: &Path) -> io::Result<Vec<PathBuf>> {
    if !is_repository(path) {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} is not a repository", path.display()),
        ));
    }

    for name in REPOSITORY_ENTRIES {
        let entry = path.join(name);
        let Ok(metadata) = entry.symlink_metadata() else {
            continue;
        };
        // A symlink is removed as a link; its target is never followed.
        if metadata.is_dir() {
            std::fs::remove_dir_all(&entry)?;
        } else {
            std::fs::remove_file(&entry)?;
        }
    }

    let mut remaining: Vec<PathBuf> = std::fs::read_dir(path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    remaining.sort();

    if remaining.is_empty() {
        std::fs::remove_dir(path)?;
    }
    Ok(remaining)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Lay out the entries a real repository has at its root.
    fn fake_repository(root: &Path) {
        fs::create_dir_all(root).unwrap();
        fs::write(root.join("config"), b"repository config").unwrap();
        for dir in ["keys", "data/00", "index", "snapshots"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        fs::write(root.join("keys/abc"), b"key").unwrap();
        fs::write(root.join("data/00/pack"), b"pack").unwrap();
    }

    #[test]
    fn init_accepts_missing_and_empty_folders() {
        let tmp = TempDir::new().unwrap();
        assert_eq!(
            check_init_location(&tmp.path().join("new")).unwrap(),
            InitCheck::Empty
        );
        assert_eq!(check_init_location(tmp.path()).unwrap(), InitCheck::Empty);
    }

    #[test]
    fn init_recognises_an_existing_repository() {
        let tmp = TempDir::new().unwrap();
        fake_repository(tmp.path());
        assert_eq!(
            check_init_location(tmp.path()).unwrap(),
            InitCheck::ExistingRepository
        );
    }

    #[test]
    fn init_refuses_non_empty_non_repository() {
        // The home-directory case: a folder full of the user's own files.
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("notes.txt"), b"mine").unwrap();
        assert_eq!(
            check_init_location(tmp.path()).unwrap(),
            InitCheck::NotEmpty
        );
    }

    #[test]
    fn delete_repository_leaves_foreign_files() {
        let tmp = TempDir::new().unwrap();
        fake_repository(tmp.path());
        fs::create_dir(tmp.path().join("Documents")).unwrap();
        fs::write(tmp.path().join("Documents/report.odt"), b"precious").unwrap();

        let remaining = delete_repository(tmp.path()).unwrap();

        for name in REPOSITORY_ENTRIES {
            assert!(!tmp.path().join(name).exists(), "{name} should be gone");
        }
        assert_eq!(
            fs::read(tmp.path().join("Documents/report.odt")).unwrap(),
            b"precious"
        );
        assert!(tmp.path().is_dir(), "the folder itself must survive");
        assert_eq!(remaining, vec![tmp.path().join("Documents")]);
    }

    #[test]
    fn delete_repository_removes_the_folder_when_nothing_else_is_there() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fake_repository(&repo);

        let remaining = delete_repository(&repo).unwrap();

        assert!(remaining.is_empty());
        assert!(!repo.exists());
    }

    #[test]
    fn delete_refuses_a_folder_that_is_not_a_repository() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("notes.txt"), b"mine").unwrap();

        let err = delete_repository(tmp.path()).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::NotFound);
        assert!(tmp.path().join("notes.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn delete_does_not_follow_a_symlinked_entry() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        fake_repository(&repo);
        let elsewhere = tmp.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        fs::write(elsewhere.join("keep.txt"), b"keep").unwrap();
        fs::remove_dir_all(repo.join("snapshots")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, repo.join("snapshots")).unwrap();

        delete_repository(&repo).unwrap();

        assert!(elsewhere.join("keep.txt").exists());
    }
}
