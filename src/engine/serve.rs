// SPDX-License-Identifier: GPL-3.0-only

//! `rclone serve restic`, started and stopped by Stellarshot itself.
//!
//! rustic can start this on its own (an `rclone:` repository), but its
//! backend only kills the process when it is dropped and never waits for it,
//! so every repository opened over rclone left a zombie behind until the
//! program exited, and it had no limit on how long to wait for rclone to come
//! up. Owning the process here fixes both, and hands rustic a plain `rest:`
//! location instead.

use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::error::{EngineError, ErrorKind};
use crate::constants::{CHILD_STDERR_DETAIL, RCLONE_SERVE_START_TIMEOUT};
use crate::debug::ENGINE;
use crate::debug_log;

/// What rclone prints on stderr once the REST server is listening, followed
/// by its address.
const READY_MARKER: &str = "Serving restic REST API on ";

/// A running `rclone serve restic`, stopped and reaped when dropped.
///
/// Declare it after whatever talks to it, so that is dropped first: fields
/// drop in declaration order.
#[derive(Debug)]
pub struct Serve {
    child: Child,
    /// `http://user:password@host:port/`, the credentials being this
    /// process's own one-off ones.
    url: String,
}

impl Serve {
    /// Start `command` (a whole command line, see `repo::rclone_command`)
    /// serving `target`, and wait for it to listen.
    pub fn start(command: &str, target: &str) -> Result<Self, EngineError> {
        Self::start_within(command, target, RCLONE_SERVE_START_TIMEOUT)
    }

    fn start_within(command: &str, target: &str, limit: Duration) -> Result<Self, EngineError> {
        let words = shell_words::split(command)
            .map_err(|err| EngineError::new(ErrorKind::Internal, err.to_string()))?;
        let Some((program, args)) = words.split_first() else {
            return Err(EngineError::new(ErrorKind::Internal, "an empty command"));
        };
        // One-off credentials for the local server, so nothing else on this
        // machine can use it while it runs.
        let user = uuid::Uuid::new_v4().simple().to_string();
        let password = uuid::Uuid::new_v4().simple().to_string();
        debug_log!(ENGINE, "starting {program} for {target}");
        // Against Stellarshot's own configuration only: see
        // `rclone::command` on why the user's `RCLONE_*` is left out.
        let mut base = if program == "rclone" {
            super::rclone::command()
        } else {
            Command::new(program)
        };
        let mut child = base
            .env("RCLONE_USER", &user)
            .env("RCLONE_PASS", &password)
            .args(args)
            .arg(target)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    EngineError::new(ErrorKind::RcloneMissing, program.as_str())
                } else {
                    EngineError::from(err)
                }
            })?;
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::new(ErrorKind::Internal, "no rclone output"));
        };
        // Read for as long as rclone runs, so a full pipe can never stall it,
        // passing on only the address and, if it fails, what it said.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stderr = BufReader::new(stderr);
            let mut line = Vec::new();
            loop {
                line.clear();
                // Bytes, not `read_line`: a non-UTF-8 line must not end the
                // reading, which would close the pipe under rclone.
                match stderr.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                let text = String::from_utf8_lossy(&line).trim_end().to_owned();
                log::info!("rclone output: {text}");
                // The receiver is gone once startup has finished or failed.
                let _ = tx.send(text);
            }
        });
        let mut said = String::new();
        let deadline = std::time::Instant::now() + limit;
        let address = loop {
            let wait = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(wait) {
                Ok(line) => {
                    // Only an address on this machine: a line from the remote
                    // that happens to contain the marker must not send the
                    // credentials, and every request, somewhere else.
                    if let Some(address) = address_in(&line).filter(|a| is_loopback(a)) {
                        break address;
                    }
                    said.push_str(&line);
                    said.push('\n');
                }
                Err(err) => {
                    let _ = child.kill();
                    let status = child.wait().ok();
                    let (kind, detail) = match err {
                        // Most likely a connection that never completes:
                        // the remote is not reachable now.
                        mpsc::RecvTimeoutError::Timeout => (
                            ErrorKind::DestinationUnavailable,
                            format!("rclone did not start serving within {limit:?}"),
                        ),
                        mpsc::RecvTimeoutError::Disconnected => (
                            if super::error::looks_unreachable(&said) {
                                ErrorKind::DestinationUnavailable
                            } else {
                                ErrorKind::Internal
                            },
                            format!("rclone exited before it could start serving: {status:?}"),
                        ),
                    };
                    return Err(EngineError::new(kind, with_tail(detail, &said)));
                }
            }
        };
        let Some(rest) = address.strip_prefix("http://") else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(EngineError::new(
                ErrorKind::Internal,
                format!("rclone announced an address that is not http://: {address}"),
            ));
        };
        Ok(Self {
            child,
            url: format!("http://{user}:{password}@{rest}"),
        })
    }

    /// The address to hand rustic as `rest:{url}`.
    pub fn url(&self) -> &str {
        &self.url
    }
}

