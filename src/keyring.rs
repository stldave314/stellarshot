// SPDX-License-Identifier: GPL-3.0-only

//! Repository passwords in the desktop keyring.
//!
//! Passwords are never written to Stellarshot's own settings. When the user
//! asks for one to be remembered it goes to the Secret Service (GNOME Keyring,
//! `KWallet`), which encrypts it with the login password. Scheduled backups need
//! this: they run with nobody there to type a password.
//!
//! A keyring that is locked (as it is after logging in without typing the
//! login password: automatic login, a fingerprint) holds the passwords but
//! will neither give them out nor take a new one, and does not offer to
//! unlock by itself: it has to be asked. The window asks, which makes the
//! keyring show its own unlock prompt, because a person is there to answer
//! it (see [`allow_unlock_prompt`]). A scheduled run never does: nobody is.
//!
//! Beyond that, every function here treats a keyring that is missing, or
//! still locked, as "not remembered". The keyring is a convenience; the
//! password prompt must keep working without it.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::constants::APP_ID;
use crate::constants::{KEYRING_DISMISSAL_MEMORY, KEYRING_TIMEOUT, KEYRING_UNLOCK_TIMEOUT};
use crate::debug::KEYRING;
use crate::engine::Secret;
use crate::{debug_log, error_log};

/// Run a keyring request, giving up after [`KEYRING_TIMEOUT`].
async fn bounded<T>(request: impl Future<Output = oo7::Result<T>>) -> Result<T, String> {
    match tokio::time::timeout(KEYRING_TIMEOUT, request).await {
        Ok(result) => result.map_err(|err| err.to_string()),
        Err(_) => Err("the keyring did not answer".to_owned()),
    }
}

/// Whether this process may ask a locked keyring to unlock.
static MAY_PROMPT: AtomicBool = AtomicBool::new(false);

/// One unlock at a time: two requests arriving together (a page loading its
/// password while another saves one) would otherwise each raise a prompt.
/// The second waits here and then finds the keyring already unlocked.
static UNLOCKING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// When the last unlock prompt was dismissed, or went unanswered. Requests
/// queued behind it (every backup's page asking at once) then fail at once,
/// rather than each raise the prompt the user just said no to.
static DISMISSED: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Whether a prompt dismissed at `dismissed` is recent enough at `now` not
/// to ask again yet.
fn recently_dismissed(dismissed: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    dismissed.is_some_and(|at| now.saturating_duration_since(at) < KEYRING_DISMISSAL_MEMORY)
}

fn note_dismissed(at: Option<std::time::Instant>) {
    *DISMISSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = at;
}

/// Let this process ask a locked keyring to unlock, which makes the keyring
/// show its own password prompt. For the window only, called once as it
/// starts: a scheduled run, or the child process a backup runs in, has
/// nobody to answer a prompt, and must go on treating a locked keyring as
/// one it cannot use.
pub fn allow_unlock_prompt() {
    MAY_PROMPT.store(true, Ordering::Relaxed);
}

/// The keyring, unlocked first if it is locked and this process may ask
/// (see [`allow_unlock_prompt`]). `what` names the request for the log.
///
/// A prompt that is dismissed, or not answered within
/// [`KEYRING_UNLOCK_TIMEOUT`], is an error like any other here: the caller
/// falls back to what it does without a keyring.
async fn open(what: &str) -> Result<oo7::Keyring, String> {
    let keyring = bounded(oo7::Keyring::new()).await?;
    if !MAY_PROMPT.load(Ordering::Relaxed) {
        return Ok(keyring);
    }
    let _one_at_a_time = UNLOCKING.lock().await;
    if !bounded(keyring.is_locked()).await? {
        return Ok(keyring);
    }
    let dismissed = *DISMISSED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if recently_dismissed(dismissed, std::time::Instant::now()) {
        return Err("the keyring's unlock prompt was just dismissed".to_owned());
    }
    debug_log!(
        KEYRING,
        "the keyring is locked: asking it to unlock for {what}"
    );
    match tokio::time::timeout(KEYRING_UNLOCK_TIMEOUT, keyring.unlock()).await {
        Ok(Ok(())) => {
            debug_log!(KEYRING, "the keyring was unlocked");
            note_dismissed(None);
            Ok(keyring)
        }
        Ok(Err(err)) => {
            error_log!(KEYRING, "the keyring was not unlocked for {what}: {err}");
            note_dismissed(Some(std::time::Instant::now()));
            Err(err.to_string())
        }
        Err(_) => {
            note_dismissed(Some(std::time::Instant::now()));
            error_log!(
                KEYRING,
                "the keyring's unlock prompt was not answered for {what}"
            );
            Err("the keyring's unlock prompt was not answered".to_owned())
        }
    }
}

