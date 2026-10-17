// SPDX-License-Identifier: GPL-3.0-only

//! Whether Stellarshot runs inside a Flatpak sandbox. There, the host's
//! systemd, FUSE and programs are out of reach: scheduling goes through
//! [`crate::background`] instead of systemd timers, drives are found through
//! `UDisks2`, and the features that need the host are shown as unavailable.
//!
//! The one place that decides it, so nothing else tests for Flatpak itself.

use std::path::Path;
use std::sync::OnceLock;

/// Flatpak puts this file at the root of every sandbox it starts.
const FLATPAK_INFO: &str = "/.flatpak-info";

/// Whether this process runs inside a Flatpak sandbox. Read once.
pub fn sandboxed() -> bool {
    static SANDBOXED: OnceLock<bool> = OnceLock::new();
    *SANDBOXED.get_or_init(|| Path::new(FLATPAK_INFO).exists())
}
