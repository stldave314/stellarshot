// SPDX-License-Identifier: GPL-3.0-only

//! Errors the engine reports, typed by what the user can do about them.
//!
//! Every error crosses a process boundary (the `--run` child reports it as
//! JSON), so the kinds are a closed, serializable set. The UI maps each kind to
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
    /// The operation was canceled before it finished.
    Canceled,
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
    /// A change that needed the keyring could not reach or write to it.
    KeyringUnavailable,
    /// This kind of destination does not support deleting a repository's
    /// data from Stellarshot: see `Location::delete_repository`'s `Rest` arm.
    DeleteUnsupported,
    /// The storage did not answer in time. The detail is the limit, in
    /// seconds.
    TimedOut,
    /// A scheduled backup's conditions (power, battery, network) were not
    /// met. The detail says which.
    ConditionsNotMet,
    /// A `Before` hook failed, so the backup did not run. The detail names
    /// the hook and, if there was one, its own error.
    HookFailed,
    /// The running app's own executable was replaced (an update installed
    /// while it kept running), so it can no longer spawn the child process
    /// an operation needs.
    AppUpdated,
    /// An rclone remote outside the shape Stellarshot itself creates, or
    /// missing from Stellarshot's own rclone configuration: see
    /// `profile::valid_rclone_remote`.
    InvalidRemote,
    /// A snapshot names a file outside the folder being restored into (a
    /// `..` component, or an absolute path): see `restore::restore_one`'s
    /// validation of every item `repo.ls` yields. The detail is the
    /// offending path as recorded in the snapshot.
    UnsafePath,
    /// A snapshot ID, or a path inside one, that this repository does not
    /// have: see `browse::not_found`. The detail is the missing ID or path.
    NotFound,
    /// A snapshot prefix that matches more than one snapshot: see
    /// `browse::Browser::snapshot`. The detail is the ambiguous prefix.
    Ambiguous,
    /// The web interface already has as many requests open on this
    /// backup's repository as it will allow at once: see
    /// `web::routes::REPOSITORY_PERMITS`.
    TooBusy,
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
        // rustic_backend's REST client (`reqwest`) can echo the URL it was
        // trying to reach back into its own error text, credentials and
        // all, for a location reached directly rather than through rclone
        // (see SEC-3 in the review plan). Scrubbed unconditionally rather
        // than only for a `Location::Rest`, since this conversion has no
        // access to which location an error came from, and scrubbing a
        // message with nothing to redact is a no-op either way.
        Self::new(kind, super::repo::scrub_url_credentials(&err.to_string()))
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
