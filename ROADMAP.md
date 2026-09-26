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
dangerous behavior inherited from upstream is gone.

- [x] Own application ID, desktop entry, AppStream metadata and icons, with the
      original authors credited
- [x] Settings carried over from the upstream application ID, once, never over
      newer settings
- [x] **Safe repository deletion**: only the repository's own entries are
      removed, never the folder around them
- [x] **Safe repository creation**: a folder holding other files is refused; an
      existing repository is opened, never re-initialized
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
      canceled and a crash cannot take the window with it
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

## 0.1.x — Fixes from testing (done)

- [x] **Cancel does nothing during a backup.** rustic starts rclone with the
      backup's output inherited; killing the backup left rclone holding it
      open. The backup runs in its own process group and Cancel stops all of
      it
- [x] **Next needs two clicks in the new-backup wizard**, then disables with
      no feedback while it works: Next checks the destination itself and
      moves on, counting the seconds on the button
- [x] **Checking a Google Drive folder stalls with no feedback**: the check
      counts its seconds, gives up after a minute with an explanation, and
      rclone retries less
- [x] **Google Drive backups are far slower than Déjà Dup's**: rustic uploads
      one pack at a time where restic uses five connections. Four uploads now
      run at once (without changing rustic, and never writing an index
      before its packs), each pack goes to Drive in one request, and deleted
      data skips Drive's trash. Still to measure on a real account: the
      speed against Déjà Dup, and rustic_backend's direct connections
      against rclone (0.4). Déjà Dup also has a Google client ID of its own,
      where Stellarshot shares rclone's
- [x] **Progress appears to stall** (at 560 MB, then 665 MB, for a long time):
      the card counts its running time, shows what has been stored, and says
      what it is waiting for after 15 seconds without movement
- [x] **Wizard layout**: focused fields are clipped on the left and every row
      is covered by the scrollbar on the right
- [x] **The estimate as arithmetic**: "45 GB included − 12 GB excluded =
      33 GB", exact
- [x] **Explain "Smart" exactly**, in the README and in the app, with tests
      for every rule
- [x] Add stldave314 to the About page's authors

## 0.2 — See what is going on

- [x] **Status in the sidebar**: an icon for each of up to date, running,
      overdue, failed and damaged, with a legend under **Help**. The sidebar
      row is text and an icon only, with no room for an inline bar, so a
      running backup's progress is a percentage in the text instead
      (`Home — 42%`); the profile page's own card still has the real bar
- [x] **The time of the next scheduled run** on the status card, read from
      systemd over D-Bus
- [x] **A summary for each backup**: folders included and excluded, and space
      freed by every clean-up it has run. Size, last backup and snapshot
      count already had a place on the status card
- [x] **Repository statistics** from rustic (`infos_files`, `infos_index`):
      the real size in storage, the compression ratio, and how much space
      could still be reclaimed, calculated on request since both read the
      whole repository
- [x] **A home screen**, labeled **Overview** in the sidebar rather than
      "Home" so it is never confused with a backup a user has named that
      (an entirely plausible name for a home-folder backup): every backup
      with its status and a **View** button, every folder backed up on this
      computer with which backups cover it, and every storage location with
      its kind and which backups keep a repository there
- [x] **An animated bar while the total is unknown**. **A one-line live view
      of the file being read** turned out to need a change inside rustic:
      its progress trait reports counts only, never a current path. Worth
      raising with the rustic project itself, per the rule at the top of
      this page; not attempted here
- [x] **An event log** of every run, failure, check and clean-up, from the
      window or a schedule, shown on the profile page and kept for the
      settings export below
- [x] **Export and import of Stellarshot's settings**: every backup and its
      history, to a file Stellarshot itself reads back. Importing only adds
      backups whose ID is new here; an existing one is left exactly as it
      is, with only its history merged in. Never a password: a profile
      never holds one
- [x] **Problems demand attention**: every run, failure, check, clean-up and
      quiet skip is in the event log now; a destination unreachable for
      long enough shows **Overdue** in the sidebar and raises one
      notification per overdue streak, not one per skipped slot. A full
      destination is not yet told apart from any other write failure:
      rustic's own error type does not expose the underlying `ENOSPC`
      cleanly enough to detect without guessing at error text, which this
      project's own rule (prove it, don't guess) rules out for now
