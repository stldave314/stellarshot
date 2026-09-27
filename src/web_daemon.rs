// SPDX-License-Identifier: GPL-3.0-only

//! Starting, stopping and watching `stellarshot-web` as a per-user systemd
//! service — the same kind of unit [`crate::schedule`] already installs for
//! scheduled backups, but a single long-running service rather than one
//! timer per profile.
//!
//! Turning the network scope off disables and removes the service, the same
//! as turning a backup's schedule to manual removes its timer: nothing left
//! behind for a setting that is off. Turning it on, or the explicit
//! Start/Stop/Restart controls in Settings, install it if needed and change
//! only whether it is running.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use atomicwrites::{AllowOverwrite, AtomicFile};

use crate::debug::WEB;
use crate::debug_log;

pub const SERVICE_NAME: &str = "stellarshot-web.service";

/// Mirrors [`crate::schedule::exec_quote`]: the same quoting rules apply to
/// any path going into any unit file's `ExecStart=` line.
fn exec_quote(path: &Path) -> Option<String> {
    let text = path.to_str()?;
    if text.contains(['\n', '\r']) {
        return None;
    }
    let escaped = text
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    Some(format!("\"{escaped}\""))
}

/// The `stellarshot-web` binary's path: the main executable's own directory
/// (resolved the same way [`crate::schedule::executable`] resolves it, a
/// package upgrade's "(deleted)" suffix stripped the same way), with its file
/// name swapped for the daemon's — they are always installed side by side.
fn web_executable() -> Result<PathBuf, String> {
    Ok(crate::schedule::executable()?.with_file_name("stellarshot-web"))
}

fn unit_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("systemd/user"))
}

fn service_text(executable: &Path) -> Option<String> {
    use crate::constants::{
        WEB_GRACEFUL_SHUTDOWN_TIMEOUT, WEB_UNIT_LIMIT_NOFILE, WEB_UNIT_MEMORY_MAX,
        WEB_UNIT_RESTART_SECS, WEB_UNIT_START_LIMIT_BURST, WEB_UNIT_START_LIMIT_INTERVAL_SECS,
    };
    let exec = exec_quote(executable)?;
    // `TimeoutStopSec` is double the graceful-shutdown window `web::serve`
    // itself waits out (see WEB-8), so systemd never has to force-kill a
    // shutdown that is still within its own budget.
    let timeout_stop = WEB_GRACEFUL_SHUTDOWN_TIMEOUT.as_secs() * 2;
    Some(format!(
        "# Written by Stellarshot; changes are overwritten.\n\
         [Unit]\n\
         Description=Stellarshot web interface\n\
         StartLimitIntervalSec={WEB_UNIT_START_LIMIT_INTERVAL_SECS}\n\
         StartLimitBurst={WEB_UNIT_START_LIMIT_BURST}\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exec}\n\
         Restart=on-failure\n\
         RestartSec={WEB_UNIT_RESTART_SECS}\n\
         TimeoutStopSec={timeout_stop}\n\
         NoNewPrivileges=yes\n\
         UMask=0077\n\
         LockPersonality=yes\n\
         RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\n\
         LimitNOFILE={WEB_UNIT_LIMIT_NOFILE}\n\
         MemoryMax={WEB_UNIT_MEMORY_MAX}\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n"
    ))
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|err| format!("systemctl: {err}"))?;
    debug_log!(
        WEB,
        "systemctl --user {}: {}",
        args.join(" "),
        output.status
    );
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

/// See [`crate::schedule::write_if_changed`]: the same atomic, no-op-avoiding
/// write, so an unclean shutdown mid-write never leaves a half-written unit
/// file, and a config change that did not actually change anything does not
/// needlessly bounce the daemon.
fn write_if_changed(path: &Path, text: &str) -> Result<bool, String> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == text) {
        return Ok(false);
    }
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(text.as_bytes()))
        .map_err(|err| format!("{}: {}", path.display(), std::io::Error::from(err)))?;
    Ok(true)
}

/// Write the unit file if needed and make sure it is enabled and running.
pub fn start() -> Result<(), String> {
    let dir = unit_dir().ok_or("no configuration directory")?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let text = service_text(&web_executable()?)
        .ok_or("the web interface's executable path cannot go in a systemd unit")?;
    let changed = write_if_changed(&dir.join(SERVICE_NAME), &text)?;
    systemctl(&["daemon-reload"])?;
    systemctl(&["enable", SERVICE_NAME])?;
    if changed {
        systemctl(&["restart", SERVICE_NAME])
    } else {
        systemctl(&["start", SERVICE_NAME])
    }
}

