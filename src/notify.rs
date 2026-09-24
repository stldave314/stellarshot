// SPDX-License-Identifier: GPL-3.0-only

//! Desktop notifications for scheduled runs, through
//! `org.freedesktop.Notifications`.

use std::collections::HashMap;
use std::process::Command;

use futures_util::StreamExt;
use zbus::zvariant::Value;

use crate::app::APP_ID;
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

/// Show a notification that something went wrong with a backup, and wait
/// (at most [`NOTIFICATION_WAIT`]) for it to be clicked or dismissed. A
/// click opens the backup in Stellarshot.
pub async fn failure(summary: &str, body: &str, open_label: &str, profile_id: &str) {
    if let Err(err) = show_and_wait(summary, body, open_label, profile_id).await {
        error_log!(SCHED, "could not show a notification: {err}");
    }
}

async fn show_and_wait(
    summary: &str,
    body: &str,
    open_label: &str,
    profile_id: &str,
) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = NotificationsProxy::new(&connection).await?;
    // Listen before showing it, so a quick click cannot be missed.
    let mut clicks = proxy.receive_action_invoked().await?;
    let mut closes = proxy.receive_notification_closed().await?;
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
    if tokio::time::timeout(NOTIFICATION_WAIT, wait)
        .await
        .unwrap_or(false)
    {
        open_profile(profile_id);
    }
    Ok(())
}

/// Start Stellarshot on `profile_id`'s page. It is started through systemd
/// as a unit of its own: a scheduled run is a service, and systemd stops
/// everything a service started when the service ends.
fn open_profile(profile_id: &str) {
    let Ok(program) = crate::schedule::executable() else {
        return;
    };
    let launched = Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", "--"])
        .arg(&program)
        .args(["--profile", profile_id])
        .status()
        .is_ok_and(|status| status.success());
    debug_log!(SCHED, "opened {profile_id} from a notification: {launched}");
}