- [x] **The wizard beside the backup list**, not over the whole window, once
      at least one backup exists (setting up the very first one still fills
      the window, since there is no list yet to sit beside). Resumable:
      Cancel offers **Finish Later** (the draft stays, reachable again from
      the sidebar's **Resume setup** entry) or **Discard**. A draft is never
      silently replaced: opening the wizard again from anywhere, including
      Edit or Change Schedule on a different backup, resumes the one already
      in progress rather than losing it. This covers one session; a draft
      does not yet survive quitting Stellarshot and reopening it
- [x] **In-app help**: a **Help** item under **View** (and <kbd>F1</kbd>)
      opens the icon legend above and a glossary of repository, snapshot,
      rclone, prune and keep

## 0.3 — What is backed up, its history, and getting it back

- [x] **Browse folders with their sizes** to include and exclude, like a disk
      usage tree: a "Browse…" button on each included folder in the wizard's
      What step opens it, rooted there, with every row sized on demand as it
      is expanded — a folder's size is everything under it, computed the
      first time it is opened rather than the whole tree up front, so
      browsing a large home folder does not mean walking all of it first.
      Each folder shows **Included**, **Excluded**, or **Partly included**
      (something inside it is excluded), and a button to flip that. Rooted
      at an already-chosen source, not anywhere on disk: browsing only ever
      adds or removes entries in the exclude list, never adds a new source,
      so a folder already marked excluded can still be opened to see what
      is being left out, but nothing under it can be individually
      re-included — there is no way in Stellarshot's own exclude list to
      say "this whole folder, except this one thing inside it", so the tree
      does not offer a control that would quietly do nothing
- [x] **Global exclusions** (`node_modules`, `.cache`, Rust `target`, …)
      applied to every backup, set once, under **Settings**
- [x] **Exclusions that maintain themselves**: skip any folder marked as a
      cache with a `CACHEDIR.TAG` file (`exclude_if_present`), and honor each
      project's `.gitignore` (`git_ignore`, and `no_require_git` so a
      `.gitignore` works even outside an actual git repository), both as
      toggles in the wizard's Advanced section
- [x] **More exclusion rules** in the engine and the profile itself: files
      over a chosen size (`exclude_larger_than`), case-insensitive patterns
      (`iglobs`), and pattern lists kept in a file, read by Stellarshot itself
      rather than handed to rustic's own `glob_files` (whose lines are
      exclusions only with a leading `!` nothing ever adds). Not yet reachable
      from the wizard: a profile edited by hand or through settings export can
      use them today, but there is no field for them in the UI yet
- [x] **No empty snapshots**: when nothing changed, a backup records nothing
      (`skip_if_unchanged`), as a wizard toggle
- [x] **Extended attributes** saved and restored (`set_xattrs`): already
      rustic's own default behavior, confirmed with a round-trip test rather
      than assumed; no Stellarshot code was needed
- [ ] **Application settings instead of all of `~/.config`**: offer the
      settings of installed applications by name, and game saves (Proton,
      Wine, native) without their multi-gigabyte binaries, using a community
      manifest such as Ludusavi's. Not started: parsing and shipping that
      manifest, and mapping it to installed applications, is a sizeable
      feature of its own
- [ ] **A dry run before a big first backup**: rustic's backup `dry_run`
      gives the deduplicated size without writing, and `BackupRequest` now
      carries the flag through to it; not yet wired to anything in the
      window, and no transfer-time estimate from measured upload speed exists
      yet either
- [ ] **System state alongside the files**, as an option: installed packages
      (`dpkg --get-selections`), Flatpaks, dconf settings and crontab dumped
      to a file before each backup, to rebuild a machine quickly. Not started
- [ ] **Back up a command's output** (`stdin_command`), such as a database
      dump (`pg_dump`, `mysqldump`), as a file in the snapshot, without
      writing it to disk first. Not started: rustic's `stdin_command` replaces
      the whole backup source rather than adding to it, so this needs its own
      design, likely a second snapshot rather than folding into the main one
