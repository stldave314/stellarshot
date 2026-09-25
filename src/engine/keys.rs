// SPDX-License-Identifier: GPL-3.0-only

//! A repository's keys: each is the same master key, wrapped by a
//! different password, so several people or machines can each have their
//! own.

use rustic_core::KeyOptions;
use rustic_core::repofile::KeyId;
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::repo::{Repo, Secret, open};
use crate::debug::ENGINE;
use crate::debug_log;

/// One password that can open the repository.
///
/// Not included: who added it, or when. A key file carries `hostname`,
/// `username` and `created` fields for that, but reading one back needs a
/// raw, undecrypted read of a `Key`-type file, and the only read rustic_core
/// exposes publicly (`cat_file`, used for every other file type) always
/// decrypts with the repository's master key — the wrong key entirely for a
/// key file, which is protected by the password instead. Worth raising with
/// the rustic project itself, per the rule at the top of this page; not
/// attempted here. **Caught by a test that actually opened a second key**,
/// not assumed: the first version used `cat_file` anyway, and every test
/// that added a key then opened the repository again failed with a garbled
/// decryption error instead of finding it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeySummary {
    /// Full key ID, in hex.
    pub id: String,
    /// This is the key the repository was opened with.
    pub current: bool,
}

impl Repo {
    /// Every key that can open this repository.
    pub fn keys(&self) -> Result<Vec<KeySummary>, EngineError> {
        let current = self.inner.key_id().map(KeyId::into_inner);
        let mut ids: Vec<KeyId> = self.inner.list::<KeyId>()?.collect();
        ids.sort_by_key(|id| id.into_inner().to_hex().to_string());
        let keys: Vec<KeySummary> = ids
            .into_iter()
            .map(|id| KeySummary {
                current: current == Some(id.into_inner()),
                id: id.into_inner().to_hex().to_string(),
            })
            .collect();
        debug_log!(ENGINE, "{} keys", keys.len());
        Ok(keys)
    }

    /// Add a new key with `password`, for another person or machine, or as
    /// the second key of a recovery sheet. Returns its full ID.
    pub fn add_key(&self, password: &str) -> Result<String, EngineError> {
        let id = self.inner.add_key(password, &KeyOptions::default())?;
        let hex = id.into_inner().to_hex().to_string();
        debug_log!(ENGINE, "added key {hex}");
        Ok(hex)
    }

    /// Remove a key by its full ID. Refuses to remove the key this
    /// repository was opened with, the same as rustic itself: losing every
    /// key that can open a repository is unrecoverable, so the one in use
    /// right now must always survive removing any other.
    pub fn delete_key(&self, id: &str) -> Result<(), EngineError> {
        let key_id: KeyId = id
            .parse()
            .map_err(|_| EngineError::new(ErrorKind::Internal, format!("not a key ID: {id}")))?;
        self.inner.delete_key(&key_id)?;
        debug_log!(ENGINE, "deleted key {id}");
        Ok(())
    }

    /// Change the password this repository opens with: adds a new key for
    /// `new_password`, then removes the key that was used to open it.
    /// Any other keys (another person's, or a recovery sheet's) are left
    /// alone.
    ///
    /// The removal needs a second, fresh handle opened with the new
    /// password: `key_id()` reports whichever key a `Repository` was
    /// *opened* with, fixed for that handle's whole lifetime, and rustic
    /// itself always refuses to delete that one — a safeguard against ending
    /// up with no key at all. Adding the new key first doesn't change what
    /// `self` was opened with, so deleting the old key through `self` trips
    /// that same guard. **Caught by a test**, not assumed: it opens the
    /// repository again with the new password afterwards and expects to
    /// find only one key.
    pub fn change_password(&self, new_password: &str) -> Result<(), EngineError> {
        let previous = *self.inner.key_id();
        self.inner.add_key(new_password, &KeyOptions::default())?;
        if let Some(previous) = previous {
            let reopened = open(&self.location, &Secret::new(new_password))?;
            reopened.inner.delete_key(&previous)?;
        }
        debug_log!(ENGINE, "changed the repository's password");
        Ok(())
    }
}
