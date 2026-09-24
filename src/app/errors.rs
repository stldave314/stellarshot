// SPDX-License-Identifier: GPL-3.0-only

//! What to tell the user when something fails.

use crate::engine::{EngineError, ErrorKind};
use crate::fl;

/// A localized explanation of `error`. `context` says what was being
/// attempted ("The snapshot could not be created."). Kinds with a clear cause
/// get their own sentence; the rest keep rustic's detail, which cannot be
/// translated, under a localized heading.
pub fn describe(context: &str, error: &EngineError) -> String {
    match error.kind {
        ErrorKind::Cancelled => explain(error),
        _ => format!("{context}\n\n{}", explain(error)),
    }
}

/// The localized explanation of `error` on its own, without saying what was
/// being attempted.
pub fn explain(error: &EngineError) -> String {
    let path = || {
        error
            .path()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| error.detail.clone())
    };
    match error.kind {
        ErrorKind::WrongPassword => fl!("error-wrong-password"),
        ErrorKind::NotARepository => fl!("error-not-a-repository", path = path()),
        ErrorKind::AlreadyExists => fl!("error-already-exists", path = path()),
        ErrorKind::LocationNotEmpty => fl!("location-not-empty", path = path()),
        ErrorKind::DestinationUnavailable => {
            fl!("error-destination-unavailable", path = error.detail.clone())
        }
        ErrorKind::Locked => fl!("error-locked"),
        ErrorKind::Cancelled => fl!("error-cancelled"),
        ErrorKind::RepositoryDamaged => fl!("error-repository-damaged"),
        ErrorKind::RcloneMissing => fl!("error-rclone-missing"),
        ErrorKind::PasswordNotRemembered => fl!("error-password-not-remembered"),
        ErrorKind::AuthFailed => fl!("error-auth-failed", details = error.detail.clone()),
        ErrorKind::TimedOut => fl!("error-timed-out", seconds = error.detail.clone()),
        ErrorKind::Io | ErrorKind::Internal => {
            fl!("error-details", details = error.detail.clone())
        }
    }
}
