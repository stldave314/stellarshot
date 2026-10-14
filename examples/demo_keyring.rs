// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! Store, look for or forget a demo profile's password in the keyring,
//! through Stellarshot's own keyring module. `scripts/screenshots.sh` runs
//! this so the demo profile opens unlocked, then removes the entry again;
//! `scripts/verify-locked-keyring.sh` runs it against a locked keyring.
//!
//! Usage: cargo run --example `demo_keyring` -- [--may-prompt] store <profile-id>
//!        cargo run --example `demo_keyring` -- [--may-prompt] load <profile-id>
//!        cargo run --example `demo_keyring` -- [--may-prompt] forget <profile-id>
//!        cargo run --example `demo_keyring` -- lock | locked
//! `store` reads the password from the `DEMO_PASSWORD` environment variable.
//! `--may-prompt` behaves as the window does: a locked keyring is asked to
//! unlock, which shows its prompt. Without it, as a scheduled run does.

use stellarshot::engine::Secret;
use stellarshot::keyring;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // As the window does: a locked keyring may be asked to unlock, which
    // shows its prompt. Without it this behaves as a scheduled run would.
    if args.first().is_some_and(|arg| arg == "--may-prompt") {
        keyring::allow_unlock_prompt();
        args.remove(0);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    let result = match args.as_slice() {
        [command, profile] if command == "store" => {
            let password = std::env::var("DEMO_PASSWORD").expect("DEMO_PASSWORD must be set");
            runtime.block_on(keyring::store(
                profile,
                "Screenshot demo",
                &Secret::new(password),
            ))
        }
        [command, profile] if command == "forget" => runtime.block_on(keyring::forget(profile)),
        // Says whether one is remembered, never what it is.
        [command, profile] if command == "load" => runtime
            .block_on(keyring::load_checked(profile))
            .map(|found| {
                let answer = if found.is_some() {
                    "remembered"
                } else {
                    "none"
                };
                println!("{answer}");
            }),
        // For `scripts/verify-locked-keyring.sh`, against its own private
        // keyring only: lock the default collection, or say whether it is.
        [command] if command == "lock" => runtime.block_on(async {
            let keyring = oo7::Keyring::new().await.map_err(|err| err.to_string())?;
            keyring.lock().await.map_err(|err| err.to_string())
        }),
        [command] if command == "locked" => runtime.block_on(async {
            let keyring = oo7::Keyring::new().await.map_err(|err| err.to_string())?;
            let locked = keyring.is_locked().await.map_err(|err| err.to_string())?;
            println!("{}", if locked { "locked" } else { "unlocked" });
            Ok(())
        }),
        _ => {
            eprintln!(
                "usage: demo_keyring [--may-prompt] store|load|forget <profile-id>\n       demo_keyring lock|locked"
            );
            std::process::exit(2);
        }
    };
    if let Err(err) = result {
        eprintln!("keyring: {err}");
        std::process::exit(1);
    }
}
