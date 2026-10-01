# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

- **A restore checks every name in the snapshot.** A name holding a `/`
  passed as two ordinary path parts and, in a crafted repository shared by
  someone else, could have landed inside a link created by the same restore.
- **"Open a copy" no longer makes the runtime folder readable by others**,
  which also made every later backup refuse to take its lock until a reboot.
- **A scheduled backup can no longer be set to run a program from `/tmp`**,
  where another user could put their own program after a reboot.
- **Your settings and history folders are private.** The `profiles` file
  holds hook commands, a password command and SFTP details, and was
  readable by other users of the machine under the usual umask. Both
  folders are now tightened to owner-only every time the window, the applet
  or a backup starts.
- **"Only on a trusted network" no longer fails open.** When NetworkManager
  could not be reached, or did not answer within ten seconds, the backup
  ran anyway; it is now skipped with a reason. A virtual machine's tap
  device no longer counts as a VPN, and a slow UPower or NetworkManager can
  no longer leave a scheduled run hanging and every later one skipped.
- **No more guessing at `/tmp`.** With no home directory in the environment
  (`su -`, cron, a service), the lock folder and the rclone settings file fell
  back to `/tmp`, where another user could have created the folder first. The
  home directory is now found from the password database, and with none at all
  the operation fails instead. The rclone settings folder is tightened, or
  refused if it is a symlink or someone else's, before a sign-in token goes in.
- **A scheduled backup can no longer leave a core dump with the password in
  it**, as a manual one already could not, and a password command's output is
  wiped from memory on every path, including when it is not valid text.
- **A password command gets no standard input, and is judged by what it
  printed, not by what it left behind.** Launched from a terminal, one that
  prompted waited on an invisible prompt; one that started a background process
  could be reported as timed out after it had printed the password. Its output
  is capped at 64 KiB; a hook keeps only the last 16 KiB of what it reports,
  and an rclone listing at most 1 MiB, so none can fill memory.
- **Your own `RCLONE_*` environment no longer reaches the rclone commands
  Stellarshot runs against its own settings.** `RCLONE_DRY_RUN=true` made
  deleting a backup report success without deleting anything.
- **A browsed or searched path can no longer step out of the snapshot.** A name
  that is empty, `.`, `..` or contains a `/`, and a path with `..` in it, are
  refused rather than resolved to some other file.
- **A backup ID that is not letters, digits and dashes is refused when the
  settings are read**, rather than reaching the keyring, the lock names and the
  settings store.
- **The developer debug log moved out of `/tmp`** to
  `~/.local/state/stellarshot/`, and an existing log left readable by others
  is tightened to owner-only. Still compiled out of every release build.

- **Restoring can no longer write through a symlink already in the folder
  you restore into.** A symlink left there (easy to arrange with two
  snapshots in a shared repository) made the restore put the snapshot's
  files wherever it pointed, outside the folder you chose. Overwrite now
  refuses with an explanation; Skip and Keep Both leave that part alone and
  count it as a conflict.
- **A settings file from someone else is checked much more strictly.** One
  backup with a retention of 0 days would have deleted every snapshot at its
  first clean-up; that, a huge value, a repository address that is not
  `http`/`https`, an out-of-range battery level and an oversized file are
  now refused or brought into range, a file from a newer version is refused
  by its version instead of a parse error, and the same backup listed twice
  is added once.
