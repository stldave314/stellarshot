// SPDX-License-Identifier: GPL-3.0-only

//! The rclone commands Stellarshot needs besides the backup itself.
//!
//! Backups to SFTP and cloud storage go through `rclone serve restic`, which
//! rustic starts on its own. What rustic does not do — checking what is at a
//! location before creating a repository there, deleting a repository's
//! entries, signing in to a cloud account — is done here with plain rclone
//! commands.
//!
//! Stellarshot keeps its own rclone configuration and passes it with
//! `--config` on every command, so the user's own `rclone.conf` is never read
//! or changed, and removing a backup can never damage a remote the user set up
//! for something else.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::error::{EngineError, ErrorKind};
use super::location::REPOSITORY_ENTRIES;
use super::repo::Probe;
use crate::constants::{PROBE_TIMEOUT, RCLONE_LOOK_FLAGS};
use crate::debug::ENGINE;
use crate::debug_log;

/// The rclone executable.
const RCLONE: &str = "rclone";

/// rclone's exit status for "directory not found".
const EXIT_DIRECTORY_NOT_FOUND: i32 = 3;

/// Stellarshot's own rclone configuration file.
pub fn config_path() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
        .join("stellarshot")
        .join("rclone.conf")
}

/// `remote:path` in rclone's syntax.
pub fn target(remote: &str, path: &str) -> String {
    format!("{remote}:{path}")
}

/// Run rclone with Stellarshot's configuration.
fn rclone(config: &Path, args: &[&str]) -> Result<Output, EngineError> {
    debug_log!(ENGINE, "rclone {}", args.join(" "));
    Command::new(RCLONE)
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                EngineError::new(ErrorKind::RcloneMissing, RCLONE)
            } else {
                EngineError::from(err)
            }
        })
}