impl Drop for Serve {
    fn drop(&mut self) {
        debug_log!(ENGINE, "stopping rclone serve");
        let _ = self.child.kill();
        // Without this the process stays a zombie until the program exits.
        let _ = self.child.wait();
    }
}

/// Stop every `rclone serve restic` left running with Stellarshot's own
/// rclone configuration (`config`) whose parent is no longer a Stellarshot
/// process: one that a window or `--run` child started and then crashed
/// without stopping, still holding its connection to the remote. Only this
/// user's processes are looked at. Returns how many were stopped.
pub fn stop_orphans(config: &Path) -> usize {
    use std::os::unix::fs::MetadataExt;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return 0;
    };
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    let mut stopped = 0;
    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<i32>().ok())
        else {
            continue;
        };
        if !entry.metadata().is_ok_and(|meta| meta.uid() == uid) {
            continue;
        }
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        if !is_our_serve(&cmdline, config) || parent_is_stellarshot(pid) {
            continue;
        }
        if let Some(pid) = rustix::process::Pid::from_raw(pid)
            && rustix::process::kill_process(pid, rustix::process::Signal::TERM).is_ok()
        {
            debug_log!(ENGINE, "stopped a left-over rclone serve ({pid:?})");
            stopped += 1;
        }
    }
    stopped
}

/// Whether `cmdline` (NUL-separated, as in `/proc/<pid>/cmdline`) is
/// `rclone … serve restic … --config <config>`.
fn is_our_serve(cmdline: &[u8], config: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let args: Vec<&[u8]> = cmdline.split(|&byte| byte == 0).collect();
    let program_is_rclone = args.first().is_some_and(|program| {
        Path::new(std::ffi::OsStr::from_bytes(program)).file_name()
            == Some(std::ffi::OsStr::new("rclone"))
    });
    let has = |word: &[u8]| args.contains(&word);
    let uses_config = args
        .windows(2)
        .any(|pair| pair[0] == b"--config" && pair[1] == config.as_os_str().as_bytes());
    program_is_rclone && has(b"serve") && has(b"restic") && uses_config
}

/// Whether `pid`'s parent is a Stellarshot process (the window, a `--run`
/// child or a `--scheduled` run), which still owns and will stop it.
fn parent_is_stellarshot(pid: i32) -> bool {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return true;
    };
    // `pid (comm) state ppid …`: the name can hold spaces and parentheses.
    let Some(ppid) = stat
        .rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().nth(1))
    else {
        return true;
    };
    std::fs::read_to_string(format!("/proc/{ppid}/comm"))
        .is_ok_and(|comm| comm.trim() == "stellarshot")
}

/// The address in a "Serving restic REST API on …" line. rclone 1.61 and
/// later put it in brackets.
fn address_in(line: &str) -> Option<String> {
    let at = line.find(READY_MARKER)?;
    let address = line[at + READY_MARKER.len()..].trim_end();
    Some(address.trim_matches(['[', ']']).to_owned())
}

/// Whether `address` (`http://host:port/`) is on this machine.
fn is_loopback(address: &str) -> bool {
    url::Url::parse(address).is_ok_and(|url| match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name == "localhost",
        None => false,
    })
}

