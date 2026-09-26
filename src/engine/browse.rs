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
use rustic_core::{IndexedFullStatus, LsOptions, Repository, TreeId};
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

/// One entry's metadata for mounting a snapshot as a filesystem
/// ([`crate::engine::mount`]): like [`TreeEntry`], but with what a real
/// filesystem needs and the tree browser does not — a symlink's target,
/// and the original Unix permission bits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<i64>,
    pub mode: Option<u32>,
    pub symlink_target: Option<PathBuf>,
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
    repo: Mutex<Repository<IndexedFullStatus>>,
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

fn mount_entry(node: &rustic_core::repofile::Node) -> MountEntry {
    MountEntry {
        name: node.name().to_string_lossy().into_owned(),
        kind: kind_of(node),
        size: node.meta.size,
        modified: node.meta.mtime.map(|time| time.as_second()),
        mode: node.meta.mode,
        symlink_target: node
            .is_symlink()
            .then(|| node.node_type.to_link().to_path_buf()),
    }
}

fn not_found(what: &str) -> EngineError {
    EngineError::new(
        ErrorKind::Internal,
        format!("{what} is not in this snapshot"),
    )
}

fn wrong_kind(path: &Path, expected: &str) -> EngineError {
    EngineError::new(
        ErrorKind::Internal,
        format!("{} is not a {expected} in this snapshot", path.display()),
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
        let repo = self.inner.to_indexed()?;
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

    fn repo(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Repository<IndexedFullStatus>>, EngineError> {
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

    /// `path`'s own metadata, for mounting the snapshot as a filesystem:
    /// unlike `list`, this resolves one path directly rather than listing
    /// its parent, and carries a symlink's target and its Unix permission
    /// bits, which [`TreeEntry`] does not.
    pub fn mount_stat(&self, snapshot: &str, path: &Path) -> Result<MountEntry, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = repo.node_from_snapshot_and_path(file, &path.to_string_lossy())?;
        Ok(mount_entry(&node))
    }

    /// `dir`'s entries, for mounting the snapshot as a filesystem; see
    /// [`Self::mount_stat`] for why this is not just `list`.
    pub fn mount_list(&self, snapshot: &str, dir: &Path) -> Result<Vec<MountEntry>, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = repo.node_from_snapshot_and_path(file, &dir.to_string_lossy())?;
        let subtree = node
            .subtree
            .ok_or_else(|| not_found(&dir.display().to_string()))?;
        let tree = repo.get_tree(&subtree)?;
        Ok(tree.nodes.iter().map(mount_entry).collect())
    }

    /// `path`'s whole content, for mounting the snapshot as a filesystem: a
    /// filesystem reads by byte range, which rustic's own `dump` does not
    /// support directly, so a mounted file is read once into memory on
    /// open and served from there — fine for the documents and archives
    /// this is for, less so for something huge, which is read whole into
    /// memory regardless of how much of it is actually opened.
    pub fn read_file(&self, snapshot: &str, path: &Path) -> Result<Vec<u8>, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = repo.node_from_snapshot_and_path(file, &path.to_string_lossy())?;
        if !node.is_file() {
            return Err(wrong_kind(path, "file"));
        }
        let mut buffer = Vec::new();
        repo.dump(&node, &mut buffer)?;
        Ok(buffer)
    }

    /// Write `path`'s content, as it was in `snapshot`, to `destination`,
    /// without restoring anything else. Fails if `path` is not a file.
    pub fn dump_file(
        &self,
        snapshot: &str,
        path: &Path,
        destination: &Path,
    ) -> Result<(), EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = repo.node_from_snapshot_and_path(file, &path.to_string_lossy())?;
        if !node.is_file() {
            return Err(wrong_kind(path, "file"));
        }
        let mut out = std::fs::File::create(destination)?;
        repo.dump(&node, &mut out)?;
        Ok(())
    }

    /// Write `path` (a folder), as it was in `snapshot`, to `destination` as
    /// a gzip-compressed tar archive, without restoring anything. Fails if
    /// `path` is not a folder.
    pub fn archive_folder(
        &self,
        snapshot: &str,
        path: &Path,
        destination: &Path,
    ) -> Result<(), EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let root = repo.node_from_snapshot_and_path(file, &path.to_string_lossy())?;
        if !root.is_dir() {
            return Err(wrong_kind(path, "folder"));
        }
        let out = std::fs::File::create(destination)?;
        let gzip = flate2::write::GzEncoder::new(out, flate2::Compression::default());
        let mut tar = tar::Builder::new(gzip);
        for item in repo.ls(&root, &LsOptions::default())? {
            let (relative, node) = item?;
            let mut header = tar::Header::new_gnu();
            header.set_mode(node.meta.mode.unwrap_or(0o644));
            if let Some(mtime) = node.meta.mtime {
                header.set_mtime(mtime.as_second().max(0) as u64);
            }
            if let Some(uid) = node.meta.uid {
                header.set_uid(u64::from(uid));
            }
            if let Some(gid) = node.meta.gid {
                header.set_gid(u64::from(gid));
            }
            if node.is_dir() {
                header.set_entry_type(tar::EntryType::Directory);
                header.set_size(0);
                header.set_cksum();
                tar.append_data(&mut header, &relative, std::io::empty())?;
            } else if node.is_symlink() {
                header.set_entry_type(tar::EntryType::Symlink);
                header.set_size(0);
                header.set_cksum();
                tar.append_link(&mut header, &relative, node.node_type.to_link())?;
            } else if node.is_file() {
                header.set_size(node.meta.size);
                header.set_cksum();
                let mut content = Vec::new();
                repo.dump(&node, &mut content)?;
                tar.append_data(&mut header, &relative, content.as_slice())?;
            }
        }
        tar.into_inner()?.finish()?;
        Ok(())
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
    repo: &Repository<IndexedFullStatus>,
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