- **An exported settings file is private from the moment it is written**
  (it can hold a hook's command line, and so a credential), not after a
  permission change that left a window in which others could read it.
- **A REST server's password is kept in the keyring**, not in the settings
  file. One already saved in a backup's address moves there the next time
  the window starts; if the keyring cannot take it, it stays where it was.
- **Restored files never keep setuid or setgid**, so restoring from a
  repository someone else wrote cannot plant a privileged program.
- **A crafted repository can no longer crash or mislead a restore.** A
  folder whose contents were never recorded is refused, comparing snapshots
  stops at a sane folder depth, and a restore's recorded choices are
  checked again against the snapshot as each item is written.
- **rclone is only ever reached on a loopback address**, and the job handed
  to a background backup is read into memory that is wiped afterward,
  without stray copies.
- **The portable tarball's installer no longer re-owns `/usr`.** Installed
  as root from an archive extracted by an ordinary user, it copied that
  user's ownership onto `/usr` and `/usr/bin`. It now installs each file
  on its own, owned by root, and `install-tarball.sh uninstall` removes
  them again.

### Added

- **Test the automatic backup.** A backup with a schedule has a **Run Now**
  that starts its scheduled run through systemd at once, exactly as the
  timer would, so a password command or SSH agent the timer cannot see
  shows up now rather than at the next slot.

### Fixed

- **Backups set to run when their drive is connected run once per
  connection.** They ran again and again for as long as the drive stayed
  plugged in, or stopped working for good after one quick run.
- **Cancel works for every operation.** Canceling a restore, check,
  clean-up or snapshot deletion was reported as an internal error; it is now
  simply canceled. A backup canceled while a Before hook is running stops that
  hook and still runs its After hooks, and a failing Before hook no longer
  leaves an earlier one's work undone.
- **Deleting or pinning a snapshot, or restoring, with the drive unplugged no
  longer leaves the page stuck as busy** until a restart.
- **Overwrite refuses to replace a folder with a file or a file with a
  folder**, with an explanation, instead of crashing partway through the
  restore. Keep both and Skip handle it as before.
- **A destination that is not reachable right now is skipped quietly by a
  scheduled backup**: an unmounted network share or drive mount point, and an
  SFTP server, cloud account or REST server that cannot be reached. It used to
  be recorded as a failure and notified at every slot. A refused login still
  is.
- **Signing in to Google with your own client ID keeps working after the
  first hour**: the client is now saved with the remote. Sign-in can be
  canceled from its dialog and gives up after ten minutes.
- **Dialogs and the keyboard.** Shortcuts no longer act behind an open dialog
  (Ctrl+B could start a backup behind "Remove this backup?"); Escape closes a
  dialog; a busy dialog can no longer get stuck behind Quit; "Delete
  everything" cannot be dismissed while it runs; pressing Ctrl+Q twice no
  longer stacks two questions.
- **Importing settings shows the new backups in the sidebar at once.**
- **Opening the restore page again while a restore runs keeps that restore**
  instead of replacing its page.
- **Discarding the setup wizard returns to the home screen** instead of a
  blank page.
- **The uninstall instructions work**: the old command was refused by
  `systemctl` and missed on-connect backups. The README now lists them.
- **Opening an SFTP or cloud backup no longer leaves a dead `rclone`
  process behind each time.** Every snapshot list, statistics view or restore
  preview of such a backup started an rclone that was stopped but never
  collected, so a window left open for days piled up defunct processes
  against the per-user process limit. Stellarshot now starts and stops that
  rclone itself, gives up if it does not start within a minute, and keeps its
  output in the backend log.
- **An unlocked encrypted USB drive is found again.** Drives from an
  unlocked LUKS volume were never recognized, so a scheduled backup to one
  was skipped as "destination unavailable". One mount with a non-UTF-8 name
  anywhere on the system also made every drive look unplugged; it no longer
  does.
- **Déjà Dup folders with an apostrophe import correctly.** GLib prints such
  a name in double quotes, which the importer did not read: the folder fell
  back to the machine's name and an excluded folder was backed up after all.
  All of GLib's escapes are decoded now too.
- **Results on the restore page can no longer land under the wrong
  snapshot.** Searching, comparing, finding deleted files and searching
  every snapshot each tag their answer with what it was asked, and an answer
  for something you have since moved on from is dropped. They also no longer
  share one "busy" flag, so one running no longer grays out another's button.
- **A finished background task no longer replaces a question you are
  answering.** A backup error arriving while "Remove this backup?" or
  "Delete everything?" was on screen replaced it; it now waits its turn. A
  finished sign-in closes only its own "go to your browser" message.
- **Restoring a whole home folder no longer holds every file's record in
  memory.** It kept the complete list of files, twice over, which for millions
  of files is gigabytes at exactly the moment of a disaster recovery. It now
  reads the snapshot as it goes and remembers one byte per file.
- **Restoring two same-named files into one folder is refused** with an
  explanation, instead of the second silently overwriting the first.
- **A file's history and the deleted-files list no longer hide a damaged
  repository.** Any read error was taken for "not in this snapshot" and
  the list came back shorter.
- **The "Replace N files…" button** replaces **Restore…** when overwriting
  files in place would destroy some.
- **A multi-part restore reports the total**, not the last part's counts.
- **"Open a copy" cleans up after itself and refuses huge files.** The copies
  live in memory-backed storage and were never removed; ones over a day old are
  now, and a file over 512 MiB is offered a normal restore instead.
- **The applet no longer piles up refreshes behind a stalled network mount**,
  says so when it cannot start the window, and names the status next to its
  warning icon for screen readers.
- **A size estimate that fails says so** in the new-backup wizard, instead of
  leaving "Counting…" or a stale figure.
- **Closing the folder browser in the new-backup wizard stops the size scan.**
- **A drive folder containing `..` is refused**, since it could point a backup
  (and "Delete everything") at the internal disk.
- **Moving settings from the old app ID is all or nothing**, so an interruption
  cannot leave a half-copy that blocks the real one.
- **Buttons named for what they remove** ("Remove Downloads", not "Remove"),
  password dialogs focus their first field and submit with Enter, and a handful
  of strings that could not be translated (engine details in the wizard, list
  separators, arrows and multiplication signs) now go through the locale files.
- **Deja Dup detection at startup no longer runs on the window's thread.**
- **Screenshots in the store listing point at the release's own tag**, so a
  later change to an image cannot alter what an old version shows.
- **Importing settings can no longer push out your recent history.** An
  export with 200 older events on a backup that already had a full log
  dropped every recent entry. Imported events now merge in by date, the
  oldest are trimmed afterward, imported entries are marked as coming from
  elsewhere, and the merge happens off the window's thread.
- **Looking for deleted files no longer freezes browsing.** The search for
  files that are gone from disk kept the backup open for its whole run, so one
  stalled network mount in the folder being checked blocked every other
  browse, search, and the mounted view until it returned. The disk check now
  runs without holding it.
- **An unreadable list of backups can no longer wipe your schedules or be
  saved over.** If the saved list cannot be read (a downgrade, or a file a
  newer version wrote), the window used to treat it as "no backups", remove
  every scheduled backup's timer, and overwrite the file with the empty list
  on the next save. It now says so, keeps a copy of the file, changes no
  timer, and refuses to save until it is resolved. A scheduled run says the
  list could not be read instead of claiming the backup was removed.
- **Cancel, stopping a scheduled run, and logging out now run a backup's
  "after" hooks.** A service stopped by a "before" hook stayed stopped
  every time a backup was canceled. A backup that crashes or is killed by
  the system is now reported as a failure with its reason, not as canceled.
- **A backup's hook that leaves a background process behind can no longer
  hang the backup forever** (while it held the repository lock).
- **A retention rule of 0, or an absurdly large one, is refused** instead
  of forgetting every snapshot (including the newest) or crashing.
- **Downloading a folder as an archive no longer corrupts it when a file
  was changing during the backup.** One shrunken file misaligned every
  entry after it.
- **A failed save of your settings is now shown**, and the window stops
  displaying the change as if it had been kept. A failed save no longer
  goes on to start a backup, or install a schedule, for something that
  was not saved.
- **Cancel and Back are disabled while the wizard is creating or opening a
  repository**, and a result arriving for a wizard that was discarded is
  ignored instead of acting on the next one.
- **Changing a repository's password now always switches the running
  session over to it**, even if the dialog was closed or replaced meanwhile.
  Cancel is disabled while the change runs.
- **"Remember password" now saves it only after it worked**, instead of
  saving a mistyped one (and, for a failed Create, leaving an orphaned
  keyring item).
- **Finishing an edit applies it to the backup as it is now**, not to the
  copy the dialog opened with, so it no longer rolls back a run that
  finished meanwhile or brings back a backup that was removed (which is
  now reported).
- **Quit asks before cutting off a restore, a repository being created, or
  a running password change or deletion**, not only a running backup.
- **Removing a backup says so when its saved password could not be
  deleted from the keyring.** The restore page can be closed if its backup
  disappeared from under it.
- **A mounted snapshot survives a bug in one file operation** instead of
  becoming "Transport endpoint is not connected".
- **A run state written by a newer version no longer gets stuck**: an
  error kind this version does not know is read as unknown, and an
  unreadable state is reported as a failed update rather than a silent
  success.
- **Scheduled backups have a start timeout, a stop policy and a private
  umask**, and a removed package makes them skip quietly instead of failing
  at every slot.

- **Restoring hard-linked files over ones still on disk no longer aborts
  at the end** with "file exists", and Keep both no longer picks a name
  the same restore also writes.
- **Failures show up when they should.** A clean-up or check failing in the
  same second as the backup succeeded was hidden; a time in the future (a
  wrong clock) no longer hides failures or postpones checks; a check that
  cannot run waits a day instead of trying at every slot.
- **An unplugged drive no longer pushes real history out of the log**:
  repeated "skipped" entries collapse into one.
- **A keyring that is not ready yet (a timer firing at login) skips the
  backup quietly** instead of reporting that no password was remembered.
- **The window and a scheduled run no longer lose each other's changes** to
  the run state and history: both are updated under a lock every
  Stellarshot process shares.
- **A failed password change no longer leaves the new key behind**, and a
  backup stops uploading as soon as one upload fails.
- **A scheduled run that shows a failure notification ends right away**
  instead of staying active for up to 15 minutes waiting for a click,
  which made the next scheduled slot skip. A small helper waits for the
  click instead.
- **"Only on a trusted network" can be met on a wired connection**, named
  as in your network settings; it only ever looked at Wi-Fi, so a desktop
  on a cable never backed up with it on. The Bulgarian description of the
  setting also no longer says the opposite of what it does.
- **The wizard says when it cannot read the list of mounted drives**,
  instead of claiming no drive is plugged in.
- **Previewing a restore of several items reads the backup's index once**,
  not once per item, which on cloud storage was a download each.
- **A snapshot left mounted by a crash is unmounted** the next time the
  window starts, instead of its folder answering "Transport endpoint is not
  connected" until you ran `fusermount -u` yourself.
- **A scheduled run holds its backup's lock from the backup through the
  clean-up and check**, and opens the repository once for all of them: a
  backup started from the window in between used to make the clean-up fail
  as locked.
- **Snapshots taken within the same second are listed in the order they
  were taken**, where file history and the deleted-files list could pick
  the wrong one as newest.
- **An rclone left running by a crash is stopped** the next time the window
  starts.
- **A backup whose status cannot be read is no longer stuck as "damaged".**
  It says what happened instead, and **Reset Status** starts the status
  afresh; before, even a passing check could not clear it.
- **Removing a backup deletes its history and status too**, instead of
  leaving them in the state folder for good.
- **The window no longer reads and writes the status files on its own
  thread** every 30 seconds and after every action, which could stutter
  while a scheduled run held them.
- **Settings that cannot be saved say so** (theme, exclusions, cache):
  before, only profile changes did.
- **Canceling the setup wizard for your first backup** no longer offers to
  finish it later, which led to an empty window.
- **The restore page no longer shows "Searching…" for good** after you
  clear the search or move to another folder while one runs, and an
  unmount whose backup was removed meanwhile is still done, off the
  window's thread.
- **An SFTP backup with no home directory in the environment** fails with a
  clear error instead of looking for `known_hosts` in whatever folder it was
  started from.

### Changed

- The History page labels an entry recorded by another program "Other
  program". Entries written by earlier versions with a source this version
  does not know still load.
- The restore page's Browse, Deleted, Compare and Search are real tabs, and
  each backup on the home screen has a name screen readers announce.
- Page widths and the folder-size list follow one set of sizes, the History
  page is as wide as the others, and the "Browse…" hint is wide enough for
  its translations.
- The launcher entry, its New Backup and Restore Files actions, and the
  store listing's summary are translated, and a test keeps them in step
  with the locale files.
- Menu ellipses and quotation marks are consistent across locales; Swiss
  German uses the product name "Stellarshot", and Swedish says "Radera" for
  a permanent delete.
- The code the background processes share with the window (settings,
  formatting, error text, logging) moved out of the window's module, most
  modules are private to the crate, and the systemd unit module is now
  `timers`, to tell it apart from `scheduled`.
- Debug and test builds optimize the key derivation, encryption and
  compression crates, which took the library tests from about 16 minutes to
  under one.
- The REST server tests run against `rclone serve restic` and run in CI
  again; a coverage report is produced on every CI run.
- The release is built with a fixed Rust toolchain, `./install.sh package`
  builds once rather than once per format, and re-running an older
  release's build no longer fails on tags made after it.
- Packages are installed, started and removed in clean containers in CI,
  the release is checked against the version in `Cargo.toml` and re-runs
  the lint and advisory checks, and every shipped binary is verified to
  carry its dependency data.

### Removed

- The web interface and its REST API (added in 0.6.0) are no longer part of Stellarshot: the
  `stellarshot-web` program, its Settings page and its documentation now
  live in a project of their own. Stellarshot no longer installs it; a
  systemd user unit an earlier version created for it can be removed with
  `systemctl --user disable --now stellarshot-web.service` and deleting
  `~/.config/systemd/user/stellarshot-web.service`.

## [0.8.2] - 2026-09-29

### Security

- **A release build no longer touches `/tmp/stellarshot-debug.log` at
  all.** The fix that made only the window truncate that file (rather
  than every process racing to) called its setup unconditionally at
  every entry point, which on its own defeated the whole point of
  `release-build`: the file still got created, and the window still
  truncated it, in a build that is supposed to carry no trace of
  developer logging. No log content was ever written — writing itself
  was already correctly gated — but the file was. Caught by the
  project's own standing "prove it, don't assume it" check, not by
  reading the code.

## [0.8.1] - 2026-09-29

### Changed

- Two tests that only ever failed while the whole test suite was running
  under load are fixed. No change to the application itself.

## [0.8.0] - 2026-09-29

### Security

- **A symlink placed at a backup's progress file can no longer be
  followed.** The lock and log files already refused a pre-existing
  symlink at their own paths; writing progress data now does too,
  instead of silently writing through it to wherever it points.
- **A hostile snapshot name can no longer be used to probe whether a
  file exists elsewhere on the machine.** Restoring already refused a
  snapshot name trying to escape its own folder with `..` or an
  absolute path; the same check now covers every other place a
  snapshot's own untrusted names are read — browsing, mounting, and
  looking for missing files.

### Added

- **An "Estimate Size" button next to "Back Up Now"** shows how much a
  backup would cover right now — a file count and total size — without
  opening it or asking for its password. It only walks the source folders
  on disk, the same as the setup wizard's own live estimate, so it does not
  account for what is already stored or how long an actual backup would
  take.

### Fixed

- **A count of exactly one no longer reads like "1 files" or "1 changed"
  in six places** across every language ("N files" in the size estimate,
  "N found so far" while browsing, "N changed" in Compare, a password's
  character count, and forgetting N snapshots) — each now uses its
  language's own singular form for a count of one.
- **Restoring into a folder where a snapshot expects a directory but a
  plain file already exists there now reports it as a real conflict and
  honors Skip**, instead of silently reporting no conflicts and then
  failing partway through the restore.
- **Uninstalling now names the exact leftover systemd unit files to
  remove by hand** (a scheduled backup's timer) instead of just saying
  they exist.
- **The app's metainfo now lists the applet binary it installs, and
  declares keyboard and pointer support**, for software
  centers that read it.
- **A password keystroke or a running backup's progress no longer rebuilds
  the entire sidebar.** Every backup's own status still updates live, but
  the sidebar's other rows, its selection, and whatever had keyboard focus
  in it are no longer disturbed by an unrelated backup's own updates.
- **The panel applet now shows a failure, an overdue backup and real
  damage as three distinct icons**, instead of the same generic warning
  sign for all three — which damage could never actually reach in the
  first place, so it was never shown at all. "Remove backup" is now
  styled as the destructive action it is; "Unmount" (never destructive)
  no longer is.
- **The panel applet no longer polls every profile's status every 3
  seconds forever, whether its popup is open or not.** It now does that
  only while the popup is actually open, slows to once a minute
  otherwise, runs the check itself off the UI thread, and refreshes
  immediately when a backup is added, removed or changed elsewhere
  instead of waiting for the next tick.
- **Accessibility: every icon button, checkbox and the two fields that
  most needed it now work properly with a screen reader or the keyboard
  alone.** The wizard's folder-tree expand/collapse button, its
  include/exclude checkbox in every state (previously only when partly
  included), the hooks list's enabled toggle, and the restore page's
  three selection checkboxes all now announce something meaningful
  instead of nothing. The "Delete everything" confirmation field and the
  unlock field both focus themselves automatically and confirm on Enter,
  instead of needing a mouse click or several Tab presses first.
- **Comparing two snapshots with a very large difference, browsing a
  folder with many entries, or excluding folders from deep inside a very
  large one no longer redoes the same work on every tick while idle, or
  tries to render every single entry at once.** A comparison's own counts
  and folder grouping are now computed once when it finishes rather than
  recomputed continuously; folder listings, the deleted-files list, and
  the wizard's own folder-size browser now show up to 500 entries at a
  time, the same limit search results already used.
- **"Stop setting up this backup?" now has a "Keep Editing" option**,
  alongside "Finish Later" and "Discard" — closing the dialog without
  choosing either used to be the only way back to the wizard.

## [0.7.0] - 2026-09-27

A full security and reliability audit: hardening across secrets handling
and file safety.

### Security

- **Importing a settings file from someone else can no longer run
  commands.** An imported backup now always starts with its schedule set to
  Manual and every hook turned off (kept, not discarded, so they can be
  reviewed first), with a dialog explaining why. A remote destination in the
  import is only accepted if it already exists in Stellarshot's own rclone
  configuration, closing off a crafted remote string that could otherwise
  make rclone run a command of its own. Exported settings files are now
  written readable only by their owner.
- **Restoring can no longer write outside the folder you chose.** A
  snapshot from a repository shared with someone else could in principle
  name a file in a way that would land somewhere else entirely; such an
  item is now refused and the restore stops, rather than silently writing
  there.
- **Signing in to Google Drive no longer puts your OAuth client secret
  somewhere any other program running as you could read it** (`ps` and
  `/proc/<pid>/cmdline`, for as long as sign-in takes). It now reaches
  rclone through the environment instead, which only your own user can
  read.
- **The lock file and log a backup uses can no longer be forced onto a
  location another user could tamper with.** When the usual per-session
  location isn't available, Stellarshot now falls back to a private folder
  of its own under your home directory instead of a shared temporary
  directory, and refuses to use a folder that already exists with the
  wrong owner or permissions.
- **A scheduled backup now refuses to run from a location another user
  could replace**, such as a portable download left in a temporary folder,
  rather than quietly trusting whatever program happens to be at that path
  after a reboot.
- **The repository password is now wiped from memory as soon as it is no
  longer needed**, rather than just left for the allocator to reuse later,
  and the backup/restore/check process that holds it disables core dumps
  for itself so a crash cannot write the password to disk.

### Added

- **Every released `.deb`, `.rpm` and tarball now carries a signed
  attestation** that GitHub Actions actually built it from this
  repository's own source, checkable with `gh attestation verify` before
  installing it.
- **A settings icon in the header bar**, right-aligned next to the window
  controls, matching where COSMIC Store and COSMIC Files put theirs. It
  opens the same Settings page as View → Settings.
- **A hint points at the Browse button** the first time you set up a new
  backup, next to the folder it starts with. Exclusions and the size
  estimate live behind it, and nothing else on that page says so. It
  disappears the first time you use Browse, or if you dismiss it directly,
  and only ever shows once, for a brand new backup's first folder — not for
  every folder, and not while editing an existing backup.

### Fixed

- **Two profiles sharing one repository no longer prune each other's
  snapshots.** Every new snapshot now carries a tag identifying which backup
  made it, and cleanup only ever removes snapshots carrying the current
  backup's tag (or, for a snapshot made before this change, one whose
  recorded source folders still match).
- **A hook meant to run after a backup ("start the database back up") now
  always runs**, even when the backup itself couldn't start — a wrong
  password or an unreachable destination no longer leaves a stopped service
  stopped.
- **A backup of a folder with hundreds of unreadable files or subfolders no
  longer hangs forever.** Its diagnostic output is now read continuously in
  the background instead of only after it finishes, so it can no longer fill
  up and block the backup mid-run.
- **A pack upload that panics instead of merely failing no longer hangs the
  backup forever.** The panic is now caught and treated as the same kind of
  failure a returned error already was.
- **"Keep both" can no longer overwrite an existing file it meant to leave
  alone.** It used to compare modification times only to the second; a file
  whose time matched the backup's to the second but not the fraction of a
  second looked unchanged and was restored over anyway. That comparison now
  matches full precision, the same as the underlying restore engine's own.
- **The setup wizard's "excluded" size no longer undercounts.** Its
  "nothing excluded" baseline was still applying the maximum-size limit,
  cache-folder skipping, `.gitignore`, and pattern-file exclusions, only
  ignoring the plain exclude list and glob patterns — so a backup using
  any of those reported less excluded than it actually excluded.
- **An excluded path containing a wildcard character (`*`, `?`, `[`) in its
  actual name is now excluded correctly**, instead of being interpreted as a
  pattern that could match unrelated files — or, in one case, the backup's
  own repository, causing it to back up into itself.
- **Files and folders with names that aren't valid UTF-8 text can now be
  browsed, mounted, and restored individually**, instead of being skipped.
- **Checking a backup's status no longer occasionally reports a running
  backup as "Locked" and fails it.**
- **A repository's lock now recognizes the same local folder consistently**,
  even when it's reached through a symlink or a relative path, so two
  profiles pointing at the same folder in different ways correctly take
  turns instead of racing.
- **The applet's Open button now always opens the actual window**, instead
  of occasionally launching a second, useless instance of the applet itself.
- **There is now a way to quit Stellarshot entirely** (Ctrl+Q, and a Quit
  entry in the window menu), rather than only minimizing to the panel
  applet or closing the last window.
- **Deleting a snapshot now asks for confirmation first**, matching every
  other destructive action in the app.
- **A rare Fluent syntax error no longer silently drops the "this word is
  only shown once" warning** from every language's translation file.
- **A window reopened from the panel applet no longer shows two title
  bars.** Closing the window (minimizing it to the panel) and reopening it
  from there used a plain window configuration that asked the compositor to
  draw its own title bar on top of the app's own, instead of the
  client-side-only decoration the window starts with. Only the reopen path
  was affected; a fresh launch was never doubled.
- **A backup or restore started right after a package upgrade replaced
  Stellarshot's own binary now just works**, rather than failing with a
  bare "os error 2" or needing the app quit and reopened first: every
  operation spawns through the exact executable image this process is
  still running, which keeps working even after the file it was launched
  from has been replaced. If launching it somehow still fails regardless,
  that is now reported as "Stellarshot was updated while it was running"
  rather than the raw operating-system error.
- **Browsing or restoring by a snapshot ID prefix that matches more than one
  snapshot now says so**, instead of being reported as "not in this
  snapshot" — which was true of neither snapshot it matched.
- **Pinning or unpinning a snapshot no longer risks reporting the wrong
  snapshot as the result** in the rare case its new ID couldn't be
  confirmed; it now fails with an error instead, even though the pin itself
  already took effect.
- **A missed scheduled backup's overdue notification is no longer marked as
  seen if it never actually showed.**
- **Starting Stellarshot with a non-UTF-8 command-line argument no longer
  crashes it on launch.**
- **If "Remember password" cannot actually save to your keyring, you are now
  told**, instead of finding out only when a later scheduled backup fails
  with no password to use.
- **A scheduled backup's cleanup or integrity check no longer reports a
  spurious failure notification** when it happens to run into the
  repository's lock — the same retry-next-time treatment a locked backup
  itself already got.
- **Opening a large file from a mounted snapshot no longer reads the
  whole thing into memory first.** A multi-gigabyte file now opens and
  reads instantly, the same as a normal file would.
- **Downloading a folder as a `.tar.gz` no longer buffers each file fully
  in memory before writing it**, and no longer leaves a truncated archive
  behind if something goes wrong partway — both it and downloading a
  single file are now written to a temporary location first and only put
  in place once complete. A folder entry with no recorded permissions now
  extracts as an ordinary, enterable folder instead of one nothing can be
  opened inside.
- **The desktop entry's "New Backup" and "Restore Files" actions, and a
  failed backup's notification, now do what they say even when Stellarshot
  is already running** (closing the window keeps it running in the panel,
  so this is the common case) — they used to just raise the window with no
  idea what was actually asked for.

## [0.6.0] - 2026-09-26

Getting more out of a backup: mounting a snapshot as a folder, a History
page across every backup, searching by filename across every snapshot, and
folder-grouped comparisons.

### Added

- **Compression level**, chosen when a backup is created: Default, Fast or
  Best.
- **Mount a snapshot as a folder**, through FUSE: browse and open it with any
  application, without restoring or downloading anything first. A "Mount as
  Folder…" button on the Browse tab; "Open Folder" and "Unmount" while it is
  live.
- **A History page**, in the sidebar, showing every backup's activity in one
  place: backups, checks, clean-ups, restores, snapshot deletions, pin
  changes, password changes, and mounts, newest first.
- **Search across every snapshot** by filename, in a new "Search everywhere"
  tab: shows every snapshot a match was found in, and jumps straight to
  Browse at the one you pick.
- **The Compare tab groups changes by folder**, collapsed behind a count and
  expanded on request, instead of one flat list of every changed path.

### Fixed

- **A snapshot's size no longer looks like a bug on a brand-new backup.**
  "5.7 GB new" on a backup's very first run, next to a larger total size,
  read like new data was missing; it is deduplication (even within one
  backup, identical files or repeated byte patterns are only stored once),
  not a mistake. Now reads "5.7 GB new, deduplicated."
- **The Browse tree's include/exclude control now reads as a selector, not a
  status label.** A folder row's "Included"/"Excluded"/"Partly included"
  button, whose text was the current state rather than an action, is now a
  checkbox: checked to go in, unchecked to be left out.
- **A folder's shown size in the Browse tree no longer hides what is
  excluded beneath it.** Marking a subfolder excluded now reduces every
  ancestor's own displayed size ("80 MB of 150 MB") rather than leaving it
  showing the unreduced total with no sign anything changed.
- **Signing in to Google Drive now says "switch to your browser" as a
  dialog**, not a line of page text below a button that had just
  disappeared. Closes on its own once sign-in finishes, successfully or
  not.
- **The Browse tree's checkbox no longer sits under the scrollbar**: the
  scrollable list now reserves space on the right for it, rather than
  letting the scrollbar overlay draw on top of a row's own controls.
- **A shield in place of a plain hard drive** for a backup that is up to
  date, in the sidebar and the status legend: "up to date" is a claim
  about safety, not about where the data happens to sit.

## [0.5.0] - 2026-09-25

Automation: conditions for when a scheduled backup is allowed to run, hooks
before and after one, starting one when its drive connects, and a panel
applet to keep an eye on it all without opening the window. Plus the rest
of getting files back without a full restore: browsing a folder by size,
and downloading a file or folder straight from a snapshot.

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
- **A wrong password never re-initializes an existing repository.** Upstream
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
