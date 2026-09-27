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

/// Where the web interface listens unless a setting says otherwise. A
/// `const` rather than a magic number in three places (the default below,
/// the settings page, and whatever a fresh `WebConfig` gets in a test).
pub const DEFAULT_WEB_PORT: u16 = 8737;

fn default_web_port() -> u16 {
    DEFAULT_WEB_PORT
}

/// The web interface's settings: never a secret itself, only what is turned
/// on. The shared password lives in the keyring (`crate::keyring`); an API
/// token is kept here only as a hash, since the hash alone is enough to
/// check one without ever storing the raw value anywhere but the moment it
/// is generated.
///
/// New fields need `#[serde(default = ...)]` (or to be an `Option`): this
/// struct is not itself version-gated the way `StellarshotConfig` is, so an
/// old config missing a field added later must still deserialize, rather
/// than the whole section silently reverting to defaults and dropping
/// whatever the user already turned on.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WebConfig {
    pub scope: NetworkScope,
    #[serde(default = "default_web_port")]
    pub port: u16,
    pub password_enabled: bool,
    pub token_enabled: bool,
    pub pam_enabled: bool,
    /// The current API token's SHA-256 hash, hex-encoded. `None` until one
    /// has been generated; regenerating replaces it. If the daemon is
    /// running, Settings restarts it right away so the old token stops
    /// working immediately rather than on the next manual restart — see
    /// `App::restart_web_daemon_if_active`.
    pub token_hash: Option<String>,
    /// Addresses or CIDR ranges allowed to reach the web interface, on top
    /// of whatever `scope` itself already allows. Empty means every address
    /// `scope` allows, unrestricted. Adding or removing one restarts the
    /// daemon the same way a token change does, if it is running.
    pub allowed_addresses: Vec<String>,
    /// A certificate and private key to use instead of the daemon's own
    /// self-signed one, generated once and kept under its data directory
    /// (see `crate::web_tls`). Either both are set or neither is: one
    /// without the other falls back to the self-signed certificate.
    #[serde(default)]
    pub tls_cert_path: Option<PathBuf>,
    #[serde(default)]
    pub tls_key_path: Option<PathBuf>,
}

impl Default for WebConfig {
    fn default() -> Self {
        Self {
            scope: NetworkScope::default(),
            port: DEFAULT_WEB_PORT,
            password_enabled: false,
            token_enabled: false,
            pam_enabled: false,
            token_hash: None,
            allowed_addresses: Vec::new(),
            tls_cert_path: None,
            tls_key_path: None,
        }
    }
}

impl WebConfig {
    /// Both a certificate and a key are configured, and neither path is
    /// empty. A mismatched pair (one set, one not) is treated as "use the
    /// self-signed certificate instead" rather than an error: it means a
    /// field was cleared but its neighbor was not saved yet.
    pub fn custom_tls(&self) -> Option<(&std::path::Path, &std::path::Path)> {
        match (&self.tls_cert_path, &self.tls_key_path) {
            (Some(cert), Some(key)) => Some((cert.as_path(), key.as_path())),
            _ => None,
        }
    }
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

    #[test]
    fn a_config_saved_before_the_port_and_tls_fields_existed_still_loads() {
        // What `WebConfig` serialized to before this change: the port and
        // TLS fields are simply absent, the way a real config saved by an
        // older Stellarshot would be, not merely `null`.
        let old = r#"{
            "scope": "Lan",
            "password_enabled": true,
            "token_enabled": false,
            "pam_enabled": false,
            "token_hash": null,
            "allowed_addresses": []
        }"#;
        let config: WebConfig = serde_json::from_str(old).unwrap();
        assert_eq!(config.scope, NetworkScope::Lan, "the old settings survive");
        assert!(config.password_enabled);
        assert_eq!(
            config.port, DEFAULT_WEB_PORT,
            "a missing port defaults rather than failing the whole section"
        );
        assert_eq!(config.tls_cert_path, None);
        assert_eq!(config.tls_key_path, None);
    }

    #[test]
    fn custom_tls_needs_both_a_certificate_and_a_key() {
        let mut config = WebConfig::default();
        assert_eq!(config.custom_tls(), None);
        config.tls_cert_path = Some(PathBuf::from("/etc/stellarshot/cert.pem"));
        assert_eq!(config.custom_tls(), None, "a key alone is not enough");
        config.tls_key_path = Some(PathBuf::from("/etc/stellarshot/key.pem"));
        assert_eq!(
            config.custom_tls(),
            Some((
                std::path::Path::new("/etc/stellarshot/cert.pem"),
                std::path::Path::new("/etc/stellarshot/key.pem")
            ))
        );
    }
}
