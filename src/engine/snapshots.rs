// SPDX-License-Identifier: GPL-3.0-only

//! Listing and deleting snapshots.

use rustic_core::repofile::SnapshotFile;
use serde::{Deserialize, Serialize};

use super::error::EngineError;
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

/// What the UI needs to know about one snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    /// Full snapshot ID, in hex.
    pub id: String,
    /// When the snapshot was taken, in Unix seconds.
    pub time: i64,
    /// The source paths it covers.
    pub paths: Vec<String>,
    pub hostname: String,
    /// Files that were new in this snapshot.
    pub files_new: u64,
    /// Files that changed since the parent snapshot.
    pub files_changed: u64,
    /// Files unchanged since the parent snapshot.
    pub files_unmodified: u64,
    /// Bytes of new data this snapshot added to the repository, before
    /// compression.
    pub data_added: u64,
    /// Total size of the files the snapshot covers.
    pub total_bytes: u64,
}

impl SnapshotSummary {
    /// The first eight hex characters, as restic and rustic show IDs.
    pub fn short_id(&self) -> &str {
        self.id.get(..8).unwrap_or(&self.id)
    }
}

impl From<&SnapshotFile> for SnapshotSummary {
    fn from(snap: &SnapshotFile) -> Self {
        let summary = snap.summary.as_ref();
        Self {
            id: snap.id.to_string(),
            time: snap.time.timestamp().as_second(),
            paths: snap.paths.iter().cloned().collect(),
            hostname: snap.hostname.clone(),
            files_new: summary.map_or(0, |s| s.files_new),
            files_changed: summary.map_or(0, |s| s.files_changed),
            files_unmodified: summary.map_or(0, |s| s.files_unmodified),
            data_added: summary.map_or(0, |s| s.data_added),
            total_bytes: summary.map_or(0, |s| s.total_bytes_processed),
        }
    }
}

impl Repo {
    /// Every snapshot, newest first.
    pub fn snapshots(&self) -> Result<Vec<SnapshotSummary>, EngineError> {
        let mut snapshots: Vec<SnapshotSummary> = self
            .inner
            .get_all_snapshots()?
            .iter()
            .map(SnapshotSummary::from)
            .collect();
        snapshots.sort_by(|a, b| b.time.cmp(&a.time).then_with(|| a.id.cmp(&b.id)));
        debug_log!(ENGINE, "{} snapshots", snapshots.len());
        Ok(snapshots)
    }

    /// Delete snapshots by ID (full or unambiguous prefix). The data they
    /// alone referenced stays until the repository is pruned.
    pub fn delete_snapshots(&self, ids: &[String]) -> Result<(), EngineError> {
        let snapshots = self.inner.get_snapshots(ids)?;
        let ids: Vec<_> = snapshots.iter().map(|snap| snap.id).collect();
        debug_log!(ENGINE, "deleting {} snapshots", ids.len());
        self.inner.delete_snapshots(&ids)?;
        Ok(())
    }
}
