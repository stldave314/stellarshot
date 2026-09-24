# Roadmap

What is done, what is next, and what is deliberately out of scope.

**The goal** is a backup application the open-source world can choose over
commercial ones: set up once, trusted, and understandable, from a single
laptop up to a centrally watched fleet.

**Stellarshot complements rustic; it never changes it.** rustic is a separate
project with its own goals. Everything here is built on the public
`rustic_core` and `rustic_backend` APIs and the restic repository format, so
every backup stays readable by the `rustic` and `restic` tools. The aim is to
make rustic's full feature set easy to reach. Anything that would need a
change inside rustic is proposed there, not patched here.

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
      (schedule and retention settings added in M5)
- [x] Status-first main screen: last backup, how often it runs, **Back Up
      Now**, **Restore…**
- [x] A **Create a Backup…** button on the empty main screen
- [x] Setup wizard: what, where, password ("when" added in M5)
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

## M4 — Restore (done)

- [x] Browse any snapshot as a file tree, with search
- [x] Restore selected files and folders to their original place or elsewhere
- [x] Conflict handling: overwrite, keep both, or skip, with a dry-run summary
      before anything is written
- [x] Every version of a file, with identical versions marked
- [x] Deleted files: what is in your backups but no longer on disk
- [x] Compare any two snapshots
- [x] Open one version of a file read-only without restoring it
- [x] A "Restore Files" launcher action

## M5 — Automation (done)

- [x] Scheduled backups through systemd user timers, catching up on missed
      runs
- [x] Retention: smart (7 daily, 4 weekly, 12 monthly), at least 3 months,
      6 months or a year, or forever; only this computer's snapshots are
      forgotten
- [x] Freeing space automatically where it is safe (folders and drives on
      this computer), paused while a check has found damage
- [x] Desktop notifications for scheduled runs that fail; clicking opens the
      backup
- [x] Periodic integrity checks (every 30 days), and **Check Now** and
      **Clean Up Now** on the page
- [x] Déjà Dup's schedule and "Keep" setting come across on import

## 0.1.x — Fixes from testing

- [ ] **Cancel does nothing during a backup.** The child-process cancel is
      tested, so look at what a real backup (and the rclone process under
      it) does with the signal
- [ ] **Next needs two clicks in the new-backup wizard**, then disables with
      no feedback while it works: one click, and a visible "checking…" state
- [ ] **Checking a Google Drive folder stalls with no feedback**: show
      progress, and time it out with an explanation
- [ ] **Google Drive backups are far slower than Déjà Dup's**, which also runs
      restic through rclone: compare the rclone flags and rustic's pack sizes
      (`set_datapack_size`, which rustic already exposes) and tune them, and
      measure rustic_backend's direct connections against rclone
- [ ] **Progress appears to stall** (at 560 MB, then 665 MB, for a long time):
      keep the card moving and say what is happening while bytes are not
- [ ] **Wizard layout**: focused fields are clipped on the left and every row
      is covered by the scrollbar on the right
- [ ] **The estimate as arithmetic**: "45 GB − 12 GB excluded = 33 GB", plus
      the summary section from the design mockups
- [ ] **Explain "Smart" exactly**, in the README and in the app, with no
      ambiguity: the newest snapshot of each calendar day, week and month
      that has one; days without a backup do not count; this computer's
      snapshots only, grouped by folders; everything else is forgotten
- [ ] Add stldave314 to the About page's authors

## 0.2 — See what is going on

- [ ] **Status in the sidebar**: an icon per state (up to date, running,
      overdue, failed, damaged) with a label or legend, and a progress bar
      while a backup runs
- [ ] **The time of the next scheduled run** on the status card
- [ ] **A summary for each backup**: folders included and excluded, size, last
      backup, number of snapshots, space freed by clean-ups
- [ ] **Repository statistics** from rustic (`infos_files`, `infos_index`):
      the real size in storage, the compression ratio, and how much space
      is unused and could be reclaimed
- [ ] **A home screen** with the state of every backup, every folder backed up
      on this computer, and every storage location with its kind
- [ ] **A one-line live view** of the file being read, and an animated bar
      while the total is unknown
- [ ] **An event log** of every run, failure, check and clean-up, kept with
      the settings (and so included in their export)
- [ ] **Export and import of Stellarshot's settings**: every backup and
      setting, never a password
- [ ] **Problems demand attention**: a full or vanished destination is logged,
      marked on the backup and raised as an urgent notification (Wayland does
      not let an app take focus by itself, by design)
- [ ] **The wizard beside the backup list**, not over the whole window, and
      resumable: Cancel offers "finish later" or "discard"
- [ ] **In-app help** for terms a newcomer may not know (repository,
      snapshot, rclone, prune) and at the places a mistake is easy

## 0.3 — What is backed up, its history, and getting it back

- [ ] **Browse folders with their sizes** to include and exclude, like a disk
      usage tree: sizes on every row, and clear marks for "this folder",
      "everything inside" and "partly"
