# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Browse folders with their sizes**: a "Browse…" button on each included
  folder in the wizard's What step opens a disk-usage tree rooted there,
  sizing each row on demand as it is expanded, with a mark for whether a
  folder is going in whole, left out, or partly one and partly the other,
  and a button to flip it.
- **Download straight from a snapshot, without restoring anything**: a
  "Download…" button on every row of the Browse tab saves a file as it was
  backed up, or a folder as a `.tar.gz`; the same button on an older version
  of a file downloads that version specifically.
- **Conditions for a scheduled backup**: only on mains power, only above a
  battery level, not on a metered connection, only on a trusted Wi-Fi
  network or a VPN. A scheduled run that finds a condition unmet is skipped
  quietly, the same way an unreachable destination already was; the next
  slot tries again.
- **A COSMIC panel applet** (`stellarshot-applet`), addable from COSMIC
  Settings: a status icon and a popup with each backup's status and an
  Open Stellarshot button. Closing the main window now minimizes it to the
  panel instead of quitting; launching Stellarshot again reopens the same
  window rather than a second one.
- **Hooks**: run a command or program before and after a backup, from a
  new page reached through Manage → Hooks. Before the backup, after a
  success, after a failure, or after either. A failing `Before` hook stops
  the backup from running.
- **Start a backup when its drive is connected**: a 4th frequency choice
  for a backup whose destination is a removable drive, alongside hourly,
  daily and weekly. No fixed schedule to miss and catch up on — it just
  runs the next time the drive is plugged in.

### Fixed

- **"Delete backup and all data" no longer refuses a backup whose data was
  already removed by hand** (directly at the storage, outside Stellarshot):
  that used to be treated the same as "this is not a repository" and
  refused outright, with no way past it to forget the backup locally.
  Deleting something already gone now succeeds as a no-op.
- **A crash or power loss during a write could leave a systemd unit file
  empty**, silently breaking that backup's schedule until it was saved
  again: writing one now goes to a temporary file, fsynced, then renamed
  into place and the directory fsynced too, rather than a plain write with
  neither step. The same fix applies to writing a settings export.
- **Every typed destination path (an SSH server, a Google Drive folder, a
  custom rclone remote, a REST server URL) now checks itself when you press
  Enter**, not only when Check or Next is explicitly clicked: a folder
  picker or a drive selection already checked themselves the moment
  something was chosen, but a typed path stayed silent about what was
  already there — a backup already at that path, or files left over from
  an earlier interrupted one — until a button was found and pressed.
- **The wizard's Google sign-in button now reads as acting on the
  credentials above it**: entering a client ID and secret used to sit
  below the Sign In button rather than above it, so nothing connected the
  two, and it was easy to enter credentials and never realize Sign In was
  the next step.

## [0.4.0] - 2026-09-25

What is backed up, its history, and getting it back, and storage and
credentials: exclusions, snapshot pinning, restore options, bandwidth
limits, a password from a command, REST server destinations, several keys
per backup, and append-only mode.

### Added

- **Global exclusions**, under **Settings**: glob patterns such as
  `node_modules` or `target`, left out of every backup at once instead of
  added to each one.
- **Leave out cache folders and honor `.gitignore`**, as two toggles in the
  wizard's Advanced section.
- **Skip empty backups**: a toggle so a backup records no snapshot when
  nothing has changed since the last one.
- **Pin a snapshot** from its own backup's page, so cleaning up never removes
  it, however old it gets.
- **Restore options**: verify existing files by content instead of trusting
  their size and date, and choose how ownership is restored (as backed up,
  numeric IDs, or not at all) — both in the restore sheet's Advanced section.
- **A bandwidth limit** per backup, in rclone's own syntax, set in the
  wizard's Where step for a destination reached through rclone.
- **A password from a command**, instead of the keyring, for a password
  manager with a command-line client — set from the profile page's own
  **Manage** section.
