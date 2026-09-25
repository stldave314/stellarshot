// SPDX-License-Identifier: GPL-3.0-only

//! Taking a snapshot.

use std::path::PathBuf;
use std::sync::Arc;

use bytesize::ByteSize;
use rustic_core::{BackupOptions, PathList, SnapshotOptions};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::progress::ProgressSink;
use super::repo::Repo;
use super::snapshots::SnapshotSummary;
use crate::debug::ENGINE;
use crate::debug_log;

/// The name `CACHEDIR.TAG` inside a folder marks it as disposable cache data
/// per the [Cache Directory Tagging
/// Specification](https://bford.info/cachedir/); excluding folders that carry
/// it is `rustic_core`'s `exclude_if_present`.
const CACHEDIR_TAG: &str = "CACHEDIR.TAG";

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
    /// Same as `exclude_patterns`, but case-insensitive.
    #[serde(default)]
    pub exclude_patterns_ignoring_case: Vec<String>,
    /// Files with glob patterns to exclude, one per line, in the style of a
    /// `.gitignore`.
    #[serde(default)]
    pub exclude_pattern_files: Vec<PathBuf>,
    /// Leave out files larger than this many bytes.
    #[serde(default)]
    pub exclude_larger_than: Option<u64>,
    /// Leave out any folder containing a `CACHEDIR.TAG` file.
    #[serde(default)]
    pub exclude_caches: bool,
    /// Honor each project's own `.gitignore`.
    #[serde(default)]
    pub git_ignore: bool,
    /// Do not descend into other mounted filesystems.
    pub one_file_system: bool,
    /// Write no snapshot when nothing changed since the last one.
    #[serde(default)]
    pub skip_if_unchanged: bool,
    /// Estimate the result without writing any data or a snapshot.
    #[serde(default)]
    pub dry_run: bool,
    /// When the snapshot says it was taken, in Unix seconds; now if unset.
    /// Only the tests and the demo repository set it, to build a history.
    #[serde(default)]
    pub time: Option<i64>,
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
    pub(crate) fn globs(&self) -> Result<Vec<String>, EngineError> {
        Ok(self
            .excludes
            .iter()
            .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
            .map(|path| format!("!{}", path.display()))
            .chain(
                self.exclude_patterns
                    .iter()
                    .map(|pattern| format!("!{pattern}")),
            )
            .chain(
                self.pattern_file_lines()?
                    .into_iter()
                    .map(|line| format!("!{line}")),
            )
            .collect())
    }

    /// Same as `globs`, but for `exclude_patterns_ignoring_case`: matched
    /// through a separate, case-insensitive override.
    pub(crate) fn iglobs(&self) -> Vec<String> {
        self.exclude_patterns_ignoring_case
            .iter()
            .map(|pattern| format!("!{pattern}"))
            .collect()
    }

    /// Every non-blank, non-comment line from `exclude_pattern_files`.
    ///
    /// Read here rather than left to rustic's own `glob_files`, whose lines
    /// are exclusions only with a leading `!` rustic never adds — passing
    /// the files straight through would silently restrict the backup to
    /// them instead of leaving them out.
    ///
    /// Fails rather than skipping a file that cannot be read: a moved,
    /// deleted, or briefly unreachable exclude file would otherwise make a
    /// backup silently include whatever it was meant to leave out, possibly
    /// something private, with nothing to show for it.
    fn pattern_file_lines(&self) -> Result<Vec<String>, EngineError> {
        let mut lines = Vec::new();
        for path in &self.exclude_pattern_files {
            let text = std::fs::read_to_string(path).map_err(|err| {
                EngineError::new(ErrorKind::Io, format!("{}: {err}", path.display()))
            })?;
            lines.extend(
                text.lines()
                    .map(str::to_owned)
                    .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#')),
            );
        }
        Ok(lines)
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

    pub(crate) fn options(&self) -> Result<BackupOptions, EngineError> {
        let mut options = BackupOptions::default();
        options.excludes.globs = self.globs()?;
        options.excludes.iglobs = self.iglobs();
        options.ignore_filter_opts.one_file_system = self.one_file_system;
        options.ignore_filter_opts.exclude_larger_than = self.exclude_larger_than.map(ByteSize::b);
        options.ignore_filter_opts.git_ignore = self.git_ignore;
        // Applies a project's `.gitignore` whether or not the project is
        // itself a git repository with a `.git` folder: rustic's default
        // (matching the `ignore` crate ripgrep uses) is to require one,
        // which would silently do nothing for most backed-up folders.
        options.ignore_filter_opts.no_require_git = self.git_ignore;
        if self.exclude_caches {
            options.ignore_filter_opts.exclude_if_present = vec![CACHEDIR_TAG.to_string()];
        }
        options.parent_opts.skip_if_unchanged = self.skip_if_unchanged;
        options.dry_run = self.dry_run;
        Ok(options)
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
        let mut options = SnapshotOptions::default();
        if let Some(time) = request.time {
            let time = jiff::Timestamp::from_second(time)
                .map_err(|err| EngineError::new(ErrorKind::Internal, err.to_string()))?;
            options.time = Some(time.to_zoned(jiff::tz::TimeZone::system()));
        }
        let snapshot = options.to_snapshot()?;
        let snapshot = repo.backup(&request.options()?, &sources, snapshot)?;
        debug_log!(ENGINE, "created snapshot {}", snapshot.id);
        Ok(BackupReport {
            snapshot: SnapshotSummary::from(&snapshot),
        })
    }
}
