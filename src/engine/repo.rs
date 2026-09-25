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
    /// A repository on a rest-server or rustic-server, reached directly
    /// (not through rclone). `url` is the full server URL including the
    /// repository name and any HTTP basic auth (`http://user:pass@host:port/repo/`).
    Rest { url: String },
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
            Self::Rclone { .. } | Self::Rest { .. } => None,
        }
    }

    /// How the location reads in messages. Never includes a REST URL's own
    /// embedded credentials, if it has any.
    pub fn describe(&self) -> String {
        match self {
            Self::Local { path } => path.display().to_string(),
            Self::Rclone { remote, path, .. } => super::rclone::target(remote, path),
            Self::Rest { url } => redact_url(url),
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
            Self::Rest { url } => Sha256::digest(url.as_bytes()),
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
            Self::Rest { url } => Ok(BackendOptions::default().repository(format!("rest:{url}"))),
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

/// `url` with any embedded HTTP basic auth (`user:pass@`) removed, for
/// showing in messages and logs. Returned unchanged if it does not parse as
/// a URL at all, rather than hiding a typo.
fn redact_url(url: &str) -> String {
    let Ok(mut parsed) = url::Url::parse(url) else {
        return url.to_owned();
    };
    if parsed.username().is_empty() && parsed.password().is_none() {
        return url.to_owned();
    }
    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.to_string()
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
        // No generic way to list a REST server path's contents the way a
        // filesystem walk or `rclone lsf` does, so "holds other files, not
        // a repository" is not distinguished here: only whether a `config`
        // file already exists.
        Location::Rest { .. } => {
            let bars = SinkBars::default();
            match unopened(location, &bars)?.config_id()? {
                Some(_) => Ok(Probe::Repository),
                None => Ok(Probe::Empty),
            }
        }
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
        // Deleting only the repository's own entries, never anything else
        // at that path, needs a generic directory listing; rustic_core
        // exposes no public API for that against a REST server. "Remove
        // from Stellarshot" (forgetting it here, leaving the data) still
        // works; only wiping it from here does not.
        Location::Rest { .. } => Err(EngineError::new(
            ErrorKind::Internal,
            "Stellarshot cannot delete a REST server repository's data; remove it on the server, or use Remove from Stellarshot to forget it here",
        )),
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

    /// Whether this repository is append-only: `forget` and `prune` refuse
    /// to run against it, the same as rustic's own tools, rather than
    /// failing against the server on every attempt.
    pub fn is_append_only(&self) -> bool {
        self.inner.config().append_only == Some(true)
    }
}

fn unopened(location: &Location, bars: &SinkBars) -> Result<Repository<()>, EngineError> {
    let mut backends = location.backend_options()?.to_backends()?;
    // Storage behind a network connection gets several uploads at once.
    if let Location::Rclone { .. } = location {
        let uploads = ParallelUploads::new(backends.repository(), bars.slot.clone());
        backends = RepositoryBackends::new(Arc::new(uploads), backends.repo_hot());
    }
    let mut options = RepositoryOptions::default();
    super::cache_settings::apply(&mut options);
    Ok(Repository::new_with_progress(
        &options,
        &backends,
        bars.clone(),
    )?)
}

/// Create a repository. Refuses a location that already holds a repository or
/// anything else.
pub fn init(location: &Location, secret: &Secret) -> Result<Repo, EngineError> {
    init_with(location, secret, false)
}

/// Create a repository, optionally in append-only mode from the start.
///
/// Append-only cannot be turned on later through this project's own tools:
/// rustic's `config` command, the only way to change it, itself stops
/// working once it is set (append-only commands are all that work any
/// longer), so it can only be chosen here, at creation.
pub fn init_with(
    location: &Location,
    secret: &Secret,
    append_only: bool,
) -> Result<Repo, EngineError> {
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
    debug_log!(
        ENGINE,
        "init {} (append-only: {append_only})",
        location.describe()
    );
    let bars = SinkBars::default();
    let config = if append_only {
        ConfigOptions::default().set_append_only(true)
    } else {
        ConfigOptions::default()
    };
    let inner = unopened(location, &bars)?.init(
        &Credentials::password(secret.expose()),
        &KeyOptions::default(),
        &config,
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
