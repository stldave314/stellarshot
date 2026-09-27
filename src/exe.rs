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

/// This process's own running image, for spawning a `--run` child: the
/// magic `/proc/self/exe` link, not [`installed_path`]. The kernel resolves
/// it to whatever inode this process is actually executing from, so opening
/// or executing it keeps working even after a package upgrade unlinks the
/// path it was launched from — confirmed directly (a running process whose
/// own backing file was deleted still re-executed itself successfully
/// through this link), not assumed from how the mechanism is documented.
/// Spawning through here rather than [`installed_path`] also guarantees the
/// child runs the *same* binary as this process, so the `--run` child's
/// stdin/stdout JSON protocol always matches — [`installed_path`] would
/// launch whatever is newly installed, a version this process has not
/// necessarily negotiated a protocol with.
pub fn running_image() -> PathBuf {
    PathBuf::from("/proc/self/exe")
}

/// Whether this process's own binary has been replaced since it started —
/// a package upgrade unlinked the file it was executing from, so
/// `/proc/self/exe`'s target now carries the marker [`installed_path`]
/// strips. A long-running process (the web daemon) polls this to notice an
/// upgrade landed and exit for `Restart=` to pick up the new binary,
/// something a short-lived one (a wizard step, a scheduled run) never
/// needs to ask since it is done before an upgrade could matter.
pub fn was_replaced() -> bool {
    std::env::current_exe()
        .ok()
        .is_some_and(|path| strip_deleted_marker(&path).is_some())
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

    /// The exact property [`running_image`] depends on: a running
    /// process's own `/proc/<pid>/exe` link (`/proc/self/exe` is that same
    /// link, by another name, for the calling process itself) stays
    /// resolvable and executable after the file it names is deleted —
    /// confirmed directly here, not assumed from how `/proc` is documented.
    #[test]
    fn a_running_processs_exe_link_stays_executable_after_its_file_is_deleted() {
        let dir = tempfile::TempDir::new().unwrap();
        let copy = dir.path().join("copy-of-sleep");
        std::fs::copy("/bin/sleep", &copy).expect("this test needs /bin/sleep to exist");
        let mut long_lived = std::process::Command::new(&copy).arg("5").spawn().unwrap();
        let exe_link = format!("/proc/{}/exe", long_lived.id());

        std::fs::remove_file(&copy).unwrap();

        let status = std::process::Command::new(&exe_link)
            .arg("0")
            .status()
            .expect("the exe link should still be executable after its file was deleted");

        assert!(status.success());
        let _ = long_lived.kill();
        let _ = long_lived.wait();
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
