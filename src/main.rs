// SPDX-License-Identifier: GPL-3.0-only

use std::process::ExitCode;

use stellarshot::app::{App, settings};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--run") {
        return stellarshot::runner::main(&args[1..]);
    }

    let (settings, flags) = settings::init();
    match cosmic::app::run::<App>(settings, flags) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("stellarshot: {err}");
            ExitCode::FAILURE
        }
    }
}
