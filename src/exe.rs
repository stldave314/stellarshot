// SPDX-License-Identifier: GPL-3.0-only

//! Where Stellarshot's own binaries are installed, and handling this
//! process's own binary having been replaced while it kept running (a
//! package upgrade): see [`installed_path`]'s own doc comment.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// This process's own binary as it is installed on disk right now, with the
/// kernel's `" (deleted)"` marker stripped off. The kernel appends that to
/// `/proc/self/exe`'s target once the file it named has been unlinked,
/// which is exactly what happens to a running process's own binary during a
/// package upgrade; the path itself is still where the new binary is.
///
/// For launching a **new** process from an already-running one (the
/// applet's Open button, "New window", and — once every caller goes through
/// here — a notification click and a scheduled unit): the marker is
/// compared and stripped as raw bytes, never through a lossy string
/// conversion, so a path containing non-UTF-8 bytes is still handled
/// correctly.
pub fn installed_path() -> Result<PathBuf, std::io::Error> {
    let path = std::env::current_exe()?;
    Ok(strip_deleted_marker(&path).unwrap_or(path))
}

/// `path` with the kernel's `" (deleted)"` marker removed, comparing as an
/// `OsStr` rather than a lossy string.
fn strip_deleted_marker(path: &Path) -> Option<PathBuf> {
    const MARKER: &str = " (deleted)";
    let bytes = path.as_os_str().as_encoded_bytes();
    let stripped = bytes.strip_suffix(MARKER.as_bytes())?;
    // SAFETY: `stripped` is a prefix of `bytes` (itself a valid
    // platform-encoded byte sequence from `as_encoded_bytes`), split
    // exactly where an all-ASCII suffix was removed — always a boundary
    // `from_encoded_bytes_unchecked` accepts.
    Some(PathBuf::from(unsafe {
        OsStr::from_encoded_bytes_unchecked(stripped)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_deleted_marker_is_stripped() {
        let path = Path::new("/usr/bin/stellarshot (deleted)");
        assert_eq!(
            strip_deleted_marker(path),
            Some(PathBuf::from("/usr/bin/stellarshot"))
        );
    }

    #[test]
    fn a_normal_path_is_left_alone() {
        assert_eq!(
            strip_deleted_marker(Path::new("/usr/bin/stellarshot")),
            None
        );
    }

    #[test]
    fn a_name_that_merely_contains_the_marker_is_left_alone() {
        // Only a trailing marker means "unlinked"; one that happens to be
        // part of a real file name (however unlikely) is not.
        assert_eq!(
            strip_deleted_marker(Path::new("/usr/bin/stellarshot (deleted) v2")),
            None
        );
    }
}
