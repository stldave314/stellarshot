// SPDX-License-Identifier: GPL-3.0-only

//! Repository passwords in the desktop keyring.
//!
//! Passwords are never written to Stellarshot's own settings. When the user
//! asks for one to be remembered it goes to the Secret Service (GNOME Keyring,
//! KWallet), which encrypts it with the login password. Scheduled backups need
//! this: they run with nobody there to type a password.
//!
//! Every function here treats a missing or locked keyring as "not
//! remembered". The keyring is a convenience; the password prompt must keep
//! working without it.

use std::future::Future;

use crate::app::APP_ID;
use crate::constants::KEYRING_TIMEOUT;
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
    let keyring = bounded(oo7::Keyring::new()).await?;
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
    let keyring = bounded(oo7::Keyring::new())
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not open the keyring for {what}: {err}"))?;
    let items = bounded(keyring.search_items(attrs))
        .await
        .inspect_err(|err| error_log!(KEYRING, "could not search the keyring for {what}: {err}"))?;
    let Some(item) = items.first() else {
        debug_log!(KEYRING, "no {what} remembered");
        return Ok(None);
    };
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
    let keyring = bounded(oo7::Keyring::new()).await?;
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
