// SPDX-License-Identifier: GPL-3.0-only

//! Backup profiles: what to back up, where to, and how.
//!
//! A profile is what the user sees as "a backup" — "Home to the USB drive",
//! "Photos to Google Drive". Stellarshot keeps several, each independent. They
//! are user settings, so they live in `cosmic-config`; the password is never
//! part of a profile and lives only in the keyring.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::debug::CONFIG;
use crate::engine::{BackupRequest, EngineError, ErrorKind, KeepRules, Location, Secret, rclone};
use crate::{error_log, keyring, password_command};

/// Folders under the home directory that are rarely worth backing up and are
/// excluded from a new profile by default, as Déjà Dup does.
pub const DEFAULT_HOME_EXCLUDES: &[&str] = &[".cache", ".local/share/Trash", "Downloads"];

/// Where a profile's repository lives.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// A repository on a rest-server or rustic-server, reached directly.
    Rest {
        /// The full server URL, including the repository name and any HTTP
        /// basic auth (`http://user:pass@host:port/repo/`).
        url: String,
    },
}

/// Hand-written so `Rest`'s `url` (which can carry HTTP basic auth
/// credentials, `http://user:pass@host:port/repo/`) is redacted the same way
/// `engine::Location`'s own `Debug` already redacts it, rather than a
/// `#[derive]` printing the password straight into a log line, an error
/// detail, or `format!("{:?}", …)` anywhere this type ends up.
impl std::fmt::Debug for Destination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local { path } => f.debug_struct("Local").field("path", path).finish(),
            Self::Removable {
                uuid,
                relative_path,
                label,
            } => f
                .debug_struct("Removable")
                .field("uuid", uuid)
                .field("relative_path", relative_path)
                .field("label", label)
                .finish(),
            Self::Sftp {
                host,
                user,
                port,
                path,
            } => f
                .debug_struct("Sftp")
                .field("host", host)
                .field("user", user)
                .field("port", port)
                .field("path", path)
                .finish(),
            Self::Rclone {
                remote,
                path,
                provider,
            } => f
                .debug_struct("Rclone")
                .field("remote", remote)
                .field("path", path)
                .field("provider", provider)
                .finish(),
            Self::Rest { url } => f
                .debug_struct("Rest")
                .field("url", &crate::engine::redact_url(url))
                .finish(),
        }
    }
}

impl Destination {
    /// A short human-readable description, for the status card.
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => crate::core::format::path(path),
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
            // Reuses `Location::describe`'s own redaction of a URL's
            // embedded credentials, rather than a second copy of it here.
            Self::Rest { url } => Location::Rest { url: url.clone() }.describe(),
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
            Self::Rest { .. } => crate::fl!("place-rest"),
        }
    }

    /// A symbolic icon for the kind of place this is.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Local { .. } => "folder-symbolic",
            Self::Removable { .. } => "drive-removable-media-symbolic",
            Self::Sftp { .. } => "network-server-symbolic",
            Self::Rclone { .. } => "folder-remote-symbolic",
            Self::Rest { .. } => "network-server-symbolic",
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
                // Never a relative path, which rclone would look for in
                // whatever directory it was started from.
                let known_hosts = crate::paths::home_dir()
                    .ok_or_else(|| {
                        EngineError::new(ErrorKind::Internal, "no home directory for known_hosts")
                    })?
                    .join(".ssh/known_hosts");
                Ok(Location::rclone(
                    rclone::sftp_remote(host, user, *port, &known_hosts),
                    path.clone(),
                ))
            }
            Self::Rclone { remote, path, .. } => {
                if !valid_rclone_remote(remote) {
                    // Covers a hand-edited settings file as well as an
                    // imported one `settings_export::merge` already rejects:
                    // a remote outside this shape can smuggle rclone
                    // connection options or flags (see SEC-1 in the review).
                    return Err(EngineError::new(ErrorKind::InvalidRemote, remote.clone()));
                }
                Ok(Location::rclone(remote.clone(), path.clone()))
            }
            Self::Rest { url } => Ok(Location::Rest { url: url.clone() }),
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

/// When backups run on their own, through a systemd user timer or path unit.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schedule {
    #[default]
    Manual,
    Hourly,
    Daily,
    Weekly,
    /// As soon as the destination drive is connected, rather than on a
    /// fixed timer. Only meaningful for a [`Destination::Removable`]
    /// destination.
    OnConnect,
}

