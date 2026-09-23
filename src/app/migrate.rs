// SPDX-License-Identifier: GPL-3.0-only

//! One-time migration of settings saved under the upstream application ID.
//!
//! `cosmic-config` stores each application's settings under
//! `$XDG_CONFIG_HOME/cosmic/<app-id>/v<version>/`, so changing the app ID would
//! otherwise make every existing repository disappear from the sidebar.

use std::io;
use std::path::{Path, PathBuf};

use crate::profile::{Profile, profiles_from_v1};

/// The application ID this fork was created from.
pub const OLD_APP_ID: &str = "com.github.cosmic-utils.Stellarshot";

/// The directory `cosmic-config` resolves its per-user settings under.
pub fn config_root() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn version_dir(config_root: &Path, app_id: &str, version: u64) -> PathBuf {
    config_root
        .join("cosmic")
        .join(app_id)
        .join(format!("v{version}"))
}

/// Copy settings from [`OLD_APP_ID`] to `new_app_id`, once.
///
/// Nothing happens if there are no old settings, or if settings already exist
/// under the new ID — those are newer and must never be overwritten. Returns
/// whether anything was copied. The old settings are left in place.
pub fn migrate_app_id(config_root: &Path, new_app_id: &str, version: u64) -> io::Result<bool> {
    let old = version_dir(config_root, OLD_APP_ID, version);
    let new = version_dir(config_root, new_app_id, version);

    if !old.is_dir() || new.exists() {
        return Ok(false);
    }

    std::fs::create_dir_all(&new)?;
    for entry in std::fs::read_dir(&old)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            std::fs::copy(entry.path(), new.join(entry.file_name()))?;
        }
    }
    Ok(true)
}

/// The profiles to create from version 1 `repositories`, once.
///
/// `None` when there is nothing to do: version 2 already has a `profiles`
/// setting (even an empty one — the user may have removed them all), or
/// there were no version 1 repositories.
pub fn v1_profiles(config_root: &Path, app_id: &str) -> Option<Vec<Profile>> {
    if version_dir(config_root, app_id, 2)
        .join("profiles")
        .exists()
    {
        return None;
    }
    let v1 =
        std::fs::read_to_string(version_dir(config_root, app_id, 1).join("repositories")).ok()?;
    let profiles = profiles_from_v1(&v1);
    (!profiles.is_empty()).then_some(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const NEW: &str = "io.example.New";

    fn write_old(root: &Path, key: &str, value: &str) {
        let dir = version_dir(root, OLD_APP_ID, 1);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(key), value).unwrap();
    }

    #[test]
    fn migrates_old_config_once() {
        let tmp = TempDir::new().unwrap();
        write_old(tmp.path(), "repositories", "[(name: \"a\", path: \"/a\")]");

        assert!(migrate_app_id(tmp.path(), NEW, 1).unwrap());
        let copied = version_dir(tmp.path(), NEW, 1).join("repositories");
        assert_eq!(
            std::fs::read_to_string(&copied).unwrap(),
            "[(name: \"a\", path: \"/a\")]"
        );

        // A second launch finds the new directory and does nothing.
        std::fs::write(&copied, "changed since").unwrap();
        assert!(!migrate_app_id(tmp.path(), NEW, 1).unwrap());
        assert_eq!(std::fs::read_to_string(&copied).unwrap(), "changed since");
    }

    #[test]
    fn does_not_overwrite_existing_new_config() {
        let tmp = TempDir::new().unwrap();
        write_old(tmp.path(), "repositories", "old");
        let new = version_dir(tmp.path(), NEW, 1);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("repositories"), "new").unwrap();

        assert!(!migrate_app_id(tmp.path(), NEW, 1).unwrap());
        assert_eq!(
            std::fs::read_to_string(new.join("repositories")).unwrap(),
            "new"
        );
    }

    #[test]
    fn v1_repositories_become_profiles_once() {
        let tmp = TempDir::new().unwrap();
        let v1 = version_dir(tmp.path(), NEW, 1);
        std::fs::create_dir_all(&v1).unwrap();
        std::fs::write(
            v1.join("repositories"),
            "[(name: \"home\", path: \"/backups/home\")]",
        )
        .unwrap();

        let profiles = v1_profiles(tmp.path(), NEW).expect("one profile to create");
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].name, "home");

        // Once version 2 has its own list, version 1 is never read again.
        let v2 = version_dir(tmp.path(), NEW, 2);
        std::fs::create_dir_all(&v2).unwrap();
        std::fs::write(v2.join("profiles"), "[]").unwrap();
        assert!(
            v1_profiles(tmp.path(), NEW).is_none(),
            "profiles are not duplicated"
        );
    }

    #[test]
    fn no_old_config_is_a_no_op() {
        let tmp = TempDir::new().unwrap();

        assert!(!migrate_app_id(tmp.path(), NEW, 1).unwrap());
        assert!(!version_dir(tmp.path(), NEW, 1).exists());
    }
}
