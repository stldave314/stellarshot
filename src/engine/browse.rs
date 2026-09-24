// SPDX-License-Identifier: GPL-3.0-only

//! Looking inside snapshots: folders, search, versions of a file, what
//! changed between two snapshots, and what has been deleted since.
//!
//! A [`Browser`] keeps one repository open with its tree index loaded for as
//! long as the restore page is open, so moving between folders does not
//! re-read the index each time.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rustic_core::repofile::SnapshotFile;
use rustic_core::{IndexedIdsStatus, LsOptions, Repository, TreeId};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::repo::Repo;
use super::snapshots::SnapshotSummary;

/// What kind of thing an entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
    Other,
}

/// One entry in a snapshot's folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeEntry {
    pub name: String,
    /// The absolute path the entry had when it was backed up.
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    /// Last modified, in Unix seconds.
    pub modified: Option<i64>,
}

/// One version of a file: its state in one snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileVersion {
    pub snapshot: SnapshotSummary,
    pub size: u64,
    pub modified: Option<i64>,
    /// The content is identical to the next newer version listed.
    pub same_as_newer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Change {
    Added,
    Removed,
    Modified,
}

/// One difference between two snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffEntry {
    pub path: PathBuf,
    pub change: Change,
    /// A whole folder was added or removed; its contents are not listed.
    pub is_dir: bool,
    pub size: u64,
}

/// A file that is in a backup but no longer on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingEntry {
    pub path: PathBuf,
    pub size: u64,
    /// The newest snapshot that still has it: the one to restore from.
    pub last_seen: SnapshotSummary,
}

/// An open repository for looking around in.
pub struct Browser {
    repo: Mutex<Repository<IndexedIdsStatus>>,
    /// Newest first.
    snapshots: Vec<(SnapshotFile, SnapshotSummary)>,
}

impl std::fmt::Debug for Browser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Browser")
            .field("snapshots", &self.snapshots.len())
            .finish()
    }
}

fn kind_of(node: &rustic_core::repofile::Node) -> EntryKind {
    if node.is_dir() {
        EntryKind::Directory
    } else if node.is_file() {
        EntryKind::File
    } else if node.is_symlink() {
        EntryKind::Symlink
    } else {
        EntryKind::Other
    }
}

fn entry(path: PathBuf, node: &rustic_core::repofile::Node) -> TreeEntry {
    TreeEntry {
        name: node.name().to_string_lossy().into_owned(),
        path,
        kind: kind_of(node),
        size: node.meta.size,
        modified: node.meta.mtime.map(|time| time.as_second()),
    }
}

fn not_found(what: &str) -> EngineError {
    EngineError::new(
        ErrorKind::Internal,
        format!("{what} is not in this snapshot"),
    )
}

impl Repo {
    /// Load the tree index and every snapshot, for browsing.
    pub fn browse(self) -> Result<Browser, EngineError> {
        let mut snapshots: Vec<(SnapshotFile, SnapshotSummary)> = self
            .inner
            .get_all_snapshots()?
            .into_iter()
            .map(|snapshot| {
                let summary = SnapshotSummary::from(&snapshot);
                (snapshot, summary)
            })
            .collect();
        snapshots.sort_by(|a, b| b.1.time.cmp(&a.1.time).then_with(|| a.1.id.cmp(&b.1.id)));
        let repo = self.inner.to_indexed_ids()?;
        Ok(Browser {
            repo: Mutex::new(repo),
            snapshots,
        })
    }
}

impl Browser {
    pub fn snapshots(&self) -> Vec<SnapshotSummary> {
        self.snapshots
            .iter()
            .map(|(_, summary)| summary.clone())
            .collect()
    }

