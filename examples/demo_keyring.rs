// SPDX-License-Identifier: GPL-3.0-only

//! Store or forget a demo profile's password in the keyring, through
//! Stellarshot's own keyring module: `scripts/screenshots.sh` runs this so
//! the demo profile opens unlocked, then removes the entry again.
//!
//! Usage: cargo run --example demo_keyring -- store <profile-id>
//!        cargo run --example demo_keyring -- forget <profile-id>
//! `store` reads the password from the `DEMO_PASSWORD` environment variable.

use stellarshot::engine::Secret;
use stellarshot::keyring;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
        _ => {
            eprintln!("usage: demo_keyring store|forget <profile-id>");
            std::process::exit(2);
        }
    };
    if let Err(err) = result {
        eprintln!("keyring: {err}");
        std::process::exit(1);
    }
}
