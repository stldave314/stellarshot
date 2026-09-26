// SPDX-License-Identifier: GPL-3.0-only

//! Stellarshot's settings, stored through `cosmic-config`.

use std::path::PathBuf;

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
    /// Glob patterns left out of every backup, such as `node_modules` or
    /// Rust's `target`, set once instead of on each profile.
    pub global_exclude_patterns: Vec<String>,
    /// Use this folder instead of rustic's own default cache location
    /// (`~/.cache/rustic`), for every repository. Ignored when `no_cache`
    /// is on.
    pub cache_dir: Option<PathBuf>,
    /// Do not cache repository index data locally at all: slower, but
    /// nothing worth keeping on a machine low on disk space.
    pub no_cache: bool,
    /// The web interface's own settings: never any secret material itself
    /// (the shared password lives in the keyring, like a repository's own;
    /// an API token is kept only as a hash), just what is turned on and who
    /// may reach it.
    pub web: WebConfig,
}

impl StellarshotConfig {
    pub fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    pub fn config() -> StellarshotConfig {
        let config = match Self::config_handler() {
            Some(config_handler) => {
                StellarshotConfig::get_entry(&config_handler).unwrap_or_else(|(errs, config)| {
                    debug_log!(CONFIG, "errors loading config: {errs:?}");
                    config
                })
            }
            None => StellarshotConfig::default(),
        };
        crate::engine::cache_settings::set(config.cache_dir.clone(), config.no_cache);
        config
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

/// Who can reach the web interface, network-wise. Independent of whether any
/// authentication method is turned on: `Off` is the only setting that
/// actually stops the daemon from listening at all.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum NetworkScope {
    /// The daemon does not listen at all.
    #[default]
    Off,
    /// Bound to `127.0.0.1` only: reachable through the machine's own SSH
    /// tunnel or a reverse proxy, never directly from another device.
    Localhost,
    /// Bound to every interface, reachable from the rest of the LAN.
    Lan,
}

/// The web interface's settings: never a secret itself, only what is turned
/// on. The shared password lives in the keyring (`crate::keyring`); an API
/// token is kept here only as a hash, since the hash alone is enough to
/// check one without ever storing the raw value anywhere but the moment it
/// is generated.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WebConfig {
    pub scope: NetworkScope,
    pub password_enabled: bool,
    pub token_enabled: bool,
    pub pam_enabled: bool,
    /// The current API token's SHA-256 hash, hex-encoded. `None` until one
    /// has been generated; regenerating replaces it, invalidating the old
    /// token immediately.
    pub token_hash: Option<String>,
    /// Addresses or CIDR ranges allowed to reach the web interface, on top
    /// of whatever `scope` itself already allows. Empty means every address
    /// `scope` allows, unrestricted.
    pub allowed_addresses: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_web_interface_defaults_to_off() {
        // A fresh install, or one that predates this setting entirely, must
        // never come up listening on a network by surprise.
        assert_eq!(WebConfig::default().scope, NetworkScope::Off);
    }
}
