// SPDX-License-Identifier: GPL-3.0-only

//! Scheduled backups as systemd user timers.
//!
//! Each scheduled profile gets `stellarshot-backup-<id>.service`, which runs
//! `stellarshot --scheduled <id>`, and a `.timer` that starts it hourly, daily
//! or weekly. The timer is persistent: a slot missed while the computer was
//! off or asleep runs as soon as the user is back. Nothing user-supplied
//! goes into a unit file, only the executable's path, escaped for systemd,
//! and the profile's ID, which must be letters, digits and dashes.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::debug::SCHED;
use crate::profile::{Profile, Schedule};
use crate::{debug_log, error_log};

const PREFIX: &str = "stellarshot-backup-";

/// Where the user's own systemd units live.
fn unit_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|config| config.join("systemd/user"))
}

/// A profile ID safe to put in a unit name and a command line.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

pub fn service_name(id: &str) -> String {
    format!("{PREFIX}{id}.service")
}

pub fn timer_name(id: &str) -> String {
    format!("{PREFIX}{id}.timer")
}

fn on_calendar(schedule: Schedule) -> Option<&'static str> {
    match schedule {
        Schedule::Manual => None,
        Schedule::Hourly => Some("hourly"),
        Schedule::Daily => Some("daily"),
        Schedule::Weekly => Some("weekly"),
    }
}

/// `path` as one argument of a systemd `ExecStart=` line: quoted, with `\`
/// and `"` escaped, and `%` and `$` doubled so systemd does not expand them.
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

pub fn service_text(executable: &Path, id: &str) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    let exec = exec_quote(executable)?;
    Some(format!(
        "# Written by Stellarshot; changes are overwritten.\n\
         [Unit]\n\
         Description=Stellarshot scheduled backup\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={exec} --scheduled {id}\n\
         Nice=10\n\
         IOSchedulingClass=idle\n"
    ))
}

pub fn timer_text(id: &str, schedule: Schedule) -> Option<String> {
    if !valid_id(id) {
        return None;
    }
    let calendar = on_calendar(schedule)?;
    Some(format!(
        "# Written by Stellarshot; changes are overwritten.\n\
         [Unit]\n\
         Description=Stellarshot scheduled backup\n\
         \n\
         [Timer]\n\
         OnCalendar={calendar}\n\
         Persistent=true\n\
         RandomizedDelaySec=10min\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n"
    ))
}

/// This program's path, for the service to run. After a package upgrade
/// replaces the binary, Linux reports the old one as "… (deleted)"; the
/// path itself is still where the new one is.
pub fn executable() -> Result<PathBuf, String> {
    let path = std::env::current_exe().map_err(|err| err.to_string())?;
    let text = path.to_string_lossy();
    Ok(match text.strip_suffix(" (deleted)") {
        Some(original) => PathBuf::from(original),
        None => path,
    })
}

fn systemctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .output()
        .map_err(|err| format!("systemctl: {err}"))?;
    debug_log!(
        SCHED,
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

/// Write `text` to `path` unless it already holds exactly that. Returns
/// whether anything changed.
fn write_if_changed(path: &Path, text: &str) -> Result<bool, String> {
    if std::fs::read_to_string(path).is_ok_and(|existing| existing == text) {
        return Ok(false);
    }
    std::fs::write(path, text).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(true)
}

/// Make the timer match the profile's schedule: installed and running, or
/// gone.
pub fn apply(profile: &Profile) -> Result<(), String> {
    if profile.schedule == Schedule::Manual {
        return remove(&profile.id);
    }
    let dir = unit_dir().ok_or("no configuration directory")?;
    let service = service_text(&executable()?, &profile.id)
        .ok_or("the program's path or the backup's ID cannot go in a systemd unit")?;
    let timer = timer_text(&profile.id, profile.schedule).ok_or("the backup's ID is not usable")?;
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let changed = write_if_changed(&dir.join(service_name(&profile.id)), &service)?
        | write_if_changed(&dir.join(timer_name(&profile.id)), &timer)?;
    let timer_unit = timer_name(&profile.id);
    if changed {
        debug_log!(SCHED, "installed {timer_unit} ({:?})", profile.schedule);
        systemctl(&["daemon-reload"])?;
        systemctl(&["enable", &timer_unit])?;
        systemctl(&["restart", &timer_unit])
    } else {
        systemctl(&["enable", "--now", &timer_unit])
    }
}

/// Stop and delete a profile's timer and service. Succeeds if there were
/// none.
pub fn remove(id: &str) -> Result<(), String> {
    let Some(dir) = unit_dir() else {
        return Ok(());
    };
    let timer = dir.join(timer_name(id));
    let service = dir.join(service_name(id));
    if !timer.exists() && !service.exists() {
        return Ok(());
    }
    // Disabling fails if systemd never loaded the unit; the files still go.
    if let Err(err) = systemctl(&["disable", "--now", &timer_name(id)]) {
        debug_log!(SCHED, "disabling {id}: {err}");
    }
    for path in [&timer, &service] {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(format!("{}: {err}", path.display())),
        }
    }
    debug_log!(SCHED, "removed the timer for {id}");
    systemctl(&["daemon-reload"])
}

