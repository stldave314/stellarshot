// SPDX-License-Identifier: GPL-3.0-only

//! Taking a snapshot.

use std::path::PathBuf;
use std::sync::Arc;

use rustic_core::{BackupOptions, PathList, SnapshotOptions};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::progress::ProgressSink;
use super::repo::Repo;
use super::snapshots::SnapshotSummary;
use crate::debug::ENGINE;
use crate::debug_log;

/// What to back up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRequest {
    /// Folders and files to include.
    pub sources: Vec<PathBuf>,
    /// Folders and files to leave out, even when inside a source.
    pub excludes: Vec<PathBuf>,
    /// Glob patterns to leave out wherever they match, such as `*.tmp` or
    /// `node_modules`.
    pub exclude_patterns: Vec<String>,
    /// Do not descend into other mounted filesystems.
    pub one_file_system: bool,
}

impl BackupRequest {
    /// The exclusions as rustic override globs. A leading `!` makes a glob an
    /// exclusion; a glob without one would instead restrict the backup to
    /// matching paths only.
    ///
    /// Excluded paths are canonicalised, because the backup canonicalises its
    /// sources: where `/home` is a symlink to `/var/home`, the walk sees
    /// `/var/home/dave/.cache`, and an exclude written as `/home/dave/.cache`
    /// would silently never match.
    pub(crate) fn globs(&self) -> Vec<String> {
        self.excludes
            .iter()
            .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
            .map(|path| format!("!{}", path.display()))
            .chain(
                self.exclude_patterns
                    .iter()
                    .map(|pattern| format!("!{pattern}")),
            )
            .collect()
    }

    /// The sources as the backup walks them: canonical, with nested paths
    /// merged into the folder that contains them.
    pub(crate) fn path_list(&self) -> Result<PathList, EngineError> {
        self.sources
            .iter()
            .cloned()
            .collect::<PathList>()
            .sanitize()
            .map_err(|err| EngineError::new(ErrorKind::Io, err.to_string()))
    }

    pub(crate) fn options(&self) -> BackupOptions {
        let mut options = BackupOptions::default();
        options.excludes.globs = self.globs();
        options.ignore_filter_opts.one_file_system = self.one_file_system;
        options
    }
}

/// The outcome of a backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupReport {
    pub snapshot: SnapshotSummary,
}

impl Repo {
    /// Back up `request.sources` into a new snapshot.
    pub fn backup(
        self,
        request: &BackupRequest,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<BackupReport, EngineError> {
        if request.sources.is_empty() {
            return Err(EngineError::new(ErrorKind::Internal, "nothing to back up"));
        }
        let _reporting = self.report_to(progress);
        let sources = request.path_list()?;
        debug_log!(ENGINE, "backup of {} sources", sources.len());

        let repo = self.inner.to_indexed_ids()?;
        let snapshot = SnapshotOptions::default().to_snapshot()?;
        let snapshot = repo.backup(&request.options(), &sources, snapshot)?;
        debug_log!(ENGINE, "created snapshot {}", snapshot.id);
        Ok(BackupReport {
            snapshot: SnapshotSummary::from(&snapshot),
        })
    }
}
