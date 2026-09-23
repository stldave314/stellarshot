// SPDX-License-Identifier: GPL-3.0-only

//! Opening and creating repositories.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rustic_backend::BackendOptions;
use rustic_core::{
    ConfigOptions, Credentials, KeyOptions, OpenStatus, Repository, RepositoryOptions,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::{EngineError, ErrorKind};
use super::location::{InitCheck, check_init_location, is_repository};
use super::progress::{ProgressSink, SinkBars, SlotGuard};
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
pub struct Location {
    path: PathBuf,
}

impl Location {
    /// A repository in a local folder.
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A short stable identifier, used to name the lock and progress files
    /// every process writing to this repository shares.
    pub fn key(&self) -> String {
        let digest = Sha256::digest(self.path.as_os_str().as_encoded_bytes());
        digest[..8]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    fn backend_string(&self) -> Result<String, EngineError> {
        self.path.to_str().map(str::to_owned).ok_or_else(|| {
            EngineError::new(
                ErrorKind::Io,
                format!("{} is not valid UTF-8", self.path.display()),
            )
        })
    }

    /// Fail with `DestinationUnavailable` when the place the repository should
    /// be cannot be reached at all, as with an unplugged drive whose mount
    /// point has gone.
    fn check_reachable(&self) -> Result<(), EngineError> {
        let reachable = self.path.exists() || self.path.parent().is_some_and(Path::exists);
        if reachable {
            Ok(())
        } else {
            Err(EngineError::new(
                ErrorKind::DestinationUnavailable,
                self.path.display().to_string(),
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
    Ok(match check_init_location(location.path())? {
        InitCheck::Empty => Probe::Empty,
        InitCheck::ExistingRepository => Probe::Repository,
        InitCheck::NotEmpty => Probe::NotEmpty,
    })
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
    let backends = BackendOptions::default()
        .repository(location.backend_string()?)
        .to_backends()?;
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
        Probe::NotEmpty => return Err(EngineError::location_not_empty(location.path())),
        Probe::Repository => {
            return Err(EngineError::new(
                ErrorKind::AlreadyExists,
                location.path().display().to_string(),
            ));
        }
        Probe::Empty => {}
    }
    debug_log!(ENGINE, "init {}", location.path().display());
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
    if !is_repository(location.path()) {
        return Err(EngineError::not_a_repository(location.path()));
    }
    debug_log!(ENGINE, "open {}", location.path().display());
    let bars = SinkBars::default();
    let inner = unopened(location, &bars)?.open(&Credentials::password(secret.expose()))?;
    Ok(Repo {
        location: location.clone(),
        bars,
        inner,
    })
}
