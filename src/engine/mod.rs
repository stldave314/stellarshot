// SPDX-License-Identifier: GPL-3.0-only

//! The backup engine: the only part of Stellarshot that talks to rustic.
//!
//! Everything here is synchronous and returns plain types, so the rest of the
//! application never sees a `rustic_core` type and the engine can be tested
//! without a UI or an async runtime. Callers decide where it runs: reads on a
//! blocking thread, writes in a `--run` child process.

mod backup;
mod error;
pub mod location;
pub mod lock;
mod maintenance;
pub mod progress;
mod repo;
mod restore;
mod snapshots;

pub use backup::{BackupReport, BackupRequest};
pub use error::{EngineError, ErrorKind};
pub use progress::{NoProgress, Phase, ProgressEvent, ProgressSink};
pub use repo::{Location, Probe, Repo, Secret, init, open, probe};
pub use snapshots::SnapshotSummary;

#[cfg(test)]
mod tests;
