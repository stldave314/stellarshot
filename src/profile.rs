// SPDX-License-Identifier: GPL-3.0-only

//! Backup profiles: what to back up, where to, and how.
//!
//! A profile is what the user sees as "a backup" — "Home to the USB drive",
//! "Photos to Google Drive". Stellarshot keeps several, each independent. They
//! are user settings, so they live in `cosmic-config`; the password is never
//! part of a profile and lives only in the keyring.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::engine::{BackupRequest, EngineError, ErrorKind, KeepRules, Location, Secret, rclone};
use crate::{keyring, password_command};

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

    /// What kind of place this is, in the same words the wizard's "where"
    /// step uses.
    pub fn kind_label(&self) -> String {
        match self {
            Self::Local { .. } => crate::fl!("place-folder"),
            Self::Removable { .. } => crate::fl!("place-drive"),
            Self::Sftp { .. } => crate::fl!("place-server"),
            Self::Rclone { provider, .. } => provider.clone(),
        }
    }

    /// A symbolic icon for the kind of place this is.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Local { .. } => "folder-symbolic",
            Self::Removable { .. } => "drive-removable-media-symbolic",
            Self::Sftp { .. } => "network-server-symbolic",
            Self::Rclone { .. } => "folder-remote-symbolic",
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

/// When backups run on their own, through a systemd user timer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schedule {
    #[default]
    Manual,
    Hourly,
    Daily,
    Weekly,
}

impl Schedule {
    /// How often a backup on this schedule is expected, for judging whether
    /// one has fallen overdue. `None` for `Manual`, which has no expectation.
    pub fn period(self) -> Option<i64> {
        const HOUR: i64 = 3600;
        match self {
            Self::Manual => None,
            Self::Hourly => Some(HOUR),
            Self::Daily => Some(24 * HOUR),
            Self::Weekly => Some(7 * 24 * HOUR),
        }
    }
}

/// How long snapshots are kept. Only this computer's snapshots are ever
/// forgotten; see [`crate::engine::Repo::forget`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Retention {
    #[default]
    KeepForever,
    /// 7 daily, 4 weekly and 12 monthly snapshots.
    Smart,
    /// Every snapshot from the `days` before the newest one: Déjà Dup's "Keep
    /// at least…". Measured from the newest snapshot, not from today, so a
    /// backup that has not run for a while keeps its history.
    KeepFor { days: u32 },
}

