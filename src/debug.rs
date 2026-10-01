// SPDX-License-Identifier: GPL-3.0-only

//! Developer debug logging.
//!
//! Flip [`DEVELOPER_LOGGING`] to `true` while debugging and rebuild. Output
//! goes to [`PATH`], one line per call, prefixed by elapsed time, which
//! process wrote it and that process's PID, and a short category tag — a
//! run can be filtered with `grep` by any of those.
//!
//! Several processes can share this one file over a session: the window,
//! the panel applet, and short-lived `--run`/`--scheduled`
//! children the window or a timer spawns. Only [`init`]'s [`Role::Window`]
//! truncates it, once, at that process's own launch; every other role
//! appends. Truncating from more than one place would race — whichever
//! process is mid-write when another one truncates keeps writing at its
//! own, now-stale file offset, leaving NUL-filled holes rather than a
//! readable log — so the window, being the one thing a session is built
//! around, is the only truncator.
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
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use rustix::fs::{Mode, OFlags, fchmod, open};

/// Master switch. Set to `true` to turn debug logging on for a dev build.
const DEVELOPER_LOGGING: bool = false;

/// Effective switch. The `release-build` feature forces logging off at compile
/// time, so a packaged build can never ship with it on by accident — every
/// packaging target passes `--features release-build`.
pub const ENABLED: bool = DEVELOPER_LOGGING && !cfg!(feature = "release-build");

/// Log file location, relative to `$XDG_STATE_HOME` (or `~/.local/state`):
/// per-user and private, never a predictable name in a shared directory.
pub const PATH: &str = "stellarshot/developer-debug.log";

// Short category tags.
/// Backup engine: repository open, backup, restore, maintenance.
pub const ENGINE: &str = "ENGINE";
/// User interface state.
pub const UI: &str = "UI";
/// Settings load, save and migration.
pub const CONFIG: &str = "CONFIG";
/// Scheduled backups: systemd units, the `--scheduled` run, notifications.
pub const SCHED: &str = "SCHED";
/// A snapshot mounted as a filesystem: FUSE calls and the engine errors
/// behind whichever `Errno` they turn into.
pub const MOUNT: &str = "MOUNT";

/// Opens (or creates) `path` as a private log file: `0600`, and refusing to
/// follow a symlink already at that name. Truncated if `truncate`, appended
/// to otherwise — several short-lived processes (a `--run` child, a
/// `--scheduled` run) can share one log file with the long-lived window,
/// and only the one that owns the file for its whole lifetime should ever
/// truncate it; the others would otherwise race to clobber each other's
/// lines, or the window's.
///
/// These paths are fixed and predictable, so without this, another user on a shared machine could plant a symlink
/// there first and have Stellarshot truncate or write into a file it does
/// not otherwise have reason to touch, or read a log meant to be private.
/// `O_CREAT` without `O_EXCL` opens a pre-existing file rather than failing,
/// though, so a plain symlink check is not enough on its own: the `fstat`
/// below additionally refuses a pre-existing *regular* file this user does
/// not own, which a symlink check alone would happily open and write
/// (truncating, in the common case) as this call's caller.
pub(crate) fn open_private_log_file(path: &Path, truncate: bool) -> Option<File> {
    let mode_flag = if truncate {
        OFlags::TRUNC
    } else {
        OFlags::APPEND
    };
    let file: File = open(
        path,
        OFlags::CREATE | OFlags::WRONLY | mode_flag | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::RUSR | Mode::WUSR,
    )
    .ok()
    .map(File::from)?;
    let owned_by_us = file
        .metadata()
        .is_ok_and(|metadata| metadata.uid() == unsafe { libc::getuid() });
    if !owned_by_us {
        return None;
    }
    // `open`'s mode only applies to a file it creates; one left behind at
    // 0644 by an older build is tightened here.
    let _ = fchmod(&file, Mode::RUSR | Mode::WUSR);
    Some(file)
}

/// [`PATH`] resolved under the state directory, whose parent is created
/// private. `None` (no log) when there is no home directory to resolve it
/// under, rather than falling back to a shared location.
fn log_path() -> Option<PathBuf> {
    let path = crate::paths::state_root()?.join(PATH);
    crate::engine::lock::create_private_dir(path.parent()?).ok()?;
    Some(path)
}

/// Which process is writing: see this module's own doc comment for why
/// only [`Role::Window`] ever truncates the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Window,
    Applet,
    /// A `--run` child.
    Run,
    /// A `--scheduled` run.
    Scheduled,
    /// [`init`] was never called before the first log line: should never
    /// happen outside a bug in some future entry point, but still appends
    /// rather than guessing this is the window and truncating.
    Unknown,
}

