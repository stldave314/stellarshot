// SPDX-License-Identifier: GPL-3.0-only

//! Listing snapshots, and changing or deleting them one at a time.

use std::collections::BTreeSet;

use rustic_core::repofile::{DeleteOption, SnapshotFile, SnapshotModification};
use rustic_core::{RewriteOptions, StringList};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::maintenance::is_internal_tag;
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::{debug_log, error_log};

/// What the UI needs to know about one snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Its name, notes and tags. Empty from a version that did not send
    /// them.
    #[serde(default)]
    pub details: SnapshotDetails,
}

/// What a person can say about a snapshot: a name (rustic's `label`),
/// notes (its `description`) and tags. Tags only ever lists the person's
/// own: the tag Stellarshot puts on every snapshot to tell which backup
/// made it is never shown, and never changed through this.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDetails {
    pub name: String,
    pub notes: String,
    /// Sorted, without duplicates.
    pub tags: Vec<String>,
}

impl SnapshotDetails {
    /// `name`, `notes` and comma-separated `tags` as typed, tidied the way
    /// they are saved: surrounding spaces trimmed, and tags split on commas
    /// with empty and repeated ones dropped. A tag Stellarshot reserves for
    /// itself is dropped too, since saving it would claim the snapshot for
    /// another backup's retention rules.
    pub fn from_input(name: &str, notes: &str, tags: &str) -> Self {
        let tags: BTreeSet<String> = tags
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty() && !is_internal_tag(tag))
            .map(ToOwned::to_owned)
            .collect();
        Self {
            name: name.trim().to_owned(),
            notes: notes.trim().to_owned(),
            tags: tags.into_iter().collect(),
        }
    }

    /// Whether `query` (already lowercase) appears in the name, the notes
    /// or any tag.
    pub fn matches(&self, query: &str) -> bool {
        self.name.to_lowercase().contains(query)
            || self.notes.to_lowercase().contains(query)
            || self
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().contains(query))
    }
}

impl SnapshotSummary {
    /// The first eight hex characters, as restic and rustic show IDs.
    pub fn short_id(&self) -> &str {
        short_id(&self.id)
    }

    /// Whether the snapshot recorded what it covered. One saved without
    /// that record (by an older restic, say) reads as all zeros, which is
    /// not the same as a snapshot in which nothing changed.
    pub fn has_file_counts(&self) -> bool {
        self.files_new > 0 || self.files_changed > 0 || self.files_unmodified > 0
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

/// The newest of `snapshots` (listed newest first) that is not dated further
/// ahead of `now` than [`FUTURE_SNAPSHOT_TOLERANCE_SECONDS`]: what "last
/// backed up" means. A snapshot taken once with the clock years ahead would
/// otherwise read as "backed up just now" for years, and silence the
/// overdue warning with it.
///
/// [`FUTURE_SNAPSHOT_TOLERANCE_SECONDS`]: crate::constants::FUTURE_SNAPSHOT_TOLERANCE_SECONDS
pub fn newest_settled(snapshots: &[SnapshotSummary], now: i64) -> Option<&SnapshotSummary> {
    let limit = now.saturating_add(crate::constants::FUTURE_SNAPSHOT_TOLERANCE_SECONDS);
    snapshots.iter().find(|snapshot| snapshot.time <= limit)
}

/// Newest first, to the nanosecond; the ID settles a true tie, so the order
/// is the same every time.
pub(crate) fn newest_first(a: &SnapshotSummary, b: &SnapshotSummary) -> std::cmp::Ordering {
    (b.time, b.time_subsec_ns)
        .cmp(&(a.time, a.time_subsec_ns))
        .then_with(|| a.id.cmp(&b.id))
}

/// A snapshot's own tags, without the ones Stellarshot keeps for itself.
fn own_tags(snap: &SnapshotFile) -> BTreeSet<String> {
    snap.tags
        .iter()
        .filter(|tag| !is_internal_tag(tag))
        .cloned()
        .collect()
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
            details: SnapshotDetails {
                name: snap.label.clone(),
                notes: snap.description.clone().unwrap_or_default(),
                tags: own_tags(snap).into_iter().collect(),
            },
        }
    }
}

