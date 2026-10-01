// SPDX-License-Identifier: GPL-3.0-only

//! The rclone commands Stellarshot needs besides the backup itself.
//!
//! Backups to SFTP and cloud storage go through `rclone serve restic`, which
//! [`super::serve`] starts. What rustic does not do — checking what is at a
//! location before creating a repository there, deleting a repository's
//! entries, signing in to a cloud account — is done here with plain rclone
//! commands.
//!
//! Stellarshot keeps its own rclone configuration and passes it with
//! `--config` on every command, so the user's own `rclone.conf` is never read
//! or changed, and removing a backup can never damage a remote the user set up
//! for something else.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use super::error::{EngineError, ErrorKind};
use super::location::REPOSITORY_ENTRIES;
use super::repo::Probe;
use crate::constants::{
    CHILD_STDERR_TAIL, PROBE_TIMEOUT, PROCESS_POLL_INTERVAL, RCLONE_CHANGE_TIMEOUT,
    RCLONE_LISTING_LIMIT, RCLONE_LOOK_FLAGS,
};
use crate::debug::ENGINE;
use crate::debug_log;

/// The rclone executable.
const RCLONE: &str = "rclone";

/// `args`, with the value of any `client_secret=…` replaced, for the debug
/// log: a user's own Google API credentials should not sit in a log file
/// even one that developer logging (off by default, stripped from release
/// builds) has to be turned on to read.
fn redact(args: &[&str]) -> Vec<String> {
    args.iter()
        .map(|arg| match arg.split_once('=') {
            Some((key, _)) if key.eq_ignore_ascii_case("client_secret") => {
                format!("{key}=<redacted>")
            }
            _ => (*arg).to_owned(),
        })
        .collect()
}

/// rclone's exit status for "directory not found".
const EXIT_DIRECTORY_NOT_FOUND: i32 = 3;
/// rclone's exit status for "file not found".
const EXIT_FILE_NOT_FOUND: i32 = 4;

/// Stellarshot's own rclone configuration file.
///
/// With no home directory to put it in, a path nothing can be created at
/// ([`crate::paths::unusable`]): never a shared directory such as `/tmp`,
/// where another user could already own the folder that would hold OAuth
/// tokens.
pub fn config_path() -> PathBuf {
    match crate::paths::config_root() {
        Some(root) => root.join("stellarshot").join("rclone.conf"),
        None => crate::paths::unusable().join("rclone.conf"),
    }
}

/// `remote:path` in rclone's syntax.
pub fn target(remote: &str, path: &str) -> String {
    format!("{remote}:{path}")
}

/// rclone, set up to run against Stellarshot's own configuration and nothing
/// of the user's: untranslated output (`LC_ALL=C`, for anything that reads
/// it), and none of the `RCLONE_*` environment the user's shell may carry.
/// One of those can change what a command does, `RCLONE_DRY_RUN=true`
/// turning a delete into a no-op that still reports success, say. What a
/// caller needs rclone to see it sets itself, after this.
pub(super) fn command() -> Command {
    let mut command = Command::new(RCLONE);
    command.env("LC_ALL", "C");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("RCLONE_") {
            command.env_remove(name);
        }
    }
    command
}

/// `err` from starting rclone: only "not found" means it is not installed.
fn spawn_error(err: std::io::Error) -> EngineError {
    if err.kind() == std::io::ErrorKind::NotFound {
        EngineError::new(ErrorKind::RcloneMissing, RCLONE)
    } else {
        EngineError::from(err)
    }
}

/// Run rclone with Stellarshot's configuration, for the commands that change
/// things (deleting a repository's entries, forgetting a remote). Bounded by
/// [`RCLONE_CHANGE_TIMEOUT`], which is far longer than a look: a big
/// repository takes a while to delete, but a stalled connection must still
/// end in an error, not a dialog that never closes.
fn rclone(config: &Path, args: &[&str]) -> Result<Output, EngineError> {
    rclone_limited(config, &[], args, RCLONE_CHANGE_TIMEOUT)
}

