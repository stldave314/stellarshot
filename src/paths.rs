// SPDX-License-Identifier: GPL-3.0-only

//! Where this user's settings and state live, and keeping them private.
//!
//! `cosmic-config` creates its directories and files with the process umask,
//! so under the usual 022 the `profiles` file (hook commands, a
//! `password_command`, SFTP user and host) is world-readable inside a
//! world-traversable directory. [`tighten_app_dirs`] closes that from our own
//! side at every entry point: a `0700` directory stops anyone else reaching
//! anything inside it, whatever the files' own modes are.

use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::app::APP_ID;
use crate::debug::CONFIG;
use crate::error_log;

/// The current user's home directory: `$HOME` if it is an absolute path,
/// else the one the password database lists, which is still right under
/// `su -`, cron and a service that clears the environment.
pub fn home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return Some(home);
    }
    let mut buffer = vec![0u8; 4096];
    // SAFETY: an all-zero `passwd` (null pointers and zero numbers) is a
    // valid value, and `getpwuid_r` only writes into it and `buffer`, within
    // the length given.
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(
            libc::getuid(),
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() || entry.pw_dir.is_null() {
        return None;
    }
    // SAFETY: `pw_dir` points at a NUL-terminated string inside `buffer`,
    // which is still alive.
    let dir = unsafe { std::ffi::CStr::from_ptr(entry.pw_dir) };
    let path = PathBuf::from(std::ffi::OsStr::from_bytes(dir.to_bytes()));
    path.is_absolute().then_some(path)
}

/// A path nothing can ever be created at or below, for when no home
/// directory can be found at all: whatever tries to use it fails with an
/// error, rather than falling back to a shared directory such as `/tmp`
/// where another user could have put something first.
pub fn unusable() -> PathBuf {
    PathBuf::from("/dev/null/stellarshot")
}

/// `$var` if it is an absolute path, else `<home>/<home_suffix>`.
fn xdg_base(var: &str, home_suffix: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home_dir().map(|home| home.join(home_suffix)))
}

/// `$XDG_CACHE_HOME`, or `~/.cache`.
pub fn cache_root() -> Option<PathBuf> {
    xdg_base("XDG_CACHE_HOME", ".cache")
}

/// `$XDG_CONFIG_HOME`, or `~/.config`.
pub fn config_root() -> Option<PathBuf> {
    xdg_base("XDG_CONFIG_HOME", ".config")
}

/// `$XDG_STATE_HOME`, or `~/.local/state`.
pub fn state_root() -> Option<PathBuf> {
    xdg_base("XDG_STATE_HOME", ".local/state")
}

/// Make `dir` exist with mode `0700`, owned by the current user and not a
/// symlink. An existing directory with looser permissions is tightened; one
/// that is a symlink or belongs to someone else is refused, since changing
/// its mode would act on whatever it points at.
pub fn tighten_private(dir: &Path) -> io::Result<()> {
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {}
        Err(err) => return Err(err),
    }
    let metadata = std::fs::symlink_metadata(dir)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::other(format!(
            "{} is not a plain directory, leaving its permissions alone",
            dir.display()
        )));
    }
    // SAFETY: `getuid` has no preconditions and cannot fail.
    if metadata.uid() != unsafe { libc::getuid() } {
        return Err(io::Error::other(format!(
            "{} is not owned by the current user, leaving its permissions alone",
            dir.display()
        )));
    }
    if metadata.permissions().mode() & 0o777 != 0o700 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// [`tighten_private`] for this app's own settings and state directories
/// (`$XDG_{CONFIG,STATE}_HOME/cosmic/<app id>`). Call from every entry point,
/// after [`crate::debug::init`]. A failure is reported and otherwise ignored:
/// the app still works, only without the tightened permissions.
pub fn tighten_app_dirs() {
    for root in [config_root(), state_root()].into_iter().flatten() {
        let dir = root.join("cosmic").join(APP_ID);
        if let Err(err) = tighten_private(&dir) {
            error_log!(CONFIG, "could not make {} private: {err}", dir.display());
        }
        // Settings from before the app ID changed, copied over once but
        // still there with whatever mode they had: the same hook commands
        // and password command. Tightened only if present, never created.
        let old = root.join("cosmic").join(crate::app::migrate::OLD_APP_ID);
        if old.is_dir()
            && let Err(err) = tighten_private(&old)
        {
            error_log!(CONFIG, "could not make {} private: {err}", old.display());
        }
    }
}

