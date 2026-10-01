// SPDX-License-Identifier: GPL-3.0-only

//! Where rustic's, rclone's and Stellarshot's own `log` and `tracing` output
//! goes: stderr and the backend log (see [`crate::debug::backend_log_path`]).

use std::sync::Mutex;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Route `log` output (what `rustic_core`, `rustic_backend` and the rclone
/// process they run all use) into `tracing`, then to stderr and
/// [`crate::debug::backend_log_path`], at `warn` unless `RUST_LOG` says otherwise. Call once
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
    init_tracing(crate::debug::backend_log_path(), true);
}

/// [`set_logger`], for a `--run` child or a `--scheduled` run: every real
/// backup happens in one of these, never in the window's own process (see
/// `runner.rs`), so without this call the fix above never actually reached a
/// real backup's own diagnostics, only ones read back or probed in-process.
/// Appends rather than truncates, since several of these can run over a
/// window's lifetime, or with no window open at all, and none of them owns
/// the file the way the window does.
pub fn set_logger_for_child() {
    init_tracing(crate::debug::backend_log_path(), false);
}

/// The actual setup, taking the log path as an argument so a test can point
/// it somewhere private instead of [`crate::debug::backend_log_path`]. `log_path` of `None`
/// (the private state directory could not be created or verified) still
/// bridges `log` into `tracing` and reaches stderr — the log-to-file part of
/// the fix is best-effort, not something that should silently take the
/// `log`/`tracing` bridge down with it if it fails.
fn init_tracing(log_path: Option<std::path::PathBuf>, truncate: bool) {
    let _ = tracing_log::LogTracer::init();
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // `engine::serve` passes rclone's own output on at `info`.
        EnvFilter::new(
            "stellarshot=warn,stellarshot::engine::serve=info,rustic_core=warn,rustic_backend=info",
        )
    });
    let log_file = log_path.and_then(|path| crate::debug::open_private_log_file(&path, truncate));
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(log_file.map(|file| fmt::layer().with_writer(Mutex::new(file)).with_ansi(false)))
        .with(filter)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Proves the fix for a real backup failure whose error dialog said
    /// "check the logs" while `set_logger` had no working path from `log`
    /// (what `rustic_core`, `rustic_backend` and rclone's own output all go
    /// through) into anywhere at all: the bridge from `log` into `tracing`
    /// was never actually installed, despite this function's own doc comment
    /// already claiming it was.
    #[test]
    fn rustic_backend_output_reaches_the_log_file() {
        let path = std::env::temp_dir().join(format!(
            "stellarshot-backend-log-test-{}.log",
            std::process::id()
        ));
        init_tracing(Some(path.clone()), true);
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
