// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! A real round trip against a REST server, the destination added in 0.4:
//! `Location::Rest` reached directly, without Stellarshot starting rclone
//! itself. The server here is `rclone serve restic`, which speaks the same
//! protocol as rest-server and is already a dependency of the rclone
//! destinations, so the tests need nothing installed that the app does not.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use stellarshot::engine::{self, BackupRequest, Location, NoProgress, Secret};
use tempfile::TempDir;

/// One server at a time, so the tests do not compete for the CPU on a busy
/// machine; held for a whole test's real work.
static ONLY_ONE_SERVER_AT_A_TIME: Mutex<()> = Mutex::new(());

/// What rclone prints on stderr once it is listening, followed by its
/// address.
const READY_MARKER: &str = "Serving restic REST API on ";

fn require_rclone() {
    assert!(
        Command::new("rclone").arg("version").output().is_ok(),
        "rclone must be installed for these tests"
    );
}

struct Server {
    child: Child,
    /// The server's root, ending in `/`.
    url: String,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts a real REST server over `data_dir`, on a port the OS picks, and
/// waits until it says it is listening. The address is read from its own
/// log, so no port is picked here and then raced for.
fn spawn_server(data_dir: &Path, login: Option<(&str, &str)>) -> Server {
    std::fs::create_dir_all(data_dir).unwrap();
    let mut command = Command::new("rclone");
    command.args([
        "serve",
        "restic",
        "--addr",
        "127.0.0.1:0",
        "--config",
        "/dev/null",
    ]);
    if let Some((user, pass)) = login {
        command.args(["--user", user, "--pass", pass]);
    }
    let mut child = command
        .arg(data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = child.stderr.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    // Keeps draining after the address is found, echoing every line, so a
    // failed request has the server's side of it in the test output.
    std::thread::spawn(move || {
        let mut sender = Some(sender);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("rclone: {line}");
            if let Some(at) = line.find(READY_MARKER) {
                let url = line[at + READY_MARKER.len()..].trim().to_owned();
                if let Some(sender) = sender.take() {
                    let _ = sender.send(url);
                }
            }
        }
    });
    let mut server = Server {
        child,
        url: String::new(),
    };
    match receiver.recv_timeout(Duration::from_secs(30)) {
        Ok(url) if url.ends_with('/') => server.url = url,
        Ok(url) => server.url = format!("{url}/"),
        Err(_) => panic!("rclone serve restic did not start listening in time"),
    }
    server
}

fn secret() -> Secret {
    Secret::new("correct horse battery staple")
}

/// Runs `scenario` against a freshly spawned server, with the server's
/// root URL.
fn with_server(scenario: impl FnOnce(&Path, &str)) {
    require_rclone();
    let _guard = ONLY_ONE_SERVER_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let scratch = TempDir::new().unwrap();
    let server = spawn_server(&scratch.path().join("data"), None);
    scenario(scratch.path(), &server.url);
}

#[test]
fn backup_and_restore_round_trip_through_a_rest_server() {
    with_server(|scratch, url| {
        let location = Location::Rest {
            url: format!("{url}test-repo/"),
        };

        engine::init(&location, &secret()).unwrap();

        let source = scratch.join("source");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("report.txt"), b"quarterly numbers").unwrap();

        let request = BackupRequest {
            sources: vec![source.clone()],
            ..BackupRequest::default()
        };
        let report = engine::open(&location, &secret())
            .unwrap()
            .backup(&request, Arc::new(NoProgress))
            .unwrap();
        assert!(report.snapshot.files_new >= 1);

        let destination = scratch.join("restore");
        engine::open(&location, &secret())
            .unwrap()
            .restore_all("latest", &destination, Arc::new(NoProgress))
            .unwrap();

        let restored = destination.join(source.strip_prefix("/").unwrap_or(&source));
        assert_eq!(
            std::fs::read(restored.join("report.txt")).unwrap(),
            b"quarterly numbers"
        );
    });
}

#[test]
fn probe_finds_an_empty_location_then_the_repository_once_created() {
    with_server(|_scratch, url| {
        let location = Location::Rest {
            url: format!("{url}probe-repo/"),
        };

        assert_eq!(engine::probe(&location).unwrap(), engine::Probe::Empty);
        engine::init(&location, &secret()).unwrap();
        assert_eq!(engine::probe(&location).unwrap(), engine::Probe::Repository);
    });
}

#[test]
fn deleting_a_rest_repository_is_refused_rather_than_attempted() {
    with_server(|_scratch, url| {
        let location = Location::Rest {
            url: format!("{url}delete-repo/"),
        };
        engine::init(&location, &secret()).unwrap();

        let err = engine::delete_repository(&location).unwrap_err();

        assert_eq!(err.kind, engine::ErrorKind::DeleteUnsupported);
        // The repository itself must be untouched: still there afterwards.
        assert_eq!(engine::probe(&location).unwrap(), engine::Probe::Repository);
    });
}

/// A REST server password typed into the address is moved to the keyring,
/// the saved address keeps only the user name, and the backup still logs
/// in. Needs a running, unlocked Secret Service, like `tests/keyring.rs`.
#[test]
fn a_rest_password_moves_to_the_keyring_and_still_logs_in() {
    use stellarshot::profile::{self, Destination, Profile};

    struct Forget<'a>(&'a tokio::runtime::Runtime, String);
    impl Drop for Forget<'_> {
        fn drop(&mut self) {
            let _ = self.0.block_on(profile::forget_rest_password(&self.1));
        }
    }

    require_rclone();
    let _guard = ONLY_ONE_SERVER_AT_A_TIME
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let scratch = TempDir::new().unwrap();
    let server = spawn_server(&scratch.path().join("data"), Some(("alice", "p@ss/w")));
    let typed = server
        .url
        .replacen("http://", "http://alice:p%40ss%2Fw@", 1)
        + "login-repo/";
    let mut backup = Profile::new(
        "REST login test".into(),
        Destination::Rest { url: typed },
        Vec::new(),
    );
    backup.id = uuid::Uuid::new_v4().to_string();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let _cleanup = Forget(&runtime, backup.id.clone());

    let moved = runtime.block_on(profile::secure_rest_passwords(vec![backup.clone()]));
    let saved = server.url.replacen("http://", "http://alice@", 1) + "login-repo/";
    assert_eq!(moved, vec![(backup.id.clone(), saved.clone())]);
    backup.destination = Destination::Rest { url: saved };

    // Read back from the keyring, as another process would.
    runtime
        .block_on(profile::load_rest_password(&backup))
        .expect("a Secret Service must be running and unlocked for this test");
    let location = backup.location().unwrap();
    engine::init(&location, &secret()).unwrap();
    assert_eq!(engine::probe(&location).unwrap(), engine::Probe::Repository);

    // Without it, the server turns the login away: the password is really
    // what let the backup in.
    let Location::Rest { url } = &location else {
        panic!("not a REST location");
    };
    let without = Location::Rest {
        url: url.replacen("alice:p%40ss%2Fw@", "alice@", 1),
    };
    assert!(engine::open(&without, &secret()).is_err());
}
