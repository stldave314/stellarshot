// SPDX-License-Identifier: GPL-3.0-only

//! Backup profiles: what to back up, where to, and how.
//!
//! A profile is what the user sees as "a backup" — "Home to the USB drive",
//! "Photos to Google Drive". Stellarshot keeps several, each independent. They
//! are user settings, so they live in `cosmic-config`; the password is never
//! part of a profile and lives only in the keyring.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::{BackupRequest, EngineError, ErrorKind, Location, rclone};

/// Folders under the home directory that are rarely worth backing up and are
/// excluded from a new profile by default, as Déjà Dup does.
pub const DEFAULT_HOME_EXCLUDES: &[&str] = &[".cache", ".local/share/Trash", "Downloads"];

/// Where a profile's repository lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Destination {
    /// A folder on this computer.
    Local { path: PathBuf },
    /// A folder on a removable drive, found by the drive's filesystem UUID
    /// wherever it is mounted.
    Removable {
        uuid: String,
        relative_path: PathBuf,
        label: String,
    },
    /// A folder on an SSH server, reached through rclone's SFTP backend.
    Sftp {
        host: String,
        user: String,
        port: u16,
        path: String,
    },
    /// A folder on a remote in Stellarshot's own rclone configuration: a
    /// cloud account signed in from Stellarshot, or a copy of one of the
    /// user's remotes.
    Rclone {
        remote: String,
        path: String,
        /// What to call it: "Google Drive", or the user's remote name.
        provider: String,
    },
}

impl Destination {
    /// A short human-readable description, for the status card.
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => crate::app::format::path(path),
            Self::Removable {
                label,
                relative_path,
                ..
            } => format!("{label}: /{}", relative_path.display()),
            Self::Sftp {
                host, user, path, ..
            } if user.is_empty() => format!("{host}:{path}"),
            Self::Sftp {
                host, user, path, ..
            } => format!("{user}@{host}:{path}"),
            Self::Rclone { path, provider, .. } => format!("{provider}: {path}"),
        }
    }

    /// Where the engine finds the repository right now. A removable drive
    /// that is not plugged in is `DestinationUnavailable`, naming the drive.
    pub fn location(&self) -> Result<Location, EngineError> {
        match self {
            Self::Local { path } => Ok(Location::local(path)),
            Self::Removable {
                uuid,
                relative_path,
                label,
            } => crate::drives::mount_point(uuid)
                .map(|mount| Location::local(mount.join(relative_path)))
                .ok_or_else(|| EngineError::new(ErrorKind::DestinationUnavailable, label.clone())),
            Self::Sftp {
                host,
                user,
                port,
                path,
            } => {
                let known_hosts = std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join(".ssh/known_hosts");
                Ok(Location::rclone(
                    rclone::sftp_remote(host, user, *port, &known_hosts),
                    path.clone(),
                ))
            }
            Self::Rclone { remote, path, .. } => Ok(Location::rclone(remote.clone(), path.clone())),
        }
    }

    /// A local destination as the matching kind: a folder on a removable
    /// drive becomes `Removable`, so the backup still finds it when the
    /// drive is mounted somewhere else.
    pub fn for_folder(path: PathBuf) -> Self {
        match crate::drives::drive_for(&path) {
            Some((drive, relative_path)) => Self::Removable {
                uuid: drive.uuid,
                relative_path,
                label: drive.label,
            },
            None => Self::Local { path },
        }
    }
}

/// When backups run on their own. Only `Manual` is offered until scheduling
/// exists, so a profile never claims a schedule nothing honours.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schedule {
    #[default]
    Manual,
    Hourly,
    Daily,
    Weekly,
}

/// How long snapshots are kept. Only `KeepForever` is in effect until
/// retention exists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retention {
    #[default]
    KeepForever,
    /// 7 daily, 4 weekly and 12 monthly snapshots.
    Smart,
}

/// One backup the user has set up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable identifier: the keyring item, logs and (later) the schedule are
    /// keyed by it, so renaming a profile never breaks them.
    pub id: String,
    pub name: String,
    pub destination: Destination,
    /// Folders and files to back up.
    pub sources: Vec<PathBuf>,
    /// Folders and files to leave out, even inside a source.
    #[serde(default)]
    pub excludes: Vec<PathBuf>,
    /// Glob patterns to leave out wherever they match.
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Stay on the filesystems the sources are on.
    #[serde(default = "default_true")]
    pub one_file_system: bool,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub retention: Retention,
    /// When the last backup finished, in Unix seconds. A cache for the main
    /// screen: the repository's own snapshot list is the truth.
    #[serde(default)]
    pub last_success: Option<i64>,
}

fn default_true() -> bool {
    true
}

impl Profile {
    /// A new profile with a fresh ID and the default exclusions.
    pub fn new(name: String, destination: Destination, sources: Vec<PathBuf>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            destination,
            sources,
            excludes: Vec::new(),
            exclude_patterns: Vec::new(),
            one_file_system: true,
            schedule: Schedule::Manual,
            retention: Retention::KeepForever,
            last_success: None,
        }
    }

    /// Where the engine finds this profile's repository right now.
    pub fn location(&self) -> Result<Location, EngineError> {
        self.destination.location()
    }

    /// What the engine should back up.
    ///
    /// A repository inside one of the sources — `~` backed up to
    /// `~/Backups/home` — is always excluded, or every backup would copy the
    /// repository into itself and grow without bound.
    pub fn backup_request(&self) -> BackupRequest {
        let mut excludes = self.excludes.clone();
        if let Some(path) = self
            .location()
            .ok()
            .and_then(|location| location.local_path().map(Path::to_path_buf))
        {
            let inside_a_source = self.sources.iter().any(|source| path.starts_with(source));
            if inside_a_source && !excludes.contains(&path) {
                excludes.push(path);
            }
        }
        BackupRequest {
            sources: self.sources.clone(),
            excludes,
            exclude_patterns: self.exclude_patterns.clone(),
            one_file_system: self.one_file_system,
        }
    }
}

