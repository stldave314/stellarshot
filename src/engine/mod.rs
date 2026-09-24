// SPDX-License-Identifier: GPL-3.0-only

//! The backup engine: the only part of Stellarshot that talks to rustic.
//!
//! Everything here is synchronous and returns plain types, so the rest of the
//! application never sees a `rustic_core` type and the engine can be tested
//! without a UI or an async runtime. Callers decide where it runs: reads on a
//! blocking thread, writes in a `--run` child process.

mod backup;
pub mod browse;
mod error;
mod estimate;
pub mod location;
pub mod lock;
mod maintenance;
pub mod progress;
pub mod rclone;
mod repo;
mod restore;
mod snapshots;
mod uploads;

pub use backup::{BackupReport, BackupRequest};
pub use browse::{Browser, Change, DiffEntry, EntryKind, FileVersion, MissingEntry, TreeEntry};
pub use error::{EngineError, ErrorKind};
pub use estimate::{ExclusionBreakdown, SizeEstimate, estimate, exclusion_breakdown};
pub use maintenance::{ForgetReport, KeepRules, PruneReport, hostname};
pub use progress::{NoProgress, Phase, ProgressEvent, ProgressSink};
pub use repo::{Location, Probe, Repo, Secret, delete_repository, init, open, probe};
pub use restore::{ConflictPolicy, RestorePreview, RestoreRequest, Target};
pub use snapshots::SnapshotSummary;

#[cfg(test)]
mod tests;
