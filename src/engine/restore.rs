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

impl std::ops::AddAssign for RestorePreview {
    fn add_assign(&mut self, other: Self) {
        self.files += other.files;
        self.bytes += other.bytes;
        self.unchanged += other.unchanged;
        self.conflicts += other.conflicts;
    }
}

/// `name.ext` → `name (restored 2026-09-23).ext`, or with a counter if that
/// exists too. Built with `OsString`, not `to_string_lossy`, so a stem or
/// extension that is not valid UTF-8 keeps its exact bytes rather than
/// having them replaced.
fn keep_both_name(path: &Path, date: &str) -> PathBuf {
    keep_both_name_with(path, date, |candidate| candidate.symlink_metadata().is_ok())
}

/// [`keep_both_name`], with `taken` deciding whether a candidate is already
/// in use: on disk, or also by something the same restore will write.
fn keep_both_name_with(path: &Path, date: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
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
    while taken(&candidate) {
        candidate = parent.join(name(Some(counter)));
        counter += 1;
    }
    candidate
}

/// Whether what is at `path` already matches `node`. Files compare by size
/// and modification time, at full precision — exactly the comparison
/// rustic's own `add_file` makes (`meta.len() == file.meta.size && mtime ==
/// file.meta.mtime`, confirmed by reading `rustic_core::commands::restore`'s
/// own source, not assumed) before deciding a file needs restoring at all.
/// Comparing only to the second used to disagree with that: two files with
/// the same size and second but a different nanosecond looked identical
/// here, so **Keep Both** never renamed the existing one aside — and then
/// rustic, underneath, restored over it anyway, since *its* comparison
/// still called them different. Symlinks compare by where they point, and
/// are never followed: a link to a file that changed is still the same
/// link.
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
        .and_then(|time| jiff::Timestamp::try_from(time).ok());
    meta.len() == node.meta.size && modified == node.meta.mtime
}

/// Whether some ancestor of `path`, between it and `base`, exists as
/// something other than a directory — the only way `path.symlink_metadata()`
/// can fail even though `path` conceptually sits inside `base`, which the
/// failure alone does not distinguish from `path` simply not existing yet
/// (the common, harmless case: nothing here yet, safe to create). `base` is
/// assumed to already be a real directory.
fn ancestor_is_not_a_directory(base: &Path, path: &Path) -> bool {
    let mut current = path.parent();
    while let Some(dir) = current {
        if let Ok(meta) = dir.symlink_metadata() {
            return !meta.is_dir();
        }
        if dir == base {
            break;
        }
        current = dir.parent();
    }
    false
}

impl Repo {
    /// Work out what restoring `request` would do, without writing anything.
    pub fn preview_restore(self, request: &RestoreRequest) -> Result<RestorePreview, EngineError> {
        run(self, request, None)
    }

