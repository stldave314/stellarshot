// SPDX-License-Identifier: GPL-3.0-only

//! Listing and deleting snapshots.

use rustic_core::RewriteOptions;
use rustic_core::repofile::{DeleteOption, SnapshotFile, SnapshotModification};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::{debug_log, error_log};

/// What the UI needs to know about one snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotSummary {
    /// Full snapshot ID, in hex.
    pub id: String,
    /// When the snapshot was taken, in Unix seconds.
    pub time: i64,
    /// The nanoseconds within that second: what orders two snapshots taken
    /// in the same one. Zero from a version that did not send it.
    #[serde(default)]
    pub time_subsec_ns: u32,
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
    /// Kept forever: "Keep" never forgets it, however old it gets.
    pub pinned: bool,
}

impl SnapshotSummary {
    /// The first eight hex characters, as restic and rustic show IDs.
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }
}

/// The first [`SHORT_ID_LEN`] hex characters of a snapshot ID, as restic
/// and rustic show IDs. A free function, not only [`SnapshotSummary::short_id`],
/// for a caller (`event_log`) that only ever has the plain ID string on
/// hand, never a whole `SnapshotSummary`.
pub const SHORT_ID_LEN: usize = 8;

pub fn short_id(id: &str) -> &str {
    id.get(..SHORT_ID_LEN).unwrap_or(id)
}

/// Newest first, to the nanosecond; the ID settles a true tie, so the order
/// is the same every time.
pub(crate) fn newest_first(a: &SnapshotSummary, b: &SnapshotSummary) -> std::cmp::Ordering {
    (b.time, b.time_subsec_ns)
        .cmp(&(a.time, a.time_subsec_ns))
        .then_with(|| a.id.cmp(&b.id))
}

impl From<&SnapshotFile> for SnapshotSummary {
    fn from(snap: &SnapshotFile) -> Self {
        let summary = snap.summary.as_ref();
        Self {
            id: snap.id.to_string(),
            time: snap.time.timestamp().as_second(),
            time_subsec_ns: u32::try_from(snap.time.timestamp().subsec_nanosecond())
                .unwrap_or_default(),
            paths: snap.paths.iter().cloned().collect(),
            hostname: snap.hostname.clone(),
            files_new: summary.map_or(0, |s| s.files_new),
            files_changed: summary.map_or(0, |s| s.files_changed),
            files_unmodified: summary.map_or(0, |s| s.files_unmodified),
            data_added: summary.map_or(0, |s| s.data_added),
            total_bytes: summary.map_or(0, |s| s.total_bytes_processed),
            pinned: snap.delete == DeleteOption::Never,
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
        snapshots.sort_by(newest_first);
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

    /// Pin a snapshot so [`Repo::forget`] never removes it, or release it
    /// back to the usual retention rules. Returns the snapshot under its new
    /// ID.
    ///
    /// A snapshot's ID is a hash of its own content, so changing anything
    /// about it, even just this flag, makes a new one: the old snapshot is
    /// saved under a new ID and the original is then removed, the same
    /// two-step rustic itself uses to rewrite a snapshot's metadata.
    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<SnapshotSummary, EngineError> {
        let snapshots = self.inner.get_snapshots(&[id])?;
        let Some(current) = snapshots.first().cloned() else {
            return Err(EngineError::new(
                ErrorKind::Internal,
                "no snapshot with that ID",
            ));
        };
        let wanted = if pinned {
            DeleteOption::Never
        } else {
            DeleteOption::NotSet
        };
        if current.delete == wanted {
            return Ok(SnapshotSummary::from(&current));
        }
        let before: Vec<_> = self
            .inner
            .get_all_snapshots()?
            .into_iter()
            .map(|s| s.id)
            .collect();
        let modification = if pinned {
            SnapshotModification::default().set_delete_never(true)
        } else {
            SnapshotModification::default().remove_delete(true)
        };
        let opts = RewriteOptions::default()
            .modification(modification)
            .forget(true);
        self.inner.rewrite_snapshots(snapshots, &opts)?;
        // `rewrite_snapshots` returns the rewritten snapshots under their old
        // ID, not the new one a changed snapshot is actually saved under (its
        // own `save_snapshots` call mutates a clone, so the caller never sees
        // the new ID either): the new one is found by comparing the snapshot
        // list before and after. A concurrent writer changing the repository
        // at the same moment could make that diff ambiguous or empty; rather
        // than guess and risk reporting the wrong snapshot as newly pinned,
        // this treats anything but exactly one new ID as a failure to report
        // back, even though the pin itself already took effect.
        let after = self.inner.get_all_snapshots()?;
        let mut added = after.iter().filter(|s| !before.contains(&s.id));
        let snapshot = match (added.next(), added.next()) {
            (Some(snapshot), None) => snapshot,
            (None, _) => {
                error_log!(
                    ENGINE,
                    "{id} pinned: {pinned}, but no new snapshot id appeared to identify it by"
                );
                return Err(EngineError::new(
                    ErrorKind::Internal,
                    "the pin was saved, but its updated snapshot could not be identified",
                ));
            }
            (Some(_), Some(_)) => {
                error_log!(
                    ENGINE,
                    "{id} pinned: {pinned}, but more than one new snapshot id appeared \
                     (a concurrent writer?); refusing to guess which one"
                );
                return Err(EngineError::new(
                    ErrorKind::Internal,
                    "the pin was saved, but its updated snapshot could not be identified",
                ));
            }
        };
        debug_log!(ENGINE, "{id} pinned: {pinned}");
        Ok(SnapshotSummary::from(snapshot))
    }
}