/// [`rclone`], with extra environment variables set on the child. Used only
/// by [`sign_in`], to pass the OAuth client ID and secret without ever
/// putting them on argv (see its own doc comment).
fn rclone_with_env(
    config: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<Output, EngineError> {
    debug_log!(ENGINE, "rclone {}", redact(args).join(" "));
    command()
        .envs(envs.iter().copied())
        .arg("--config")
        .arg(config)
        .args(args)
        .output()
        .map_err(spawn_error)
}

/// Run rclone with Stellarshot's configuration, killing it if it has not
/// finished within `limit`. For commands that only look: a location that does
/// not answer must turn into an explanation, not a wait without end.
fn rclone_within(config: &Path, args: &[&str], limit: Duration) -> Result<Output, EngineError> {
    rclone_limited(config, RCLONE_LOOK_FLAGS, args, limit)
}

/// [`rclone_within`], with the extra `flags` placed before `args`.
fn rclone_limited(
    config: &Path,
    flags: &[&str],
    args: &[&str],
    limit: Duration,
) -> Result<Output, EngineError> {
    debug_log!(
        ENGINE,
        "rclone {} (within {limit:?})",
        redact(args).join(" ")
    );
    let mut child = command()
        .arg("--config")
        .arg(config)
        .args(flags)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own group, so a timeout reaches anything rclone started too.
        .process_group(0)
        .spawn()
        .map_err(spawn_error)?;
    // Read both pipes as they fill, so a long listing cannot block rclone,
    // but keep only what is used: a folder with millions of entries must not
    // fill memory. Only the start of a listing matters (see [`classify`]),
    // and only the end of what rclone said explains a failure.
    let stdout = {
        let pipe = child.stdout.take();
        std::thread::spawn(move || match pipe {
            Some(pipe) => crate::bounded::read_head(pipe, RCLONE_LISTING_LIMIT).0,
            None => Vec::new(),
        })
    };
    let stderr = {
        let pipe = child.stderr.take();
        std::thread::spawn(move || match pipe {
            Some(pipe) => crate::bounded::read_tail(pipe, CHILD_STDERR_TAIL),
            None => Vec::new(),
        })
    };
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            if let Some(group) = i32::try_from(child.id())
                .ok()
                .and_then(rustix::process::Pid::from_raw)
            {
                let _ = rustix::process::kill_process_group(group, rustix::process::Signal::KILL);
            }
            let _ = child.kill();
            let _ = child.wait();
            debug_log!(ENGINE, "rclone {} timed out", redact(args).join(" "));
            return Err(EngineError::new(
                ErrorKind::TimedOut,
                limit.as_secs().to_string(),
            ));
        }
        std::thread::sleep(PROCESS_POLL_INTERVAL);
    };
    Ok(Output {
        status,
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// Whether rclone is installed and runs.
pub fn available() -> bool {
    command()
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
        &["lsf", "--max-depth", "1", "--", &target(remote, path)],
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
/// that holds something else, not a repository. Succeeds as a no-op if
/// there is nothing there at all — the data could already have been
/// removed by hand outside Stellarshot, and the state being asked for (no
/// repository at this location) is already true.
pub fn delete_repository(config: &Path, remote: &str, path: &str) -> Result<(), EngineError> {
    match probe(config, remote, path)? {
        Probe::Empty => return Ok(()),
        Probe::Repository => {}
        Probe::NotEmpty => {
            return Err(EngineError::new(
                ErrorKind::NotARepository,
                target(remote, path),
            ));
        }
    }
    for name in REPOSITORY_ENTRIES {
        let entry = target(remote, &join(path, name));
        let command = if *name == "config" {
            "deletefile"
        } else {
            "purge"
        };
        let output = rclone(config, &[command, "--", &entry])?;
        // An entry the repository never created is not an error. The exit
        // code is checked first (language-independent); the message is only
        // a fallback, and reliable now that `LC_ALL=C` guarantees it is in
        // English rather than the operator's own locale.
        if !output.status.success()
            && !matches!(
                output.status.code(),
                Some(EXIT_DIRECTORY_NOT_FOUND | EXIT_FILE_NOT_FOUND)
            )
        {
            let message = stderr(&output);
            if !message.contains("not found") {
                return Err(EngineError::new(ErrorKind::DestinationUnavailable, message));
            }
        }
    }
    // Removes the folder only if nothing else is left in it.
    let _ = rclone(config, &["rmdir", "--", &target(remote, path)]);
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

/// Whether Stellarshot's own rclone configuration already has a section
/// named `name`. Reads the file directly rather than asking rclone, so it
/// works even before rclone is confirmed to be installed, and cannot be
/// fooled by anything rclone itself might do with a crafted section name.
pub(crate) fn remote_exists(config: &Path, name: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(config) else {
        return false;
    };
    let header = format!("[{name}]");
    text.lines().any(|line| line.trim() == header)
}

/// The remotes in the user's own rclone configuration.
pub fn user_remotes() -> Result<Vec<String>, EngineError> {
    // The user's own configuration, so their own `RCLONE_*` environment
    // (an encrypted config's password, say) is left as it is.
    let output = Command::new(RCLONE)
        .env("LC_ALL", "C")
        .arg("listremotes")
        .output()
        .map_err(spawn_error)?;
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
        .env("LC_ALL", "C")
        .args(["config", "show", "--", user_remote])
        .output()
        .map_err(spawn_error)?;
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
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    // Always, not only when it is missing: one that already exists may be
    // someone else's, or readable by others.
    if let Some(dir) = config.parent() {
        crate::paths::tighten_private(dir).map_err(|err| EngineError::io(dir, err))?;
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
///
/// `credentials` (an OAuth client ID and secret, for a user's own Google
/// Cloud project rather than rclone's bundled one) are passed as
/// `RCLONE_<PROVIDER>_CLIENT_ID`/`_SECRET` environment variables, never as
/// `client_id=…`/`client_secret=…` arguments: argv is world-readable for the
/// process's lifetime through `/proc/<pid>/cmdline`, while `/proc/<pid>/environ`
/// is readable only by its own user.
pub fn sign_in(
    config: &Path,
    name: &str,
    provider: &str,
    params: &[&str],
    credentials: Option<(&str, &str)>,
) -> Result<(), EngineError> {
    make_private(config)?;
    let mut args = vec!["config", "create", "--", name, provider];
    args.extend_from_slice(params);
    let provider_upper = provider.to_ascii_uppercase();
    let id_var = format!("RCLONE_{provider_upper}_CLIENT_ID");
    let secret_var = format!("RCLONE_{provider_upper}_CLIENT_SECRET");
    let envs: Vec<(&str, &str)> = match credentials {
        Some((id, secret)) => vec![(id_var.as_str(), id), (secret_var.as_str(), secret)],
        None => Vec::new(),
    };
    let output = rclone_with_env(config, &args, &envs)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EngineError::new(ErrorKind::AuthFailed, stderr(&output)))
    }
}

/// Remove a remote Stellarshot created.
pub fn delete_remote(config: &Path, name: &str) -> Result<(), EngineError> {
    let output = rclone(config, &["config", "delete", "--", name])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(EngineError::new(ErrorKind::Internal, stderr(&output)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_client_secret_is_redacted_for_the_log_but_nothing_else_is() {
        let args = redact(&[
            "config",
            "create",
            "stellarshot-abc",
            "drive",
            "scope=drive",
            "client_id=my-id",
            "client_secret=hunter2",
        ]);
        assert_eq!(
            args,
            vec![
                "config",
                "create",
                "stellarshot-abc",
                "drive",
                "scope=drive",
                "client_id=my-id",
                "client_secret=<redacted>",
            ]
        );
    }

    #[test]
    fn remote_exists_matches_only_a_real_section() {
        let dir = tempfile::TempDir::new().unwrap();
        let config = dir.path().join("rclone.conf");
        std::fs::write(&config, "[stellarshot-deadbeef]\ntype = drive\n").unwrap();

        assert!(remote_exists(&config, "stellarshot-deadbeef"));
        assert!(!remote_exists(&config, "stellarshot-deadbee0"));
        assert!(!remote_exists(&config, "not-a-real-section"));
        assert!(!remote_exists(
            dir.path().join("missing.conf").as_path(),
            "x"
        ));
    }

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
    fn a_loose_folder_for_the_configuration_is_tightened_not_trusted() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::TempDir::new().unwrap();
        let folder = root.path().join("stellarshot");
        std::fs::create_dir(&folder).unwrap();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o777)).unwrap();

        make_private(&folder.join("rclone.conf")).unwrap();

        assert_eq!(mode(&folder), 0o700);
    }

    #[test]
    fn a_symlinked_folder_for_the_configuration_is_refused() {
        let root = tempfile::TempDir::new().unwrap();
        let elsewhere = root.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let folder = root.path().join("stellarshot");
        std::os::unix::fs::symlink(&elsewhere, &folder).unwrap();

        assert!(make_private(&folder.join("rclone.conf")).is_err());
        assert!(
            !elsewhere.join("rclone.conf").exists(),
            "no token file was created through the link"
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
