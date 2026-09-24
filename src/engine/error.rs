// SPDX-License-Identifier: GPL-3.0-only

//! Errors the engine reports, typed by what the user can do about them.
//!
//! Every error crosses a process boundary (the `--run` child reports it as
//! JSON), so the kinds are a closed, serialisable set. The UI maps each kind to
//! a localized message; `detail` carries whatever technical text rustic gave,
//! for the "Details" line.

use std::path::PathBuf;

use rustic_core::RusticError;
use serde::{Deserialize, Serialize};

/// What went wrong, as far as the user is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    /// The password does not open the repository.
    WrongPassword,
    /// The location holds no repository.
    NotARepository,
    /// A repository already exists where one was to be created.
    AlreadyExists,
    /// The location holds other files, so no repository may be created there.
    LocationNotEmpty,
    /// The storage could not be reached: a missing drive, a dropped network.
    DestinationUnavailable,
    /// Another process is already writing to this repository.
    Locked,
    /// The operation was cancelled before it finished.
    Cancelled,
    /// An integrity check found problems.
    RepositoryDamaged,
    /// Reading or writing local files failed.
    Io,
    /// rclone is needed for this location and is not installed.
    RcloneMissing,
    /// Signing in to a cloud account did not complete.
    AuthFailed,
    /// A scheduled backup found no remembered password to open the
    /// repository with.
    PasswordNotRemembered,
    /// The storage did not answer in time. The detail is the limit, in
    /// seconds.
    TimedOut,
    /// Anything else; the detail is the only explanation available.
    Internal,
}

/// An engine failure: a kind for the UI to act on, and technical detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{kind:?}: {detail}")]
pub struct EngineError {
    pub kind: ErrorKind,
    pub detail: String,
}

impl EngineError {
    pub fn new(kind: ErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    pub fn location_not_empty(path: &std::path::Path) -> Self {
        Self::new(ErrorKind::LocationNotEmpty, path.display().to_string())
    }

    pub fn not_a_repository(path: &std::path::Path) -> Self {
        Self::new(ErrorKind::NotARepository, path.display().to_string())
    }

    /// The path this error is about, for errors that carry one.
    pub fn path(&self) -> Option<PathBuf> {
        matches!(
            self.kind,
            ErrorKind::LocationNotEmpty | ErrorKind::NotARepository | ErrorKind::AlreadyExists
        )
        .then(|| PathBuf::from(&self.detail))
    }
}

impl From<RusticError> for EngineError {
    /// rustic exposes no stable error kind, only error codes, and a wrong
    /// password is the one code worth acting on. Whether the destination is
    /// reachable is checked explicitly before a repository is opened, rather
    /// than guessed from error text.
    fn from(err: RusticError) -> Self {
        let kind = if err.is_incorrect_password() {
            ErrorKind::WrongPassword
        } else {
            ErrorKind::Internal
        };
        Self::new(kind, err.to_string())
    }
}

impl From<Box<RusticError>> for EngineError {
    fn from(err: Box<RusticError>) -> Self {
        Self::from(*err)
    }
}

impl From<std::io::Error> for EngineError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, err.to_string())
    }
}
