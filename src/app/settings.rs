// SPDX-License-Identifier: GPL-3.0-only

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
}

/// Route `log` and `tracing` output (including rustic's) to stderr, at `warn`
/// unless `RUST_LOG` says otherwise.
pub fn set_logger() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("stellarshot=warn,rustic_core=warn"));
    let _ = tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
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
