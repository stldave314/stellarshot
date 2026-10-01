# Release review: tasks before 0.9

**Date:** 2026-09-30 · A final code review and security audit of the working
tree after the production-readiness work (see
`2026-09-29-production-readiness-tasks.md`). The verdict: **not ready to
release.** Section 1 lists what blocks it.

## How to work these tasks

- **Write the failing test first.** Every task has a **Verify** line. Make that
  test fail on the current code, then fix, then watch it pass. If you cannot
  make it fail, stop and say so in the task: the finding may be wrong.
- **Prove a fix with a negative control** for anything security-related:
  remove the fix, confirm the test goes red, put the fix back.
- **Status** says how sure we are:
  - **Confirmed (test):** reproduced on a real system.
  - **Confirmed (code):** read and traced, not run.
  - **Verify first:** plausible, not yet traced end to end. Reproduce it before
    changing anything; if it does not reproduce, close it with a note.
- Before calling a task done: `cargo fmt --check`, `cargo clippy --all-targets`
  (zero warnings), and the tests the task touches. Any user-visible string goes
  through `fl!` and into **all five** locales (`tests/i18n.rs` checks this).
  The German locale addresses the user as "du", not "Sie".
- Update `README.md`, `SECURITY.md` and `CHANGELOG.md` `[Unreleased]` when
  behavior a user can see changes.
- Resource limits on this machine: run cargo through
  `systemd-run --user --scope -q -p MemoryMax=6G -p CPUQuota=300% nice -n 10 cargo … -j 4`,
  one heavy job at a time. The rclone integration tests can stall behind the
  local firewall (OpenSnitch) on a freshly built test binary; that is not a
  code bug.

Severity: **High** (data loss, security, or a stuck UI/backup) · **Medium** ·
**Low**. Effort: **S** (under half a day) · **M** (1–2 days).

## Summary

