// SPDX-License-Identifier: GPL-3.0-only

//! Developer debug logging.
//!
//! Flip [`DEVELOPER_LOGGING`] to `true` while debugging and rebuild. Output
//! goes to [`PATH`], truncated once per process launch, with each line
//! prefixed by elapsed time and a short category tag so a run can be filtered
//! with `grep`.
//!
//! Logging goes to a *file* rather than stderr on purpose: a scheduled backup
//! runs under a systemd timer, where stderr ends up in a journal most people
//! never read.
//!
//! Genuine errors still go to stderr via [`error_log!`]; this module is for
//! diagnostics, not a replacement for real error reporting.

use std::fmt::Arguments;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use rustix::fs::{Mode, OFlags, open};

/// Master switch. Set to `true` to turn debug logging on for a dev build.
const DEVELOPER_LOGGING: bool = false;

/// Effective switch. The `release-build` feature forces logging off at compile
/// time, so a packaged build can never ship with it on by accident — every
/// packaging target passes `--features release-build`.
pub const ENABLED: bool = DEVELOPER_LOGGING && !cfg!(feature = "release-build");

/// Log file location.
pub const PATH: &str = "/tmp/stellarshot-debug.log";

// Short category tags.
/// Backup engine: repository open, backup, restore, maintenance.
pub const ENGINE: &str = "ENGINE";
/// User interface state.
pub const UI: &str = "UI";
/// Settings load, save and migration.
pub const CONFIG: &str = "CONFIG";
/// Scheduled backups: systemd units, the `--scheduled` run, notifications.
pub const SCHED: &str = "SCHED";

/// Opens (or creates) `path` as a private log file: `0600`, and refusing to
/// follow a symlink already at that name. Truncated if `truncate`, appended
/// to otherwise — several short-lived processes (a `--run` child, a
/// `--scheduled` run) can share one log file with the long-lived window,
/// and only the one that owns the file for its whole lifetime should ever
/// truncate it; the others would otherwise race to clobber each other's
/// lines, or the window's.
///
/// These paths are fixed and predictable (`/tmp/stellarshot-*.log`), so
/// without this, another user on a shared machine could plant a symlink
/// there first and have Stellarshot truncate or write into a file it does
/// not otherwise have reason to touch, or read a log meant to be private.
pub(crate) fn open_private_log_file(path: &str, truncate: bool) -> Option<File> {
    let mode_flag = if truncate {
        OFlags::TRUNC
    } else {
        OFlags::APPEND
    };
    open(
        path,
        OFlags::CREATE | OFlags::WRONLY | mode_flag | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .ok()
    .map(File::from)
}

struct Sink {
    file: Option<File>,
    start: Instant,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

fn sink() -> &'static Mutex<Sink> {
    SINK.get_or_init(|| {
        // Truncate once per process launch.
        let file = open_private_log_file(PATH, true);
        Mutex::new(Sink {
            file,
            start: Instant::now(),
        })
    })
}

/// Write one already-formatted line. Called by [`debug_log!`]; not intended to
/// be used directly.
pub fn write(category: &str, args: Arguments<'_>) {
    if !ENABLED {
        return;
    }
    let Ok(mut sink) = sink().lock() else {
        return;
    };
    let elapsed = sink.start.elapsed().as_secs_f64();
    if let Some(file) = sink.file.as_mut() {
        let _ = writeln!(file, "[{elapsed:9.3}] {category:<7} {args}");
        let _ = file.flush();
    }
}

/// Emit a line to the debug log.
///
/// Expands to `if ENABLED { .. }`, so the optimiser removes it when logging is
/// off — but the arguments are still type-checked, which stops disabled call
/// sites from silently rotting.
#[macro_export]
macro_rules! debug_log {
    ($category:expr, $($arg:tt)*) => {
        if $crate::debug::ENABLED {
            $crate::debug::write($category, format_args!($($arg)*));
        }
    };
}

/// Report a genuine error: always to stderr, and to the debug log as well.
///
/// Unlike [`debug_log!`] this is never compiled out.
pub fn error(category: &str, args: Arguments<'_>) {
    eprintln!("stellarshot: {category}: {args}");
    write(category, args);
}

/// Report a genuine error. See [`error`].
#[macro_export]
macro_rules! error_log {
    ($category:expr, $($arg:tt)*) => {
        $crate::debug::error($category, format_args!($($arg)*))
    };
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{PermissionsExt, symlink};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn a_symlink_already_at_the_path_is_not_followed() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"do not touch").unwrap();
        let link = dir.path().join("log");
        symlink(&target, &link).unwrap();

        let opened = open_private_log_file(link.to_str().unwrap(), true);

        assert!(
            opened.is_none(),
            "a symlink at the log path must be refused, not followed"
        );
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"do not touch",
            "the symlink's target must be untouched"
        );
    }

    #[test]
    fn the_log_file_is_private() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");

        let file = open_private_log_file(path.to_str().unwrap(), true).unwrap();

        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the log file must be readable by no one else");
    }

    #[test]
    fn truncate_false_appends_instead_of_overwriting() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, b"first\n").unwrap();

        let mut file = open_private_log_file(path.to_str().unwrap(), false).unwrap();
        writeln!(file, "second").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
    }
}
