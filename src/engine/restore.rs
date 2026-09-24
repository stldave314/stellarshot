// SPDX-License-Identifier: GPL-3.0-only

//! Restoring from a snapshot.
//!
//! rustic restores the stream of `(path, node)` pairs it is given into a
//! destination, rewriting files that differ and leaving identical ones alone.
//! What to do about a file that already exists is decided here, by shaping
//! that stream before rustic sees it: **Skip** leaves existing files out,
//! **Keep both** renames the restored copy, **Overwrite** passes them
//! through. rustic's option to delete files that are not in the snapshot is
//! never used.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustic_core::repofile::{Node, SnapshotFile};
use rustic_core::{IndexedFullStatus, LocalDestination, LsOptions, Repository, RestoreOptions};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::progress::ProgressSink;
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

/// Where restored files go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// Back where they were when backed up.
    Original,
    /// Into this folder, each selected item under its own name.
    Folder(PathBuf),
}

/// What to do when a file being restored already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictPolicy {
    /// Replace it with the backed-up copy.
    Overwrite,
    /// Leave it, and restore the backed-up copy next to it under a new name.
    KeepBoth,
    /// Leave it, and do not restore that file.
    Skip,
}

/// What to restore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// A snapshot ID, a unique prefix, or `latest`.
    pub snapshot: String,
    /// Absolute paths as they were backed up: files or folders.
    pub paths: Vec<PathBuf>,
    pub target: Target,
    pub policy: ConflictPolicy,
}

/// What a restore will do, or did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestorePreview {
    /// Files that will be written.
    pub files: u64,
    /// Bytes that will be written.
    pub bytes: u64,
    /// Files already present and identical, left untouched.
    pub unchanged: u64,
    /// Existing files that differ: replaced, kept alongside, or skipped,
    /// depending on the policy.
    pub conflicts: u64,
}

/// `name.ext` → `name (restored 2026-09-23).ext`, or with a counter if that
/// exists too.
fn keep_both_name(path: &Path, date: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let extension = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let parent = path.parent().unwrap_or(Path::new(""));
    let mut candidate = parent.join(format!("{stem} (restored {date}){extension}"));
    let mut counter = 2;
    while candidate.symlink_metadata().is_ok() {
        candidate = parent.join(format!("{stem} (restored {date} {counter}){extension}"));
        counter += 1;
    }
    candidate
}

/// Whether what is at `path` already matches `node`. Files compare by size
/// and modification time, the same quick check rustic uses before deciding a
/// file needs restoring. Symlinks compare by where they point, and are never
/// followed: a link to a file that changed is still the same link.
fn looks_identical(path: &Path, node: &Node) -> bool {
    let Ok(meta) = path.symlink_metadata() else {
        return false;
    };
    if node.is_symlink() {
        return meta.file_type().is_symlink()
            && std::fs::read_link(path).is_ok_and(|target| target == node.node_type.to_link());
    }
    if !meta.is_file() || !node.is_file() {
        return false;
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64);
    meta.len() == node.meta.size && modified == node.meta.mtime.map(|time| time.as_second())
}

impl Repo {
    /// Work out what restoring `request` would do, without writing anything.
    pub fn preview_restore(self, request: &RestoreRequest) -> Result<RestorePreview, EngineError> {
        run(self, request, None)
    }

