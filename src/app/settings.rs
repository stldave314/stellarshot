// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Mutex;

use super::config::StellarshotConfig;
use super::{APP_ID, Flags, migrate};
use crate::constants::{WINDOW_HEIGHT, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH, WINDOW_WIDTH};
use crate::debug::CONFIG;
use crate::{debug_log, error_log};
use cosmic::app::Settings;
use cosmic::iced::{Limits, Size};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

pub fn init() -> (Settings, Flags) {
    set_logger();
    crate::core::localization::init();
    migrate_settings();
    let settings = get_app_settings();
    let flags = get_flags();
    (settings, flags)
}

/// Bring settings from older versions forward before anything reads them.
/// Failures are reported but never fatal: the app still starts, just without
/// the old entries.
fn migrate_settings() {
    let Some(root) = migrate::config_root() else {
        return;
    };
    // 1. Settings saved under the upstream application ID.
    match migrate::migrate_app_id(&root, APP_ID, 1) {
        Ok(true) => debug_log!(CONFIG, "migrated settings from {}", migrate::OLD_APP_ID),
        Ok(false) => {}
        Err(err) => error_log!(CONFIG, "could not migrate old settings: {err}"),
    }
    // 2. Version 1 repositories become version 2 backup profiles, once.
    if let Some(profiles) = migrate::v1_profiles(&root, APP_ID) {
        let Some(handler) = StellarshotConfig::config_handler() else {
            return;
        };
        let mut config = StellarshotConfig::config();
        let count = profiles.len();
        match config.set_profiles(&handler, profiles) {
            Ok(_) => debug_log!(
                CONFIG,
                "created {count} profiles from version 1 repositories"
            ),
            Err(err) => error_log!(CONFIG, "could not create profiles from old settings: {err}"),
        }
    }
}

pub fn get_app_settings() -> Settings {
    let config = StellarshotConfig::config();

    Settings::default()
        .theme(config.app_theme.theme())
        .size_limits(
            Limits::NONE
                .min_width(WINDOW_MIN_WIDTH)
                .min_height(WINDOW_MIN_HEIGHT),
        )
        .size(Size::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .debug(false)
        // Closing the window minimizes to the panel applet rather than
        // quitting: see `App::on_close_requested` and `App::dbus_activation`.
        .exit_on_close(false)
}

/// Where rustic's and rclone's own diagnostics go, in addition to stderr.
/// Stellarshot is normally launched from the desktop, not a terminal, so
/// stderr alone is not somewhere a person having a backup fail can actually
/// go looking — the same reason `crate::debug` logs to a file rather than
/// stderr. Truncated once per launch, like that file.
const RUSTIC_LOG_PATH: &str = "/tmp/stellarshot-backend.log";

/// Route `log` output (what rustic_core, rustic_backend and the rclone
/// process they run all use) into `tracing`, then to stderr and
/// [`RUSTIC_LOG_PATH`], at `warn` unless `RUST_LOG` says otherwise. Call once
/// from the window's own startup, which owns the file for its whole run and
/// truncates it fresh; see [`set_logger_for_child`] for everything else that
/// can write to the same repository.
///
/// `rustic_core` and `rustic_backend` never call the `tracing` macros this
/// otherwise sets up for — only `log`'s. Without
/// [`tracing_log::LogTracer::init`] bridging the two, every one of their
/// `log::info!`/`warn!` calls, including the line rclone itself prints when a
/// backup to it fails, went nowhere: an error dialog could say "check the
/// logs" while there were none to check.
pub fn set_logger() {
    init_tracing(RUSTIC_LOG_PATH, true);
}

/// [`set_logger`], for a `--run` child or a `--scheduled` run: every real
/// backup happens in one of these, never in the window's own process (see
/// `runner.rs`), so without this call the fix above never actually reached a
/// real backup's own diagnostics, only ones read back or probed in-process.
/// Appends rather than truncates, since several of these can run over a
/// window's lifetime, or with no window open at all, and none of them owns
/// the file the way the window does.
pub fn set_logger_for_child() {
    init_tracing(RUSTIC_LOG_PATH, false);
}

/// The actual setup, taking the log path as an argument so a test can point
/// it somewhere private instead of [`RUSTIC_LOG_PATH`].
fn init_tracing(log_path: &str, truncate: bool) {
    let _ = tracing_log::LogTracer::init();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("stellarshot=warn,rustic_core=warn,rustic_backend=info")
    });
    let log_file = crate::debug::open_private_log_file(log_path, truncate);
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(log_file.map(|file| fmt::layer().with_writer(Mutex::new(file)).with_ansi(false)))
        .with(filter)
        .try_init();
}

pub fn get_flags() -> Flags {
    Flags {
        config_handler: StellarshotConfig::config_handler(),
        config: StellarshotConfig::config(),
        start_wizard: false,
        start_restore: false,
        select: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the fix for a real backup failure whose error dialog said
    /// "check the logs" while `set_logger` had no working path from `log`
    /// (what rustic_core, rustic_backend and rclone's own output all go
    /// through) into anywhere at all: the bridge from `log` into `tracing`
    /// was never actually installed, despite this function's own doc comment
    /// already claiming it was.
    #[test]
    fn rustic_backend_output_reaches_the_log_file() {
        let path = std::env::temp_dir().join(format!(
            "stellarshot-backend-log-test-{}.log",
            std::process::id()
        ));
        init_tracing(path.to_str().unwrap(), true);
        log::warn!(target: "rustic_backend::rclone", "a marker line for the bridge test");

        let contents = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_file(&path);

        assert!(
            contents.contains("a marker line for the bridge test"),
            "the log file must contain what rustic_backend logged: {contents:?}"
        );
    }

    // `set_logger_for_child` is `init_tracing` with `truncate: false`; the
    // truncate/append difference itself is `open_private_log_file`'s own
    // concern and is proven in `debug.rs`'s tests
    // (`truncate_false_appends_instead_of_overwriting`), not here.
    // `tracing_subscriber::registry().try_init()` can only ever succeed once
    // per process, so a second test in this file calling `init_tracing`
    // again would silently no-op and pass or fail depending on which of the
    // two tests happened to run first — not a real test of either behavior.
}
