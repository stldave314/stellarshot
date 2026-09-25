// SPDX-License-Identifier: GPL-3.0-only

//! Opening and creating repositories.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustic_backend::BackendOptions;
use rustic_core::{
    ConfigOptions, Credentials, KeyOptions, OpenStatus, Repository, RepositoryBackends,
    RepositoryOptions,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::{EngineError, ErrorKind};
use super::location::{InitCheck, check_init_location, is_repository};
use super::progress::{ProgressSink, SinkBars, SlotGuard};
use super::uploads::ParallelUploads;
use crate::constants::RCLONE_SERVE_FLAGS;
use crate::debug::ENGINE;
use crate::debug_log;

/// A repository password. Never printed.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// Where a repository lives.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Location {
    /// A folder on this computer (including a mounted drive).
    Local { path: PathBuf },
    /// A path on an rclone remote: SFTP, cloud storage, or rclone's own
    /// `:local:` backend in tests. `config` is the rclone configuration that
    /// defines the remote.
    Rclone {
        remote: String,
        path: String,
        config: PathBuf,
        /// rclone's `--bwlimit` syntax (`"1M"`, `"8M:2M"` for up:down, or
        /// empty for no limit).
        #[serde(default)]
        bandwidth_limit: String,
    },
}

impl Location {
    /// A repository in a local folder.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self::Local { path: path.into() }
    }

    /// A repository on an rclone remote defined in Stellarshot's own rclone
    /// configuration.
    pub fn rclone(remote: impl Into<String>, path: impl Into<String>) -> Self {
        Self::Rclone {
            remote: remote.into(),
            path: path.into(),
            config: super::rclone::config_path(),
            bandwidth_limit: String::new(),
        }
    }

    /// The same location, with a bandwidth limit applied if this is an
    /// rclone location; a no-op for a local one, which has no transfer to
    /// limit.
    pub fn with_bandwidth_limit(mut self, limit: &str) -> Self {
        if let Self::Rclone {
            bandwidth_limit, ..
        } = &mut self
        {
            *bandwidth_limit = limit.to_owned();
        }
        self
    }

    /// The folder, for a local repository.
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Self::Local { path } => Some(path),
            Self::Rclone { .. } => None,
        }
    }

    /// How the location reads in messages.
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => path.display().to_string(),
            Self::Rclone { remote, path, .. } => super::rclone::target(remote, path),
        }
    }

    /// A short stable identifier, used to name the lock and progress files
    /// every process writing to this repository shares.
    pub fn key(&self) -> String {
        let digest = match self {
            Self::Local { path } => Sha256::digest(path.as_os_str().as_encoded_bytes()),
            Self::Rclone { remote, path, .. } => {
                Sha256::digest(super::rclone::target(remote, path).as_bytes())
            }
        };
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn backend_options(&self) -> Result<BackendOptions, EngineError> {
        match self {
            Self::Local { path } => {
                let repository = path.to_str().map(str::to_owned).ok_or_else(|| {
                    EngineError::new(
                        ErrorKind::Io,
                        format!("{} is not valid UTF-8", path.display()),
                    )
                })?;
                Ok(BackendOptions::default().repository(repository))
            }
            Self::Rclone {
                remote,
                path,
                config,
                bandwidth_limit,
            } => {
                if !super::rclone::available() {
                    return Err(EngineError::new(ErrorKind::RcloneMissing, "rclone"));
                }
                // rustic starts `rclone serve restic` itself; this is how it
                // is told to use Stellarshot's configuration and nothing else,
                // and how rclone is tuned for backups (see `RCLONE_SERVE_FLAGS`).
                let mut command = format!(
                    "rclone serve restic --addr localhost:0 --config '{}' {}",
                    config.display(),
                    RCLONE_SERVE_FLAGS.join(" ")
                );
                if !bandwidth_limit.is_empty() {
                    // Single-quoted like `--config` above: rclone's own
                    // syntax (`1M`, `8M:2M`) never contains a quote itself.
                    command.push_str(&format!(" --bwlimit '{bandwidth_limit}'"));
                }
                let mut options = BTreeMap::new();
                options.insert("rclone-command".to_owned(), command);
                Ok(BackendOptions::default()
                    .repository(format!("rclone:{}", super::rclone::target(remote, path)))
                    .options(options))
            }
        }
    }

    /// Fail with `DestinationUnavailable` when a local location cannot be
    /// reached at all, as with an unplugged drive whose mount point has gone.
    /// Remote locations are checked by trying them.
    fn check_reachable(&self) -> Result<(), EngineError> {
        let Self::Local { path } = self else {
            return Ok(());
        };
        let reachable = path.exists() || path.parent().is_some_and(Path::exists);
        if reachable {
            Ok(())
        } else {
            Err(EngineError::new(
                ErrorKind::DestinationUnavailable,
                path.display().to_string(),
            ))
        }
    }
}