/// Run `change` holding an exclusive lock shared by every Stellarshot process
/// of this user: for a read-change-write of the run state or the history,
/// which the window and a scheduled run can do at the same moment, each
/// otherwise losing the other's change. The lock is held only for that, so
/// waiting for it is brief. Without a state folder to keep the lock file in,
/// `change` runs unlocked rather than not at all.
pub fn with_state_lock<T>(change: impl FnOnce() -> T) -> T {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    let file = state_root().and_then(|root| {
        let dir = root.join("stellarshot");
        crate::engine::lock::create_private_dir(&dir).ok()?;
        std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join("state.lock"))
            .ok()
    });
    if let Some(file) = &file {
        // SAFETY: a valid, open descriptor for the duration of the call.
        // Released when `file` is dropped (closed) after `change`.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    } else {
        crate::debug_log!(CONFIG, "no state lock; updating unlocked");
    }
    let result = change();
    drop(file);
    result
}

/// Delete `key` from the state store, which cosmic-config cannot do: it has
/// `get` and `set` and nothing else. The key's file is found by saving a
/// marker under it and then looking for that marker where cosmic-config
/// keeps state (`<state>/cosmic/<app>/v<version>/<key>`). If the file there
/// does not hold it, the layout is not what this expects, and nothing is
/// deleted: the marker is left, under a key nothing reads again. Call under
/// [`with_state_lock`].
pub fn remove_state_key(key: &str) -> Result<(), String> {
    use cosmic::cosmic_config::{Config, ConfigSet};
    let store = Config::new_state(crate::app::APP_ID, crate::app::config::CONFIG_VERSION)
        .map_err(|err| err.to_string())?;
    let path = state_root()
        .ok_or("no state directory")?
        .join("cosmic")
        .join(crate::app::APP_ID)
        .join(format!("v{}", crate::app::config::CONFIG_VERSION))
        .join(key);
    if std::fs::symlink_metadata(&path).is_err() {
        // Never saved, or already gone.
        return Ok(());
    }
    let marker = format!(
        "removed-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    );
    store.set(key, &marker).map_err(|err| err.to_string())?;
    let saved = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
    if saved.trim() != format!("\"{marker}\"") {
        return Err(format!(
            "{} is not where the state store keeps {key}",
            path.display()
        ));
    }
    std::fs::remove_file(&path).map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn the_home_directory_is_found_even_without_the_environment() {
        let home = home_dir().expect("the test user has a home directory");
        assert!(home.is_absolute());
    }

    #[test]
    fn nothing_can_be_created_at_the_unusable_path() {
        assert!(tighten_private(&unusable()).is_err());
        assert!(std::fs::create_dir_all(unusable().join("x")).is_err());
    }

    #[test]
    fn a_loose_directory_is_tightened_to_0700() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("app");
        std::fs::create_dir(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();

        tighten_private(&dir).unwrap();

        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn a_missing_directory_is_created_private_with_its_parents() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path().join("cosmic/app");

        tighten_private(&dir).unwrap();

        assert_eq!(mode(&dir), 0o700);
    }

    #[test]
    fn a_symlink_is_refused_and_its_target_left_alone() {
        let tmp = tempfile::TempDir::new().unwrap();
        let target = tmp.path().join("target");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = tmp.path().join("link");
        symlink(&target, &link).unwrap();

        assert!(tighten_private(&link).is_err());

        assert_eq!(mode(&target), 0o755, "the target's mode was not touched");
    }

    #[test]
    fn the_state_lock_runs_the_change_and_returns_its_value() {
        assert_eq!(with_state_lock(|| 41 + 1), 42);
        // And again, so a lock left held would show up as a hang here.
        assert_eq!(with_state_lock(|| "again"), "again");
    }
}