- [ ] **Global exclusions** (`node_modules`, `.cache`, Rust `target`, …)
      applied to every backup, set once
- [ ] **Exclusions that maintain themselves**: skip any folder marked as a
      cache with a `CACHEDIR.TAG` file (`exclude_if_present`), and honour each
      project's `.gitignore` (`git_ignore`)
- [ ] **More exclusion rules**: files over a chosen size
      (`exclude_larger_than`), case-insensitive patterns (`iglobs`), and
      pattern lists kept in a file (`glob_files`)
- [ ] **No empty snapshots**: when nothing changed, an hourly backup records
      nothing (`skip_if_unchanged`)
- [ ] **Extended attributes** saved and restored (`set_xattrs`), for SELinux
      labels and applications that keep data there
- [ ] **Application settings instead of all of `~/.config`**: offer the
      settings of installed applications by name, and game saves (Proton,
      Wine, native) without their multi-gigabyte binaries, using a community
      manifest such as Ludusavi's
- [ ] **A dry run before a big first backup**: rustic's backup `dry_run`
      gives the deduplicated size without writing; add a transfer-time
      estimate from the measured upload speed
- [ ] **System state alongside the files**, as an option: installed packages
      (`dpkg --get-selections`), Flatpaks, dconf settings and crontab dumped
      to a file before each backup, to rebuild a machine quickly
- [ ] **Back up a command's output** (`stdin_command`), such as a database
      dump (`pg_dump`, `mysqldump`), as a file in the snapshot, without
      writing it to disk first
- [ ] **Pin a snapshot** ("before the upgrade") so "Keep" never forgets it
      (`delete_never`), or give one its own expiry date (`delete_after`)
- [ ] **Names and notes on snapshots**: tags, a label and a description,
      shown in the list, searchable, and able to protect a snapshot from
      retention (`keep_tags`)
- [ ] **Remove something from history**: take an accidentally backed-up
      secret or huge file out of every snapshot (`rewrite_snapshots`), then
      prune to free the space, with a clear warning that this cannot be undone
- [ ] **Download straight from a snapshot**: a file, or a folder as a zip or
      tar archive (`dump`), without restoring it
- [ ] **Restore checks existing files by content**, not only by size and date
      (`verify_existing`)
- [ ] **Restore onto another machine or user** without ownership errors
      (`no_ownership`, `numeric_id`)
- [ ] **Sparse restores**, so disk images and virtual machine files do not
      fill the disk (`sparse`)

## 0.4 — Storage and credentials

- [ ] **S3-compatible storage** with its own choice in the wizard (it works
      today as one of your rclone remotes)
- [ ] **Direct connections without rclone** through rustic_backend's
      `opendal` feature: S3, Backblaze B2, Azure Blob Storage, Google Cloud
      Storage, WebDAV and SFTP
