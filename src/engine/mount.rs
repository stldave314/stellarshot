// SPDX-License-Identifier: GPL-3.0-only

//! Mounting a snapshot as a read-only folder through FUSE.
//!
//! rustic_core exposes no `vfs` or mount feature of its own (unlike the
//! separate `rustic` command-line tool, which links `libfuse` directly for
//! this); [`SnapshotFs`] is a small filesystem of Stellarshot's own,
//! reading through the same [`Browser`] the restore page already does, so
//! nothing about how a snapshot's tree is read is duplicated.
//!
//! Read-only by mount option as well as by never implementing a single
//! write operation: every one of those falls through to
//! [`fuser::Filesystem`]'s own default, which replies `ENOSYS`.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use fuser::{
    Errno, FileAttr, FileHandle, FileType, Filesystem, Generation, INodeNo, MountOption, OpenFlags,
    ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, ReplyOpen, Request,
};

use super::browse::{Browser, EntryKind, MountEntry};

/// A snapshot never changes once taken, so there is nothing a short TTL
/// would ever need to catch — a stat or a directory listing is good until
/// the filesystem is unmounted.
const TTL: Duration = Duration::from_secs(365 * 24 * 3600);

/// A mounted snapshot, unmounted (and its background thread stopped) when
/// dropped.
pub struct Mount {
    session: Option<fuser::BackgroundSession>,
    point: PathBuf,
}

impl Mount {
    /// Where this snapshot is mounted.
    pub fn point(&self) -> &Path {
        &self.point
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        // `BackgroundSession` itself unmounts on drop; taking it out only
        // to immediately drop it here is just documenting that this is the
        // moment it happens, for whoever reads this next.
        drop(self.session.take());
    }
}

/// Mount `snapshot` (from `browser`) read-only at `mount_point`, which must
/// already exist as an empty folder.
pub fn mount(
    browser: Arc<Browser>,
    snapshot: String,
    mount_point: &Path,
) -> Result<Mount, std::io::Error> {
    let fs = SnapshotFs::new(browser, snapshot);
    let mut config = fuser::Config::default();
    // Deliberately no `DefaultPermissions`: whoever mounted this already
    // authenticated with the repository's own password, and every entry
    // is reported owned by root (see `attr`) since the original owner is
    // rarely who is looking at it now — enforcing the original mode would
    // only lock the mounting user out of their own `0600` files.
    config.mount_options = vec![
        MountOption::RO,
        MountOption::FSName("stellarshot".to_owned()),
    ];
    let session = fuser::spawn_mount(fs, mount_point, &config)?;
    Ok(Mount {
        session: Some(session),
        point: mount_point.to_path_buf(),
    })
}

/// Paths within the snapshot, numbered as FUSE inodes: assigned the first
/// time something asks about a path, kept for as long as the mount lives.
#[derive(Default)]
struct Inodes {
    by_ino: HashMap<u64, PathBuf>,
    by_path: HashMap<PathBuf, u64>,
    next: u64,
}

impl Inodes {
    fn new() -> Self {
        let mut inodes = Self {
            next: 2, // 1 is FUSE's own reserved root inode.
            ..Self::default()
        };
        inodes.by_ino.insert(1, PathBuf::from("/"));
        inodes.by_path.insert(PathBuf::from("/"), 1);
        inodes
    }

    fn path(&self, ino: u64) -> Option<PathBuf> {
        self.by_ino.get(&ino).cloned()
    }

    fn ino_for(&mut self, path: &Path) -> u64 {
        if let Some(ino) = self.by_path.get(path) {
            return *ino;
        }
        let ino = self.next;
        self.next += 1;
        self.by_ino.insert(ino, path.to_path_buf());
        self.by_path.insert(path.to_path_buf(), ino);
        ino
    }
}

