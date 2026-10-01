// SPDX-License-Identifier: GPL-3.0-only

//! Stellarshot's settings, stored through `cosmic-config`.

use std::path::PathBuf;

use cosmic::{
    cosmic_config::{
        self, Config, ConfigGet, CosmicConfigEntry, cosmic_config_derive::CosmicConfigEntry,
    },
    theme,
};
use serde::{Deserialize, Serialize};

use crate::constants::APP_ID;
use crate::debug::CONFIG;
use crate::profile::Profile;
use crate::{debug_log, error_log};

pub use crate::constants::CONFIG_VERSION;

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
}

impl StellarshotConfig {
    pub fn config_handler() -> Option<Config> {
        Config::new(APP_ID, CONFIG_VERSION).ok()
    }

    pub fn config() -> StellarshotConfig {
        Self::load().0
    }

    /// Loads the config exactly like [`Self::config`], plus whether the
    /// `profiles` key specifically could not be read at all — as opposed
    /// to genuinely not existing yet (a fresh install, or a config saved
    /// before backups existed), which is not a failure.
    ///
    /// Checked with its own direct read rather than folded into
    /// `get_entry`'s aggregated errors: those carry no field name for a
    /// value this binary cannot parse (only for the file read itself, via
    /// `Error::GetKey`), so there would be no way to tell "profiles is
    /// unreadable" apart from, say, "the cache setting has a stray value" —
    /// and only the first of those is worth refusing to build on. One
    /// unreadable profile entry (a downgrade, or a variant a newer
    /// Stellarshot wrote that this one does not know) must not be allowed
    /// to look identical to "no backups yet": see `App::profiles_read_only`
    /// for what a `true` here changes.
    pub fn load() -> (StellarshotConfig, bool) {
        let Some(config_handler) = Self::config_handler() else {
            return (StellarshotConfig::default(), false);
        };
        let config =
            StellarshotConfig::get_entry(&config_handler).unwrap_or_else(|(errs, config)| {
                debug_log!(CONFIG, "errors loading config: {errs:?}");
                config
            });
        crate::engine::cache_settings::set(config.cache_dir.clone(), config.no_cache);
        (config, profiles_unreadable(&config_handler))
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|profile| profile.id == id)
    }
}

/// Whether the `profiles` key specifically failed to load: see
/// [`StellarshotConfig::load`]'s own doc comment for why this is checked
/// on its own rather than folded into `get_entry`'s aggregated errors.
///
/// Two errors mean "not there", not "unreadable". `NotFound` is the
/// obvious one. `NoConfigDirectory` is the less obvious one: when the
/// user's own file is absent, `ConfigGet::get` falls through to the
/// system-wide default under `/usr/share`, and a `Config` with no such
/// directory at all (every install that ships no system default, which is
/// this one) reports *that* as `NoConfigDirectory` — so it is what a fresh
/// install with no backups yet actually gets. `Config::new` itself never
/// hands out a handler without a user directory, so nothing else here can
/// produce it. Every other error is a file that exists and could not be
/// read or parsed.
pub(crate) fn profiles_unreadable(handler: &Config) -> bool {
    match handler.get::<Vec<Profile>>("profiles") {
        Ok(_) | Err(cosmic_config::Error::NotFound | cosmic_config::Error::NoConfigDirectory) => {
            false
        }
        Err(err) => {
            error_log!(CONFIG, "the profiles list could not be read: {err}");
            true
        }
    }
}

