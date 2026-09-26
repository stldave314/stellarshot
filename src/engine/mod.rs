// SPDX-License-Identifier: GPL-3.0-only

//! The backup engine: the only part of Stellarshot that talks to rustic.
//!
//! Everything here is synchronous and returns plain types, so the rest of the
//! application never sees a `rustic_core` type and the engine can be tested
//! without a UI or an async runtime. Callers decide where it runs: reads on a
//! blocking thread, writes in a `--run` child process.

mod backup;
pub mod browse;
pub mod cache_settings;
pub mod disk_tree;
mod error;
mod estimate;
mod keys;
pub mod location;
pub mod lock;
mod maintenance;
pub mod mount;
pub mod progress;
pub mod rclone;
mod repo;
mod restore;
mod snapshots;
mod statistics;
mod uploads;

pub use backup::{BackupReport, BackupRequest};
pub use browse::{
    Browser, Change, DiffEntry, EntryKind, FileVersion, GlobalMatch, MissingEntry, MountEntry,
    TreeEntry,
};
pub use disk_tree::{DiskEntry, list_with_sizes};
pub use error::{EngineError, ErrorKind};
pub use estimate::{ExclusionBreakdown, SizeEstimate, estimate, exclusion_breakdown};
pub use keys::KeySummary;
pub use maintenance::{ForgetReport, KeepRules, PruneReport, hostname};
pub use progress::{NoProgress, Phase, ProgressEvent, ProgressSink};
pub use repo::{
    Location, Probe, Repo, Secret, delete_repository, init, init_with, open, probe, redact_url,
};
pub use restore::{ConflictPolicy, Ownership, RestorePreview, RestoreRequest, Target};
pub use snapshots::SnapshotSummary;
pub use statistics::Statistics;

#[cfg(test)]
mod tests;
