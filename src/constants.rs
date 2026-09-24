// SPDX-License-Identifier: GPL-3.0-only

//! Implementation tuning values.
//!
//! These rarely change and are not user settings, so they live here as
//! compile-time constants rather than in a runtime config file. User-facing
//! settings belong in `cosmic-config` instead. Add a value here only when code
//! uses it.

/// Initial window width, in logical pixels.
pub const WINDOW_WIDTH: f32 = 800.0;

/// Initial window height, in logical pixels.
pub const WINDOW_HEIGHT: f32 = 800.0;

/// Smallest width the window may be resized to.
pub const WINDOW_MIN_WIDTH: f32 = 400.0;

/// Smallest height the window may be resized to.
pub const WINDOW_MIN_HEIGHT: f32 = 180.0;

/// Shortest interval between two progress reports from one operation. rustic
/// reports per blob; anything faster than this only costs redraws.
pub const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

/// Longest wait for the desktop keyring. Long enough to type a password into
/// an unlock prompt; short enough that a keyring that never answers (no
/// Secret Service, or one waiting on a prompt nobody can see) turns into
/// "not remembered" instead of a request that never ends.
pub const KEYRING_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// How often a scheduled backup also checks the repository for damage. A
/// check reads every index and tree, which is slow on a large backup behind
/// a slow connection, so it runs after a backup at most this often.
pub const CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30 * 86_400);

/// How long a scheduled run waits for the user to click its failure
/// notification before exiting. Clicking opens the backup in Stellarshot.
pub const NOTIFICATION_WAIT: std::time::Duration = std::time::Duration::from_secs(15 * 60);