/// The `profiles` key's file on disk, mirroring `cosmic_config::Config`'s
/// own directory layout (`$XDG_CONFIG_HOME/cosmic/<name>/v<version>/<key>`)
/// since `Config` itself exposes no accessor for the path it resolved to.
/// Used only to make a safety copy before anything is ever saved over an
/// unreadable `profiles` file — see `App::profiles_read_only`.
pub fn profiles_key_path() -> Option<std::path::PathBuf> {
    Some(
        crate::paths::config_root()?
            .join("cosmic")
            .join(APP_ID)
            .join(format!("v{CONFIG_VERSION}"))
            .join("profiles"),
    )
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum AppTheme {
    Dark,
    Light,
    #[default]
    System,
}

impl AppTheme {
    /// Where each is in the Settings list, in order: the match-the-desktop
    /// choice first.
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    /// This theme's position in [`AppTheme::ALL`].
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|&t| t == self).unwrap_or(0)
    }

    /// The theme at `index` in [`AppTheme::ALL`], or the desktop's own if it
    /// is out of range.
    pub fn from_index(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    pub fn theme(&self) -> theme::Theme {
        match self {
            Self::Dark => theme::Theme::dark(),
            Self::Light => theme::Theme::light(),
            Self::System => theme::system_preference(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_round_trips_through_its_list_position() {
        for theme in AppTheme::ALL {
            assert_eq!(AppTheme::from_index(theme.index()), theme);
        }
        assert_eq!(AppTheme::System.index(), 0, "matching the desktop is first");
        assert_eq!(AppTheme::from_index(99), AppTheme::System);
    }
    use cosmic_config::ConfigSet;

    /// A real `cosmic_config::Config` under a private temporary directory,
    /// laid out exactly as the live one is, plus the path of its
    /// `profiles` key file for a test to write to directly.
    fn private_config() -> (tempfile::TempDir, Config, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let handler = Config::with_custom_path(APP_ID, CONFIG_VERSION, dir.path().to_owned())
            .expect("a config under a fresh temporary directory");
        let profiles_file = dir
            .path()
            .join("cosmic")
            .join(APP_ID)
            .join(format!("v{CONFIG_VERSION}"))
            .join("profiles");
        (dir, handler, profiles_file)
    }

    #[test]
    fn a_profiles_key_that_does_not_exist_yet_is_not_unreadable() {
        // A fresh install, or a config from before any backup was set up:
        // no file at all is "no backups", not a failure to read them.
        let (_dir, handler, profiles_file) = private_config();
        assert!(!profiles_file.exists());

        assert!(!profiles_unreadable(&handler));
    }

    #[test]
    fn a_profiles_key_this_binary_can_parse_is_not_unreadable() {
        let (_dir, handler, _) = private_config();
        let profile = Profile::new(
            "Home".into(),
            crate::profile::Destination::Local {
                path: "/backup".into(),
            },
            vec!["/home/alex".into()],
        );
        handler.set("profiles", vec![profile]).unwrap();

        assert!(!profiles_unreadable(&handler));
    }

    #[test]
    fn a_profiles_key_this_binary_cannot_parse_is_unreadable() {
        // What a newer Stellarshot would leave behind after adding a
        // `Destination` variant this one does not know, or what a
        // downgrade finds: a file that exists and is well-formed RON, but
        // does not deserialize as this binary's `Vec<Profile>`.
        let (_dir, handler, profiles_file) = private_config();
        std::fs::write(
            &profiles_file,
            r#"[(id: "abc", name: "Home", destination: FromTheFuture(x: 1))]"#,
        )
        .unwrap();

        assert!(profiles_unreadable(&handler));
    }

    /// The hazard the probe exists for, proven rather than assumed: with
    /// exactly the unreadable file above, the derived `get_entry` hands
    /// back an *empty* profile list — indistinguishable, to any caller
    /// that only looks at the config, from a machine with no backups.
    /// That is what `timers::reconcile` would then remove every timer
    /// against, and what the next save would write over the file. If
    /// cosmic-config ever changed to fail loudly instead, this test is
    /// what says the extra probe is no longer load-bearing.
    #[test]
    fn get_entry_silently_turns_an_unreadable_profiles_file_into_no_profiles() {
        let (_dir, handler, profiles_file) = private_config();
        std::fs::write(
            &profiles_file,
            r#"[(id: "abc", name: "Home", destination: FromTheFuture(x: 1))]"#,
        )
        .unwrap();

        let (errors, config) = StellarshotConfig::get_entry(&handler)
            .expect_err("a file that does not parse must at least be reported");
        assert!(!errors.is_empty());
        assert!(
            config.profiles.is_empty(),
            "the default it falls back to is an empty list, which is the whole problem"
        );
        assert!(
            errors
                .iter()
                .all(|err| !matches!(err, cosmic_config::Error::GetKey(..))),
            "a parse failure carries no key name (only a read failure does), which is why \
             `load` probes the profiles key on its own instead of matching on these: {errors:?}"
        );
    }
}
