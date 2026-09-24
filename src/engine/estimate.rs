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
    let roots = canonical(&request.sources);
    let mut total = SizeEstimate {
        per_source: vec![0; request.sources.len()],
        ..SizeEstimate::default()
    };
    let mut last_report = Instant::now();
    let finished = walk(request, cancel, &mut |path, size| {
        total.files += 1;
        total.bytes += size;
        for (row, root) in total.per_source.iter_mut().zip(&roots) {
            if path.starts_with(root) {
                *row += size;
            }
        }
        if last_report.elapsed() >= PROGRESS_INTERVAL {
            progress(total.clone());
            last_report = Instant::now();
        }
    })?;
    Ok(finished.then_some(total))
}

/// What a backup's exclusions take out, next to what its sources hold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExclusionBreakdown {
    /// Bytes under the sources with nothing excluded: only the rule about
    /// other filesystems, which is about where the sources end rather than
    /// what is left out, still applies. Overlapping sources count once.
    pub included: u64,
    /// The same, for each source in the request's order.
    pub per_source: Vec<u64>,
    /// Bytes under each of the folders asked about, in the order given.
    pub per_folder: Vec<u64>,
    /// Bytes under any of those folders, counted once where they nest.
    pub in_folders: u64,
}

impl ExclusionBreakdown {
    /// Everything the exclusions take out, given the backup's own `total`:
    /// `included − excluded = total`, exactly, because both walks follow
    /// the same rules apart from the exclusions.
    pub fn excluded(&self, total: u64) -> u64 {
        self.included.saturating_sub(total)
    }

    /// The part of [`Self::excluded`] the folders do not account for: what
    /// patterns remove outside them.
    pub fn by_patterns(&self, total: u64) -> u64 {
        self.excluded(total).saturating_sub(self.in_folders)
    }
}

/// Walk `request`'s sources without its exclusions, sizing each of
/// `folders` on the way. With [`estimate`]'s total, this gives the
/// arithmetic the setup shows: "45 GB − 12 GB excluded = 33 GB". `Ok(None)`
/// if cancelled.
pub fn exclusion_breakdown(
    request: &BackupRequest,
    folders: &[PathBuf],
    cancel: &AtomicBool,
) -> Result<Option<ExclusionBreakdown>, EngineError> {
    let everything = BackupRequest {
        excludes: Vec::new(),
        exclude_patterns: Vec::new(),
        ..request.clone()
    };
    let roots = canonical(&request.sources);
    let folders = canonical(folders);
    let mut breakdown = ExclusionBreakdown {
        per_source: vec![0; request.sources.len()],
        per_folder: vec![0; folders.len()],
        ..ExclusionBreakdown::default()
    };
    let finished = walk(&everything, cancel, &mut |path, size| {
        breakdown.included += size;
        for (row, root) in breakdown.per_source.iter_mut().zip(&roots) {
            if path.starts_with(root) {
                *row += size;
            }
        }
        let mut inside = false;
        for (row, folder) in breakdown.per_folder.iter_mut().zip(&folders) {
            if path.starts_with(folder) {
                *row += size;
                inside = true;
            }
        }
        if inside {
            breakdown.in_folders += size;
        }
    })?;
    Ok(finished.then_some(breakdown))
}

/// Paths as the walk reports them: canonical, so `/home` and `/var/home`
/// compare equal where one is a link to the other.
fn canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect()
}

/// Visit every file the backup of `request` would read, with its size, the
/// way the backup reads them: rustic's own `LocalSource`, built from the
/// same options. Returns `false` if cancelled.
fn walk(
    request: &BackupRequest,
    cancel: &AtomicBool,
    visit: &mut dyn FnMut(&Path, u64),
) -> Result<bool, EngineError> {
    // Missing sources are skipped rather than failing the whole walk, so the
    // wizard can show a total while the user is still editing the list.
    let existing = BackupRequest {
        sources: request
            .sources
            .iter()
            .filter(|path| path.exists())
            .cloned()
            .collect(),
        ..request.clone()
    };
    if existing.sources.is_empty() {
        return Ok(true);
    }
    let sources = existing.path_list()?.paths();
    let options = existing.options();
    let source = LocalSource::new(
        LocalSourceSaveOptions::default(),
        &options.excludes,
        &options.ignore_filter_opts,
        &sources,
    )
    .map_err(|err| EngineError::new(ErrorKind::Io, err.to_string()))?;

    for entry in source.entries() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let Ok(entry) = entry else {
            continue;
        };
        if entry.node.is_file() {
            visit(&entry.path, entry.node.meta.size);
        }
    }
    Ok(true)
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
    fn the_arithmetic_adds_up_to_the_estimate() {
        let dir = TempDir::new().unwrap();
        let request = tricky_request(dir.path());
        let cache = dir.path().join("home/.cache");
        let nested = cache.join("inner");
        write(&nested.join("more.bin"), 7_000);
        let never = AtomicBool::new(false);

        let total = estimate(&request, &never, &mut |_| {}).unwrap().unwrap();
        let breakdown = exclusion_breakdown(&request, &[cache, nested], &never)
            .unwrap()
            .unwrap();

        let everything = 10_000 * 2 + 300 + 500_000 + 7_000 + 40_000 + 2_000;
        assert_eq!(breakdown.included, everything);
        assert_eq!(
            breakdown.included - breakdown.excluded(total.bytes),
            total.bytes,
            "included − excluded = total"
        );
        assert_eq!(breakdown.per_folder, vec![507_000, 7_000]);
        assert_eq!(breakdown.in_folders, 507_000, "nested folders count once");
        assert_eq!(
            breakdown.by_patterns(total.bytes),
            40_000,
            "node_modules, which only the pattern takes out"
        );
        assert_eq!(breakdown.per_source, vec![everything, 10_000 + 300]);
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
