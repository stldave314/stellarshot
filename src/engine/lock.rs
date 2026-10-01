// SPDX-License-Identifier: GPL-3.0-only

//! One writer per repository.
//!
//! rustic takes no repository lock of its own, so two processes could back up
//! to, or prune, the same repository at once. Every process that writes — the
//! window, a `--run` child, a scheduled run — first takes an exclusive lock
//! on a file named after the repository's location. The kernel releases it if
//! the holder dies, so a killed backup can never leave a stale lock behind.
//!
//! The lock itself is an open-file-description lock
//! (`fcntl(F_OFD_SETLK)`/`F_OFD_GETLK`), not `flock()` (what
//! `std::fs::File::try_lock` uses) or a classic per-process `fcntl` record
//! lock (what `rustix::fs`'s own `fcntl_lock` exposes). Both of those only
//! ever answer "did I just get it", so checking whether someone else holds
//! one means taking it, however briefly, and releasing it again — exactly
//! what [`is_running`] must not do (see its own doc comment). An OFD lock's
//! `F_OFD_GETLK` command answers that question directly, without ever
//! acquiring anything.

use crate::constants::STATUS_KEY_MAX_AGE;
use crate::debug::LOCK;
use crate::debug_log;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use super::error::{EngineError, ErrorKind};
use super::repo::Location;

/// The per-user directory lock and progress files live in.
///
/// Never falls back to a shared, world-writable location like `/tmp`: on a
/// multi-user machine, anyone who creates `/tmp/stellarshot` first (trivial,
/// since it does not exist yet) could hold `flock` on a predictable lock
/// file name to make every backup here look permanently `Locked`, or plant a
/// symlink a progress file write would follow. `$XDG_RUNTIME_DIR` is unset
/// in a few real situations (`su -`, SSH without `pam_systemd`, cron), so
/// this falls back to a private, per-user cache directory instead — still
/// verified private by [`create_private_dir`] before anything is written
/// into it, since even `~/.cache` could in principle already exist with the
/// wrong owner or permissions.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|dir| dir.join("stellarshot"))
        .unwrap_or_else(|| cache_dir().join("stellarshot/run"))
}

/// Create `dir/name` for an "Open a copy", private from the start: `dir`
/// itself through [`create_private_dir`] (creating it with the umask's mode
/// instead would leave it readable by others, and every later lock would
/// refuse it), and `name` with mode `0700` rather than chmod-ed afterward.
/// Fails if `name` already exists.
pub fn create_open_copy_dir(dir: &Path, name: &str) -> io::Result<PathBuf> {
    use std::os::unix::fs::DirBuilderExt;
    create_private_dir(dir)?;
    let folder = dir.join(name);
    std::fs::DirBuilder::new().mode(0o700).create(&folder)?;
    Ok(folder)
}

/// Remove the "Open a copy" folders (`open-*`) in [`runtime_dir`] that are
/// older than `max_age`: nothing else ever does, and the runtime folder is
/// memory-backed. Returns how many were removed.
pub fn remove_stale_open_copies(max_age: std::time::Duration) -> usize {
    remove_stale_open_copies_in(&runtime_dir(), max_age)
}

/// [`remove_stale_open_copies`] in `dir`.
pub fn remove_stale_open_copies_in(dir: &Path, max_age: std::time::Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("open-") {
            continue;
        }
        // `symlink_metadata`: never follow a link out of this folder.
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let old = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= max_age);
        if metadata.is_dir() && old && std::fs::remove_dir_all(entry.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// `$XDG_CACHE_HOME`, or `~/.cache` if that too is unset. Only reached when
/// `XDG_RUNTIME_DIR` is not set; falling back to `std::env::temp_dir()` here
/// would be exactly the shared-directory problem [`runtime_dir`] exists to
/// avoid, so with no home directory at all this is a path nothing can be
/// created at ([`crate::paths::unusable`]) and the write that needed a lock
/// fails.
fn cache_dir() -> PathBuf {
    crate::paths::cache_root().unwrap_or_else(crate::paths::unusable)
}

/// Where the latest progress of a write to `location` is published, for a
/// window that did not start it.
pub fn progress_path(location: &Location) -> PathBuf {
    progress_path_in(&runtime_dir(), location)
}

/// [`progress_path`], with the lock directory given explicitly.
pub fn progress_path_in(dir: &Path, location: &Location) -> PathBuf {
    dir.join(format!("{}.progress", location.key()))
}

/// Held for the duration of a write; released when dropped (closing the
/// file description releases an OFD lock, the same as `flock` releases on
/// close). Also holds the legacy lock file, when [`Location::legacy_key`]
/// says this location has one, so an older Stellarshot still running
/// during an upgrade sees the same write as locked.
#[derive(Debug)]
pub struct WriteLock {
    _file: File,
    _legacy: Option<File>,
    key: String,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        debug_log!(LOCK, "released the write lock {}", self.key);
    }
}