/// What a location holds, before anything is created or opened there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Probe {
    /// Missing or empty: a repository can be created here.
    Empty,
    /// Already a repository: open it.
    Repository,
    /// Holds other files: neither create nor open.
    NotEmpty,
}

/// Classify a location without touching it.
pub fn probe(location: &Location) -> Result<Probe, EngineError> {
    location.check_reachable()?;
    match location {
        Location::Local { path } => Ok(match check_init_location(path)? {
            InitCheck::Empty => Probe::Empty,
            InitCheck::ExistingRepository => Probe::Repository,
            InitCheck::NotEmpty => Probe::NotEmpty,
        }),
        Location::Rclone {
            remote,
            path,
            config,
            ..
        } => super::rclone::probe(config, remote, path),
    }
}

/// Delete the repository at `location`: only the entries the repository
/// format creates, never anything else there.
pub fn delete_repository(location: &Location) -> Result<(), EngineError> {
    match location {
        Location::Local { path } => {
            super::location::delete_repository(path)?;
            Ok(())
        }
        Location::Rclone {
            remote,
            path,
            config,
            ..
        } => super::rclone::delete_repository(config, remote, path),
    }
}

/// An open repository.
pub struct Repo {
    pub(crate) location: Location,
    pub(crate) bars: SinkBars,
    pub(crate) inner: Repository<OpenStatus>,
}

impl fmt::Debug for Repo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Repo")
            .field("location", &self.location)
            .finish()
    }
}

impl Repo {
    pub fn location(&self) -> &Location {
        &self.location
    }

    /// Route rustic's progress to `sink` until the guard is dropped.
    pub(crate) fn report_to(&self, sink: Arc<dyn ProgressSink>) -> SlotGuard {
        self.bars.slot.attach(sink)
    }
}

fn unopened(location: &Location, bars: &SinkBars) -> Result<Repository<()>, EngineError> {
    let mut backends = location.backend_options()?.to_backends()?;
    // Storage behind a network connection gets several uploads at once.
    if let Location::Rclone { .. } = location {
        let uploads = ParallelUploads::new(backends.repository(), bars.slot.clone());
        backends = RepositoryBackends::new(Arc::new(uploads), backends.repo_hot());
    }
    Ok(Repository::new_with_progress(
        &RepositoryOptions::default(),
        &backends,
        bars.clone(),
    )?)
}

/// Create a repository. Refuses a location that already holds a repository or
/// anything else.
pub fn init(location: &Location, secret: &Secret) -> Result<Repo, EngineError> {
    match probe(location)? {
        Probe::NotEmpty => {
            return Err(EngineError::new(
                ErrorKind::LocationNotEmpty,
                location.describe(),
            ));
        }
        Probe::Repository => {
            return Err(EngineError::new(
                ErrorKind::AlreadyExists,
                location.describe(),
            ));
        }
        Probe::Empty => {}
    }
    debug_log!(ENGINE, "init {}", location.describe());
    let bars = SinkBars::default();
    let inner = unopened(location, &bars)?.init(
        &Credentials::password(secret.expose()),
        &KeyOptions::default(),
        &ConfigOptions::default(),
    )?;
    Ok(Repo {
        location: location.clone(),
        bars,
        inner,
    })
}

/// Open an existing repository.
pub fn open(location: &Location, secret: &Secret) -> Result<Repo, EngineError> {
    location.check_reachable()?;
    if let Location::Local { path } = location
        && !is_repository(path)
    {
        return Err(EngineError::not_a_repository(path));
    }
    debug_log!(ENGINE, "open {}", location.describe());
    let bars = SinkBars::default();
    let inner = unopened(location, &bars)?.open(&Credentials::password(secret.expose()))?;
    Ok(Repo {
        location: location.clone(),
        bars,
        inner,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rclone_location(bandwidth_limit: &str) -> Location {
        Location::rclone(":local", "/tmp/somewhere").with_bandwidth_limit(bandwidth_limit)
    }

    #[test]
    fn a_bandwidth_limit_is_passed_to_rclone() {
        if !super::super::rclone::available() {
            return;
        }
        let options = rclone_location("1M").backend_options().unwrap();
        let command = options.options.get("rclone-command").unwrap();
        assert!(
            command.contains("--bwlimit '1M'"),
            "the command must carry the limit: {command}"
        );
    }

    #[test]
    fn no_bandwidth_limit_adds_no_flag() {
        if !super::super::rclone::available() {
            return;
        }
        let options = rclone_location("").backend_options().unwrap();
        let command = options.options.get("rclone-command").unwrap();
        assert!(
            !command.contains("--bwlimit"),
            "an empty limit must not add the flag: {command}"
        );
    }

    #[test]
    fn a_local_location_ignores_a_bandwidth_limit() {
        let location = Location::local("/tmp/somewhere").with_bandwidth_limit("1M");
        assert_eq!(location, Location::local("/tmp/somewhere"));
    }
}
