// SPDX-License-Identifier: GPL-3.0-only

use std::sync::Mutex;

use super::config::{StellarshotConfig, CONFIG_VERSION};
use super::icon_cache::{IconCache, ICON_CACHE};
use super::migrate;
use crate::app::{App, Flags};
use crate::constants::{WINDOW_HEIGHT, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH, WINDOW_WIDTH};
use crate::debug::CONFIG;
use crate::{debug_log, error_log};
use cosmic::app::Settings;
use cosmic::iced::{Limits, Size};
use cosmic::Application;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init() -> (Settings, Flags) {
    set_logger();
    migrate_settings();
    set_icon_cache();
    let settings = get_app_settings();
    let flags = get_flags();
    (settings, flags)
}

/// Carry settings over from the upstream application ID before anything reads
/// them. A failure is reported but never fatal: the app still starts, just
/// without the old repository list.
fn migrate_settings() {
    let Some(root) = migrate::config_root() else {
        return;
    };
    match migrate::migrate_app_id(&root, App::APP_ID, CONFIG_VERSION) {
        Ok(true) => debug_log!(CONFIG, "migrated settings from {}", migrate::OLD_APP_ID),
        Ok(false) => {}
        Err(err) => error_log!(CONFIG, "could not migrate old settings: {err}"),
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
    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(filter)
        .init();
}

pub fn set_icon_cache() {
    ICON_CACHE.get_or_init(|| Mutex::new(IconCache::new()));
}

pub fn get_flags() -> Flags {
    Flags {
        config_handler: StellarshotConfig::config_handler(),
        config: StellarshotConfig::config(),
    }
}