struct SnapshotFs {
    browser: Arc<Browser>,
    snapshot: String,
    inodes: Mutex<Inodes>,
    /// A file's whole content, cached from `open` until `release`, keyed by
    /// the file handle this filesystem hands out (not the inode, so the
    /// same file opened twice does not share or fight over one buffer).
    open_files: Mutex<HashMap<u64, Vec<u8>>>,
    next_handle: AtomicU64,
    /// The mounting user's own uid and gid, reported as the owner of
    /// everything in the mount: whoever the original owner was, this is
    /// the person who authenticated with the repository's own password,
    /// looking at their own backup.
    uid: u32,
    gid: u32,
}

impl SnapshotFs {
    fn new(browser: Arc<Browser>, snapshot: String) -> Self {
        Self {
            browser,
            snapshot,
            inodes: Mutex::new(Inodes::new()),
            open_files: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            uid: rustix::process::getuid().as_raw(),
            gid: rustix::process::getgid().as_raw(),
        }
    }

    fn path_of(&self, ino: INodeNo) -> Option<PathBuf> {
        self.inodes.lock().unwrap().path(u64::from(ino))
    }

    fn ino_for(&self, path: &Path) -> INodeNo {
        INodeNo(self.inodes.lock().unwrap().ino_for(path))
    }
}

fn file_type(kind: EntryKind) -> FileType {
    match kind {
        EntryKind::Directory => FileType::Directory,
        EntryKind::Symlink => FileType::Symlink,
        // A device, socket or other node type this project's `EntryKind`
        // does not distinguish: shown as an empty regular file rather than
        // refused entirely, since it is rare and this is browsing, not a
        // restore that needs to recreate it exactly.
        EntryKind::File | EntryKind::Other => FileType::RegularFile,
    }
}

