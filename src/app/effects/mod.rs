// SPDX-License-Identifier: GPL-3.0-only

//! What the pages and the wizard ask for, carried out: each `run_*_effects`
//! turns the effects a page's `update` returned into tasks, and feeds their
//! results back as messages.

mod dialog;
mod profile;
mod restore;
mod settings;
mod wizard;
