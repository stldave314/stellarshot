# Production readiness: status

**Date:** 2026-09-30 · Worked from a full review of the codebase. Nothing is
committed; the working tree holds all of it.

Each finished item has a test that failed before the fix and passes after;
security controls and hangs also had a negative control (the fix removed, the
test confirmed red, the fix restored). Items below marked **Open** keep their
detailed steps.

## Finished

| ID | What |
|---|---|
| CI-31 | Packaged-binary logging check could never fail |
| CI-30 | Tarball installer re-owned /usr to the extracting user |
| DATA-1 | Unreadable profiles file became an empty list and was saved over |
| DATA-3 | One import could add the same backup twice |
| DATA-4 | Import size cap, version check, field validation |
| DATA-5 | Exports were briefly world-readable |
| DATA-8 | Unreadable run state stuck; unknown error kinds |
| ENG-1 | Retention of 0 forgot every snapshot; huge values panicked |
| ENG-2 | Folder archive corrupted when a file shrank mid-backup |
| PROC-1 | Cancel/stop/SIGTERM skipped After hooks |
| PROC-2 | A hook could hang the backup forever |
| PROC-3 | A crashed child was shown as Canceled |
| PROC-4 | Scheduled unit had no start timeout or stop policy |
| UI-31 | Failed profile saves were never shown (partial: settings other than profiles still use the old path) |
| TST-30 | Tests ran with the cache off |
| TST-31 | Three security tests could not fail |
| TST-35 | Tests touched real state |
| TST-36 | Two tests were flaky under load |
| CI-32 | cargo-auditable never installed; .deb built separately |
| CI-33 | Release did not check tag against Cargo.toml |
| CI-34 | Packages never installed or started in CI |
| CI-35 | Removed package left failing user units |
| CI-36 | Dependabot/advisories only on push |
| CI-37 | Release checksums; floating toolchain (toolchain pin not done) |
| CI-38 | Implicit test dependencies (coverage job not done) |
| DOC-30 | Two released versions had no changelog entry |
| DOC-31 | README/CONTRIBUTING drift |
| DOC-32 | Test prerequisites undocumented |
| UI-32 | Password change now applies even if the dialog was closed; Cancel disabled while running |
| UI-33 | Wizard Cancel/Back disabled while busy; results tagged with a wizard session and dropped when stale (no App-level test harness exists, so not unit tested) |
| UI-34 | Remember password only after the repository opened (wizard and unlock) |
| UI-36 | Restore page can close when its backup vanished; state pruned on config change |
| UI-37 | Edits applied to the backup as it is now (`Wizard::apply_to`, tested) |
| UI-39 | Quit also asks for a running restore, wizard creation, busy dialogs |
| UI-40 | Keyring forget failure on remove is reported |
| ENG-3 | Restore never writes through a symlink in the destination (two tests, first red before the fix) |
| ENG-5 | FUSE callbacks guarded; `Browser::missing` no longer holds the repository lock while statting the disk |
| DATA-2 | Importing history evicted the newest local events; it now merges by date, trims the oldest afterward, marks imported events as coming from elsewhere, and runs off the UI thread |
| DATA-6 | Settings and state folders tightened to owner-only at every start (`paths::tighten_app_dirs`) |
| DATA-7 | "Trusted network" fails closed, a tap device is no longer a VPN, the D-Bus reads time out |
| DATA-9 | Drifted constants moved to `constants.rs` |
| DATA-10 | Profile IDs are validated when read. Remote names are not: an older shape must keep loading, and `location()` still checks them |
| DATA-11 | Debug log under `$XDG_STATE_HOME/stellarshot/`; an existing log is tightened to 0600; the release-build strip check passes with the new path |
| ENG-4 | rclone is started and reaped by Stellarshot (`engine::serve`); the zombie test was red (21) before and is green |
| ENG-6 | Whole-snapshot restore streams the tree and keeps one byte per file. Peak memory at 200 000 files was **not measured** |
| ENG-7 | The two `/tmp` fallbacks are gone (home found from the password database; otherwise an unusable path, so the operation fails) |
| ENG-8 | `reject_unsafe_name`, `..` in a path, search results checked |
| ENG-9 | Two same-named items into one folder are refused with a localized error |
| ENG-10 | `versions()`/`missing()` only skip "not found" |
| ENG-11 | A panicking progress sink no longer hangs the index write (red without the fix) |
| ENG-12 | `flock` built portably, `EACCES` handled, lock errors name the path, rclone change commands have a timeout, non-UTF-8 config path refused, the status poll remembers the lock key for a minute |
| PROC-5 | Encrypted drives found through their mapper link; one non-UTF-8 mount no longer hides every drive |
| PROC-6 | Déjà Dup importer reads GLib's double-quoted strings and all its escapes |
| PROC-8 | Hook, password-command and rclone output is capped (`bounded`) |
| PROC-9 | Scheduled runs are non-dumpable; password-command output is zeroized |
| PROC-10 | Password command: no stdin, judged by what it printed, not its orphans |
| PROC-11 | rclone isolated from the user's `RCLONE_*`, `LC_ALL=C` everywhere, group kill on timeout, `--` before a positional remote |
| PROC-12 | Documented in the README. The "test as a scheduled run" button was **not** added |
| PROC-13 | Déjà Dup detection off the UI thread |
| UI-35 | Restore page results are keyed; the shared `busy` flag is gone |
| UI-38 | Dialogs queue: a background result waits behind a confirmation; sign-in closes only its own dialog |
| UI-41 | Applet: one refresh at a time, open failure shown, no `unwrap`, status named beside the icon |
| UI-42 | Typed errors in the wizard, localized list separator, snapshot/hook/size/ratio messages, icon for the arrow, estimate failures shown |
| UI-43 | Remove buttons named for their item; password dialogs focus and submit |
| UI-45 | "Replace N files…" destructive button |
| UI-46 | Folder-size scans are canceled on close, reopen and discard |
| UI-47 | Open-a-copy size cap and age-based cleanup |
| UI-48 | Drive folder with `..` refused |
| UI-49 | Settings migration is atomic |
| UI-50 | Multi-part restores report the total. Reusing `profile::progress` for the card was not done |
| ARC-32 | Restore page messages carry a session (no App-level test harness, so not unit tested) |
| ARC-33 | `[lints]` added with `unwrap_used`, `expect_used`, `panic`, `unsafe_op_in_unsafe_fn`, `missing_debug_implementations` and a `clippy.toml` allowing them in tests. **Pedantic was not adopted**: it reports about 1 490 warnings on this code |
| TST-32 | Large file, mtime, directory mode, hard-link pair, FIFO added. Disk-full added (`RLIMIT_FSIZE`); "next run reuses uploaded packs" not done; not negative-controlled against a metadata-skipping restore |
| TST-33 | Scheduled failure path tested on a private bus (no password; wrong password); red with an empty secret |
| DOC-33 | Screenshot URLs name the release tag; `validate-metadata.sh` fails on one that does not |
| ARC-30 | `app.rs` split from 3 433 to about 1 150 lines: `launch`, `nav`, `dialog`, `effects/{profile,restore,wizard,settings,dialog}`, `pages/{help,settings}`; `app/settings.rs` is now `app/startup.rs`. The moved modules glob-import their parent's scope; narrowing those, and trimming `app.rs` toward 800, remains |
| ARC-31 | Remember-password task, theme index, `tasks::snapshots`, `hostname()`, `end_before_start`, `edit_wizard`, the "ago" sentences, the shared `row()`, one streaming helper in `tasks.rs`. The progress card is still two copies (UI-50) |

Not done from the finished items: UI-31's other settings paths, CI-37's
toolchain pin, CI-38's coverage job, DATA-8's "reset status" action, PROC-4's
notification waiter; and the partials noted in the rows above.

## Open

Nothing is open from the review. What remains is follow-up:

- The restore page's progress card duplicates `profile::progress`; sharing it
  means giving the restore page the same running state (started, last moved,
  stall notice).
- Narrow the `use super::*` globs in the `app/` modules to named imports.
- Pedantic clippy (about 1 490 warnings on this code), in one pass.
- A "test as a scheduled run" button (PROC-12).
- "The next run after a killed backup reuses the packs already uploaded"
  (TST-32).