    fn repo(&self) -> Result<std::sync::MutexGuard<'_, Repository<IndexedIdsStatus>>, EngineError> {
        self.repo
            .lock()
            .map_err(|_| EngineError::new(ErrorKind::Internal, "browser lock poisoned"))
    }

    /// A snapshot by full ID, unique prefix, or `latest`.
    fn snapshot(&self, id: &str) -> Result<&(SnapshotFile, SnapshotSummary), EngineError> {
        if id == "latest" {
            return self.snapshots.first().ok_or_else(|| not_found("latest"));
        }
        let mut matches = self.snapshots.iter().filter(|(_, s)| s.id.starts_with(id));
        match (matches.next(), matches.next()) {
            (Some(found), None) => Ok(found),
            _ => Err(not_found(id)),
        }
    }

    /// The entries of `dir` in `snapshot`: folders first, then by name.
    pub fn list(&self, snapshot: &str, dir: &Path) -> Result<Vec<TreeEntry>, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = repo.node_from_snapshot_and_path(file, &dir.to_string_lossy())?;
        let subtree = node
            .subtree
            .ok_or_else(|| not_found(&dir.display().to_string()))?;
        let tree = repo.get_tree(&subtree)?;
        let mut entries: Vec<TreeEntry> = tree
            .nodes
            .iter()
            .map(|node| entry(dir.join(node.name()), node))
            .collect();
        entries.sort_by(|a, b| {
            (b.kind == EntryKind::Directory)
                .cmp(&(a.kind == EntryKind::Directory))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(entries)
    }

    /// Every entry in `snapshot` whose name contains `query` (ignoring case),
    /// up to `limit`.
    pub fn search(
        &self,
        snapshot: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<TreeEntry>, EngineError> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let root = repo.node_from_snapshot_and_path(file, "/")?;
        let mut found = Vec::new();
        for item in repo.ls(&root, &LsOptions::default())? {
            let (path, node) = item?;
            if node
                .name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&query)
            {
                found.push(entry(Path::new("/").join(path), &node));
                if found.len() >= limit {
                    break;
                }
            }
        }
        Ok(found)
    }

    /// `path` in every snapshot that has it, newest first.
    pub fn versions(&self, path: &Path) -> Result<Vec<FileVersion>, EngineError> {
        let repo = self.repo()?;
        let mut versions: Vec<FileVersion> = Vec::new();
        let mut newer_content = None;
        for (file, summary) in &self.snapshots {
            let Ok(node) = repo.node_from_snapshot_and_path(file, &path.to_string_lossy()) else {
                newer_content = None;
                continue;
            };
            let content = (node.content.clone(), node.meta.size);
            let same_as_newer = newer_content.as_ref() == Some(&content);
            versions.push(FileVersion {
                snapshot: summary.clone(),
                size: node.meta.size,
                modified: node.meta.mtime.map(|time| time.as_second()),
                same_as_newer,
            });
            newer_content = Some(content);
        }
        Ok(versions)
    }

    /// What changed from snapshot `from` to snapshot `to`.
    ///
    /// Folders whose tree is identical in both are skipped without being
    /// read, so comparing two snapshots of a mostly unchanged home folder
    /// only reads the folders that changed.
    pub fn diff(&self, from: &str, to: &str) -> Result<Vec<DiffEntry>, EngineError> {
        let (a, _) = self.snapshot(from)?;
        let (b, _) = self.snapshot(to)?;
        let repo = self.repo()?;
        let mut out = Vec::new();
        diff_trees(&repo, a.tree, b.tree, Path::new("/"), &mut out)?;
        Ok(out)
    }

    /// Files under `scope` that are in a snapshot taken at or after `since`
    /// (Unix seconds) but no longer exist on disk, each with the newest
    /// snapshot that has it. At most `limit` are returned.
    pub fn missing(
        &self,
        scope: &Path,
        since: i64,
        limit: usize,
    ) -> Result<Vec<MissingEntry>, EngineError> {
        let repo = self.repo()?;
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut missing = Vec::new();
        for (file, summary) in self.snapshots.iter().filter(|(_, s)| s.time >= since) {
            let Ok(node) = repo.node_from_snapshot_and_path(file, &scope.to_string_lossy()) else {
                continue;
            };
            if !node.is_dir() {
                continue;
            }
            for item in repo.ls(&node, &LsOptions::default())? {
                let (relative, node) = item?;
                let path = scope.join(relative);
                if !node.is_file() || !seen.insert(path.clone()) {
                    continue;
                }
                if path.symlink_metadata().is_err() {
                    missing.push(MissingEntry {
                        path,
                        size: node.meta.size,
                        last_seen: summary.clone(),
                    });
                    if missing.len() >= limit {
                        return Ok(missing);
                    }
                }
            }
        }
        Ok(missing)
    }
}

fn diff_trees(
    repo: &Repository<IndexedIdsStatus>,
    a: TreeId,
    b: TreeId,
    prefix: &Path,
    out: &mut Vec<DiffEntry>,
) -> Result<(), EngineError> {
    if a == b {
        return Ok(());
    }
    let by_name =
        |id: TreeId| -> Result<BTreeMap<String, rustic_core::repofile::Node>, EngineError> {
            Ok(repo
                .get_tree(&id)?
                .nodes
                .into_iter()
                .map(|node| (node.name().to_string_lossy().into_owned(), node))
                .collect())
        };
    let old = by_name(a)?;
    let new = by_name(b)?;
    let names: std::collections::BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    for name in names {
        let path = prefix.join(name);
        match (old.get(name), new.get(name)) {
            (Some(before), None) => out.push(DiffEntry {
                path,
                change: Change::Removed,
                is_dir: before.is_dir(),
                size: before.meta.size,
            }),
            (None, Some(after)) => out.push(DiffEntry {
                path,
                change: Change::Added,
                is_dir: after.is_dir(),
                size: after.meta.size,
            }),
            (Some(before), Some(after)) => match (before.subtree, after.subtree) {
                (Some(x), Some(y)) => diff_trees(repo, x, y, &path, out)?,
                _ if before.is_dir() != after.is_dir() => {
                    out.push(DiffEntry {
                        path: path.clone(),
                        change: Change::Removed,
                        is_dir: before.is_dir(),
                        size: before.meta.size,
                    });
                    out.push(DiffEntry {
                        path,
                        change: Change::Added,
                        is_dir: after.is_dir(),
                        size: after.meta.size,
                    });
                }
                _ if before.content != after.content
                    || before.meta.size != after.meta.size
                    || before.node_type != after.node_type =>
                {
                    out.push(DiffEntry {
                        path,
                        change: Change::Modified,
                        is_dir: false,
                        size: after.meta.size,
                    });
                }
                _ => {}
            },
            (None, None) => {}
        }
    }
    Ok(())
}
