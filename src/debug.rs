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
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

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

struct Sink {
    file: Option<File>,
    start: Instant,
}

static SINK: OnceLock<Mutex<Sink>> = OnceLock::new();

fn sink() -> &'static Mutex<Sink> {
    SINK.get_or_init(|| {
        // Truncate once per process launch.
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(PATH)
            .ok();
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