/// Stop and disable the service, and remove its unit file: nothing left
/// running or installed for a network scope that is off. Succeeds if there
/// was none, the same as [`crate::schedule::remove`].
pub fn stop() -> Result<(), String> {
    let Some(dir) = unit_dir() else {
        return Ok(());
    };
    let path = dir.join(SERVICE_NAME);
    if !path.exists() {
        return Ok(());
    }
    systemctl(&["disable", "--now", SERVICE_NAME])?;
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("{}: {err}", path.display())),
    }
    systemctl(&["daemon-reload"])
}

/// Restart the service if it is already installed, or start it fresh
/// otherwise — the button Settings shows either way is just "Restart".
pub fn restart() -> Result<(), String> {
    let installed = unit_dir().is_some_and(|dir| dir.join(SERVICE_NAME).exists());
    if installed {
        start()?;
        systemctl(&["restart", SERVICE_NAME])
    } else {
        start()
    }
}

/// Whether the service is running, for the status indicator in Settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Status {
    Active,
    Inactive,
    Failed,
    /// Not installed at all, or systemd could not be reached.
    #[default]
    Unknown,
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait Manager {
    fn load_unit(&self, name: &str) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
trait Unit {
    #[zbus(property)]
    fn load_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
}

pub async fn status() -> Status {
    match query_status().await {
        Ok(status) => status,
        Err(err) => {
            debug_log!(WEB, "could not read the web interface's status: {err}");
            Status::Unknown
        }
    }
}

async fn query_status() -> zbus::Result<Status> {
    let connection = zbus::Connection::session().await?;
    let manager = ManagerProxy::new(&connection).await?;
    let path = manager.load_unit(SERVICE_NAME).await?;
    let unit = UnitProxy::builder(&connection).path(&path)?.build().await?;
    if unit.load_state().await? != "loaded" {
        return Ok(Status::Unknown);
    }
    Ok(match unit.active_state().await?.as_str() {
        "active" | "reloading" => Status::Active,
        "failed" => Status::Failed,
        _ => Status::Inactive,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_service_runs_the_daemon_and_restarts_on_failure() {
        let text = service_text(Path::new("/usr/bin/stellarshot-web")).unwrap();
        assert!(text.contains("ExecStart=\"/usr/bin/stellarshot-web\"\n"));
        assert!(text.contains("Type=simple\n"));
        assert!(text.contains("Restart=on-failure\n"));
        assert!(text.contains("TimeoutStopSec=60\n"));
        assert!(text.contains("WantedBy=default.target\n"));
    }

    /// A daemon that cannot even start (a bad TLS path, the port already
    /// taken, the binary removed) must settle into `failed`, not restart
    /// every few seconds forever — the start limit and the hardening below
    /// it are what `Restart=on-failure` alone does not provide.
    #[test]
    fn the_service_is_hardened_against_a_restart_loop_and_privilege_escalation() {
        let text = service_text(Path::new("/usr/bin/stellarshot-web")).unwrap();
        assert!(text.contains("StartLimitIntervalSec=300\n"));
        assert!(text.contains("StartLimitBurst=5\n"));
        assert!(text.contains("RestartSec=30\n"));
        assert!(text.contains("NoNewPrivileges=yes\n"));
        assert!(text.contains("UMask=0077\n"));
        assert!(text.contains("LockPersonality=yes\n"));
        assert!(text.contains("RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6\n"));
        assert!(text.contains("LimitNOFILE=1024\n"));
        assert!(text.contains("MemoryMax=1G\n"));
    }

    #[test]
    fn a_newline_in_the_executable_path_is_rejected() {
        assert_eq!(
            service_text(Path::new("/tmp/bad\nExecStartPost=/bin/rm")),
            None
        );
    }

    #[test]
    fn exec_paths_are_escaped_for_systemd() {
        let text = service_text(Path::new("/home/alex/My Apps/100%/$bin/stellarshot-web")).unwrap();
        assert!(
            text.contains(r#"ExecStart="/home/alex/My Apps/100%%/$$bin/stellarshot-web"\n"#)
                || text.contains(r#"ExecStart="/home/alex/My Apps/100%%/$$bin/stellarshot-web""#),
            "{text}"
        );
    }

    #[test]
    fn a_written_unit_never_ends_up_empty_or_half_written() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(SERVICE_NAME);

        assert!(write_if_changed(&path, "first version\n").unwrap());
        assert!(!write_if_changed(&path, "first version\n").unwrap());
        assert!(write_if_changed(&path, "second version\n").unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second version\n");
    }
}