- [ ] **REST servers** as a destination: rest-server and rustic-server
      (rustic_backend's `rest` feature)
- [ ] **Several destinations for one backup** (cloud and a USB drive), each
      with its own "in sync" state, using rustic's repository `copy`
- [ ] **One Google sign-in per account**, shared by every backup that uses
      it, so a restore needs one sign-in per account rather than per backup
- [ ] **Google sign-in lifetime**: find out when rclone's Google tokens can
      expire, show it, and ask for a renewal before a scheduled backup
      would fail
- [ ] **Passwords from a command** instead of the keyring, run only when a
      job needs one (for example the Bitwarden CLI and Vaultwarden)
- [ ] **Bandwidth limits** for cloud and server destinations (rclone's
      `--bwlimit`), and I/O priority for manual backups as well as
      scheduled ones
- [ ] **Append-only destinations**: a rest-server or rustic-server in
      append-only mode, or storage with object lock, so a compromised
      account cannot delete old snapshots. rustic's own append-only setting
      (`set_append_only`) is offered as well, explained as a guard against
      mistakes rather than attacks, since other tools do not have to obey it
- [ ] **A recovery sheet** when a backup is created: a second repository key
      (rustic's `add_key`) printed as a QR code and text, for when the
      password is forgotten
- [ ] **Change a backup's password**, and manage several keys per backup
      (one per person or machine) (`add_key`, `delete_key`)
- [ ] **Verify data before it is uploaded** (`set_extra_verify`): each piece
      is checked after compression, catching memory errors on machines
      without ECC memory
- [ ] **Where rustic keeps its cache**: another disk, or none on small
      machines (`cache_dir`, `no_cache`)

## 0.5 — Automation and alerts

- [ ] **Hooks**: commands or programs before and after a backup, on their own
      page, with conditions for when each runs
- [ ] **Start a backup when a particular USB drive is connected**
- [ ] **Conditions for laptops**: on mains power, above a battery level, not
      on a metered or mobile connection, only on trusted Wi-Fi networks or a
      VPN interface (Tailscale, WireGuard)
- [ ] **Alerts beyond the desktop**: a webhook field sending a JSON payload
      (with presets for Discord, Slack, Teams, Gotify and PagerDuty), and
      email through the system's mail transfer agent, for failures, stalls
      and full destinations
- [ ] **Rolling data verification**: each scheduled check also reads a
      different slice of the stored data (rustic's `read_data_subset`), so
      over time every pack is downloaded and its hashes verified
- [ ] **Healing damaged data**: when a check finds a bad pack, show which
      files depend on it and have the next backup store them again, with
      rustic's repair commands underneath
- [ ] **Bit-rot warnings**: an occasional full read (rustic's backup `force`)
      and a comparison with the previous snapshot; a file with the same date
      and size but different content points at a failing disk
- [ ] **A COSMIC applet**, optional and addable from the app or COSMIC
      Settings: running backups, last backup, errors, live through a D-Bus
      bridge to the app. It also covers "minimise to the panel"

## 1.0 — Hardening

- [ ] Every locale complete and reviewed
- [ ] Accessibility pass
- [ ] Screenshots of every screen
- [ ] End-to-end "restore actually works" tests in CI
- [ ] Every dependency at its newest version (also a standing step before
      every release)

---

## After 1.0

- **Mount a snapshot** as a folder through FUSE (rustic_core's `vfs`)
- **Search across every snapshot** at once, over time
- **A side-by-side comparison** of a file in a snapshot against the file on
  disk, before restoring over it
- **Compression level** per backup (restic's format uses zstd; rustic's
  `set_compression` sets the level, for new data only), with a quick
  benchmark on this machine to weigh speed against space
- **A low-memory profile** for machines that back up millions of files, once
  it is clear what rustic lets a caller limit
- **A privileged helper for system folders** (`/etc`, `/var/lib/docker`):
  a small root service reached over D-Bus and authorised by Polkit, so the
  window never runs as root. Container volumes (pausing a container, backing
  up its volume, resuming it) build on it, recorded under a stable path
  (`as_path`) even when read from a temporary snapshot of the volume
- **Backing up as files change**: folder watching (inotify or fanotify), with
  continuous protection through eBPF considered once the privileged helper
  exists, since eBPF needs root
- **A ransomware guard**: sample changed files for a sudden jump in entropy
  and stop the backup before it stores encrypted garbage
- **A disaster-recovery USB image** with Stellarshot and the backups' settings
  on it, booting straight into restore
- "Revert to previous version" from the COSMIC file manager
- Flatpak (it needs full filesystem access and an rclone binary, which works
  against the sandbox, so it waits)
- Converting Déjà Dup duplicity backups

## To investigate

Questions to answer before anything is built.

- **Import from more than Déjà Dup.** Rename the button to **Import** and list
  what it can read. restic-based tools (autorestic, resticprofile, Backrest)
  could come across with their full history. Borg- and rsync-based tools
  (Pika Backup, Vorta, Back In Time, Timeshift) could only bring their folders,
  exclusions and schedule; their history stays readable in the tool that made
  it
- **rustic-server and rustic-scheduler**: what an append-only REST
  destination, and central scheduling of several machines, would give
- **A web interface**, alongside the desktop one: it needs its own server,
  authentication, restrictions by address, interface and network, and an
  audit trail
- **A central dashboard** for many machines, hosted or on-premises in a
  container, and finding the machines on the network, each with its own
  permissions
- **Other desktops and systems**: GNOME, KDE, Windows, macOS, Android and
  iOS, under the same licence where possible. libcosmic targets COSMIC first
- **Tiered storage**: a snapshot lives in one repository, so "small files here,
  big files there" in one job is really two backups with complementary
  filters. rustic's hot/cold repositories (metadata kept fast, data in cheap
  storage) may give most of the saving more simply, with rustic's warm-up
  options (`warm_up_command`, `warm_up_wait`) for restoring from archive
  tiers such as Glacier that must be thawed first
- **The licence**: moving away from GPL-3.0 would mean replacing everything
  the original authors and translators contributed, or their consent. The
  rustic crates are MIT or Apache-2.0 and libcosmic is MPL-2.0, so the
  dependencies do not force the choice

## Decided against

- **A security-key tap before pruning or deleting.** A compromised account
  does not need Stellarshot to delete snapshots: it can run `rustic` or
  `restic` with the password from the keyring. Append-only destinations
  (0.4) protect old snapshots for real.
- **A separate pruning page.** Freeing space belongs with each backup's
  summary (0.2) and **Clean Up Now**, together with a preview of which
  snapshots the "Keep" setting will forget next.
- **Printing the master key.** The password unlocks a key file that holds the
  master key; a printed password covers forgetting it, and a second key
  (the recovery sheet, 0.4) covers losing it, without exporting the master
  key itself.

## Out of scope

- **Changing rustic.** See the top of this page.
- **Reading duplicity or borg repositories.** rustic reads the restic format
  only.
- **Importing passwords** from Déjà Dup or anywhere else.
- **Storing passwords in Stellarshot's own settings.** They belong in the
  keyring, a password command you choose (0.4), or nowhere.
