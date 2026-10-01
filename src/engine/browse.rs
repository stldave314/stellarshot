// SPDX-License-Identifier: GPL-3.0-only

//! Looking inside snapshots: folders, search, versions of a file, what
//! changed between two snapshots, and what has been deleted since.
//!
//! A [`Browser`] keeps one repository open with its tree index loaded for as
//! long as the restore page is open, so moving between folders does not
//! re-read the index each time.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use rustic_core::repofile::{Node, NodeType, SnapshotFile};
use rustic_core::vfs::OpenFile;
use rustic_core::{IndexedFullStatus, LsOptions, Repository, TreeId};
use serde::{Deserialize, Serialize};

use super::error::{EngineError, ErrorKind};
use super::repo::Repo;
use super::restore::{check_walked, reject_unsafe_name, reject_unsafe_relative_path};
use super::snapshots::SnapshotSummary;
use crate::debug::ENGINE;
use crate::error_log;

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
///
/// `name` is an `OsString`, not a `String`: FUSE `readdir` hands it
/// straight to the kernel, which must see the entry's exact bytes. A lossy
/// `to_string_lossy` here would let `readdir` list a non-UTF-8 name that a
/// later `lookup` for that same (now-mangled) name could never match,
/// which is exactly "can see it, can't open it" for a mounted snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub name: std::ffi::OsString,
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

/// One path found by name across every snapshot, not just one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlobalMatch {
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: u64,
    /// Every snapshot this exact path was found in, newest first.
    pub snapshots: Vec<SnapshotSummary>,
}

/// An open repository for looking around in.
pub struct Browser {
    /// Newest first.
    snapshots: Vec<(SnapshotFile, SnapshotSummary)>,
    repo: Mutex<Repository<IndexedFullStatus>>,
    /// The rclone process `repo` talks to; after it, so dropped after it.
    _serve: Option<super::serve::Serve>,
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

/// [`entry`], rejecting `node` first if its own name (taken straight from
/// the snapshot's tree data — see [`reject_unsafe_relative_path`]) would
/// land outside `dir` once joined onto it.
fn checked_entry(dir: &Path, node: &rustic_core::repofile::Node) -> Result<TreeEntry, EngineError> {
    reject_unsafe_name(&node.name())?;
    Ok(entry(dir.join(node.name()), node))
}

fn mount_entry(node: &rustic_core::repofile::Node) -> MountEntry {
    MountEntry {
        name: node.name().into_owned(),
        kind: kind_of(node),
        size: node.meta.size,
        modified: node.meta.mtime.map(|time| time.as_second()),
        mode: node.meta.mode,
        symlink_target: node
            .is_symlink()
            .then(|| node.node_type.to_link().to_path_buf()),
    }
}

/// [`mount_entry`], rejecting `node` first the same way [`checked_entry`]
/// does.
fn checked_mount_entry(node: &rustic_core::repofile::Node) -> Result<MountEntry, EngineError> {
    reject_unsafe_name(&node.name())?;
    Ok(mount_entry(node))
}

/// Resolve `path` to its [`Node`] within `snapshot`, comparing each
/// component as an `OsStr` — never through `Repository::node_from_snapshot_and_path`,
/// whose public signature only takes a `&str` and so loses a name that is
/// not valid UTF-8 to `to_string_lossy`'s replacement character before the
/// comparison even starts. This mirrors that method's own internal
/// algorithm (`Tree::node_from_path`, not itself public): start at a
/// synthetic root node standing for `snapshot.tree`, then descend one
/// `Component::Normal` at a time; every other component (`/`, `.`, `..`)
/// is skipped, since a path built from a snapshot's own recorded names or
/// this app's own UI never legitimately contains one.
pub(super) fn node_at(
    repo: &Repository<IndexedFullStatus>,
    snapshot: &SnapshotFile,
    path: &Path,
) -> Result<Node, EngineError> {
    let mut node = Node::new_node(OsStr::new(""), NodeType::Dir, Default::default());
    node.subtree = Some(snapshot.tree);
    let missing = || not_found(&path.display().to_string());
    for component in path.components() {
        let name = match component {
            Component::RootDir => continue,
            Component::Normal(name) => name,
            // `..` or `.`: skipping it would resolve a different node from
            // the literal path a caller may go on to use as a restore
            // destination.
            _ => {
                return Err(EngineError::new(
                    ErrorKind::UnsafePath,
                    path.display().to_string(),
                ));
            }
        };
        let subtree = node.subtree.ok_or_else(missing)?;
        let tree = repo.get_tree(&subtree)?;
        node = tree
            .nodes
            .into_iter()
            .find(|node| &*node.name() == name)
            .ok_or_else(missing)?;
    }
    Ok(node)
}

/// `what` is a bare snapshot ID or path: [`crate::app::errors::explain`]'s
/// own `error-not-found` message already supplies "is not in this
/// snapshot," so it must not be repeated here.
fn not_found(what: &str) -> EngineError {
    EngineError::new(ErrorKind::NotFound, what)
}

/// `what` is the ambiguous prefix: [`crate::app::errors::explain`]'s own
/// `error-ambiguous` message already says it matches more than one
/// snapshot, so it must not be repeated here.
fn ambiguous(what: &str) -> EngineError {
    EngineError::new(ErrorKind::Ambiguous, what)
}

/// The position of the one snapshot ID starting with `prefix`, among
/// `ids` (newest first, as [`Browser::snapshots`] is sorted). Kept as a
/// free function over plain strings, rather than inline in
/// [`Browser::snapshot`], so the not-found-vs-ambiguous distinction can be
/// tested without building a whole indexed repository.
fn index_of<'a>(ids: impl Iterator<Item = &'a str>, prefix: &str) -> Result<usize, EngineError> {
    let mut matches = ids.enumerate().filter(|(_, id)| id.starts_with(prefix));
    match (matches.next(), matches.next()) {
        (Some((index, _)), None) => Ok(index),
        (Some(_), Some(_)) => Err(ambiguous(prefix)),
        (None, _) => Err(not_found(prefix)),
    }
}

