// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Rustic error: {0}")]
    Rustic(#[from] rustic_core::RusticError),
    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
    /// The chosen folder already holds files that are not a repository.
    #[error("{} already contains other files", .0.display())]
    LocationNotEmpty(PathBuf),
    /// The path cannot be passed to the backend because it is not valid UTF-8.
    #[error("{} is not a valid UTF-8 path", .0.display())]
    NonUtf8Path(PathBuf),
}