- **A REST server destination**: rest-server and rustic-server, reached
  directly instead of through rclone, with a URL field in the wizard's Where
  step. Everything but full deletion of the repository's data from
  Stellarshot works, which is refused rather than attempted (see
  ROADMAP.md's 0.4 section for why).
- **Cache location**, under **Settings**: another folder, or no local cache
  at all, for every repository this computer opens.
- **Change a backup's password**, from the profile page's own **Manage**
  section. If the old password was remembered, the keyring entry is
  replaced with the new one rather than left stale.
- **Append-only mode**, offered as a toggle when creating a backup: rustic
  itself then refuses to delete a snapshot from it. Stellarshot offers no
  way to turn it off again once the backup exists.

### Fixed

- **rustic's and rclone's own diagnostics now actually reach a log**, at
  `/tmp/stellarshot-backend.log` as well as stderr. `set_logger` set up a
  `tracing` subscriber for them, but rustic_core and rustic_backend (and the
  rclone process they run) only ever log through the separate `log` crate,
  and nothing bridged the two — every line they logged, including the one
  rclone itself prints when a backup to it fails, went nowhere. Found this
  way: a real first backup to Google Drive failed with "Backoff failed,
  please check the logs for more information", and there were none to check.
  The first version of this fix only reached the window's own process; every
  real backup runs in a `--run` child or a `--scheduled` run instead, found
  during this release's own code review, so both now install the bridge too.
- **A backup's password, changed from the profile page, now goes through
  the same write lock and child process as every other change to a
  repository**, rather than running in the window's own process unlocked —
  it could otherwise race a scheduled backup also touching the repository's
  keys. A keyring update that then fails is reported instead of silently
  leaving the keyring with the old, now-wrong password.
- **An append-only repository opened rather than created by Stellarshot is
  now recognized as one**, read back from the repository itself instead of
  assumed `false`; Clean Up Now, pinning and deleting a single snapshot are
  hidden for such a backup rather than offered and then refused by rustic.
- **A settings export no longer includes a REST destination's credentials**,
  matching the export's own promise never to include a password.
- **A missing or unreadable exclude-pattern file now fails the backup**
  instead of silently backing up everything it was meant to leave out.
- **A failing password command is now reported as itself**, not flattened
  into the same "password not remembered" message as never having set one,
  and runs under a timeout so one stuck waiting on an unseen prompt cannot
  hang a scheduled backup forever.
- **The debug log and the new one above can no longer be tricked into
  overwriting an arbitrary file.** Both sit at a fixed, predictable path
  under `/tmp`; opening either now refuses to follow a symlink already there
  and creates the file mode `0600`, rather than the previous plain
  create-and-truncate, which another user on a shared machine could have
  pointed at any file this one could write to.
- **Every icon-only button now has a name a screen reader can read**, not
  only a visual tooltip: pin, delete a snapshot, remove a folder or exclude
  pattern, back, and up a folder.

## [0.2.0] - 2026-09-24

Seeing what is going on: status, history, and repository statistics.

### Added

- **A status icon for every backup in the sidebar**: up to date, running,
  overdue, failed or damaged, with a legend under the new **Help** item
  (<kbd>F1</kbd>), which also explains repository, snapshot, rclone, prune
  and keep. A running backup's progress shows as a percentage in the
  sidebar text, since the row has no room for a bar; the profile page's own
  progress card is unchanged.
- **The next scheduled run** on the status card, read from systemd.
- **Folders included and excluded**, and space freed by every clean-up,
  on the profile page.
- **Repository statistics** on request: the real size in storage, the
  compression ratio, and how much space could still be reclaimed.
- **An animated progress bar** while a phase's total is not known yet,
  instead of sitting at an empty 0%.
- **A history** of every run, failure, check, clean-up and quiet skip, kept
  per backup and shown on its page.
- **One notification for a destination that stays unreachable**, instead of
  silence: a backup that has missed its schedule for a while now shows
  **Overdue** in the sidebar and raises a notification once per overdue
  streak, not once per skipped attempt.
- **Export and import of every backup's settings and history**, under
  **Settings**. Never a password. Importing only adds a backup whose ID is
  new; one already here keeps its own settings, with only its history
  merged in.
- **An "Overview" screen**, first in the sidebar: every backup's status,
  every folder backed up on this computer and which backups cover it, and
  every storage location and which backups keep a repository there.
- **The wizard sits beside the backup list** now, once at least one backup
  exists, instead of covering the whole window. **Cancel** offers **Finish
  Later** (the sidebar gets a **Resume setup** entry to come back to it) or
  **Discard**. Only one draft exists at a time; opening the wizard again
  from anywhere resumes it rather than losing it.
- **Use my own Google API credentials…**, under Google Drive in the wizard:
  sign in with a Google Cloud client of your own instead of the one rclone
  shares with everyone who has not set one up.

## [0.1.1] - 2026-09-24

Fixes from testing 0.1.0.

### Fixed

- **Cancel stops a backup to an SSH server or cloud storage.** rustic starts
  `rclone serve restic` for these, with the backup's output inherited. Cancel
  killed the backup process but not rclone, which kept the output open, so the
  window never saw the backup end, and rclone kept running. The backup now
  runs in its own process group and Cancel stops all of it; the window also
  stops waiting two seconds after the backup process ends, whatever it left
  behind.
- **Google Drive backups upload four packs at once.** rustic uploads one pack
  at a time and waits for each, which left a slow connection idle between
  packs and made the backup appear to stall; restic, which Déjà Dup runs,
  uses several connections. Uploads to SSH servers and cloud storage now run
  four at a time, and no index or snapshot is written until every pack it
  names has arrived, so a failed upload fails the backup exactly as before.
  Google Drive also receives each pack in one request instead of 8 MiB
  chunks.
- **Cleaning up Google Drive frees space.** rclone moved deleted data to
  Drive's trash, where it still counted against the quota for 30 days. Data
  rustic deletes is deleted permanently now, as Déjà Dup does.
- **One press of Next.** On the "where" step, Next was unavailable until the
  destination had been checked, and a destination checked while it was being
  edited could stay "checking" for good. Next now checks the destination
  itself and moves on when the check passes, and a check only counts for what
  it checked.
- **Checking a cloud location gives up after a minute** and says so, instead
  of waiting without end; rclone retries fewer times, so a real error shows
  sooner. Next tries again.
- **The wizard's fields are no longer clipped**: the focus ring on the left
  and the rows under the scrollbar on the right.
- **The suggested backup name follows the destination** as it is typed,
  instead of stopping at the first letter of a server's name.

### Added

- **Running times.** The progress card counts how long a backup has been
  running, shows how much has been stored on SSH or cloud storage, and after
  15 seconds without movement says what it is waiting for. Checking a
  destination and creating or opening a backup count their seconds on the
  button.
- **The estimate as a sum**: "45 GB included − 12 GB excluded = 33 GB",
  exact, because both figures come from the same walk rules. Each included
  folder shows everything it holds, and the patterns show what they remove
  beyond the excluded folders.
- **"Smart" explained exactly**, in the app and in the README, with tests
  that hold each rule to what rustic does.
- stldave314 on the About page.

## [0.1.0] - 2026-09-24

The first release: several independent backups, local, removable, SSH and
cloud destinations, a complete restore, and scheduled backups with retention,
checks and notifications.

### Security

- **Cloud tokens are never written into a readable file.** Stellarshot's
  rclone configuration was made private only after a sign-in had written the
  token, and rclone keeps an existing file's mode, so a configuration that had
  become readable by others (copied, restored from a backup) held the token in
  the open until then; copying a remote in never tightened it at all. The file
  is now made owner-only, in an owner-only folder, before anything is written.
- **Backups run with a per-repository lock.** rustic takes no lock of its own,
  so two backups (or a backup and a delete) could write to one repository at
  once. Every write now holds an exclusive lock, which the kernel releases if
  the process dies, so it can never be left stale.
- **The password reaches the backup process on stdin only**, never in its
  command line or environment, which other programs running as the same user
  can read.
- **Deleting a repository no longer deletes the folder it is in.** Upstream
  called `remove_dir_all` on the repository's folder. A repository created in
  the home directory, which upstream allowed, would have taken the whole home
  directory with it on Delete. Only the entries the repository format creates
  (`config`, `keys`, `data`, `index`, `snapshots`, `locks`) are removed now,
  symlinked entries are removed as links without following them, and the
  folder is removed only if nothing else is left in it.
- **A repository can no longer be created in a folder that holds other
  files.** The folder must be empty, not yet exist, or already be a
  repository.
- **A wrong password never re-initialises an existing repository.** Upstream
  initialized whenever opening failed, for any reason.

### Added

- **Scheduled backups.** Hourly, daily or weekly through a systemd user timer
  per backup, running `stellarshot --scheduled <id>` in the background at low
  priority, whether or not the window is open. Timers are persistent, so a
  run missed while the computer was off happens at the next login. The window
  keeps the timers in line with the settings every time it starts. New
  backups run daily by default.
- **Retention.** Keep a smart history (7 daily, 4 weekly, 12 monthly) or at
  least 3 months, 6 months or a year before the newest snapshot, or
  everything. Only this computer's snapshots are forgotten, so another
  computer sharing the repository keeps its own history.
- **Freeing space.** After forgetting, data no snapshot needs is pruned: on by
  default for folders and drives on this computer, off for servers and cloud
  storage, and paused while a check has found damage. rustic's delay before
  deleting unused data protects a backup running elsewhere.
- **Integrity checks every 30 days**, after a scheduled backup, plus **Check
  Now** and **Clean Up Now** on each backup's page.
- **Notifications for scheduled runs that fail.** An unreachable drive or
  server, or a repository already being written to, is skipped quietly and
  retried at the next slot; anything else is recorded, shown on the page with
  a way to retry, marked in the sidebar, and raised as a notification whose
  click opens the backup (`stellarshot --profile <id>`).
- **A "When" step in the setup wizard**, and **When it runs → Change…** to
  edit it later. Importing from Déjà Dup carries over its automatic-backup
  setting and "Keep" period.
- **Restore inside the app.** A restore page for each backup, with three
  tabs: **Browse** any snapshot as a folder tree with search and every version
  of a file (identical versions marked), **Deleted files** that a backup from
  the last 30 days still has, and **Compare** two snapshots. Selected files and
  folders go back where they were or into another folder.
- **Keep both, Overwrite or Skip** for files that already exist, with Keep
  both the default: the existing file is never touched and the restored copy
  is named `name (restored YYYY-MM-DD).ext`. The decision is made on the file
  list before rustic sees it, and rustic's option to delete files missing from
  the snapshot is never used.
- **A dry run before every restore** counts what will be restored, replaced,
  kept alongside, skipped or left alone because it is already identical. The
  restore cannot start until the dry run for the current choices has
  finished, and a test holds the dry run's counts to what the restore then
  does.
- **Open Copy** opens one version of a file read-only from a private folder
  in the session's runtime directory, without restoring it.
- **A "Restore Files" launcher action** (`stellarshot --restore`) that opens
  the selected backup's restore page as soon as it is unlocked.
- **More places to keep a backup:** removable drives recognized by their
  filesystem ID wherever they are mounted, SSH servers (through rclone, with
  host keys checked against `~/.ssh/known_hosts`), Google Drive with sign-in
  from Stellarshot, and any of the user's own rclone remotes. Stellarshot keeps
  its own rclone configuration and passes it explicitly, so the user's
  `rclone.conf` is never read or changed except to copy a remote the user
  picked.
- **Import from Déjà Dup.** Déjà Dup's settings are read from the Flatpak
  keyfile or dconf, missing keys take Déjà Dup's schema defaults, and folder
  tokens such as `$DOWNLOAD` follow the user's own folder names. Only
  restic-format backups are imported; passwords never are.
- **Backup profiles.** Each backup has its own name, folders to include and
  exclude, glob patterns to leave out, destination and password, and they sit
  side by side in the sidebar. Repositories from earlier versions become
  profiles once, on first launch.
- **A setup wizard**: what to back up, where to keep it, the password. It
  starts from the home folder with `~/.cache`, the Trash and `~/Downloads` left
  out, refuses a destination that holds other files, and points out one that
  already holds a backup. The same wizard opens an existing backup (taking its
  folders from the latest snapshot) and edits what a backup covers.
- **A live size estimate that subtracts exclusions.** It walks the same
  filtered file list the backup reads, so it equals what the backup
  processes; a test holds it to the byte. Each included folder shows its size
  and each exclusion what it removes.
- **A status-first main screen**: last backup, destination, snapshot count,
  **Back Up Now** (<kbd>Ctrl</kbd>+<kbd>B</kbd>), recent snapshots, and a
  **Create a Backup…** button on the first screen.
- **Passwords remembered in the keyring** (Secret Service), on by default,
  with every keyring request bounded so a keyring that never answers cannot
  hang the page.
- **"Remove from Stellarshot" and "Delete backup and all data"** as separate
  actions; the second needs the backup's name typed exactly.
- A **New Backup** launcher action, also available as
  `stellarshot --new-backup`.
- `scripts/screenshots.sh`, which rebuilds every screenshot from the real app
  with demo data.
- **A new backup engine** on rustic 0.13 (from 0.2), in one module that is
  the only code touching rustic. Errors are typed (wrong password, not a
  repository, destination unreachable, busy, canceled, damaged) and each has
  its own localized message.
- **Backups run in the background, with progress and Cancel.** Each write runs
  in a `stellarshot --run` child process, so the window never freezes and a
  backup can be stopped at any point. Closing the window lets a running backup
  finish.
- Snapshot rows show the date and time, the short ID, the size and how much
  new data the snapshot added.
- Integrity checking in the engine, used by the tests and by scheduled
  checks later.
- Application ID `io.github.stldave314.Stellarshot`, with a desktop entry,
  AppStream metadata (screenshots, release notes, branding) and a symbolic icon.
- Settings are copied once from the upstream application ID on first launch,
  so existing repositories stay in the sidebar. Newer settings are never
  overwritten.
- Error dialogs. Failures to create a repository, take or delete a snapshot, or
  delete a repository used to be logged and otherwise ignored.
- The About page reads its version, license and links from the package
  manifest, and credits the original authors.
- `install.sh`, the single path for building, installing and packaging
  (`.deb`, `.rpm`, tarball).
- Developer debug logging to `/tmp/stellarshot-debug.log`, compiled out of
  every build made with `--features release-build`, which every packaging
  target passes. `scripts/verify-release-build.sh` proves it at the binary
  level.
- `scripts/validate-metadata.sh`, which checks what `appstreamcli` does not:
  screenshots and captions, the newest release matching `Cargo.toml`, and the
  application ID agreeing across every file.
- `tests/i18n.rs`, which fails if any locale's keys or placeholders differ from
  English.
- CI (formatting, clippy with warnings as errors, tests, the release-strip
  check, package builds, metadata validation) and tag-triggered releases.
- README, ROADMAP, SECURITY, VALIDATION, CONTRIBUTING and CODE_OF_CONDUCT.

### Changed

- libcosmic moves to 1.0. The old file-chooser crate, which drew a
  future-incompatibility warning, is replaced by libcosmic's own portal
  dialogs.
- A backup whose folder sits inside one of its sources (`~` backed up to
  `~/Backups/home`) now leaves that folder out automatically instead of
  backing the repository up into itself.
- Excluded paths are resolved through symlinks, so an exclusion still applies
  where `/home` is a symlink (for example to `/var/home`).
- Paths under the home folder are shown as `~/…`.
- Opening a repository, listing snapshots and creating a repository run off
  the UI thread.
- A wrong password now says so, and returns the view to "no repository
  selected" instead of showing an empty snapshot list.
- Settings subscribe to the application's real configuration type, so a theme
  changed in one window applies to the others.
- `RUST_LOG` is respected rather than overwritten on every launch.
- The file-chooser titles, the password field label, the password dialog title
  and every error are localized. Missing keys were added to German, Swedish and
  Swiss German, and orphaned ones removed.

### Fixed

- **Translations were never used.** Upstream loaded only the English
  fallback and never selected the desktop's language, so every translation
  shipped and none appeared. The desktop's language is now selected at
  startup, and a test proves a requested language is actually used.
- Folders with spaces or other escaped characters in their names were stored
  percent-encoded (`My%20Backups`), which pointed at a different directory.
- Reloading the snapshot list after deleting a snapshot used the literal
  password `"password"`, so it always failed.
- Selecting a snapshot's details crashed the app (`todo!()`).
- Only the first of several background tasks requested at once was run.
- Delete did nothing unless the repository had first been unlocked.
- A failed repository creation left a placeholder entry in the sidebar.

### Removed

- The `justfile`, replaced by `install.sh`.
- Tests that wrote to `/tmp/test` and backed up `/etc`. They are replaced by
  tests that work in temporary directories.