    /// [`Self::preview_restore`] for several requests, added up. The
    /// repository's index is read once for all of them, not once each: on
    /// cloud storage that is a download per request.
    pub fn preview_restores(
        self,
        requests: &[RestoreRequest],
    ) -> Result<RestorePreview, EngineError> {
        let repo = self.inner.to_indexed()?;
        let mut total = RestorePreview::default();
        for request in requests {
            total += run_indexed(&repo, request, true)?;
        }
        Ok(total)
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
    run_indexed(&repo, request, dry_run)
}

/// [`run`], on a repository whose index is already read.
fn run_indexed(
    repo: &Repository<IndexedFullStatus>,
    request: &RestoreRequest,
    dry_run: bool,
) -> Result<RestorePreview, EngineError> {
    let snapshot = repo.get_snapshot_from_str(&request.snapshot, |_| true)?;
    let date = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();

    // Into a folder, each item lands under its own name: two with the same
    // name would overwrite each other, whatever the conflict policy says.
    if let Target::Folder(_) = &request.target {
        let mut names = std::collections::HashSet::new();
        for name in request.paths.iter().filter_map(|path| path.file_name()) {
            if !names.insert(name) {
                return Err(EngineError::new(
                    ErrorKind::DuplicateName,
                    name.to_string_lossy().into_owned(),
                ));
            }
        }
    }

    let mut total = RestorePreview::default();
    for path in &request.paths {
        let preview = restore_one(repo, &snapshot, path, request, &date, dry_run)?;
        total += preview;
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

/// Rejects a name or relative path taken directly from a snapshot's own
/// tree data (a single node name, or, as `rustic_core`'s `NodeStreamer`
/// yields it, a whole walked relative path) that would land outside its
/// intended base once joined onto it: a `..` component walks back up, and
/// an absolute path replaces the join outright (`PathBuf::join` with an
/// absolute right-hand side discards the left side). Neither should occur
/// — a well-formed snapshot's paths are always relative and self-contained
/// — but a snapshot's tree is deserialized from repository data, which for
/// a repository shared with another person or machine (`keys.rs`) may not
/// be trustworthy. Used here by [`restore_one`], and by `browse`'s own
/// listing, search, missing-files and mount code for the same reason
/// (see SEC-2 in the review plan). An empty path (the node being restored
/// or looked up itself, not one of its descendants) has no components and
/// always passes.
/// Whether writing `on_disk` (somewhere at or below `destination`) would go
/// through a symlink that is already there: `destination` itself or a
/// folder between it and `on_disk`, or `on_disk` being a symlink when what
/// is to be written there is not one (a file written over a symlink
/// follows it; a symlink written over a symlink just replaces it).
fn writes_through_a_symlink(destination: &Path, on_disk: &Path, item_is_symlink: bool) -> bool {
    fn is_symlink(path: &Path) -> bool {
        path.symlink_metadata()
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
    }
    let Ok(rest) = on_disk.strip_prefix(destination) else {
        return false;
    };
    let components: Vec<_> = rest.components().collect();
    if let Some((_, folders)) = components.split_last() {
        let mut current = destination.to_path_buf();
        if is_symlink(&current) {
            return true;
        }
        for component in folders {
            current.push(component);
            if is_symlink(&current) {
                return true;
            }
        }
    }
    !item_is_symlink && is_symlink(on_disk)
}

pub(super) fn reject_unsafe_relative_path(relative: &Path) -> Result<(), EngineError> {
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

/// What to do with one item of the snapshot, given what is on disk.
enum Decision {
    /// Restore it where it says.
    Keep,
    /// Leave it out.
    Skip,
    /// Restore it under this path (relative to the destination) instead.
    Rename(PathBuf),
}

/// What [`shape`] needs to know about the restore as a whole.
struct ShapeContext<'a> {
    destination: &'a Path,
    /// Whether the snapshot itself has an item at this path (relative to
    /// `destination`): a Keep both name must not be one the same restore is
    /// about to write.
    in_snapshot: &'a dyn Fn(&Path) -> bool,
    policy: ConflictPolicy,
    date: &'a str,
    /// A single file is being restored: its one item lands at
    /// `destination` itself, whose conflicts `restore_one` has already
    /// handled.
    single_file: bool,
}

/// `mode` (as a restic snapshot stores it: Go's `os.FileMode` layout, where
/// setuid and setgid are bits 23 and 22, not `0o4000`/`0o2000`) without
/// setuid and setgid.
fn without_set_id(mode: u32) -> u32 {
    const GO_SETUID: u32 = 1 << 23;
    const GO_SETGID: u32 = 1 << 22;
    mode & !(GO_SETUID | GO_SETGID)
}

/// What rustic uses to recognize names of one hard-linked file: the same
/// device and inode, on a file with more than one link.
fn hardlink_key(node: &Node) -> Option<(u64, u64)> {
    (node.is_file() && node.meta.links > 1 && node.meta.device_id != 0 && node.meta.inode != 0)
        .then_some((node.meta.device_id, node.meta.inode))
}

/// Overwrite cannot replace a folder with a file or a file with a folder.
fn type_conflict(path: &Path) -> EngineError {
    EngineError::new(ErrorKind::TypeConflict, path.display().to_string())
}

/// The decisions for every item, in the order the snapshot lists them.
#[derive(Default)]
struct Decisions {
    skipped: Vec<bool>,
    renamed: std::collections::HashMap<usize, PathBuf>,
    /// Second and later names of a hard-linked file whose path already
    /// exists: restored as a file of their own. rustic would otherwise link
    /// them to the first name in its last pass, which fails on a path that
    /// is already there and aborts the whole restore.
    unlinked: std::collections::HashSet<usize>,
}

impl Decisions {
    fn push(&mut self, decision: Decision) {
        let index = self.skipped.len();
        self.skipped.push(matches!(decision, Decision::Skip));
        if let Decision::Rename(path) = decision {
            self.renamed.insert(index, path);
        }
    }

    /// `None` for an item left out; otherwise the relative path to restore
    /// it under, if that is not its own. `Err` for an item that was never
    /// decided on: the listing has changed since it was read, and nothing
    /// may be restored on a guess.
    fn get(&self, index: usize) -> Result<Option<Option<&PathBuf>>, ()> {
        match self.skipped.get(index) {
            None => Err(()),
            Some(true) => Ok(None),
            Some(false) => Ok(Some(self.renamed.get(&index))),
        }
    }
}

/// Decide what to do with `item`, at `relative` below `context.destination`,
/// counting what it conflicts with. Reads the disk, never writes it.
fn shape(
    context: &ShapeContext<'_>,
    relative: &Path,
    item: &Node,
    conflicts: &mut u64,
    unchanged_by_us: &mut u64,
) -> Result<Decision, EngineError> {
    let ShapeContext {
        destination,
        in_snapshot,
        policy,
        date,
        single_file,
    } = *context;
    let on_disk = if relative.as_os_str().is_empty() || single_file {
        destination.to_path_buf()
    } else {
        destination.join(relative)
    };
    // A directory item conflicts with something already at its own
    // path that is not itself a directory (a file or symlink in the
    // way). A non-directory item whose own path cannot even be looked
    // up because some ancestor between it and `destination` is not a
    // directory — not merely absent — is the same shape of problem
    // discovered one level lower: `on_disk.symlink_metadata()` failing
    // does not by itself distinguish "genuinely does not exist yet"
    // (safe) from "cannot exist because something is in the way
    // higher up" (a real conflict). Neither can be renamed aside for
    // Keep Both the way an ordinary file conflict is: every item here
    // is shaped independently against the same, fixed `destination`,
    // with no way to carry a rename down to a directory's own
    // descendants, or to rename a path that does not itself exist.
    // Both are treated the same as Skip instead — left untouched —
    // rather than attempt a rename that cannot actually work.
    // A symlink already in the way of where this item would be written
    // is followed by the filesystem calls rustic makes, so the file
    // would land wherever it points — possibly outside `destination`
    // altogether. Overwrite refuses outright; Skip and Keep Both leave
    // it alone, like any other path that cannot be restored into.
    if writes_through_a_symlink(destination, &on_disk, item.is_symlink()) {
        if policy == ConflictPolicy::Overwrite {
            return Err(EngineError::new(
                ErrorKind::UnsafePath,
                on_disk.display().to_string(),
            ));
        }
        *conflicts += 1;
        return Ok(Decision::Skip);
    }
    if single_file {
        return Ok(Decision::Keep);
    }
    // A folder where a file is, or a file below one: rustic cannot write
    // either (it panics on the second), so Overwrite refuses with an
    // explanation instead of passing it through.
    let type_blocked = if item.is_dir() {
        on_disk.symlink_metadata().is_ok_and(|meta| !meta.is_dir())
    } else {
        on_disk.symlink_metadata().is_err() && ancestor_is_not_a_directory(destination, &on_disk)
    };
    if type_blocked {
        if policy == ConflictPolicy::Overwrite {
            return Err(type_conflict(&on_disk));
        }
        *conflicts += 1;
        return Ok(Decision::Skip);
    }
    // A file where a folder is: Keep Both can still restore it under
    // another name, Skip leaves it, Overwrite cannot replace a folder.
    if !item.is_dir()
        && policy == ConflictPolicy::Overwrite
        && on_disk.symlink_metadata().is_ok_and(|meta| meta.is_dir())
    {
        return Err(type_conflict(&on_disk));
    }
    if item.is_dir() || relative.as_os_str().is_empty() || on_disk.symlink_metadata().is_err() {
        return Ok(Decision::Keep);
    }
    if looks_identical(&on_disk, item) {
        return Ok(if policy == ConflictPolicy::Skip {
            *unchanged_by_us += 1;
            Decision::Skip
        } else {
            Decision::Keep
        });
    }
    *conflicts += 1;
    Ok(match policy {
        ConflictPolicy::Overwrite => Decision::Keep,
        ConflictPolicy::KeepBoth => {
            let renamed = keep_both_name_with(&on_disk, date, |candidate| {
                candidate.symlink_metadata().is_ok()
                    || candidate.strip_prefix(destination).is_ok_and(in_snapshot)
            });
            Decision::Rename(
                renamed
                    .strip_prefix(destination)
                    .map_or_else(|_| relative.to_path_buf(), Path::to_path_buf),
            )
        }
        ConflictPolicy::Skip => Decision::Skip,
    })
}

/// How an item's path below the destination becomes one below the root given
/// to `LocalDestination` (see the long comment in [`restore_one`]).
enum Rooting {
    Same,
    /// A folder: its own name, relative to the root, goes in front.
    Below(PathBuf),
    /// A single file: its possibly renamed final name replaces its path.
    Replace(PathBuf),
}

/// The snapshot's items with [`Decisions`] applied: what rustic is given, one
/// item at a time, as often as it asks.
fn shaped_stream<'a>(
    repo: &'a Repository<IndexedFullStatus>,
    node: &Node,
    options: &LsOptions,
    decisions: &'a Decisions,
    rooted: &'a Rooting,
) -> Result<impl Iterator<Item = rustic_core::RusticResult<(PathBuf, Node)>> + 'a, EngineError> {
    let items = repo.ls(node, options)?;
    Ok(items
        .enumerate()
        .filter_map(move |(index, entry)| match entry {
            Err(err) => Some(Err(err)),
            Ok((relative, item)) => {
                let decided = match decisions.get(index) {
                    Ok(decided) => decided?,
                    Err(()) => {
                        return Some(Err(rustic_core::RusticError::new(
                            rustic_core::ErrorKind::Internal,
                            "the snapshot's listing changed between reading it and restoring it",
                        )));
                    }
                };
                // Checked again here, not trusted from the first pass: it
                // is what is actually handed to rustic.
                if check_walked(&relative, &item).is_err() {
                    return Some(Err(rustic_core::RusticError::new(
                        rustic_core::ErrorKind::Internal,
                        "an unsafe path appeared in the snapshot's listing",
                    )));
                }
                let relative = decided.cloned().unwrap_or(relative);
                let mut item = item;
                if decisions.unlinked.contains(&index) {
                    item.meta.links = 1;
                }
                // A file comes back without setuid or setgid: in a snapshot
                // from someone else's machine (or a crafted one) they would
                // make a restored program run with the restoring user's
                // rights for whoever can reach it.
                if !item.is_dir() {
                    item.meta.mode = item.meta.mode.map(without_set_id);
                }
                let relative = match rooted {
                    Rooting::Same => relative,
                    Rooting::Below(leaf) => leaf.join(relative),
                    Rooting::Replace(leaf) => leaf.clone(),
                };
                Some(Ok((relative, item)))
            }
        }))
}

/// [`reject_unsafe_relative_path`] for a single name taken from a snapshot's
/// tree, where an empty name (which lists as the directory itself and loops
/// a mount), `.`, `..` or a `/` inside it must never be accepted: as a path
/// those would slip through as "no components" or a two-part name.
pub(super) fn reject_unsafe_name(name: &std::ffi::OsStr) -> Result<(), EngineError> {
    use std::os::unix::ffi::OsStrExt;
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes == b"." || bytes == b".." || bytes.contains(&b'/') {
        Err(EngineError::new(
            ErrorKind::UnsafePath,
            name.to_string_lossy().into_owned(),
        ))
    } else {
        Ok(())
    }
}

/// Checks one `(relative, node)` pair as `repo.ls` walks a snapshot's tree,
/// before anything acts on it. A repository can be shared with someone else,
/// so its tree is untrusted: the walked path must stay below where the walk
/// started ([`reject_unsafe_relative_path`]), the node's own name must be a
/// single plain name ([`reject_unsafe_name`]), and the path must end in that
/// name. A node named `a/evil` would otherwise pass as the two ordinary
/// components `a` and `evil`, and land inside whatever `a` turns out to be,
/// such as a symlink the same restore creates later.
pub(super) fn check_walked(relative: &Path, node: &Node) -> Result<(), EngineError> {
    reject_unsafe_relative_path(relative)?;
    let name = node.name();
    reject_unsafe_name(&name)?;
    if relative.file_name() != Some(&*name) {
        return Err(EngineError::new(
            ErrorKind::UnsafePath,
            relative.display().to_string(),
        ));
    }
    Ok(())
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
    // What is already at the destination is of the other kind (a file where
    // the folder goes, or a folder where the file goes): Keep Both restores
    // beside it, Skip leaves it, Overwrite cannot.
    let existing = destination.symlink_metadata().ok();
    if existing
        .as_ref()
        .is_some_and(|meta| meta.is_dir() != node.is_dir() && !meta.file_type().is_symlink())
    {
        match request.policy {
            ConflictPolicy::Overwrite => return Err(type_conflict(&destination)),
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
    } else if !node.is_dir() && existing.is_some() && !looks_identical(&destination, &node) {
        // A single file restored over a different existing one: the whole
        // destination moves aside or is skipped.
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

    // What to do with each item is decided once, against the disk as it is
    // now, and remembered (a byte per item, plus the rare rename): rustic
    // reads the stream twice more, and by the second time its own writes
    // have changed what the disk looks like, which would change the answers.
    // Nothing is collected into memory, so a whole-home restore costs a byte
    // per file instead of a copy of every node.
    let ls_options = LsOptions::default();
    let in_snapshot = |relative: &Path| node_at(repo, snapshot, &path.join(relative)).is_ok();
    let context = ShapeContext {
        destination: &destination,
        in_snapshot: &in_snapshot,
        policy: request.policy,
        date,
        single_file: !node.is_dir(),
    };
    let mut decisions = Decisions::default();
    let mut linked = std::collections::HashSet::new();
    super::browse::listable(&node)?;
    for entry in repo.ls(&node, &ls_options)? {
        let (relative, item) = entry?;
        check_walked(&relative, &item)?;
        let index = decisions.skipped.len();
        let decision = shape(
            &context,
            &relative,
            &item,
            &mut conflicts,
            &mut unchanged_by_us,
        )?;
        // Where this item will land, for the hard-link check below.
        let lands = match &decision {
            Decision::Skip => None,
            Decision::Keep if !node.is_dir() || relative.as_os_str().is_empty() => {
                Some(destination.clone())
            }
            Decision::Keep => Some(destination.join(&relative)),
            Decision::Rename(renamed) => Some(destination.join(renamed)),
        };
        let link = hardlink_key(&item);
        decisions.push(decision);
        if let (Some(key), Some(lands)) = (link, lands)
            && !linked.insert(key)
            && lands.symlink_metadata().is_ok()
        {
            decisions.unlinked.insert(index);
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
    let rooted = if root == destination {
        Rooting::Same
    } else if node.is_dir() {
        Rooting::Below(leaf)
    } else {
        Rooting::Replace(leaf)
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
    let plan = repo.prepare_restore(
        &options,
        shaped_stream(repo, &node, &ls_options, &decisions, &rooted)?,
        &dest,
        dry_run,
    )?;
    let files = &plan.stats.files;
    let preview = RestorePreview {
        files: files.restore + files.modify,
        bytes: plan.restore_size,
        unchanged: files.unchanged + files.verified + unchanged_by_us,
        conflicts,
    };
    if !dry_run {
        repo.restore(
            plan,
            &options,
            shaped_stream(repo, &node, &ls_options, &decisions, &rooted)?,
            &dest,
        )?;
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_are_remembered_by_position_in_the_listing() {
        let mut decisions = Decisions::default();
        decisions.push(Decision::Keep);
        decisions.push(Decision::Skip);
        decisions.push(Decision::Rename(PathBuf::from("a (restored)")));

        assert_eq!(decisions.get(0), Ok(Some(None)), "restored as it is");
        assert_eq!(decisions.get(1), Ok(None), "left out");
        assert_eq!(
            decisions.get(2),
            Ok(Some(Some(&PathBuf::from("a (restored)")))),
            "restored under another name"
        );
        assert_eq!(
            decisions.get(3),
            Err(()),
            "never decided: refused, not guessed"
        );
    }

    fn file_node(name: &str) -> Node {
        Node::new_node(
            std::ffi::OsStr::new(name),
            rustic_core::repofile::NodeType::File,
            rustic_core::repofile::Metadata::default(),
        )
    }

    #[test]
    fn a_walked_item_must_be_named_what_its_path_ends_in() {
        assert!(check_walked(Path::new("docs/report.pdf"), &file_node("report.pdf")).is_ok());
        // A node whose own name holds a `/` walks as two plain components.
        let err =
            check_walked(Path::new("a/evil.desktop"), &file_node("a/evil.desktop")).unwrap_err();
        assert_eq!(err.kind, ErrorKind::UnsafePath);
        for name in ["", ".", ".."] {
            assert!(
                check_walked(&Path::new("dir").join(name), &file_node(name)).is_err(),
                "{name:?} must be refused"
            );
        }
        assert!(check_walked(Path::new("x/other"), &file_node("report.pdf")).is_err());
    }

    #[test]
    fn a_single_name_must_be_a_plain_name() {
        use std::ffi::OsStr;
        for bad in ["", ".", "..", "a/b", "/"] {
            assert!(
                reject_unsafe_name(OsStr::new(bad)).is_err(),
                "{bad:?} must be refused"
            );
        }
        assert!(reject_unsafe_name(OsStr::new("notes.txt")).is_ok());
        assert!(reject_unsafe_name(OsStr::new("..hidden")).is_ok());
    }

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
