// SPDX-License-Identifier: GPL-3.0-only

//! Stellarshot: a backup application for the COSMIC desktop.
//!
//! The library holds everything; `main.rs` only chooses between the window
//! and a `--run` child process. Keeping it a library lets integration tests
//! drive the engine and the child process directly.

pub mod app;
pub mod constants;
pub mod core;
pub mod debug;
pub mod dejadup;
pub mod drives;
pub mod engine;
pub mod keyring;
pub mod notify;
pub mod profile;
pub mod run_state;
pub mod runner;
pub mod schedule;
pub mod scheduled;
