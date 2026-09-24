// SPDX-License-Identifier: GPL-3.0-only

//! The rclone configuration holds cloud tokens, so it must already be private
//! when rclone writes a token into it, not just afterwards. A stand-in
//! `rclone` records the file's mode at the moment it is run.
//!
//! This is its own test binary because it changes `PATH` for the whole
//! process, which would send other tests to the stand-in.

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use stellarshot::engine::rclone;

fn mode(path: &Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn the_configuration_is_private_before_rclone_writes_a_token() {
    let dir = tempfile::TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let seen = dir.path().join("mode-seen-by-rclone");
    // Called as `rclone --config <file> config create …`: record the file's
    // mode, then write a token the way rclone would.
    let stub = bin.join("rclone");
    std::fs::write(
        &stub,
        format!(
            "#!/bin/sh\nstat -c %a \"$2\" > '{}'\nprintf '[probe]\\ntoken = secret\\n' >> \"$2\"\n",
            seen.display()
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
    std::fs::write(&config, "[old]\ntype = local\n").unwrap();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o644)).unwrap();

    rclone::sign_in(&config, "probe", "drive", &[]).unwrap();

    let seen = std::fs::read_to_string(&seen).expect("the stand-in rclone ran");
    assert_eq!(
        seen.trim(),
        "600",
        "the file was readable by others when the token went in"
    );
    assert_eq!(mode(&config), 0o600);
}