/// `detail`, then the last of what rclone said, bounded.
fn with_tail(detail: String, said: &str) -> String {
    let said = said.trim();
    if said.is_empty() {
        return detail;
    }
    let start = said.len().saturating_sub(CHILD_STDERR_DETAIL);
    let start = (start..said.len())
        .find(|&i| said.is_char_boundary(i))
        .unwrap_or(said.len());
    format!("{detail}\n{}", &said[start..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::CommandExt;

    #[test]
    fn the_address_is_found_with_or_without_brackets() {
        assert_eq!(
            address_in("2026/09/30 NOTICE: Serving restic REST API on http://127.0.0.1:41234/\n")
                .as_deref(),
            Some("http://127.0.0.1:41234/")
        );
        assert_eq!(
            address_in("Serving restic REST API on [http://[::1]:5000/]").as_deref(),
            Some("http://[::1]:5000/")
        );
        assert_eq!(address_in("NOTICE: starting up"), None);
    }

    #[test]
    fn a_command_that_never_serves_times_out_and_is_reaped() {
        let started = std::time::Instant::now();

        let err = Serve::start_within("sh -c 'sleep 30' --", "x", Duration::from_millis(300))
            .unwrap_err();

        assert!(err.detail.contains("did not start serving"), "{err:?}");
        assert_eq!(err.kind, ErrorKind::DestinationUnavailable);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the wait is bounded by the limit, not by the command"
        );
    }

    #[test]
    fn a_command_that_exits_reports_what_it_said() {
        let err = Serve::start_within(
            "sh -c 'echo no such remote >&2; exit 3' --",
            "x",
            Duration::from_secs(10),
        )
        .unwrap_err();

        assert!(err.detail.contains("exited before"), "{err:?}");
        assert!(err.detail.contains("no such remote"), "{err:?}");
    }

    #[test]
    fn a_missing_program_is_rclone_missing() {
        let err = Serve::start("stellarshot-no-such-program", "x").unwrap_err();
        assert_eq!(err.kind, ErrorKind::RcloneMissing);
    }

    #[test]
    fn a_served_address_gets_one_off_credentials_and_drop_reaps() {
        // `sh` stands in for rclone: it prints the marker and stays up.
        let serve = Serve::start_within(
            "sh -c 'echo Serving restic REST API on http://127.0.0.1:9/ >&2; sleep 30' --",
            "x",
            Duration::from_secs(10),
        )
        .unwrap();
        assert!(serve.url().starts_with("http://"));
        assert!(serve.url().contains('@'));
        assert!(serve.url().ends_with("@127.0.0.1:9/"));
        let pid = serve.child.id();

        drop(serve);

        assert!(
            !std::path::Path::new(&format!("/proc/{pid}")).exists(),
            "the process is gone, not a zombie"
        );
    }

    /// What rclone prints when an SFTP host cannot be reached, checked
    /// against a real rclone: it exits before serving.
    #[test]
    fn a_remote_that_cannot_be_reached_is_unavailable_not_an_internal_error() {
        let err = Serve::start_within(
            "sh -c 'echo NewFs: couldnt connect SSH: dial tcp: lookup nas.invalid: no such host >&2; exit 1' --",
            "x",
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert_eq!(err.kind, ErrorKind::DestinationUnavailable, "{err:?}");

        let err = Serve::start_within(
            "sh -c 'echo NewFs: couldnt connect SSH: ssh: handshake failed: ssh: unable to authenticate >&2; exit 1' --",
            "x",
            Duration::from_secs(10),
        )
        .unwrap_err();
        assert_eq!(
            err.kind,
            ErrorKind::Internal,
            "a refused login is a real failure, not a quiet skip"
        );
    }

    #[test]
    fn only_our_own_rclone_serve_is_recognized() {
        let config = Path::new("/home/a/.config/stellarshot/rclone.conf");
        let ours = b"rclone\0serve\0restic\0--addr\0localhost:0\0--config\0/home/a/.config/stellarshot/rclone.conf\0--\0r:x\0";
        assert!(is_our_serve(ours, config));
        let other_config = b"rclone\0serve\0restic\0--config\0/home/a/.config/rclone/rclone.conf\0";
        assert!(!is_our_serve(other_config, config), "the user's own rclone");
        let not_serve =
            b"/usr/bin/rclone\0lsf\0--config\0/home/a/.config/stellarshot/rclone.conf\0";
        assert!(!is_our_serve(not_serve, config));
    }

    /// A left-over serve whose parent is not Stellarshot (here, the test
    /// binary) is stopped; the test makes its own stand-in with the same
    /// command line.
    #[test]
    fn a_left_over_serve_is_stopped() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join("rclone.conf");
        let mut child = std::process::Command::new("python3")
            .arg0("rclone")
            .args([
                "-c",
                "import time; time.sleep(60)",
                "serve",
                "restic",
                "--config",
            ])
            .arg(&config)
            .spawn()
            .expect("python3 stands in for rclone here");
        // Give the exec a moment to settle the command line.
        std::thread::sleep(Duration::from_millis(300));

        assert_eq!(stop_orphans(&config), 1);
        let status = child.wait().unwrap();
        assert!(!status.success(), "stopped, not finished: {status:?}");
    }

    #[test]
    fn only_a_loopback_address_is_accepted() {
        assert!(is_loopback("http://127.0.0.1:41234/"));
        assert!(is_loopback("http://[::1]:5000/"));
        assert!(is_loopback("http://localhost:5000/"));
        assert!(!is_loopback("http://10.0.0.5:8080/"));
        assert!(!is_loopback("http://evil.example:80/"));
    }
}