- [x] **Pin a snapshot** ("before the upgrade") so "Keep" never forgets it
      (`delete_never`), from the profile page's snapshot list. A snapshot's ID
      is a hash of its own content, so pinning saves it under a new ID and
      removes the old one, the same two-step rustic itself uses to rewrite a
      snapshot's metadata; proven with a test that pins the oldest of several
      snapshots and confirms retention keeps it anyway. Giving a snapshot its
      own expiry date (`delete_after`) instead is not yet exposed
- [ ] **Names and notes on snapshots**: tags, a label and a description,
      shown in the list, searchable, and able to protect a snapshot from
      retention (`keep_tags`). Not started; pinning above uses the same
      underlying rustic mechanism (`SnapshotModification`), so this is mostly
      UI work now
- [ ] **Remove something from history**: take an accidentally backed-up
      secret or huge file out of every snapshot (`rewrite_snapshots`), then
      prune to free the space, with a clear warning that this cannot be
      undone. Not started on purpose: this destroys data if it goes wrong,
      and deserved more time than was left in this pass rather than a rushed
      first version
- [x] **Download straight from a snapshot**: a file (rustic's own `dump`), or
      a folder as a `.tar.gz` Stellarshot builds itself by walking the
      snapshot's tree (`tar` + `flate2`, chosen over a `.zip` to keep Unix
      permissions, ownership and symlinks intact, and over rustic's own zstd
      archive for wider recognizability outside a backup tool). A "Download…"
      button on the Browse tab, and on each older version of a file; proven
      with a test that downloads a folder and confirms the extracted tree
      matches the original byte-for-byte, permission-for-permission
- [x] **Mount a snapshot as a folder through FUSE**: a whole snapshot, browsed
      and opened with any application, without restoring or downloading
      anything first. rustic_core has no mount feature of its own to build
      on (unlike the separate `rustic` command-line tool, which links
      `libfuse` directly), so this is a small read-only filesystem of
      Stellarshot's own (the `fuser` crate), reading through the same
      browser the Browse tab already does. A "Mount as Folder…" button on
      the Browse tab asks for an empty folder, mounts into it, and shows
      "Open Folder" and "Unmount" while it is live; closing the page
      unmounts automatically. Proven with a test that mounts a snapshot and
      reads a file, a symlink and a nested folder back through ordinary
      filesystem calls, and a test that a write through the mount is
      refused. One real bug was found and fixed this way: an early version
      turned on the kernel's own permission enforcement and reported every
      entry as owned by root, which locked the mounting user out of their
      own private files (the kernel checked the fake root ownership against
      the real, unprivileged mounting user, and refused). Fixed by turning
      that enforcement off and reporting every entry as owned by whoever
      mounted the snapshot instead, since FUSE already limits the mount to
      that one user and the point of a read-only mount is to look, not to
      reproduce the original owner's access rules
- [x] **A History page across every backup**, not just one profile's own
      history section: every backup, check, clean-up, restore, snapshot
      deletion, pin change, password change, and mount/unmount, merged and
      shown newest first, on its own entry in the sidebar. Each entry also
      now records where the action came from (`event_log::Source`, currently
      always the desktop or a scheduled run), shown as a "Web" badge once
      something records one — groundwork for the web interface below, so
      that feature does not need its own separate log or a later migration
      of everything already recorded
- [x] **Restore checks existing files by content**, not only by size and date
      (`verify_existing`), as a toggle in the restore sheet's Advanced
      section; proven with a test that corrupts a file without changing its
      size or date and confirms only `verify_existing` catches it
- [x] **Restore onto another machine or user** without ownership errors
      (`no_ownership`, `numeric_id`), as a choice in the restore sheet.
      Actually changing an owner needs root, which no automated test may
      assume; the option is proven wired through without breaking a restore,
      not proven to change ownership for real (see VALIDATION.md)
- [ ] **Sparse restores**, so disk images and virtual machine files do not
      fill the disk (`sparse`). Blocked upstream: `RestoreOptions.sparse` is a
      public field, but rustic_core 0.13 never re-exports the `SparseRestore`
      type it takes, so it cannot be named from outside the crate at all.
      Worth raising with the rustic project itself, per the rule at the top
      of this page; not attempted here

## 0.4 — Storage and credentials

- [ ] **S3-compatible storage** with its own choice in the wizard (it works
      today as one of your rclone remotes)