impl Schedule {
    /// How often a backup on this schedule is expected, for judging whether
    /// one has fallen overdue. `None` for `Manual`, which has no
    /// expectation, and for `OnConnect`, which depends on a drive being
    /// plugged in rather than anything on a clock.
    pub fn period(self) -> Option<i64> {
        const HOUR: i64 = 3600;
        match self {
            Self::Manual | Self::OnConnect => None,
            Self::Hourly => Some(HOUR),
            Self::Daily => Some(24 * HOUR),
            Self::Weekly => Some(7 * 24 * HOUR),
        }
    }
}

/// zstd compression, set once when a repository is created. rustic's own
/// `config` command can still change it on an existing repository, but
/// Stellarshot offers no UI for that, matching how it already treats
/// append-only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compression {
    /// rustic's own default level.
    #[default]
    Default,
    /// Faster, larger snapshots: zstd level -3.
    Fast,
    /// Slower, smaller snapshots: zstd level 19.
    Best,
}

impl Compression {
    /// The level to pass rustic's own `ConfigOptions::set_compression`.
    /// `None` leaves rustic's default in place rather than overriding it
    /// with rustic's own default level spelled out, so a future rustic
    /// version choosing a different default is not silently pinned to
    /// today's.
    pub fn level(self) -> Option<i32> {
        match self {
            Self::Default => None,
            Self::Fast => Some(-3),
            Self::Best => Some(19),
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

/// When a scheduled backup is allowed to actually run, beyond its time
/// slot. Checked only for a run the timer starts; **Back Up Now** always
/// runs regardless. See [`crate::conditions`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conditions {
    /// Skip while running on battery.
    #[serde(default)]
    pub require_ac: bool,
    /// Skip below this battery percentage. `None` means no minimum.
    #[serde(default)]
    pub min_battery_percent: Option<u8>,
    /// Skip on a connection the system has marked metered.
    #[serde(default)]
    pub block_metered: bool,
    /// Skip unless connected to one of `trusted_networks`, a Wi-Fi or wired
    /// connection matched by its name in NetworkManager, or a VPN interface
    /// (Tailscale, WireGuard, or any other) is up.
    #[serde(default)]
    pub require_trusted_network: bool,
    #[serde(default)]
    pub trusted_networks: Vec<String>,
}

impl Conditions {
    /// Whether every condition is off: the common case, and the one where
    /// a scheduled run should read no system state at all.
    pub fn is_empty(&self) -> bool {
        !self.require_ac
            && self.min_battery_percent.is_none()
            && !self.block_metered
            && !self.require_trusted_network
    }
}

/// When a hook runs, relative to a backup. See [`crate::hooks`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HookTiming {
    /// Before the backup starts. A failure stops the backup from running at
    /// all, since the hook exists to make it safe to take (stopping a
    /// database, say) and a backup taken without it may not be.
    #[default]
    Before,
    /// After a successful backup.
    AfterSuccess,
    /// After a backup that failed.
    AfterFailure,
    /// After the backup, whether it succeeded or not.
    After,
}

/// A command or program run before or after a backup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hook {
    /// A short label: shown on the Hooks page and in the run history, never
    /// passed to the command itself.
    pub name: String,
    /// Split the same way `password_command` is: without invoking a real
    /// shell, so it is never subject to shell injection. A small wrapper
    /// script covers a pipe or another shell operator if one is needed.
    pub command: String,
    pub timing: HookTiming,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// One backup the user has set up.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    /// Stable identifier: the keyring item, logs and (later) the schedule are
    /// keyed by it, so renaming a profile never breaks them. Checked when
    /// read (see [`deserialize_id`]): it becomes a config key, a keyring
    /// attribute, a lock name and a unit name.
    #[serde(deserialize_with = "deserialize_id")]
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
    /// Honor each project's own `.gitignore`.
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
    ///
    /// Skipped when empty (the common case) rather than always writing the
    /// field name out, so a settings export with none set never has the
    /// word "password" appear in it at all, matching a plain empty backup.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub password_command: String,
    /// Set once, at creation: rustic's own append-only mode, a guard against
    /// Stellarshot itself (or a scheduled run) mistakenly deleting this
    /// backup's history, not against an attacker with the repository's own
    /// password — `rustic`'s own `config` command can still turn this back
    /// off with nothing more than that same password, which is also all a
    /// compromised account would need. Guarded by rustic itself, not just
    /// Stellarshot's own UI; see [`Profile::prune_enabled`] and the wizard's
    /// When step.
    #[serde(default)]
    pub append_only: bool,
    /// Set once, at creation: see [`Compression`].
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default)]
    pub conditions: Conditions,
    /// Commands run before and after a backup. See [`crate::hooks`].
    #[serde(default)]
    pub hooks: Vec<Hook>,
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
            append_only: false,
            compression: Compression::default(),
            schedule: Schedule::Manual,
            conditions: Conditions::default(),
            hooks: Vec::new(),
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
        // Not just a UI nicety: rustic itself refuses to forget or prune an
        // append-only repository, but attempting it every scheduled run
        // regardless would still mean a failure logged every time.
        !self.append_only
            && self.prune.unwrap_or(match self.destination {
                Destination::Local { .. } | Destination::Removable { .. } => true,
                Destination::Sftp { .. }
                | Destination::Rclone { .. }
                | Destination::Rest { .. } => false,
            })
    }

    /// The password to open this backup's repository with, right now: from
    /// `password_command` if one is set, the keyring otherwise. `Ok(None)`
    /// means neither had one — the same as an unremembered password today,
    /// so a caller with an existing "ask for it" fallback needs no change.
    /// `Err` means a `password_command` was set and actually ran, but
    /// failed (a locked vault, a wrong command) — kept distinct from
    /// `Ok(None)` so a caller can say what really happened rather than
    /// reporting every one of these the same misleading way, as a
    /// scheduled run repeatedly would if this were silently swallowed here.
    pub async fn password(&self) -> Result<Option<Secret>, EngineError> {
        let command = self.password_command.trim();
        if command.is_empty() {
            keyring::load_checked(&self.id)
                .await
                .map_err(|detail| EngineError::new(ErrorKind::KeyringUnavailable, detail))
        } else {
            password_command::run(command).await.map(Some)
        }
    }

    /// Where the engine finds this profile's repository right now. A REST
    /// server's saved address leaves its password out; the one remembered
    /// in the keyring goes back in here (see [`secure_rest_passwords`]).
    pub fn location(&self) -> Result<Location, EngineError> {
        let mut location = self.destination.location()?;
        if let Location::Rest { url } = &mut location
            && let Some(password) = rest_password(&self.id)
        {
            *url = with_rest_password(url, &password);
        }
        Ok(location.with_bandwidth_limit(&self.bandwidth_limit))
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
            profile_tag: crate::engine::profile_tag(&self.id),
        }
    }
}

