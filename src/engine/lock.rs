// SPDX-License-Identifier: GPL-3.0-only

//! One writer per repository.
//!
//! rustic takes no repository lock of its own, so two processes could back up
//! to, or prune, the same repository at once. Every process that writes — the
//! window, a `--run` child, a scheduled run — first takes an exclusive `flock`
//! on a file named after the repository's location. The kernel releases it if
//! the holder dies, so a killed backup can never leave a stale lock behind.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::error::{EngineError, ErrorKind};
use super::repo::Location;

/// The per-user directory lock and progress files live in.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(std::env::temp_dir)
        .join("stellarshot")
}

/// Where the latest progress of a write to `location` is published, for a
/// window that did not start it.
pub fn progress_path(location: &Location) -> PathBuf {
    runtime_dir().join(format!("{}.progress", location.key()))
}

/// Held for the duration of a write; released when dropped.
#[derive(Debug)]
pub struct WriteLock {
    _file: File,
}

/// Take the write lock for `location`, or fail with `Locked` at once if another
/// process holds it.
pub fn acquire(location: &Location) -> Result<WriteLock, EngineError> {
    acquire_in(&runtime_dir(), location)
}

/// [`acquire`], with the lock directory given explicitly.
pub fn acquire_in(dir: &Path, location: &Location) -> Result<WriteLock, EngineError> {
    create_private_dir(dir)?;
    let path = dir.join(format!("{}.lock", location.key()));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)?;
    match file.try_lock() {
        Ok(()) => Ok(WriteLock { _file: file }),
        Err(TryLockError::WouldBlock) => {
            Err(EngineError::new(ErrorKind::Locked, location.describe()))
        }
        Err(TryLockError::Error(err)) => Err(err.into()),
    }
}

/// Whether some process currently holds the write lock for `location`: a
/// backup running from a schedule, or another window, that this process did
/// not itself start. Never blocks; an unlocked repository returns `false`
/// at once, and any other error (the lock directory could not even be
/// created) is treated the same way, since this is a status display, not a
/// guard against writing.
pub fn is_running(location: &Location) -> bool {
    matches!(acquire(location), Err(err) if err.kind == ErrorKind::Locked)
}

fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn a_second_lock_on_the_same_repository_is_refused() {
        let dir = TempDir::new().unwrap();
        let location = Location::local("/backups/home");

        let first = acquire_in(dir.path(), &location).unwrap();
        let second = acquire_in(dir.path(), &location).unwrap_err();
        assert_eq!(second.kind, ErrorKind::Locked);

        drop(first);
        acquire_in(dir.path(), &location).expect("released on drop");
    }

    #[test]
    fn different_repositories_do_not_block_each_other() {
        let dir = TempDir::new().unwrap();
        let _a = acquire_in(dir.path(), &Location::local("/backups/a")).unwrap();
        let _b = acquire_in(dir.path(), &Location::local("/backups/b")).unwrap();
    }

    #[test]
    fn is_running_reflects_a_real_held_lock_without_taking_it_over() {
        let dir = TempDir::new().unwrap();
        let location = Location::local("/backups/home");
        assert!(
            !is_running_in(dir.path(), &location),
            "nothing holds it yet"
        );

        let held = acquire_in(dir.path(), &location).unwrap();
        assert!(is_running_in(dir.path(), &location));

        drop(held);
        assert!(!is_running_in(dir.path(), &location), "released on drop");
    }

    fn is_running_in(dir: &Path, location: &Location) -> bool {
        matches!(acquire_in(dir, location), Err(err) if err.kind == ErrorKind::Locked)
    }
}
