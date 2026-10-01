// SPDX-License-Identifier: GPL-3.0-only

//! Desktop notifications for scheduled runs, through
//! `org.freedesktop.Notifications`.

use std::collections::HashMap;

use futures_util::StreamExt;
use tokio::process::Command;
use zbus::zvariant::Value;

use crate::constants::APP_ID;
use crate::constants::NOTIFICATION_WAIT;
use crate::debug::SCHED;
use crate::{debug_log, error_log};

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;

    #[zbus(signal)]
    fn notification_closed(&self, id: u32, reason: u32) -> zbus::Result<()>;
}

/// Show a notification that something went wrong with a backup. A click
/// opens the backup in Stellarshot. Returns whether it was actually shown,
/// so a caller that only wants to record something once a real
/// notification reached the user — not once merely attempting to — knows
/// not to record it on a failed attempt.
///
/// Waiting for the click (at most [`NOTIFICATION_WAIT`]) is handed to a
/// process of its own (see [`await_main`]), so a scheduled run ends as soon
/// as its work does: while its unit is still active, its next timer slot
/// is skipped. Only if that cannot be started does this wait itself.
pub async fn failure(summary: &str, body: &str, open_label: &str, profile_id: &str) -> bool {
    let shown = async {
        let connection = zbus::Connection::session().await?;
        let proxy = NotificationsProxy::new(&connection).await?;
        // Listen before showing it, so a quick click is not missed while
        // this process is still the one waiting.
        let clicks = proxy.receive_action_invoked().await?;
        let closes = proxy.receive_notification_closed().await?;
        let id = show(&proxy, summary, body, open_label).await?;
        zbus::Result::Ok((id, clicks, closes))
    };
    let (id, clicks, closes) = match shown.await {
        Ok(shown) => shown,
        Err(err) => {
            error_log!(SCHED, "could not show a notification: {err}");
            return false;
        }
    };
    if !hand_off(id, profile_id).await && wait_for_click(id, clicks, closes).await {
        open_profile(profile_id).await;
    }
    true
}

async fn show(
    proxy: &NotificationsProxy<'_>,
    summary: &str,
    body: &str,
    open_label: &str,
) -> zbus::Result<u32> {
    let hints = HashMap::from([
        ("desktop-entry", Value::from(APP_ID)),
        // Critical: it stays until the user sees it.
        ("urgency", Value::from(2u8)),
    ]);
    let id = proxy
        .notify(
            "Stellarshot",
            0,
            APP_ID,
            summary,
            body,
            &["default", open_label],
            hints,
            0,
        )
        .await?;
    debug_log!(SCHED, "notification {id} shown");
    Ok(id)
}

/// Start `stellarshot --await-notification <id> <profile>` as a transient
/// unit of its own: started from the scheduled run's service directly, it
/// would be stopped with it. Whether it started.
async fn hand_off(id: u32, profile_id: &str) -> bool {
    let Ok(program) = crate::timers::executable() else {
        return false;
    };
    let started = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .arg(&program)
        .arg("--await-notification")
        .arg(id.to_string())
        .arg(profile_id)
        .status()
        .await
        .is_ok_and(|status| status.success());
    debug_log!(SCHED, "handed notification {id} to its own unit: {started}");
    started
}

/// Whether notification `id` was clicked, rather than closed or left alone
/// for [`NOTIFICATION_WAIT`].
async fn wait_for_click(
    id: u32,
    mut clicks: ActionInvokedStream,
    mut closes: NotificationClosedStream,
) -> bool {
    let wait = async {
        loop {
            tokio::select! {
                Some(signal) = clicks.next() => {
                    if let Ok(args) = signal.args() && args.id == id {
                        return true;
                    }
                }
                Some(signal) = closes.next() => {
                    if let Ok(args) = signal.args() && args.id == id {
                        return false;
                    }
                }
                else => return false,
            }
        }
    };
    tokio::time::timeout(NOTIFICATION_WAIT, wait)
        .await
        .unwrap_or(false)
}

/// `stellarshot --await-notification <id> <profile>`: wait for a click on a
/// notification a scheduled run showed, and open the backup if it comes.
/// A click in the moment between the run showing it and this starting to
/// listen is missed; the backup can still be opened from the menu.
pub fn await_main(args: &[String]) -> std::process::ExitCode {
    use std::process::ExitCode;
    crate::debug::init(crate::debug::Role::Scheduled);
    let (Some(id), Some(profile_id)) = (
        args.first().and_then(|id| id.parse::<u32>().ok()),
        args.get(1).filter(|id| crate::profile::valid_id(id)),
    ) else {
        error_log!(
            SCHED,
            "--await-notification: expected a notification and a backup ID"
        );
        return ExitCode::FAILURE;
    };
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return ExitCode::FAILURE;
    };
    runtime.block_on(async {
        let listening = async {
            let connection = zbus::Connection::session().await?;
            let proxy = NotificationsProxy::new(&connection).await?;
            let clicks = proxy.receive_action_invoked().await?;
            let closes = proxy.receive_notification_closed().await?;
            zbus::Result::Ok((connection, clicks, closes))
        };
        match listening.await {
            Ok((_connection, clicks, closes)) => {
                if wait_for_click(id, clicks, closes).await {
                    open_profile(profile_id).await;
                }
                ExitCode::SUCCESS
            }
            Err(err) => {
                error_log!(SCHED, "--await-notification: {err}");
                ExitCode::FAILURE
            }
        }
    })
}

/// Start Stellarshot on `profile_id`'s page. It is started through systemd
/// as a unit of its own: a scheduled run is a service, and systemd stops
/// everything a service started when the service ends.
async fn open_profile(profile_id: &str) {
    let Ok(program) = crate::timers::executable() else {
        return;
    };
    let launched = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .arg(&program)
        .args(["--profile", profile_id])
        .status()
        .await
        .is_ok_and(|status| status.success());
    debug_log!(SCHED, "opened {profile_id} from a notification: {launched}");
}