fn attr(ino: INodeNo, entry: &MountEntry, uid: u32, gid: u32) -> FileAttr {
    let kind = file_type(entry.kind);
    let default_perm = if kind == FileType::Directory {
        0o555
    } else {
        0o444
    };
    // Real permission bits when the snapshot has them, since this is a
    // read-only mount either way; a sensible read-only default otherwise.
    let perm = entry
        .mode
        .map(|mode| (mode & 0o777) as u16)
        .unwrap_or(default_perm);
    let mtime = entry
        .modified
        .and_then(|seconds| u64::try_from(seconds).ok())
        .map(|seconds| SystemTime::UNIX_EPOCH + Duration::from_secs(seconds))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    FileAttr {
        ino,
        size: entry.size,
        blocks: entry.size.div_ceil(512),
        atime: mtime,
        mtime,
        ctime: mtime,
        crtime: mtime,
        kind,
        perm,
        nlink: 1,
        uid,
        gid,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

impl Filesystem for SnapshotFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let Some(parent_path) = self.path_of(parent) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let path = parent_path.join(name);
        match self.browser.mount_stat(&self.snapshot, &path) {
            Ok(entry) => {
                let ino = self.ino_for(&path);
                reply.entry(&TTL, &attr(ino, &entry, self.uid, self.gid), Generation(0));
            }
            Err(_) => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let Some(path) = self.path_of(ino) else {
            reply.error(Errno::ENOENT);
            return;
        };
        match self.browser.mount_stat(&self.snapshot, &path) {
            Ok(entry) => reply.attr(&TTL, &attr(ino, &entry, self.uid, self.gid)),
            Err(_) => reply.error(Errno::ENOENT),
        }
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        let Some(path) = self.path_of(ino) else {
            reply.error(Errno::ENOENT);
            return;
        };
        match self.browser.mount_stat(&self.snapshot, &path) {
            Ok(entry) => match entry.symlink_target {
                Some(target) => reply.data(target.as_os_str().as_encoded_bytes()),
                None => reply.error(Errno::EINVAL),
            },
            Err(_) => reply.error(Errno::ENOENT),
        }
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        _fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let Some(dir) = self.path_of(ino) else {
            reply.error(Errno::ENOENT);
            return;
        };
        let entries = match self.browser.mount_list(&self.snapshot, &dir) {
            Ok(entries) => entries,
            Err(_) => {
                reply.error(Errno::ENOENT);
                return;
            }
        };
        // "." and ".." first, then every real entry, each numbered so a
        // reader that stopped partway can resume from the next one.
        let mut rows: Vec<(INodeNo, FileType, PathBuf)> = vec![
            (ino, FileType::Directory, PathBuf::from(".")),
            (ino, FileType::Directory, PathBuf::from("..")),
        ];
        for entry in &entries {
            let path = dir.join(&entry.name);
            rows.push((self.ino_for(&path), file_type(entry.kind), path));
        }
        for (index, (row_ino, kind, path)) in rows.iter().enumerate().skip(offset as usize) {
            let name = if *path == Path::new(".") || *path == Path::new("..") {
                path.as_os_str()
            } else {
                path.file_name().unwrap_or_default()
            };
            if reply.add(*row_ino, (index + 1) as u64, *kind, name) {
                break;
            }
        }
        reply.ok();
    }

    fn open(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let Some(path) = self.path_of(ino) else {
            reply.error(Errno::ENOENT);
            return;
        };
        match self.browser.read_file(&self.snapshot, &path) {
            Ok(content) => {
                let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);
                self.open_files.lock().unwrap().insert(handle, content);
                reply.opened(FileHandle(handle), fuser::FopenFlags::empty());
            }
            Err(_) => reply.error(Errno::EIO),
        }
    }

    fn read(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyData,
    ) {
        let files = self.open_files.lock().unwrap();
        let Some(content) = files.get(&u64::from(fh)) else {
            reply.error(Errno::EBADF);
            return;
        };
        let start = (offset as usize).min(content.len());
        let end = start.saturating_add(size as usize).min(content.len());
        reply.data(&content[start..end]);
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        _flush: bool,
        reply: fuser::ReplyEmpty,
    ) {
        self.open_files.lock().unwrap().remove(&u64::from(fh));
        reply.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EntryKind;

    fn entry(kind: EntryKind, size: u64) -> MountEntry {
        MountEntry {
            name: "x".to_owned(),
            kind,
            size,
            modified: Some(1_700_000_000),
            mode: None,
            symlink_target: None,
        }
    }

    #[test]
    fn a_new_inode_table_starts_with_only_the_root() {
        let mut inodes = Inodes::new();
        assert_eq!(inodes.path(1), Some(PathBuf::from("/")));
        assert_eq!(
            inodes.ino_for(Path::new("/")),
            1,
            "the root is never renumbered"
        );
    }

    #[test]
    fn the_same_path_always_gets_the_same_inode() {
        let mut inodes = Inodes::new();
        let a = inodes.ino_for(Path::new("/home/alex/a.txt"));
        let b = inodes.ino_for(Path::new("/home/alex/b.txt"));
        assert_ne!(a, b);
        assert_eq!(inodes.ino_for(Path::new("/home/alex/a.txt")), a);
    }

    #[test]
    fn a_folder_defaults_to_read_only_permissions_without_a_recorded_mode() {
        let attr = attr(INodeNo(2), &entry(EntryKind::Directory, 0), 1000, 1000);
        assert_eq!(attr.perm, 0o555);
        assert_eq!(attr.kind, FileType::Directory);
    }

    #[test]
    fn a_files_recorded_mode_is_kept_masked_to_permission_bits() {
        let mut file = entry(EntryKind::File, 42);
        file.mode = Some(0o100_644); // as stat(2) would report it: type bits and all
        let attr = attr(INodeNo(2), &file, 1000, 1000);
        assert_eq!(
            attr.perm, 0o644,
            "the file-type bits must not leak into perm"
        );
        assert_eq!(attr.size, 42);
    }

    #[test]
    fn an_unrecorded_mode_defaults_to_read_only_for_a_file() {
        let attr = attr(INodeNo(2), &entry(EntryKind::File, 1), 1000, 1000);
        assert_eq!(attr.perm, 0o444);
    }

    #[test]
    fn attr_reports_the_mounting_user_as_owner() {
        let attr = attr(INodeNo(2), &entry(EntryKind::File, 1), 1000, 1000);
        assert_eq!((attr.uid, attr.gid), (1000, 1000));
    }
}
