// SPDX-License-Identifier: GPL-3.0-only

//! Sizing a live folder's immediate contents, for choosing what to back up
//! before anything is backed up.
//!
//! Unlike `estimate`, which sizes what a specific `BackupRequest` already
//! excludes, this walks raw disk usage with no filtering at all: a folder's
//! true size is what should decide whether to exclude it, not the other way
//! around. Sized one folder's worth of children at a time, not the whole
//! tree up front, so browsing a large home folder does not mean walking all
//! of it before the first row appears.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};

/// One entry directly inside a folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// A file's own size; a directory's is everything under it. A symlink
    /// counts by its own size and is never followed, the same way `du`
    /// itself defaults to behaving, which also keeps a cycle (a symlink
    /// back to an ancestor) from ever being walked into.
    pub size: u64,
}

/// The immediate children of `dir`, each with its full size, largest first.
///
/// Reports each entry through `progress` as its own size finishes, so a
/// folder with a few large children shows rows filling in one at a time
/// rather than the whole row staying blank until every child is done.
/// Returns `Ok(None)` if `cancel` was set before finishing; already-reported
/// entries stand, since the caller has them from `progress` regardless.
pub fn list_with_sizes(
    dir: &Path,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(DiskEntry),
) -> Result<Option<Vec<DiskEntry>>, EngineError> {
    let read_dir = fs::read_dir(dir)
        .map_err(|err| EngineError::new(ErrorKind::Io, format!("{}: {err}", dir.display())))?;
    let mut entries = Vec::new();
    for item in read_dir {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Ok(item) = item else { continue };
        let path = item.path();
        let Ok(metadata) = item.metadata() else {
            // Gone, or unreadable (permissions) between the listing and
            // here: skipped rather than failing the whole folder over one
            // entry neither Stellarshot nor the user can do anything about.
            continue;
        };
        let is_dir = metadata.is_dir();
        let size = if is_dir {
            match dir_size(&path, cancel)? {
                Some(size) => size,
                None => return Ok(None),
            }
        } else {
            metadata.len()
        };
        let entry = DiskEntry {
            name: item.file_name().to_string_lossy().into_owned(),
            path,
            is_dir,
            size,
        };
        progress(entry.clone());
        entries.push(entry);
    }
    entries.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
    Ok(Some(entries))
}

/// The total size of everything under `dir`, symlinks counted by their own
/// size and never followed. `Ok(None)` if `cancel` was set partway through.
fn dir_size(dir: &Path, cancel: &AtomicBool) -> Result<Option<u64>, EngineError> {
    let mut total = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Ok(read_dir) = fs::read_dir(&current) else {
            // Unreadable partway through (permissions, or it vanished): its
            // share is simply left out, the same as `du` does, rather than
            // failing every sibling's size along with it.
            continue;
        };
        for item in read_dir {
            let Ok(item) = item else { continue };
            let Ok(metadata) = item.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(item.path());
            } else {
                total += metadata.len();
            }
        }
    }
    Ok(Some(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tempfile::TempDir;

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn files_are_sized_by_their_own_length() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"hi").unwrap();

        let mut seen = Vec::new();
        let entries = list_with_sizes(dir.path(), &no_cancel(), &mut |entry| seen.push(entry))
            .unwrap()
            .unwrap();

        assert_eq!(entries.len(), 2);
        assert_eq!(seen.len(), 2, "each entry is also reported as progress");
        let a = entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(a.size, 5);
        assert!(!a.is_dir);
    }

    #[test]
    fn a_folders_size_is_everything_under_it() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("nested/deeper")).unwrap();
        std::fs::write(dir.path().join("nested/one.bin"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("nested/deeper/two.bin"), vec![0u8; 250]).unwrap();

        let entries = list_with_sizes(dir.path(), &no_cancel(), &mut |_| {})
            .unwrap()
            .unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "nested");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].size, 350);
    }

    #[test]
    fn largest_comes_first() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("small.bin"), vec![0u8; 10]).unwrap();
        std::fs::write(dir.path().join("big.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(dir.path().join("medium.bin"), vec![0u8; 100]).unwrap();

        let entries = list_with_sizes(dir.path(), &no_cancel(), &mut |_| {})
            .unwrap()
            .unwrap();

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["big.bin", "medium.bin", "small.bin"]);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_counts_its_own_size_and_is_never_followed() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("real")).unwrap();
        std::fs::write(dir.path().join("real/big.bin"), vec![0u8; 10_000]).unwrap();
        symlink(dir.path().join("real"), dir.path().join("link")).unwrap();

        let entries = list_with_sizes(dir.path(), &no_cancel(), &mut |_| {})
            .unwrap()
            .unwrap();

        let link = entries.iter().find(|e| e.name == "link").unwrap();
        assert!(
            !link.is_dir,
            "a symlink is not treated as the directory it points to"
        );
        assert!(
            link.size < 10_000,
            "the symlink's own size, not the 10 000 bytes it points at: {}",
            link.size
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_cycle_does_not_hang() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("real")).unwrap();
        symlink(dir.path(), dir.path().join("real/back-to-parent")).unwrap();

        // Must terminate at all; a cycle followed into `dir_size` would spin
        // forever rather than return.
        let entries = list_with_sizes(dir.path(), &no_cancel(), &mut |_| {})
            .unwrap()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "real");
    }

    #[test]
    fn cancelling_stops_the_walk() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("a")).unwrap();
        std::fs::create_dir_all(dir.path().join("b")).unwrap();
        let cancel = AtomicBool::new(true);

        let result = list_with_sizes(dir.path(), &cancel, &mut |_| {}).unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn an_unreadable_root_reports_an_error_not_an_empty_list() {
        let dir = TempDir::new().unwrap();
        let missing = dir.path().join("does-not-exist");

        let err = list_with_sizes(&missing, &no_cancel(), &mut |_| {}).unwrap_err();

        assert_eq!(err.kind, ErrorKind::Io);
    }
}