/// One tag per list: built with `add` rather than parsed, since parsing
/// splits on commas.
fn tag_lists(tags: impl IntoIterator<Item = String>) -> Vec<StringList> {
    tags.into_iter()
        .map(|tag| {
            let mut list = StringList::default();
            list.add(tag);
            list
        })
        .collect()
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

    fn one_snapshot(&self, id: &str) -> Result<SnapshotFile, EngineError> {
        self.inner
            .get_snapshots(&[id])?
            .into_iter()
            .next()
            .ok_or_else(|| EngineError::new(ErrorKind::NotFound, id))
    }

    /// Pin a snapshot so [`Repo::forget`] never removes it, or release it
    /// back to the usual retention rules. Returns the snapshot under its new
    /// ID: see [`Self::modify_one`].
    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<SnapshotSummary, EngineError> {
        let current = self.one_snapshot(id)?;
        let wanted = if pinned {
            DeleteOption::Never
        } else {
            DeleteOption::NotSet
        };
        if current.delete == wanted {
            return Ok(SnapshotSummary::from(&current));
        }
        let modification = if pinned {
            SnapshotModification::default().set_delete_never(true)
        } else {
            SnapshotModification::default().remove_delete(true)
        };
        let summary = self.modify_one(current, modification)?;
        debug_log!(ENGINE, "{id} pinned: {pinned}");
        Ok(summary)
    }

    /// Give a snapshot a new name, notes and tags. Its other tags (the one
    /// that says which backup made it) are left as they are. Returns the
    /// snapshot under its new ID: see [`Self::modify_one`].
    pub fn set_details(
        &self,
        id: &str,
        details: &SnapshotDetails,
    ) -> Result<SnapshotSummary, EngineError> {
        let current = self.one_snapshot(id)?;
        let summary = SnapshotSummary::from(&current);
        if summary.details == *details {
            return Ok(summary);
        }
        // Never through `set_tags`, which would also drop the backup's own
        // tag and with it the snapshot's place under that backup's "Keep".
        let had = own_tags(&current);
        let wanted: BTreeSet<String> = details
            .tags
            .iter()
            .filter(|tag| !is_internal_tag(tag))
            .cloned()
            .collect();
        let modification = SnapshotModification::default()
            .set_label(details.name.clone())
            .add_tags(tag_lists(wanted.difference(&had).cloned()))
            .remove_tags(tag_lists(had.difference(&wanted).cloned()));
        let modification = if details.notes.is_empty() {
            modification.remove_description(true)
        } else {
            modification.set_description(details.notes.clone())
        };
        let summary = self.modify_one(current, modification)?;
        debug_log!(ENGINE, "{id} details changed");
        Ok(summary)
    }

    /// Apply `modification` to `snapshot`. A snapshot's ID is a hash of its
    /// own content, so changing anything about it makes a new one: the
    /// changed snapshot is saved under a new ID and the original is then
    /// removed, the same two-step rustic itself uses to rewrite a snapshot's
    /// metadata. Returns the snapshot under that new ID.
    fn modify_one(
        &self,
        snapshot: SnapshotFile,
        modification: SnapshotModification,
    ) -> Result<SnapshotSummary, EngineError> {
        let id = snapshot.id.to_string();
        let before: Vec<_> = self
            .inner
            .get_all_snapshots()?
            .into_iter()
            .map(|s| s.id)
            .collect();
        let opts = RewriteOptions::default()
            .modification(modification)
            .forget(true);
        self.inner.rewrite_snapshots(vec![snapshot], &opts)?;
        // `rewrite_snapshots` returns the rewritten snapshots under their old
        // ID, not the new one a changed snapshot is actually saved under (its
        // own `save_snapshots` call mutates a clone, so the caller never sees
        // the new ID either): the new one is found by comparing the snapshot
        // list before and after. A concurrent writer changing the repository
        // at the same moment could make that diff ambiguous or empty; rather
        // than guess and risk reporting the wrong snapshot as the changed
        // one, this treats anything but exactly one new ID as a failure to
        // report back, even though the change itself already took effect.
        let after = self.inner.get_all_snapshots()?;
        let mut added = after.iter().filter(|s| !before.contains(&s.id));
        match (added.next(), added.next()) {
            (Some(snapshot), None) => Ok(SnapshotSummary::from(snapshot)),
            (None, _) => {
                error_log!(
                    ENGINE,
                    "{id} changed, but no new snapshot id appeared to identify it by"
                );
                Err(EngineError::new(
                    ErrorKind::Internal,
                    "the change was saved, but its updated snapshot could not be identified",
                ))
            }
            (Some(_), Some(_)) => {
                error_log!(
                    ENGINE,
                    "{id} changed, but more than one new snapshot id appeared \
                     (a concurrent writer?); refusing to guess which one"
                );
                Err(EngineError::new(
                    ErrorKind::Internal,
                    "the change was saved, but its updated snapshot could not be identified",
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counted(new: u64, changed: u64, unmodified: u64) -> SnapshotSummary {
        SnapshotSummary {
            id: "0123456789abcdef".into(),
            time: 0,
            time_subsec_ns: 0,
            paths: Vec::new(),
            hostname: String::new(),
            files_new: new,
            files_changed: changed,
            files_unmodified: unmodified,
            data_added: 0,
            total_bytes: 0,
            pinned: false,
            details: SnapshotDetails::default(),
        }
    }

    #[test]
    fn the_newest_settled_snapshot_skips_one_dated_in_the_future() {
        let at = |time| SnapshotSummary {
            time,
            ..counted(0, 0, 0)
        };
        let now = 1_000_000;
        let snapshots = [at(4_070_908_800), at(now - 60), at(now - 86_400)];
        assert_eq!(
            newest_settled(&snapshots, now).map(|s| s.time),
            Some(now - 60)
        );
        let slightly_ahead = [at(now + 3_600)];
        assert_eq!(
            newest_settled(&slightly_ahead, now).map(|s| s.time),
            Some(now + 3_600),
            "a clock an hour off is not a mistake worth ignoring"
        );
        assert!(newest_settled(&[at(4_070_908_800)], now).is_none());
        assert!(newest_settled(&[], now).is_none());
    }

    #[test]
    fn a_snapshot_in_which_nothing_changed_still_has_file_counts() {
        assert!(counted(0, 0, 12).has_file_counts());
        assert!(counted(3, 0, 0).has_file_counts());
        assert!(counted(0, 2, 0).has_file_counts());
    }

    #[test]
    fn a_snapshot_saved_without_a_summary_has_none() {
        assert!(!counted(0, 0, 0).has_file_counts());
    }
}
