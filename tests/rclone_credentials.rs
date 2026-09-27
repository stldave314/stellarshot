// SPDX-License-Identifier: GPL-3.0-only

//! `sign_in`'s OAuth client ID and secret must never reach argv (readable by
//! any local user through `/proc/<pid>/cmdline` for as long as the process
//! runs), only the environment (`/proc/<pid>/environ`, owner-only). A
//! stand-in `rclone` records both its own argv and its environment so this
//! can be checked directly, rather than trusted by reading the source.
//!
//! Its own test binary because it changes `PATH` for the whole process,
//! which would send other tests to the stand-in — the same reason
//! `rclone_permissions.rs` is separate too.

use std::os::unix::fs::PermissionsExt;

use stellarshot::engine::rclone;

#[test]
fn the_client_secret_reaches_rclone_through_the_environment_not_argv() {
    let dir = tempfile::TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let seen_argv = dir.path().join("argv-seen-by-rclone");
    let seen_env = dir.path().join("env-seen-by-rclone");
    // Called as `rclone --config <file> config create -- probe drive
    // scope=drive`: record argv and the environment, then succeed the way
    // rclone would.
    let stub = bin.join("rclone");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nenv > '{}'\nexit 0\n",
            seen_argv.display(),
            seen_env.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin.clone()];
    paths.extend(std::env::split_paths(&path));
    // SAFETY: this binary runs only this test, on one thread.
    unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };

    let config = dir.path().join("rclone.conf");
    std::fs::write(&config, "").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

    rclone::sign_in(
        &config,
        "probe",
        "drive",
        &["scope=drive"],
        Some(("my-client-id", "hunter2")),
    )
    .unwrap();

    let argv = std::fs::read_to_string(&seen_argv).expect("the stand-in rclone ran");
    assert!(
        !argv.contains("hunter2"),
        "the secret must never be on argv: {argv:?}"
    );
    assert!(
        !argv.contains("my-client-id"),
        "the client ID must never be on argv either: {argv:?}"
    );

    let env = std::fs::read_to_string(&seen_env).unwrap();
    assert!(
        env.contains("RCLONE_DRIVE_CLIENT_ID=my-client-id"),
        "the client ID must reach rclone through the environment: {env:?}"
    );
    assert!(
        env.contains("RCLONE_DRIVE_CLIENT_SECRET=hunter2"),
        "the secret must reach rclone through the environment: {env:?}"
    );
}

#[test]
fn signing_in_without_credentials_sets_no_client_environment_variables() {
    let dir = tempfile::TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let seen_env = dir.path().join("env-seen-by-rclone");
    let stub = bin.join("rclone");
    std::fs::write(
        &stub,
        format!("#!/bin/sh\nenv > '{}'\nexit 0\n", seen_env.display()),
    )
    .unwrap();
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin.clone()];
    paths.extend(std::env::split_paths(&path));
    // SAFETY: this binary runs only this test, on one thread.
    unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };

    let config = dir.path().join("rclone.conf");
    std::fs::write(&config, "").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o600)).unwrap();

    rclone::sign_in(&config, "probe", "local", &[], None).unwrap();

    let env = std::fs::read_to_string(&seen_env).unwrap();
    assert!(
        !env.contains("CLIENT_ID") && !env.contains("CLIENT_SECRET"),
        "no credential variable should be set when signing in without one: {env:?}"
    );
}
