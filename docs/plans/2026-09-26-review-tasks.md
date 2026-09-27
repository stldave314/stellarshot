# Code review: corrections and hardening tasks

**Date:** 2026-09-26 · **Baseline:** `main` at `7a8490b` (0.6.0) plus the
uncommitted web-interface work in the working tree (`web_daemon.rs`,
`web_tls.rs`, `ErrorKind::AppUpdated`, route and locale changes).

This is the task list from a full review of the codebase: the rustic engine,
every place that spawns or feeds an external process, the web interface and
REST API, the COSMIC GUI, i18n, packaging, CI, and documentation. Each task
says what is wrong, where, how to fix it, and how to prove the fix. Pick tasks
up in the order given under [Sequencing](#sequencing).

---

## Contents

1. [Ground rules for every task](#ground-rules-for-every-task)
2. [Status legend](#status-legend)
3. [Summary](#summary)
4. [Security: processes, secrets and files (SEC)](#security-processes-secrets-and-files-sec)
5. [Security: web interface and API (WEB)](#security-web-interface-and-api-web)
6. [Correctness and reliability (REL)](#correctness-and-reliability-rel)
7. [GUI, COSMIC conventions and accessibility (UI)](#gui-cosmic-conventions-and-accessibility-ui)
8. [i18n (I18N)](#i18n-i18n)
9. [Architecture and code quality (ARC)](#architecture-and-code-quality-arc)
10. [Tests (TST)](#tests-tst)
11. [CI, supply chain and packaging (CI)](#ci-supply-chain-and-packaging-ci)
12. [Documentation (DOC)](#documentation-doc)
13. [Standards baseline](#standards-baseline)
14. [What is already right: do not regress](#what-is-already-right-do-not-regress)
15. [Sequencing](#sequencing)
16. [Review evidence](#review-evidence)

---

## Ground rules for every task

These are the project's standing conventions. A task is not done until all of
them hold.

- **Zero warnings.** `cargo check` and `cargo clippy --all-targets` stay
  clean. Once [ARC-9](#arc-9-add-a-lints-table-and-clippytoml) lands, that
  includes the new `[lints]` table.
- **Every behavior change gets a test first.** Write the failing test, watch it
  fail for the reason in the task, then fix. Security controls get a standing
  test that **fails (not skips) when its fixture is missing**.
- **Security fixes are proven against a running system**, not by reading the
  code. Each WEB task lists a `curl` or `openssl` command; run it before and
  after and paste both results into the PR.
- **i18n:** every user-facing string goes through `fl!`. Adding, removing or
  rewording a key updates **all five** locales (`en`, `bg`, `de`, `gsw`,
  `sv`) in the same change. `cargo test --test i18n` must pass.
- **American spelling** everywhere: code, comments, identifiers, docs, commit
  messages.
- **README stays current.** If observable behavior changes, update the README
  (and `docs/web-interface.md` for API changes) in the same commit.
- **Constants** go in `src/constants.rs`, documented, never duplicated.
- **Logging:** diagnostics through `debug_log!(CATEGORY, …)`; genuine errors
  through `error_log!` (stderr and the debug log).
- **Conventional Commits**; only `feat:` and `fix:` bump the version.

## Status legend

| Mark | Meaning |
|---|---|
| **Verified** | Confirmed by reading the code during review (and by command where noted). |
| **Confirm first** | Strong evidence, but depends on third-party behavior. Write the failing test before changing anything; if it passes on current code, close the task with the test as proof. |

Severity: **Critical** (exploitable now or silent data loss in normal use) ·
**High** · **Medium** · **Low**. Effort: **S** (under half a day) · **M** (1–2
days) · **L** (3+ days).

---

## Summary

No Critical findings. The fundamentals are sound: no shell anywhere, passwords
never on argv or in the environment, private rclone config, atomic unit files,
fail-closed web auth, TLS always on, and a clean `cargo audit`.

The most important work, in order:

| ID | Sev | Title |
|---|---|---|
| [SEC-1](#sec-1-an-imported-settings-file-can-run-commands) | High | An imported settings file can run commands (hooks, schedule, rclone remote) |
| [REL-1](#rel-1-retention-rules-from-one-backup-prune-another-backups-snapshots) | High | Retention rules from one backup prune another backup's snapshots in a shared repository |
| [REL-2](#rel-2-after-hooks-are-skipped-when-the-repository-fails-to-open) | High | "After" hooks are skipped when the repository fails to open (a stopped service stays stopped) |
| [REL-3](#rel-3-the-backup-child-can-deadlock-on-a-full-stderr-pipe) | High | The backup child can deadlock on a full stderr pipe |
| [REL-4](#rel-4-hooks-and-password-commands-can-hang-past-their-timeout) | High | Hooks and password commands can hang past their timeout |
| [REL-5](#rel-5-excluded-paths-are-not-escaped-for-glob-matching) | High | Excluded paths are not escaped for glob matching (the repository can back up into itself) |
| [REL-6](#rel-6-non-utf-8-file-names-cannot-be-browsed-mounted-or-restored-singly) | High | Non-UTF-8 file names cannot be browsed, mounted, or restored individually |
| [I18N-1](#i18n-1-fluent-syntax-error-drops-the-token-shown-once-warning) | High | A Fluent syntax error drops the "token shown only once" warning in every locale |
| [UI-1](#ui-1-the-applets-open-button-launches-another-applet) | High | The applet's Open button launches another applet |
| [UI-2](#ui-2-launch-flags-are-lost-when-an-instance-is-running) | High | `--new-backup`, `--restore` and `--profile` are ignored when an instance is running |
| [UI-3](#ui-3-there-is-no-way-to-quit) | High | There is no way to quit, yet the menu and an error message say "Quit" |
| [UI-4](#ui-4-deleting-a-snapshot-has-no-confirmation) | High | Deleting a snapshot has no confirmation |
| [SEC-2](#sec-2-restoring-a-crafted-snapshot-can-write-outside-the-target) | Medium | Restoring a crafted snapshot can write outside the chosen folder |
| [WEB-1](#web-1-credential-and-profile-changes-never-reach-the-running-daemon) | Medium | Credential and profile changes never reach the running daemon |
| [WEB-2](#web-2-no-header-read-timeout-or-connection-cap-allow-list-checked-after-tls) | Medium | No header read timeout or connection cap; allow-list checked only after TLS |
| [WEB-11](#web-11-there-is-no-csrf-protection) | Medium | No CSRF protection; cross-site requests can already trigger the lockout |

Totals: 12 High findings, 39 Medium, 22 Low. Test coverage for the High items
is listed in [TST-1](#tst-1-add-regression-tests-for-every-high-finding),
which is itself marked High because it gates each fix.

---

## Security: processes, secrets and files (SEC)

### SEC-1. An imported settings file can run commands

**Status: Done.** `merge()` now rejects an unsafe ID or rclone remote
outright, resets schedule to Manual and disables every hook on every added
profile (with a review-hint dialog), `Destination::location()` independently
enforces the remote shape, the rclone-command string ends with ` --`, and
exports are written mode 0600. Not yet done: the manual end-to-end GUI check
(import a crafted file, attempt to unlock, confirm no command ran) — the
equivalent logic is unit-tested directly instead.

**High · M · Verified**

**Files:** `src/settings_export.rs:104-128`, `src/app.rs:2641-2659`,
`src/runner.rs:267-268`, `src/hooks.rs`, `src/engine/repo.rs:162-176`,
`src/profile.rs:130`

**Problem.** The module's own doc comment (`settings_export.rs:10-20`) treats
an export file as untrusted and says nothing from it may run unprompted.
`merge()` clears only `password_command`. It keeps:

1. **`hooks`.** Each hook is an arbitrary command. `apply_schedule` installs
   a systemd timer for every imported profile right away, so the hook runs at
   the first timer fire once the user ticks "Remember password". The web API's
   `run` route runs it too.
2. **`schedule`.** It is the trigger for (1).
3. **`Destination::Rclone.remote`.** It is used verbatim as `"{remote}:{path}"`,
   the last argument to `rclone serve restic`. A value such as
   `:sftp,host=h,ssh="sh -c '…'"` makes rclone 1.64+ run a local command. A
   value starting with `--` becomes an rclone flag. rustic starts rclone in
   `unopened()`, **before** the password is checked, so this fires as soon as
   the user tries to unlock the imported backup.
4. **Profile IDs** and history for any ID, without validation.

**Fix.**

1. In `merge()`, for every added profile: set `schedule = Schedule::Manual`
   and set every hook's `enabled = false`. Keep the hooks so the user can
   review them, rather than silently losing them. After an import that added
   any hooks or schedules, show a dialog: "Imported backups start with
   schedules and hooks turned off. Review them before turning them on." This
   needs new locale keys in all five locales.
2. Add `fn valid_rclone_remote(name: &str) -> bool` to `profile.rs`. It
   accepts only `^stellarshot-[0-9a-f]{8}$` (the only form
   `new_remote_name()` in `wizard/place.rs` produces) **and** requires the
   section to exist in Stellarshot's own `rclone.conf`. Enforce it in
   `merge()` (reject the profile) and in `Destination::location()` (return
   an error), so a hand-edited config is also covered.
3. Validate imported profile IDs with `schedule::valid_id`. Move it to
   `profile.rs` as `pub(crate)`. Skip history for IDs that are not among the
   added or existing profiles.
4. Defense in depth: end the `rclone-command` string built in `repo.rs` with
   ` --` so nothing rustic appends can be parsed as a flag. Check rclone's
   `serve restic` argument handling first; add a test that proves it.
5. Write exported files with mode `0600` (`app/tasks.rs:170-179`). Hooks can
   carry credentials (`mysqldump -pX`).

**Verify.**

- Extend `a_password_command_from_an_untrusted_export_is_cleared_on_import`.
  The export carries a hook, `Schedule::Hourly`,
  `Destination::Rclone { remote: ":sftp,ssh=\"touch /tmp/pwn\"" }`, and an ID
  of `../x`. Assert: the hook is disabled, the schedule is `Manual`, the rclone
  profile and the bad ID are rejected.
- Unit test: `Destination::Rclone { remote: ":local" }.location()` and
  `remote: "--config=/x"` both return `Err`.
- Manual, with rclone ≥ 1.64: import the crafted file, attempt an unlock, then
  `ls /tmp/pwn` must fail. `stat -c %a` on an export must print `600`.

---

### SEC-2. Restoring a crafted snapshot can write outside the target

**Status: Core fix done; the full malicious-snapshot integration test is a
disclosed gap.** `restore_one` now rejects, with the new `ErrorKind::UnsafePath`,
any item `repo.ls` yields whose relative path contains a component other than
`Component::Normal` — covering both a `..` walk-up and an absolute path
(`PathBuf::join` with an absolute right side discards the destination
outright). `reject_unsafe_relative_path` is unit-tested directly for both
cases plus the empty-path (the restored item itself) and ordinary-path
non-regression cases. Not done: an end-to-end test that hand-builds a tree
via `rustic_core`'s low-level (largely private) tree-saving API or ships a
crafted fixture repository and restores from it — `rustic_core::Node::new`
is public (`derive_more::Constructor`) but saving a tree as a real blob and
pointing a snapshot at it needs write access this crate doesn't otherwise
use directly. The unit tests prove the check's logic is correct; they do not
prove `NodeStreamer` is the only path a name can reach `restore_one` through.
Browse, search, versions and mount still show unvalidated names as-is — only
the write path (restore) is fixed here.

**Medium · M · Confirm first**

**Files:** `src/engine/restore.rs:240-283`; upstream
`rustic_core-0.13/src/blob/tree.rs:590-594`,
`backend/local_destination.rs:214-219`

**Problem.** Tree node names are not validated when deserialized. A node named
`../../.bashrc`, or an absolute `/home/v/.config/autostart/x.desktop`
(`PathBuf::join` with an absolute path *replaces* the base), produces a
restore path outside the chosen folder. Writing such a tree needs the
repository key, so the attacker is another machine or person with a key to a
shared repository. Stellarshot explicitly supports that (`keys.rs`, "for
another person or machine").

**Fix.** In `restore_one`, before a pair is pushed into `shaped`, require
`relative.components().all(|c| matches!(c, Component::Normal(_)))`. On
violation, fail the restore with a new `ErrorKind::UnsafePath` (not a silent
skip). Apply the same check to names shown by browse, search, versions and
mount. File an upstream issue with rustic_core asking for node-name
validation on deserialize (restic rejects such names); link it in the code
comment.

**Verify.** Engine test: build a tree by hand containing `"../escape"` and
`"/tmp/abs"` and save it with rustic_core's tree-saving API (or commit a small
fixture repository). Restore into `tmp/dest`. Assert the restore returns
`UnsafePath` and that `tmp/escape` does not exist.

---

### SEC-3. REST credentials are stored and reported in plain text

**Status: Partially done.** `Location` and `Destination`'s `Debug` impls are
now hand-written to redact a `Rest` URL through the existing `redact_url`,
instead of a `#[derive]` printing it raw (`Job` needed no separate fix: it
only embeds these two types, which now redact themselves). A new
`engine::scrub_url_credentials` strips `user:pass@` out of any *embedded*
URL in a longer message — unlike `redact_url`, which needs the whole string
to be a URL — and `EngineError`'s `From<RusticError>` runs every detail
through it unconditionally (an error text with nothing to scrub is a
no-op). The wizard now rejects a `rest_url` that is not a parseable
`http`/`https` URL, where it previously accepted any non-empty text.
SECURITY.md's "passwords are never written to Stellarshot's own files" is
corrected with an explicit REST exception. **Not done:** moving the REST
password into the keyring (item 1, the largest part of the fix) — the URL
is still stored, credentials and all, in the profile's own cosmic-config
file. This needs a migration path for existing profiles and a wizard change
to collect the password separately from the URL, neither of which fit this
session; SECURITY.md's new wording discloses the gap rather than
overclaiming it is fixed.

**Medium · M · Verified (storage) / Confirm first (error text)**

**Files:** `src/profile.rs:49-54`, `src/engine/error.rs:105-111,153-159`,
`src/engine/repo.rs:48` (`derive(Debug)`), `src/app/wizard/place.rs:235-239`,
`SECURITY.md:37`

**Problem.**

- `Destination::Rest { url }` keeps `user:pass@` inside the URL, persisted
  through cosmic-config at umask mode (0644 under the common 022 umask).
- `SECURITY.md` says "Passwords are never written to Stellarshot's own files",
  which is false for REST.
- `EngineError.detail` is `err.to_string()`. If rustic_backend or reqwest
  includes the URL in an error (``URL `…` parsing failed``), the credentials
  reach dialogs, the event log, notifications and settings exports.
- `Location`, `Destination` and `Job` derive `Debug` with the raw URL.

**Fix.**

1. Store the REST password in the keyring
   (`keyring::store_rest_credentials(profile_id)`), and keep only the
   credential-free URL in the profile. Re-insert the credentials in
   `Destination::location()`. Migrate existing profiles on first load.
2. Validate the URL with `url::Url::parse` in the wizard.
3. Hand-write `Debug` for `Location::Rest` and `Destination::Rest` using the
   existing `redact_url`.
4. When the location is REST, scrub `userinfo@` from `EngineError.detail`
   before it leaves the engine.
5. Correct `SECURITY.md`.

**Verify.** Save a REST profile with password `s3cret`, then
`grep -r s3cret ~/.config/cosmic/io.github.stldave314.Stellarshot/` finds
nothing. Unit test: an `EngineError` from probing
`rest:http://a:s3cret@127.0.0.1:1/r/` does not contain `s3cret`, and
`format!("{:?}", location)` does not either.

---

### SEC-4. The Google client secret is on argv during sign-in; sign-in cannot be canceled

**Status: Partially done.** `sign_in` now takes credentials as a separate
`Option<(&str, &str)>` rather than folding `client_id=`/`client_secret=`
into the same `params` list that becomes argv, and passes them to rclone as
`RCLONE_<PROVIDER>_CLIENT_ID`/`_CLIENT_SECRET` environment variables
instead — derived from `provider` rather than hardcoded to `DRIVE`, so it
still works if a second OAuth provider is ever added.
`tests/rclone_credentials.rs` proves it against a real child process: a
stand-in `rclone` records both its own argv and its environment, and the
test asserts the secret and client ID appear in the environment and
nowhere in argv, plus a second test that signing in without credentials
sets neither variable at all. **Not done:** a deadline and Cancel button
for the sign-in flow, and `process_group(0)` plus killing the group on
cancel — those need UI changes (a cancelable dialog) this session didn't
get to, and are unrelated to the argv/environment fix itself.

**Low · S · Verified**

**Files:** `src/app.rs:1127-1133`, `src/engine/rclone.rs:338-353`

**Problem.** `rclone config create … client_secret=…` puts the secret in
`/proc/<pid>/cmdline`, which every local user can read with `ps -ef`, for the
whole browser OAuth flow. The code already treats the value as a secret
(`redact()`). `sign_in` has no timeout or cancel. `purge` and `config delete`
through `rclone()` have no timeout either.

**Fix.** Pass the ID and secret through the environment
(`RCLONE_DRIVE_CLIENT_ID`, `RCLONE_DRIVE_CLIENT_SECRET`); `/proc/<pid>/environ`
is owner-only. Drop the `client_*=` arguments. Give sign-in a deadline and a
Cancel button, spawn with `process_group(0)`, and kill the group on cancel.
Fold this into the shared process helper from
[ARC-2](#arc-2-remove-duplicated-logic).

**Verify.** During sign-in, `ps -eo args | grep client_secret` finds nothing.
Clicking Cancel leaves no `rclone` process (`pgrep rclone`).

---

### SEC-5. Shared `/tmp` fallbacks for the runtime directory and backend log

**Status: Done, except item 3 (progress-file `O_NOFOLLOW`).**
`runtime_dir` and the new `rustic_log_path` (`app/settings.rs`) both fall
back to `$XDG_CACHE_HOME`/`$XDG_STATE_HOME` (or `~/.cache`/`~/.local/state`)
rather than `/tmp` when their usual XDG variable is unset, and
`create_private_dir` now verifies a pre-existing directory at that path
(not just one it creates itself) is not a symlink, is owned by the current
user, and is not readable or writable by group or other.
`debug::open_private_log_file` gained the matching check for a
pre-existing *file*: `O_CREAT` without `O_EXCL` opens rather than fails on
one, which a symlink check alone does not cover. Not done: routing the
`.progress.tmp` write in `runner.rs`'s `Output::emit` through
`O_NOFOLLOW` — now that its directory is verified private, a symlink there
could only have been planted by this same user, which is a much narrower
residual than the original shared-`/tmp` scenario; left as a disclosed gap
rather than done under time pressure. The README's troubleshooting section
did not reference the backend log's old path, so it needed no change;
VALIDATION.md documents all of the above.

**Low · S · Verified**

**Files:** `src/engine/lock.rs:18-24,267-273`, `src/runner.rs:189-193`,
`src/app/settings.rs:76`, `src/debug.rs:34,60-73`,
`src/engine/rclone.rs:50-58`

**Problem.**

- When `XDG_RUNTIME_DIR` is unset (`su -`, SSH without `pam_systemd`, cron),
  locks, progress files and Open Copy fall back to `/tmp/stellarshot`.
  `DirBuilder::recursive(true).mode(0o700)` succeeds silently on an existing
  directory **owned by someone else**.
- An attacker who creates that directory first can:
  - hold `flock` on the predictable lock files, so every backup is quietly
    skipped as `Locked`;
  - plant a symlink at `<key>.progress.tmp`, which `fs::write` follows and
    truncates.
- The always-on backend log `/tmp/stellarshot-backend.log` (shipped in release
  builds) collides between users. It is correctly opened with `O_NOFOLLOW`,
  but a pre-created file silently disables it.

**Fix.**

1. `runtime_dir()`: never fall back to `temp_dir()`. Fall back to
   `$XDG_CACHE_HOME/stellarshot/run` (or `~/.cache/stellarshot/run`), or
   fail.
2. After creating any private directory, `symlink_metadata` it and require:
   not a symlink, `uid == getuid()`, and `mode & 0o077 == 0`. Otherwise
   return an error.
3. Write progress files through `atomicwrites` or with `O_NOFOLLOW`.
4. Move the backend log to `$XDG_STATE_HOME/stellarshot/backend.log`. After
   opening, `fstat` and require `st_uid == getuid()`. Move both log paths into
   `debug.rs` (see [ARC-7](#arc-7-debug-logging-meets-the-standard-fully)).
5. Update the README troubleshooting section with the new log path.

**Verify.** Unit test: `acquire_in(dir)` where `dir` is mode 0777, or is a
symlink, returns `Err`.
`env -u XDG_RUNTIME_DIR stellarshot --run backup < job.json` does not touch
`/tmp`. As user B, `touch /tmp/stellarshot-backend.log`; user A's log is
still written to its new location.

---

### SEC-6. Timers and the web service can point at a binary in a world-writable directory

**Low · S · Verified**

**Files:** `src/schedule.rs:86-101,147-154`, `src/web_daemon.rs:44-71`

**Problem.** Run the portable tarball from `/tmp`, `/var/tmp` or `/dev/shm`
and the persistent user unit gets `ExecStart="/tmp/…/stellarshot"`. After a
reboot wipes `/tmp`, any local user can create that path, and the timer runs
their binary as the victim.

**Fix.** Add `fn trusted_executable(path: &Path) -> bool`. It returns false
when the file or any ancestor directory is writable by group or others,
unless the directory is sticky and root-owned, or when the file is owned by
neither root nor the current user. Refuse to write a unit for an untrusted
path, and show a localized error: "Install Stellarshot before scheduling
backups".

**Verify.** Unit test: a `tempfile` directory chmodded 0777 gives `false`;
`/usr/bin/true` gives `true`.

---

### SEC-7. Private key and Open Copy permission gaps

**Low · S · Verified**

**Files:** `src/web_tls.rs:50-92`, `src/app.rs:2035-2050`

**Problem.**

- `OpenOptions::mode(0o600)` only applies when a file is **created**. When
  `cert.pem` is missing but a looser `key.pem` exists, the regenerated key
  keeps the old mode. The web directory is created at default permissions.
- Open Copy: `set_permissions(&copy, 0o400)` follows symlinks. If the
  restored version is a symlink, the chmod lands on its target (for example
  `~/.ssh`), and `xdg-open` then opens that target.

**Fix.**

- TLS key: write to a temporary file created with
  `create_new(true).mode(0o600)`, fsync it, then rename it into place (or use
  `atomicwrites`). Create the directory with `DirBuilder::new().mode(0o700)`.
  Warn in the log if a user-supplied key file is readable by group or others.
- Open Copy: require `symlink_metadata(&copy)?.is_file()` before the chmod and
  the open; refuse anything else.

**Verify.** Test: create `key.pem` at 0644, delete `cert.pem`, call
`self_signed_paths`, and assert mode 0600. Test: Open Copy of a symlink
version returns an error and changes no permissions.

---

### SEC-8. Secrets are not zeroized

**Status: Done for the `--run` child; the web daemon's own dumpable flag is
the peer session's file, not touched here.** `Secret` is now backed by
`secrecy::SecretString`, which zeroizes on drop and redacts its own
`Debug`; a hand-written `Serialize` (documented as the one deliberate
`expose_secret` call besides `expose()` itself — `secrecy` refuses a
derived one on purpose, precisely to prevent an accidental new one) keeps
the exact same bare-JSON-string wire format `#[serde(transparent)]`
produced, proven with a round-trip test. Both stdin buffers that carry a
serialized `Job` (`app/child.rs`'s write side, `runner.rs`'s read side) are
now `zeroize::Zeroizing<Vec<u8>>` instead of a plain buffer whose `drop`
only deallocates. The `--run` child calls
`rustix::process::set_dumpable_behavior(NotDumpable)` at the very top of
`runner::main`, before parsing anything.

**Proven against a real process, not just compiled:** `/proc/<pid>/mem` is
owned by this user for an ordinary process (`/proc/self/mem`, confirmed by
hand) and by `root` once a process has disabled its own dumpable flag — the
kernel's own externally visible sign the `prctl` took effect, since a
process cannot directly observe another's dumpable state any other way.
`tests/child.rs`'s new `the_run_child_disables_core_dumps_for_itself`
spawns the real compiled `--run` child with stdin held open (blocking it
past the `prctl` call, alive long enough to inspect) and asserts
`/proc/<child-pid>/mem` is root-owned. **Proven able to fail**, not just to
pass: temporarily disabling the `set_dumpable_behavior` call made this
exact test fail with `left: 1000, right: 0` (this user's uid where root was
expected), confirming it is not vacuous, before restoring the fix.

**Not done:** the web daemon's own `prctl` call (`src/web.rs`, the peer
session's file tonight) and zeroizing the `String` momentarily produced by
`keyring::load` before it is re-wrapped into a `Secret`.

**Low · M · Verified**

**Files:** `src/engine/repo.rs:26-45` (`Secret`), `src/app/child.rs:131-134`,
`src/runner.rs:342-351`, `src/keyring.rs:57`, `src/web.rs` (`AuthConfig`)

**Problem.** `Secret` wraps a plain `String`, derives `Clone` and `Serialize`,
and is copied into the job JSON buffer, the child's stdin buffer, keyring
results, and every `job()` clone. `drop` frees the memory without wiping it.
The comment at `runner.rs:350` claims otherwise. The web daemon holds the web
password in plain memory for its whole lifetime.

**Fix.** Back `Secret` with `secrecy::SecretString` (it zeroizes on drop and
redacts `Debug`). Serialize through `ExposeSecret` only at the stdin boundary,
into a `zeroize::Zeroizing<Vec<u8>>`. In the `--run` child and the web
daemon, call `prctl(PR_SET_DUMPABLE, 0)` (`rustix::process`) so core dumps
cannot capture passwords. Fix the misleading comment.

**Verify.** `grep -rn 'expose_secret' src` lists only the intended boundaries.
Unit test: `format!("{:?}", secret)` is redacted. Check
`cat /proc/$(pgrep -f 'stellarshot --run')/status | grep -i dumpable` during
a backup (or a test calling `rustix::process::dumpable_behavior()`).

---

### SEC-9. rclone argument hygiene

**Low · S · Verified**

**Files:** `src/engine/rclone.rs:66-146,221,271,289`

**Problem.**

- `rclone_within` logs `args.join(" ")` without `redact()` (lines 86, 133),
  unlike `rclone()`.
- Positional arguments (remote names, paths) are never preceded by `--`.
- Line 221 matches the English text `"not found"` in rclone's output, which
  breaks under a non-English locale. Set `LC_ALL=C` on every rclone command,
  or match rclone's exit code (3 = directory not found, 4 = file not found).

**Fix.** Route every rclone invocation through one builder
([ARC-2](#arc-2-remove-duplicated-logic)) that:

- always uses `--config <private>`;
- sets `LC_ALL=C`;
- inserts `--` before positional arguments where the subcommand accepts it;
- always logs through `redact`;
- applies a timeout.

**Verify.** `grep -n 'Command::new(RCLONE)' src/engine/rclone.rs` shows one
match. A unit test asserts that the builder's logged form of
`client_secret=x` is redacted.

---

## Security: web interface and API (WEB)

Context: the daemon (`stellarshot-web`, a systemd user service) currently
serves five routes: health, list backups, list snapshots, browse, and
`POST …/run`. TLS is always on. The existing controls work (see
[What is already right](#what-is-already-right-do-not-regress)). These tasks
close the gaps. Tests should use `tower::ServiceExt::oneshot` with
`axum::extract::connect_info::MockConnectInfo` for fast coverage of any
client address. Each control also needs **one** real-socket test through the
production `axum_server` path.

### WEB-1. Credential and profile changes never reach the running daemon

**Medium · M · Verified**

**Files:** `src/web.rs:79-100` (everything loaded once), `src/app.rs:2757-2801`
(save password, generate token, allow-list), `src/app/config.rs:128-130`

**Problem.** The daemon reads the password, token hash, enabled methods,
allow-list, network scope **and the profile list** once, at startup.
Regenerating a token, changing the password, turning a method off, or
removing an allow-list entry only writes settings. The config doc comment
says regenerating invalidates the old token "immediately", which is false.
Backups added, edited or removed in the window stay invisible, or still
reachable, through the API until a restart.

**Fix.** Pick one:

- **(preferred)** Keep `AuthConfig`, the allow-list and profiles in
  `arc_swap::ArcSwap`, and reload them when cosmic-config changes (watch the
  config the same way the app does). The keyring password is reloaded on the
  same signal.
- **(minimum)** After any web-auth, allow-list, scope or profile change,
  restart the daemon if it is active (`web_daemon::restart()`), without
  interrupting a running backup (see [WEB-8](#web-8-graceful-shutdown-and-duplicate-run-requests)).

Either way, fix the comment at `config.rs:128-130`, and add "changes apply
immediately" or "restart to apply" wording to the Settings page and
`docs/web-interface.md`.

**Verify.** Unit test: swap the token hash and assert the old token gets 401
via `oneshot`. Live:

```sh
curl -sk -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $OLD" \
  https://127.0.0.1:8737/api/v1/health   # before regenerating: 200
# regenerate in Settings
curl -sk -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $OLD" \
  https://127.0.0.1:8737/api/v1/health   # must now be 401
```

---

### WEB-2. No header read timeout or connection cap; allow-list checked after TLS

**Medium · M · Verified**

**Files:** `src/web.rs:140-180`; upstream `hyper-1.11/src/common/time.rs`
(the default `header_read_timeout` is dropped when no timer is set, with only
a `warn!`), `axum-server-0.8/src/server.rs`

**Problem.** axum-server builds hyper's connection builder without a timer, so
the default 30-second header read timeout never applies. Any host that can
reach the port, **including one not on the allow-list** (the allow-list is
axum middleware and runs only after TLS and a full request), can open a
connection and send nothing. About 1,000 such connections exhaust the
service's file-descriptor limit.

**Fix.**

1. Configure the builder:
   ```rust
   use hyper_util::rt::TokioTimer;
   let mut server = axum_server::tls_rustls::from_tcp_rustls(listener, tls)?;
   server.http_builder().http1().timer(TokioTimer::new())
       .header_read_timeout(Duration::from_secs(10));
   server.http_builder().http2().timer(TokioTimer::new())
       .keep_alive_interval(Some(Duration::from_secs(30)))
       .keep_alive_timeout(Duration::from_secs(10))
       .max_concurrent_streams(32);
   ```
2. Enforce the allow-list at accept time. Write an `AllowListAcceptor` that
   wraps the rustls acceptor (`axum_server::accept::Accept`), checks
   `stream.peer_addr()` **before** the handshake, and drops disallowed peers.
   Keep the middleware as a second layer.
3. Cap concurrent connections: a `tokio::sync::Semaphore` permit held by the
   accepted stream wrapper, for example 64.
4. Add
   `tower_http::timeout::TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, 30s)`
   and a `RequestBodyLimitLayer` (the API takes no bodies today; 16 KiB is
   plenty).
5. Put the numbers in `constants.rs`.

**Verify.** A test spawns the real server, opens a TLS connection, sends
nothing, and asserts it is closed within about 11 seconds. Live:

```sh
(sleep 60) | openssl s_client -connect 127.0.0.1:8737 -quiet & sleep 15
ss -tn state established '( sport = :8737 )'   # must be empty after the fix
# from a host NOT on the allow-list:
openssl s_client -connect HOST:8737 </dev/null | grep -c 'BEGIN CERT'   # must print 0
```

---

### WEB-3. Lockout and brute-force protection

**Status: Done, except item 2's password-strength number is 12 not a
different value someone might argue for, and the manual `xargs -P64 curl`
proof below is not yet run against a real compiled binary tonight.** A
failure only counts against the throttle when the request presented `Basic`
or `Bearer` credentials (`presented_credentials`); lockouts escalate 5/15/60
minutes then cap at 24 hours, one level further each time an address returns
after its previous window fully passed; a global budget (50 failures/10
minutes across every address) pauses password auth for everyone, tokens
unaffected; the check-and-reserve happens under one lock
(`Throttle::try_begin`), closing the race a separate check-then-increment
left open — proven with 64 real OS threads hammering it at once, not merely
asserted; both locks use `unwrap_or_else(PoisonError::into_inner)`; the
address map is pruned after a week of inactivity and capped at 10,000
entries; Settings now refuses a web password under 12 characters
(`password_long_enough`). `docs/web-interface.md` documents the
`X-Forwarded-For` position (item 7). Automated `cargo test` coverage for the
real-socket scenarios (credential-less requests, cross-site requests,
concurrent HTTP-level lockout) could not be run to a clean finish tonight —
see this file's own note below and `VALIDATION.md`'s "Brute-force
throttling" section for why, and for the OS-thread-level test that proves
the one property those would have that a pure logic test alone could not.

**Medium · M · Verified**

**Files:** `src/web.rs:220-330` (`authenticate`, `Throttle`),
`src/app.rs:2757-2767`

**Problem.**

- **Anyone can lock out the owner.** `record_failure` runs for requests with
  **no** `Authorization` header and for `OPTIONS` preflights. A web page the
  user visits can fire five `fetch(…, {mode: "no-cors"})` calls at
  `https://127.0.0.1:8737` (the browser already trusts the certificate, as the
  docs instruct) and lock the owner out for 5 minutes, repeatedly. Behind an
  SSH tunnel or reverse proxy (both suggested in the docs) every client is
  `127.0.0.1`, so one client locks out all of them.
- **Weak rate limit.** 5 attempts per 5 minutes per IP, with no escalation and
  no global budget, gives 1,440 guesses a day per address. The only password
  rule is "not empty".
- **Race.** `locked_out` and `record_failure` take the lock separately, so
  concurrent requests get extra guesses.
- **Unbounded map.** The `HashMap` is never pruned.
- `Mutex::lock().unwrap()` panics a worker if the lock is poisoned.

**Fix.**

1. Count a failure only when an `Authorization` header was present and parsed
   as `Basic` or `Bearer`. Cross-site requests are refused before the throttle
   by [WEB-11](#web-11-there-is-no-csrf-protection). Land WEB-11 first or
   together with this task.
2. Enforce a minimum password length of 12 in Settings (OWASP ASVS 5.0 §6.2),
   with a new localized error.
3. Escalate lockouts per address (5 min, 15, 60, capped at 24 h). Add a
   global budget: more than 50 failures in 10 minutes from all addresses
   pauses password auth for everyone (tokens keep working) and logs it.
4. Check-and-reserve under one lock:
   `fn try_begin(&self, ip) -> Result<Guard, Duration>` counts the attempt
   before verification, and a success clears it.
5. Prune expired entries on insert; cap the map at, say, 10,000 entries.
6. Use `lock().unwrap_or_else(PoisonError::into_inner)`.
7. Document that behind a proxy the allow-list and throttle see the proxy's
   address. Continue to ignore `X-Forwarded-For`.

**Verify.** `oneshot` tests:

- 10 credential-less requests, then a correct Basic gives 200.
- `Sec-Fetch-Site: cross-site` gives 403 and does not count.
- 64 concurrent wrong passwords give at most 5 × 401 and the rest 429.
- The global budget trips.

Live:

```sh
for i in 1 2 3 4 5 6; do curl -sk -o /dev/null https://127.0.0.1:8737/api/v1/health; done
curl -sk -u x:"$PW" -o /dev/null -w '%{http_code}\n' https://127.0.0.1:8737/api/v1/health  # 200, not 429
seq 64 | xargs -P64 -I{} curl -sk -o /dev/null -w '%{http_code}\n' -u x:wrong \
  https://127.0.0.1:8737/api/v1/health | sort | uniq -c   # at most 5 × 401
```

---

### WEB-4. "Reachable on the network" binds every interface with an open allow-list

**Status: Done.** `is_allowed` takes the scope and falls back to
`WEB_PRIVATE_RANGES` (now in `constants.rs`) in `Lan` scope when the
allow-list is empty, and to loopback only in `Localhost` scope; `matches`
canonicalizes the address first. `docs/web-interface.md` says "every network
interface" rather than "the same network". Not run tonight: the live VPN-
interface check (this sandbox has no VPN interface to test against).

**Medium · S · Verified**

**Files:** `src/web.rs:104-110,205-215`, `src/app/config.rs:93-97,132-135`,
Settings text in all locales

**Problem.** The LAN scope binds `0.0.0.0` (VPN, Docker bridges, public Wi-Fi,
a VPS's public address). An empty allow-list allows everyone. The UI says
"any other device on the same network", which understates this.

**Fix.** In LAN scope, treat an empty allow-list as "private ranges only":
`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`,
`127.0.0.0/8`, `fc00::/7`, `fe80::/10`. Put these in `constants.rs`. Reword
the UI (all locales) and the docs to say "all network interfaces". Before any
future dual-stack bind, normalize with `IpAddr::to_canonical()` so an
IPv4-mapped IPv6 address cannot slip past an IPv4 entry. Add that call now,
with a test.

**Verify.** Unit test: `is_allowed(&[], "8.8.8.8".parse()?, Scope::Lan)` is
false and `"192.168.1.5"` is true. `is_allowed` of `::ffff:8.8.8.8` against a
`192.168.0.0/16` list is false. Live: connect through a VPN interface address
and expect a rejection.

---

### WEB-5. Security events are not logged in release builds

**Status: Partially done.** `Throttle::try_begin` logs once (`error_log!`)
the moment an address's lockout starts, and `note_global_failure` logs once
when the global budget is exceeded — both already rate-limited to one line
per event by construction, not by a separate check. Not done: the startup
warning for password auth enabled with nothing loaded from the keyring, the
fail-closed-and-loud exit when no usable method remains, and adding the
peer address/auth method to History entries from the web source.

**Medium · S · Verified**

**Files:** `src/web.rs:200,229` (only `debug_log!`), `src/keyring.rs:99-109`,
`src/web/routes.rs:188-216`

**Problem.** `debug_log!` is compiled out of releases, so a brute-force
attempt, a lockout, or a daemon whose password auth is silently dead (keyring
locked at boot) leaves nothing in
`journalctl --user -u stellarshot-web`. Web-triggered runs do not record the
client address or the auth method (OWASP ASVS 5.0 §16).

**Fix.**

- `error_log!(WEB, …)` when a lockout starts (address, count), rate-limited
  to one line per address per window.
- At startup, log a warning when password auth is enabled but no password
  loaded. If no usable method remains, log an error and exit non-zero (fail
  closed and loud).
- Add the peer address and auth method to History entries from the web
  source.

**Verify.** An integration test runs `CARGO_BIN_EXE_stellarshot-web` with a
temp config, makes 5 bad requests, and asserts stderr contains the lockout
line. Live: `journalctl --user -u stellarshot-web -n 5` shows it.

---

### WEB-6. Response hygiene: headers, status codes, error detail

**Status: Done.** Every response carries the five security headers
(`nosniff`, `no-store`, CSP, `no-referrer`, same-origin CORP), no HSTS. A 401
carries `WWW-Authenticate: Bearer realm="stellarshot"`. `WrongPassword`/
`PasswordNotRemembered` now map to 409, `AuthFailed` to 503, `UnsafePath` to
400, the new `NotFound` kind to 404 — none of the repository-password or
malformed-path cases return 401 or 500 anymore. `ApiError`'s response body
is now `{kind, message, request_id}`, never `EngineError.detail` (which can
carry a password command's or rclone's own stderr, or a local path); the
detail still reaches the log, tied to the same `request_id`, via
`error_log!`. Auth scheme matching (`Basic`/`Bearer`) is case-insensitive.
The password comparison hashes both sides before `ct_eq`, so not even the
password's length leaks through timing. Not done: a live `curl -skI` header
check against the real compiled binary tonight (covered by the same-shaped
`every_response_carries_the_fixed_security_headers_regardless_of_status`
test instead, itself part of tonight's socket-test environment gap — see
WEB-3's status note).

**Low · S · Verified**

**Files:** `src/web.rs:121-138,241-244,337-366`, `src/web/routes.rs:60-90`,
`src/engine/browse.rs:153-158`

**Problem and fix.**

- **No security headers.** Add `SetResponseHeaderLayer::overriding` for:
  - `X-Content-Type-Options: nosniff`
  - `Cache-Control: no-store`
  - `Content-Security-Policy: default-src 'none'; frame-ancestors 'none'`
  - `Referrer-Policy: no-referrer`
  - `Cross-Origin-Resource-Policy: same-origin`

  Send HSTS **only** with a user-supplied certificate.
- **401 without `WWW-Authenticate`** (RFC 9110 §11.6.1). Send
  `WWW-Authenticate: Bearer realm="stellarshot"`. Bearer keeps browsers from
  showing a Basic prompt.
- **Wrong status codes.**
  - A repository password problem (`WrongPassword`, `PasswordNotRemembered`)
    returns **401**, so a client with a valid token sees "bad credentials".
    Use 409 (or 503) with a distinct `kind`.
  - A missing or `..` path returns 500. Return 404, or 400 for a malformed
    path.
- **Internal detail leaks.** 5xx responses serialize `EngineError.detail`,
  which can include the password command's stderr, rclone's stderr and local
  paths. Return `{kind, message}` plus a request ID, and log the detail with
  `error_log!`.
- **Password comparison leaks the length** (`a.len() == b.len() &&`
  short-circuits). Compare `Sha256(a)` with `Sha256(b)` using `ct_eq`, as the
  token check already does.
- Auth scheme names are matched case-sensitively. RFC 9110 says they are
  case-insensitive; use `eq_ignore_ascii_case`.

**Verify.** One `oneshot` test asserts that a 200, 401, 403, 404 and 429 each
carry all the headers. Tests for the status mapping. A test with
`password_command = "sh -c 'echo LEAKME >&2; exit 1'"` asserts the response
does not contain `LEAKME`. Live: `curl -skI … | grep -iE 'nosniff|no-store|www-authenticate'`.

---

### WEB-7. TLS configuration hardening

**Low · S · Verified**

**Files:** `src/web_tls.rs:55-92`, `src/bin/web.rs`, `Cargo.toml` (`rcgen`),
`src/app.rs:2084`

**Problem and fix.**

- **Two crypto providers are compiled in.** `ring` comes through rcgen,
  `aws-lc-rs` through rustls/axum-server. If a future dependency enables
  `rustls/ring`, rustls cannot choose and panics.
  - Call `rustls::crypto::aws_lc_rs::default_provider().install_default()`
    at the top of `web::main`.
  - Set
    `rcgen = { version = "0.14", default-features = false, features = ["aws_lc_rs", "pem"] }`.
  - Check with `cargo tree -i ring -e features`.
- **The self-signed certificate never mismatches less.** Settings shows
  `https://127.0.0.1:port`, but the certificate has no IP subject names,
  which trains users to click through warnings. Add `127.0.0.1` and `::1`
  (and the LAN address in LAN scope) as IP SANs.
- **Validity.** It runs from 1975 to 4096 (rcgen defaults). Use about 2 years
  and add a "Regenerate certificate" button.
- **No way to verify the certificate.** Show its SHA-256 fingerprint in
  Settings so trust-on-first-use can actually be checked (see the
  `curl --pinnedpubkey` example for the docs).
- **Expiry is silent.** Warn in the log when a user-supplied certificate has
  expired.

**Verify.**

- `openssl s_client -connect 127.0.0.1:8737 </dev/null 2>/dev/null | openssl x509 -noout -ext subjectAltName`
  lists `IP Address:127.0.0.1`.
- `curl -sk --tls-max 1.1 https://127.0.0.1:8737/` fails.
- `curl http://127.0.0.1:8737/` fails.

Make the last two standing tests; today they are only checked by hand in
`VALIDATION.md`.

---

### WEB-8. Graceful shutdown and duplicate run requests

**Low · M · Verified**

**Files:** `src/web.rs:171-176`, `src/web/routes.rs:160-216`,
`src/web_daemon.rs:56-72`

**Problem.**

- No graceful shutdown. SIGTERM (Stop, Restart, the WEB-1 restart) kills a
  web-started backup with no History entry.
- `POST …/run` always returns **202**, even when the backup is already
  running. The second run fails later, records a `Locked` failure, and runs
  the password command again.
- The web daemon runs backups **in-process**, contrary to CONTRIBUTING's
  "writes run in a child process".

**Fix.**

- Use `axum_server::Handle` with
  `tokio::signal::unix::signal(SignalKind::terminate())` and
  `handle.graceful_shutdown(Some(30s))`.
- Track jobs in a `JoinSet`. On shutdown, wait for them or record them as
  `Canceled`.
- Add `TimeoutStopSec=60` to the unit.
- Before answering 202, check `lock::is_running` (after
  [REL-8](#rel-8-status-polling-can-make-a-real-backup-fail-with-locked)) and
  return **409** if the backup is already running.
- Spawn the backup through `app::child::run` like the window does, or
  document the deliberate exception in CONTRIBUTING.

**Verify.** Test: two back-to-back POSTs; the second returns 409. Live: stop
the service during a web-started backup, and History shows it as canceled.

---

### WEB-9. Bound the cost of read requests

**Low · M · Verified**

**Files:** `src/web/routes.rs:113-148`, `src/engine/browse.rs:169-185`

**Problem.** Every snapshots or browse request opens the repository and loads
its whole index, with no concurrency limit or timeout. A slow remote ties up
a blocking thread per request. Parallel requests multiply memory use.

**Fix.** Put a `tokio::sync::Semaphore` (2 permits, in `constants.rs`) around
repository-opening routes, returning 503 with `Retry-After` when saturated.
Cache the opened `Browser` per profile for a few minutes, invalidated on any
write. Plan pagination and result caps before adding search or download
routes (see [WEB-12](#web-12-rules-for-routes-not-built-yet)).

**Verify.** Run 20 parallel `curl …/snapshots` against a large repository.
RSS (`ps -o rss`) stays bounded and extra requests get 503.

---

### WEB-10. Service unit hardening and upgrade handling

**Low · S · Verified**

**Files:** `src/web_daemon.rs:56-72`, `install.sh:118-132`

**Problem.**

- `Restart=on-failure`, `RestartSec=5` and no start limit mean a daemon that
  fails at startup (bad TLS path, port in use, binary removed) restarts every
  5 seconds forever.
- A package upgrade leaves the old daemon running old code until the next
  login, so security fixes don't take effect.
- Uninstall leaves the user units behind.

**Fix.** Add to the unit:

- `StartLimitIntervalSec=300` and `StartLimitBurst=5` (in `[Unit]`)
- `RestartSec=30`
- `NoNewPrivileges=yes`, `UMask=0077`, `LockPersonality=yes`
- `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`
- `LimitNOFILE=1024`, `MemoryMax=1G`

Keep the values in `constants.rs`. For upgrades, the daemon polls
`/proc/self/exe` for the ` (deleted)` suffix every minute. When it appears,
it drains jobs (WEB-8) and exits with a distinct code, so `Restart=` starts
the new binary. Reuse the helper from
[REL-11](#rel-11-one-policy-for-a-binary-replaced-while-running).
`install.sh uninstall` prints the `systemctl --user disable --now 'stellarshot*'`
cleanup command.

**Verify.** Point the TLS certificate at a missing file and start the
service. `systemctl --user status stellarshot-web` ends in `failed` ("start
request repeated too quickly"). Replace the binary; the main PID changes
within a minute.

---

### WEB-11. There is no CSRF protection

**Status: Done.** `reject_cross_site` runs before authentication and refuses
(403) anything whose `Sec-Fetch-Site` says cross-site, or whose `Origin`
(the fallback for a client old enough not to send `Sec-Fetch-Site`) is
`null` or does not match `allowed_origins` (built from the network scope and
port). Neither header present (curl, a script, a non-browser client) is let
through. This closes WEB-3's "anyone can lock out the owner" bug too: a
cross-site request is refused before it ever reaches the throttle. Not done:
enumerating this machine's actual LAN IP addresses in `allowed_origins`
(documented as a deliberate, disclosed gap, not an oversight — see the LAN
match arm's own comment).

**Medium · S · Verified (absence) / Confirm first (URL-credential case)**

**Files:** `src/web.rs:121-138` (router: allow-list and auth layers only),
`src/web.rs:220-245` (`authenticate`), `src/web/routes.rs:160-181`
(`POST /api/v1/backups/{id}/run`)

**Problem.** Nothing checks `Origin` or `Sec-Fetch-Site`, there are no
anti-CSRF tokens, and the one state-changing route takes no body, so a plain
HTML form on any website can target it. It isn't exploitable **today**, but
only because of three incidental properties, not a deliberate control:

- The only credential is the `Authorization` header. There are no cookies.
- 401 responses carry no `WWW-Authenticate: Basic` challenge, so browsers
  never prompt for or cache Basic credentials.
- There is no CORS layer, so a page on another site can't attach an
  `Authorization` header (that needs a preflight the server never approves).

That isn't good enough, for three reasons:

1. **Cross-site requests already do harm.** A forged request arrives without
   credentials, gets a 401, and counts toward the lockout, so any web page the
   user visits can lock the owner out repeatedly (see
   [WEB-3](#web-3-lockout-and-brute-force-protection)).
2. **The protection disappears with any of three ordinary changes:**
   - sending a Basic challenge (for a browser login prompt);
   - adding CORS (for a companion web app);
   - adding a cookie login (for the planned HTML interface).

   After any of them, a forged `POST …/run` starts a backup, and backups run
   the owner's hooks.
3. **Possible edge case (confirm first).** If a user once opens the API with
   credentials in the URL (`https://user:pass@host:8737/…`), some browsers may
   reuse them for later requests to that origin, possibly including a
   cross-site form POST.

**Fix.**

1. Add a `reject_cross_site` middleware in `web.rs`. Place it **between** the
   allow-list and authentication (layers run outermost-first, so it's added
   after `authenticate` and before `allow_list`):
   ```rust
   .layer(middleware::from_fn_with_state(auth, authenticate))
   .layer(middleware::from_fn_with_state(origins, reject_cross_site))
   .layer(middleware::from_fn_with_state(allowed, allow_list))
   ```
   This order means a rejected request never reaches the lockout counter.
2. Rules, applied to **every** method (reads leak file names; writes run
   hooks):
   - `Sec-Fetch-Site` present: allow `same-origin` and `none`; reject
     `cross-site` and `same-site` with **403**. `same-site` is rejected too,
     because another port on the same host counts as the same site.
   - No `Sec-Fetch-Site`, but `Origin` present: allow only if it exactly
     matches one of the server's own origins. That means `https://` plus
     `localhost`, `127.0.0.1`, `[::1]`, the machine's hostname or mDNS name,
     and (in LAN scope) its LAN addresses, on the configured port. Reject
     anything else with 403, including `Origin: null`.
   - Neither header: allow. Non-browser clients (curl, scripts, the documented
     API examples) send neither, so nothing existing breaks. Every current
     browser sends `Origin` on cross-site POSTs and `Sec-Fetch-Site` on
     everything.
3. Build the allowed-origin set once at startup from the scope and port, and
   rebuild it with the rest of the live config
   ([WEB-1](#web-1-credential-and-profile-changes-never-reach-the-running-daemon)).
   Compare with a parsed `url::Origin`, not string prefixes.
4. Log rejections with `debug_log!(WEB, …)` (method, path, peer address, the
   offending header). Don't use `error_log!`: that would let any web page
   flood the journal.
5. Make the incidental protections deliberate, each with a test:
   - **CORS stays off.** A preflight gets no `Access-Control-Allow-*` headers.
   - **The 401 challenge is `Bearer`, never `Basic`** (see
     [WEB-6](#web-6-response-hygiene-headers-status-codes-error-detail)).
   - **No cookies are ever set.**

   Add a comment at the router explaining that these properties are
   load-bearing for CSRF safety, and that anyone adding a cookie session must
   add CSRF tokens first ([WEB-12](#web-12-rules-for-routes-not-built-yet)).
6. **Out of scope here:** DNS rebinding makes a request look same-origin, but
   the browser still has no credentials for the attacker's domain, and TLS
   name checks fail. `Host`-header validation stays in WEB-12 for the HTML UI.
7. Docs: add a "Cross-site requests" paragraph to `docs/web-interface.md` and
   `SECURITY.md`. Browsers on other sites are refused; scripts and CLI tools
   are unaffected; the check is not a reason to relax the allow-list.

**Verify.** `oneshot` tests with `MockConnectInfo`:

| Request | Expected |
|---|---|
| Valid token + `Sec-Fetch-Site: cross-site` | 403, even with good credentials (the point of the check) |
| Valid token + `Sec-Fetch-Site: same-site` | 403 |
| Valid token + `Sec-Fetch-Site: same-origin`, and separately `none` | 200 |
| Valid token + `Origin: https://evil.example`, no `Sec-Fetch-Site` | 403 |
| Valid token + `Origin: null` | 403 |
| Valid token + `Origin: https://127.0.0.1:<port>` | 200 |
| Valid token, neither header | 200 |
| 10 × cross-site without credentials, then a correct password | 200 (no lockout) |
| `OPTIONS` preflight from another origin | no `Access-Control-Allow-*` headers |
| Any 401 | `WWW-Authenticate: Bearer …`, never `Basic` |
| `POST …/run` rejected cross-site | no History entry recorded |

Keep one real-socket test through the production `axum_server` path.

Live, before and after (paste both into the PR):

```sh
BASE=https://127.0.0.1:8737/api/v1
# cross-site with a valid token: must be 403, and no backup may start
curl -sk -o /dev/null -w '%{http_code}\n' -X POST -H "Authorization: Bearer $TOKEN" \
  -H 'Sec-Fetch-Site: cross-site' "$BASE/backups/$ID/run"
# foreign Origin, no Sec-Fetch-Site: 403
curl -sk -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" \
  -H 'Origin: https://evil.example' "$BASE/health"
# plain script request still works: 200
curl -sk -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $TOKEN" "$BASE/health"
# cross-site requests do not feed the lockout: last line must be 200
for i in 1 2 3 4 5 6; do curl -sk -o /dev/null -H 'Sec-Fetch-Site: cross-site' "$BASE/health"; done
curl -sk -o /dev/null -w '%{http_code}\n' -u x:"$PW" "$BASE/health"
# CORS stays off: must print 0
curl -sk -X OPTIONS -D - -o /dev/null -H 'Origin: https://evil.example' \
  -H 'Access-Control-Request-Method: POST' -H 'Access-Control-Request-Headers: authorization' \
  "$BASE/backups/$ID/run" | grep -ci '^access-control-allow'
```

**Browser check.** Serve this page from another origin
(`python3 -m http.server --bind 127.0.0.1 8000`):

```html
<form method="post" action="https://127.0.0.1:8737/api/v1/backups/ID/run">
  <button>go</button>
</form>
```

Submit it in both Firefox and Chrome. The request must get 403, visible in the
developer tools' Network tab, and History must show no new run.

Repeat after first visiting `https://x:PASSWORD@127.0.0.1:8737/api/v1/health`
in the same browser. That settles the "confirm first" edge case. Record
whether it was exploitable **before** the fix; if it was, it gets a SECURITY.md
advisory and a CHANGELOG entry under "Security".

---

### WEB-12. Rules for routes not built yet

**Info · Process**

Before adding the planned download, search, restore or delete routes:

- **Downloads.**
  - Build `Content-Disposition` per RFC 6266 with
    `filename*=UTF-8''<percent-encoded>`.
  - Strip CR, LF, `"` and `\` from the ASCII fallback.
  - Stream the body; never buffer a whole file (see
    [REL-15](#rel-15-large-files-are-loaded-whole-into-memory)).
- **Search.** Mandatory pagination, a result cap, a timeout, and the WEB-9
  semaphore.
- **Any write.**
  - Token scopes (read vs. write).
  - Require `Content-Type: application/json` on routes that take a body.
  - Mount them behind [WEB-11](#web-11-there-is-no-csrf-protection)'s
    cross-site check. It must cover every route; add each new route to its
    tests.
  - Keep CORS off.
- **An HTML UI.**
  - Cookie sessions with `SameSite=Strict; Secure; HttpOnly`.
  - CSRF tokens.
  - `Host`-header validation against DNS rebinding.
  - A strict CSP.
- **Tokens.** Add a recognizable prefix (`ssk_…`) so secret scanners catch
  leaks.

---

## Correctness and reliability (REL)

### REL-1. Retention rules from one backup prune another backup's snapshots

**Status: Done.** Every snapshot now gets `stellarshot-profile:<id>`
(`BackupRequest.profile_tag`, applied via `SnapshotOptions::add_tags` in
`Repo::backup`). `Repo::forget` takes `tag` and `sources`, keeping a
snapshot only if it carries this profile's tag or, for one made before
tagging existed, has no tag at all and its recorded paths exactly match
this profile's canonicalized sources. Threaded through `Job` for
`Operation::Maintain` (scheduled.rs and app.rs's Clean Up Now). Two new
engine tests cover cross-profile isolation and the untagged-migration rule.
README documents the tag for CLI users.

**High · M · Verified**

**Files:** `src/engine/maintenance.rs:88-110`, `src/engine/backup.rs:175-194`,
`src/scheduled.rs:157-170`, `src/app/wizard/mod.rs:588` (`Mode::Open`)

**Problem.** `forget` selects snapshots by `hostname == host` only, then
groups them by host + label + paths and applies **this profile's** keep rules
to **every** group. Snapshots carry no tag or label identifying their
profile. Two profiles on one computer sharing a repository is a supported
setup: the wizard's "open existing" mode allows it. So profile A (Smart,
7/4/12) prunes profile B's "Keep forever" history on A's next scheduled
clean-up, and B's rules likewise apply to A.

**Fix.**

1. Tag every new snapshot `stellarshot-profile:<id>` (`SnapshotOptions`
   tags). Pass the profile ID through `Job`/`BackupRequest`.
2. In `forget`, select `hostname == host && tags contain this profile's tag`.
3. Migration for untagged snapshots: treat an untagged snapshot as this
   profile's only if its `paths` equal this profile's canonical sources.
   Otherwise leave it alone. Never prune a snapshot nobody provably owns.
4. Document the tag in the README (visible in `rustic snapshots`), because
   users of the CLI will see it.

**Verify.** Engine test: back up source X under profile A and source Y under
profile B into one repository, three times each. Call `forget` with
`KeepRules { last: Some(1) }` for A. Assert B still has three snapshots. A
second test covers the untagged-migration rule.

---

### REL-2. After hooks are skipped when the repository fails to open

**Status: Done.** `job.request` is validated before `Before` hooks run;
`AfterHookGuard` guarantees `After` hooks fire on every remaining early
return (wrong password, unreachable destination) via `Drop`, and the success
path disarms it after calling `run_after` itself. Not done: the "open the
repository before Before hooks, so an unreachable destination never stops
the service at all" enhancement mentioned as a "consider" — the mandatory
fix (After always runs) is in; that further change would alter what a
`Before` hook sees on an unreachable destination and needs its own review.

**High · S · Verified**

**Files:** `src/runner.rs:260-285`, `tests/runner.rs:466-484`

**Problem.** `run_before` runs, then `engine::open(...)?` and
`job.request.ok_or_else(...)?` can return early, and `run_after` never runs.
The documented use is stop a database before, start it after
(`profile.rs:279-282`). A wrong password, an offline NAS or a missing rclone
leaves the service **stopped**. `DestinationUnavailable` is a quiet failure
for scheduled runs, so nobody is told. The existing test
`an_after_success_hook_does_not_run_after_a_failed_backup` passes either way
and doesn't catch this.

**Fix.** Validate `job.request` **before** Before hooks run. Once `run_before`
has succeeded, guarantee `run_after(&job.hooks, success)` on every path. A
small guard struct works: its `Drop` runs After hooks with `success = false`,
and it is disarmed on the success path after running them explicitly. Also
consider opening the repository (read-only) **before** running Before hooks,
so an unreachable destination never stops the service at all.

**Verify.** Runner test: a Before hook `touch before`, an After hook
`touch after`, and a job with a wrong password (and a second case with a
missing destination). Assert both markers exist and the result is an error.

---

### REL-3. The backup child can deadlock on a full stderr pipe

**Status: Done.** `drive()` now spawns a concurrent task
(`drain_stderr_tail`) that reads the child's stderr continuously from the
moment it is spawned, keeping only the last `CHILD_STDERR_TAIL` (16 KiB);
what reaches `EngineError.detail` and the log is capped further to
`CHILD_STDERR_DETAIL` (4 KiB). Regression test with 2,000 unreadable
subdirectories, inside a 60s timeout, added to `tests/child.rs` and passes.

**High · S · Verified**

**Files:** `src/app/child.rs:117-202`, `src/app/settings.rs:100-120`

**Problem.** The `--run` child's stderr is piped, but the parent reads **only
stdout** while the child runs, and stderr after it exits. The child's tracing
writes to stderr at `rustic_core=warn` and `rustic_backend=info`. That means
one line per unreadable file, per ownership failure on restore, and per line
of rclone stderr. After about 64 KiB (roughly 300 unreadable entries), the
child blocks in `write(2)` **while holding the repository lock**. The window
shows "running" forever. A home folder with a few root-owned directories is
enough.

**Fix.** In `drive()`, read stderr concurrently on its own task, keeping a
bounded tail (the last 16 KiB, in `constants.rs`) for diagnostics and
discarding the rest. Also cap how much of it goes into `EngineError.detail`
and the event log (4 KiB).

**Verify.** Test in `tests/child.rs`: a source with 2,000 `chmod 000`
subdirectories, run through `run_with(exe(), Backup, job)` inside a 60-second
`tokio::time::timeout`. Assert `Done` arrives. It hangs on current code.

---

### REL-4. Hooks and password commands can hang past their timeout

**Status: Done (fixed directly in each call site, not yet consolidated
into ARC-2's shared helper).** Both `hooks::run_command` and
`password_command::run` now spawn with `process_group(0)` and kill the
whole group (via `rustix::process::kill_process_group`), not just the
direct child, on timeout. `hooks::wait_with_timeout` reads stderr on a
channel with a bounded `DRAIN_AFTER_EXIT` wait after the direct child
exits, killing the group if a backgrounded grandchild still holds the pipe
past that; also switched to `read_to_end` + `from_utf8_lossy` so invalid
UTF-8 no longer discards all of stderr. Regression tests added for both
modules (a backgrounded talkative child, and killing a backgrounded
grandchild on timeout). ARC-2's actual consolidation into one shared
process helper is still open.

**High · S · Verified**

**Files:** `src/hooks.rs:109-135,214-262`, `src/password_command.rs:45-60`

**Problem.**

- **Background grandchildren hang the backup.** After `sh` exits,
  `stderr_thread.join()` waits until *every* holder of the pipe closes it. A
  hook like `sh -c 'mydaemon &'` returns at once, but its background process
  keeps stderr open. The backup then blocks forever holding the lock, and
  every later scheduled run is quietly skipped as `Locked`. `HOOK_TIMEOUT`
  never applies.
- **Timeouts kill only the direct child.** On timeout, `child.kill()` kills
  only `sh`, so its children keep running. Same for `password_command`
  (`kill_on_drop`).
- **Stderr is lost.** `read_to_string` discards all of stderr if any byte is
  invalid UTF-8.

**Fix.** Spawn with `CommandExt::process_group(0)`. On timeout, call
`rustix::process::kill_process_group` (the same pattern `app/child.rs`
already uses). Read stderr into a channel and, after the child exits, wait at
most `DRAIN_AFTER_EXIT` (already in `constants.rs`) for it. Use
`read_to_end` plus `String::from_utf8_lossy`. Implement this once in the
shared process helper ([ARC-2](#arc-2-remove-duplicated-logic)).

**Verify.**

- `run_one` on `sh -c 'sleep 300 & exit 0'` returns within about 3 seconds.
- `wait_with_timeout` on `sh -c 'sleep 300; true'` with a 200 ms limit; then
  `pgrep -f 'sleep 300'` finds nothing.
- The same two tests for `password_command`.

---

### REL-5. Excluded paths are not escaped for glob matching

**Status: Done.** Added `literal_glob` (backslash-escapes
`\ * ? [ ] { } !`, fails closed on non-UTF-8), used it in `globs()`, and
fixed the British spellings in the doc comment above it. Three engine tests
added and passing: brackets/wildcards in an excluded name, a non-UTF-8
exclude failing the backup, and a bracket-containing repository-inside-a-
source path.

**High · S · Verified**

**Files:** `src/engine/backup.rs:74-91`, `src/profile.rs:480-489`

**Problem.** Excluded folders become `format!("!{}", path.display())`, a
gitignore-style glob.

- A folder named `Photos [RAW]` becomes a character class and never matches.
  The same happens with `*`, `?`, `{`, `}`.
- `display()` replaces non-UTF-8 bytes with U+FFFD, so that glob never
  matches either.

In both cases the folder is **silently backed up**. For the automatic
"repository inside a source" exclude, **the repository backs up into
itself**.

**Fix.** Add `fn literal_glob(path: &Path) -> Result<String, EngineError>`.
It returns an error for non-UTF-8 paths (fail closed, as
`pattern_file_lines` does) and backslash-escapes `\ * ? [ ] { } !`. Use it in
`globs()`. Fix the British spellings in that doc comment while you are there
(`canonicalised`, `canonicalises`).

**Verify.** Engine tests:

- Exclude `source/"a [b]"` and `source/"x*y"`; neither is in the snapshot.
- Exclude a non-UTF-8 folder (`OsStr::from_bytes(b"\xff")`); the backup
  fails rather than including it.
- A repository inside a source folder whose path contains `[` is not in the
  snapshot.

---

### REL-6. Non-UTF-8 file names cannot be browsed, mounted or restored singly

**Status: Done.** Added `engine::browse::node_at` (walks `snapshot.tree`
comparing each `Path::Component::Normal` as `OsStr`, mirroring rustic's own
internal, non-public `Tree::node_from_path`), replacing every
`node_from_snapshot_and_path(&path.to_string_lossy())` call site in
browse.rs and restore.rs. `MountEntry.name` and `diff_trees`'s internal map
are now `OsString`-keyed, not lossy `String`, fixing FUSE `readdir`/`lookup`
mismatches and a real collision bug (two differently-invalid names
collapsing to one `BTreeMap` entry). `keep_both_name` and
`drives::unescape` rebuilt on `OsString` so a restored copy's renamed name,
and a drive's mount point, keep their exact bytes.

Restoring a **single** non-UTF-8-named item (not as part of a whole-
snapshot restore) hit a second, independent problem: rustic_core's
`LocalDestination::new` takes a `&str` root, and `restore_one` had been
rooting it at the exact destination path — which, for a lone non-UTF-8 leaf,
is not valid UTF-8 either. Fixed by rooting at the nearest UTF-8-valid
ancestor instead (always `/` at worst) and re-deriving each item's relative
path from there — required working out, and getting wrong twice before a
regression test caught it, how rustic's own `NodeStreamer` shapes relative
paths differently for a directory (children/descendants, excluding the
starting node's own name) versus a single file (one item whose relative
path already *is* the node's original name, redundant and wrong after a
Keep Both rename).

Added a non-UTF-8 file to the shared `awkward_tree` fixture (exercised by
~20 existing tests for free) plus dedicated tests for list/mount/mounted
readdir+read, single-file restore, Keep Both, and the diff collision case.
All pass, including the full existing `engine::tests` suite (70/70).

**High · M · Verified**

**Files:** `src/engine/browse.rs` (lines 142, 220, 245, 254, 271, 290, 310,
441, 485, 529), `src/engine/restore.rs:98,102,206`,
`src/engine/mount.rs:280`, `src/drives.rs:160`

**Problem.** Linux file names are bytes. The engine converts paths with
`to_string_lossy()` before looking them up
(`node_from_snapshot_and_path(.., &x.to_string_lossy())`), so a name that
isn't valid UTF-8 never matches the stored name. Browse, the mount, search,
versions, single-file restore and "keep both" renames fail with a misleading
"not in this snapshot" or ENOENT. `diff_trees` keys a map by the lossy
`String`, so two distinct names can collide and one disappears from Compare.
Whole-folder restore still works. For backup software, "I can see it but
can't get it back" is a serious defect.

**Fix.**

1. Add one helper:
   `fn node_at(repo, snapshot: &SnapshotFile, path: &Path) -> Result<Node, EngineError>`.
   It walks `path.components()` from `snapshot.tree` with `repo.get_tree` and
   compares `node.name()` as `OsStr`. Replace all call sites.
2. Keep names as `OsString` in `MountEntry`, the diff map, and
   `keep_both_name`. Convert to display strings only at the UI edge.
3. Use `OsString::from_vec` in `drives::unescape`.

**Verify.** Add a file named `b"caf\xe9.txt"` to `awkward_tree` in
`engine/tests.rs`. Assert that `list`, `mount_stat`, a mounted `read_dir` and
`read`, a single-path restore, KeepBoth, and `diff_trees` all handle it.

---

### REL-7. A panicking upload worker deadlocks the backup

**Medium · S · Verified**

**Files:** `src/engine/uploads.rs:107-167,269-280`

**Problem.** If `inner.write_bytes` panics, the worker unwinds before
`in_flight -= 1`. The mutex isn't poisoned, so `settle()` and `Drop` wait on
the condvar forever.

**Fix.** A drop guard in `work` that decrements `in_flight` and calls
`notify_all`. Wrap the write in `catch_unwind` and record the panic as the
upload's `failure`.

**Verify.** A test backend whose pack write panics. `write_bytes` for a pack
and then an index, on a thread, yields `Err` within `recv_timeout(2s)`.

---

### REL-8. Status polling can make a real backup fail with "Locked"

**Status: Done — structurally, not just probabilistically.** A first pass
retried `try_lock` a few times before reporting `Locked`, but the plan's
own stress test (`is_running_in` in a tight loop racing 1,000 `acquire_in`
calls) still showed occasional false `Locked` errors — a retry can only
ever make the race less likely, since `std::fs::File::try_lock` (`flock()`)
has no way to query a lock without taking it. Switched the whole write
lock to an open-file-description lock (`fcntl(F_OFD_SETLK)`/`F_OFD_GETLK`,
added `libc` as a direct dependency for the raw call — rustix exposes only
`flock()` and classic per-process `fcntl` record locks, neither queryable
without acquiring). `is_running_in` now uses `F_OFD_GETLK`, which reports a
conflicting lock without ever taking one, so the race is gone by
construction rather than made merely unlikely. The stress test passes
reliably in isolation (5/5 runs) and every time the whole `engine::lock`
module runs alone; see the note below on a sandbox-specific interaction
with unrelated tests that fork heavily.

**Medium · S · Verified**

**Follow-up note on test stability:** run concurrently with
`hooks`/`password_command`'s tests, which fork and kill many real child
processes, this test and two pre-existing lock tests occasionally see a
genuine `EAGAIN` from `F_OFD_SETLK` with nothing else provably holding a
conflicting lock. Isolating the module (even with 8 test threads) never
reproduces it, which points at interference from the concurrently-forking
tests rather than a bug in the OFD lock code itself — the same category of
sandbox-specific flakiness already disclosed for `tests/rclone.rs` under
this session's 3-core cgroup. Not chased further here; worth a real look
if it recurs outside this sandbox.

**Files:** `src/engine/lock.rs:60-80,263-273`, `src/status.rs:32-35`,
`src/app/applet.rs:35,97`, `src/web/routes.rs:110`

**Problem.** `is_running()` takes the exclusive `flock` itself, briefly. The
applet polls every 3 seconds and the web API polls on each request. A backup
starting at that instant gets `Locked`. For a scheduled run that is a
**quiet skip**. The existing test re-implements the probe instead of testing
`is_running`.

**Fix.** Probe without acquiring: `fcntl(F_OFD_GETLK)` through `rustix`, or
retry `try_lock` in `acquire_in` 5 × 50 ms (constants) before reporting
`Locked`. Make `is_running` take a directory so the test exercises the
production function.

**Verify.** A thread calls `is_running_in` in a tight loop while the main
thread calls `acquire_in` and drops it 1,000 times; assert zero `Locked`
errors.

---

### REL-9. The lock key is not canonical

**Status: Done.** `Location::key()` canonicalizes local paths (falling
back to the raw path if that fails) and redacts a REST URL's credentials
before hashing. `Location::legacy_key()` recomputes the pre-canonicalization
key and, when it differs, `WriteLock` also takes that lock file, so an
older Stellarshot still running during an upgrade sees the same write as
locked; dropped once both files' guards go out of scope. New tests cover a
trailing slash and a symlink sharing one key, a REST URL's legacy key
dropping credentials, and an `Rclone` location never needing one. The
existing `lock_keys_are_stable_across_versions` test still passes unchanged
(its path does not exist, so canonicalization is a no-op there).

**Medium · S · Verified**

**Files:** `src/engine/repo.rs:120-132`

**Problem.** The key hashes raw path bytes, so `/backups/home`,
`/backups/home/` and a symlinked alias get different locks for one
repository. rustic takes no lock of its own, so prune can run alongside a
backup. For REST, the key includes the credentials.

**Fix.** Canonicalize local paths (fall back to the raw path if that fails),
and drop userinfo from REST URLs before hashing. `lock_keys_are_stable_across_versions`
protects older binaries still running during an upgrade, so for one release
take **both** the old and new keys, then drop the old one. Record that
decision in the CHANGELOG.

**Verify.** `acquire_in(dir, local("/x/repo"))`, then
`acquire_in(dir, local("/x/repo/"))`, returns `Locked`.

---

### REL-10. Unreadable state is replaced with defaults and saved over

**Status: Core fix done; the broader items are not.** `run_state::load`
and `event_log::load`/`record`/`merge` now go through a `load_checked`
that distinguishes `cosmic_config::Error::NotFound` (genuinely absent, a
safe default) from any other error (present but unreadable) via a new
pure, tested `run_state::is_missing`. On the unreadable path: `error_log!`
instead of silence, and `update`/`record`/`merge` do nothing rather than
saving a fresh value over data they could not parse; `run_state::load`
additionally marks the fallback `damaged: true`, so automatic pruning
stays paused. Not done: `#[serde(other)] Unknown` fallback variants (so an
*older* binary can still parse a *newer* one's write), the `<key>.unreadable`
side-copy, and the cross-process `flock` on `runtime_dir()/state-<id>.lock`
to serialize writers. A store-touching integration test for the full
load/record/merge path was deliberately not added, matching this file's
own established practice of never letting a test touch the real config
store; the safety-critical boundary (`is_missing`) is unit-tested directly
instead.

**Medium · M · Verified**

**Files:** `src/run_state.rs:192-220`, `src/event_log.rs:110-189`

**Problem.**

- **Parse errors destroy state.** `store.get(..).ok()` / `unwrap_or_default()`
  treats "cannot parse" like "absent", and `record`, `merge` and `update`
  then **save over** the value. The whole event log becomes one entry, and
  `damaged` resets to `false`, which lets automatic prune resume on a damaged
  repository.
- **This happens across upgrades.** This very change adds
  `ErrorKind::AppUpdated`. An older window still running reads a variant it
  doesn't know, falls back to the default, and overwrites the log.
- **Lost updates.** The window and `--scheduled` both load-modify-save with no
  mutual exclusion, so one writer's update can be lost.

**Fix.**

- `load` returns `Result<Option<T>, StateError>`. On a parse error:
  `error_log!`, **do not write**, and optionally copy the raw value aside
  (`<key>.unreadable`).
- Treat unknown state as unsafe: do not prune.
- Add `#[serde(other)] Unknown` fallback variants to the persisted enums
  where serde supports it, or a lenient custom `Deserialize`.
- Serialize writers with an `flock` on `runtime_dir()/state-<id>.lock`.

**Verify.** Unit test: RON containing an unknown `ErrorKind` returns an error,
not the default, and `record` against it leaves the stored value unchanged
(use a temp `XDG_STATE_HOME`, as `tests/scheduled.rs` does).

---

### REL-11. One policy for a binary replaced while running

**Medium · M · Verified (current behavior) / Confirm first (`/proc/self/exe`)**

**Files:** `src/app/child.rs:100-116`, `src/schedule.rs:147-154`,
`src/app/applet.rs:58-66`, `src/app.rs:2875-2890`

**Problem.** Four behaviors for one situation:

- `child::run` refuses to spawn and returns `AppUpdated`.
- `schedule::executable` strips ` (deleted)` and uses the new binary.
- The applet and "New window" call raw `current_exe()`, which returns ENOENT
  after an upgrade.

The ` (deleted)` literal is duplicated and compared through
`to_string_lossy`. Handling its own upgrade gracefully is a stated project
requirement.

**Fix.** Create `src/exe.rs` with two functions:

- `running_image() -> PathBuf`, returning `/proc/self/exe`, for `--run`
  children. Executing that magic link works even after the file is unlinked,
  and it runs the **same** binary as the window, so the stdin/stdout JSON
  protocol always matches. `AppUpdated` is then needed only if that exec
  fails. **Confirm first** with a test that copies the binary, starts it,
  deletes it, and runs a backup.
- `installed_path() -> PathBuf`, the existing stripping logic, for new
  launches: units, notification clicks, the applet's Open button, and "New
  window". Compare as `OsStr`, not lossy strings.

**Verify.** Extend `tests/child.rs`. Manually: copy the release binary to
`/tmp/s` and start it; `rm /tmp/s && cp new /tmp/s`; Back Up Now succeeds and
the applet's Open launches the window.

---

### REL-12. Scheduled runs take the lock and open the repository four times

**Medium · M · Verified**

**Files:** `src/scheduled.rs:121-205,290`

**Problem.** Backup, forget, check and prune each call `runner::run`, which
takes and releases the lock and runs `engine::open` (reading the index,
spawning `rclone version`). A window action can slip in between them. A
`Locked` during clean-up is reported as a **failure** with a notification,
because `is_quiet` only applies to the backup stage.

**Fix.** Add `runner::run_plan(&Job, &[Operation])`, which takes the lock and
opens the repository once for the whole plan. At minimum, treat `Locked` as
quiet in every stage.

**Verify.** A test holds the lock between stages (through a test seam) and
asserts no failure is recorded. With debug logging on, one scheduled run
shows one "holds the lock" line.

---

### REL-13. The exclusion breakdown leaves some exclusions in place

**Medium · S · Verified**

**Files:** `src/engine/estimate.rs:100-115`

**Problem.** The "everything" baseline clears only `excludes` and
`exclude_patterns`. Case-insensitive patterns, pattern files,
`exclude_larger_than`, `exclude_caches` and `git_ignore` stay active, so the
"nothing excluded" size is wrong and "by patterns" under-reports.

**Fix.** Build the baseline from
`BackupRequest { sources, one_file_system, ..Default::default() }`.

**Verify.** Extend `the_arithmetic_adds_up_to_the_estimate` with
`exclude_larger_than` and `exclude_caches`; `included` still equals
`everything`.

---

### REL-14. "Keep both" can overwrite a file whose mtime differs by under a second

**Medium · S · Confirm first**

**Files:** `src/engine/restore.rs:110-140,240-275`

**Problem.** The "already identical" check compares mtime to the second. A
file with the same size and second but a different nanosecond is passed
through unchanged. If rustic compares at full precision, it rewrites the
user's file, breaking the "never touches the existing file" promise.

**Fix.** Compare full-precision timestamps. Or, under KeepBoth, rename any
existing file whose content hasn't been verified equal.

**Verify.** With `filetime` (already a dev-dependency), set the file's mtime
to the original second + 500 ms at the same size. Restore with KeepBoth and
assert the original content is unchanged.

---

### REL-15. Large files are loaded whole into memory

**Medium · M · Verified**

**Files:** `src/engine/mount.rs:290-330` (`open` stores the whole file in
`open_files`), `src/engine/browse.rs:294-349` (`dump_file`,
`archive_folder`)

**Problem.**

- **Mount.** Opening a file in a mounted snapshot reads the **entire file**
  into a `Vec` and keeps it until release. Opening a 20 GB disk image from the
  mount tries to allocate 20 GB.
- **Archive and dump.** `archive_folder` loads each file fully before writing
  it to the tar. A failure partway leaves a truncated `.tar.gz` or dump at the
  destination. Directories get mode `0o644` when their mode is missing, so
  they are not traversable once extracted.

**Fix.**

- **Mount:** serve `read(offset, size)` by locating the blobs covering the
  range (rustic's `OpenFile::read_at`) and keep only a small per-handle cache.
  Set `MountOption::AutoUnmount` so a crashed window doesn't leave a dead
  mount. Replace `lock().unwrap()` with poison-tolerant locking.
- **Archive and dump:** write to `<dest>.part`, rename on success, delete on
  error. Stream each file into the tar using `node.meta.size` as the header
  size. Default directories to `0o755`.

**Verify.** Mount a snapshot containing a 2 GB file, then
`dd if=<mount>/big of=/dev/null bs=1M count=10` while watching RSS; it stays
flat. Archive with a failing backend; no destination file is left behind.
Measure peak memory archiving a 1 GB file with `/usr/bin/time -v`.

---

### REL-16. Keyring failures are silent

**Medium · S · Verified**

**Files:** `src/app/tasks.rs:93,246` (`let _ = keyring::store(...)`),
`src/keyring.rs:36-117`

**Problem.** When "Remember password" is ticked and the save fails, nothing
says so, and every scheduled run later fails with `PasswordNotRemembered`.
`load` and `load_web_password` swallow every error with `.ok()?` and log
nothing, so the cause cannot be diagnosed. The three profile functions and the
three web-password functions are near-duplicates.

**Fix.** Return the store error in the `Finished` and `Unlocked` messages and
show it through the existing `KeyringUpdateFailed` dialog. Write
`store_item`, `load_item` and `forget_item` once, taking attributes and a
label. `debug_log!(KEYRING, …)` at every failure point, never logging the
secret.

**Verify.** Stop the Secret Service (`pkill gnome-keyring-daemon`), create a
backup with "remember" on, and an error dialog appears. `tests/keyring.rs`
still passes.

---

### REL-17. Smaller correctness items

**Low · S each · Verified unless noted**

| Item | Where | Fix |
|---|---|---|
| Two ways to resolve a snapshot ID; an ambiguous prefix is reported as "not in this snapshot" | `browse.rs:205-214` vs `restore.rs:179` | Use rustic's `get_snapshot_from_str` everywhere, or add `NotFound` and `Ambiguous` error kinds |
| `set_pinned` finds the new ID by diffing snapshot lists; a concurrent writer or an empty diff silently returns the old one | `snapshots.rs:112-134` | Use the ID rustic returns from the save, or log and error when the diff isn't exactly one |
| SFTP `known_hosts` path becomes relative when `HOME` is unset | `profile.rs:121-124` | Fail instead (via the paths module, [ARC-2](#arc-2-remove-duplicated-logic)) |
| Only Wi-Fi counts as a "trusted network"; a wired desktop never runs (**confirm intent**) | `conditions.rs:177` | Accept Ethernet connection IDs too, or document it |
| `overdue_notified` is recorded before the notification is sent | `scheduled.rs:86` | Record after a successful send |
| `notify::open_profile` calls blocking `Command::status()` inside an async fn | `notify.rs:111` | `tokio::process` or `spawn_blocking` |
| `main` panics on a non-UTF-8 argument (`std::env::args`) | `main.rs:8` | `args_os()`; convert only the flags |
| v1 migration failure is discarded silently | `profile.rs:532` | `error_log!` |
| An unreadable `/proc/self/mountinfo` makes every drive look unplugged | `drives.rs:28` | `error_log!` and surface "cannot read drives" |
| `mount.rs` maps every engine error to ENOENT or EIO with no log | `mount.rs:225,236,269,307` | `debug_log!(MOUNT, …)` with the error |
| Failed history writes go only to `debug_log` | `event_log.rs:167,186` | `error_log!` |
| `run_state` and `event_log` have no `remove`; removing a backup saves a default state instead of deleting its keys | `app.rs:938` | Add `remove(id)` and call it on profile removal |
| The restore preview opens the repository once per request (N index downloads on cloud storage) | `app.rs:1237-1249` | Open once before the loop, or reuse the open `Browser` |

---

## GUI, COSMIC conventions and accessibility (UI)

### UI-1. The applet's Open button launches another applet

**Status: Done.** Added `src/exe.rs` with `installed_path()` (pulled
forward from REL-11, which will extend it later). The applet's Open button
and app.rs's "New window" both derive the target binary from it and launch
through `cosmic::process::spawn` (double-fork, no zombie) rather than a
bare `Command::spawn`; `schedule::executable()` now delegates to the same
function instead of duplicating the stripping logic. Confirmed
`/usr/bin/stellarshot` and `/usr/bin/stellarshot-applet` are installed as
siblings on this machine, matching what `with_file_name` assumes. Not done:
the live click-through in a real panel session (disclosed gap, consistent
with this project's practice of not touching Dave's running session).

**High · S · Verified**

**Files:** `src/app/applet.rs:58-66`

**Problem.** `open_window()` spawns `env::current_exe()`, which inside the
applet is `stellarshot-applet`. The spawned child is never waited on, so it
stays a zombie.

**Fix.** Resolve the window binary with `exe::installed_path()` from
[REL-11](#rel-11-one-policy-for-a-binary-replaced-while-running), with its
file name replaced by `stellarshot`. Launch through
`cosmic::process::spawn`, which double-forks so no zombie is left. Use the
same for "New window" (`app.rs:2882`).

**Verify.** Click Open in the panel applet. `pgrep -a stellarshot` shows the
window process and no second applet; `ps -o stat` shows no `Z`.

---

### UI-2. Launch flags are lost when an instance is running

**High · M · Verified**

**Files:** `src/main.rs:15-29`, `src/app.rs:285-293` (`CosmicFlags`),
`src/app.rs:2124-2143` (`dbus_activation` ignores `_msg`)

**Problem.** `run_single_instance` forwards only what `CosmicFlags::action()`
and `args()` return, and both are left at their defaults. Because closing the
window keeps the process alive, an instance is almost always running. So:

- the desktop actions "New Backup" and "Restore Files" just raise the window;
- a failure notification's `--profile <id>` never selects the failing backup.

The README documents all three as working.

**Fix.** Give `Flags` a `Launch` enum: `NewBackup`, `Restore`,
`Profile(id)`. Implement `action()` and `args()`. In `dbus_activation`, match
`Details::ActivateAction { action, args }` and run the same logic `init` uses,
then focus or open the window.

**Verify.** With the window closed to the panel: `stellarshot --new-backup`
opens the wizard, and `stellarshot --profile <id>` selects that backup. Add a
unit test for `Flags` → `(action, args)` → `Launch` round-tripping.

---

### UI-3. There is no way to quit

**Status: Done.** Added `Action::Quit`/`Message::Quit` (Ctrl+Q), renamed
the old "Quit" menu item to "Close Window" (new `menu-close-window` key in
all five locales) and gave it to the real `Action::Quit`. Confirms via a new
`Dialog::Quit` when any page's `ProfileState::is_busy()` is true; otherwise
calls `cosmic::iced::exit()` directly. Added the same Quit button to the
applet's popup (its own `cosmic::iced::exit()`, unrelated to any backup).
Updated the README shortcut table (added Ctrl+Q and the previously-missing
F1) and the panel-applet section. Not done: the live Ctrl+Q click-through
(disclosed gap, same as UI-1).

**High · S · Verified**

**Files:** `src/app/menu.rs:24`, `src/app/key_bind.rs:28`,
`src/app/settings.rs:68`, `src/app.rs:2867-2874`,
`i18n/*/stellarshot.ftl` (`quit`, `error-app-updated`)

**Problem.** The menu item is labeled **Quit** but maps to
`Action::WindowClose`. `exit_on_close(false)` is set and nothing ever calls
`iced::exit()`. The `error-app-updated` message and the CHANGELOG tell users
to "quit it completely", which is impossible without `pkill` or logging out.

**Fix.**

- Rename the menu item to "Close Window" (Ctrl+W), with a new key in all
  locales.
- Add `Action::Quit` (Ctrl+Q) returning `cosmic::iced::exit()`, confirming
  first when an operation is in progress. `--run` children survive anyway, by
  design.
- Add Quit to the applet popup.
- Update the README shortcut table (also add F1, which is missing).

**Verify.** Ctrl+Q, then `pgrep -x stellarshot` prints nothing.

---

### UI-4. Deleting a snapshot has no confirmation

**Status: Done.** `Message::DeleteSnapshot` now only emits
`Effect::ConfirmDeleteSnapshot { id, label }` (also gated on `!is_busy()`,
matching the button's own disabled state); the actual delete moved to a new
`delete_snapshot_confirmed` method, called only from the new
`Dialog::DeleteSnapshot`'s Confirm. New locale keys
(`delete-snapshot-title`/`-body`) in all five locales. Updated the two
existing unit tests that relied on the old direct-delete path, and added a
new one asserting the confirm step touches nothing until confirmed.

**High · S · Verified**

**Files:** `src/app/pages/profile.rs:450-458,870-880`

**Problem.** The trash icon sits next to the Pin toggle and deletes on one
click. Every other destructive action confirms, and "Delete all" even
requires typing the name.

**Fix.** Add `Dialog::DeleteSnapshot { id, snapshot, label }` with a
destructive primary button and new locale keys (`delete-snapshot-title`, and
`delete-snapshot-body` with `{ $time }`). `Message::DeleteSnapshot` returns
`Effect::ConfirmDeleteSnapshot`; only `DialogMessage::Confirm` emits
`Effect::DeleteSnapshots`.

**Verify.** Update the page's unit tests for the confirm step. Manually: click
the trash icon, Cancel, and the list is unchanged.

---

### UI-5. The sidebar is rebuilt on every profile message

**Medium · S · Verified**

**Files:** `src/app.rs:751-828` (`rebuild_nav`), `2500`, `863-874`
(`reload_runs`)

**Problem.** Every keystroke in the unlock field and every progress event
(every 250 ms during a backup) clears and re-inserts the whole nav model, with
new entity IDs each time. That drops keyboard focus in the sidebar and
rebuilds the accessibility tree. `reload_runs` reads every profile's state
file synchronously on the UI thread every 30 seconds.

**Fix.** Add `refresh_nav_row(&mut self, id)`, which updates text and icon via
`nav.text_set` and `nav.icon_set`. Call `rebuild_nav` only when the profile
list or the wizard state changes. Move `reload_runs` into `tasks::blocking`.

**Verify.** During a backup, Tab into the sidebar; focus stays put. A
`debug_log!` in `rebuild_nav` no longer fires on every tick.

---

### UI-6. The restore page does per-frame work and renders unbounded lists

**Medium · M · Verified**

**Files:** `src/app/pages/restore.rs:976-981,1182-1198`,
`src/app/wizard/browse.rs:166-167`

**Problem.** `group_diff` and the count folds run inside `view()`, which runs
after every message, including the 1-second tick. Browse and the disk tree
render every entry, so a 50,000-entry folder or diff costs 50,000 widgets per
frame.

**Fix.** Compute the grouped diff and the counts once, when `Compared`
arrives, and store them. Cap rows at `RESULT_LIMIT` with a "Show more"
button, or render through `widget::lazy`.

**Verify.** Compare two snapshots of a large tree; CPU while idle on the
Compare tab drops to near zero.

---

### UI-7. The applet polls every 3 seconds on its UI thread, forever

**Medium · S · Verified**

**Files:** `src/app/applet.rs:32-56,96-107`

**Problem.** The subscription runs unconditionally, although the doc comment
says "while its popup is open". `refresh()` runs synchronously in `update`. It
builds a new config handler, reads every key, reads the run-state files, and
takes a lock probe per profile (which also feeds
[REL-8](#rel-8-status-polling-can-make-a-real-backup-fail-with-locked)).

**Fix.** Use a cosmic-config watch subscription for config changes. Poll every
3 seconds only while the popup is open, and every 60 seconds otherwise
(constants). Run `status::all` via `Task::perform` + `spawn_blocking`.

**Verify.** With the popup closed,
`strace -f -e trace=openat -p $(pgrep stellarshot-applet)` is quiet for at
least 30 seconds.

---

### UI-8. Accessibility and keyboard focus

**Medium · M · Verified**

**Files:**

- Unlabeled checkboxes: `restore.rs:1040,1142,1246`, `wizard/browse.rs:311`.
- Tree toggle with no name: `wizard/browse.rs:280`.
- Header Settings button: `app.rs:2151`. It has a tooltip but no `.name()`;
  in libcosmic only `.name()` sets the accessible name.
- Inputs never focused: `app.rs:2308` (the Delete-all name field) and
  `profile.rs:826` (the unlock field).

**Problem.** Screen readers announce unlabeled checkboxes. Every list
"Remove" button reads the same. No dialog focuses its input, and only the
unlock field submits on Enter.

**Fix.**

- Give each input a `widget::Id` and return `text_input::focus(id)` when its
  dialog or card opens. Add `.on_submit(...)` to confirm.
- Name every icon button and checkbox. Use descriptive names:
  `fl!("remove-item", item = …)`, and new `expand` and `collapse` keys.

**Verify.** With Orca or `accerciser`: every control has a name, Tab order is
logical, and Enter confirms dialogs.

---

### UI-9. Generated API token cannot be copied; regeneration is unconfirmed

**Medium · S · Verified**

**Files:** `src/app.rs:2781-2788`

**Problem.** The token is shown once, in non-selectable dialog text, so the
user must retype 64 hex characters. The button invalidates the old token
before the dialog even opens, which breaks existing scripts without warning.

**Fix.** Add a Copy button (`cosmic::iced::clipboard::write`), or show the
token in a read-only `text_input`. Confirm before regenerating when a token
already exists.

**Verify.** Generate, Copy, paste into
`curl -H "Authorization: Bearer …"`, and get 200.

---

### UI-10. Allow-list entries and the port are not validated

**Medium · S · Verified**

**Files:** `src/app.rs:2791-2816`, `src/web.rs:205-215`

**Problem.** A typo (`192.168.1.0/33`, `nas`, `192.168.1.*`) is saved and
silently never matches. If it is the only entry, **every** client is locked
out with no explanation. An invalid port shows as `ErrorKind::Internal`
("Details: abc").

**Fix.** Expose one `web::parse_allow_entry(&str) -> Result<IpNet, …>` used
by both the UI and the daemon. Reject invalid entries with a localized
`web-allowed-invalid` error. Show `web-port-invalid` as a plain error dialog.

**Verify.** Unit test the parser; adding `abc` in Settings shows the error and
leaves the list unchanged.

---

### UI-11. Consistency and polish

**Low · S each · Verified**

| Item | Where | Fix |
|---|---|---|
| Fake tabs made from suggested and standard buttons | `restore.rs:1483-1493` | `widget::segmented_button::horizontal` with a `SingleSelectModel`, as cosmic-files and cosmic-settings do |
| Magic page widths (900, 960, 760, 460) and sizes (320, 220, 120, 24, 20) | `profile.rs:748`, `home.rs:102`, `restore.rs:924`, `wizard/mod.rs:1098,1624`, `empty.rs:25`, `wizard/browse.rs:189,289,327`, `app.rs:440`, `place.rs:531` | Named constants in `constants.rs` and theme spacing tokens. Give History the same max width. The 220 px hint clips German text |
| "Remove backup" styled `suggested`; "Unmount" styled `destructive` | `app.rs:2302`, `restore.rs:1004` | Remove = destructive; Unmount = standard |
| The applet uses `dialog-warning` for failed and overdue, which means *damaged* in the window's legend, and never shows damage | `applet.rs:175-201` | Reuse `BackupStatus::icon()` |
| The wizard-cancel dialog has no "keep editing" option | `app.rs:2313-2323` | Add a tertiary Cancel action |
| Changing the port, TLS or auth needs a restart the UI never mentions | Settings, web section | "Restart to apply" caption when settings differ from what the daemon started with (or fix [WEB-1](#web-1-credential-and-profile-changes-never-reach-the-running-daemon) with live reload) |
| Synchronous file I/O in `update` and `init` | `app.rs:1397,1598,1958` (`event_log::record`), `app.rs:2206` (`dejadup::find`) | Move into `tasks::blocking` |
| `gethostname` called in `view()` every frame | `app.rs:449,2087` | Cache it in `App` |

---

## i18n (I18N)

Locale parity is good: all five locales have the same 535 keys, with no
duplicates and matching placeholder sets, and every `en` key is used. These
tasks fix what the parity test cannot see.

### I18N-1. Fluent syntax error drops the "token shown once" warning

**Status: Done.** Indented the continuation line in all five locales.
Added `fluent-syntax` as a dev-dependency and a new
`no_locale_has_a_fluent_syntax_error` test in `tests/i18n.rs` that parses
every locale and fails on any junk entry.

**High · S · Verified**

**Files:** `i18n/en/stellarshot.ftl:585-587` and the matching lines (about
606-608) in `bg`, `de`, `gsw`, `sv`; `tests/i18n.rs`

**Problem.**

```ftl
web-token-body = { $token }

This is shown only once. Store it somewhere safe: …
```

A Fluent continuation line must be indented. The second paragraph is parsed
as a junk entry and dropped, so the dialog shows only the token, without the
warning, in every language. `tests/i18n.rs` uses a hand-written key scanner
and cannot see syntax errors.

**Fix.** Indent the continuation lines in all five files. Add
`fluent-syntax` as a dev-dependency and assert in `tests/i18n.rs` that every
locale parses with **no junk entries**. That test would have caught this.

**Verify.** The new test fails before the fix and passes after. Generating a
token shows both paragraphs.

---

### I18N-2. Plurals, joins, and reused keys

**Low · S · Verified**

- **Plurals.** `wizard-estimate` ("{ $files } files"), `event-cleaned-up`
  ("Forgot { $count } snapshots") and `show-all-snapshots` produce "1 files".
  Use `{ $count -> [one] … *[other] … }` in all five locales.
- **Joins.** `profile.rs:664-668,968` join translated fragments with
  `format!("{} · {}")`. Word order is language-specific, so make these Fluent
  messages with arguments.
- **Reused keys.** The TLS Choose and Reset buttons reuse
  `settings-cache-dir-choose` and `-reset` (`app.rs:537-539`), which gives
  translators the wrong context. Give them their own keys.
- **Number and date formatting.** `format.rs` always uses `.` as the decimal
  separator and ISO dates. That's a legitimate choice, but record it in
  CONTRIBUTING so it isn't "fixed" piecemeal.

**Verify.** Extend `tests/i18n.rs` to fail when a message containing
`{ $count }` or `{ $files }` has no selector.

---

### I18N-3. Localize the desktop entries and metainfo

**Low · M**

**Files:** `res/*.desktop`, `res/*.metainfo.xml`

**Problem.** The desktop entries have no `Name[de]` and similar keys, and the
Actions are not localized. The current COSMIC app template generates the
`.desktop` and `metainfo.xml` from the Fluent files with the `xdgen` build
dependency, so both stay in sync with the locales.

**Fix.** Adopt `xdgen`, or a small `build.rs` that does the same, using keys
`app-title`, `app-comment` and `app-keywords` (in all locales). Add
`stellarshot-applet` and `stellarshot-web` to `<provides>`. Add
`<supports><control>keyboard</control><control>pointing</control></supports>`.

**Verify.** `desktop-file-validate` and `appstreamcli validate --pedantic`
stay clean (both pass today). `grep 'Name\[de\]'` on the generated file finds
a match.

---

## Architecture and code quality (ARC)

### ARC-1. Split `app.rs`

**Medium · L · Verified**

**Files:** `src/app.rs` (3,018 lines, 63 `Message` variants). Clippy's
`too_many_lines` flags `app.rs` functions at 412, 316, 313, 254 and 190 lines,
`restore.rs` at 379, and `wizard/mod.rs` at 291.

**Problem.** The pages already use a clean Effect pattern
(`update -> Vec<Effect>`), but `app.rs` still holds the whole Settings page
(about 30 message variants and a 320-line view), every page's effect runner
(`run_profile_effects` at about 325 lines, `run_restore_effects` at about
255), the dialog system, and nav management.

**Fix.** A pure move, in separate `refactor:` commits:

```
src/app/
  mod.rs             App, the app-level Message (~20 variants), Application impl
  nav.rs             NavItem, rebuild_nav / refresh_nav_row, select_wizard, go_home
  dialog.rs          Dialog, dialog::Message, view(), can_confirm()
  pages/settings.rs  SettingsPage state + settings::Message + update() + view()
  effects/profile.rs, effects/restore.rs, effects/wizard.rs
```

The top-level `Message` becomes:

- `Settings(settings::Message)`
- `Dialog(dialog::Message)`
- `Profile(id, profile::Message)`
- `Wizard(..)`
- `Restore(..)`
- the app-level variants.

Order: Dialog first (no page dependencies), then Settings, then the effect
runners.

**Verify.** `cargo test` and clippy stay clean with no behavior change. The
new `src/app/mod.rs` is under about 700 lines.

---

### ARC-2. Remove duplicated logic

**Medium · M · Verified**

| Logic | Copies | Single home |
|---|---|---|
| systemd unit helpers (`exec_quote`, `unit_dir`, `systemctl`, `write_if_changed`, unit header text) | `schedule.rs:25,73,156,184` and `web_daemon.rs:25-27,48,74,97` | `src/systemd.rs`. `exec_quote` is security-relevant escaping; two copies **will** drift |
| XDG and HOME resolution | `rclone.rs:50`, `schedule.rs:25`, `web_daemon.rs:48`, `migrate.rs:19`, `web_tls.rs:35`, `lock.rs:18`, `profile.rs:121`, `dejadup.rs:72`, `format.rs:52`, `app.rs:2055` | `src/paths.rs`: `config_home()`, `data_home()`, `state_home()`, `runtime_dir()`, `home()`, all failing cleanly rather than falling back to `/tmp` ([SEC-5](#sec-5-shared-tmp-fallbacks-for-the-runtime-directory-and-backend-log)) |
| Process with timeout, process group, bounded pipe drain | `hooks.rs`, `rclone.rs:85-146`, `password_command.rs`, `app/child.rs` | `src/process.rs`: `run_with_timeout(Command, Duration) -> Result<Output, ProcessError>`. This fixes [REL-4](#rel-4-hooks-and-password-commands-can-hang-past-their-timeout) and [SEC-9](#sec-9-rclone-argument-hygiene) once |
| Hostname | `maintenance.rs:73`, `place.rs:39`, `app.rs:2087`, `web_tls.rs:62` | `engine::hostname()`. It must match what rustic writes, because `forget` filters on it |
| Current Unix time | `scheduled.rs:73`, `format.rs:86`, `web.rs:253`, `applet.rs:54`, `routes.rs:109,197` | `core::time::now()` |
| State store opening | `run_state.rs:192`, `event_log.rs:110` | `core/state.rs` (also home of [REL-10](#rel-10-unreadable-state-is-replaced-with-defaults-and-saved-over)) |
| Status derivation (running, failed, overdue computed twice) | `status.rs:32-44` vs `run_state.rs:90-123` | `status` uses `run_state::status_at` |
| `canonicalize(..).unwrap_or(raw)` | `backup.rs:78`, `estimate.rs:142` | `engine::canonical_or_raw` |
| Short snapshot ID (`get(..8)`) | `snapshots.rs:42`, `event_log.rs:195` | `engine::short_id` + `SHORT_ID_LEN` |
| `" (deleted)"` handling | `child.rs:107`, `schedule.rs:150` | `src/exe.rs` ([REL-11](#rel-11-one-policy-for-a-binary-replaced-while-running)) |
| Keyring store, load and forget | `keyring.rs:36-68` vs `77-117` | generic item functions ([REL-16](#rel-16-keyring-failures-are-silent)) |
| Spawn-error mapping | `rclone.rs:73-79` and `96-102`; `271,289` map *every* error to `RcloneMissing` | one `map_spawn_err` |

**Verify.** For each row, grep shows one definition. Existing tests pass.

---

### ARC-3. Centralize tuning values

**Low · S · Verified**

These values live outside `src/constants.rs`:

- `CAPACITY = 200` (`event_log.rs:26`)
- `OVERDUE_FACTOR` (`run_state.rs:25`)
- `TTL = 365 days` (`mount.rs:32`)
- 50 ms process poll (`hooks.rs:128`, `rclone.rs:139`)
- the unit settings `Nice=10`, `IOSchedulingClass=idle`,
  `RandomizedDelaySec=10min` (`schedule.rs:99-117`)
- `RestartSec=5` (`web_daemon.rs`)
- compression levels `-3` and `19` (`profile.rs:204`)
- Smart retention 7/4/12 (`profile.rs:230`)
- `HOUR` and `DAY` (`profile.rs:170`, plus an `86_400` literal in
  `constants.rs:160`)
- `CLOCK_TICK` (`app.rs:55`)
- `REFRESH = 3s` (`applet.rs:35`)
- `RESULT_LIMIT`, `RECENT`, `history::LIMIT`
- the SSH port 22 (`place.rs:35`, `dejadup.rs:362`)
- the literal `2` in `migrate.rs:62` instead of `CONFIG_VERSION`
- the applet ID duplicated as `APP_ID + ".Applet"` (and in `install.sh:32`)
- the lockout values in `web.rs`

Move them to `constants.rs` with doc comments. Format and protocol facts
(`CACHEDIR_TAG`, `REPOSITORY_ENTRIES`, exit codes) can stay where they are.

---

### ARC-4. The core depends on the GUI module

**Medium · M · Verified**

**Files:**

- `runner.rs:330,333`
- `scheduled.rs:19-20,90`
- `run_state.rs:14-15,87`
- `event_log.rs:15-17`
- `keyring.rs:14`
- `notify.rs:10`
- `profile.rs:61`
- `dejadup.rs:279`

**Problem.** Non-GUI code imports `crate::app::…` (config, errors, format,
`pages::profile::schedule_summary`, `wizard::place::hostname`). That is why
`stellarshot-web` links the whole GUI. `runner.rs:333` calls
`let _ = StellarshotConfig::config()` purely for its side effect of setting
the cache location.

**Fix.**

- Move `APP_ID` and `CONFIG_VERSION` to `constants.rs`, and `app/format.rs`
  and `errors::explain` to `core/`.
- Add `core::config::load()` and call `engine::cache_settings::set(..)`
  explicitly.
- Optional follow-up: split a `stellarshot-core` workspace crate so the
  compiler enforces the boundary, and build `stellarshot-web` without
  libcosmic.

**Verify.** `grep -rn 'crate::app' src/{engine,runner.rs,scheduled.rs,run_state.rs,event_log.rs,keyring.rs,notify.rs,profile.rs,dejadup.rs}`
returns nothing.

---

### ARC-5. Narrow the public surface and tidy module names

**Low · S · Verified**

- `lib.rs` makes 23 modules `pub`, but only `app`, `engine`, `keyring`,
  `profile`, `runner`, `scheduled` and `web` are used from outside. Make the
  rest `pub(crate)`; the `dead_code` lint then finds unused items.
- `engine::location::delete_repository(&Path)` sits beside
  `engine::delete_repository(&Location)`, two public functions with one name.
  Make the module `pub(crate)`.
- `schedule.rs` vs `scheduled.rs` is confusing; consider `timers.rs` and
  `scheduled_run.rs`.
- Move `web.rs`, `web_daemon.rs`, `web_tls.rs` and `web_token.rs` under
  `src/web/` (`mod.rs`, `daemon.rs`, `tls.rs`, `token.rs`).

---

### ARC-6. Typed errors instead of `Result<_, String>`

**Low · M · Verified**

**Files:** `schedule.rs`, `web_daemon.rs`, `run_state::save/update`,
`keyring`, `settings_export`, `hooks`; `app.rs:888-889` wraps them as
`ErrorKind::Internal`; `engine/error.rs:169-173`

**Problem.** Several modules return `Result<_, String>`, which the app then
wraps as `ErrorKind::Internal`. `io::Error → ErrorKind::Io` loses the path.

**Fix.**

- Add small `thiserror` enums: `ScheduleError`, `StateError`, `KeyringError`,
  `HookError`.
- Add `EngineError::io(path, err)` so I/O errors keep the path.
- `runner::Job` is a grab-bag of `Option`s. Replace it with an enum per
  operation so required fields are enforced by the type system.
- `Outcome` duplicates `Event::Done`.
- `Operation::from_arg` repeats the variant list; use `const ALL`.

---

### ARC-7. Debug logging meets the standard fully

**Medium · S · Verified**

**Files:** `src/debug.rs:47-91`, `src/app/settings.rs:76`

**Already correct:**

- `ENABLED = DEVELOPER_LOGGING && !cfg!(feature = "release-build")`;
- `debug_log!` expands to `if ENABLED`;
- output goes to a file opened with `O_NOFOLLOW` and mode 0600;
- category constants;
- `error_log!` writes to stderr as well;
- CI verifies the switch.

**Gaps:**

- `sink()` truncates in **every** process (window, applet, `--run`,
  `--scheduled`, web). A child truncates the window's file while the window
  keeps writing at its old offset, which leaves NUL-filled holes. This
  contradicts the module's own doc comment.
- Lines carry no PID or role, and each process's elapsed time starts from its
  own launch, so interleaved lines can't be told apart.
- `RUSTIC_LOG_PATH` lives outside `debug.rs`.
- **Missing instrumentation:**
  - hooks: start, exit, timeout;
  - lock: acquire, contention, release;
  - keyring: failure step;
  - mount: mount, unmount, FUSE errors;
  - `password_command`: start, exit code, duration (never output);
  - `settings_export`: counts;
  - state: load failures.

**Fix.** `debug::init(Role)`: only the window truncates, and every other
process appends. Prefix each line with `[role pid]`. Add the `HOOKS`, `LOCK`,
`KEYRING`, `RCLONE` and `MOUNT` categories and wire them in. Move the backend
log path into `debug.rs` ([SEC-5](#sec-5-shared-tmp-fallbacks-for-the-runtime-directory-and-backend-log)).

**Verify.** With logging on, run the window plus Back Up Now.
`grep -c $'\x00' <log>` gives 0, and both roles appear.
`scripts/verify-release-build.sh` still passes.

---

### ARC-8. Dependency hygiene

**Status: partially done (batch 1 scope only).** Removed the unused
`paste` direct dependency; `cargo tree -i paste` now prints nothing. The
`axum-server`/`rcgen` version pins, the `base64`/`rand` duplicates,
`tower-http`, and pinning `libcosmic` by `rev` are all still open, tracked
for their own later batches (rand/tower-http tie into WEB-2/WEB-6/WEB-7).

**Medium · S · Verified**

| Item | Action |
|---|---|
| `paste` is a direct dependency, **used nowhere**, and unmaintained (RUSTSEC-2024-0436) | Remove it |
| `axum-server = "0.8.0"` and `rcgen = "0.14.10"` pin patch versions and have no justification comment, unlike every other entry | Use `"0.8"` and `"0.14"` (with rcgen's features per [WEB-7](#web-7-tls-configuration-hardening)), and add comments |
| Duplicate versions: `base64` 0.22 + 0.23; `rand` 0.8 + 0.9 + 0.10 | `rand` is used only for the web token. Use `getrandom::fill` (already in the tree) and drop the direct `rand`. Align `base64` with what the tree already uses if possible |
| `tower-http` is not a direct dependency but is needed for [WEB-2](#web-2-no-header-read-timeout-or-connection-cap-allow-list-checked-after-tls) and [WEB-6](#web-6-response-hygiene-headers-status-codes-error-detail) | Add it (`features = ["timeout", "limit", "set-header"]`), matching the version already in the lockfile (0.6) |
| Git dependencies (`libcosmic`, `atomicwrites`) have no `rev` | Reproducibility rests on `Cargo.lock`, so enforce `--locked` everywhere ([CI-3](#ci-3-reproducible-and-locked-builds)). Pin `libcosmic` by `rev` in `Cargo.toml` and bump it deliberately. **Don't** pin `atomicwrites` separately: libcosmic declares it unpinned, and a rev would add a second copy |

**Verify.** `cargo tree -d` shows fewer duplicates.
`cargo tree -i paste` returns nothing. `cargo audit` is clean.

---

### ARC-9. Add a `[lints]` table and `clippy.toml`

**Medium · S (table) + M (burn-down)**

**Problem.** No lint policy is recorded in the repo. Default clippy is clean,
which is good. A stricter pass on production code (library and binaries,
not tests) finds:

- 10 `unwrap`/`expect` calls;
- 17 unchecked index or slice operations;
- 10 redundant clones (`app.rs:1439-1868`, `scheduled.rs:326-328`);
- 12 significant-drop temporaries held too long (`browse.rs`, `mount.rs`,
  `uploads.rs`, `cache_settings.rs`);
- 5 identical match arms;
- 14 functions over 100 lines.

**Fix.** Add to `Cargo.toml`:

```toml
[lints.rust]
unsafe_code = "deny"          # src/ has none; tests/rclone_permissions.rs gets a local allow
unreachable_pub = "warn"

[lints.clippy]
pedantic = { level = "warn", priority = -1 }   # the COSMIC app template's `just check` runs pedantic
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
unwrap_used = "warn"
expect_used = "warn"
indexing_slicing = "warn"
redundant_clone = "warn"
significant_drop_tightening = "warn"
await_holding_lock = "deny"
allow_attributes = "warn"     # forces #[expect(..)], which errors when no longer needed
```

and `clippy.toml`:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-indexing-slicing-in-tests = true
```

Burn down in batches, one module per PR. For the `unwrap`s:

- `Mutex::lock().unwrap()` in `mount.rs` and `web.rs` becomes
  `unwrap_or_else(PoisonError::into_inner)`.
- `localization.rs:15` becomes `#[expect(clippy::expect_used, reason = "embedded en locale is a build-time invariant")]`.

Change `notify.rs:23`'s `#[allow]` to `#[expect]`. Also add
`[profile.dev.package."*"] opt-level = 2`: the engine tests run rustic's
crypto and compression unoptimized, which is why the suite takes minutes.

**Verify.** `cargo clippy --all-targets` is clean with the table in place. CI
enforces it through the existing `-D warnings`.

---

### ARC-10. Comment and documentation drift in code

**Low · S · Verified**

- `mount.rs:74` says entries are "reported owned by root"; they report the
  mounting user.
- `runner.rs:350` claims `drop(input)` protects the password; it doesn't
  (see [SEC-8](#sec-8-secrets-are-not-zeroized)).
- `config.rs:128-130` claims token regeneration is immediate; it isn't
  (see [WEB-1](#web-1-credential-and-profile-changes-never-reach-the-running-daemon)).
- `applet.rs:32-35` says polling happens only while the popup is open; it
  doesn't (see [UI-7](#ui-7-the-applet-polls-every-3-seconds-on-its-ui-thread-forever)).
- `app/migrate.rs:14` says "this fork" in a code comment. Project content
  shouldn't reference the repository's fork status outside the About page and
  package metadata.

---

## Tests (TST)

### TST-1. Add regression tests for every High finding

**High · (with each fix)**

Each High task above names its test. They must land **with** the fix, fail on
the old code, and be listed in `VALIDATION.md`:

| Task | Test |
|---|---|
| SEC-1 | Untrusted import (hooks, schedule, rclone remote, bad ID) |
| REL-1 | Two profiles, one repository, `forget` isolation |
| REL-2 | After hooks run when `open` fails |
| REL-3 | Child stderr flood |
| REL-4 | Background grandchild and group kill on timeout |
| REL-5 | Glob metacharacters and non-UTF-8 excludes |
| REL-6 | A non-UTF-8 name through every read path |
| I18N-1 | Fluent parse with no junk |
| UI-2 | Launch flags round trip |

### TST-2. Tests that touch the developer's real state

**Medium · S · Verified**

- `settings_export.rs:244-263`
  (`history_is_merged_for_new_and_existing_backups_alike`) writes a
  random-UUID key into the **real** cosmic-config state store, which
  `event_log.rs:148-151` forbids.
- `web/routes.rs:452-468`
  (`starting_a_backup_records_it_in_the_history_under_the_web_source`) writes
  into the real history store.
- `tests/keyring.rs:58-94` restores the real web password only if no
  assertion panics.
- Engine tests probably write `~/.cache/rustic/<id>` for every test
  repository. **Confirm first:** `ls ~/.cache/rustic | wc -l` before and after
  `cargo test`.

**Fix.** Inject a state directory (a temp `XDG_STATE_HOME` in a separate test
binary) for the first two. Use a drop guard in the keyring test. Call
`cache_settings::set(None, true)` in the engine test fixture.

### TST-3. REST tests never run

**Medium · M · Verified**

All tests in `tests/rest_server.rs` are `#[ignore]`, so the REST backend (the
one whose URLs carry credentials) has no test in CI. CI still spends minutes
on `cargo install rustic_server` for them.

**Fix.** Find the CI-only `Connect` failure. Leads, **unverified**:

- the `free_port()` race between dropping the probe listener and the server
  binding;
- `localhost` resolving to IPv6;
- proxy environment variables.

Bind with `--listen 127.0.0.1:0` and parse the chosen port from the log.
Pin the install (`cargo install --locked rustic_server@<x.y.z>`, with
`RUSTFLAGS=` cleared so `-D warnings` doesn't apply to it). Then un-ignore.

### TST-4. Flaky and weak tests

**Status: 2 of 4 done.** `uploads.rs`'s `packs_upload_side_by_side` no
longer asserts an elapsed-time bound (flaky on a busy runner, and
demonstrably so tonight); `most_busy == 4` alone already proves genuine
concurrency, since sequential uploads could never reach it regardless of
speed. `an_after_success_hook_does_not_run_after_a_failed_backup` (in
`tests/runner.rs`) now also asserts an `AfterFailure` hook's own marker
*does* exist, so the test can no longer pass for the wrong reason (After
hooks entirely broken looks identical to "correctly skipped" without it —
both leave the original marker absent). Not done: splitting a pure
`rclone_command()` out of `repo.rs` (`available()` needs a real rclone;
not blocking in this sandbox, where it's installed, so lower priority),
and `lock.rs`'s `is_running` test reimplementing the probe it tests.

**Low · S · Verified**

- `engine/uploads.rs:387-390` asserts `elapsed < 1000ms`, which will flake on
  a busy runner. Assert ordering and `most_busy` only.
- `repo.rs:445-476` needs rclone to test a command string, because
  `backend_options` calls `available()`. Split out a pure
  `rclone_command(config, limit) -> String`.
- `lock.rs`'s `is_running` test re-implements the probe
  ([REL-8](#rel-8-status-polling-can-make-a-real-backup-fail-with-locked)).
- `an_after_success_hook_does_not_run_after_a_failed_backup` passes whether or
  not After hooks work ([REL-2](#rel-2-after-hooks-are-skipped-when-the-repository-fails-to-open)).

### TST-5. Coverage gaps

**Medium · M**

Add engine or integration tests for:

- `Target::Original` restores;
- file-vs-directory type conflicts on restore;
- symlink conflicts on restore;
- prune on an append-only repository;
- `KeepRules` with `Some(0)`;
- an `archive_folder` failure partway through;
- web header-parsing edge cases: Basic without a colon, invalid base64,
  non-UTF-8, lower-case schemes, an empty Bearer value, repeated
  `Authorization` headers;
- `curl --tls-max 1.1` refused and plain HTTP refused
  ([WEB-7](#web-7-tls-configuration-hardening)).

---

## CI, supply chain and packaging (CI)

### CI-1. Workflow permissions and action pinning

**Status: Done, verified against a real zizmor run, not just by reading the
YAML.** Both workflows get a top-level `permissions: contents: read` and a
`concurrency` group (`cancel-in-progress: false` on the release workflow,
since a half-published release is worse than a slow one). Every `uses:` is
pinned to a full commit SHA resolved through `gh api repos/<owner>/<repo>/commits/<tag>`
(never guessed), with a `# vX.Y.Z` comment; a new `.github/dependabot.yml`
keeps those SHAs from silently going stale. `actions/checkout` sets
`persist-credentials: false` everywhere. The release workflow is split into
a read-only `build` job (packages, tests, artifacts) and a `publish` job
that holds `contents: write` and does nothing but download the artifacts
and create the release — a `build.rs` anywhere in the dependency tree runs
under the read-only token now, never the write one. `Swatinem/rust-cache`
runs with `lookup-only: true` in the release workflow specifically, closing
the cache-poisoning angle a tag-triggered run is the classic target for.
Also replaced `softprops/action-gh-release` with a plain `gh release create`
step (one fewer third-party action; `gh` is preinstalled on the runner),
which a pedantic zizmor pass flagged as available; fixed the
template-injection and undocumented-permissions notes that same pass
raised along the way.

A `lint-workflows` job now runs `zizmor --min-severity medium` in CI itself.
Locally: `pip`'s system Python is externally managed
(`error: externally-managed-environment`), so zizmor was installed into a
throwaway venv (`python3 -m venv /tmp/zizmor-venv && .../pip install
zizmor`) rather than with `--break-system-packages`, confirmed the baseline
findings matched this task's own list exactly (2 `artipacked`, 3
`excessive-permissions`, 9 `unpinned-uses`, 1 `cache-poisoning`), then
re-ran after the fix: zero findings at every severity down to
`informational` for the default persona, and only two `--persona=pedantic`
notes left (both suggesting `dtolnay/rust-toolchain` could be replaced by
calling `rustup`/`cargo` directly) — a style opinion, not adopted, since the
action's version pinning and component installation are worth keeping.

**Medium · S · Verified**

**Files:** `.github/workflows/ci.yml`, `.github/workflows/release.yml:15-16`

**Problem.**

- The release job has `contents: write` for **every** step, including the
  build and tests. Any `build.rs` in the dependency tree (git dependencies
  included) runs with a write token.
- `ci.yml` has no `permissions:` block at all.
- Actions are pinned by tag (`@v5`, `@v2`, `@stable`), not by commit SHA.
- `actions/checkout` keeps `persist-credentials: true`.
- There is no `concurrency:` group.

**Fix.**

- `permissions: { contents: read }` at the top of both workflows.
- Split the release into a read-only `build` job (uploading artifacts) and a
  `publish` job with `contents: write` that only downloads the artifacts and
  creates the release.
- Pin every `uses:` to a full SHA with a `# vX.Y` comment.
- `persist-credentials: false`.
- `concurrency: { group: ${{ github.workflow }}-${{ github.ref }}, cancel-in-progress: true }`
  on CI.
- Add Dependabot for `github-actions`.
- Lint the workflows with `zizmor`, in CI.

**Verify.** `zizmor .github/workflows` passes. The job log's "GITHUB_TOKEN
Permissions" block shows `Contents: read` for build steps.

### CI-2. Advisory, license and MSRV checks

**Status: Done, verified against a real `cargo-deny` run on this exact
dependency tree, not written from the template alone.** A downloaded
prebuilt `cargo-deny` binary (EmbarkStudios' GitHub releases, matched by
`sha256` file, not compiled — this machine's toolchain was busy with other
work) drove the actual iteration: the licenses check alone needed 8 more
allow-list entries than guessed up front (`CC0-1.0`, `BSL-1.0`, `Unlicense`
each turned out to gate a real transitive dependency), 9 `[[licenses.clarify]]`
entries for pop-os git crates with no `license` field at all (each
expression confirmed by opening that dependency's own LICENSE file by hand:
`libcosmic`/`dbus-settings-bindings` are MPL-2.0, `cosmic-panel` is
GPL-3.0-only, `window_clipboard` is MIT — never guessed), and two transitive
git sources the plan's own "pop-os and jackpot51" list didn't anticipate
(`wash2/accesskit`, `iced-rs/cryoglyph`, both pulled in beneath `libcosmic`
by `iced`/`accesskit_winit`, not chosen by this project). `cargo deny check`
(all four categories) exits 0 against `Cargo.lock` as it stands.

CI gets two new jobs: `deny` (the official `cargo-deny-action`) and `msrv`
(reads `rust-version` out of `Cargo.toml` so the two can never drift, then
`dtolnay/rust-toolchain` with that exact version — confirmed its
`action.yml` accepts an explicit `toolchain:` input rather than assuming —
then `cargo check --locked --all-targets`).

**Medium · S · Verified**

**Problem.** CI has no `cargo-deny` or `cargo-audit`, and `rust-version =
"1.93"` is never tested (CI uses `stable` only).

**Fix.**

- Add a `deny.toml` covering:
  - `advisories` (deny vulnerabilities; warn on unmaintained, with the known
    transitive ones from libcosmic's text stack listed and dated);
  - `licenses` (allow-list compatible with GPL-3.0-only);
  - `bans` (warn on duplicates);
  - `sources` (allow only crates.io and the pop-os and jackpot51 git
    sources).
- Run `cargo deny check` in CI.
- Add an `msrv` job: `dtolnay/rust-toolchain@1.93` then
  `cargo check --locked --all-targets`.
- Keep `rust-version` at or above rustic_core's own floor (1.91).

**Verify.** Both jobs are green. Temporarily adding a crate with a known
advisory makes the deny job fail.

### CI-3. Reproducible and locked builds

**Status: Done.** `--locked` on every `cargo build`/`test`/`clippy`/`deb`
invocation in `install.sh` and both workflows (the `cargo generate-rpm`
step doesn't build anything itself — it packages what `cmd_build` already
built with `--locked` — so it needed no change). `cmd_build` now runs
through `cargo auditable build` when the tool is installed (every
packaging target and CI always has it; a plain from-source build degrades
gracefully with a warning rather than gaining a new hard dependency),
embedding the exact dependency tree into the binary itself.

**Verified for real, not just wired up:** downloaded a prebuilt
`cargo-auditable` binary (GitHub releases, matched by the version tag, not
compiled) and ran `cargo auditable build --bin stellarshot` against this
project. `cargo audit bin` itself refused the *debug* binary for exceeding
its 100 MB size cap (debug builds carry full debuginfo; the release
profile here also sets `strip = true`, so a real release binary is far
smaller and won't hit it) — worked around by extracting the embedded
`.dep-v0` ELF section directly (`objcopy --dump-section`) and confirming
it's exactly what `cargo-auditable`'s own docs promise: zlib-compressed
JSON listing every dependency (`ab_glyph`, `accesskit`, and on), not junk
or an empty section. The actual release-profile build and a `cargo audit
bin` run against it were not performed tonight (a full release compile of
this GUI app is a genuinely heavy job, and this was enough to confirm the
mechanism works against this exact dependency tree).

Also added: `actions/attest-build-provenance` to the release workflow's
`build` job (its own narrow `id-token`/`attestations` permissions, neither
of which grants push access — kept separate from `publish`'s `contents:
write`), signing every `dist/*.deb`/`*.rpm`/`*.tar.gz`. README documents
`gh attestation verify … --repo stldave314/stellarshot` in the install
section — the exact flag syntax (`--repo` vs `--owner`) was confirmed
against GitHub's own CLI manual page, since the `gh` installed in this
sandbox (2.45.0) predates the `attestation` subcommand and couldn't
verify it by running it directly.

**Medium · S · Verified**

**Problem.** `install.sh:62,141`, the CI test step and `cargo deb` don't pass
`--locked`, so a lockfile that drifted from `Cargo.toml` is silently updated
at build time.

**Fix.** Use `--locked` everywhere, including
`cargo deb -- --locked --features release-build`. Build release artifacts
with `cargo auditable` so `cargo audit bin` (and distribution scanners) can
audit the shipped binaries. Add GitHub artifact attestations
(`actions/attest-build-provenance`) to the publish job, and document
`gh attestation verify` in the README's install section.

**Verify.** `cargo update -p libcosmic` without committing the lockfile makes
the `--locked` CI step fail. `cargo audit bin target/release/stellarshot`
works.

### CI-4. Verify the shipped artifacts, not a debug build

**Status: Done, proven both ways against real extracted artifacts, not
just written and trusted.** New
`scripts/verify-packaged-binaries-strip-logging.sh` extracts every
`dist/*.deb`/`*.rpm`/`*.tar.gz`, finds every real ELF binary inside (`file
--mime-type`, so a README or `.desktop` file containing the path in an
example wouldn't cause a false pass or fail), and fails if any of them
contain the debug log path — with its own "found nothing to check" guard
so a broken extraction can't pass vacuously. Wired into both `ci.yml` and
`release.yml`, right after `./install.sh package`.

Verified locally: the real `stellarshot_0.6.0-1_amd64.deb` already in
`dist/` extracts cleanly and all three binaries pass. `rpm2cpio` is not
installed in this sandbox and `sudo apt-get install` needs an interactive
password prompt this environment can't answer, so the `.rpm` path could not
be exercised here — added to both workflows' dependency install list
(confirmed a real Ubuntu package via `apt-cache show rpm2cpio`, not
guessed) but untested locally; CI will be the first real run of that path.
The `.tar.gz` and `.deb` paths, however, were proven **both ways**, not just
against a clean artifact: a real ELF binary (`/bin/true` with the log path
string appended) packed into a throwaway tarball made the script correctly
report `FAIL`, then removing the string made it `PASS` again — so this
cannot pass vacuously the way a check that was only ever run against
already-clean output could.

**Low · S · Verified**

**Files:** `scripts/verify-release-build.sh:55-61`

**Problem.** The script checks `target/debug` and edits `src/debug.rs` in
place with `sed`. It doesn't inspect what is actually shipped.

**Fix.** After "Build packages", extract the `.deb` and assert the debug log
path is absent from every binary:

```sh
dpkg-deb -x dist/*.deb x && ! strings x/usr/bin/* | grep -F /tmp/stellarshot-debug.log
```

Do the same for the tarball and the rpm (`rpm2cpio | cpio -id`). Keep the
positive control (logging on means the string is present) so the check can't
pass vacuously.

### CI-5. Packaging fixes

**Status: 3 of 5 done** (the other two are cross-referenced to TST-3 and
WEB-10 by the plan itself, neither started).

- **The tarball's `install.sh` couldn't work:** done. New
  `install-tarball.sh` only copies the tarball's already-built `usr/` tree
  (`cp -a`, no `cargo`/source needed); `cmd_tarball` now packs it instead of
  `install.sh`. Verified against a fake staged `usr/` tree in `DESTDIR` mode
  (no root needed): files land at the right paths with the right content.
  The real-root path (`as_root cp -a`) reuses the same proven copy logic
  behind a `sudo` wrapper untestable without an interactive prompt in this
  sandbox — same disclosed limitation as everywhere else tonight that
  touched `sudo`. README documents `./install-tarball.sh` as its own step,
  distinct from `install.sh`.
- **FUSE dependency undeclared:** done. `recommends = "rclone, fuse3"` (deb)
  and `{ rclone = "*", fuse3 = "*" }` (rpm); README's Requirements section
  now mentions `fuse3` for **Mount as Folder** the same way it already did
  for rclone.
- **`cargo install rustic_server` unpinned:** not started; see
  [TST-3](#tst-3-rest-tests-never-run), which this task already deferred to.
- **web-interface docs not installed:** done for the doc file itself —
  `docs/web-interface.md` is now a packaged asset in both deb and rpm
  metadata, and `install.sh`'s own `stage()` installs it for a from-source
  install too, all under `.../doc/stellarshot/web-interface.md`. Not done:
  linking the *versioned* doc from Settings instead of `main` — that's a
  Settings-page change in `app.rs`'s web-settings section, left to whoever
  is working WEB-1 through WEB-12 tonight.
- **Uninstall leaves user units behind:** not started; see
  [WEB-10](#web-10-service-unit-hardening-and-upgrade-handling), which this
  task already deferred to.

**Medium · S · Verified**

| Item | Where | Fix |
|---|---|---|
| The release tarball contains an `install.sh` that can't work (it needs cargo and `target/release`) | `install.sh:156-177` | Ship a small `install-tarball.sh` that copies `usr/` to `${DESTDIR}${PREFIX}`, and document it in the README |
| FUSE runtime dependency undeclared; "Mount as Folder" needs `fusermount3` | `Cargo.toml` deb and rpm metadata | Deb `recommends = "rclone, fuse3"`, rpm `recommends = { rclone = "*", fuse3 = "*" }`; add to README requirements |
| `cargo install rustic_server` unpinned, unlocked, and under `-D warnings` for tests that are all ignored | `ci.yml:38-43` | See [TST-3](#tst-3-rest-tests-never-run) |
| The web interface docs are not installed | deb and rpm assets | Install `docs/web-interface.md` under `/usr/share/doc/stellarshot/` and link the versioned doc (`/blob/v{CARGO_PKG_VERSION}/…`) from Settings rather than `main` |
| Uninstall leaves user units behind | `install.sh:118-132` | See [WEB-10](#web-10-service-unit-hardening-and-upgrade-handling) |

---

## Documentation (DOC)

### DOC-1. American spelling

**Status: Done.** Fixed every item in the table below (release.yml's
"centres" had already lost "artefacts" to an earlier pass tonight; fixed
the remaining one), renamed each listed test/function identifier and
updated every file that referenced it by name (`VALIDATION.md`,
`docs/plans/2026-09-26-review-tasks.md` itself for `normalise_section`),
then re-swept the whole tree (`src/`, `tests/`, every `.md`/`.ftl`/`.toml`/
`.yml`/`.sh`) for a broader list of common British spellings beyond this
table's own items. Nothing further turned up.

**Low · S · Verified**

| File | Fix |
|---|---|
| `.github/workflows/release.yml:45,68` | centres → centers, artefacts → artifacts |
| `CHANGELOG.md:393` | re-initialises → re-initializes |
| `ROADMAP.md:694` | authorised → authorized |
| `VALIDATION.md:193,465,468,519` | cancelling/cancelled → canceling/canceled, modelled → modeled |
| `docs/plans/2026-09-23-m3-storage-and-import.md` | modelled → modeled |
| `src/engine/backup.rs` (doc comment on `globs`) | canonicalised/canonicalises → canonicalized/canonicalizes |
| `src/dejadup.rs:103,120,382` | `normalize_section` → `normalize_section`, Modelled → Modeled |
| `src/engine/error.rs:6` | serialisable → serializable |
| `src/engine/location.rs:126` | `init_recognises_…` → `init_recognizes_…` |
| `src/engine/disk_tree.rs:218`, `src/app/pages/restore.rs:1758`, `tests/child.rs:81`, `tests/rclone.rs:183` | `cancelling_…` → `canceling_…` |
| `src/app/pages/profile.rs:1405` | `a_cancelled_backup…` → `a_canceled_backup…` |

Renaming test functions means updating `VALIDATION.md` in the same commit.
Word-boundary greps miss identifiers joined with underscores, so search
without `\b` when checking.

### DOC-2. Docs that contradict the code

**Status: Every non-`docs/web-interface.md` item checked and already
resolved, except the screenshots.** `SECURITY.md:37` was corrected as part
of SEC-3 tonight. README's shortcuts table already has F1; the backend log
path README references is the *developer debug* log
(`/tmp/stellarshot-debug.log`), which SEC-5 deliberately did not move, so
there was never a stale reference to fix there; tarball install
instructions were added as part of CI-5. `CHANGELOG.md` no longer says
"quit completely" anywhere (resolved before tonight, alongside UI-3's own
Quit action landing). The `docs/web-interface.md` items are the peer
session's own territory (WEB-1/6/8). **Not done:** regenerating
`docs/screenshots/`, which needs a live COSMIC session and this session's
own AT-SPI/interactive-UI limitations apply — not attempted rather than
risk an unreliable result, or disturbing Dave's actual desktop session in
the middle of the night to drive it.

**Medium · S · Verified**

- **`docs/web-interface.md`:**
  - Document that `POST …/run` returns **202**, or 409 after
    [WEB-8](#web-8-graceful-shutdown-and-duplicate-run-requests).
  - Update the status mapping after [WEB-6](#web-6-response-hygiene-headers-status-codes-error-detail).
  - Document that profile and credential changes need a restart, until
    [WEB-1](#web-1-credential-and-profile-changes-never-reach-the-running-daemon)
    lands.
  - Say "all network interfaces" for the LAN scope.
  - Document the blast radius of a leaked token: it can list every file name
    in every snapshot and start backups, which run the owner's hooks.
- **`SECURITY.md:37`:** "Passwords are never written to Stellarshot's own
  files" is false for REST URLs until
  [SEC-3](#sec-3-rest-credentials-are-stored-and-reported-in-plain-text).
- **README:**
  - Add F1 to the shortcuts table.
  - Launch actions (`--new-backup`, `--restore`, `--profile`) are documented
    as working but don't when an instance is running
    ([UI-2](#ui-2-launch-flags-are-lost-when-an-instance-is-running)).
  - Update the backend log path after [SEC-5](#sec-5-shared-tmp-fallbacks-for-the-runtime-directory-and-backend-log).
  - Add tarball install instructions.
- **`CHANGELOG.md` (Unreleased):** it tells users to "quit completely", which
  is impossible until [UI-3](#ui-3-there-is-no-way-to-quit).
- **Screenshots** (`docs/screenshots/`, dated 2026-09-25) predate the header
  Settings icon and the Browse hint. Regenerate them with
  `scripts/screenshots.sh`.

### DOC-3. Warn against mixing rustic writers with `restic prune`

**Status: Done.** Added to README.md's "Reading your backups without
Stellarshot" section, right after the existing CLI examples. Quoted rustic's
own FAQ accurately (fetched and read directly, not from memory): "never run
`restic prune` against a repository a rustic command may be writing to,"
because rustic deletes unreferenced data only after a grace period
specifically to give a concurrent write time to finish, while restic's own
`prune` deletes at once against a lock rustic never takes. Also notes that
Stellarshot's own **Clean Up Now** and automatic clean-up are always safe,
since both go through rustic's own `forget`/`prune`, never restic's.

**Medium · S**

rustic doesn't take or honor restic's lock files, and prunes in two phases
(packs are marked, then deleted after a delay). A `restic prune` run against a
repository Stellarshot is writing to can delete packs Stellarshot just
uploaded. The README says repositories "stay readable with the restic and
rustic command-line tools", which is true. Add a **Troubleshooting / Using
the CLI** note: reading with restic is safe, but never run `restic prune` or
`restic forget --prune` while Stellarshot may be backing up to that
repository. Cite the rustic FAQ.

---

## Standards baseline

What "current best practice" means for this project as of this review. The
tasks above implement it; this is the checklist to hold future work to.

**Rust**

- Stable is **1.98.1**. Avoid 1.98.0, which has a vtable miscompile.
- Edition 2024, which uses resolver 3: MSRV-aware resolution with
  `incompatible-rust-versions = "fallback"`.
- `rust-version` must stay at or above rustic_core 0.13's floor (1.91) and
  must be **tested** in CI ([CI-2](#ci-2-advisory-license-and-msrv-checks)).
- A `[lints]` table in `Cargo.toml`, not ad-hoc flags
  ([ARC-9](#arc-9-add-a-lints-table-and-clippytoml)). Prefer `#[expect]` over
  `#[allow]`.
- Supply chain:
  - `cargo-deny` in CI;
  - `Cargo.lock` committed and `--locked` everywhere;
  - `cargo auditable` for release binaries;
  - GitHub artifact attestations for release files;
  - actions pinned by SHA;
  - `zizmor` for workflows.
- Secrets live in `secrecy::SecretString` (zeroized on drop, redacted
  `Debug`). Never pass them on argv; the environment only for short-lived,
  owner-only children.
- Processes:
  - never use a shell;
  - put a process group around anything that can fork;
  - give every external call a timeout;
  - drain all piped streams concurrently or bound them;
  - put `--` before positional arguments.

**COSMIC**

- libcosmic is still git-only (no crates.io release; last tag `v0.12`).
  Epoch 1 shipped in December 2025, and Epoch 2 (reactive rendering) is
  under way, so expect API churn. Pin libcosmic by `rev` and bump it
  deliberately.
- Current template conventions:
  - `Application` with `Task`, `Core`, `nav_bar::Model`,
    `context_drawer::ContextDrawer`;
  - `#[derive(CosmicConfigEntry)] #[version = N]` with `watch_config`;
  - `widget::about::About` fed from `env!("CARGO_PKG_*")`;
  - i18n via `i18n-embed` + `fl!`;
  - `xdgen` to generate the desktop and metainfo files from Fluent
    ([I18N-3](#i18n-3-localize-the-desktop-entries-and-metainfo));
  - a `justfile` with `check` running clippy pedantic.
- Applets:
  - `cosmic::applet::run`;
  - `core.applet.icon_button`;
  - `cosmic::applet::style()`;
  - desktop keys `NoDisplay=true`, `X-CosmicApplet=true`,
    `X-CosmicHoverPopup=Auto`.
- Every icon-only button needs `.name()` for the accessible name, not just a
  tooltip.

**Web API** (OWASP ASVS 5.0, OWASP API Security Top 10 2023)

- Tokens:
  - 256-bit, from a CSPRNG;
  - stored as SHA-256;
  - compared in constant time;
  - with a recognizable prefix;
  - revocable immediately.
- A password stored for verification (if the keyring is replaced) uses
  Argon2id per the OWASP cheat sheet: m=19456 KiB, t=2, p=1, stored as a PHC
  string.
- Rate-limit failed authentication, and never count credential-less requests.
- `tower-http` layers for timeouts (`TimeoutLayer::with_status_code`, since
  `new` is deprecated) and body limits. hyper connection timeouts need an
  explicit timer.
- Security headers on every response; `no-store` on authenticated responses;
  HSTS only with a real certificate.
- CSRF defense (`Sec-Fetch-Site`, with `Origin` as the fallback) on every
  route now ([WEB-11](#web-11-there-is-no-csrf-protection)), plus CSRF tokens
  before any cookie session.
- One rustls crypto provider, installed explicitly.

**Advisory status of the lockfile** (checked on this date): no known
vulnerabilities.

- rustls 0.23.45, h2 0.4.19, tar 0.4.46, time 0.3.55, tokio 1.53.1,
  fuser 0.18.0 and rustls-webpki 0.103.15 are all at or past the fixes for
  the 2025–2026 advisories that touch them.
- Warnings: `paste` (direct, unmaintained; remove it).
- Transitive, from libcosmic's text stack: `rustybuzz`, `ttf-parser`
  (unmaintained), `lru`, `memmap2` (unsound). Track these upstream;
  they aren't ours to fix.

---

## What is already right: do not regress

Reviewers confirmed these in the code. Each should have (or keep) a test.

- **Password handoff.** The repository password reaches the `--run` child as
  JSON on stdin, never argv or environment. The password command's stdin is
  closed, not inherited.
- **No shell anywhere.** Hooks and the password command are split with
  `shell_words`. The rclone command string quotes its config path and
  bandwidth limit, with an injection regression test.
- **SFTP.** Values are quoted against `, : " '` and spaces, and
  `known_hosts_file` is always set, so unknown or changed host keys are
  refused.
- **Private rclone config.** The file is 0600 and the directory 0700, both set
  **before** any token is written, with tests.
- **Safe deletion.** Only repository-format entries are removed, symlinks are
  never followed, and a probe runs first. This holds locally and over rclone.
- **Cancel.** Canceling kills the whole process group, with correct reasoning
  about PGID reuse, and a drain deadline for orphans.
- **Unit files.** `exec_quote` escapes `\ " % $` and rejects newlines. IDs are
  allow-listed. Writes are atomic and fsynced, with injection tests.
- **Log files.** Opened `O_NOFOLLOW | O_CLOEXEC` at 0600. Developer logging is
  compiled out by `release-build`, verified in CI.
- **Secret handling.** `Secret`'s `Debug` is redacted, and `redact_url` fails
  closed. Export strips `password_command` and REST credentials.
- **Web auth:**
  - It fails closed when no method is enabled; the scope defaults to Off.
  - The allow-list is the outermost layer, and auth covers every route
    including health.
  - A lockout rejects even correct credentials, and success clears the count.
  - The token is 256-bit, stored only as SHA-256, and compared in constant
    time.
  - The web password lives in the keyring.
  - TLS is always on, a broken custom certificate fails loudly, and
    `X-Forwarded-For` is never trusted. There's no permissive CORS.
- **Path containment in browse.** `?path=` only walks the snapshot's own tree;
  `..` is rejected by rustic, and the host filesystem is never touched. The
  `tar` crate refuses `..` and absolute paths in archives.
- **FUSE mount.** Read-only, no `allow_other`, and ownership mapped to the
  mounting user.
- **Uploads.** The `settle()` ordering guarantees an index never references a
  pack that didn't arrive.
- **Tests fail instead of skipping** when a tool is missing: keyring, rclone,
  curl. Only the REST suite is an exception ([TST-3](#tst-3-rest-tests-never-run)).
- **Metadata.** Locale parity is enforced, `desktop-file-validate` and
  `appstreamcli validate --pedantic` pass, and no build artifacts are
  committed.

---

## Sequencing

Suggested batches. Each batch is independently shippable; within a batch,
tasks can go to different developers in parallel.

1. **Stop the bleeding.** Small, high-impact changes:
   - SEC-1, REL-2, REL-3, REL-5, I18N-1, UI-1, UI-3, UI-4;
   - ARC-8 (`paste` removal only).

   Most are **S**, and each comes with its TST-1 test.
2. **Data safety.**
   - REL-1 (with its migration rule), REL-4 (via ARC-2's process helper),
     REL-6;
   - REL-10, REL-8, REL-9.
3. **Web interface hardening, before advertising the feature.**
   - WEB-11 first (small; WEB-3 depends on it), then WEB-1, WEB-2, WEB-3,
     WEB-4, WEB-5;
   - then WEB-6 to WEB-10.
   - Each needs its live `curl` or `openssl` proof in the PR.
4. **Secrets and filesystem.** SEC-2 through SEC-9.
5. **CI and supply chain.** CI-1 through CI-5, ARC-9 (the table), TST-2,
   TST-3.
6. **Structure.**
   - ARC-4, then ARC-1 (pure moves), ARC-2, ARC-3, ARC-5, ARC-6, ARC-7;
   - UI-5 to UI-11.
7. **Polish.** I18N-2, I18N-3, REL-11 to REL-17, DOC-1 to DOC-3, TST-4,
   TST-5, and the ARC-9 burn-down.

Documentation updates (README, `docs/web-interface.md`, SECURITY.md,
CHANGELOG) ride along with the task that changes the behavior. They aren't
saved for batch 7.

---

## Review evidence

- **Clippy.** `cargo clippy --all-targets` (default lints) is clean. The
  stricter pass above was run on library and binary targets only, so test code
  doesn't drown the signal.
- **Test suite.** `cargo test --all-features` on the working tree (rclone,
  rustic-server and gnome-keyring present): 415 passed, 0 failed, 3 ignored
  (all of `tests/rest_server.rs`, see [TST-3](#tst-3-rest-tests-never-run)).
  The library unit tests alone took 17 minutes in the unoptimized dev
  profile, which is why [ARC-9](#arc-9-add-a-lints-table-and-clippytoml)
  sets `opt-level = 2` for dependencies.
- **`cargo audit`.** No vulnerabilities; five warnings, listed under
  [Standards baseline](#standards-baseline).
- **`cargo outdated -R --depth 1`.** All direct dependencies are current.
- **Validators.** `desktop-file-validate res/*.desktop` passes, and so does
  `appstreamcli validate --pedantic`, with one pedantic note about the
  uppercase component ID, which is expected for this RDNN.
- **Locale parity.** Scripted: 535 keys in each of five locales, matching
  placeholder sets, and every key referenced.
- **Findings.** Line numbers are from the working tree on the review date and
  will drift as tasks land; search for the quoted code if a line has moved.