/// Reads a profile ID, refusing one [`valid_id`] would not accept. A
/// hand-edited `a/b` would otherwise reach the config key (`run-a/b` is a
/// sub-path), the keyring attributes and the lock names.
fn deserialize_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let id = String::deserialize(deserializer)?;
    if valid_id(&id) {
        Ok(id)
    } else {
        Err(serde::de::Error::custom(format!(
            "{id:?} is not a valid backup ID (letters, digits and dashes only, at most 64)"
        )))
    }
}

/// A profile ID safe to put in a unit name and a command line: non-empty,
/// letters, digits and dashes only, at most 64 characters. Shared by
/// `schedule` (a systemd unit name and `--scheduled <id>` argument) and
/// `settings_export::merge` (an imported profile's ID is untrusted).
pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Whether `name` is a remote Stellarshot itself could have created — the
/// exact shape `new_remote_name()` in `app/wizard/place.rs` produces
/// (`stellarshot-` plus 8 lowercase hex digits) — and it actually has a
/// matching section in Stellarshot's own rclone configuration.
///
/// `Destination::Rclone.remote` is used verbatim as `"{remote}:{path}"`, the
/// last argument to `rclone serve restic`. A value outside this shape (from
/// a hand-edited settings file, or one imported from another installation;
/// see [`crate::settings_export::merge`]) could otherwise be interpreted by
/// rclone as connection options (`:sftp,host=h,ssh="…"`) or, if it starts
/// with `--`, as a flag.
pub fn valid_rclone_remote(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("stellarshot-") else {
        return false;
    };
    let shape_ok = suffix.len() == 8
        && suffix
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    shape_ok && rclone::remote_exists(&rclone::config_path(), name)
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
    // The caller (`app::migrate::v1_profiles`) only reaches here once it has
    // already confirmed the version 1 file exists and was read; a failure
    // here is always the RON itself being unreadable, not the file being
    // absent, so every one of its repositories used to disappear silently
    // with no way to tell why.
    let repositories: Vec<V1Repository> = ron::from_str(ron_text).unwrap_or_else(|err| {
        error_log!(CONFIG, "could not parse version 1 settings: {err}");
        Vec::new()
    });
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

/// REST server passwords read from the keyring, by profile ID, for
/// [`Profile::location`]: a lookup there has to be instant, and the keyring
/// is not. Filled by [`secure_rest_passwords`] and [`load_rest_password`].
static REST_PASSWORDS: std::sync::Mutex<std::collections::BTreeMap<String, Secret>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

fn rest_password(profile_id: &str) -> Option<Secret> {
    REST_PASSWORDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(profile_id)
        .cloned()
}

fn set_rest_password(profile_id: &str, password: Option<Secret>) {
    let mut passwords = REST_PASSWORDS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match password {
        Some(password) => passwords.insert(profile_id.to_owned(), password),
        None => passwords.remove(profile_id),
    };
}

/// `url` without its password, and the password; `None` if it has none
/// (or does not parse, in which case it is left alone).
pub fn split_rest_password(url: &str) -> Option<(String, Secret)> {
    let mut parsed = url::Url::parse(url).ok()?;
    let password = percent_decode(parsed.password()?);
    parsed.set_password(None).ok()?;
    Some((parsed.to_string(), Secret::new(password)))
}

/// `url` with `password` in it, unless it already has one of its own.
fn with_rest_password(url: &str, password: &Secret) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.to_owned();
    };
    if parsed.password().is_some() || parsed.set_password(Some(password.expose())).is_err() {
        return url.to_owned();
    }
    parsed.to_string()
}

