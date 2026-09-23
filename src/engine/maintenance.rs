// SPDX-License-Identifier: GPL-3.0-only

//! Repository integrity checks.

use rustic_core::CheckOptions;

use super::error::{EngineError, ErrorKind};
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

impl Repo {
    /// Verify the repository's structure: that every snapshot, tree and index
    /// entry is present and consistent. Pack contents are not re-read.
    pub fn check(&self) -> Result<(), EngineError> {
        let results = self.inner.check(CheckOptions::default())?;
        debug_log!(ENGINE, "check found {} findings", results.0.len());
        results
            .is_ok()
            .map_err(|err| EngineError::new(ErrorKind::RepositoryDamaged, err.to_string()))
    }
}
