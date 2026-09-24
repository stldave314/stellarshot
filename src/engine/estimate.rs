// SPDX-License-Identifier: GPL-3.0-only

//! How much a backup will cover, before it runs.
//!
//! Déjà Dup shows the size of each included folder but does not subtract
//! excluded folders that sit inside them, so its estimate is simply wrong for
//! the common case of backing up `~` without `~/.cache`. This estimate walks
//! the very same filtered file list a backup reads — rustic's own
//! `LocalSource`, built from the same options — so exclusions, patterns and
//! filesystem boundaries are applied exactly as the backup will apply them.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rustic_core::{LocalSource, LocalSourceSaveOptions, ReadSource};
use serde::{Deserialize, Serialize};

use super::backup::BackupRequest;
use super::error::{EngineError, ErrorKind};
use crate::constants::PROGRESS_INTERVAL;

/// Files and bytes a backup would read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SizeEstimate {
    pub files: u64,
    pub bytes: u64,
    /// Bytes under each of the request's sources, in the same order, after
    /// exclusions. A file inside two overlapping sources counts towards both
    /// rows but only once in `bytes`.
    pub per_source: Vec<u64>,
}

/// Walk what `request` would back up.
///
/// `progress` receives running totals every [`PROGRESS_INTERVAL`]. Returns
/// `Ok(None)` if `cancel` was set before the walk finished. Files are counted
/// the way the backup counts them: a hard-linked file once per name (its
/// contents are stored once either way), overlapping sources once, unreadable
/// entries not at all.
pub fn estimate(
    request: &BackupRequest,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(SizeEstimate),
) -> Result<Option<SizeEstimate>, EngineError> {
    // Missing sources are skipped rather than failing the whole estimate, so
    // the wizard can show a total while the user is still editing the list.
    let existing = BackupRequest {
        sources: request
            .sources
            .iter()
            .filter(|path| path.exists())
            .cloned()
            .collect(),
        ..request.clone()
    };
    let mut total = SizeEstimate {
        per_source: vec![0; request.sources.len()],
        ..SizeEstimate::default()
    };
    if existing.sources.is_empty() {
        return Ok(Some(total));
    }
    // Entries arrive under canonical paths; compare against canonical sources.
    let roots: Vec<PathBuf> = request
        .sources
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect();
    let sources = existing.path_list()?.paths();

    let options = existing.options();
    let source = LocalSource::new(
        LocalSourceSaveOptions::default(),
        &options.excludes,
        &options.ignore_filter_opts,
        &sources,
    )
    .map_err(|err| EngineError::new(ErrorKind::Io, err.to_string()))?;

    let mut last_report = Instant::now();
    for entry in source.entries() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let Ok(entry) = entry else {
            continue;
        };
        let node = &entry.node;
        if !node.is_file() {
            continue;
        }
        let size = node.meta.size;
        total.files += 1;
        total.bytes += size;
        for (row, root) in total.per_source.iter_mut().zip(&roots) {
            if entry.path.starts_with(root) {
                *row += size;
            }
        }
        if last_report.elapsed() >= PROGRESS_INTERVAL {
            progress(total.clone());
            last_report = Instant::now();
        }
    }
    Ok(Some(total))
}

/// The size of everything under `path`, with no filtering: what excluding it
/// takes out. `None` if cancelled or unreadable.
pub fn folder_size(path: &Path, cancel: &AtomicBool) -> Option<u64> {
    let request = BackupRequest {
        sources: vec![path.to_path_buf()],
        ..BackupRequest::default()
    };
    estimate(&request, cancel, &mut |_| {})
        .ok()
        .flatten()
        .map(|total| total.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Location, NoProgress, Secret};
    use std::fs;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![7u8; bytes]).unwrap();
    }

    /// Includes with an exclude nested inside, an overlapping include, a
    /// pattern, and a hard link.
    fn tricky_request(root: &Path) -> BackupRequest {
        let home = root.join("home");
        write(&home.join("docs/report.odt"), 10_000);
        write(&home.join("docs/notes.txt"), 300);
        write(&home.join(".cache/big.bin"), 500_000);
        write(&home.join("project/node_modules/lib.js"), 40_000);
        write(&home.join("project/main.rs"), 2_000);
        fs::hard_link(home.join("docs/report.odt"), home.join("report-link.odt")).unwrap();
        BackupRequest {
            // `docs` is inside `home`: an overlapping include.
            sources: vec![home.clone(), home.join("docs")],
            excludes: vec![home.join(".cache")],
            exclude_patterns: vec!["node_modules".into()],
            one_file_system: true,
            time: None,
        }
    }

    #[test]
    fn estimate_matches_the_backup() {
        let dir = TempDir::new().unwrap();
        let request = tricky_request(dir.path());

        let estimated = estimate(&request, &AtomicBool::new(false), &mut |_| {})
            .unwrap()
            .unwrap();

        // The cache and node_modules are out; the report counts once per
        // name (it is hard-linked), and `docs` once despite being included
        // twice.
        assert_eq!(estimated.bytes, 10_000 * 2 + 300 + 2_000);
        assert_eq!(estimated.files, 4);
        // Per row: `home` holds everything counted; `docs` just its two files.
        assert_eq!(estimated.per_source, vec![estimated.bytes, 10_000 + 300]);

        let location = Location::local(dir.path().join("repo"));
        let secret = Secret::new("pw");
        engine::init(&location, &secret).unwrap();
        let report = engine::open(&location, &secret)
            .unwrap()
            .backup(&request, Arc::new(NoProgress))
            .unwrap();
        // The regression Déjà Dup has: the estimate must be exactly what the
        // backup processes, not "includes minus nothing".
        assert_eq!(report.snapshot.total_bytes, estimated.bytes);
    }

    #[test]
    fn estimate_respects_cancel() {
        let dir = TempDir::new().unwrap();
        let request = tricky_request(dir.path());

        let result = estimate(&request, &AtomicBool::new(true), &mut |_| {}).unwrap();

        assert_eq!(result, None);
    }

    #[test]
    fn folder_size_counts_everything_inside() {
        let dir = TempDir::new().unwrap();
        write(&dir.path().join("a/b/c.bin"), 1234);
        write(&dir.path().join("a/d.bin"), 66);

        assert_eq!(
            folder_size(&dir.path().join("a"), &AtomicBool::new(false)),
            Some(1300)
        );
    }

    #[test]
    fn missing_sources_estimate_to_nothing() {
        let request = BackupRequest {
            sources: vec!["/nonexistent-stellarshot-source".into()],
            ..BackupRequest::default()
        };
        let result = estimate(&request, &AtomicBool::new(false), &mut |_| {}).unwrap();
        assert_eq!(result.map(|total| total.bytes), Some(0));
    }
}