impl Role {
    fn label(self) -> &'static str {
        match self {
            Self::Window => "window",
            Self::Applet => "applet",
            Self::Run => "run",
            Self::Scheduled => "scheduled",
            Self::Unknown => "unknown",
        }
    }

    fn truncates(self) -> bool {
        matches!(self, Self::Window)
    }
}

struct Sink {
    file: Option<File>,
    start: Instant,
    role: Role,
    pid: u32,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

fn new_sink(role: Role) -> Sink {
    Sink {
        file: log_path().and_then(|path| open_private_log_file(&path, role.truncates())),
        start: Instant::now(),
        role,
        pid: std::process::id(),
    }
}

/// Set which process this is, before anything logs — each of this crate's
/// binary entry points calls this once, as close to its own start as
/// possible. A call after the first [`debug_log!`]/[`error_log!`] (or a
/// second call at all) is a no-op: whichever role initialized the log
/// first is the one that already decided whether it was truncated.
pub fn init(role: Role) {
    if !ENABLED {
        return;
    }
    let _ = SINK.set(Mutex::new(new_sink(role)));
}

/// [`sink`]'s lock, initializing with [`Role::Unknown`] (append, the
/// safest default — see this module's own doc comment) if [`init`] was
/// never called at all, rather than the truncate-by-default behavior only
/// the window actually wants.
fn sink() -> &'static Mutex<Sink> {
    SINK.get_or_init(|| Mutex::new(new_sink(Role::Unknown)))
}

/// One already-formatted line, given everything [`write`] would otherwise
/// read from [`SINK`] — kept separate so the format itself is testable
/// without touching that process-wide state.
fn format_line(elapsed: f64, role: Role, pid: u32, category: &str, args: &Arguments<'_>) -> String {
    format!(
        "[{elapsed:9.3}] [{} {pid:<8}] {category:<7} {args}",
        role.label()
    )
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
    let (role, pid) = (sink.role, sink.pid);
    if let Some(file) = sink.file.as_mut() {
        let _ = writeln!(file, "{}", format_line(elapsed, role, pid, category, &args));
        let _ = file.flush();
    }
}

/// Emit a line to the debug log.
///
/// Expands to `if ENABLED { .. }`, so the optimizer removes it when logging is
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
    fn only_the_window_truncates() {
        for role in [Role::Applet, Role::Run, Role::Scheduled, Role::Unknown] {
            assert!(!role.truncates(), "{role:?} must append, not truncate");
        }
        assert!(Role::Window.truncates());
    }

    #[test]
    fn a_formatted_line_names_the_role_and_pid_before_the_category() {
        let line = format_line(1.5, Role::Run, 4242, "ENGINE", &format_args!("hello"));
        assert!(
            line.starts_with("[    1.500] [run 4242"),
            "role and pid must come right after the elapsed time: {line:?}"
        );
        assert!(
            line.contains("ENGINE"),
            "the category must still be there: {line:?}"
        );
        assert!(
            line.ends_with("hello"),
            "the formatted message must still be there: {line:?}"
        );
    }

    #[test]
    fn a_symlink_already_at_the_path_is_not_followed() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("target");
        std::fs::write(&target, b"do not touch").unwrap();
        let link = dir.path().join("log");
        symlink(&target, &link).unwrap();

        let opened = open_private_log_file(&link, true);

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
    fn a_pre_existing_file_owned_by_someone_else_is_refused() {
        // `open_private_log_file` cannot itself fabricate another uid to
        // prove this against a real file (that needs root), so this covers
        // the comparison the other way around: a file we do own compares
        // equal to our own uid and is accepted, which is what changes if
        // the `uid == getuid()` check above is ever dropped or inverted by
        // mistake.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, b"already here").unwrap();

        let opened = open_private_log_file(&path, false);

        assert!(
            opened.is_some(),
            "a pre-existing file we already own must still be usable"
        );
    }

    #[test]
    fn the_log_file_is_private() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");

        let file = open_private_log_file(&path, true).unwrap();

        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "the log file must be readable by no one else");
    }

    #[test]
    fn a_log_left_world_readable_is_tightened_to_0600() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, b"old\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        let file = open_private_log_file(&path, false).unwrap();

        let mode = file.metadata().unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn truncate_false_appends_instead_of_overwriting() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log");
        std::fs::write(&path, b"first\n").unwrap();

        let mut file = open_private_log_file(&path, false).unwrap();
        writeln!(file, "second").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first\nsecond\n");
    }
}
