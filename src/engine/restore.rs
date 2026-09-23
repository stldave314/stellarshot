// SPDX-License-Identifier: GPL-3.0-only

//! Restoring a snapshot.
//!
//! This is the whole-snapshot restore the engine needs to prove backups are
//! readable. Selective restore with conflict handling builds on it.

use std::path::Path;
use std::sync::Arc;

use rustic_core::{LocalDestination, LsOptions, RestoreOptions};

use super::error::EngineError;
use super::progress::ProgressSink;
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

impl Repo {
    /// Restore every file in `snapshot` (an ID, a unique prefix, or `latest`)
    /// into `destination`, which is created if needed.
    pub fn restore_all(
        self,
        snapshot: &str,
        destination: &Path,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<(), EngineError> {
        let _reporting = self.report_to(progress);
        let repo = self.inner.to_indexed()?;
        let node = repo.node_from_snapshot_path(snapshot, |_| true)?;
        let listing = LsOptions::default();
        let nodes = repo.ls(&node, &listing)?;

        let destination = destination.to_str().ok_or_else(|| {
            EngineError::from(std::io::Error::other("destination is not valid UTF-8"))
        })?;
        let dest = LocalDestination::new(destination, true, !node.is_dir())?;
        let options = RestoreOptions::default();
        let plan = repo.prepare_restore(&options, nodes.clone(), &dest, false)?;
        debug_log!(ENGINE, "restoring {snapshot} to {destination}");
        repo.restore(plan, &options, nodes, &dest)?;
        Ok(())
    }
}
