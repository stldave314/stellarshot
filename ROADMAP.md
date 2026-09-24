# Roadmap

What is done, what is next, and what is deliberately out of scope.

This is a plan, not a promise. Items move when they turn out to be harder,
easier, or less useful than they looked. The design behind it is in
[docs/specs/](docs/specs/).

---

## M0 — Foundation (done)

The project can be built, tested, packaged and released on its own, and the
dangerous behaviour inherited from upstream is gone.

- [x] Own application ID, desktop entry, AppStream metadata and icons, with the
      original authors credited
- [x] Settings carried over from the upstream application ID, once, never over
      newer settings
- [x] **Safe repository deletion**: only the repository's own entries are
      removed, never the folder around them
- [x] **Safe repository creation**: a folder holding other files is refused; an
      existing repository is opened, never re-initialised
- [x] Folder names with spaces handled correctly (file-chooser URLs decoded)
- [x] Errors shown to the user instead of being logged and dropped
- [x] Developer logging to a file, compiled out of every release build, and a
      script that proves it
- [x] Zero `cargo check` and `cargo clippy` warnings, enforced in CI
- [x] Locale parity enforced by a test; every locale complete
- [x] `.deb`, `.rpm` and tarball packages from `install.sh`, built on every push
      and attached to tagged releases
- [x] README, security policy, validation guide, contributing guide, changelog

## M1 — Engine (done)

A new backup engine behind one module, on the current rustic release.

- [x] rustic 0.2 → 0.13, isolated in `src/engine/` so no other code touches it
- [x] Reads (listing, browsing) on background threads; writes (backup, restore,
      maintenance) in a child process, `stellarshot --run`, so a backup can be
      cancelled and a crash cannot take the window with it
- [x] Progress reporting: phase, bytes done and total
- [x] A per-repository lock shared by the window and scheduled runs
- [x] Typed errors, each mapped to a message and an action
- [x] Tests: back up and restore a tree with awkward names, links and
      permissions, and compare it byte for byte; kill a backup midway and prove
      the repository is still sound

## M2 — Backup profiles and the main screen (done)

- [x] Backup profiles: name, destination, folders to include and exclude
      (schedule and retention settings arrive with M5)
- [x] Status-first main screen: last backup, **Back Up Now** (next backup
      arrives with M5, **Restore…** with M4)
- [x] A **Create a Backup…** button on the empty main screen
- [x] Setup wizard: what, where, password ("when" arrives with M5)
- [x] Live estimate of the backup size that subtracts excluded folders inside
      included ones, computed from the same file list the backup will read
- [x] Passwords in the Secret Service keyring, "Remember" on by default
- [x] "Remove from Stellarshot" and "Delete repository and all data" as separate
      actions

## M3 — Storage locations and import (done)

- [x] USB drives identified by volume UUID, so a drive mounted somewhere new
      still works
- [x] SFTP (through rclone, with host-key checking)
- [x] Google Drive with sign-in inside the app, and any of your own rclone
      remotes, through Stellarshot's own rclone configuration
- [ ] A OneDrive sign-in button of its own (OneDrive works today as one of
      your rclone remotes)
- [x] Existing repositories detected when a destination is chosen
- [x] Import from Déjà Dup (restic-format backups only; passwords are never
      imported)

## M4 — Restore

- [ ] Browse any snapshot as a file tree, with search
- [ ] Restore selected files and folders to their original place or elsewhere
- [ ] Conflict handling: overwrite, keep both, or skip, with a dry-run summary
      before anything is written
- [ ] Every version of a file, with identical versions collapsed
- [ ] Deleted files: what is in your backups but no longer on disk
- [ ] Compare any two snapshots

## M5 — Automation

- [ ] Scheduled backups through systemd user timers
- [ ] Retention: keep forever, smart (7 daily, 4 weekly, 12 monthly), or custom
- [ ] Desktop notifications for scheduled runs that fail
- [ ] Periodic integrity checks

## 1.0 — Hardening

- [ ] Every locale complete and reviewed
- [ ] Accessibility pass
- [ ] Screenshots of every screen
- [ ] End-to-end "restore actually works" tests in CI

---

## After 1.0

- Flatpak (it needs full filesystem access and an rclone binary, which works
  against the sandbox, so it waits)
- "Revert to previous version" from the COSMIC file manager
- Back up automatically when a drive is plugged in
- A background service, if timers turn out not to be enough
- Converting Déjà Dup duplicity backups

## Out of scope

- **Reading duplicity or borg repositories.** rustic reads the restic format
  only.
- **Importing passwords** from Déjà Dup or anywhere else.
- **Storing passwords in Stellarshot's own settings.** They belong in the
  keyring or nowhere.