impl Retention {
    /// The rules to forget by; `None` keeps everything.
    pub fn keep_rules(self) -> Option<KeepRules> {
        match self {
            Self::KeepForever => None,
            Self::Smart => Some(KeepRules {
                daily: Some(7),
                weekly: Some(4),
                monthly: Some(12),
                ..KeepRules::default()
            }),
            Self::KeepFor { days } => Some(KeepRules {
                within_days: Some(days),
                ..KeepRules::default()
            }),
        }
    }
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
    /// Leave out any folder containing a `CACHEDIR.TAG` file.
    #[serde(default)]
    pub exclude_caches: bool,
    /// Honour each project's own `.gitignore`.
    #[serde(default)]
    pub git_ignore: bool,
    /// Write no snapshot when nothing changed since the last one.
    #[serde(default)]
    pub skip_if_unchanged: bool,
    /// Leave out files larger than this many bytes.
    #[serde(default)]
    pub exclude_larger_than: Option<u64>,
    /// Same as `exclude_patterns`, but case-insensitive.
    #[serde(default)]
    pub exclude_patterns_ignoring_case: Vec<String>,
    /// Files with glob patterns to exclude, one per line.
    #[serde(default)]
    pub exclude_pattern_files: Vec<PathBuf>,
    /// rclone's `--bwlimit` syntax (`"1M"`, `"8M:2M"` for up:down), or empty
    /// for no limit. Only applies to a destination reached through rclone.
    #[serde(default)]
    pub bandwidth_limit: String,
    /// A command that prints the password on its standard output, run fresh
    /// every time one is needed, instead of the keyring. Empty for none.
    #[serde(default)]
    pub password_command: String,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub retention: Retention,
    /// Delete data no snapshot needs any more after forgetting. `None` means
    /// the default for the destination: see [`Profile::prune_enabled`].
    #[serde(default)]
    pub prune: Option<bool>,
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
            exclude_caches: false,
            git_ignore: false,
            skip_if_unchanged: false,
            exclude_larger_than: None,
            exclude_patterns_ignoring_case: Vec::new(),
            exclude_pattern_files: Vec::new(),
            bandwidth_limit: String::new(),
            password_command: String::new(),
            schedule: Schedule::Manual,
            retention: Retention::KeepForever,
            prune: None,
            last_success: None,
        }
    }

    /// Whether unused data is deleted automatically after forgetting.
    ///
    /// rustic takes no lock of its own, so pruning while another computer
    /// backs up to the same place could delete data that backup is about to
    /// reference. Folders and drives on this computer are rarely shared and
    /// prune by default; servers and cloud storage often are, and do not
    /// unless the user turns it on.
    pub fn prune_enabled(&self) -> bool {
        self.prune.unwrap_or(match self.destination {
            Destination::Local { .. } | Destination::Removable { .. } => true,
            Destination::Sftp { .. } | Destination::Rclone { .. } => false,
        })
    }

    /// The password to open this backup's repository with, right now: from
    /// `password_command` if one is set, the keyring otherwise. `None` means
    /// neither had one — the same as an unremembered password today, so a
    /// caller with an existing "ask for it" fallback needs no change.
    pub async fn password(&self) -> Option<Secret> {
        let command = self.password_command.trim();
        if command.is_empty() {
            keyring::load(&self.id).await
        } else {
            password_command::run(command).await.ok()
        }
    }

    /// Where the engine finds this profile's repository right now.
    pub fn location(&self) -> Result<Location, EngineError> {
        Ok(self
            .destination
            .location()?
            .with_bandwidth_limit(&self.bandwidth_limit))
    }

    /// What the engine should back up.
    ///
    /// A repository inside one of the sources — `~` backed up to
    /// `~/Backups/home` — is always excluded, or every backup would copy the
    /// repository into itself and grow without bound.
    ///
    /// `global_exclude_patterns` come from settings shared by every backup
    /// (`node_modules`, `.cache`, …); they are merged in here rather than
    /// stored on the profile, so changing the shared list does not have to
    /// rewrite every profile that uses it.
    pub fn backup_request(&self, global_exclude_patterns: &[String]) -> BackupRequest {
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
        let mut exclude_patterns = self.exclude_patterns.clone();
        for pattern in global_exclude_patterns {
            if !exclude_patterns.contains(pattern) {
                exclude_patterns.push(pattern.clone());
            }
        }
        BackupRequest {
            sources: self.sources.clone(),
            excludes,
            exclude_patterns,
            exclude_patterns_ignoring_case: self.exclude_patterns_ignoring_case.clone(),
            exclude_pattern_files: self.exclude_pattern_files.clone(),
            exclude_larger_than: self.exclude_larger_than,
            exclude_caches: self.exclude_caches,
            git_ignore: self.git_ignore,
            one_file_system: self.one_file_system,
            skip_if_unchanged: self.skip_if_unchanged,
            dry_run: false,
            time: None,
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

    #[tokio::test]
    async fn a_password_command_is_used_instead_of_the_keyring() {
        let mut profile = Profile::new("Home".into(), local("/mnt/backup"), Vec::new());
        profile.password_command = "printf hunter2".into();

        let secret = profile.password().await.unwrap();

        assert_eq!(secret.expose(), "hunter2");
    }

    #[test]
    fn prune_defaults_to_on_only_for_this_computer() {
        let mut profile = Profile::new("Home".into(), local("/mnt/backup"), Vec::new());
        assert!(profile.prune_enabled());
        profile.destination = Destination::Sftp {
            host: "nas".into(),
            user: "alex".into(),
            port: 22,
            path: "backups".into(),
        };
        assert!(!profile.prune_enabled(), "a server may be shared");
        profile.prune = Some(true);
        assert!(profile.prune_enabled(), "unless the user says otherwise");
    }

    #[test]
    fn retention_rules() {
        assert_eq!(Retention::KeepForever.keep_rules(), None);
        let smart = Retention::Smart.keep_rules().unwrap();
        assert_eq!(
            (smart.daily, smart.weekly, smart.monthly),
            (Some(7), Some(4), Some(12))
        );
        assert_eq!(
            Retention::KeepFor { days: 180 }
                .keep_rules()
                .unwrap()
                .within_days,
            Some(180)
        );
    }

    #[test]
    fn older_settings_load_with_no_prune_choice() {
        let text = r#"(
            id: "a",
            name: "Home",
            destination: Local(path: "/mnt/backup"),
            sources: ["/home/alex"],
        )"#;
        let profile: Profile = ron::from_str(text).unwrap();
        assert_eq!(profile.prune, None);
        assert_eq!(profile.retention, Retention::KeepForever);
        assert_eq!(profile.schedule, Schedule::Manual);
    }

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

        let request = profile.backup_request(&[]);

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

        assert!(profile.backup_request(&[]).excludes.is_empty());
    }

    #[test]
    fn global_exclude_patterns_are_merged_in_without_duplicating_a_profiles_own() {
        let mut profile = Profile::new(
            "Home".into(),
            local("/media/usb/backup"),
            vec![PathBuf::from("/home/dave")],
        );
        profile.exclude_patterns = vec!["node_modules".into()];

        let request = profile.backup_request(&["node_modules".into(), "target".into()]);

        assert_eq!(
            request.exclude_patterns,
            vec!["node_modules".to_string(), "target".to_string()]
        );
    }

    #[test]
    fn a_similar_prefix_is_not_inside() {
        // `/home/dave2` starts with the string `/home/dave` but is not inside it.
        let profile = Profile::new(
            "Home".into(),
            local("/home/dave2/backup"),
            vec![PathBuf::from("/home/dave")],
        );

        assert!(profile.backup_request(&[]).excludes.is_empty());
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
    fn a_profiles_bandwidth_limit_reaches_its_rclone_location() {
        let mut profile = Profile::new(
            "Backup".into(),
            Destination::Sftp {
                host: "nas.local".into(),
                user: "alex".into(),
                port: 22,
                path: "backups".into(),
            },
            vec![PathBuf::from("/home/dave")],
        );
        profile.bandwidth_limit = "1M".into();

        match profile.location().unwrap() {
            Location::Rclone {
                bandwidth_limit, ..
            } => assert_eq!(bandwidth_limit, "1M"),
            other => panic!("expected an rclone location, got {other:?}"),
        }
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