/// The attributes that identify one profile's password.
fn attributes(profile_id: &str) -> [(&'static str, &str); 2] {
    [("application", APP_ID), ("profile", profile_id)]
}

/// Store `secret` under `label`, identified by `attrs`, replacing any
/// earlier item with the same attributes. `what` names it for the log.
///
/// `attrs` is a fixed-size array, not a slice: `oo7`'s own `AsAttributes`
/// is implemented for `[(K, V)]`, an *unsized* type, so passing a slice
/// reference to its generic, implicitly-`Sized` type parameter does not
/// type-check even though the trait impl itself would otherwise apply.
async fn store_item(
    label: &str,
    attrs: &[(&str, &str); 2],
    secret: &Secret,
    what: &str,
) -> Result<(), String> {
    let keyring = open(what).await?;
    bounded(keyring.create_item(label, attrs, secret.expose(), true))
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not store {what} in the keyring: {err}"))?;
    debug_log!(KEYRING, "stored {what}");
    Ok(())
}

/// The item identified by `attrs`: `Ok(None)` if the keyring holds none,
/// `Err` if the keyring could not be reached or read at all (not unlocked
/// yet at login, no Secret Service, a timeout). `what` names it for the
/// log; every failure is logged.
async fn load_item(attrs: &[(&str, &str); 2], what: &str) -> Result<Option<Secret>, String> {
    let keyring = open(what)
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not open the keyring for {what}: {err}"))?;
    let items = bounded(keyring.search_items(attrs))
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not search the keyring for {what}: {err}"))?;
    let Some(item) = items.first() else {
        debug_log!(KEYRING, "no {what} remembered");
        return Ok(None);
    };
    if items.len() > 1 {
        // Each store replaces the one before, so this should not happen;
        // the first is used, and the log says which question to ask.
        error_log!(
            KEYRING,
            "{} items match {what}; using the first",
            items.len()
        );
    }
    let secret = bounded(item.secret())
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not read {what} from the keyring: {err}"))?;
    match String::from_utf8(secret.as_bytes().to_vec()) {
        Ok(password) => {
            debug_log!(KEYRING, "loaded {what}");
            Ok(Some(Secret::new(password)))
        }
        Err(err) => {
            error_log!(KEYRING, "{what} in the keyring was not valid UTF-8");
            // The bytes are the password, or most of it: wiped, not just
            // dropped.
            zeroize::Zeroize::zeroize(&mut err.into_bytes());
            Ok(None)
        }
    }
}

/// Forget the item identified by `attrs`. Succeeds if there was none.
async fn forget_item(attrs: &[(&str, &str); 2], what: &str) -> Result<(), String> {
    let keyring = open(what).await?;
    bounded(keyring.delete(attrs)).await?;
    debug_log!(KEYRING, "forgot {what}");
    Ok(())
}

/// Remember `secret` for the profile, replacing any earlier one.
pub async fn store(profile_id: &str, profile_name: &str, secret: &Secret) -> Result<(), String> {
    let label = format!("Stellarshot backup password: {profile_name}");
    store_item(
        &label,
        &attributes(profile_id),
        secret,
        &format!("the password for profile {profile_id}"),
    )
    .await
}

/// The remembered password for the profile, if there is one and the keyring
/// can be read.
pub async fn load(profile_id: &str) -> Option<Secret> {
    load_checked(profile_id).await.ok().flatten()
}

/// [`load`], telling "nothing remembered" (`Ok(None)`) apart from "the
/// keyring could not be reached" (`Err`): a scheduled run at login can start
/// before the keyring is unlocked, which is a reason to try again at the
/// next slot, not to report that no password is remembered.
pub async fn load_checked(profile_id: &str) -> Result<Option<Secret>, String> {
    load_item(
        &attributes(profile_id),
        &format!("the password for profile {profile_id}"),
    )
    .await
}

/// Forget the profile's password. Succeeds if there was none.
pub async fn forget(profile_id: &str) -> Result<(), String> {
    forget_item(
        &attributes(profile_id),
        &format!("the password for profile {profile_id}"),
    )
    .await
}

/// The attributes that identify one profile's REST server password: a
/// different second attribute from [`attributes`], so a search for the
/// repository password can never find this one instead.
fn rest_attributes(profile_id: &str) -> [(&'static str, &str); 2] {
    [("application", APP_ID), ("rest-server", profile_id)]
}

/// Remember the password a REST server backup logs in with, so the address
/// saved in the settings can leave it out.
pub async fn store_rest_password(
    profile_id: &str,
    profile_name: &str,
    secret: &Secret,
) -> Result<(), String> {
    let label = format!("Stellarshot REST server password: {profile_name}");
    store_item(
        &label,
        &rest_attributes(profile_id),
        secret,
        &format!("the REST server password for profile {profile_id}"),
    )
    .await
}

/// The remembered REST server password, as [`load_checked`] tells them
/// apart.
pub async fn load_rest_password(profile_id: &str) -> Result<Option<Secret>, String> {
    load_item(
        &rest_attributes(profile_id),
        &format!("the REST server password for profile {profile_id}"),
    )
    .await
}

/// Forget the profile's REST server password. Succeeds if there was none.
pub async fn forget_rest_password(profile_id: &str) -> Result<(), String> {
    forget_item(
        &rest_attributes(profile_id),
        &format!("the REST server password for profile {profile_id}"),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dismissed_prompt_is_not_raised_again_straight_away() {
        let now = std::time::Instant::now();
        assert!(!recently_dismissed(None, now));
        assert!(recently_dismissed(Some(now), now));
        let later = now + KEYRING_DISMISSAL_MEMORY + std::time::Duration::from_secs(1);
        assert!(!recently_dismissed(Some(now), later));
    }
}
