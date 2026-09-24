# Security

Stellarshot handles two things that matter: copies of your files, and the
password that encrypts them. This document says what it protects, what it does
not, and how to report a problem.

## Reporting a vulnerability

**Please do not open a public issue for a security problem.** Use GitHub's
private vulnerability reporting instead: the **Security** tab of this
repository, then **Report a vulnerability**. Only the maintainer can see the
report.

Include what you did, what happened, and what you expected. A proof of concept
helps, but is not required. You will get an acknowledgement, and a fix or an
explanation, as promptly as circumstances allow. Credit is given in the
changelog unless you would rather it was not.

## Supported versions

Only the latest release receives fixes. Stellarshot is pre-1.0; see the status
note in the [README](README.md).

## What protects your backups

- **Encryption.** Repositories use the restic format: data is encrypted with
  AES-256 in counter mode and authenticated with Poly1305-AES, and the key is
  derived from your password with scrypt. The storage location only ever sees
  ciphertext. Stellarshot does not implement any of this itself. It is done by
  [rustic](https://rustic.cli.rs/), which implements the published restic
  format.
- **No lock-in.** Every repository can be read with the `restic` or `rustic`
  command-line tools, so a bug in, or the end of, this app does not strand your
  backups.
- **Passwords are never written to Stellarshot's own files.** A password is
  either typed each session or, when you choose **Remember password**, kept in
  the desktop keyring (the Secret Service: GNOME Keyring or KWallet), which
  encrypts it with your login password. The keyring item is labelled with the
  backup's name and removed when you remove the backup. Backups run in a
  separate process; the password is handed to it on its standard input, never
  on its command line or in its environment, which other programs running as
  you can read from `/proc`.
- **Deletion only touches the repository.** Deleting a backup's data needs its
  name typed exactly, holds the repository's lock, removes the entries the
  repository format creates and nothing else, and does not follow symlinks. Creating a repository in a folder that already holds other files is
  refused. Both rules exist because the upstream code could have deleted a home
  directory; see the [changelog](CHANGELOG.md).
- **Cloud sign-ins stay in Stellarshot's own file.** Signing in to Google Drive
  stores rclone's access token in `~/.config/stellarshot/rclone.conf`, readable
  only by you. Your own `~/.config/rclone/rclone.conf` is never written; a
  remote you pick from it is copied, so removing a Stellarshot backup cannot
  break a remote you use for something else.
- **SSH servers must be known.** SFTP backups check the server's host key
  against your `~/.ssh/known_hosts`, so an unknown server or a changed key is
  refused rather than trusted. Stellarshot never stores an SSH password; it
  uses your SSH agent or keys.
- **Déjà Dup's password is never read.** Importing reads Déjà Dup's settings
  only; the module that does it has no keyring code at all.
- **One writer per repository.** Every backup, restore, check and deletion
  holds an exclusive lock on the repository, so two of them can never write
  to it at once. The lock lives in your private runtime directory and is
  released by the kernel when the holder exits or crashes.
- **Release builds cannot carry debug logging.** Developer logging is compiled
  out by the `release-build` feature that every packaging target passes. CI
  proves this by checking the built binary, not by trusting the source.

## What it does not protect against

- **Anyone who can act as your user account.** Stellarshot runs as you. Malware
  running as you can read your files directly, read the password while a
  repository is unlocked, or delete your backups if they are on a disk you can
  write to. Keep at least one backup somewhere your everyday account cannot
  delete, such as a drive you unplug.
- **Anyone who can unlock your keyring.** A remembered password is as safe as
  your login keyring. If that matters for a particular backup, turn
  **Remember password** off for it; you will be asked each session instead.
- **A forgotten password.** There is no recovery. Nobody, including the
  maintainer, can decrypt a repository without its password.
- **Metadata at the storage location.** Whoever holds the storage can see how
  many files the repository has, their approximate sizes, and when snapshots
  were taken. They cannot see names or contents.
- **A weak password.** scrypt slows guessing down; it cannot make a short or
  reused password strong.
- **Data that was never backed up.** Files you cannot read (other users'
  files, some system files) are skipped.

## Handling of your data

Stellarshot has no telemetry and sends nothing anywhere except to the storage
location you choose. Network connections are made by rclone, to the server or
cloud service you configured, and by your browser during a sign-in.