/// Run rclone with Stellarshot's configuration, killing it if it has not
/// finished within `limit`. For commands that only look: a location that does
/// not answer must turn into an explanation, not a wait without end.
fn rclone_within(config: &Path, args: &[&str], limit: Duration) -> Result<Output, EngineError> {
    debug_log!(ENGINE, "rclone {} (within {limit:?})", args.join(" "));
    let mut child = Command::new(RCLONE)
        .arg("--config")
        .arg(config)
        .args(RCLONE_LOOK_FLAGS)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                EngineError::new(ErrorKind::RcloneMissing, RCLONE)
            } else {
                EngineError::from(err)
            }
        })?;
    // Read both pipes as they fill, so a long listing cannot block rclone.
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            bytes
        })
    };
    let stdout = read(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = read(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            debug_log!(ENGINE, "rclone {} timed out", args.join(" "));
            return Err(EngineError::new(
                ErrorKind::TimedOut,
                limit.as_secs().to_string(),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Whether rclone is installed and runs.
pub fn available() -> bool {
    Command::new(RCLONE)
        .arg("version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

/// What is at `remote:path`, as [`super::probe`] reports for a folder.
pub fn probe(config: &Path, remote: &str, path: &str) -> Result<Probe, EngineError> {
    let output = rclone_within(
        config,
        &["lsf", "--max-depth", "1", &target(remote, path)],
        PROBE_TIMEOUT,
    )?;
    if !output.status.success() {
        return if output.status.code() == Some(EXIT_DIRECTORY_NOT_FOUND) {
            Ok(Probe::Empty)
        } else {
            Err(EngineError::new(
                ErrorKind::DestinationUnavailable,
                stderr(&output),
            ))
        };
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<&str> = listing.lines().filter(|line| !line.is_empty()).collect();
    Ok(classify(&entries))
}

/// Classify a directory listing from `rclone lsf` (folders end in `/`).
fn classify(entries: &[&str]) -> Probe {
    if entries.is_empty() {
        Probe::Empty
    } else if entries.contains(&"config") && entries.contains(&"keys/") {
        Probe::Repository
    } else {
        Probe::NotEmpty
    }
}

/// Delete a repository at `remote:path`: only the entries the repository
/// format creates, then the folder if it is left empty. Refuses a location
/// that is not a repository.
pub fn delete_repository(config: &Path, remote: &str, path: &str) -> Result<(), EngineError> {
    if probe(config, remote, path)? != Probe::Repository {
        return Err(EngineError::new(
            ErrorKind::NotARepository,
            target(remote, path),
        ));
    }
    for name in REPOSITORY_ENTRIES {
        let entry = target(remote, &join(path, name));
        let command = if *name == "config" {
            "deletefile"
        } else {
            "purge"
        };
        let output = rclone(config, &[command, &entry])?;
        // An entry the repository never created is not an error.
        if !output.status.success() && output.status.code() != Some(EXIT_DIRECTORY_NOT_FOUND) {
            let message = stderr(&output);
            if !message.contains("not found") {
                return Err(EngineError::new(ErrorKind::DestinationUnavailable, message));
            }
        }
    }
    // Removes the folder only if nothing else is left in it.
    let _ = rclone(config, &["rmdir", &target(remote, path)]);
    Ok(())
}

/// `path/name`, without doubling the separator.
fn join(path: &str, name: &str) -> String {
    if path.is_empty() {
        name.to_owned()
    } else {
        format!("{}/{name}", path.trim_end_matches('/'))
    }
}

/// An on-the-fly SFTP remote: `:sftp,host=…,user=…,port=…,known_hosts_file=…`.
///
/// Host keys are checked against the user's `known_hosts`, so a server that is
/// unknown or whose key changed is refused instead of trusted silently.
/// Authentication is the SSH agent or the user's own keys.
pub fn sftp_remote(host: &str, user: &str, port: u16, known_hosts: &Path) -> String {
    let mut remote = format!(":sftp,host={},port={port}", quote(host));
    if !user.is_empty() {
        remote.push_str(&format!(",user={}", quote(user)));
    }
    remote.push_str(&format!(
        ",known_hosts_file={}",
        quote(&known_hosts.display().to_string())
    ));
    remote
}

/// Quote a connection-string value if it contains rclone's separators.
fn quote(value: &str) -> String {
    if value.contains([',', ':', '"', '\'', ' ']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// The remotes in the user's own rclone configuration.
pub fn user_remotes() -> Result<Vec<String>, EngineError> {
    let output = Command::new(RCLONE)
        .arg("listremotes")
        .output()
        .map_err(|err| EngineError::new(ErrorKind::RcloneMissing, err.to_string()))?;
    if !output.status.success() {
        return Err(EngineError::new(ErrorKind::Internal, stderr(&output)));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim_end_matches(':').to_owned())
        .filter(|name| !name.is_empty())
        .collect())
}

/// Copy one of the user's remotes into Stellarshot's configuration, under
/// `name`, so Stellarshot keeps working even if the user later changes or
/// removes their own copy.
pub fn copy_user_remote(config: &Path, user_remote: &str, name: &str) -> Result<(), EngineError> {
    let output = Command::new(RCLONE)
        .args(["config", "show", user_remote])
        .output()
        .map_err(|err| EngineError::new(ErrorKind::RcloneMissing, err.to_string()))?;
    if !output.status.success() {
        return Err(EngineError::new(ErrorKind::Internal, stderr(&output)));
    }
    let section = String::from_utf8_lossy(&output.stdout);
    let body: String = section
        .lines()
        .skip_while(|line| !line.starts_with('['))
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n");
    append_section(config, name, &body)
}

/// Add `[name]` with `body` to the configuration file.
fn append_section(config: &Path, name: &str, body: &str) -> Result<(), EngineError> {
    use std::io::Write;
    make_private(config)?;
    let mut file = std::fs::OpenOptions::new().append(true).open(config)?;
    writeln!(file, "\n[{name}]\n{}", body.trim())?;
    Ok(())
}

/// Make sure the configuration file exists and only its owner can read it,
/// *before* anything secret is written to it. Opening a file for writing
/// keeps the mode it already has, and rclone does the same, so a file that
/// had become readable by others (copied, restored from a backup) would
/// otherwise receive a token first and be tightened only afterwards.
fn make_private(config: &Path) -> Result<(), EngineError> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    if let Some(dir) = config.parent().filter(|dir| !dir.exists()) {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(config)?;
    std::fs::set_permissions(config, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

/// Sign in to a cloud provider: `rclone config create`. rclone opens the
/// browser for the provider's login page and receives the token on
/// localhost; the token is stored only in Stellarshot's configuration.
/// Blocks until the sign-in finishes or fails.
pub fn sign_in(
    config: &Path,
    name: &str,
    provider: &str,
    params: &[&str],
) -> Result<(), EngineError> {
    make_private(config)?;
    let mut args = vec!["config", "create", name, provider];
    args.extend_from_slice(params);
    let output = rclone(config, &args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EngineError::new(ErrorKind::AuthFailed, stderr(&output)))
    }
}

/// Remove a remote Stellarshot created.
pub fn delete_remote(config: &Path, name: &str) -> Result<(), EngineError> {
    let output = rclone(config, &["config", "delete", name])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EngineError::new(ErrorKind::Internal, stderr(&output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn a_new_configuration_is_private_from_the_start() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join("stellarshot/rclone.conf");

        append_section(&config, "copied", "type = drive\ntoken = secret").unwrap();

        assert_eq!(mode(&config), 0o600);
        assert_eq!(mode(config.parent().unwrap()), 0o700);
    }

    #[test]
    fn a_readable_configuration_is_tightened_before_a_token_goes_in() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join("rclone.conf");
        std::fs::write(&config, "[old]\ntype = local\n").unwrap();
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            mode(&config),
            0o644,
            "the fixture starts readable by others"
        );

        make_private(&config).unwrap();
        assert_eq!(mode(&config), 0o600, "tightened before anything is written");

        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();
        append_section(&config, "copied", "token = secret").unwrap();
        assert_eq!(mode(&config), 0o600, "appending tightens it first too");
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(
            text.contains("[old]") && text.contains("[copied]"),
            "nothing lost"
        );
    }

    #[test]
    fn listings_are_classified_like_folders() {
        assert_eq!(classify(&[]), Probe::Empty);
        assert_eq!(
            classify(&["config", "data/", "index/", "keys/", "snapshots/"]),
            Probe::Repository
        );
        assert_eq!(classify(&["notes.txt"]), Probe::NotEmpty);
        assert_eq!(
            classify(&["config"]),
            Probe::NotEmpty,
            "config alone is not enough"
        );
    }

    #[test]
    fn sftp_remotes_check_host_keys() {
        let remote = sftp_remote(
            "backup.example.com",
            "alex",
            22,
            Path::new("/home/alex/.ssh/known_hosts"),
        );
        assert_eq!(
            remote,
            ":sftp,host=backup.example.com,port=22,user=alex,known_hosts_file=/home/alex/.ssh/known_hosts"
        );
    }

    #[test]
    fn values_with_separators_are_quoted() {
        assert_eq!(quote("plain"), "plain");
        assert_eq!(quote("a,b"), "\"a,b\"");
        assert_eq!(quote("say \"hi\""), "\"say \"\"hi\"\"\"");
    }

    #[test]
    fn joining_never_doubles_the_separator() {
        assert_eq!(join("backups/", "keys"), "backups/keys");
        assert_eq!(join("", "keys"), "keys");
    }
}