/// A URL's password as typed: `Url` keeps it percent-encoded.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && let (Some(high), Some(low)) = (
                bytes.get(i + 1).copied().and_then(hex),
                bytes.get(i + 2).copied().and_then(hex),
            )
        {
            // Both digits are below 16, so this fits a byte.
            decoded.push(u8::try_from(high * 16 + low).unwrap_or_default());
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// For every REST server backup: a password still in its saved address is
/// moved to the keyring, and one already there is read for
/// [`Profile::location`]. Returns the backups whose saved address should
/// now leave the password out, with that address. One the keyring cannot
/// take keeps its password where it was, so it goes on working.
pub async fn secure_rest_passwords(profiles: Vec<Profile>) -> Vec<(String, String)> {
    let mut moved = Vec::new();
    for profile in profiles {
        let Destination::Rest { url } = &profile.destination else {
            continue;
        };
        match split_rest_password(url) {
            Some((without, password)) => {
                match keyring::store_rest_password(&profile.id, &profile.name, &password).await {
                    Ok(()) => {
                        set_rest_password(&profile.id, Some(password));
                        moved.push((profile.id.clone(), without));
                    }
                    Err(err) => error_log!(
                        CONFIG,
                        "kept the REST server password of {} in its settings: {err}",
                        profile.id
                    ),
                }
            }
            None => {
                if let Err(err) = load_rest_password(&profile).await {
                    error_log!(
                        CONFIG,
                        "could not read the REST server password of {}: {err}",
                        profile.id
                    );
                }
            }
        }
    }
    moved
}

/// Read `profile`'s REST server password from the keyring for
/// [`Profile::location`], if it is a REST server backup whose saved address
/// leaves it out. `Err` if the keyring could not be reached.
pub async fn load_rest_password(profile: &Profile) -> Result<(), String> {
    let Destination::Rest { url } = &profile.destination else {
        return Ok(());
    };
    if split_rest_password(url).is_some() {
        return Ok(());
    }
    let password = keyring::load_rest_password(&profile.id).await?;
    set_rest_password(&profile.id, password);
    Ok(())
}

/// Forget a removed backup's REST server password, here and in the keyring.
pub async fn forget_rest_password(profile_id: &str) -> Result<(), String> {
    set_rest_password(profile_id, None);
    keyring::forget_rest_password(profile_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_with_an_unsafe_id_does_not_deserialize() {
        let mut profile = Profile::new("Home".into(), local("/mnt/backup"), Vec::new());
        for bad in ["a/b", "../x", "", "with space", &"x".repeat(65)] {
            profile.id = bad.to_owned();
            let text = ron::to_string(&profile).unwrap();
            assert!(
                ron::from_str::<Profile>(&text).is_err(),
                "{bad:?} must be refused when read"
            );
        }
        profile.id = "abc-123".to_owned();
        let text = ron::to_string(&profile).unwrap();
        assert_eq!(ron::from_str::<Profile>(&text).unwrap().id, "abc-123");
    }

    #[tokio::test]
    async fn a_password_command_is_used_instead_of_the_keyring() {
        let mut profile = Profile::new("Home".into(), local("/mnt/backup"), Vec::new());
        profile.password_command = "printf hunter2".into();

        let secret = profile.password().await.unwrap().unwrap();

        assert_eq!(secret.expose(), "hunter2");
    }

    #[tokio::test]
    async fn a_failing_password_command_is_reported_rather_than_treated_as_unremembered() {
        let mut profile = Profile::new("Home".into(), local("/mnt/backup"), Vec::new());
        profile.password_command = "sh -c 'echo vault is locked 1>&2; exit 1'".into();

        let err = profile.password().await.unwrap_err();

        assert_eq!(
            err.detail, "vault is locked",
            "the command's own reason must survive, not be flattened into a generic \
             'not remembered' outcome: {err:?}"
        );
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
        assert!(profile.hooks.is_empty());
        assert_eq!(profile.compression, Compression::Default);
    }

    #[test]
    fn a_remote_outside_the_wizards_own_shape_is_rejected() {
        assert!(!valid_rclone_remote(":local"));
        assert!(!valid_rclone_remote("--config=/x"));
        assert!(!valid_rclone_remote("stellarshot-tooshort"));
        assert!(!valid_rclone_remote("stellarshot-UPPERCASE"));
        assert!(!valid_rclone_remote("not-stellarshot-deadbeef"));
    }

    #[test]
    fn an_rclone_destination_with_an_invalid_remote_fails_to_locate() {
        for remote in [
            ":local",
            "--config=/x",
            ":sftp,host=h,ssh=\"touch /tmp/pwn\"",
        ] {
            let destination = Destination::Rclone {
                remote: remote.into(),
                path: "backups".into(),
                provider: "Custom".into(),
            };
            let err = destination.location().unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidRemote, "remote {remote:?}");
        }
    }

    #[test]
    fn valid_ids_are_alphanumeric_and_dashes_only() {
        assert!(valid_id("a-valid-id-123"));
        assert!(!valid_id(""));
        assert!(!valid_id("../x"));
        assert!(!valid_id("has spaces"));
        assert!(!valid_id(&"x".repeat(65)));
    }

    #[test]
    fn on_connect_has_no_fixed_period() {
        // Unlike a timer, there is nothing to be "overdue" against: it runs
        // whenever the drive is next connected, not on a clock.
        assert_eq!(Schedule::OnConnect.period(), None);
    }

    #[test]
    fn a_rest_password_is_split_out_of_its_address_as_typed() {
        let (without, password) =
            split_rest_password("http://alice:p%40ss%2Fw@host:8000/repo/").unwrap();
        assert_eq!(without, "http://alice@host:8000/repo/");
        assert_eq!(password.expose(), "p@ss/w");
        assert!(split_rest_password("http://alice@host:8000/repo/").is_none());
        assert!(split_rest_password("http://host:8000/repo/").is_none());
    }

    #[test]
    fn a_remembered_rest_password_goes_back_into_the_location() {
        let mut profile = Profile::new(
            "Server".into(),
            Destination::Rest {
                url: "http://alice@host:8000/repo/".into(),
            },
            Vec::new(),
        );
        profile.id = uuid::Uuid::new_v4().to_string();
        let url = |profile: &Profile| match profile.location().unwrap() {
            Location::Rest { url } => url,
            other => panic!("not a REST location: {other:?}"),
        };
        assert_eq!(url(&profile), "http://alice@host:8000/repo/");

        set_rest_password(&profile.id, Some(Secret::new("p@ss/w")));
        assert_eq!(url(&profile), "http://alice:p%40ss%2Fw@host:8000/repo/");
        // Split again, it is the same password.
        let (_, password) = split_rest_password(&url(&profile)).unwrap();
        assert_eq!(password.expose(), "p@ss/w");

        // One typed into the address itself wins.
        profile.destination = Destination::Rest {
            url: "http://alice:typed@host:8000/repo/".into(),
        };
        assert_eq!(url(&profile), "http://alice:typed@host:8000/repo/");
        set_rest_password(&profile.id, None);
    }
}