/// Run `write` against a temporary file next to `destination`, moving it
/// into place only once `write` returns `Ok`. A failure at any point —
/// `write`'s own error, or an I/O error saving or renaming — leaves
/// `destination` exactly as it was before the call, never a truncated
/// file: the temporary file lives in its own temporary directory, deleted
/// together with it if `write` never reaches the end.
fn write_atomically(
    destination: &Path,
    write: impl FnOnce(&mut std::fs::File) -> Result<(), EngineError>,
) -> Result<(), EngineError> {
    atomicwrites::AtomicFile::new(destination, atomicwrites::AllowOverwrite)
        .write(write)
        .map_err(|err| match err {
            atomicwrites::Error::Internal(err) => EngineError::from(err),
            atomicwrites::Error::User(err) => err,
        })
}

/// Reads an [`OpenFile`]'s content one `read` at a time, through
/// [`OpenFile::read_at`], rather than all at once: used to stream a file
/// straight into a tar entry (see [`Browser::archive_folder`]) without
/// ever holding its whole content in memory.
struct BlobReader<'a> {
    repo: &'a Repository<IndexedFullStatus>,
    open_file: OpenFile,
    position: usize,
    size: usize,
}

impl Read for BlobReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.position >= self.size {
            return Ok(0);
        }
        let want = buf.len().min(self.size - self.position);
        let data = self
            .open_file
            .read_at(self.repo, self.position, want)
            .map_err(|err| std::io::Error::other(err.to_string()))?;
        let read = data.len();
        if read == 0 {
            // The file shrank between being scanned and actually read
            // during the backup that produced this snapshot, so its
            // stored content has fewer bytes than `node.meta.size` says,
            // and `read_at` has nothing further to give. Filling the rest
            // with zeros, rather than returning `Ok(0)` here, is the same
            // thing restore already does with `set_len(meta.size)` on a
            // short file: `tar::Builder::append_data` pads an entry by the
            // bytes it actually copied, not by the header's declared size
            // (see `builder.rs`'s own `append`/`pad_zeroes`), so stopping
            // early here would misalign every entry written after this one
            // in the archive rather than merely truncating this one file.
            buf[..want].fill(0);
            self.position += want;
            return Ok(want);
        }
        buf[..read].copy_from_slice(&data);
        self.position += read;
        Ok(read)
    }
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
            snapshots,
            repo: Mutex::new(repo),
            _serve: self.serve,
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
        let index = index_of(self.snapshots.iter().map(|(_, s)| s.id.as_str()), id)?;
        Ok(&self.snapshots[index])
    }

    /// The entries of `dir` in `snapshot`: folders first, then by name.
    pub fn list(&self, snapshot: &str, dir: &Path) -> Result<Vec<TreeEntry>, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = node_at(&repo, file, dir)?;
        let subtree = node
            .subtree
            .ok_or_else(|| not_found(&dir.display().to_string()))?;
        let tree = repo.get_tree(&subtree)?;
        let mut entries: Vec<TreeEntry> = tree
            .nodes
            .iter()
            .map(|node| checked_entry(dir, node))
            .collect::<Result<_, EngineError>>()?;
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
        let node = node_at(&repo, file, path)?;
        Ok(mount_entry(&node))
    }

    /// `dir`'s entries, for mounting the snapshot as a filesystem; see
    /// [`Self::mount_stat`] for why this is not just `list`.
    pub fn mount_list(&self, snapshot: &str, dir: &Path) -> Result<Vec<MountEntry>, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = node_at(&repo, file, dir)?;
        let subtree = node
            .subtree
            .ok_or_else(|| not_found(&dir.display().to_string()))?;
        let tree = repo.get_tree(&subtree)?;
        tree.nodes.iter().map(checked_mount_entry).collect()
    }

    /// Open `path` for reading by byte range, for mounting the snapshot as
    /// a filesystem. Unlike a plain `dump`, this reads nothing itself: the
    /// returned [`OpenFile`] is just the list of blobs the file's content
    /// is stored across (see [`Self::read_open_file`]), so opening even a
    /// huge file costs nothing proportional to its size.
    pub fn open_file(&self, snapshot: &str, path: &Path) -> Result<OpenFile, EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = node_at(&repo, file, path)?;
        if !node.is_file() {
            return Err(not_found(&path.display().to_string()));
        }
        Ok(repo.open_file(&node)?)
    }

    /// Up to `length` bytes of `open_file` starting at `offset`, as a
    /// filesystem's own `read(offset, size)` needs: fewer than asked for,
    /// including zero, once `offset` reaches the end, never an error just
    /// for reading past it. Only the blobs this range actually touches
    /// are fetched (through rustic's own blob cache, so reading the same
    /// range twice does not refetch it).
    pub fn read_open_file(
        &self,
        open_file: &OpenFile,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, EngineError> {
        let repo = self.repo()?;
        Ok(open_file.read_at(&repo, offset, length)?.to_vec())
    }

    /// Write `path`'s content, as it was in `snapshot`, to `destination`,
    /// without restoring anything else. Fails if `path` is not a file.
    /// Written to a temporary file first, moved into place only once
    /// complete, so a failure partway never leaves a truncated file at
    /// `destination`.
    pub fn dump_file(
        &self,
        snapshot: &str,
        path: &Path,
        destination: &Path,
    ) -> Result<(), EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let node = node_at(&repo, file, path)?;
        if !node.is_file() {
            return Err(not_found(&path.display().to_string()));
        }
        write_atomically(destination, |out| {
            repo.dump(&node, out)?;
            Ok(())
        })
    }

    /// Write `path` (a folder), as it was in `snapshot`, to `destination` as
    /// a gzip-compressed tar archive, without restoring anything. Fails if
    /// `path` is not a folder. Written to a temporary file first, moved
    /// into place only once complete, for the same reason as
    /// [`Self::dump_file`].
    pub fn archive_folder(
        &self,
        snapshot: &str,
        path: &Path,
        destination: &Path,
    ) -> Result<(), EngineError> {
        let (file, _) = self.snapshot(snapshot)?;
        let repo = self.repo()?;
        let root = node_at(&repo, file, path)?;
        if !root.is_dir() {
            return Err(not_found(&path.display().to_string()));
        }
        write_atomically(destination, |out| {
            let gzip = flate2::write::GzEncoder::new(out, flate2::Compression::default());
            let mut tar = tar::Builder::new(gzip);
            for item in repo.ls(&root, &LsOptions::default())? {
                let (relative, node) = item?;
                check_walked(&relative, &node)?;
                let mut header = tar::Header::new_gnu();
                // A directory with no recorded mode of its own must still
                // be enterable once extracted: `0o644` (no execute bit)
                // would leave `tar` itself unable to write anything inside
                // it before this fix was even reached.
                let default_mode = if node.is_dir() { 0o755 } else { 0o644 };
                header.set_mode(node.meta.mode.unwrap_or(default_mode));
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
                    // Streamed through `BlobReader`, not read whole into a
                    // `Vec` first: a single huge file in the folder being
                    // archived no longer costs its own full size in memory.
                    let mut reader = BlobReader {
                        repo: &repo,
                        open_file: repo.open_file(&node)?,
                        position: 0,
                        size: node.meta.size as usize,
                    };
                    tar.append_data(&mut header, &relative, &mut reader)?;
                    if reader.position != reader.size {
                        // Zero-padded by `BlobReader::read` above rather
                        // than left to corrupt the archive, but this file's
                        // stored content genuinely did not match its
                        // recorded size at backup time, which is worth a
                        // trail even though the archive itself is sound.
                        error_log!(
                            ENGINE,
                            "{}: only {} of {} recorded bytes were available; the rest \
                             was archived as zeros",
                            relative.display(),
                            reader.position,
                            reader.size
                        );
                    }
                }
            }
            tar.into_inner()?.finish()?;
            Ok(())
        })
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
        let root = node_at(&repo, file, Path::new("/"))?;
        let mut found = Vec::new();
        for item in repo.ls(&root, &LsOptions::default())? {
            let (path, node) = item?;
            check_walked(&path, &node)?;
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

    /// Every path whose name contains `query` (ignoring case) in *any*
    /// snapshot, with every snapshot it was found in, up to `limit` distinct
    /// paths. Rustic keeps no persistent search index of its own — this
    /// walks every snapshot's tree once, through `find_matching_nodes`,
    /// which shares the walk of any subtree byte-identical across
    /// snapshots (the common case for a backup) rather than repeating it
    /// once per snapshot.
    pub fn search_all(&self, query: &str, limit: usize) -> Result<Vec<GlobalMatch>, EngineError> {
        let query = query.to_lowercase();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let repo = self.repo()?;
        let ids = self.snapshots.iter().map(|(file, _)| file.tree);
        let result = repo.find_matching_nodes(ids, &|_path, node| {
            node.name()
                .to_string_lossy()
                .to_lowercase()
                .contains(&query)
        })?;
        let mut by_path: BTreeMap<PathBuf, (usize, Vec<usize>)> = BTreeMap::new();
        for (snapshot_index, hits) in result.matches.iter().enumerate() {
            for &(path_index, node_index) in hits {
                // `find_matching_nodes` returns paths relative to the tree
                // root, without the leading `/` every other path in this
                // module carries; same fix-up as `search` above.
                reject_unsafe_relative_path(&result.paths[path_index])?;
                let path = Path::new("/").join(&result.paths[path_index]);
                let (_, snapshot_indices) = by_path.entry(path).or_insert((node_index, Vec::new()));
                snapshot_indices.push(snapshot_index);
            }
        }
        let mut matches: Vec<GlobalMatch> = by_path
            .into_iter()
            .map(|(path, (node_index, snapshot_indices))| {
                let node = &result.nodes[node_index];
                GlobalMatch {
                    path,
                    kind: kind_of(node),
                    size: node.meta.size,
                    snapshots: snapshot_indices
                        .into_iter()
                        .map(|index| self.snapshots[index].1.clone())
                        .collect(),
                }
            })
            .collect();
        matches.truncate(limit);
        Ok(matches)
    }

    /// `path` in every snapshot that has it, newest first.
    pub fn versions(&self, path: &Path) -> Result<Vec<FileVersion>, EngineError> {
        let repo = self.repo()?;
        self.versions_with(|file| node_at(&repo, file, path))
    }

    /// [`versions`](Self::versions) with the lookup injected, so a test can
    /// make it fail the way a damaged repository does.
    pub(super) fn versions_with(
        &self,
        mut find: impl FnMut(&SnapshotFile) -> Result<Node, EngineError>,
    ) -> Result<Vec<FileVersion>, EngineError> {
        let mut versions: Vec<FileVersion> = Vec::new();
        let mut newer_content = None;
        for (file, summary) in &self.snapshots {
            // Only "this snapshot does not have it" is a gap in the history;
            // an unreadable repository is an error, not a shorter list.
            let node = match find(file) {
                Ok(node) => node,
                Err(err) if err.kind == ErrorKind::NotFound => {
                    newer_content = None;
                    continue;
                }
                Err(err) => return Err(err),
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
        self.missing_where(scope, since, limit, |path| path.symlink_metadata().is_err())
    }

    /// [`missing`](Self::missing) with the "is it gone" check injected.
    ///
    /// Each snapshot's candidate files are read while the repository lock is
    /// held, but the check runs with it released: it touches the live
    /// filesystem, and a stalled network mount there would otherwise freeze
    /// every other use of this browser, including the mounted view.
    pub(super) fn missing_where(
        &self,
        scope: &Path,
        since: i64,
        limit: usize,
        mut is_gone: impl FnMut(&Path) -> bool,
    ) -> Result<Vec<MissingEntry>, EngineError> {
        let mut seen: HashSet<PathBuf> = HashSet::new();
        let mut missing = Vec::new();
        for (file, summary) in self.snapshots.iter().filter(|(_, s)| s.time >= since) {
            let mut candidates = Vec::new();
            {
                let repo = self.repo()?;
                let node = match node_at(&repo, file, scope) {
                    Ok(node) => node,
                    Err(err) if err.kind == ErrorKind::NotFound => continue,
                    Err(err) => return Err(err),
                };
                if !node.is_dir() {
                    continue;
                }
                for item in repo.ls(&node, &LsOptions::default())? {
                    let (relative, node) = item?;
                    check_walked(&relative, &node)?;
                    let path = scope.join(relative);
                    if node.is_file() && seen.insert(path.clone()) {
                        candidates.push((path, node.meta.size));
                    }
                }
            }
            for (path, size) in candidates {
                if is_gone(&path) {
                    missing.push(MissingEntry {
                        path,
                        size,
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

    /// Whether the repository lock is free right now (tests only).
    #[cfg(test)]
    pub(super) fn is_unlocked(&self) -> bool {
        self.repo.try_lock().is_ok()
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
    // Keyed by `OsString`, not a lossy `String`: two distinct names that
    // both happen to contain non-UTF-8 bytes can otherwise collapse to the
    // identical replacement-character string, and a plain `BTreeMap`
    // collect silently keeps only the last of them (see REL-6).
    let by_name = |id: TreeId| -> Result<
        BTreeMap<std::ffi::OsString, rustic_core::repofile::Node>,
        EngineError,
    > {
        Ok(repo
            .get_tree(&id)?
            .nodes
            .into_iter()
            .map(|node| (node.name().into_owned(), node))
            .collect())
    };
    let old = by_name(a)?;
    let new = by_name(b)?;
    let names: std::collections::BTreeSet<&std::ffi::OsString> =
        old.keys().chain(new.keys()).collect();
    for name in names {
        reject_unsafe_name(name)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A file whose stored content is shorter than the size recorded in
    /// its snapshot node (it shrank between being scanned and actually
    /// read during the backup that produced the snapshot) must not corrupt
    /// every archive entry written after it. `BlobReader::read` handles
    /// this by filling the shortfall with zeros rather than ending the
    /// stream early; this proves the technique against the real `tar`
    /// crate, which is what actually determines whether it works —
    /// `tar::Builder`'s own `append` pads an entry by the bytes its reader
    /// returned, not by the header's declared size (see `builder.rs`'s
    /// `append`/`pad_zeroes`), so a reader that stops early there
    /// misaligns every entry that follows.
    #[test]
    fn a_reader_shorter_than_its_header_size_is_padded_not_left_to_corrupt_the_archive() {
        /// Mirrors the relevant part of `BlobReader::read`'s fixed
        /// behavior: yields `content`, then zeros up to `declared_size`.
        struct ShortThenZeros<'a> {
            content: &'a [u8],
            position: usize,
            declared_size: usize,
        }
        impl std::io::Read for ShortThenZeros<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.position >= self.declared_size {
                    return Ok(0);
                }
                let want = buf.len().min(self.declared_size - self.position);
                if self.position < self.content.len() {
                    let from_content = want.min(self.content.len() - self.position);
                    buf[..from_content].copy_from_slice(
                        &self.content[self.position..self.position + from_content],
                    );
                    buf[from_content..want].fill(0);
                } else {
                    buf[..want].fill(0);
                }
                self.position += want;
                Ok(want)
            }
        }

        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);

            let mut header = tar::Header::new_gnu();
            header.set_size(1000); // the node's recorded size
            header.set_mode(0o644);
            header.set_cksum();
            let mut reader = ShortThenZeros {
                content: b"only ten!!",
                position: 0,
                declared_size: 1000,
            };
            builder
                .append_data(&mut header, "shrunk.bin", &mut reader)
                .unwrap();

            let mut header = tar::Header::new_gnu();
            let second_content = b"the second file must survive intact";
            header.set_size(second_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "second.txt", &second_content[..])
                .unwrap();

            builder.finish().unwrap();
        }

        // Each entry's content has to be read (or otherwise disposed of)
        // during iteration, before asking for the next one: `tar`'s own
        // entry iterator uses the read position, not the header's
        // recorded size, to know where the next header starts, so a
        // stored `Entry` handle read only after later entries have
        // already been iterated past no longer reflects its own data —
        // that would retest a variant of the very bug this proves fixed,
        // not the archive's actual well-formedness.
        let mut archive = tar::Archive::new(&tar_bytes[..]);
        let mut entries = archive.entries().unwrap();

        let first = entries
            .next()
            .expect("two entries were written")
            .expect("the first entry must parse");
        assert_eq!(first.path().unwrap().to_str().unwrap(), "shrunk.bin");
        assert_eq!(first.header().size().unwrap(), 1000);
        drop(first);

        let mut second = entries
            .next()
            .expect("two entries were written")
            .expect("the second entry must parse; a misaligned header corrupts the rest");
        assert_eq!(second.path().unwrap().to_str().unwrap(), "second.txt");
        let mut content = Vec::new();
        second
            .read_to_end(&mut content)
            .expect("the second entry's content must not be corrupted by the first");
        assert_eq!(content, b"the second file must survive intact");
        drop(second);

        assert!(entries.next().is_none(), "exactly two entries were written");
    }

    /// The control for the test above: a reader that ends at its real
    /// content instead of padding out to the header's declared size —
    /// `BlobReader::read`'s behavior before this fix — must actually
    /// corrupt the archive, not merely truncate the one file. If this ever
    /// stopped failing, the test above would no longer be proving
    /// anything.
    #[test]
    fn without_the_padding_a_short_reader_corrupts_every_later_entry() {
        struct ShortEof<'a> {
            content: &'a [u8],
            position: usize,
        }
        impl std::io::Read for ShortEof<'_> {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.position >= self.content.len() {
                    return Ok(0);
                }
                let n = buf.len().min(self.content.len() - self.position);
                buf[..n].copy_from_slice(&self.content[self.position..self.position + n]);
                self.position += n;
                Ok(n)
            }
        }

        let mut tar_bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_bytes);
            let mut header = tar::Header::new_gnu();
            header.set_size(1000);
            header.set_mode(0o644);
            header.set_cksum();
            let mut reader = ShortEof {
                content: b"only ten!!",
                position: 0,
            };
            builder
                .append_data(&mut header, "shrunk.bin", &mut reader)
                .unwrap();

            let mut header = tar::Header::new_gnu();
            let second_content = b"the second file must survive intact";
            header.set_size(second_content.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, "second.txt", &second_content[..])
                .unwrap();
            builder.finish().unwrap();
        }

        let mut archive = tar::Archive::new(&tar_bytes[..]);
        let result = archive
            .entries()
            .and_then(|entries| entries.collect::<std::io::Result<Vec<_>>>());
        assert!(
            result.is_err(),
            "an unpadded short entry must corrupt the archive past it, proving the \
             padding above is load-bearing and not merely tidy"
        );
    }

    #[test]
    fn a_unique_prefix_finds_its_snapshot() {
        let ids = ["aabb1111", "ccdd2222", "eeff3333"];
        assert_eq!(index_of(ids.into_iter(), "cc").unwrap(), 1);
        assert_eq!(index_of(ids.into_iter(), "ccdd2222").unwrap(), 1);
    }

    #[test]
    fn an_ambiguous_prefix_is_reported_as_ambiguous_not_missing() {
        let ids = ["aabb1111", "aabb2222", "ccdd3333"];

        let error = index_of(ids.into_iter(), "aabb").unwrap_err();

        assert_eq!(
            error.kind,
            ErrorKind::Ambiguous,
            "two snapshots share this prefix; it exists, just not uniquely"
        );
    }

    #[test]
    fn a_prefix_matching_nothing_is_reported_as_not_found() {
        let ids = ["aabb1111", "ccdd2222"];

        let error = index_of(ids.into_iter(), "zzzz").unwrap_err();

        assert_eq!(error.kind, ErrorKind::NotFound);
    }

    /// `Node::new_node` needs no repository or backup at all, so this
    /// proves `list`'s (and `diff_trees`', same shape) own wiring actually
    /// rejects a hostile name straight from a snapshot's tree data, not
    /// just that `reject_unsafe_relative_path` itself is correct in
    /// isolation (already proven in `restore`'s own tests) — see SEC-2 in
    /// the review plan. An end-to-end test through a real malicious
    /// snapshot is still the same disclosed gap `restore`'s own tests
    /// describe: writing one needs `rustic_core`'s largely private
    /// tree-saving API.
    #[test]
    fn a_maliciously_named_node_is_rejected_when_listed() {
        let node = Node::new_node(OsStr::new("../escape"), NodeType::File, Default::default());

        let error = checked_entry(Path::new("/some/dir"), &node).unwrap_err();

        assert_eq!(error.kind, ErrorKind::UnsafePath);
    }

    #[test]
    fn an_ordinary_node_name_lists_fine() {
        let node = Node::new_node(OsStr::new("report.pdf"), NodeType::File, Default::default());

        let listed = checked_entry(Path::new("/some/dir"), &node).unwrap();

        assert_eq!(listed.path, Path::new("/some/dir/report.pdf"));
    }

    #[test]
    fn a_maliciously_named_node_is_rejected_when_mounted() {
        let node = Node::new_node(OsStr::new("../escape"), NodeType::File, Default::default());

        let error = checked_mount_entry(&node).unwrap_err();

        assert_eq!(error.kind, ErrorKind::UnsafePath);
    }
}