| ID | Sev | Title |
|---|---|---|
| [B-1](#b-1-on-connect-backups-run-back-to-back-then-the-path-unit-fails) | High | On-connect backups run back to back, then the path unit fails |
| [B-2](#b-2-restore-does-not-check-each-node-name-from-the-snapshot) | High | Restore does not check each node name from the snapshot |
| [B-3](#b-3-open-a-copy-can-break-every-backup-until-reboot) | High | "Open a copy" can break every backup until reboot |
| [B-4](#b-4-delete-or-pin-with-the-drive-gone-leaves-the-page-busy-forever) | High | Delete or pin with the drive gone leaves the page busy forever |
| [B-5](#b-5-a-restore-that-cannot-start-leaves-the-restore-page-stuck) | High | A restore that cannot start leaves the restore page stuck |
| [B-6](#b-6-cargo-deny-will-stop-the-release-workflow) | High | `cargo deny` will stop the release workflow |
| [B-7](#b-7-the-release-job-does-not-install-fuse3-or-cpio) | High | The release job does not install `fuse3` or `cpio` |
| [S-1](#s-1-canceling-anything-but-a-backup-is-reported-as-a-failure) | Medium | Canceling anything but a backup is reported as a failure |
| [S-2](#s-2-a-failed-or-canceled-before-hook-phase-never-undoes-earlier-hooks) | Medium | A failed or canceled Before-hook phase never undoes earlier hooks |
| [S-3](#s-3-a-binary-run-from-tmp-is-accepted-for-the-timer) | Medium | A binary run from `/tmp` is accepted for the timer |
| [S-4](#s-4-custom-google-credentials-may-not-survive-sign-in) | Medium | Custom Google credentials may not survive sign-in |
| [E-1](#e-1-overwrite-with-a-filefolder-type-conflict-crashes-the-restore) | Medium | Overwrite with a file/folder type conflict crashes the restore |
| [E-2](#e-2-an-unmounted-local-destination-fails-loudly-as-not-a-repository) | Medium | An unmounted local destination fails loudly as "not a repository" |
| [E-3](#e-3-an-unreachable-sftp-cloud-or-rest-destination-fails-loudly) | Medium | An unreachable SFTP, cloud or REST destination fails loudly |
| [U-1](#u-1-an-operation-can-only-close-its-dialog-when-that-dialog-is-in-front) | Medium | An operation can only close its dialog when that dialog is in front |
| [U-2](#u-2-delete-everything-can-be-canceled-while-it-runs) | Medium | "Delete everything" can be canceled while it runs |
| [U-3](#u-3-keyboard-shortcuts-act-behind-an-open-dialog) | Medium | Keyboard shortcuts act behind an open dialog |
| [U-4](#u-4---restore-replaces-a-restore-page-that-is-already-open) | Medium | `--restore` replaces a restore page that is already open |
| [U-5](#u-5-importing-settings-does-not-refresh-the-sidebar) | Medium | Importing settings does not refresh the sidebar |
| [U-6](#u-6-discarding-the-wizard-leaves-a-blank-page) | Medium | Discarding the wizard leaves a blank page |
| [U-7](#u-7-a-wizard-finish-that-returns-early-leaves-the-sidebar-stale) | Medium | A wizard finish that returns early leaves the sidebar stale |
| [P-1](#p-1-the-uninstall-instructions-do-not-work) | Medium | The uninstall instructions do not work |
| [D-1](#d-1-documentation-corrections) | Medium | Documentation corrections |
| [L-1](#l-1-low-priority-items) | Low | Low-priority items (do after the above) |

---

## 1. Release blockers

### B-1. On-connect backups run back to back, then the path unit fails

**High · M · Confirmed (test)**

**Files:** `src/schedule.rs:148-163` (`path_text`), `src/schedule.rs:90-122`
(`service_text`), `src/scheduled.rs`

**Problem.** "Back up when the drive is connected" writes a `.path` unit with
`PathExists=/dev/disk/by-uuid/<uuid>` that starts a `Type=oneshot` service.
systemd checks the path again as soon as the service exits, and starts it again
while the path still exists. A transient unit with the same shape, pointed at a
file that stays in place, ran 5 times in under a second and the path unit then
went to `failed (Result: unit-start-limit-hit)`, never to fire again until
reset. With a real drive, each run is a full backup (minutes long), so the
start limit is never hit: the backups simply run one after another for as long
as the drive is plugged in. The README (`:190`, `:360`) promises one backup when
the drive is connected. `VALIDATION.md:391` notes this was never tested with a
real drive.

**Reproduce.**

```sh
touch /tmp/flag
systemd-run --user --unit=pathloop --path-property=PathExists=/tmp/flag \
  --property=Type=oneshot sh -c 'date >> /tmp/pathloop.log'
sleep 5; wc -l /tmp/pathloop.log; systemctl --user status pathloop.path
systemctl --user stop pathloop.path; systemctl --user reset-failed pathloop.path pathloop.service
rm -f /tmp/flag /tmp/pathloop.log
```

**Fix.** Make "the drive appeared" an edge, not a level. Two options; pick one
and write the reason in `schedule.rs`:

1. Keep the service active while the drive is present: `RemainAfterExit=yes` on
   the service, plus `BindsTo=`/`After=` on the drive's `.device` unit
   (`dev-disk-by\x2duuid-<escaped uuid>.device`) so it stops when the drive goes
   and the path unit can trigger again next time. Prefer a device-unit
   `WantedBy=` over a path unit entirely if that proves simpler.
2. Keep the path unit, and have `--scheduled` skip quietly when this
   profile already succeeded since the drive was mounted (compare
   `last_success` with the mount time of the drive). Still leaves a retrigger
   per exit, so add a minimum interval (a new constant).

**Verify.** The reproduce block above, with the generated unit text, runs the
service once and the path unit stays `active (waiting)`. Unplug and replug a
real USB drive: one backup each time. Add a unit-text test in `schedule.rs` for
whatever directives the fix relies on.

---

### B-2. Restore does not check each node name from the snapshot

**High · S · Confirmed (code) for the missing check; the full attack is not yet reproduced**

**Files:** `src/engine/restore.rs:573-584` (pass-1 loop), `src/engine/restore.rs:497-516`
(`reject_unsafe_name`), `src/engine/browse.rs` (`archive_folder`, `search`)

**Problem.** A repository can be shared with someone else, so its tree data is
untrusted. Restore checks only the walked relative path
(`reject_unsafe_relative_path`), never each node's own name. A node named
`a/evil.desktop` passes: as a path it is two normal components. Browse and
mount already reject such names with `reject_unsafe_name`; restore does not.

This chain was traced by reading rustic_core 0.13 (`restore.rs`,
`local_destination.rs`). It has **not** been reproduced:

- A crafted tree holds, in this order, a symlink node `a` pointing to
  `~/.config/autostart`, a file `z`, and a file node named `a/evil.desktop`
  that is a hard link to `z`.
- rustic writes contents first and creates links and symlinks last.
- The symlink `a` is therefore created before `a/evil.desktop` is hard-linked,
  and that write follows the symlink out of the restore folder.
- `writes_through_a_symlink` cannot see it, because the symlink does not exist
  yet when restore decides what to write.

**Fix.**

1. In the pass-1 loop, `reject_unsafe_name(&item.name())?` for every item, and
   require `relative.file_name() == Some(&*item.name())` (the walked path must
   end in the node's own name).
2. Keep a set of the relative paths of every non-directory item seen so far
   (symlinks, files, special files). Refuse any later item whose path has one of
   them as a strict prefix: nothing can live "inside" a symlink or a file.
3. Apply the same name check in `Browser::archive_folder` and `Browser::search`.

**Verify.** Build the crafted tree in an engine test. Use rustic's own types to
write tree blobs directly (see how `src/engine/tests.rs` builds fixtures; a
helper that saves a `Tree` with chosen nodes is needed). Assert the restore
returns `ErrorKind::UnsafePath` and nothing exists outside the destination.
Then do the negative control. Also add unit tests for a node name `a/b`, `""`
and `.` reaching the pass-1 loop.

---

### B-3. "Open a copy" can break every backup until reboot

**High · S · Confirmed (code)**

**Files:** `src/app/effects/restore.rs:326-348` (`open_copy`),
`src/engine/lock.rs:129-131,253-279` (`create_private_dir`)

**Problem.** `open_copy` calls `create_dir_all(runtime_dir()/open-<uuid>)`. If
`$XDG_RUNTIME_DIR/stellarshot` does not exist yet (the first thing done in a
login session), `create_dir_all` creates it with the umask's mode, normally
0755. Every later lock is taken through `create_private_dir`, which refuses a
directory readable by group or other. From then on every backup, scheduled run,
check and clean-up fails with "readable or writable by group or other" until
the folder is removed or the machine reboots.

**Reproduce.** Log in, do not back up, open a snapshot, choose **Open a copy**.
Then `stat -c %a "$XDG_RUNTIME_DIR/stellarshot"` prints `755`, and
**Back Up Now** fails.

**Fix.** At the start of `open_copy`, `lock::create_private_dir(&lock::runtime_dir())?`.
Create the `open-…` folder with `DirBuilder::new().mode(0o700).create(..)` rather
than creating it and changing the mode afterwards.

**Verify.** A test that calls the folder-creation part of `open_copy` against a
fresh runtime directory (inject the directory, as `remove_stale_open_copies_in`
does) and then `lock::acquire_in` on the same directory succeeds.

---

### B-4. Delete or pin with the drive gone leaves the page busy forever

**High · S · Confirmed (code)**

**Files:** `src/app/effects/profile.rs:92-99` (`DeleteSnapshots`),
`src/app/effects/profile.rs:122-129` (`SetPinned`)

**Problem.** The page marks itself busy (`start_modify`) before the effect
runs. If `profile.location()` then fails (the drive was unplugged), the effect
shows an error and `continue`s, so no child starts and nothing ever clears the
busy state. The page shows a progress card forever. Back up, Check, Clean up,
Remove and Delete are disabled. Quit asks for confirmation every time. Only a
restart clears it. `BackUp`, `Check` and `CleanUp` already handle this through
`end_before_start`.

**Fix.** Replace both `show_error` + `continue` blocks with
`tasks.push(self.end_before_start(&id, &profile, profile::Message::SnapshotsDeleted, err)); continue;`
and the same with `profile::Message::Pinned`.

**Verify.** A `ProfileState` test: start a delete (`delete_snapshot_confirmed`),
then feed `Message::SnapshotsDeleted(ChildEvent::Ended(err))`; `is_busy()` is
false afterward. (The effect-runner side has no test harness; check by hand
with a USB drive.)

---

### B-5. A restore that cannot start leaves the restore page stuck

**High · S · Confirmed (code)**

**Files:** `src/app/effects/restore.rs:133-140` (`Effect::Restore`)

**Problem.** Same shape as B-4. `StartRestore` sets `running` before the effect
runs. If `profile.location()` fails, the effect shows an error and `continue`s.
`running` is never cleared: Back is disabled, Cancel does nothing (no child
handle), Quit always asks. It also happens between parts of a multi-part
restore if the drive goes away.

**Fix.** On that error, feed `restore::Message::Restore(ChildEvent::Ended(err))`
into the page (`page.update(..)`) and run the effects it returns, instead of
`show_error`.

**Verify.** A `RestorePage` test: `StartRestore`, then
`Message::Restore(ChildEvent::Ended(err))`; `is_restoring()` is false and the
effects contain a `ShowError`.

---

### B-6. `cargo deny` will stop the release workflow

**High · S · Verify first (cargo-deny is not installed locally)**

**Files:** `deny.toml`, `.github/workflows/release.yml:172-184`, `Cargo.lock`

**Problem.** `cargo audit` finds no vulnerabilities, but reports:

| Advisory | Crate | Kind | Comes in through |
|---|---|---|---|
| RUSTSEC-2026-0253 | `lru` 0.16.4 | unsound | `cryoglyph` ← libcosmic's `iced_wgpu` |
| RUSTSEC-2026-0186 | `memmap2` 0.8.0 | unsound | `xkbcommon` ← libcosmic's `iced_winit` |
| (yanked) | `yoke-derive` 0.8.3 | yanked | `icu_*` ← `idna` |

`deny.toml` ignores only three unmaintained crates. cargo-deny treats an
unsound advisory as an error by default, and the release `publish` job needs
the `deny` job, so a tag would build everything and then not publish.

**Fix.**

1. `cargo update -p yoke-derive` (0.8.4 is available).
2. `cargo update` as a whole (a newer libcosmic revision, `6af8b705`, is
   available). Check whether it moves `lru` and `memmap2` past the advisories:
   `cargo tree -i lru`, `cargo tree -i memmap2`.
3. Whatever is still flagged and only reachable through libcosmic: add each
   advisory ID to `deny.toml`'s `ignore` list with a reason in the same style as
   the existing entries (crate, why it is transitive, why it is not reachable
   in a way we use, if you can say so).
4. Run `cargo check` **and** `cargo clippy --all-targets` after the update: a
   git-dependency bump can break the build.

**Verify.** `cargo install cargo-deny --locked` (or run the CI job on a branch),
then `cargo deny check` passes.

---

### B-7. The release job does not install `fuse3` or `cpio`

**High · S · Confirmed (code)**

**Files:** `.github/workflows/release.yml:47-50`, `.github/workflows/ci.yml:37-40`

**Problem.** The release workflow runs `cargo test --all-features --locked`.
`ci.yml` installs `fuse3` and `cpio` for that; `release.yml` does not.
`a_mounted_snapshot_can_be_read_with_plain_filesystem_calls`
(`src/engine/tests.rs`) needs `fusermount3`, so the release can fail its own
test step after the tag is pushed.

**Fix.** Make `release.yml`'s `apt-get install` line match `ci.yml`'s (add
`cpio fuse3`).

**Verify.** Diff the two package lists; run the release workflow on a test tag
in a fork, or with `workflow_dispatch` against the current tag.

---

## 2. Security

### S-1. Canceling anything but a backup is reported as a failure

**Medium · S · Confirmed (code)**

**Files:** `src/runner.rs:376-383`, `src/proc_signal.rs:155-194`,
`src/app/child.rs:257-287`

**Problem.** Only a backup arms the SIGTERM handler (`proc_signal::arm`). For a
restore, check, clean-up or snapshot delete, nothing is armed. Cancel sends
SIGTERM, `on_term` finds nothing armed and calls `_exit(143)`. The window sees
a normal exit with code 143, not a signal, so it reports "internal error: exit
status 143" with a stderr tail. The same happens to a backup canceled before
`arm()` (while taking the lock or during Before hooks). `tests/child.rs` only
covers canceling a backup.

**Fix.**

1. Arm for every operation (empty hooks for non-backups), so a cancel always
   reports `Canceled`.
2. In `drive()`, treat exit code 143 while `canceled` is set as canceled
   (`Ok(false)`), like a signal.

**Verify.** A `tests/child.rs` test that starts a restore (or a check) of a
large fixture, cancels it, and expects a canceled outcome, not an error.

---

### S-2. A failed or canceled Before-hook phase never undoes earlier hooks

**Medium · M · Confirmed (code)**

**Files:** `src/runner.rs:369-383`, `src/hooks.rs:37-51`, `src/proc_signal.rs`

**Problem.** `run_before(..)?` returns before the After-hook guard and the
SIGTERM handler exist.

- If Before hook 1 ("stop the database") succeeds and Before hook 2 fails, no
  After hook runs, so the database stays stopped. README `:275-276` says After
  hooks still run.
- If Cancel, `systemctl --user stop` or logout lands during a Before hook,
  nothing is armed, so the process exits at once. The running hook is in its own
  process group, so it is orphaned and keeps running.

**Fix.** Create the guard and arm `proc_signal` before the first Before hook.
If a Before hook fails or a SIGTERM arrives, run the After (failure) hooks when
at least one Before hook already succeeded. On SIGTERM, kill the process group
of a hook that is still running. Decide and document what "After" means when
the backup never started (README hooks section).

**Verify.** `tests/runner.rs`: two Before hooks, the second `false`, and an After
hook that writes a marker file. The marker exists after the run. A second test
sends SIGTERM while a Before hook `sleep 30`s; the After marker exists and no
`sleep` from that hook is left running (`pgrep`).

---

### S-3. A binary run from `/tmp` is accepted for the timer

**Medium · S · Confirmed (code)**

**Files:** `src/schedule.rs:196-225` (`trusted_executable`), doc comment at `:165-173`

**Problem.** The check is meant to refuse writing a timer that runs a binary
from a world-writable place, because another user could replace it. Its
`safe_from_others` treats a root-owned sticky directory as safe, so
`/tmp/stellarshot/stellarshot` passes: `/tmp` is root-owned and sticky, and
`/tmp/stellarshot` is the user's and 0755. After a reboot wipes `/tmp`, another
local user can create that path first, and the user's timer runs their binary
as the user.

**Fix.** Refuse any path under `std::env::temp_dir()`, `/tmp`, `/var/tmp` and
`/dev/shm` outright, and drop the sticky-directory exemption for ancestors.
Keep the doc comment and the code in agreement.

**Verify.** Unit test: a binary copied to a temporary directory under `/tmp`
is not `trusted_executable`; one under the test's own private directory (not
in `/tmp`) is.

---

### S-4. Custom Google credentials may not survive sign-in

**Medium · S · Verify first**

**Files:** `src/engine/rclone.rs:423-446` (`sign_in`), `src/engine/rclone.rs:78-87` (`command`)

**Problem.** A custom OAuth client ID and secret reach `rclone config create`
only as `RCLONE_DRIVE_CLIENT_ID`/`_SECRET` environment variables. If rclone
does not write environment-sourced values into `rclone.conf`, later commands
(which deliberately strip inherited `RCLONE_*` variables) refresh the token with
rclone's bundled client ID. The refresh would then fail, and Drive backups
would stop working about an hour after sign-in.

**Reproduce.** Sign in with your own client ID and secret, then
`grep client_ ~/.config/stellarshot/rclone.conf`. If they are missing, wait for
the access token to expire (or edit its expiry) and run a backup.

**Fix (only if it reproduces).** Pass `client_id=…` as a normal parameter (it is
not secret) and keep only the secret in the environment for the sign-in; check
that `rclone.conf` then holds both and the token refreshes.

**Verify.** The grep above shows both values; a backup more than an hour after
sign-in succeeds.

---

## 3. Engine

### E-1. Overwrite with a file/folder type conflict crashes the restore

**Medium · S · Confirmed (code), including the upstream `unwrap`**

**Files:** `src/engine/restore.rs:425-451` (`shape`), rustic_core
`commands/restore.rs:658,663`

**Problem.** When a folder in the snapshot meets a file of the same name on
disk (or the other way round), Overwrite passes the item through
(`Decision::Keep`). The comment above it says these cases are treated as Skip.
For a file with content, rustic's `restore_contents` then calls `set_length`
and `write_at` with `.unwrap()` inside its worker threads. Those calls panic,
the `--run` child dies partway, and the metadata pass never runs. Restoring a
folder onto an existing *file* at the top level makes rustic write every child
into that one file. The existing test
`a_file_vs_directory_conflict_is_reported_not_silently_ignored` only checks the
conflict count.

**Fix.** For Overwrite, a type-blocked item returns a clean `EngineError`
naming the path (or is skipped and counted). Do the same for a file item whose
path on disk is a directory, and for a folder restore whose destination is an
existing file.

**Verify.** Engine tests for both directions with a file that has content:
the restore returns an error (or skips), the existing file and folder are
unchanged, and nothing panics.

---

### E-2. An unmounted local destination fails loudly as "not a repository"

**Medium · S · Confirmed (code)**

**Files:** `src/engine/repo.rs:262-279` (`check_reachable`),
`src/engine/repo.rs:574-580` (`open`), `src/scheduled.rs:67-72` (`is_quiet`)

**Problem.** A local destination such as `/mnt/nas/backup` with the share not
mounted: the path or its parent still exists, so `check_reachable` passes and
`open` returns `NotARepository`. That is not a quiet error, so every scheduled
slot records a failure and sends a notification. The quiet-skip test only uses
a path whose parent is also missing.

**Fix.** In `open` (not `init` or `probe`): a destination path that does not
exist, or an existing empty folder with no `config`, is
`DestinationUnavailable`.

**Verify.** Engine test: an existing empty folder and a missing folder under an
existing parent both give `DestinationUnavailable` from `open`. A
`tests/scheduled.rs` case records `Skipped`, not `Failed`.

---

### E-3. An unreachable SFTP, cloud or REST destination fails loudly

**Medium · M · Confirmed (code) for the error kind; check rclone's behavior with the reproduce step**

**Files:** `src/engine/repo.rs:223-261`, `src/engine/serve.rs:106-132`,
`src/scheduled.rs:11-14` (doc)

**Problem.** `scheduled.rs` says "no network" is skipped quietly. Only local
and removable destinations ever produce `DestinationUnavailable`. With the
server down or no network, opening an rclone or REST repository fails as
`Internal`, which records a failure and notifies on every slot.

**Reproduce.** `rclone serve restic --addr localhost:0 :sftp,host=unreachable.invalid:/x`:
does it exit, or hang before printing "Serving restic REST API"? That decides
whether the failure comes from `Serve::start` or from the first request.

**Fix.** In a scheduled run, before opening, call `rclone::probe` for an rclone
destination: it already maps "cannot reach" to `DestinationUnavailable`. For
REST, map a connect or timeout error from reqwest to `DestinationUnavailable`.
Fix the comment at `constants.rs` (`RCLONE_SERVE_START_TIMEOUT`) if the
reproduce step shows rclone does connect at start-up.

**Verify.** A scheduled run against `:sftp,host=unreachable.invalid` records
`Skipped` and sends no notification.

---

## 4. User interface

### U-1. An operation can only close its dialog when that dialog is in front

**Medium · S · Confirmed (code)**

**Files:** `src/app/dialog.rs:156-160` (`Dialogs::close_if`), callers in
`src/app/effects/dialog.rs` and `src/app.rs` (sign-in)

**Problem.** Start a password change (its dialog goes busy), press Ctrl+Q:
Quit goes in front and the busy dialog waits behind it. When the change
finishes, `close_if` looks only at the front (Quit), so the busy dialog is
never closed. Cancel on Quit then shows the busy dialog with Save and Cancel
both disabled, and nothing can close it. The same applies to "Delete
everything" and the sign-in dialog.

**Fix.** Add `Dialogs::close_where(pred)` that removes the first matching dialog
wherever it is (front or queue); use it at every `close_if` site, then remove
`close_if`.

**Verify.** Unit test in `dialog.rs`: open a busy `ChangePassword`, open
`Quit`, `close_where(ChangePassword)`, close Quit; the queue is empty.

---

### U-2. "Delete everything" can be canceled while it runs

**Medium · S · Confirmed (code)**

**Files:** `src/app/dialog.rs:224-234`

**Problem.** The Cancel button is always enabled for `DeleteAll`; for
`ChangePassword` it is disabled while busy. Canceling a running delete hides
the dialog while the delete keeps going on a background thread. Quit then sees
no busy dialog and exits without asking, leaving a half-deleted repository.

**Fix.** Build the DeleteAll Cancel with `.on_press_maybe((!busy).then_some(..))`,
as ChangePassword does.

**Verify.** By hand: type the name, Delete, and Cancel is disabled until it
finishes.

---

### U-3. Keyboard shortcuts act behind an open dialog

**Medium · S · Confirmed (code)**

**Files:** `src/app.rs:1081-1087` (`Message::Key`)

**Problem.** The modal only captures mouse input; key presses still reach the
shortcuts. With "Remove this backup?" open, Ctrl+B starts a backup behind it.
Ctrl+N opens the wizard, Ctrl+W closes the window, and Ctrl+Q stacks Quit
dialogs. There is no Escape to dismiss a dialog, so a keyboard-only user is
stuck.

**Fix.** In `Message::Key`, while `self.dialogs.front().is_some()`, ignore every
shortcut except Escape, which sends `DialogMessage::Close` unless the front
dialog is busy (`DeleteAll { busy: true }`, `ChangePassword { busy: true }`).
Make the Quit guard (`app.rs`, `Message::Quit`) look at every queued dialog, not
just the front one, and do not open a second Quit.

**Verify.** By hand with each shortcut while a confirmation is open. Unit test a
small helper that decides whether a key is allowed while a dialog is open.

---

### U-4. `--restore` replaces a restore page that is already open

**Medium · S · Confirmed (code)**

**Files:** `src/app/effects/profile.rs:152-162` (`OpenRestore`), `src/app.rs`
(`Launch::Restore`)

**Problem.** With a restore running, launching the desktop entry's "Restore
Files" action opens a fresh restore page in place of the running one. The
running restore loses its page: its result is never shown or logged, Quit stops
asking, and a mounted snapshot is dropped on the UI thread.

**Fix.** Ignore `OpenRestore` (and the activation) while `self.restore.is_some()`;
just show the existing page.

**Verify.** By hand: start a long restore, run
`stellarshot --restore`; the running restore stays on screen.

---

### U-5. Importing settings does not refresh the sidebar

**Medium · S · Confirmed (code)**

**Files:** `src/app/effects/settings.rs` (`import_settings`)

**Problem.** `save_profiles` updates `self.config.profiles` itself, so the
config-changed notification that follows sees no change and does not rebuild
the sidebar. After an import, the new backups are missing from the sidebar
(and on a fresh install the main area is blank) until the next 30-second tick.

**Fix.** After a successful save in `import_settings`, call
`self.rebuild_nav(None)` and `self.activate_selected()`.

**Verify.** By hand on a fresh settings folder: import a file; the backups
appear at once.

---

### U-6. Discarding the wizard leaves a blank page

**Medium · S · Verify first**

**Files:** `src/app/nav.rs` (`discard_wizard`, `rebuild_nav`)

**Problem.** New backup → Cancel → Discard. `rebuild_nav` keeps the wizard's
row selected because the wizard row was showing, but the wizard is gone, so the
selected row is "New backup", which no view handles: the main area is empty.

**Fix.** In `discard_wizard`, call `self.go_home()` after rebuilding; or make
`keep_wizard` require `self.wizard.is_some()`.

**Verify.** By hand with at least one backup set up.

---

### U-7. A wizard finish that returns early leaves the sidebar stale

**Medium · S · Verify first**

**Files:** `src/app/effects/wizard.rs:188-208` (`on_wizard_finished`)

**Problem.** If the backup being edited was removed meanwhile, or saving the
new profile failed, the function returns early. The wizard has already closed,
the sidebar is not rebuilt, and the main area is blank. On a failed save the
user's typed settings are lost, though the error says otherwise.

**Fix.** Call `self.rebuild_nav(None)` on both early returns. On a failed save,
keep the wizard open with its data.

**Verify.** By hand: edit a backup, Finish later, remove it from a second
window, then finish the edit.

---

## 5. Packaging

### P-1. The uninstall instructions do not work

**Medium · S · Confirmed (test)**

**Files:** `install.sh:155-156`, `install-tarball.sh:71-72`, `README.md` (install
and uninstall section)

**Problem.** `systemctl --user disable --now 'stellarshot*'` fails: "globs are
not supported for this". The `rm` line also misses the `.path` units that
on-connect backups create.

**Fix.** Use this text in both scripts and the README:

```sh
systemctl --user list-unit-files --no-legend 'stellarshot-backup-*.timer' 'stellarshot-backup-*.path' \
  | awk '{print $1}' | xargs -r systemctl --user disable --now
rm -f ~/.config/systemd/user/stellarshot-backup-*.{service,timer,path}
systemctl --user daemon-reload
```

**Verify.** With one scheduled backup and one on-connect backup set up, run the
block; `systemctl --user list-unit-files 'stellarshot*'` shows nothing.

---

## 6. Documentation

### D-1. Documentation corrections

**Medium · S · Confirmed (code)** — one task, one commit (`docs:`).

| Where | What to change |
|---|---|
| `CONTRIBUTING.md` | Add a "Releasing" section: a version bump needs `Cargo.toml` + `Cargo.lock`, a new `<release>` in the metainfo, all five screenshot `<image>` URLs moved to `/v<new>/`, and a `## [<new>]` changelog section, in one commit. `scripts/validate-metadata.sh` checks all of them and the release job runs it after the tag is pushed. |
| `README.md:890-899`, `CONTRIBUTING.md:20-24` | Mention the always-on backend log, `~/.local/state/stellarshot/backend.log`. Note that under `cargo run` both logs are under `target/test-xdg/state/stellarshot/` (`.cargo/config.toml` redirects `XDG_STATE_HOME`). Add the `SCHED` and `MOUNT` categories. |
| `README.md` Settings table (`:578-582`) | Add **Export / Import settings**: every backup's folders, destination, schedule and history, never a password; imported backups start with schedule and hooks off. |
| `SECURITY.md:77-79` | Folders that are a symlink or someone else's are "left alone, and an error is reported", not "refused". |
| `SECURITY.md:80-85` | Unit files also hold the drive's filesystem UUID (on-connect backups); the password can come from a password command, not only the keyring. |
| `SECURITY.md:70-74`, `README.md:344` | "Lives in memory" only when `$XDG_RUNTIME_DIR` is set; otherwise `~/.cache/stellarshot/run`. |
| `VALIDATION.md:608-610` | The developer log is no longer under `/tmp`; it is `$XDG_STATE_HOME/stellarshot/developer-debug.log`, mode 0600. |
| `VALIDATION.md:163,598` | `src/app/settings.rs` is now `src/app/startup.rs`. |
| `VALIDATION.md:12-19,762-766` | List the CI checks that exist now: package smoke tests (deb, rpm, lintian), tarball-as-root, the two packaged-binary scripts, SHA256SUMS and attestation, `cargo deny`, the MSRV job. |
| `CONTRIBUTING.md:44-51` | `rustic-server` is only needed for `cargo test --test rest_server -- --ignored`; plain `cargo test` never uses it. |
| `CHANGELOG.md` `[Unreleased]` | The 64 KiB cap is for the password command only; a hook keeps the last 16 KiB of stderr and rclone listings are capped at 1 MiB. Check whether the "web interface removed" section refers to anything in a released version; if not, delete it. |
| `i18n/de/stellarshot.ftl` | `error-ambiguous`, `error-too-large-to-open`, `error-duplicate-name`, `error-too-busy` use "Sie"; rewrite with "du" ("Verwende ein längeres Präfix.", "Stelle es stattdessen in einem Ordner wieder her.", "Stelle sie einzeln oder in verschiedene Ordner wieder her.", "Versuche es in Kürze erneut."). |
| `Cargo.toml:72`, `EXPLANATION.md:34` | `app::settings::set_logger` → `app::startup::set_logger`. |
| `EXPLANATION.md:56` | `/tmp/stellarshot-backend.log` → `~/.local/state/stellarshot/backend.log`. |
| `Cargo.toml:163-164` | `install.sh` builds with `release-build` first and then runs `cargo deb --no-build`; update the comment. |
| `src/event_log.rs:9` | `app::settings_export` → `settings_export`. |
| `README.md:986-1022` | Add the missing modules to the table: `app::startup`, `settings_export`, `password_command`, `proc_signal`, `exe`, `engine::disk_tree`. |
| Spelling | `capitalises` → `capitalizes` (`src/schedule.rs:388`, `VALIDATION.md:786`); `acknowledgement` → `acknowledgment` (`SECURITY.md:15`); `towards` → `toward` (`VALIDATION.md:470`, `src/app/pages/profile.rs:531`, `src/engine/estimate.rs:30`). |

**Verify.** `scripts/validate-metadata.sh`, `cargo test --test i18n`, and a
reread of each changed paragraph against the code it describes.

---

## 7. Low priority

### L-1. Low-priority items

Do these after everything above. Each is **S** unless noted. "Code" means
confirmed by reading; "verify" means reproduce first.

**Engine**

1. *(code)* A clean-up or check failure in the same second as the backup's
   success is hidden: `RunState::current_failure` uses `>`
   (`src/run_state.rs:179-184`). Use `>=` for non-backup stages, or add a
   sequence number.
2. *(code)* Every quiet skip adds an event, so an hourly backup with the drive
   unplugged pushes all real history out of the 200-entry log in about a week
   (`src/scheduled.rs:284-291,328-333`). Replace the previous event's time
   when it is the same `Skipped { kind }`.
3. *(code)* A keyring that is not unlocked yet is reported as "no password
   remembered" (`src/keyring.rs:60-80` turns every error into `None`). Return a
   distinct "keyring unreachable" result and skip quietly, or use
   `ErrorKind::KeyringUnavailable`.
4. *(code)* A future timestamp (clock jump) stops the 30-day check, overdue
   detection and failure display until time catches up
   (`src/scheduled.rs:51`, `src/run_state.rs:115,183`). Treat a time after `now`
   as unset.
5. *(code)* `change_password` can leave both keys if a step after `add_key`
   fails, and silently skips removal when `key_id()` is `None`
   (`src/engine/keys.rs:90-99`). Undo the new key on failure; error on `None`.
6. *(code)* After an upload fails, later packs are still uploaded
   (`src/engine/uploads.rs:261-268`). Refuse pack writes once `failed` is set.
7. *(code)* A check that fails with anything but "damaged" runs again on every
   slot (`src/scheduled.rs:180-197`). Record the attempt time and back off.
8. *(verify, M)* `run_state` and `event_log` read-modify-write without a lock
   across processes (`src/run_state.rs:263-271`, `src/event_log.rs:186-197`);
   a window and a scheduled run can lose each other's update. Add a small
   advisory file lock.
9. *(code)* rclone is left running if a `--run` child is killed without
   Cancel (OOM) or the window crashes (`src/app/child.rs:244-253`,
   `src/engine/serve.rs`). Call `handle.kill_group()` after every `wait()`, not
   only after a cancel; consider `PR_SET_PDEATHSIG` for the window's own rclone.
10. *(code)* After a SIGTERM, the main thread can return while the signal
    thread still runs After hooks (`src/runner.rs:319-327`). Park the main
    thread in the "already claimed" branch until `on_term` exits.
11. *(code)* The rclone "Serving restic REST API on" line is matched anywhere in
    any stderr line and the address is not checked (`src/engine/serve.rs:134-169`).
    Require a loopback host (`127.0.0.1`, `::1`, `localhost`).
12. *(code)* The password is copied into growing, non-zeroized buffers when the
    job is serialized for the child and read back (`src/app/child.rs:171-174`,
    `src/runner.rs:479-490`), and `keyring::load_item`'s non-UTF-8 error path
    drops a copy without wiping it (`src/keyring.rs:77-85`). Serialize and read
    into preallocated `Zeroizing` buffers; zeroize `err.into_bytes()`.
13. *(code)* Restore decision replay fails open if the listing ever differs
    between passes: an index past the recorded ones means "restore as is"
    (`src/engine/restore.rs`, `Decisions::get`, `shaped_stream`). Make it an
    error, and check the counts match at the end.
14. *(verify)* An in-place restore that includes a hard-linked file can fail at
    the end with "file exists" (rustic's metadata pass does `fs::hard_link`
    without removing the existing path). Skip existing non-first hard-link
    names in `shape`.
15. *(code, M)* A crafted repository can crash the app: `diff_trees`
    (`src/engine/browse.rs:753-831`) recurses once per tree level; a directory
    node without a subtree panics inside rustic's `ls`. Cap the depth (or use an
    explicit stack) and check `node.subtree.is_some()` before `ls`.
16. *(code)* A single-file restore counts one conflict twice
    (`src/engine/restore.rs:541-558` with `shape`).
17. *(code)* Keep Both can choose a name another item in the same restore uses
    (`keep_both_name` only checks the disk).
18. *(code)* Restore keeps setuid/setgid bits from the snapshot. Mask `0o6000`
    after restore, or document it.
19. *(code)* Settings migrated from the old app ID leave the old folder
    (`~/.config/cosmic/com.github.cosmic-utils.Stellarshot`) at its original
    mode. Tighten it too, or remove it after a successful migration.

**User interface**

20. *(code)* A browse search result can come back after the user cleared the
    field or opened a folder (`src/app/pages/restore.rs`, `Open` and
    `Search("")` keep `searching`). Clear `searching` there.
21. *(code)* A mount result that arrives after the restore page closed is
    dropped on the UI thread, which waits for the FUSE session to end
    (`src/app.rs`, the dropped-message branch; `src/app/effects/restore.rs:21-30`).
    Turn it into an `Unmount` effect.
22. *(code)* "Finish later" does nothing visible when there are no backups yet.
    Hide it in that state.
23. *(code)* `"the settings directory"` in `src/app.rs` (passed to
    `error-config-unreadable`) is not translated.
24. *(code)* Every row on the home page has a button named only "View"; give
    each an accessible name with the backup's name (new key, five locales).
25. *(code)* Small disk reads and writes run on the UI thread: `reload_runs`
    every 30 s, `event_log::record` in several effects, `profile.location()`
    for removable drives (reads mountinfo). Move to `tasks::blocking` if it
    shows as stutter.

**Packaging and tooling**

26. Re-running the release for an older tag fails once a newer tag exists
    (`scripts/validate-metadata.sh:80-95` checks every tag against the old
    checkout's changelog). Only check tags up to the one being released.
27. `ci.yml:55-56` installs `rustic_server` unpinned, unlocked, for tests that
    are all `#[ignore]`: drop the step or pin it. `ci.yml:242` installs `zizmor`
    unpinned.
28. `scripts/lib/packages.sh:46` replaces the caller's `EXIT` trap, leaking the
    strip-logging script's temp file. Chain traps instead.
29. `install.sh:218-223` calls `cmd_build` four times; build once.
30. `release.yml:167-171`: the "only job with `contents: write`" comment sits
    above `deny`, not `publish`.
31. English and Swedish/Bulgarian `menu-settings`/`menu-about` use `...` where
    every other string uses `…`; a few English and Swedish strings use straight
    quotes where the rest use curly ones. The Swiss German locale uses both
    "Sterneschuss" and "Stellarshot" for the product name: pick one. Swedish
    uses "Ta bort" for both Remove and Delete. Bulgarian uses two words for
    Keep. (Ask a speaker before changing translations.)

---

## Considered and not a task

- **Open the repository before running Before hooks** (so a hook does not run
  for a backup that cannot happen). Rejected: a Before hook that mounts the
  destination is a documented use (README hooks section pairs it with
  unmounting After), and that ordering would break it.
- **Another local user connecting to the `rclone serve restic` port.** Checked
  on a real server with credentials from the environment: no credentials and
  wrong credentials both get 401.
- **Credentials in logs.** reqwest strips user and password from request URLs
  before its errors and rustic's retry warnings print them; rustic masks the
  password in `location()`; rclone's ready line has no credentials.
- **`cargo audit`**: no vulnerabilities. Unmaintained and unsound transitive
  crates are covered by B-6.

## What was checked and found sound

Recorded so a later review does not repeat it:

- **Secrets:** the password reaches child processes only on stdin (never argv
  or environment), `--run` and `--scheduled` are non-dumpable, password-command
  output is capped and zeroized, and the keyring calls are bounded.
- **Imports:** an import clears password commands, disables hooks and sets the
  schedule to manual. It refuses unsafe IDs, rclone remotes and REST URLs, and
  bounds retention and file size. Exports are written atomically at 0600.
- **Locking:** lock and progress files live in a verified private directory
  with OFD locks. The status probe never takes the lock, and the lock name is
  stable across processes.
- **Retention:** forget is scoped to this computer and this backup, pinned
  snapshots are kept, and prune is paused while the repository is marked
  damaged.
- **Process lifecycle:** `rclone serve` is reaped on drop and on every failed
  start, the upload accounting is panic-safe, and `Repo` and `Browser` drop the
  repository before its rclone.
- **Unit files:** quoting and escaping are correct, `ConditionFileIsExecutable`
  skips quietly after uninstall, and `UMask=0077` is set.
- **Mount:** read-only, with no `allow_other` and setuid bits masked.
- **Packaging:** every packaging path passes `release-build`, and the release
  checks the tag against `Cargo.toml`. Checksums and provenance attestation are
  produced, actions are pinned to SHAs, and workflow permissions are minimal.
- **Locales:** all five have the same keys and placeholders.
- **No tooling or personal references** anywhere in the tree.
