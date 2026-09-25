// SPDX-License-Identifier: GPL-3.0-only

//! How much of a repository's storage is actually used.
//!
//! Two of rustic's own reports, read together: `infos_files` lists every
//! file at the destination (what a cloud bill would actually charge for),
//! and `infos_index` gives the index's own picture of what those files hold
//! (how much is your data, once compressed, and how much of it nothing
//! refers to any more).
//!
//! `infos_index` breaks its totals down by blob type (file data versus the
//! folder trees that describe it), but rustic_core does not make the
//! `BlobType` those totals are keyed by part of its public API, so the two
//! cannot be told apart here. Both are counted together instead: trees are a
//! small fraction of a real backup, so the combined ratio is close to the
//! one for file data alone, without depending on a type rustic does not
//! expose.

use serde::{Deserialize, Serialize};

use super::error::EngineError;
use super::repo::Repo;

/// How much space a repository actually uses, and how.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statistics {
    /// Every file at the destination, of every kind (packs, indexes,
    /// snapshots, keys, the config): the real size in storage.
    pub stored_bytes: u64,
    /// Your files' size before compression, counting each unique piece of
    /// data once, however many files or snapshots hold it.
    pub original_bytes: u64,
    /// The stored size of everything still referenced, after compression:
    /// the denominator [`Self::compression_ratio`] divides by. Smaller than
    /// `stored_bytes`, which also counts data nothing refers to any more.
    pub packed_bytes: u64,
    /// Bytes a clean-up would free: data no remaining snapshot needs any
    /// more, forgotten but not yet reclaimed.
    pub reclaimable_bytes: u64,
}

impl Statistics {
    /// How much smaller compression and deduplication together make your
    /// data: 3.0 means it takes a third of its original size. `None` before
    /// anything has been backed up.
    pub fn compression_ratio(&self) -> Option<f64> {
        (self.packed_bytes > 0).then(|| self.original_bytes as f64 / self.packed_bytes as f64)
    }
}

impl Repo {
    /// Read the repository's statistics. Reads every index file and lists
    /// every file at the destination, so it is worth caching rather than
    /// calling often, especially on a slow connection.
    pub fn statistics(&self) -> Result<Statistics, EngineError> {
        let stored_bytes = self
            .inner
            .infos_files()?
            .repo
            .iter()
            .map(|file| file.size)
            .sum();
        let index = self.inner.infos_index()?;
        let original_bytes = index.blobs.iter().map(|blob| blob.data_size).sum();
        let packed_bytes = index.blobs.iter().map(|blob| blob.size).sum();
        let reclaimable_bytes = index.blobs_delete.iter().map(|blob| blob.size).sum();
        Ok(Statistics {
            stored_bytes,
            original_bytes,
            packed_bytes,
            reclaimable_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::engine::{self, BackupRequest, Location, NoProgress, Secret};
    use tempfile::TempDir;

    #[test]
    fn statistics_match_a_real_backup() {
        let dir = TempDir::new().unwrap();
        // Compressible and already-repeated, so the ratio is clearly not 1.
        let source = dir.path().join("source");
        std::fs::create_dir_all(&source).unwrap();
        let text = "the quick brown fox jumps over the lazy dog\n".repeat(20_000);
        std::fs::write(source.join("a.txt"), &text).unwrap();
        std::fs::write(source.join("b.txt"), &text).unwrap();
        let location = Location::local(dir.path().join("repo"));
        let secret = Secret::new("pw");
        engine::init(&location, &secret).unwrap();
        engine::open(&location, &secret)
            .unwrap()
            .backup(
                &BackupRequest {
                    sources: vec![source],
                    ..BackupRequest::default()
                },
                Arc::new(NoProgress),
            )
            .unwrap();

        let stats = engine::open(&location, &secret)
            .unwrap()
            .statistics()
            .unwrap();

        assert!(stats.stored_bytes > 0);
        assert!(
            stats.original_bytes >= (text.len() as u64),
            "two copies of the file, deduplicated to at least one"
        );
        assert!(
            stats.original_bytes < 2 * text.len() as u64,
            "identical files must be deduplicated, not stored twice"
        );
        let ratio = stats.compression_ratio().expect("data was backed up");
        assert!(
            ratio > 1.5,
            "highly repetitive text should compress well, got {ratio}"
        );
        assert_eq!(stats.reclaimable_bytes, 0, "nothing has been forgotten yet");
    }

    #[test]
    fn an_empty_repository_has_no_ratio() {
        let dir = TempDir::new().unwrap();
        let location = Location::local(dir.path().join("repo"));
        let secret = Secret::new("pw");
        engine::init(&location, &secret).unwrap();

        let stats = engine::open(&location, &secret)
            .unwrap()
            .statistics()
            .unwrap();

        assert_eq!(stats.compression_ratio(), None);
        assert!(stats.stored_bytes > 0, "the config and key files exist");
    }
}
