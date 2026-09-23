// SPDX-License-Identifier: GPL-3.0-only

//! Build a demo repository for screenshots: `scripts/screenshots.sh` runs
//! this. Creates a repository at `<repository>` and takes `<count>` real
//! snapshots of `<source>`, changing a file between them so each one has
//! something new in it.
//!
//! Usage: cargo run --example demo_repository -- <repository> <source> <count>
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
    let request = BackupRequest {
        sources: vec![source.clone()],
        excludes: vec![source.join(".cache"), source.join("Downloads")],
        exclude_patterns: vec!["node_modules".into()],
        one_file_system: true,
    };
    for round in 0..count {
        std::fs::write(
            source.join("Documents/notes.odt"),
            format!("notes, revision {round}"),
        )
        .expect("the demo source must be writable");
        engine::open(&location, &secret)
            .and_then(|repo| repo.backup(&request, Arc::new(NoProgress)))
            .expect("the backup failed");
    }
    println!("{count} snapshots in {repository}");
}
