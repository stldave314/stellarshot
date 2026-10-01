// SPDX-License-Identifier: GPL-3.0-only

use std::process::ExitCode;

use stellarshot::app::{App, Launch, startup};

fn main() -> ExitCode {
    // `args()` panics outright on a non-UTF-8 argument; `args_os()` never
    // does, and every flag matched below is plain ASCII, so converting
    // lossily costs nothing real — even a profile ID that happened to
    // contain non-UTF-8 bytes only ever affects which page opens first, not
    // anything that reads a repository or a file.
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    match args.first().map(String::as_str) {
        Some("--run") => return stellarshot::runner::main(&args[1..]),
        Some("--scheduled") => return stellarshot::scheduled::main(&args[1..]),
        Some("--await-notification") => return stellarshot::notify::await_main(&args[1..]),
        _ => {}
    }

    let (settings, mut flags) = startup::init();
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
    // What an already-running instance is told to do instead of a second
    // window: see `Launch`'s own doc comment.
    flags.launch = Launch::from_flags(flags.start_wizard, flags.start_restore, &flags.select);
    match cosmic::app::run_single_instance::<App>(settings, flags) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("stellarshot: {err}");
            ExitCode::FAILURE
        }
    }
}
