// SPDX-License-Identifier: GPL-3.0-only

//! What happened when each backup last ran: facts, not settings.
//!
//! They are kept in cosmic-config's *state* store, one key per profile,
//! rather than in the profile list. A scheduled run and the window both
//! write them, and neither ever rewrites the other's settings: had they
//! lived in the profiles, a scheduled run finishing while the user edited a
//! backup could have saved the old profile list over the edit.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use serde::{Deserialize, Serialize};

use crate::app::APP_ID;
use crate::app::config::CONFIG_VERSION;
use crate::debug::CONFIG;
use crate::debug_log;
use crate::engine::{EngineError, ErrorKind};

/// Which part of a scheduled run failed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    #[default]
    Backup,
    /// Forgetting old snapshots or pruning, after the backup succeeded.
    Cleanup,
    /// The integrity check, after the backup succeeded.
    Check,
}

/// A run that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Failure {
    /// Unix seconds.
    pub time: i64,
    #[serde(default)]
    pub stage: Stage,
    pub kind: ErrorKind,
    pub detail: String,
}

impl Failure {
    pub fn error(&self) -> EngineError {
        EngineError::new(self.kind, self.detail.clone())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunState {
    /// When a scheduled backup last finished, in Unix seconds.
    #[serde(default)]
    pub last_success: Option<i64>,
    /// When an integrity check last ran to the end, damaged or not.
    #[serde(default)]
    pub last_check: Option<i64>,
    /// The last scheduled run that failed. It stays until a later run
    /// succeeds, and is shown while it is newer than the last success.
    #[serde(default)]
    pub failure: Option<Failure>,
    /// The last check found damage. Automatic pruning waits until a check
    /// passes, because pruning a damaged repository can lose more.
    #[serde(default)]
    pub damaged: bool,
}

impl RunState {
    /// The failure to show, if it happened after the last success. `other`
    /// is a success recorded elsewhere, such as a backup from the window.
    pub fn current_failure(&self, other: Option<i64>) -> Option<&Failure> {
        let last_success = self.last_success.max(other);
        self.failure
            .as_ref()
            .filter(|failure| last_success.is_none_or(|success| failure.time > success))
    }
}

fn store() -> Option<Config> {
    Config::new_state(APP_ID, CONFIG_VERSION)
        .inspect_err(|err| debug_log!(CONFIG, "no state store: {err}"))
        .ok()
}

fn key(profile_id: &str) -> String {
    format!("run-{profile_id}")
}

pub fn load(profile_id: &str) -> RunState {
    store()
        .and_then(|store| store.get(&key(profile_id)).ok())
        .unwrap_or_default()
}

pub fn save(profile_id: &str, state: &RunState) -> Result<(), String> {
    let store = store().ok_or("no state directory")?;
    store
        .set(&key(profile_id), state)
        .map_err(|err| err.to_string())
}

/// Change one profile's state in place.
pub fn update(profile_id: &str, change: impl FnOnce(&mut RunState)) -> Result<(), String> {
    let mut state = load(profile_id);
    change(&mut state);
    save(profile_id, &state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(time: i64) -> Option<Failure> {
        Some(Failure {
            time,
            stage: Stage::Backup,
            kind: ErrorKind::WrongPassword,
            detail: String::new(),
        })
    }

    #[test]
    fn a_failure_shows_until_a_later_success() {
        let mut state = RunState {
            last_success: Some(100),
            failure: failure(200),
            ..RunState::default()
        };
        assert!(state.current_failure(None).is_some());
        assert!(
            state.current_failure(Some(300)).is_none(),
            "a later backup from the window clears it"
        );
        state.last_success = Some(300);
        assert!(state.current_failure(None).is_none());
    }

    #[test]
    fn a_failure_with_no_success_ever_shows() {
        let state = RunState {
            failure: failure(5),
            ..RunState::default()
        };
        assert!(state.current_failure(None).is_some());
    }
}
