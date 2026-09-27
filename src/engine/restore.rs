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

use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use rustic_core::repofile::{Node, SnapshotFile};
use rustic_core::{IndexedFullStatus, LocalDestination, LsOptions, Repository, RestoreOptions};
use serde::{Deserialize, Serialize};

use super::browse::node_at;
use super::error::{EngineError, ErrorKind};
use super::progress::ProgressSink;
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

/// Where restored files go.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    /// Back where they were when backed up.
    #[default]
    Original,
    /// Into this folder, each selected item under its own name.
    Folder(PathBuf),
}

/// What to do when a file being restored already exists.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictPolicy {
    /// Replace it with the backed-up copy.
    #[default]
    Overwrite,
    /// Leave it, and restore the backed-up copy next to it under a new name.
    KeepBoth,
    /// Leave it, and do not restore that file.
    Skip,
}

/// Whose user and group IDs a restored file gets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Ownership {
    /// The backed-up user and group names, as on the original machine.
    #[default]
    Preserve,
    /// The backed-up numeric IDs, unresolved: for restoring onto another
    /// machine or user where the names would not mean the same accounts.
    Numeric,
    /// Whatever restoring the files themselves leaves them with; no attempt
    /// to set an owner or group at all.
    None,
}

/// What to restore.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// A snapshot ID, a unique prefix, or `latest`.
    pub snapshot: String,
    /// Absolute paths as they were backed up: files or folders.
    pub paths: Vec<PathBuf>,
    pub target: Target,
    pub policy: ConflictPolicy,
    /// Read and verify a file that already looks unchanged by its size and
    /// modification time, rather than trusting them.
    #[serde(default)]
    pub verify_existing: bool,
    #[serde(default)]
    pub ownership: Ownership,
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
/// exists too. Built with `OsString`, not `to_string_lossy`, so a stem or
/// extension that is not valid UTF-8 keeps its exact bytes rather than
/// having them replaced.
fn keep_both_name(path: &Path, date: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_os_string();
    let extension = path.extension();
    let parent = path.parent().unwrap_or(Path::new(""));
    let name = |counter: Option<u32>| -> std::ffi::OsString {
        let mut name = stem.clone();
        name.push(" (restored ");
        name.push(date);
        if let Some(counter) = counter {
            name.push(format!(" {counter}"));
        }
        name.push(")");
        if let Some(extension) = extension {
            name.push(".");
            name.push(extension);
        }
        name
    };
    let mut candidate = parent.join(name(None));
    let mut counter = 2;
    while candidate.symlink_metadata().is_ok() {
        candidate = parent.join(name(Some(counter)));
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
            ..RestoreRequest::default()
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

/// The nearest ancestor of `path` that is valid UTF-8 (always `/` at
/// worst, which always is), and `path` made relative to it. Used only to
/// satisfy `LocalDestination::new`'s `&str` root; every path below that
/// root moves as an `OsString`-preserving `PathBuf`, untouched.
fn utf8_root_and_relative(path: &Path) -> (PathBuf, PathBuf) {
    let mut root = path;
    while root.to_str().is_none() {
        match root.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => root = parent,
            _ => break,
        }
    }
    let relative = path
        .strip_prefix(root)
        .unwrap_or(Path::new(""))
        .to_path_buf();
    (root.to_path_buf(), relative)
}

/// Rejects a snapshot item whose relative path (as `rustic_core`'s
/// `NodeStreamer` yields it, straight from the tree's own node names) would
/// land outside `destination` once joined onto it: a `..` component walks
/// back up, and an absolute path replaces the join outright (`PathBuf::join`
/// with an absolute right-hand side discards the left side). Neither should
/// occur — a well-formed snapshot's paths are always relative and
/// self-contained — but a snapshot's tree is deserialized from repository
/// data, which for a repository shared with another person or machine
/// (`keys.rs`) may not be trustworthy. An empty relative path (the node
/// being restored itself, not one of its descendants) has no components and
/// always passes.
fn reject_unsafe_relative_path(relative: &Path) -> Result<(), EngineError> {
    if relative
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        Ok(())
    } else {
        Err(EngineError::new(
            ErrorKind::UnsafePath,
            relative.display().to_string(),
        ))
    }
}

