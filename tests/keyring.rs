// SPDX-License-Identifier: GPL-3.0-only

//! Passwords round-trip through a real Secret Service.
//!
//! This needs a running Secret Service with an unlocked default collection:
//! the desktop session's keyring locally, and an unlocked gnome-keyring inside
//! `dbus-run-session` in CI. It fails, rather than skips, when there is none,
//! so a green run always means the keyring was really exercised.

use stellarshot::engine::Secret;
use stellarshot::keyring;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn keyring_round_trip() {
    let runtime = runtime();
    // A profile ID no real profile will ever have, so the test cannot touch
    // the user's own saved passwords.
    let profile = format!("test-{}", uuid::Uuid::new_v4());

    runtime.block_on(async {
        assert!(
            keyring::load(&profile).await.is_none(),
            "a fresh profile has nothing stored"
        );

        keyring::store(&profile, "Keyring test", &Secret::new("first"))
            .await
            .expect("a Secret Service must be running and unlocked for this test");
        assert_eq!(
            keyring::load(&profile).await.map(|s| s.expose().to_owned()),
            Some("first".to_owned())
        );

        // Storing again replaces, never duplicates.
        keyring::store(&profile, "Keyring test", &Secret::new("second"))
            .await
            .unwrap();
        assert_eq!(
            keyring::load(&profile).await.map(|s| s.expose().to_owned()),
            Some("second".to_owned())
        );

        keyring::forget(&profile).await.unwrap();
        assert!(keyring::load(&profile).await.is_none(), "forgotten");
        keyring::forget(&profile)
            .await
            .expect("forgetting twice is not an error");
    });
}