/// An exclusive OFD lock covering the whole file, non-blocking.
fn exclusive_lock() -> libc::flock {
    // Zeroed, then the fields that matter set: a struct literal would not
    // compile on a libc target whose `flock` has padding or extra fields.
    // SAFETY: an all-zero `flock` is a valid value (plain integers).
    let mut lock: libc::flock = unsafe { std::mem::zeroed() };
    lock.l_type = libc::F_WRLCK as libc::c_short;
    lock.l_whence = libc::SEEK_SET as libc::c_short;
    lock
}

/// Take the write lock for `location`, or fail with `Locked` at once if
/// another process holds it.
pub fn acquire(location: &Location) -> Result<WriteLock, EngineError> {
    acquire_in(&runtime_dir(), location)
}

/// [`acquire`], with the lock directory given explicitly.
pub fn acquire_in(dir: &Path, location: &Location) -> Result<WriteLock, EngineError> {
    create_private_dir(dir)?;
    let file = lock_file(dir, &location.key(), location)?;
    let legacy = match location.legacy_key() {
        Some(key) => Some(lock_file(dir, &key, location)?),
        None => None,
    };
    debug_log!(LOCK, "acquired the write lock {}", location.key());
    Ok(WriteLock {
        _file: file,
        _legacy: legacy,
        key: location.key(),
    })
}

fn lock_file(dir: &Path, key: &str, location: &Location) -> Result<File, EngineError> {
    let path = dir.join(format!("{key}.lock"));
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|err| EngineError::io(&path, err))?;
    let mut lock = exclusive_lock();
    // SAFETY: `file` is a valid, open file description for the lifetime of
    // this call, and `lock` is a valid `flock` the kernel only reads and
    // (for `F_OFD_GETLK`, not used here) writes back into.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_SETLK, &mut lock) };
    if result == 0 {
        return Ok(file);
    }
    let err = io::Error::last_os_error();
    // POSIX allows `EACCES` as well as `EAGAIN` for a conflicting lock.
    if matches!(
        err.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::PermissionDenied
    ) {
        debug_log!(LOCK, "the write lock {key} is held by another process");
        Err(EngineError::new(ErrorKind::Locked, location.describe()))
    } else {
        Err(err.into())
    }
}

/// Whether some process currently holds the write lock for `location`: a
/// backup running from a schedule, or another window, that this process did
/// not itself start. Never blocks, and never itself takes the lock even
/// briefly (see this module's own doc comment on why that matters): an
/// unlocked repository returns `false` at once, and any other error (the
/// lock directory could not even be created) is treated the same way,
/// since this is a status display, not a guard against writing.
pub fn is_running(location: &Location) -> bool {
    let dir = runtime_dir();
    let key = status_key(location);
    probe_locked(&dir, &key).unwrap_or(false)
        || location
            .legacy_key()
            .is_some_and(|key| probe_locked(&dir, &key).unwrap_or(false))
}

/// [`Location::key`] for the status poll, remembered for
/// [`STATUS_KEY_MAX_AGE`]: working it out canonicalizes the path, which can
/// block for as long as a hung network mount takes, and the poll runs every
/// few seconds. Only for showing status: taking the lock always works its key
/// out fresh, since two processes must agree on it exactly.
fn status_key(location: &Location) -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use std::time::Instant;
    static KEYS: OnceLock<Mutex<HashMap<String, (Instant, String)>>> = OnceLock::new();
    let keys = KEYS.get_or_init(Mutex::default);
    let name = location.describe();
    let found = keys.lock().ok().and_then(|keys| {
        keys.get(&name)
            .filter(|(at, _)| at.elapsed() < STATUS_KEY_MAX_AGE)
            .map(|(_, key)| key.clone())
    });
    if let Some(key) = found {
        return key;
    }
    // Not under the lock: this is the call that can block.
    let key = location.key();
    if let Ok(mut keys) = keys.lock() {
        keys.insert(name, (Instant::now(), key.clone()));
    }
    key
}

