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
use crate::debug::CONFIG;
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

/// Remember `secret` for the profile, replacing any earlier one.
pub async fn store(profile_id: &str, profile_name: &str, secret: &Secret) -> Result<(), String> {
    let keyring = bounded(oo7::Keyring::new()).await?;
    let label = format!("Stellarshot backup password: {profile_name}");
    bounded(keyring.create_item(&label, &attributes(profile_id), secret.expose(), true))
        .await
        .inspect_err(|err| {
            error_log!(CONFIG, "could not store a password in the keyring: {err}")
        })?;
    debug_log!(CONFIG, "stored the password for profile {profile_id}");
    Ok(())
}

/// The remembered password for the profile, if there is one and the keyring
/// can be read.
pub async fn load(profile_id: &str) -> Option<Secret> {
    let keyring = bounded(oo7::Keyring::new()).await.ok()?;
    let items = bounded(keyring.search_items(&attributes(profile_id)))
        .await
        .ok()?;
    let item = items.first()?;
    let secret = bounded(item.secret()).await.ok()?;
    let password = String::from_utf8(secret.as_bytes().to_vec()).ok()?;
    debug_log!(CONFIG, "loaded the password for profile {profile_id}");
    Some(Secret::new(password))
}

/// Forget the profile's password. Succeeds if there was none.
pub async fn forget(profile_id: &str) -> Result<(), String> {
    let keyring = bounded(oo7::Keyring::new()).await?;
    bounded(keyring.delete(&attributes(profile_id))).await?;
    debug_log!(CONFIG, "forgot the password for profile {profile_id}");
    Ok(())
}

/// The attributes that identify the web interface's own shared password: a
/// fixed key, not tied to any profile, since only one such password exists
/// per install.
fn web_attributes() -> [(&'static str, &'static str); 2] {
    [("application", APP_ID), ("purpose", "web-interface")]
}

/// Remember the web interface's shared password, replacing any earlier one.
pub async fn store_web_password(secret: &Secret) -> Result<(), String> {
    let keyring = bounded(oo7::Keyring::new()).await?;
    bounded(keyring.create_item(
        "Stellarshot web interface password",
        &web_attributes(),
        secret.expose(),
        true,
    ))
    .await
    .inspect_err(|err| {
        error_log!(
            CONFIG,
            "could not store the web interface password in the keyring: {err}"
        )
    })?;
    debug_log!(CONFIG, "stored the web interface password");
    Ok(())
}

/// The web interface's remembered shared password, if there is one and the
/// keyring can be read.
pub async fn load_web_password() -> Option<Secret> {
    let keyring = bounded(oo7::Keyring::new()).await.ok()?;
    let items = bounded(keyring.search_items(&web_attributes()))
        .await
        .ok()?;
    let item = items.first()?;
    let secret = bounded(item.secret()).await.ok()?;
    let password = String::from_utf8(secret.as_bytes().to_vec()).ok()?;
    debug_log!(CONFIG, "loaded the web interface password");
    Some(Secret::new(password))
}

/// Forget the web interface's shared password. Succeeds if there was none.
pub async fn forget_web_password() -> Result<(), String> {
    let keyring = bounded(oo7::Keyring::new()).await?;
    bounded(keyring.delete(&web_attributes())).await?;
    debug_log!(CONFIG, "forgot the web interface password");
    Ok(())
}
