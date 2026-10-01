// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! What a scheduled run does when it cannot run: no remembered password, or
//! a wrong one. The promise is that such a backup "does not run, and says
//! so" (SECURITY.md): a recorded failure, no snapshot, and an exit, rather
//! than a backup made with an empty password or a run that hangs.
//!
//! These run against a private D-Bus session bus with nothing on it: no
//! Secret Service and no notification daemon. That is what makes them safe
//! to run anywhere (a failing run reaches `notify::failure`, which must never
//! reach a real desktop's notification daemon; see the note in
//! `tests/scheduled.rs`) and needs no unlocked keyring. The notification call
//! has nobody to answer it, so it has to fail fast, which is part of what is
//! checked: the run must be over well inside [`DEADLINE`].

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use stellarshot::engine::{self, Location, Secret};
use tempfile::TempDir;

const APP_ID: &str = "io.github.stldave314.Stellarshot";
const PASSWORD: &str = "correct horse battery staple";
const DEADLINE: Duration = Duration::from_secs(30);

/// A session bus with no services and none that can be started.
struct PrivateBus {
    daemon: Child,
    address: String,
    _dir: TempDir,
}

impl PrivateBus {
    fn start() -> Self {
        let dir = TempDir::new().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                dir.path().join("bus").display()
            ),
        )
        .unwrap();
        let mut daemon = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--print-address=1", "--nofork"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon must be installed for these tests");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(address.starts_with("unix:"), "no bus address: {address:?}");
        Self {
            daemon,
            address: address.trim().to_owned(),
            _dir: dir,
        }
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

struct Home {
    dir: TempDir,
    repository: PathBuf,
    source: PathBuf,
}

impl Home {
    fn new() -> Self {
        let dir = TempDir::new().unwrap();
        for sub in ["config", "state", "runtime", "source"] {
            std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        }
        std::fs::write(dir.path().join("source/file.txt"), b"contents").unwrap();
        let repository = dir.path().join("repository");
        engine::init(&Location::local(&repository), &Secret::new(PASSWORD)).unwrap();
        let source = dir.path().join("source");
        Self {
            dir,
            repository,
            source,
        }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub)
    }

    /// One daily profile; `password_command` empty means "from the keyring".
    fn save_profile(&self, id: &str, password_command: &str) {
        let settings = self.path("config").join(format!("cosmic/{APP_ID}/v2"));
        std::fs::create_dir_all(&settings).unwrap();
        std::fs::write(
            settings.join("profiles"),
            format!(
                r#"[
    (
        id: "{id}",
        name: "Scheduled failure test",
        destination: Local(path: "{}"),
        sources: ["{}"],
        schedule: Daily,
        retention: Smart,
        password_command: "{password_command}",
    ),
]"#,
                self.repository.display(),
                self.source.display()
            ),
        )
        .unwrap();
    }

    /// Run `--scheduled <id>` against `bus`, and wait for it for at most
    /// [`DEADLINE`]: a hung run is a failed test, not a stuck suite.
    fn run(&self, id: &str, bus: &PrivateBus) -> std::process::ExitStatus {
        let mut child = Command::new(env!("CARGO_BIN_EXE_stellarshot"))
            .args(["--scheduled", id])
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            // Nothing here needs the system bus; make sure nothing finds it.
            .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                return status;
            }
            if started.elapsed() > DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the scheduled run did not finish within {DEADLINE:?}");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn run_state(&self, id: &str) -> String {
        read(
            &self
                .path("state")
                .join(format!("cosmic/{APP_ID}/v2/run-{id}")),
        )
    }

    fn event_log(&self, id: &str) -> String {
        read(
            &self
                .path("state")
                .join(format!("cosmic/{APP_ID}/v2/event-log-{id}")),
        )
    }

    fn snapshots(&self) -> usize {
        std::fs::read_dir(self.repository.join("snapshots"))
            .map(|entries| entries.count())
            .unwrap_or(0)
    }
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn a_backup_with_no_remembered_password_does_not_run_and_says_so() {
    let bus = PrivateBus::start();
    let home = Home::new();
    let id = uuid::Uuid::new_v4().to_string();
    home.save_profile(&id, "");

    let status = home.run(&id, &bus);

    assert!(!status.success(), "a run that could not start is a failure");
    assert_eq!(home.snapshots(), 0, "nothing was backed up");
    let state = home.run_state(&id);
    assert!(
        state.contains("failure: Some(") && state.contains("password-not-remembered"),
        "the failure is recorded for the window to show: {state:?}"
    );
    assert!(
        home.event_log(&id).contains("password-not-remembered"),
        "and it is in the history"
    );
}

#[test]
fn a_wrong_password_from_a_command_fails_the_run_without_a_backup() {
    let bus = PrivateBus::start();
    let home = Home::new();
    let id = uuid::Uuid::new_v4().to_string();
    home.save_profile(&id, "printf not-the-password");

    let status = home.run(&id, &bus);

    assert!(!status.success());
    assert_eq!(home.snapshots(), 0);
    let state = home.run_state(&id);
    assert!(
        state.contains("failure: Some(") && state.contains("wrong-password"),
        "{state:?}"
    );
}