fn restore_one(
    repo: &Repository<IndexedFullStatus>,
    snapshot: &SnapshotFile,
    path: &Path,
    request: &RestoreRequest,
    date: &str,
    dry_run: bool,
) -> Result<RestorePreview, EngineError> {
    let node = node_at(repo, snapshot, path)?;
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
    for (relative, _) in &items {
        reject_unsafe_relative_path(relative)?;
    }
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

    // `LocalDestination::new` needs a UTF-8 `&str` root, but `destination`
    // itself may not be (a non-UTF-8 name being restored by itself, not as
    // part of a whole-snapshot restore, ends up as this call's own leaf).
    // Rooting at the nearest ancestor that *is* UTF-8 instead keeps the
    // actual restore working: only the root argument needs to be a `&str`,
    // not any path below it. Left alone (the common case: `destination`
    // was already UTF-8, so `root == destination` and `leaf` is empty),
    // this changes nothing.
    //
    // What each item's own relative path from `repo.ls` needs to become
    // depends on whether `destination` names a directory or a single
    // file, because rustic's `NodeStreamer` treats those differently: for
    // a directory it yields children/descendants relative to that
    // directory (never including the directory's own name), so `leaf`
    // (this directory's own name, relative to `root`) needs to be
    // prepended; for a single file it yields exactly one item whose own
    // relative path is just the node's original name — redundant even
    // when unchanged, and wrong after a Keep Both rename — so `leaf` (the
    // possibly-renamed final name) replaces it outright rather than being
    // prepended to it.
    let (root, leaf) = utf8_root_and_relative(&destination);
    let shaped: Vec<(PathBuf, Node)> = if root == destination {
        shaped
    } else if node.is_dir() {
        shaped
            .into_iter()
            .map(|(relative, item)| (leaf.join(&relative), item))
            .collect()
    } else {
        shaped
            .into_iter()
            .map(|(_, item)| (leaf.clone(), item))
            .collect()
    };
    let destination_text = root.to_str().ok_or_else(|| {
        EngineError::new(
            ErrorKind::Io,
            format!("{} is not valid UTF-8", root.display()),
        )
    })?;
    // True only when nothing had to move up to find a UTF-8 ancestor, so
    // the root is still exactly `destination` and the node's own kind
    // applies to it directly.
    let expect_file = root == destination && !node.is_dir();
    // A dry run must not create anything, so its destination is not created.
    let dest = LocalDestination::new(destination_text, !dry_run, expect_file)?;
    // `RestoreOptions` is `#[non_exhaustive]`: built from its own default and
    // then adjusted, not as a struct literal.
    let mut options = RestoreOptions::default();
    options.verify_existing = request.verify_existing;
    match request.ownership {
        Ownership::Preserve => {}
        Ownership::Numeric => options.numeric_id = true,
        Ownership::None => options.no_ownership = true,
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ordinary_relative_path_is_accepted() {
        assert!(reject_unsafe_relative_path(Path::new("docs/report.pdf")).is_ok());
    }

    #[test]
    fn the_empty_path_of_the_restored_item_itself_is_accepted() {
        assert!(reject_unsafe_relative_path(Path::new("")).is_ok());
    }

    #[test]
    fn a_parent_dir_component_is_rejected() {
        let err = reject_unsafe_relative_path(Path::new("../escape")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsafePath);
    }

    #[test]
    fn a_parent_dir_component_buried_partway_through_is_still_rejected() {
        let err = reject_unsafe_relative_path(Path::new("docs/../../escape")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsafePath);
    }

    #[test]
    fn an_absolute_path_is_rejected() {
        // `PathBuf::join` with an absolute right-hand side discards the
        // left side outright, so this would otherwise land at `/tmp/abs`
        // rather than under the chosen destination.
        let err = reject_unsafe_relative_path(Path::new("/tmp/abs")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsafePath);
    }
}
