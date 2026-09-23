// SPDX-License-Identifier: GPL-3.0-only

//! Stellarshot's settings, stored through `cosmic-config`.

use cosmic::{
    cosmic_config::{self, Config, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry},
    theme,
};
use serde::{Deserialize, Serialize};

use super::APP_ID;
use crate::debug::CONFIG;
use crate::debug_log;
use crate::profile::Profile;

/// Version 2 replaced the `repositories` list with backup profiles.
pub const CONFIG_VERSION: u64 = 2;

#[derive(Clone, Default, Debug, Eq, PartialEq, Deserialize, Serialize, CosmicConfigEntry)]
#[version = 2]
pub struct StellarshotConfig {
    pub app_theme: AppTheme,
    pub profiles: Vec<Profile>,
}

impl StellarshotConfig {
    pub fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    pub fn config() -> StellarshotConfig {
        match Self::config_handler() {
            Some(config_handler) => {
                StellarshotConfig::get_entry(&config_handler).unwrap_or_else(|(errs, config)| {
                    debug_log!(CONFIG, "errors loading config: {errs:?}");
                    config
                })
            }
            None => StellarshotConfig::default(),
        }
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AppTheme {
    Dark,
    Light,
    #[default]
    System,
}

impl AppTheme {
    pub fn theme(&self) -> theme::Theme {
        match self {
            Self::Dark => theme::Theme::dark(),
            Self::Light => theme::Theme::light(),
            Self::System => theme::system_preference(),
        }
    }
}