/// Bring every timer in line with the settings: install or update the ones
/// scheduled profiles need, and remove any left from profiles that are gone
/// or no longer scheduled. Returns what could not be done.
pub fn reconcile(profiles: &[Profile]) -> Vec<String> {
    let mut errors = Vec::new();
    for profile in profiles.iter().filter(|p| p.schedule != Schedule::Manual) {
        if let Err(err) = apply(profile) {
            error_log!(SCHED, "the schedule for {} failed: {err}", profile.id);
            errors.push(err);
        }
    }
    let Some(entries) = unit_dir().and_then(|dir| std::fs::read_dir(dir).ok()) else {
        return errors;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name
            .strip_prefix(PREFIX)
            .and_then(|rest| rest.strip_suffix(".timer"))
        else {
            continue;
        };
        let wanted = profiles
            .iter()
            .any(|p| p.id == id && p.schedule != Schedule::Manual);
        if !wanted && let Err(err) = remove(id) {
            error_log!(SCHED, "a stale timer for {id} could not be removed: {err}");
            errors.push(err);
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b9c7a52-3c1e-4d4f-9a55-0f6f8f1c2d3e";

    #[test]
    fn timer_units_catch_up_missed_runs() {
        let timer = timer_text(ID, Schedule::Daily).unwrap();
        assert!(timer.contains("OnCalendar=daily\n"));
        assert!(
            timer.contains("Persistent=true\n"),
            "a slot missed while the computer was off runs at the next login"
        );
        assert!(timer.contains("WantedBy=timers.target\n"));
        assert!(
            timer_text(ID, Schedule::Hourly)
                .unwrap()
                .contains("OnCalendar=hourly\n")
        );
        assert!(
            timer_text(ID, Schedule::Weekly)
                .unwrap()
                .contains("OnCalendar=weekly\n")
        );
        assert_eq!(timer_text(ID, Schedule::Manual), None);
    }

    #[test]
    fn the_service_runs_the_scheduled_backup_gently() {
        let service = service_text(Path::new("/usr/bin/stellarshot"), ID).unwrap();
        assert!(service.contains(&format!(
            "ExecStart=\"/usr/bin/stellarshot\" --scheduled {ID}\n"
        )));
        assert!(service.contains("Type=oneshot\n"));
        assert!(service.contains("Nice=10\n"));
        assert!(service.contains("IOSchedulingClass=idle\n"));
    }

    #[test]
    fn exec_paths_are_escaped_for_systemd() {
        let service = service_text(
            Path::new("/home/alex/My Apps/100%/$bin/\"x\"/stellarshot"),
            ID,
        )
        .unwrap();
        assert!(
            service.contains(
                r#"ExecStart="/home/alex/My Apps/100%%/$$bin/\"x\"/stellarshot" --scheduled"#
            ),
            "{service}"
        );
        assert_eq!(
            service_text(Path::new("/tmp/bad\nExecStartPost=/bin/rm"), ID),
            None,
            "a newline could add a line to the unit"
        );
    }

    #[test]
    fn only_plain_ids_go_into_units() {
        for id in ["", "../../etc", "a b", "x;rm", &"a".repeat(65)] {
            assert_eq!(service_text(Path::new("/usr/bin/stellarshot"), id), None);
            assert_eq!(timer_text(id, Schedule::Daily), None, "{id:?}");
        }
    }
}
