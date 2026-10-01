// SPDX-License-Identifier: GPL-3.0-only

//! Stellarshot: a backup application for the COSMIC desktop.
//!
//! The library holds everything; `main.rs` only chooses between the window
//! and a `--run` child process. Keeping it a library lets integration tests
//! drive the engine and the child process directly.

pub mod app;
pub(crate) mod bounded;
pub(crate) mod conditions;
pub(crate) mod constants;
pub(crate) mod core;
pub(crate) mod debug;
pub(crate) mod dejadup;
pub(crate) mod drives;
pub mod engine;
pub mod event_log;
pub(crate) mod exe;
pub(crate) mod hooks;
pub mod keyring;
pub mod notify;
pub(crate) mod password_command;
pub(crate) mod paths;
pub(crate) mod proc_signal;
pub mod profile;
pub mod run_state;
pub mod runner;
pub mod scheduled;
pub mod settings_export;
pub(crate) mod status;
pub(crate) mod timers;

/// Make this process's memory unreadable through a core dump, for a process
/// that holds a repository password (a `--run` child, a `--scheduled` run).
/// Best-effort: a failure (an old kernel without this `prctl`, say) is not
/// itself a reason to refuse to run a backup. Not for the window: the file
/// chooser and other portals identify a process through `/proc/<pid>`,
/// which a non-dumpable process hides from them.
pub fn harden_process() {
    let _ = rustix::process::set_dumpable_behavior(rustix::process::DumpableBehavior::NotDumpable);
}