/// [`is_running`], with the lock directory given explicitly, so a test can
/// exercise the exact function production code calls rather than
/// reimplementing the probe.
pub fn is_running_in(dir: &Path, location: &Location) -> bool {
    probe_locked(dir, &location.key()).unwrap_or(false)
        || location
            .legacy_key()
            .is_some_and(|key| probe_locked(dir, &key).unwrap_or(false))
}

/// Whether the lock file named `key` is currently held by anyone, checked
/// with `F_OFD_GETLK`, which reports a conflicting lock without taking one.
fn probe_locked(dir: &Path, key: &str) -> io::Result<bool> {
    let path = dir.join(format!("{key}.lock"));
    let file = match OpenOptions::new().read(true).write(true).open(&path) {
        Ok(file) => file,
        // No lock file yet means nothing has ever written here: unlocked.
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err),
    };
    let mut lock = exclusive_lock();
    // SAFETY: as in `lock_file`; `F_OFD_GETLK` additionally writes the
    // result back into `lock`, which is a valid, appropriately sized
    // `flock` the kernel may write into in full.
    let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_OFD_GETLK, &mut lock) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(lock.l_type as libc::c_int != libc::F_UNLCK)
}

/// Creates `dir` (and any missing parent) mode `0700` if it does not exist
/// yet, then verifies it really is private: `DirBuilder::mode` only applies
/// to a directory it actually creates, so a pre-existing `dir` — planted by
/// another user before this one ever ran, on a shared fallback location — is
/// otherwise accepted silently. Refuses a `dir` that turns out to be a
/// symlink, owned by someone else, or readable/writable/executable by group
/// or other, rather than writing a lock or progress file into it anyway.
pub(crate) fn create_private_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let metadata = std::fs::symlink_metadata(dir)?;
    let current_uid = unsafe { libc::getuid() };
    if metadata.file_type().is_symlink() {
        Err(io::Error::other(format!(
            "{} is a symlink, refusing to use it as a private directory",
            dir.display()
        )))
    } else if metadata.uid() != current_uid {
        Err(io::Error::other(format!(
            "{} is not owned by the current user, refusing to use it as a private directory",
            dir.display()
        )))
    } else if metadata.permissions().mode() & 0o077 != 0 {
        Err(io::Error::other(format!(
            "{} is readable or writable by group or other, refusing to use it as a private directory",
            dir.display()
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// A `TempDir` explicitly set to `0700`: `tempfile::TempDir::new`'s own
    /// mode is not fixed, only whatever `mkdir`'s default (`0777`) becomes
    /// after the process `umask` is applied — under a permissive one (this
    /// sandbox's own is `0007`, allowing the whole group), that lands on
    /// `0770`, which `create_private_dir`'s own ownership/mode check
    /// (correctly) refuses. Every test below stands in for `runtime_dir`'s
    /// real fallback location, which is created with an explicit mode of
    /// its own and does not have this problem; this only exists to make the
    /// *test fixture* as private as the real thing, not to work around the
    /// check.
    fn private_dir() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn a_second_lock_on_the_same_repository_is_refused() {
        let dir = private_dir();
        let location = Location::local("/backups/home");

        let first = acquire_in(dir.path(), &location).unwrap();
        let second = acquire_in(dir.path(), &location).unwrap_err();
        assert_eq!(second.kind, ErrorKind::Locked);

        drop(first);
        acquire_in(dir.path(), &location).expect("released on drop");
    }

    #[test]
    fn different_repositories_do_not_block_each_other() {
        let dir = private_dir();
        let _a = acquire_in(dir.path(), &Location::local("/backups/a")).unwrap();
        let _b = acquire_in(dir.path(), &Location::local("/backups/b")).unwrap();
    }

    #[test]
    fn is_running_reflects_a_real_held_lock_without_taking_it_over() {
        let dir = private_dir();
        let location = Location::local("/backups/home");
        assert!(
            !is_running_in(dir.path(), &location),
            "nothing holds it yet"
        );

        let held = acquire_in(dir.path(), &location).unwrap();
        assert!(is_running_in(dir.path(), &location));

        drop(held);
        assert!(!is_running_in(dir.path(), &location), "released on drop");
    }

    #[test]
    fn a_thousand_concurrent_probes_never_see_a_false_locked() {
        // REL-8's own regression test: `is_running_in` must never itself
        // cause a real `acquire_in` happening at the same moment to fail.
        // Unlike a retry-based mitigation, `F_OFD_GETLK` never takes the
        // lock at all, so this holds even under a prober with no delay
        // between iterations whatsoever.
        let dir = private_dir();
        let location = Location::local("/backups/home");
        let probe_dir = dir.path().to_path_buf();
        let probe_location = location.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let probe_stop = stop.clone();
        let prober = std::thread::spawn(move || {
            while !probe_stop.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = is_running_in(&probe_dir, &probe_location);
            }
        });

        let mut locked_errors = 0;
        for _ in 0..1_000 {
            match acquire_in(dir.path(), &location) {
                Ok(guard) => drop(guard),
                Err(err) if err.kind == ErrorKind::Locked => locked_errors += 1,
                Err(err) => panic!("unexpected error: {err:?}"),
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        prober.join().unwrap();

        assert_eq!(
            locked_errors, 0,
            "a real acquire must never lose to the status probe's own brief hold"
        );
    }

    #[test]
    fn a_world_writable_pre_existing_directory_is_refused() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("shared");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o777)).unwrap();

        let location = Location::local("/backups/home");
        let err = acquire_in(&target, &location).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Io);
    }

    #[test]
    fn a_pre_existing_symlink_is_refused_rather_than_followed() {
        let dir = TempDir::new().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let location = Location::local("/backups/home");
        let err = acquire_in(&link, &location).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Io);
    }

    #[test]
    fn only_old_open_copy_folders_are_removed() {
        use std::time::Duration;
        let dir = tempfile::TempDir::new().unwrap();
        let old = dir.path().join("open-old");
        let other = dir.path().join("somebody-elses");
        std::fs::create_dir_all(old.join("inner")).unwrap();
        std::fs::write(old.join("inner/copy.txt"), b"x").unwrap();
        std::fs::set_permissions(
            old.join("inner/copy.txt"),
            std::fs::Permissions::from_mode(0o400),
        )
        .unwrap();
        std::fs::create_dir(&other).unwrap();

        assert_eq!(
            remove_stale_open_copies_in(dir.path(), Duration::from_secs(3600)),
            0,
            "a fresh one is left"
        );
        assert!(old.exists());

        assert_eq!(remove_stale_open_copies_in(dir.path(), Duration::ZERO), 1);
        assert!(!old.exists(), "including its read-only file");
        assert!(other.exists(), "anything not named open-* is never touched");
    }

    #[test]
    fn the_status_key_is_the_real_key_and_stays_the_same_when_remembered() {
        let dir = tempfile::TempDir::new().unwrap();
        let location = Location::local(dir.path().join("repo"));

        let first = status_key(&location);
        let second = status_key(&location);

        assert_eq!(first, location.key());
        assert_eq!(second, first);
    }

    #[test]
    fn an_open_copy_folder_keeps_the_runtime_folder_usable_for_locks() {
        let root = tempfile::TempDir::new().unwrap();
        let dir = root.path().join("stellarshot");
        let folder = create_open_copy_dir(&dir, "open-test").unwrap();
        assert_eq!(
            std::fs::metadata(&folder).unwrap().permissions().mode() & 0o777,
            0o700
        );
        let location = Location::local(root.path().join("repo"));
        assert!(
            acquire_in(&dir, &location).is_ok(),
            "the lock folder is still accepted after an Open a copy created it"
        );
    }

    #[test]
    fn a_private_pre_existing_directory_is_still_accepted() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("mine");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o700)).unwrap();

        let location = Location::local("/backups/home");
        acquire_in(&target, &location).expect("a directory we already own and lock down is fine");
    }
}