- [ ] **Direct connections without rclone** through rustic_backend's
      `opendal` feature: S3, Backblaze B2, Azure Blob Storage, Google Cloud
      Storage, WebDAV and SFTP
- [x] **REST servers** as a destination: rest-server and rustic-server
      (rustic_backend's `rest` feature), a URL field in the wizard's Where
      step, reached directly rather than through rclone. Deleting a REST
      repository's data from Stellarshot ("Delete backup and all data") is
      refused rather than attempted: doing that safely, the way Local and
      rclone destinations do (only ever removing the repository format's own
      entries), needs a generic directory listing rustic_core exposes no
      public API for against a REST server; "Remove from Stellarshot" still
      works. Every test against a real local `rustic-server` passes reliably
      by hand but fails in CI specifically, always the same way — a
      connection error on a write shortly after the server confirms itself
      ready, even once a timing race and CPU contention between the tests
      were each tried and disproven for real (not just reasoned about) by
      pushing a fix and reading CI's own logs. All three are marked
      `#[ignore]` for now (see `tests/rest_server.rs`, which also captures
      the server's own output on a future failure instead of discarding it,
      so the next attempt has more to go on); `rustic-server`'s own
      `private-repos` ACL default was also found to not actually respect
      being turned off from the command line or its environment variables,
      worked around with a repository-specific ACL section instead of the
      default one)
- [ ] **Several destinations for one backup** (cloud and a USB drive), each
      with its own "in sync" state, using rustic's repository `copy`
