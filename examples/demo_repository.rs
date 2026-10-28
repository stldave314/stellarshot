// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! Build a demo repository for screenshots: `scripts/screenshots.sh` runs
//! this. Creates a repository at `<repository>` and takes `<count>` real
//! snapshots of `<source>`, changing a file between them so each one has
//! something new in it. They are dated a day apart, ending two hours ago, so
//! the history looks like a backup that has been running for a while.
//!
//! Usage: cargo run --example `demo_repository` -- <repository> <source> <count>
//! The password is read from the `DEMO_PASSWORD` environment variable.

use std::path::PathBuf;
use std::sync::Arc;

use stellarshot::engine::{self, BackupRequest, Location, NoProgress, Secret};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [repository, source, count] = args.as_slice() else {
        eprintln!("usage: demo_repository <repository> <source> <count>");
        std::process::exit(2);
    };
    let count: usize = count.parse().expect("count must be a number");
    let password = std::env::var("DEMO_PASSWORD").expect("DEMO_PASSWORD must be set");

    let location = Location::local(repository);
    let secret = Secret::new(password);
    engine::init(&location, &secret).expect("the repository could not be created");

    let source = PathBuf::from(source);
    let mut request = BackupRequest {
        sources: vec![source.clone()],
        excludes: vec![source.join(".cache"), source.join("Downloads")],
        exclude_patterns: vec!["node_modules".into()],
        one_file_system: true,
        ..BackupRequest::default()
    };
    let now = jiff::Timestamp::now().as_second();
    for round in 0..count {
        let days_ago = (count - 1 - round) as i64;
        request.time = Some(now - 2 * 3600 - days_ago * 86_400);
        // A new version each time, growing and dated to its own snapshot,
        // so a search across them shows each one apart.
        let notes = source.join("Documents/notes.odt");
        std::fs::write(
            &notes,
            format!("notes, revision {round}\n").repeat(round + 1),
        )
        .expect("the demo source must be writable");
        let edited = std::time::UNIX_EPOCH
            + std::time::Duration::from_secs((now - 3 * 3600 - days_ago * 86_400).cast_unsigned());
        std::fs::File::options()
            .write(true)
            .open(&notes)
            .and_then(|file| file.set_modified(edited))
            .expect("the demo source must be writable");
        engine::open(&location, &secret)
            .and_then(|repo| repo.backup(&request, Arc::new(NoProgress)))
            .expect("the backup failed");
    }
    println!("{count} snapshots in {repository}");
}
