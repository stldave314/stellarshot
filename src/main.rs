// SPDX-License-Identifier: GPL-3.0-only

use std::process::ExitCode;

use stellarshot::app::{App, settings};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--run") => return stellarshot::runner::main(&args[1..]),
        Some("--scheduled") => return stellarshot::scheduled::main(&args[1..]),
        _ => {}
    }

    let (settings, mut flags) = settings::init();
    // `--new-backup` opens the setup wizard straight away: the desktop
    // entry's "New Backup" action.
    flags.start_wizard = args.iter().any(|arg| arg == "--new-backup");
    // `--restore` opens the restore page for the selected backup once it is
    // unlocked: the "Restore Files" action.
    flags.start_restore = args.iter().any(|arg| arg == "--restore");
    // `--profile <id>` selects a backup: a scheduled run's failure
    // notification opens it this way.
    flags.select = args
        .iter()
        .position(|arg| arg == "--profile")
        .and_then(|index| args.get(index + 1))
        .cloned();
    match cosmic::app::run_single_instance::<App>(settings, flags) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("stellarshot: {err}");
            ExitCode::FAILURE
        }
    }
}