- [ ] **A Google API client of Stellarshot's own, bundled with the app, used
      by default: no Google Cloud console for most people.** 0.2 added
      **Use my own Google API credentials…**, for anyone who already has a
      Google Cloud project; this is the other half, for everyone else.
      rclone's own shared client (what Stellarshot signs in with today,
      unless a user overrides it) is retiring during 2026, so this moves
      from "faster" to "required" this year. Register one Google Cloud
      project under the project's own account, request the `drive.file`
      scope rather than today's full `drive` (Google classifies `drive.file`
      as non-sensitive, needing no verification review at all, versus
      `drive`'s "restricted" tier, which needs a review and an annual paid
      security assessment indefinitely — not something a solo project can
      keep up), and ship the resulting client ID compiled into the binary.
      `drive.file` only sees files the app itself created or that the user
      hands it through Google's own picker, which needs checking against a
      real account: does opening a backup made on another computer (so this
      installation's sign-in did not create those files) still work, or does
      it need a picker step to select the existing folder first? A shared
      client ID is itself a shared fate: if it is ever abused by someone
      else, Google could throttle or suspend it for every Stellarshot user
      at once, which the escape hatch (a user's own credentials) exists to
      route around. A real first backup on 2026-09-24 failed outright partway
      through, on rclone's shared client and the default four-parallel-upload
      tuning from 0.1.1: `rustic_core` gave up after retrying an upload five
      times, each attempt ending the same way (`send failed because receiver
      is gone` — the local rclone process closing the connection mid-request).
      Consistent with, though not confirmed as, exactly this shared quota; no
      diagnostic output from rclone itself survived to say for certain, since
      `set_logger` did not actually capture it (fixed separately, see
      CHANGELOG.md). If this keeps happening once that fix can capture
      rclone's own reason, cutting the default parallelism back down is the
      more conservative fallback to try before assuming it is the quota
- [ ] **One Google sign-in per account**, shared by every backup that uses
      it, so a restore needs one sign-in per account rather than per backup
- [ ] **Google sign-in lifetime**: find out when rclone's Google tokens can
      expire, show it, and ask for a renewal before a scheduled backup
      would fail. Researched, not yet built: the stored token's own
      "expiry" is the short-lived access token, which rclone already
      refreshes on its own before every use, so reading it back and
      showing it would not tell a user anything true about when they will
      actually be signed out. The refresh token behind it, the one that
      matters, carries no expiry at all in Google's response — it is valid
      until revoked, unused for 6 months, or (for an app still in Google's
      "Testing" publishing status, which a small client id can stay in
      indefinitely without submitting for review) discarded after 7 days.
      None of that is a date Stellarshot can show in advance. The buildable
      version is reactive rather than predictive: catch an auth failure
      from a real backup (now something the fixed logging in this release
      can actually see rclone's own reason for, see CHANGELOG.md) and offer
      a re-sign-in from there, rather than a countdown that cannot exist
- [x] **Passwords from a command** instead of the keyring, run only when a
      job needs one (for example the Bitwarden CLI and Vaultwarden), set from
      the profile page's own **Manage** section. The command is split into
      an argument list the way rustic's own `stdin_command` is, without a
      real shell, so pipes are not supported directly — a small wrapper
      script covers that if it is ever needed
- [x] **Bandwidth limits** for cloud and server destinations (rclone's
      `--bwlimit`), set per backup in the wizard's Where step. I/O priority
      for manual backups as well as scheduled ones is not done — `Nice=10`
      already applies to scheduled runs (see `src/schedule.rs`), a manual
      **Back Up Now** does not yet get the same treatment
- [x] **Append-only destinations**: rustic's own append-only setting
      (`set_append_only`), offered as a toggle on the wizard's When step
      when creating a backup, explained in its own description as a guard
      against mistakes rather than attacks, since other tools do not have to
      obey it. Only offered at creation, and Stellarshot exposes no way to
      turn it off again: rustic's `config` command — the only way to change
      it — refuses every other change to an append-only repository, but
      still allows turning append-only itself back off with nothing more
      than the repository's own password, after which `config` works
      normally again (including turning it back on). So this is not a
      guarantee against someone with that password, only against
      Stellarshot's own tools never doing it by themselves. Opening an
      existing append-only repository (rather than creating one) still
      recognizes it as one, read back from the repository itself rather
      than assumed. Forget and Prune are skipped for an append-only backup
      rather than attempted and failing every run, since rustic already
      refuses both against it; Clean Up Now, and pinning or deleting a
      single snapshot, are hidden in the profile page for the same reason.
      What is not built: a rest-server or rustic-server run in its *own*
      append-only mode, or storage with object lock — those guard the
      server side even against a compromised Stellarshot; this setting only
      guards against Stellarshot itself misbehaving or being told to by a
      compromised account with no other write access
- [ ] **A recovery sheet** when a backup is created: a second repository key
      (rustic's `add_key`) printed as a QR code and text, for when the
      password is forgotten. `Repo::add_key` itself exists and is tested;
      not done is generating and showing the sheet, which needs a QR code
      renderer this project does not currently depend on, and a decision
      about when in the wizard to offer it
- [x] **Change a backup's password**, from the profile page's own **Manage**
      section: adds a key for the new password, then removes the one it was
      unlocked with (`add_key`, `delete_key`); if the old password was
      remembered, the keyring entry is replaced rather than left stale, so
      a scheduled backup keeps working. **Manage several keys per backup**
      (one per person or machine) itself is not built: the engine already
      supports listing, adding and removing individual keys
      (`Repo::keys`/`add_key`/`delete_key`, all tested against a real
      repository), but there is no list UI for it yet — the profile page
      only exposes the single "change the key I am using" action above
- [x] **Verify data before it is uploaded** (`set_extra_verify`): each piece
      is checked after compression, catching memory errors on machines
      without ECC memory. Already rustic's own default for every repository;
      no Stellarshot code needed changing, confirmed with a test rather than
      assumed, the same way 0.3 found extended attributes already worked
- [x] **Where rustic keeps its cache**: another disk, or none on small
      machines (`cache_dir`, `no_cache`), under **Settings**. Machine-wide
      rather than per-backup, so the window, the scheduler and each `--run`
      child process (a separate program each time, per operation) all read
      it from a small shared value set once when settings are loaded, rather
      than it being threaded through every call that can open a repository
- [x] **Compression level**, set once when a backup is created (restic's
      format uses zstd; rustic's `set_compression` sets the level, for new
      data only, and refuses it outright on a v1 repository — checked
      against a real repository rather than assumed). A curated three-way
      choice (Default, Fast, Best) rather than the raw -7 to 22 zstd range;
      no quick benchmark on this machine to weigh speed against space, which
      was part of the original idea for this — not built

## 0.5 — Automation and alerts

- [x] **Hooks**: commands or programs before and after a backup, on their own
      page (a focused wizard step, `Wizard::hooks`, reached from the
      profile page's Manage section, the same pattern `Wizard::schedule`
      already used), with conditions for when each runs — before the
      backup, after a success, after a failure, or after either. Split and
      run the same way a password command is, without invoking a real
      shell. A `Before` hook that fails stops the backup from running at
      all; an `After` hook's failure is logged but does not undo an
      already-finished backup. Proven against the real `--run` child
      process, not only the pure hook-running logic in isolation
- [x] **Start a backup when a particular USB drive is connected**: a 4th
      frequency choice, "When its drive is connected," offered only when
      the backup's own destination is a removable drive. Reuses
      `schedule.rs`'s existing systemd unit machinery: a `.path` unit
      (`PathExists=/dev/disk/by-uuid/<uuid>`) in place of a `.timer`,
      triggering the identical `stellarshot-backup-<id>.service` a
      time-based schedule already uses, so nothing about `scheduled.rs`'s
      own run logic needed to change. No udev rule and no root needed: a
      user systemd path unit watching a `/dev/disk/by-uuid/` symlink, the
      same permission level the existing timers already run at
- [x] **Conditions for laptops**: on mains power, above a battery level, not
      on a metered or mobile connection, only on trusted Wi-Fi networks or a
      VPN interface (Tailscale, WireGuard). A new Conditions section on the
      wizard's When step, shown only while automatic backups are on; a
      scheduled run reads the real state over D-Bus (UPower for power and
      battery, NetworkManager for the connection) and skips quietly,
      exactly like an unreachable destination, when a condition is not met.
      Reading the state and deciding whether it satisfies a profile's
      conditions are kept apart, so the decision itself is proven with pure
      tests; the D-Bus reads are also proven once against the real system
      bus, not just assumed to match the documented interface. A service
      that cannot be reached (no battery, no NetworkManager) leaves its
      part of the check satisfied rather than blocking a schedule forever
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
- [x] **A COSMIC applet**, addable from COSMIC Settings' panel applet list:
      a status icon (idle, running, or needing attention) with a popup
      listing each backup's status and an Open Stellarshot button. Deliberately
      read-only — it does not start a backup or talk to the window over
      D-Bus at all, since everything it shows (each backup's run history,
      and whether something currently holds its repository's write lock)
      is already readable from disk by any process, the same way the
      window itself reads it; `crate::status` is shared by both. It also
      covers "minimize to the panel": closing the window now hides it
      rather than quitting, through libcosmic's own single-instance
      D-Bus activation — launching `stellarshot` again (from the applet,
      or a second launcher click) reopens or refocuses the same window
      rather than starting a second one

## 0.6 — Remote access

A web interface, reachable over the LAN, for looking at and controlling a
backup without sitting at the machine, plus a REST API behind it.

- [x] **An audit trail ready for it**: every history entry already records
      whether it came from the desktop (or a scheduled run) or the web
      (`event_log::Source`), so the web interface's own actions land in the
      same History page other actions do, marked as such, from the moment it
      exists — see 0.3's History page entry
- [x] **A daemon**: a new `stellarshot-web` binary (axum), binding according
      to the network scope setting and enforcing the IP allow-list before any
      route is reached — proven against a real socket, both by an
      integration test that serves the real router on an ephemeral port and
      by running the actual compiled binary and reaching it (and failing to
      reach it) with real `curl` requests. Not yet a systemd service of its
      own: today it only runs if started by hand; installing and managing it
      as a per-user unit (the way `crate::schedule` already does for backup
      timers) is not started. It has exactly one route (a health check) and
      no authentication at all yet — everything reaching it is let through
      once past the allow-list, which is acceptable only because nothing
      behind it does anything yet
- [x] **A network scope setting**: off, localhost-only, or LAN-reachable, as
      a choice in Settings (`StellarshotConfig.web.scope`) and now genuinely
      enforced by the daemon above. Defaults to off, proven with a test that
      a fresh config never comes up any other way, and confirmed for real: a
      `Localhost`-scoped daemon answers `curl` on `127.0.0.1` and refuses a
      connection on the machine's own LAN address, not merely "untested but
      presumably fine"
- [ ] **Authentication**: a shared password (kept in the OS keyring the same
      way a repository's own password is), a generated API token (kept only
      as a SHA-256 hash, shown once), and PAM, each turned on or off
      independently in Settings. The settings exist and persist; none of the
      three is actually checked by the daemon yet, so every request that
      passes the IP allow-list currently reaches the one route that exists.
      PAM specifically: verifying that a Linux user's password actually
      works through it from an unprivileged per-user service (it does, via
      `unix_chkpwd`, so long as it is only ever checking its own user) is
      design research done ahead of building it, not yet wired to a real
      check
- [x] **An IP allow-list**: addresses or CIDR ranges, added and removed in
      Settings the same way a global exclusion pattern is, and enforced by
      the daemon before any route runs — proven both by an integration test
      and by a real `curl` request rejected with a real `403` from the
      actual running binary
- [ ] **A REST API**, versioned, covering at least: listing backups and
      snapshots, starting a backup, browsing and restoring a snapshot, and
      reading the History page's own data. Not started
- [ ] **A web UI** on top of the API: browse, restore, and see the same
      History page the desktop app shows. Not started
- [ ] **TLS**, if the network scope ever grows beyond a trusted LAN. Not
      started; deliberately deferred until the scope that needs it is
      actually offered

## 1.0 — Hardening

- [x] **Every locale complete**, mechanically enforced rather than assumed:
      `tests/i18n.rs` fails if any locale is missing a key, has one the
      fallback does not, or loses a `{ $placeholder }` somewhere along the
      way. Not independently reviewed by a native speaker of each language;
      each string was translated as it was added, kept consistent with the
      vocabulary already established in that file, but that is not the same
      guarantee
- [ ] **Accessibility pass.** One real, verified gap found and fixed:
      seven icon-only buttons (pin, delete a snapshot, remove a folder or
      exclude pattern, back, up a folder) had a visual `.tooltip()` but no
      `.name()`, traced through libcosmic's and Iced's own source
      (`widget/button/icon.rs`, `widget/button/widget.rs`) to confirm a
      tooltip is never exposed to accessibility tools on its own — only
      `.name()` reaches the AccessKit node a screen reader sees. All seven
      now set both. Attempted to confirm this live against a running
      instance's AT-SPI tree; the attempt itself failed on this machine (the
      accessibility bus could not be reached with the environment a demo
      instance needs), not yet retried. Not attempted: color contrast,
      keyboard tab order through the wizard's own multi-step flow, and a
      real screen reader read-through
- [x] **Screenshots**, regenerated for this release (`scripts/screenshots.sh`):
      first launch, the wizard, an unlocked backup, the restore page, and
      the profile page in the light theme. Not literally every screen —
      individual dialogs (change password, password source, delete
      confirmations) and every wizard step are not separately captured
- [x] **End-to-end "restore actually works" tests in CI**: `cargo test
      --all-features`, already a CI step, includes `round_trip_preserves_tree`
      and its relatives (`src/engine/tests.rs`), each backing up a real tree
      with awkward names, permissions and symlinks to a real repository,
      restoring it, and diffing the result byte for byte against the
      original — not a mock, and not skipped in CI
- [x] Every dependency at its newest version (also a standing step before
      every release): `cargo update`, then `cargo outdated --root-deps-only`
      confirms every direct dependency already tracks its newest compatible
      major

---

## After 1.0

- **Search across every snapshot** at once, over time
- **A side-by-side comparison** of a file in a snapshot against the file on
  disk, before restoring over it
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
- **A central dashboard** for many machines, hosted or on-premises in a
  container, and finding the machines on the network, each with its own
  permissions
- **Other desktops and systems**: GNOME, KDE, Windows, macOS, Android and
  iOS, under the same license where possible. libcosmic targets COSMIC first
- **Tiered storage**: a snapshot lives in one repository, so "small files here,
  big files there" in one job is really two backups with complementary
  filters. rustic's hot/cold repositories (metadata kept fast, data in cheap
  storage) may give most of the saving more simply, with rustic's warm-up
  options (`warm_up_command`, `warm_up_wait`) for restoring from archive
  tiers such as Glacier that must be thawed first
- **The license**: moving away from GPL-3.0 would mean replacing everything
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