    /// Restore `request`, reporting progress.
    pub fn restore(
        self,
        request: &RestoreRequest,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<RestorePreview, EngineError> {
        run(self, request, Some(progress))
    }

    /// Restore every file in `snapshot` (an ID, a unique prefix, or `latest`)
    /// into `destination`, keeping their absolute paths below it.
    pub fn restore_all(
        self,
        snapshot: &str,
        destination: &Path,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<(), EngineError> {
        let request = RestoreRequest {
            snapshot: snapshot.to_owned(),
            paths: vec![PathBuf::from("/")],
            target: Target::Folder(destination.to_path_buf()),
            policy: ConflictPolicy::Overwrite,
        };
        run(self, &request, Some(progress)).map(drop)
    }
}

fn run(
    repo: Repo,
    request: &RestoreRequest,
    progress: Option<Arc<dyn ProgressSink>>,
) -> Result<RestorePreview, EngineError> {
    let dry_run = progress.is_none();
    let _reporting = progress.map(|sink| repo.report_to(sink));
    let repo = repo.inner.to_indexed()?;
    let snapshot = repo.get_snapshot_from_str(&request.snapshot, |_| true)?;
    let date = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();

    let mut total = RestorePreview::default();
    for path in &request.paths {
        let preview = restore_one(&repo, &snapshot, path, request, &date, dry_run)?;
        total.files += preview.files;
        total.bytes += preview.bytes;
        total.unchanged += preview.unchanged;
        total.conflicts += preview.conflicts;
    }
    debug_log!(
        ENGINE,
        "restore {} (dry run: {dry_run}): {total:?}",
        request.snapshot
    );
    Ok(total)
}

fn restore_one(
    repo: &Repository<IndexedFullStatus>,
    snapshot: &SnapshotFile,
    path: &Path,
    request: &RestoreRequest,
    date: &str,
    dry_run: bool,
) -> Result<RestorePreview, EngineError> {
    let node = repo.node_from_snapshot_and_path(snapshot, &path.to_string_lossy())?;
    let mut destination = match &request.target {
        Target::Original => path.to_path_buf(),
        // Restoring the whole snapshot ("/") into a folder keeps the full
        // paths below it; anything else lands under its own name.
        Target::Folder(folder) => match path.file_name() {
            Some(name) => folder.join(name),
            None => folder.clone(),
        },
    };

    let mut conflicts = 0;
    let mut unchanged_by_us = 0;
    // A single file restored over a different existing one: the whole
    // destination moves aside or is skipped.
    if !node.is_dir()
        && destination.symlink_metadata().is_ok()
        && !looks_identical(&destination, &node)
    {
        match request.policy {
            ConflictPolicy::Overwrite => conflicts += 1,
            ConflictPolicy::KeepBoth => {
                conflicts += 1;
                destination = keep_both_name(&destination, date);
            }
            ConflictPolicy::Skip => {
                return Ok(RestorePreview {
                    conflicts: 1,
                    ..RestorePreview::default()
                });
            }
        }
    }

    let items: Vec<(PathBuf, Node)> = repo
        .ls(&node, &LsOptions::default())?
        .collect::<Result<_, _>>()?;
    let mut shaped = Vec::with_capacity(items.len());
    for (relative, item) in items {
        let on_disk = if relative.as_os_str().is_empty() {
            destination.clone()
        } else {
            destination.join(&relative)
        };
        if item.is_dir() || relative.as_os_str().is_empty() || on_disk.symlink_metadata().is_err() {
            shaped.push((relative, item));
            continue;
        }
        if looks_identical(&on_disk, &item) {
            unchanged_by_us += u64::from(request.policy == ConflictPolicy::Skip);
            if request.policy != ConflictPolicy::Skip {
                shaped.push((relative, item));
            }
            continue;
        }
        conflicts += 1;
        match request.policy {
            ConflictPolicy::Overwrite => shaped.push((relative, item)),
            ConflictPolicy::KeepBoth => {
                let renamed = keep_both_name(&on_disk, date);
                let relative = renamed
                    .strip_prefix(&destination)
                    .map(Path::to_path_buf)
                    .unwrap_or(relative);
                shaped.push((relative, item));
            }
            ConflictPolicy::Skip => {}
        }
    }

    let destination_text = destination.to_str().ok_or_else(|| {
        EngineError::new(
            ErrorKind::Io,
            format!("{} is not valid UTF-8", destination.display()),
        )
    })?;
    // A dry run must not create anything, so its destination is not created.
    let dest = LocalDestination::new(destination_text, !dry_run, !node.is_dir())?;
    let options = RestoreOptions::default();
    let plan = repo.prepare_restore(&options, shaped.iter().cloned().map(Ok), &dest, dry_run)?;
    let files = &plan.stats.files;
    let preview = RestorePreview {
        files: files.restore + files.modify,
        bytes: plan.restore_size,
        unchanged: files.unchanged + files.verified + unchanged_by_us,
        conflicts,
    };
    if !dry_run {
        repo.restore(plan, &options, shaped.into_iter().map(Ok), &dest)?;
    }
    Ok(preview)
}