/// The default exclusions for a profile that backs up `home`.
pub fn default_excludes(home: &Path) -> Vec<PathBuf> {
    DEFAULT_HOME_EXCLUDES
        .iter()
        .map(|relative| home.join(relative))
        .collect()
}

/// A repository entry from version 1 of the settings.
#[derive(Debug, Deserialize)]
struct V1Repository {
    name: String,
    path: PathBuf,
}

/// Turn the version 1 `repositories` setting (RON) into profiles. Version 1
/// knew nothing about what to back up, so the sources start empty and the
/// profile page asks for them.
pub fn profiles_from_v1(ron_text: &str) -> Vec<Profile> {
    let repositories: Vec<V1Repository> = ron::from_str(ron_text).unwrap_or_default();
    repositories
        .into_iter()
        .map(|repository| {
            Profile::new(
                repository.name,
                Destination::Local {
                    path: repository.path,
                },
                Vec::new(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(path: &str) -> Destination {
        Destination::Local {
            path: PathBuf::from(path),
        }
    }

    #[test]
    fn repository_inside_a_source_is_excluded() {
        let profile = Profile::new(
            "Home".into(),
            local("/home/dave/Backups/home"),
            vec![PathBuf::from("/home/dave")],
        );

        let request = profile.backup_request();

        assert!(
            request
                .excludes
                .contains(&PathBuf::from("/home/dave/Backups/home"))
        );
    }

    #[test]
    fn repository_elsewhere_is_not_added_to_the_excludes() {
        let profile = Profile::new(
            "Home".into(),
            local("/media/usb/backup"),
            vec![PathBuf::from("/home/dave")],
        );

        assert!(profile.backup_request().excludes.is_empty());
    }

    #[test]
    fn a_similar_prefix_is_not_inside() {
        // `/home/dave2` starts with the string `/home/dave` but is not inside it.
        let profile = Profile::new(
            "Home".into(),
            local("/home/dave2/backup"),
            vec![PathBuf::from("/home/dave")],
        );

        assert!(profile.backup_request().excludes.is_empty());
    }

    #[test]
    fn remote_destinations_describe_themselves() {
        let sftp = Destination::Sftp {
            host: "nas.local".into(),
            user: "alex".into(),
            port: 22,
            path: "backups/laptop".into(),
        };
        assert_eq!(sftp.describe(), "alex@nas.local:backups/laptop");
        let drive = Destination::Rclone {
            remote: "stellarshot-1".into(),
            path: "Stellarshot/laptop".into(),
            provider: "Google Drive".into(),
        };
        assert_eq!(drive.describe(), "Google Drive: Stellarshot/laptop");
    }

    #[test]
    fn an_unplugged_drive_is_unavailable_by_name() {
        let drive = Destination::Removable {
            uuid: "no-such-uuid-0000".into(),
            relative_path: "Stellarshot".into(),
            label: "Backup SSD".into(),
        };
        let err = drive.location().unwrap_err();
        assert_eq!(err.kind, ErrorKind::DestinationUnavailable);
        assert_eq!(err.detail, "Backup SSD");
    }

    #[test]
    fn sftp_goes_through_rclone_with_host_key_checking() {
        let destination = Destination::Sftp {
            host: "nas.local".into(),
            user: "alex".into(),
            port: 2222,
            path: "backups".into(),
        };
        match destination.location().unwrap() {
            Location::Rclone { remote, path, .. } => {
                assert!(remote.starts_with(":sftp,host=nas.local,port=2222,user=alex"));
                assert!(remote.contains("known_hosts_file="));
                assert_eq!(path, "backups");
            }
            other => panic!("expected an rclone location, got {other:?}"),
        }
    }

    #[test]
    fn v1_repositories_become_profiles() {
        let v1 = r#"[
            (name: "dave", path: "/home/dave"),
            (name: "usb", path: "/media/usb/backup"),
        ]"#;

        let profiles = profiles_from_v1(v1);

        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].name, "dave");
        assert_eq!(profiles[1].destination, local("/media/usb/backup"));
        assert!(profiles[0].sources.is_empty());
        assert_ne!(profiles[0].id, profiles[1].id);
    }

    #[test]
    fn unreadable_v1_settings_become_no_profiles() {
        assert!(profiles_from_v1("not ron").is_empty());
    }

    #[test]
    fn default_excludes_are_home_relative() {
        let excludes = default_excludes(Path::new("/home/dave"));
        assert!(excludes.contains(&PathBuf::from("/home/dave/.cache")));
        assert!(excludes.contains(&PathBuf::from("/home/dave/Downloads")));
    }

    #[test]
    fn old_profiles_without_new_fields_still_load() {
        // A profile saved before `one_file_system` existed must default it on.
        let saved = r#"(id: "x", name: "n", destination: Local(path: "/b"), sources: ["/h"])"#;
        let profile: Profile = ron::from_str(saved).unwrap();
        assert!(profile.one_file_system);
        assert_eq!(profile.schedule, Schedule::Manual);
    }
}
