# COSMIC Store audit: security, bugs, and the path to a store listing

**Date:** 2026-10-16 · **Baseline:** `main` at `280f021` (0.10.1), working
tree clean.

A security and suitability audit of Stellarshot as a COSMIC Store candidate:
every place it spawns a process, handles a secret, or touches the filesystem,
the window's behavior under bad data, the store metadata, and what packaging
the store can actually list. Each task says what is wrong, where, how to fix
it, and how to prove the fix, and is sized for a developer who is new to the
codebase. Tasks that need a decision from the maintainer before anyone starts
are marked **Decision**.

## Decisions (2026-10-16)

| Question | Answer |
|---|---|
| App ID (ST-0) | Keep `io.github.stldave314.Stellarshot`. If the project moves upstream later, use Flathub's one-time rename |
| Sandboxed scheduling (ST-2) | Option (b): use the Background portal when it exists; otherwise write the autostart entry directly (`--filesystem=xdg-config/autostart:create`) |
| Closing the window (S-3) | Quit when nothing is running and the applet is not on a panel; otherwise keep running, and forget unlocked passwords once work ends |
| Flatpak manifest (ST-1) | Written by hand by the maintainer. Everything the manifest depends on is built here, plus a written spec of what the manifest needs; no manifest file is generated |
| Host-access features in the sandbox (ST-4) | Option (b), the recommendation: shown as unavailable, with the reason |

## Status (2026-10-16, end of day)

Worked in the order of the roadmap below; uncommitted. Each fixed task has
its test in `VALIDATION.md` ("Added for the COSMIC Store audit").

| Task | Resolution |
|---|---|
| R-1 | Fixed: the parent is judged by `/proc/<ppid>/exe`, not its name; an unreadable parent (init) is not ours |
| R-2 | Fixed: snapshots more than a day ahead are kept and left out of retention; the status card and "open existing" ignore them |
| R-3 | Fixed: at most `limit` paths accepted inside rustic's own matcher, 20 snapshots listed per path with a count, "narrow your search" shown. The minimum query length was not added: the bound makes a short query safe |
| S-1 | Fixed in code: `restore::Walk` on every walk, with NUL, length and duplicate-name checks. **Open:** the end-to-end proof with a hand-built repository |
| R-4 | Fixed: `load_checked`, so a keyring error is reported |
| R-5 | Fixed: `error_log!` ignores a failed stderr write. `catch_unwind` around `on_term` was not added; the panic it guarded against is gone |
| R-6 | Fixed: 200 rows on the Snapshots page, 500 versions, each with a count of the rest. Lowercasing per frame and display truncation were not changed |
| S-2 | Fixed: unused `stellarshot-` sections removed at start, after removing a backup, and after the wizard finishes or is discarded; never while the wizard is open or the profiles file is unreadable |
| S-3 | Fixed, per the decision above; the applet is found through COSMIC's panel configuration |
| S-4 | Fixed: only this computer's folders, at most 100; none means the page asks. The **What** step was not added to `Mode::Open` |
| S-5 | Fixed: a running hook is stopped only between Before and After; `run_before` stops once terminating. R-8 item 3 fixed with it |
| ST-0 | Decided (above) |
| ST-1 | Spec written (`docs/specs/2026-10-16-flatpak-packaging.md`); the manifest is the maintainer's to write |
| ST-2 | Built: `src/background.rs` and `src/sandbox.rs`, chosen at run time. **Not run in a Flatpak yet** |
| ST-3 | Built: UDisks2 in a sandbox. **Not run in a Flatpak yet** |
| ST-4 | Built: Mount as Folder, hooks and the password source shown as unavailable in a sandbox |
| ST-5 | Fixed, with a check in `validate-metadata.sh` |
| ST-6 | Applet metainfo added, packaged and validated. **Open:** a screenshot of its popup; the upstream Flatpak blocker; how a sandboxed applet reads the app's state |
| ST-7 | **Open:** needs GNOME and KDE to test on |
| S-6 | Fixed: `Destination::paths_are_sane`, applied on locate and import |
| S-7 | Fixed: downloads written `0600` |
| S-8 | Fixed: `Redacted` for typed passwords and the client secret, masked secret field, `Serve`'s `Debug`, the URL scrubber. **Open:** the REST password still lives inside `Location::Rest.url`; the client secret is still passed in rclone's environment (verify first) |
| S-9 | Fixed: all five items |
| R-7 | Fixed: all four items (walk limit in `Walk`, tar padding bound, `bounded::output_within` for helpers, import cap). The tar bound has no unit test: `BlobReader` needs a repository |
| R-8 | Fixed: `bounded::reap_within` at all four sites, a parent-death signal for hooks, and item 3 with S-5 |
| R-9 | Fixed: all four items |
| M-1 | Fixed in all five locales, with a length check |
| M-2 | Partly: the empty-state screenshot is out of the listing. **Open:** regenerate the screenshots without the `/tmp` path (the script stores a demo password in the real keyring) |
| M-3 | Items 1 and 2 done (a display-length requirement added; no touch claim). **Open:** items 3 and 4 |
| P-1 | Not a problem after all: `cargo deny` does not evaluate unsound advisories, so entries for them are reported as matching nothing. Instead, four new advisories against rustls (RUSTSEC-2026-0346 to 0349) were fixed by updating it to 0.23.46 |
| D-1 | Done: `SECURITY.md`, `README.md`, `ROADMAP.md`, `CONTRIBUTING.md`, `CHANGELOG.md` |

## Verdict

**The code is in good shape; the packaging is not yet store-ready.**

- No high-severity issue in process spawning or secrets handling. Three
  high-severity bugs elsewhere (one restore path-safety gap, one cloud backup
  killer, one retention data-loss case) and one memory blow-up. All four are
  small, contained fixes.
- The COSMIC Store does not take submissions of its own. It lists what the
  system's sources carry (see [How apps reach the COSMIC
  Store](#how-apps-reach-the-cosmic-store)). Stellarshot today ships only
  `.deb`, `.rpm` and a tarball from its own GitHub releases, which no store
  source carries, so **it cannot appear in the store at all yet.** Getting it
  there means a Flatpak on Flathub (the app) and on the COSMIC Flatpak
  repository (the applet), and the app's scheduling design does not survive a
  sandbox as it stands (ST-2).

## How apps reach the COSMIC Store

The store shows apps from whatever sources the system has configured. On
Pop!\_OS these are **Flathub**, the **COSMIC Flatpak repository**
(`https://apt.pop-os.org/cosmic/cosmic.flatpakrepo`), and the distribution's
own packages through PackageKit. The store's own source (`src/main.rs` in
`pop-os/cosmic-store`) adds the Flathub and COSMIC remotes as its two
recommended sources.

| Route | Who decides | Fits | Notes |
|---|---|---|---|
| Flathub | Flathub reviewers | The app | Flathub rejects applets and desktop extensions outright, so the applet cannot go here |
| COSMIC Flatpak repository (`pop-os/cosmic-flatpak`) | System76 | The applet; the app only if Flathub refuses it | Its README says "For COSMIC apps, please first try to submit to Flathub" |
| Distribution packages (Pop!\_OS, Debian/Ubuntu, Fedora) | Each distribution | Both | Slow, and Debian/Fedora policy forbids the 53 git dependencies `Cargo.lock` carries today; not a practical first route |

Two things in the store depend on metadata Stellarshot does not have yet:

- The **"Made for COSMIC"** explore section lists only components whose
  metainfo has `<provides><id>com.system76.CosmicApplication</id></provides>`
  (ST-5).
- The **Applets** category lists only components that provide
  `com.system76.CosmicApplet`. Stellarshot's applet has no metainfo at all
  (ST-6).

## What was run

| Check | Result |
|---|---|
| `cargo clippy --all-targets --all-features --locked` (pedantic set) | 0 warnings |
| `cargo deny check` | advisories, bans, licenses, sources ok |
| `cargo audit` | 0 vulnerabilities; 5 warnings, all transitive through libcosmic: 3 unmaintained (already in `deny.toml`) and 2 unsound (`lru` 0.16.4, RUSTSEC-2026-0253; `memmap2` 0.8.0, RUSTSEC-2026-0186; not in `deny.toml`, see P-1) |
| `scripts/validate-metadata.sh` | pass; one pedantic hint (uppercase letters in the component ID, which Flathub allows) |
| CI, Release, CodeQL for `280f021` | all green |
| Hardcoded English in widgets (`text("…")` and friends under `src/app`, `src/bin`) | none found |
| A process started through `/proc/self/exe` is named `exe`, not `stellarshot` | confirmed on this machine; this is what makes R-1 real |
| Installed COSMIC Flatpaks (`minimon-applet`, `CosmicTweaks`, `CosmicExtAppletDrives`) | their permissions and metainfo used as the reference for ST-1 and ST-6 |

The rest is a source review of every file under `src/`, plus the rustic_core,
rustic_backend, fuser, tar, oo7 and atomicwrites sources where a finding
depends on them. Nothing in the three earlier task lists
(`2026-09-26-review-tasks.md`, `2026-09-29-production-readiness-tasks.md`,
`2026-09-30-release-review-tasks.md`) is repeated here, and none of their
fixes has regressed.

## How to work these tasks

- **Write the failing test first.** Every task has a **Verify** line. Make that
  test fail on the current code, then fix, then watch it pass. If you cannot
  make it fail, stop and say so in the task: the finding may be wrong.
- **Prove security fixes with a negative control:** remove the fix, confirm
  the test goes red, put the fix back. For S-1 a unit test is not enough; see
  its own **Verify**.
- **Status** says how sure we are:
  - **Confirmed (test):** reproduced on a real system.
  - **Confirmed (code):** read and traced, not run.
  - **Verify first:** plausible, not traced end to end. Reproduce it before
    changing anything; if it does not reproduce, close it with a note.
- Before calling a task done: `cargo fmt --check`, `cargo clippy --all-targets
  --all-features` (zero warnings), and the tests the task touches. Any
  user-visible string goes through `fl!` and into **all five** locales
  (`tests/i18n.rs` checks this); the German locale addresses the user as "du".
  New tuning values go in `src/constants.rs`, documented, never duplicated.
- Update `README.md`, `SECURITY.md`, `VALIDATION.md` and `CHANGELOG.md`
  `[Unreleased]` when behavior a user can see changes.
- Resource limits on this machine: run cargo through
  `systemd-run --user --scope -q -p MemoryMax=6G -p CPUQuota=300% nice -n 10 cargo … -j 4`,
  one heavy job at a time. The rclone integration tests can stall behind the
  local firewall (OpenSnitch) on a freshly built test binary; that is not a
  code bug.

Severity: **High** (data loss, security, or a failed/stuck backup) ·
**Medium** · **Low**. Effort: **S** (under half a day) · **M** (1–2 days) ·
**L** (more than 2 days).

## Summary

### Store blockers

| ID | Sev | Effort | Title |
|---|---|---|---|
| [ST-0](#st-0-decide-the-app-id-and-the-route-before-the-first-submission) | High | S | **Decision:** the app ID and the route, before the first submission |
| [ST-1](#st-1-there-is-no-flatpak-manifest) | High | L | There is no Flatpak manifest |
| [ST-2](#st-2-scheduling-depends-on-the-hosts-systemd) | High | L | Scheduling depends on the host's systemd |
| [ST-3](#st-3-removable-drives-are-found-through-paths-a-sandbox-hides) | High | M | Removable drives are found through paths a sandbox hides |
| [ST-4](#st-4-mount-as-folder-hooks-and-the-password-command-need-host-access) | Medium | M | **Decision:** Mount as Folder, hooks and the password command need host access |
| [ST-5](#st-5-the-metainfo-does-not-say-this-is-a-cosmic-application) | Medium | S | The metainfo does not say this is a COSMIC application |
| [ST-6](#st-6-the-applet-has-no-store-identity-and-no-route-today) | Medium | M | The applet has no store identity, and no route today |
| [ST-7](#st-7-the-app-is-described-as-cosmic-only) | Medium | S | The app is described as COSMIC-only |

### Security

| ID | Sev | Effort | Title |
|---|---|---|---|
| [S-1](#s-1-restore-accepts-children-of-a-non-directory-node) | High | S | Restore accepts children of a non-directory node |
| [S-2](#s-2-rclone-credentials-are-never-removed-from-stellarshots-rcloneconf) | Medium | M | rclone credentials are never removed from Stellarshot's `rclone.conf` |
| [S-3](#s-3-closing-the-window-leaves-an-invisible-process-holding-passwords) | Medium | M | Closing the window leaves an invisible process holding passwords |
| [S-4](#s-4-opening-an-existing-repository-adopts-its-folders-unseen) | Medium | S | Opening an existing repository adopts its folders unseen |
| [S-5](#s-5-a-sigterm-during-the-after-hooks-kills-the-running-after-hook) | Medium | S | A SIGTERM during the After hooks kills the running After hook |
| [S-6](#s-6-imported-local-and-removable-paths-are-not-validated) | Low | S | Imported local and removable paths are not validated |
| [S-7](#s-7-downloads-get-the-umask-mode-not-a-private-one) | Low | S | Downloads get the umask mode, not a private one |
| [S-8](#s-8-secret-hygiene-gaps) | Low | M | Secret hygiene gaps |
| [S-9](#s-9-smaller-hardening-items) | Low | S | Smaller hardening items |

### Bugs and robustness

| ID | Sev | Effort | Title |
|---|---|---|---|
| [R-1](#r-1-the-orphan-sweep-kills-the-rclone-of-a-live-backup) | High | S | The orphan sweep kills the rclone of a live backup |
| [R-2](#r-2-a-future-dated-snapshot-makes-keep-for-n-days-forget-everything) | High | S | A future-dated snapshot makes "Keep for N days" forget everything |
| [R-3](#r-3-search-everywhere-can-exhaust-memory) | High | S | Search everywhere can exhaust memory |
| [R-4](#r-4-a-password-change-can-leave-the-old-password-in-the-keyring) | Medium | S | A password change can leave the old password in the keyring |
| [R-5](#r-5-a-closed-stderr-can-hang-a-backup-holding-the-repository-lock) | Medium | S | A closed stderr can hang a backup holding the repository lock |
| [R-6](#r-6-snapshot-lists-are-rendered-without-a-cap) | Medium | S | Snapshot lists are rendered without a cap |
| [R-7](#r-7-bound-everything-a-repository-or-a-helper-process-controls) | Low | M | Bound everything a repository or a helper process controls |
| [R-8](#r-8-process-lifetime-gaps) | Low | M | Process lifetime gaps |
| [R-9](#r-9-smaller-robustness-items) | Low | S | Smaller robustness items |

### Store listing, packaging and documentation

| ID | Sev | Effort | Title |
|---|---|---|---|
| [M-1](#m-1-the-store-summary-is-too-long) | Medium | S | The store summary is too long |
| [M-2](#m-2-screenshots-show-a-temporary-path-and-an-empty-state) | Medium | S | Screenshots show a temporary path and an empty state |
| [M-3](#m-3-smaller-listing-items) | Low | S | Smaller listing items |
| [P-1](#p-1-two-unsound-advisories-are-not-recorded-in-denytoml) | Low | S | Two unsound advisories are not recorded in `deny.toml` |
| [D-1](#d-1-documentation-corrections) | Low | S | Documentation corrections |

Then follow the [Roadmap to submission](#roadmap-to-submission).

---

## 1. Store blockers

### ST-0. Decide the app ID and the route before the first submission

**High · S · Decision**

**Problem.** Flathub treats an app ID as permanent: renames "are not accepted
immediately after the initial submission" or "if made frequently." The fork
is on offer back to cosmic-utils (upstream PR #46). If that lands, the
project's natural ID may become a `io.github.cosmic_utils.*` one, and a
Flathub listing under `io.github.stldave314.Stellarshot` would then be wrong
for good, or need a rename with an `end-of-life-rebase`.

**Decide, in this order, and write the answers at the top of this document:**

1. Does the project stay at `io.github.stldave314.Stellarshot`, or move with
   the upstream offer? Do not submit anywhere until this is settled.
2. Route: Flathub for the app, COSMIC Flatpak repository for the applet
   (recommended; see [How apps reach the COSMIC Store](#how-apps-reach-the-cosmic-store)).
3. Which features ship in the sandboxed build (ST-4).

**Note for whoever writes the manifest.** Flathub's requirements forbid
generated content in a manifest. Write it by hand.

---

### ST-1. There is no Flatpak manifest

**High · L · Confirmed (code)** · Depends on ST-0, ST-2, ST-3, ST-4

**Problem.** Nothing in the repository builds a Flatpak, and no store source
carries the `.deb`/`.rpm`/tarball from GitHub releases. `ROADMAP.md` ("After
1.0") defers Flatpak because the app "needs full filesystem access and an
rclone binary, which works against the sandbox." Both are solved problems
for backup tools already on Flathub: Déjà Dup ships `--filesystem=host` and
builds rclone and restic from source in its manifest.

**Fix.**

1. Add `io.github.stldave314.Stellarshot.yml` at the repository root (or in a
   separate packaging repository; Flathub hosts the manifest in
   `flathub/<app-id>` once accepted).
   - Runtime: `org.freedesktop.Platform` with `org.freedesktop.Sdk` and
     `org.freedesktop.Sdk.Extension.rust-stable`, the current version at
     submission time (the installed COSMIC Flatpaks use 25.08 and 26.08).
   - Cargo sources: generate `cargo-sources.json` with
     `flatpak-cargo-generator.py Cargo.lock` from `flatpak-builder-tools`. It
     handles the git dependencies; the build itself has no network.
   - Build with `--features release-build` (the debug-logging rule).
   - A module that builds **rclone from source** (Go, vendored modules),
     modeled on Déjà Dup's `rclone` module (`flathub/org.gnome.DejaDup`).
   - Install the desktop file, both icons, the metainfo and the license to
     `$FLATPAK_DEST/share/licenses/$FLATPAK_ID`.
2. Start from these permissions, and justify each one in the submission:

   | Permission | Why |
   |---|---|
   | `--share=network` | SFTP, REST and cloud destinations |
   | `--share=ipc`, `--socket=wayland`, `--socket=fallback-x11`, `--device=dri` | The window |
   | `--filesystem=host` | Back up and restore any folder the user chooses (Déjà Dup's precedent) |
   | `--filesystem=xdg-config/cosmic`, `--talk-name=com.system76.CosmicSettingsDaemon` | Follow the COSMIC theme; what every installed COSMIC Flatpak here asks for |
   | `--socket=ssh-auth` | SFTP through the user's SSH agent |
   | `--talk-name=org.freedesktop.Notifications` | Failure notifications. The Notification portal is exported on COSMIC (through the gtk backend) and needs no permission; prefer it if click handling can move to the portal's actions |
   | `--system-talk-name=org.freedesktop.UPower`, `--system-talk-name=org.freedesktop.NetworkManager` | Scheduled-backup conditions (`src/conditions.rs`). The NetworkMonitor portal (exported on COSMIC) answers "metered" without a permission; battery level and trusted Wi-Fi still need these |
   | `--system-talk-name=org.freedesktop.UDisks2` | Removable drives, after ST-3 |

   **Not** `--talk-name=org.freedesktop.systemd1` or
   `--filesystem=xdg-config/systemd`: either one lets the app run arbitrary
   commands outside the sandbox, and reviewers will refuse it. ST-2 removes
   the need. The keyring needs no permission: `oo7::Keyring::new()`
   (`src/keyring.rs:62`) detects the sandbox and uses the Secret portal on
   its own (**Verify first** in a real Flatpak; S-8 notes its file backend
   was not reviewed).
3. Add a CI job that builds the Flatpak with `flatpak-builder` and runs
   `flatpak run --command=flatpak-builder-lint org.flatpak.Builder manifest`
   and `… repo`. Flathub counts linter warnings as failures.
4. Settings live in `~/.var/app/<id>/config` inside a Flatpak, so a user
   moving from the `.deb` starts empty. Document **Export Settings** then
   **Import Settings** as the move, in the README's install section.

**Verify.** `flatpak-builder --user --install --force-clean build
io.github.stldave314.Stellarshot.yml` builds offline; `flatpak run
io.github.stldave314.Stellarshot` backs up to a local folder, an SFTP server
and Google Drive, restores, remembers a password across a restart, and
follows a theme change. The linter passes with zero warnings.

---

### ST-2. Scheduling depends on the host's systemd

**High · L · Confirmed (code) · Decision** (which replacement; see the
COSMIC caveat under **Fix**)

**Files:** `src/timers.rs:29`, `:203`, `:219-267`, `:270`, `:351`, `:428-453`;
`src/notify.rs:104-121`, `:198-213`; `src/scheduled.rs`

**Problem.** Every automatic backup is a pair of unit files Stellarshot writes
into `~/.config/systemd/user` and drives with `systemctl --user`. The
on-connect trigger is a `.path` unit on `/dev/disk/by-uuid/<uuid>`. A
notification click is handed to `systemd-run --user`. Inside a Flatpak none
of this exists: the sandbox has no `systemctl` or `systemd-run`, cannot see
the host's unit directory, and `ExecStart` would name a path inside the
sandbox. Without a permission reviewers refuse (ST-1), scheduling is gone,
and scheduling is half of what a backup app is for.

**Fix.** Keep the systemd backend for native packages; add a second backend
for the sandbox, chosen at run time:

1. A `Scheduler` trait in `src/timers.rs` with the operations the rest of the
   code uses today (install, remove, next run). The existing code becomes
   `SystemdScheduler`.
2. `PortalScheduler`: a long-running `stellarshot --background` process that
   keeps its own timers (the schedule fields already in each profile), runs
   due backups through the same `scheduled::run` path, and watches UDisks2
   for on-connect profiles (ST-3). It asks for autostart through the
   Background portal (`org.freedesktop.portal.Background.RequestBackground`
   with `autostart: true` and `commandline: ["stellarshot", "--background"]`).
   This is how Pika Backup runs its scheduled backups on Flathub (its
   "Monitor" process; GNOME Help, "Scheduled Backups").

   **COSMIC caveat (Confirmed (test), 2026-10-16):** on this COSMIC session
   (xdg-desktop-portal 1.18.4, xdg-desktop-portal-cosmic 1.10.0) the portal
   frontend does **not** export `org.freedesktop.portal.Background`: neither
   `cosmic.portal` nor the `gtk.portal` fallback lists
   `org.freedesktop.impl.portal.Background`. Check it with
   `gdbus introspect --session --dest org.freedesktop.portal.Desktop
   --object-path /org/freedesktop/portal/desktop | grep Background`.
   So on COSMIC today a Flatpak cannot ask to autostart. Choose one before
   building this:

   - (a) Contribute a Background implementation to
     `pop-os/xdg-desktop-portal-cosmic` (or ask System76 for one), and ship
     scheduling in the Flatpak only once it is released. Cleanest; not in
     this project's control.
   - (b) Grant `--filesystem=xdg-config/autostart:create` and write the
     autostart entry (`Exec=flatpak run --command=stellarshot
     io.github.stldave314.Stellarshot --background`) directly, falling back
     to it only when the portal is missing. Works today; reviewers may
     object, so justify it with the missing portal.
   - (c) Ship the first Flatpak without unattended scheduling: due backups
     run while the window (or its background process, started by the user
     opening the app) is running, and the app says plainly that automatic
     backups need the native package. Honest, but weak for a backup app.

   Whatever the choice, handle `RequestBackground` failing or being denied
   with a visible message (Pika's users hit misconfigured portals often).
3. Pick the backend once at start: `Path::new("/.flatpak-info").exists()`
   means sandboxed. Put the check in one function; nothing else tests for
   Flatpak.
4. Notifications from the background process open the window through the
   window's single-instance D-Bus activation (already used by
   `App::dbus_activation`), not `systemd-run`.
5. Catch-up after the computer was off (`Persistent=true` today) becomes:
   at start, run anything whose last run is older than its interval.

**Verify.** In the Flatpak: schedule an hourly backup, log out and back in,
and confirm it runs on time and after a missed slot; the README's scheduling
section describes both backends. Native: the existing `tests/scheduled.rs`
suite still passes unchanged.

---

### ST-3. Removable drives are found through paths a sandbox hides

**High · M · Confirmed (code)**

**Files:** `src/drives.rs:29`, `:38-44`, `:98-99`; `src/timers.rs:203`

**Problem.** Drives are listed from `/proc/self/mountinfo` and
`/dev/disk/by-uuid`/`by-label`. In a sandbox, `mountinfo` describes the
sandbox's own mounts and `/dev/disk` is not there, so a USB backup drive is
never recognized, and the on-connect `.path` unit cannot exist (ST-2).

**Fix.** Read drives from UDisks2 over the system bus
(`org.freedesktop.UDisks2`, `GetManagedObjects`: block devices with
`IdUUID`, `IdLabel` and the filesystem's `MountPoints`). Keep the current
parser as the native fallback, behind one `drives::list()` that chooses. The
background process (ST-2) subscribes to `InterfacesAdded` to start on-connect
backups.

**Verify.** A unit test feeds a recorded `GetManagedObjects` reply and gets the
same `Drive` list the existing `mountinfo` tests expect. In the Flatpak, plug
in a drive that holds a backup: it is found, and an on-connect backup runs
once.

---

### ST-4. Mount as Folder, hooks and the password command need host access

**Medium · M · Decision**

**Files:** `src/engine/mount.rs:83-86`, `:115-125`; `src/hooks.rs:122`;
`src/password_command.rs:77`; `src/dejadup.rs:85`

**Problem.** Three features reach outside a sandbox:

- **Mount as Folder** needs `/dev/fuse` and `fusermount3`. Déjà Dup does it
  through the host's `fusermount` with `--talk-name=org.freedesktop.Flatpak`,
  which is a full sandbox escape that reviewers accepted for it.
- **Hooks** and the **password command** run whatever the user typed. In a
  sandbox they see the runtime's `/usr`, not the host's tools: a `pg_dump`
  hook or a `pass show …` password command fails with "not found".
- **Déjà Dup import** of a native install runs `dconf dump`, which the
  runtime does not ship. The Flatpak keyfile path still works.

**Decide** per feature: (a) grant `org.freedesktop.Flatpak` and run through
`flatpak-spawn --host` (it works, but the store will show the app as having
full system access, and reviewers may refuse it), or (b) ship it disabled in
the sandboxed build with an explanation. (b) is recommended for the first
submission.

**Fix for (b).** One `sandboxed()` check (shared with ST-2). Where it is true,
show each of these controls as unavailable with a one-line reason
(`fl!` in all five locales), and say so in the README's Flatpak section. Hooks
and the password command saved in an imported profile are already disabled on
import; keep them disabled.

**Verify.** In the Flatpak: each control is unavailable and says why; nothing
fails with "not found". Native build: unchanged.

---

### ST-5. The metainfo does not say this is a COSMIC application

**Medium · S · Confirmed (code)**

**Files:** `res/io.github.stldave314.Stellarshot.metainfo.xml:241-244`;
`scripts/validate-metadata.sh`

**Problem.** The COSMIC Store fills its **Made for COSMIC** section only with
components that provide `com.system76.CosmicApplication` (libcosmic's README;
`search_explore_page` in the store's `src/main.rs`). Stellarshot's
`<provides>` lists only its two binaries.

**Fix.** Add `<id>com.system76.CosmicApplication</id>` inside `<provides>`.
Add a check for it to `scripts/validate-metadata.sh`, so it cannot be dropped
later.

**Verify.** `scripts/validate-metadata.sh` passes, and fails with the line
removed.

---

### ST-6. The applet has no store identity, and no route today

**Medium · M · Confirmed (code)** · Partly blocked upstream

**Files:** `res/io.github.stldave314.Stellarshot.Applet.desktop`;
`src/app/applet.rs:34`

**Problem.**

1. The store's **Applets** category lists only components that provide
   `com.system76.CosmicApplet`. The applet has a desktop file
   (`X-CosmicApplet=true`) but no metainfo.
2. Flathub rejects applets, so the applet must go to the COSMIC Flatpak
   repository.
3. **Upstream blocker:** Flatpak 1.18.4 strips `X-CosmicApplet` from exported
   desktop files (a security fix), so third-party Flatpak applets disappear
   from COSMIC Settings' applet list. Tracked in `pop-os/cosmic-panel#694`,
   open as of 2026-10-01 with no fix landed; System76 plans a Flatpak change.
   This machine runs Flatpak 1.18.1, which still works.
4. **Verify first:** the applet reads backup status from the app's state
   directory and lock directory. If the applet ships as its own Flatpak, its
   `~/.var/app` directories are not the app's, and it will show nothing.

**Fix.**

1. Add `res/io.github.stldave314.Stellarshot.Applet.metainfo.xml` with
   `<provides><id>com.system76.CosmicApplet</id></provides>`, a summary,
   a screenshot of the panel popup, and the same release list. Extend
   `validate-metadata.sh` to cover it. Install it in all three packages.
2. Answer item 4 before writing the applet's manifest: either ship both
   binaries in one Flatpak (the applet's desktop ID already starts with the
   app ID, which Flatpak requires for an export), or give the applet
   read-only access to exactly the app's state paths. Write the answer here.
3. Submit to `pop-os/cosmic-flatpak` only after `cosmic-panel#694` is fixed,
   and after the app is on Flathub (that repository depends on Flathub).

**Verify.** `appstreamcli validate` passes on the new file; after install, the
applet appears in COSMIC Settings' applet list and shows the app's real
status.

---

### ST-7. The app is described as COSMIC-only

**Medium · S · Verify first**

**Files:** `README.md:68-70` ("COSMIC desktop (Pop!\_OS 24.04 or newer, or
any COSMIC session)"); metainfo description, first paragraph

**Problem.** Flathub turns down "apps requiring a specific desktop
environment," with exceptions case by case. libcosmic apps run on other
Wayland desktops, and only the applet is COSMIC-specific. The app's own
requirements text invites the rejection.

**Fix.** Run the window on GNOME (Wayland) and KDE Plasma (Wayland): back up,
restore, remember a password, and close and reopen the window. Fix or
document what breaks. Then reword the requirements as "Designed for COSMIC;
also runs on other Wayland desktops. The panel applet needs COSMIC." Keep
"for the COSMIC desktop" in the metainfo; it is accurate and is what the
store's COSMIC section is about.

**Verify.** The test above, written into `VALIDATION.md`'s manual checks with
the date.

---

## 2. Security

### S-1. Restore accepts children of a non-directory node

**High · S · Confirmed (code); the escape chain is traced, not run**

**Files:** `src/engine/restore.rs:612-650` (`reject_unsafe_name`,
`check_walked`), `:728-756` (the walk); `src/engine/browse.rs:486-488`
(`archive_folder`), `:569-571` (`search`), `:730-732` (`missing`)

**Problem.** The 2026-09-30 fix B-2 had two parts. Part 1 (each walked name is
a plain name) landed as `check_walked`. Part 2 ("refuse any later item whose
path has a non-directory item as a strict prefix") did not. rustic's tree
walker descends into any node that has a `subtree`, whatever the node's type
(`rustic_core` 0.13.0, `src/blob/tree.rs:589-601`), so a crafted snapshot can
give a **symlink** node children. Each child passes `check_walked`.

Traced through rustic's restore order (contents first, then metadata in walk
order):

- Symlink `a` → `~/.config/autostart`, carrying a hard-link child
  `a/evil.desktop` that shares an inode with an ordinary file `z`. The
  metadata pass creates the symlink, then `hard_link(z, a/evil.desktop)`
  creates parents through it: an attacker's file lands in the user's
  autostart folder.
- Without hard links: a fifo or device child named after an existing file
  (`a/authorized_keys` under a symlink to `~/.ssh`) gets rustic's
  `set_permissions`, which follows the symlink: an attacker-chosen chmod.

`writes_through_a_symlink` cannot see either, because the symlink does not
exist on disk until the restore creates it. Anyone who can write to a
repository the user restores from can do this; Stellarshot supports shared
repositories.

**Fix.** A guard that tracks which directories are open on the current path,
fed every walked item, called from `check_walked` so every walk gets it:

```rust
/// The names of the directory items open on the current walk path. An item
/// whose parent is not one of them is a child of a non-directory node,
/// which rustic walks into but which must never be acted on.
pub(super) struct Nesting(Vec<std::ffi::OsString>);

impl Nesting {
    pub(super) fn check(&mut self, relative: &Path, node: &Node) -> Result<(), EngineError> {
        let depth = relative.components().count();
        if depth == 0 {
            return Ok(());
        }
        if self.0.len() < depth - 1 {
            return Err(EngineError::new(ErrorKind::UnsafePath, relative.display().to_string()));
        }
        self.0.truncate(depth - 1);
        if node.is_dir() {
            self.0.push(node.name().into_owned());
        }
        Ok(())
    }
}
```

Also, in `reject_unsafe_name`, refuse a NUL byte and names over 255 bytes
(both reach `unwrap()`s inside rustic's restore and crash the `--run` child),
and have `Nesting` refuse a name equal to the previous sibling's.

**Verify.**

1. Unit tests with `Node::new_node`, like the existing `check_walked` tests:
   `[symlink a, file a/x]` → `UnsafePath`; `[dir d, file d/f, symlink a,
   file b]` → ok; `[dir a, symlink a, file a/x]` → `UnsafePath`;
   `"a\0b"` and a 256-byte name → `UnsafePath`. Negative control: remove the
   call, watch them go red.
**Done (2026-10-16):** `restore::Walk` is used by every walk (both restore
passes, `archive_folder`, `search`, `missing`); the unit tests above pass and
go red with both of its checks disabled (negative control); the full restore
and engine suites still pass. **Not done:** the end-to-end proof in item 2;
rustic's public API cannot write such a tree, so it needs a hand-built
repository fixture.

2. **Required before closing:** one end-to-end proof. Build a repository by
   hand in a scratch directory holding the symlink-with-children tree (it
   needs rustic's tree and pack writers directly, not `backup`), restore it
   into a new empty folder, and confirm with `find` and `stat` that nothing
   outside that folder exists or changed. Keep it as a standing test that
   fails, not skips, if its fixture cannot be built.

---

### S-2. rclone credentials are never removed from Stellarshot's `rclone.conf`

**Medium · M · Confirmed (code)**

**Files:** `src/engine/rclone.rs:580-587` (`delete_remote`, no callers),
`:410-426` (`copy_user_remote`), `:469-521` (`sign_in`);
`src/app/wizard/place.rs:430-440` (`PickRemote`);
`src/app/profiles.rs:119-173` (`remove_profile`)

**Problem.** Every Google sign-in and every remote clicked in the wizard
writes a `[stellarshot-xxxxxxxx]` section into
`~/.config/stellarshot/rclone.conf`: OAuth tokens, a custom client secret,
or a plaintext copy of a remote's credentials from the user's own rclone
config (plaintext even when the user's config is encrypted). Nothing ever
deletes one: not removing the backup, not Delete everything, not discarding
the wizard, not clicking a different remote. The removal dialog says the
backup "and its saved password" are forgotten; the credentials stay.

**Fix.**

1. `fn orphaned_remotes(config_text: &str, profiles: &[Profile]) -> Vec<String>`:
   every `stellarshot-…` section that no `Destination::Rclone` names.
2. Call `delete_remote` for each: after `remove_profile`, after a wizard
   discard, when `PickRemote` replaces an earlier copy, and once at start.
3. `SECURITY.md`, "Cloud sign-ins stay in Stellarshot's own file": say the
   section is removed with the backup.

**Verify.** Unit test `orphaned_remotes` with three sections and two
profiles. Engine test: `append_section` then `delete_remote` leaves the file
at mode 0600 with the section gone; it fails, not skips, without rclone.

---

### S-3. Closing the window leaves an invisible process holding passwords

**Medium · M · Confirmed (code)**

**Files:** `src/app/startup.rs:66` (`exit_on_close(false)`);
`src/app/update.rs:170-177` (`WindowClose`); `src/app/pages/profile.rs:108`
(`secret: Option<Secret>`); `README.md:521-529`

**Problem.** Closing the window always keeps the process running ("minimizes
to the panel"), whether or not the applet is on the panel. Without the
applet, which is the default and the only option on Flathub (ST-6), nothing
on screen shows the app is still running, and every backup the user unlocked
keeps its password in memory until logout. Store reviewers flag apps that
keep running after their window closes with nothing visible.

**Fix.**

1. On `WindowClose`, drop every page's `secret` (lock the backups) unless that
   page is busy, in which case drop it when the operation ends.
2. Quit on close when nothing is running **and** the applet is not on a
   panel. Find whether it is by reading COSMIC's panel configuration
   (`com.system76.CosmicPanel.Panel`/`…Dock`, the `plugins_center` and
   `plugins_wings` entries) through `cosmic-config`, and treat "cannot tell"
   as "not on the panel". Inside a Flatpak, the background process (ST-2)
   already does the watching, so quit the window.
3. README: describe the new rule where it describes closing today.

**Verify.** A unit test of the decision function (busy × applet present ×
sandboxed). By hand: with no applet, close the window and confirm with
`pgrep -f /usr/bin/stellarshot` that nothing is left; with the applet, the
process stays and reopening asks for the password again.

---

### S-4. Opening an existing repository adopts its folders unseen

**Medium · S · Confirmed (code)**

**Files:** `src/app/tasks.rs:258-261`; `src/app/wizard/mod.rs:60` (`Mode::Open`
has no **What** step)

**Problem.** Opening an existing repository sets the new backup's folders to
the paths of the newest snapshot from **any** computer. A repository someone
else shares with the user can name `~/.ssh`, and the user's next backup
(scheduled, if they pick a schedule) uploads it where the other person can
read it. The paths are not shown before the backup exists.

**Fix.** Prefer the newest snapshot whose hostname is this computer's; show
the inherited folders for review (a prefilled **What** step in `Mode::Open`)
before finishing; cap the number of inherited paths
(`OPEN_INHERITED_PATHS_LIMIT` in `constants.rs`).

**Verify.** Extend `finishing_open_takes_the_folders_from_the_latest_snapshot`:
a newer snapshot from another hostname is not preferred, and more paths than
the cap are cut to it.

---

### S-5. A SIGTERM during the After hooks kills the running After hook

**Medium · S · Confirmed (code)**

**Files:** `src/proc_signal.rs:172-181` (`on_term`); `src/hooks.rs:29-40`,
`:55-69` (`run_before`); `src/runner.rs:347-359`

**Problem.** `on_term` calls `hooks::kill_running()` before it looks at
`armed`. If Cancel, logout or `systemctl --user stop` arrives while an After
hook is running ("start the database again"), that hook's whole process group
is SIGKILLed; the later After hooks still run, but the one that matters was
cut off. Second, narrower: a SIGTERM between two Before hooks starts the After
hooks on the signal thread while `run_before`, which never checks
`terminating`, starts the next Before hook on the main thread.

**Fix.** In `on_term`, take `armed` first and call `kill_running()` only if it
was `Some` (once After hooks are claimed, `armed` is `None`). Make
`run_before` check `state().terminating` before each spawn.

**Verify.** `tests/runner.rs`: a job whose After hook is `sh -c 'sleep 3;
touch marker'`; SIGTERM the `--run` child while it sleeps; assert `marker`
exists. It fails today.

---

### S-6. Imported local and removable paths are not validated

**Low · S · Confirmed (code)**

**Files:** `src/settings_export.rs:253-261` (`remote_is_safe` returns `true`
for `Local` and `Removable`); `src/profile.rs:157-164`;
`src/engine/repo.rs:228-235`; compare `src/app/wizard/place.rs:188-192`

**Problem.** The wizard refuses a removable path that leaves the drive
(`is_plain_folder`); an imported settings file is not checked at all. A
relative `Local` path such as `rclone::sftp,…` would be read by rustic as an
rclone backend (the SEC-1 hole by another door). Today `check_reachable`
stops it by accident, because no such relative folder exists.

**Fix.** In `merge`, reject a `Local` path that is not absolute, and a
`Removable` relative path with any component other than `Normal`. Also make
`Destination::location()` return an error for both, so the rule holds for a
hand-edited profiles file.

**Verify.** `merge` rejects `Local { path: "rclone::x" }`,
`Removable { relative_path: "../../home/x" }` and `"/etc"`;
`location()` on a hand-built profile returns `Err` without starting any
process.

---

### S-7. Downloads get the umask mode, not a private one

**Low · S · Confirmed (code)**

**Files:** `src/engine/browse.rs:261-271` (`write_atomically`), used by
`dump_file` (`:458`) and `archive_folder` (`:481`); compare
`src/app/tasks.rs:173-185`

**Problem.** `AtomicFile::write` creates its file with default options, so a
downloaded 0600 key file, or a `.tar.gz` of a private folder, lands as 0644
under the usual umask.

**Fix.** Use `write_with_options` with `.mode(0o600)`, as the settings export
already does.

**Verify.** With the process umask at 022, download a 0600 file and a folder;
both outputs are 0600. Set the umask explicitly in the test (see the
umask-dependent fixtures note in `VALIDATION.md`).

---

### S-8. Secret hygiene gaps

**Low · M · Confirmed (code)**

None of these leaks today (no `{:?}` reaches them); each is one careless
`debug_log!` away from leaking.

| Where | What | Fix |
|---|---|---|
| `src/profile.rs:531-539`, `:707-715` | The REST password is put back into `Location::Rest.url`, a `Clone + Hash + Serialize` `String`, for the whole session | `Location::Rest { url, password: Option<Secret> }`; compose the URL only in `backend_options` |
| `src/app/dialog.rs:74-79`; `src/app/effects/dialog.rs:58`, `:146`, `:155` | `Dialog::ChangePassword { password, confirm }` is plaintext, `Debug`, and cloned | Hold `Secret`s; hand-written `Debug` |
| `src/app/wizard/place.rs:64-65`, `:110`, `:132-135`, `:704-709` | The Google client secret is plaintext in `Place`, `Message`, `Effect`, all `derive(Debug)`, and its field is not masked | `.secure(true)` on the field; `Secret`; hand-written `Debug` |
| `src/engine/rclone.rs:166`, `:493-505` | The client secret is passed in rclone's environment, which `xdg-open` and a newly started browser inherit | **Verify first**, then write the client ID and secret into the private config section before `rclone config reconnect`, with nothing in the environment |
| `src/engine/serve.rs:31-37` | `Serve` derives `Debug` with the one-off `user:pass@` URL | Hand-written `Debug` |
| `src/engine/repo.rs:338-354` | `scrub_url_credentials` stops at the first `@` and skips a word with `/` before the `@`, so a password containing `@` or `/` leaks a tail | Take the last `@` before the first `/` after `://`; redact the whole word when unsure |

**Verify.** `format!("{:?}", …)` of each type with `"hunter2"` in it does not
contain `hunter2`; `scrub_url_credentials` on `http://a:p@ss@h/x` and
`http://a:p/ss@h/x` contains no `ss`; the existing stand-in-rclone test in
`tests/rclone_credentials.rs` asserts the secret is absent from the child's
environment.

---

### S-9. Smaller hardening items

**Low · S · Confirmed (code)**

1. **FUSE mount options** (`src/engine/mount.rs:83-86`). Only `RO` and
   `FSName` are set; `nosuid,nodev` come from fuser's defaults and there is
   no `noexec`, so a foreign snapshot's "executable" file is one click from
   running. Add `MountOption::NoExec`, `NoSuid`, `NoDev` explicitly. Verify:
   the mount's line in `/proc/self/mountinfo` has all three.
2. **`trusted_executable` ancestors** (`src/timers.rs:241-267`) check only
   `mode & 0o022`, not the owner: a 0755 directory owned by a third user
   passes. Require each ancestor's uid to be 0 or the current user. Verify
   with a mocked uid lookup.
3. **`condition_path`** (`src/timers.rs:93-99`) accepts a trailing `\`, which
   systemd reads as a line continuation. Reject it. Verify with a unit test.
4. **`exclude_pattern_files` from an import** (`src/engine/backup.rs:158-169`)
   is read with an unbounded `read_to_string`; `/dev/zero` or a FIFO hangs or
   exhausts the backup. Require `is_file()` and read through `.take(1 MiB)`.
   Verify: pointing it at `/dev/zero` fails at once.
5. **Tar headers** (`src/engine/browse.rs`). setuid/setgid are dropped only
   because `tar` truncates the Go-style mode. Apply
   `map_mode_from_go(mode) & 0o777` explicitly so it does not depend on that.

---

## 3. Bugs and robustness

### R-1. The orphan sweep kills the rclone of a live backup

**High · S · Confirmed (test)**

**Files:** `src/engine/serve.rs:193`, `:224-237` (`parent_is_stellarshot`);
`src/exe.rs:39-41` (`running_image`); `src/app/child.rs:123`, `:152`;
`src/app.rs:640`; `src/app/effects/restore.rs:349-352`

**Problem.** `stop_orphans` stops any `rclone serve restic` using
Stellarshot's config whose parent's `/proc/<ppid>/comm` is not `stellarshot`.
The window starts every `--run` child through `/proc/self/exe`, and the
kernel names a process after the basename of the path it was executed
through, so the child is named **`exe`**. Confirmed on this machine:

```sh
python3 -I -c 'import os; os.execv("/proc/self/exe", ["python3","-I","-c",
  "print(open(\"/proc/self/comm\").read())"])'     # prints: exe
```

So every SFTP, cloud or REST backup, restore or clean-up started from the
window looks orphaned. The sweep runs at every window start and **every time
the restore page closes**: browsing one backup's snapshots and closing the
restore page kills the rclone under another backup running to Google Drive,
which then fails. Scheduled runs are not affected (systemd executes the real
path).

**Fix.** Identify the parent by its binary: `std::fs::read_link(
"/proc/<ppid>/exe")`, strip a trailing `" (deleted)"` (reuse
`exe::strip_deleted_marker`), and compare the file name with `stellarshot`.
Keep returning `true` (owned, leave it alone) when the link cannot be read.

**Verify.** A test beside `a_left_over_serve_is_stopped`: copy `python3` to
`<tmp>/stellarshot`, start it through a symlink `<tmp>/exe` (comm `exe`, exe
link `stellarshot`), have it start a grandchild whose argv is `rclone … serve
restic --config <cfg>`; assert `stop_orphans(&cfg) == 0` and the grandchild
is alive. It returns 1 today. By hand: start a cloud backup, open and close
the restore page, and the backup completes.

---

### R-2. A future-dated snapshot makes "Keep for N days" forget everything

**High · S · Confirmed (code)**

**Files:** `src/engine/maintenance.rs:270-292` (`forget`); `rustic_core`
0.13.0 `src/commands/forget.rs:544-546`, `:589`; `src/app/tasks.rs:258-261`;
`src/app/pages/profile.rs:983-990`

**Problem.** rustic measures "keep within N days" from the **newest snapshot
in the group** (`latest_time = snapshots[0].time`), not from now. One snapshot
dated in the future (a backup taken once with the clock wrong, e.g. a dead
RTC battery, or a crafted one in a shared repository) becomes the "latest",
every real snapshot falls outside the window, and the next Clean Up or
scheduled prune forgets them all. The same snapshot makes the status card say
"backed up just now" forever and is copied into `last_success`, so the
overdue notification never fires either.

**Fix.** In `forget`, leave out of the grouping any snapshot with `time > now +
FUTURE_SNAPSHOT_TOLERANCE` (one day, in `constants.rs`): such snapshots are
kept, never forgotten, and never define the window. Ignore future times in the
status card, `finish` and run-state logic, as `ago` already does.

**Verify.** Engine test: three real snapshots and one dated 2099 (via
`BackupRequest.time`), all with the profile tag; `forget(KeepRules {
within_days: Some(30), .. })`; the three real ones survive. Red today.

---

### R-3. Search everywhere can exhaust memory

**High · S · Confirmed (code)**

**Files:** `src/engine/browse.rs:594-636` (`search_all`);
`src/app/pages/restore.rs:800-809` (`GlobalSearchNow`), `:1591-1620`
(`global_match_row`); `src/app/effects/restore.rs:119-127`

**Problem.** `search_all` collects every match in every snapshot, builds a
`GlobalMatch` for each path, and clones a full `SnapshotSummary` (paths,
hostname, notes) for every path × snapshot pair, and only then truncates to
`limit`. A two-letter search of a home-folder backup with 100 snapshots is
tens of millions of clones, in the window process. The UI then draws one
button per snapshot per row, uncapped. No hostile input needed.

**Fix.**

1. Truncate the sorted `by_path` to `limit` before building any `GlobalMatch`.
2. Hold `(id, time)` per snapshot instead of a cloned `SnapshotSummary`, and
   keep at most `GLOBAL_MATCH_SNAPSHOTS` (e.g. 20) plus a count.
3. Draw at most that many chips and an "and N more" caption (`fl!`, five
   locales).
4. Require at least 3 characters in `GlobalSearchNow`.
5. Count matches inside the `find_matching_nodes` closure with an
   `AtomicUsize`, stop past `limit * 20`, and show a "narrow your search" hint.

**Verify.** A fixture of 30 snapshots × 3,000 files named `file-N.txt`;
`search_all("file", 50)` returns at most 50 matches, each with at most the
chip cap. A unit test for the chip-selection helper.

---

### R-4. A password change can leave the old password in the keyring

**Medium · S · Confirmed (code)**

**Files:** `src/app/effects/dialog.rs:220-234`; `src/keyring.rs:174-176`

**Problem.** After the repository password changes, the keyring is updated
only `if keyring::load(&id).await.is_some()`. `load` turns every error into
`None`: a locked keyring, a dismissed unlock prompt, a timeout. Then the old
password stays in the keyring, `KeyringUpdateFailed(None)` shows nothing, and
the next scheduled backup fails with a wrong password. A keyring locked at
boot makes this the normal case, not a corner case.

**Fix.** Use `load_checked`: on `Err(e)` return it, so the existing "could not
update the keyring" dialog appears; skip the store only on `Ok(None)`.

**Verify.** Factor the decision into
`fn keyring_action(loaded: Result<Option<Secret>, String>) -> Action` and test
that `Err` maps to "report", `Ok(None)` to "nothing", `Ok(Some(_))` to
"store".

---

### R-5. A closed stderr can hang a backup holding the repository lock

**Medium · S · Confirmed (code)**

**Files:** `src/debug.rs:255-258` (`error` uses `eprintln!`);
`src/proc_signal.rs:132-143`, `:186`, `:213`; `src/runner.rs:309`, `:636`

**Problem.** `eprintln!` panics when the write fails, and Rust ignores SIGPIPE,
so a closed pipe returns EPIPE. A `--run` child's stderr is a pipe to the
window, and **Quit** while busy lets the child outlive the window. If the
child then logs an error: at `runner.rs:636` it exits 101 and the error event
is lost; inside `on_term` (`proc_signal.rs:186`) the signal thread dies before
`_exit(143)`, the main thread parks forever in `wait_if_terminating`, and the
repository lock and rclone stay held. Every later scheduled run is skipped as
"locked" until logout.

**Fix.** In `debug::error`, `let _ = writeln!(std::io::stderr(), …);` so a
failed write is ignored. Also wrap `on_term`'s body in `catch_unwind` and call
`_exit` regardless.

**Verify.** `tests/runner.rs`: `--run backup` with its stderr read end dropped,
a Before hook that sleeps and an After hook of `false`; SIGTERM; assert exit
status 143 within 10 s. Negative control: put `eprintln!` back.

---

### R-6. Snapshot lists are rendered without a cap

**Medium · S · Confirmed (code)**

**Files:** `src/app/pages/profile/snapshots.rs:234-288` (`list_view`, loop at
`:243`); `src/engine/snapshots.rs:84-91` (`matches`);
`src/app/pages/restore.rs:1326-1375` (`versions_view`, loop at `:1336`)

**Problem.** The Snapshots page builds a row with two icon buttons for every
snapshot on every `view()`, and the filter lowercases every snapshot's name,
notes and tags on every frame. **Versions** draws a five-widget row per
version. With the default "keep forever" and an hourly schedule that is
about 8,700 snapshots a year; a shared repository has more.

**Fix.** `SNAPSHOT_ROW_LIMIT` (e.g. 200) in `constants.rs`;
`fn visible<'a>(snapshots: &'a [SnapshotSummary], query: &str, limit: usize)
-> (Vec<&'a SnapshotSummary>, usize)` returning the rows and how many are
hidden, shown as "N more; narrow the filter". Cap `versions_view` at
`RESULT_LIMIT` the same way. Lowercase the search text once when snapshots
load. Truncate long names, hostnames and notes for display.

**Verify.** Unit test `visible` with 5,000 snapshots: at most the limit, and
the right remainder.

---

### R-7. Bound everything a repository or a helper process controls

**Low · M · Confirmed (code)**

1. **Tar download pads to the declared size** (`src/engine/browse.rs:284-314`,
   `:516-529`). `BlobReader` fills zeros up to `node.meta.size`, which comes
   from the repository; a huge declared size with a tiny body writes until the
   disk is full, uncancelable, in the window process. Sum the node's blob
   lengths from the index and fail the entry if the declared size exceeds it by
   more than 64 MiB. Verify with a 10-byte reader declaring 2^40 bytes.
2. **No cap on walked entries.** A tree that references the same child twice
   per level is tiny on disk and yields 2^40 entries. `missing_where`,
   `archive_folder`, `search` and the restore walk have no limit
   (`diff_trees` already has `TREE_MAX_DEPTH`). Add `TREE_MAX_ENTRIES`,
   count in each loop, stop with an error. Verify with a synthetic iterator.
3. **Helper processes without a timeout:** `rclone version`
   (`src/engine/rclone.rs:254-259`), `systemctl` (`src/timers.rs:270-273`,
   `:351-354`), `systemd-run` (`src/notify.rs:111-119`, `:206-212`; these run
   inside a scheduled run, so a wedged user manager stalls it for hours),
   `dconf dump` (`src/dejadup.rs:85-88`, every window start), `fusermount -u`
   (`src/engine/mount.rs:115-125`). Route them through a bounded runner
   (`rclone::run_bounded` and `hooks::wait_with_timeout` are the models).
   Verify with a `sleep 300` stub first on `PATH`.
4. **Settings import has no profile-count cap**
   (`src/settings_export.rs:155-213`). A file with tens of thousands of tiny
   profiles freezes the UI. Count anything over a cap (e.g. 200) as rejected.
   Verify by merging 1,000.

---

### R-8. Process lifetime gaps

**Low · M · Confirmed (code)**

1. **`wait()` after SIGKILL is unbounded** (`src/hooks.rs:198-199`,
   `src/engine/rclone.rs:230-231`, `src/engine/serve.rs:115-116`,
   `:160-162`). A process stuck on a hung NFS or FUSE mount ignores SIGKILL
   until the I/O returns, holding the repository lock meanwhile. One helper,
   `reap_within(&mut Child, Duration) -> bool`, polling `try_wait` for
   `DRAIN_AFTER_EXIT`, for all four sites. Verify with a stub whose
   `try_wait` never returns a status.
2. **Hooks have no parent-death signal** (`src/hooks.rs:122-131`). Hooks run in
   their own process group, so if the `--run` child is SIGKILLed a running
   hook is orphaned, unbounded, and no After hook runs. In `--run` and
   `--scheduled` only, add a `pre_exec` with
   `rustix::process::set_parent_process_death_signal(Some(Signal::KILL))`.
   **Not** for rclone started from the window: the death signal follows the
   spawning thread, and `tasks::blocking` threads exit when idle.
3. **`kill(0, SIGTERM)` assumes the process leads its group**
   (`src/proc_signal.rs:211-214`). Run from a script, it signals the script's
   whole group. Only call it when `getpgrp() == getpid()`.

---

### R-9. Smaller robustness items

**Low · S · Confirmed (code)**

1. **A 0755 runtime directory left by an older version is refused**
   (`src/engine/lock.rs:288-314`), failing every backup until logout.
   `paths::tighten_private` already repairs this safely (owned by us, not a
   symlink → chmod 0700); use it, and merge the two near-duplicate helpers.
2. **Keyring details** (`src/keyring.rs:44`, `:66`, `:130`). Remember a
   dismissed unlock prompt for ~30 s so queued callers fail fast instead of
   each raising another prompt; after a successful `store`, delete duplicate
   items; log when more than one item matches.
3. **`ConfigChanged` drops a live mount on the UI thread**
   (`src/app/update.rs:290-298`). Take `page.mounted` first and send it
   through `unmount_off_thread`.
4. **A copy of an unreadable profiles file is made on every start**
   (`src/app.rs:242-270`, `:581-585`). Skip it when an identical copy
   exists, or keep the newest three.

---

## 4. Store listing, packaging and documentation

### M-1. The store summary is too long

**Medium · S · Confirmed (code)**

**Files:** metainfo `<summary>` and its four translations;
`tests/desktop_l10n.rs`

**Problem.** "Back up your files with encrypted snapshots" is 43 characters.
Flathub's quality guidelines: ideally 10–25, never over 35; sentence case, no
trailing period, preferably starting with an imperative verb, no technical
terms.

**Fix.** For example "Back up and restore your files" (30). Update all four
translations in the same change; `tests/desktop_l10n.rs` keeps them in step.
Keep "encrypted" and "snapshot" in `<keywords>` (already there) and the
description.

**Verify.** `tests/desktop_l10n.rs` and `validate-metadata.sh` pass. Add a
length check (≤ 35) to `validate-metadata.sh`.

---

### M-2. Screenshots show a temporary path and an empty state

**Medium · S · Confirmed (code)**

**Files:** `scripts/screenshots.sh:49` (`mktemp -d /tmp/stellarshot-demo.XXXXXX`);
`docs/screenshots/*.png`; metainfo `<screenshots>`

**Problem.** The default screenshot (`profile.png`) shows the destination as
`/tmp/stellarshot-demo.Qf7jhK/backups/alex-home`. The fourth is the empty
first-launch screen; Flathub's guidelines say screenshots "must not show empty
states." The images are 1400×1162, inside the 2000×1400 HiDPI limit.

**Fix.** Make the demo destination read like a real one (run the demo with a
destination such as an SFTP remote or a removable-drive path, or under a
mount namespace that shows `/media/alex/Backup`); drop `empty.png` from the
metainfo (keep it for the README); regenerate; inspect every image.

**Verify.** No `/tmp` in any image; `validate-metadata.sh` still passes;
`VALIDATION.md`'s screenshot row updated with the date.

---

### M-3. Smaller listing items

**Low · S**

1. The description's last paragraph ("a fork of the cosmic-utils Stellarshot
   project and is under active development") reads as unfinished to a store
   visitor, and Flathub turns away apps that look incomplete. Keep the credit
   in the README's **Credits**; end the description on what the app does.
2. Add `<url type="help">` (the README), `<url type="translate">` and
   `<url type="contribute">` (`CONTRIBUTING.md`), and `<requires><display_length
   compare="ge">360</display_length></requires>` with
   `<recommends><control>touch</control></recommends>` only if touch is
   tested.
3. Have the icon checked against Flathub's icon guidelines (optional; it
   affects featuring, not acceptance): its base layer is the same shape
   offset 4 px down, which can read as a baked-in shadow, and the dark
   `#252525` body sits against the edges on a dark background.
4. Run `flatpak-builder-lint appstream` on the metainfo once ST-1 exists; it
   is stricter than `appstreamcli` and its warnings count as failures.

---

### P-1. Two unsound advisories are not recorded in `deny.toml`

**Low · S · Confirmed (test)**

**Files:** `deny.toml`

**Problem.** `cargo audit` reports `lru` 0.16.4 (RUSTSEC-2026-0253, via
`cryoglyph` → `iced_wgpu`) and `memmap2` 0.8.0 (RUSTSEC-2026-0186, via
`xkbcommon` → `iced_winit`), both through libcosmic. `cargo deny` passes only
because it treats "unsound" as a warning. The 2026-09-30 status table (B-6)
says the unsound advisories are listed in `deny.toml` with their reason; these
two are not.

**Fix.** Add both to `[advisories] ignore` with a reason, as the three
unmaintained ones are. Check whether a newer libcosmic revision drops them,
and if so open the upgrade as its own task (libcosmic is pinned deliberately;
see `dependabot.yml`).

**Verify.** `cargo deny check advisories` prints no warning for either.

---

### D-1. Documentation corrections

**Low · S**

1. `SECURITY.md`: after S-2, say rclone sections are removed with their backup;
   add a "Flatpak build" paragraph once ST-1 lands (what the sandbox does and
   does not protect when `--filesystem=host` is granted, and which features
   are unavailable per ST-4).
2. `README.md`: an install section for Flathub and the COSMIC Flatpak
   repository, the settings move (ST-1 item 4), the close-window rule (S-3),
   and requirements (ST-7).
3. `ROADMAP.md`, "After 1.0": Flatpak is now planned work; point here.
4. `docs/plans/2026-09-30-release-review-tasks.md`, B-6 row: correct it per
   P-1.

---

## Roadmap to submission

Do the phases in order. Within a phase, tasks are independent unless noted.

**Phase 0: decisions (maintainer).** ST-0, the scheduling choice in ST-2
(COSMIC has no Background portal today), and the choices in ST-4. Nothing
below is submitted until ST-0 is answered.

**Phase 1: fix what a reviewer or a user would hit.** R-1, R-2, R-3, S-1 (with
its end-to-end proof), then R-4, R-5, R-6, S-2, S-4, S-5. Release as 0.11.0
through the normal tag workflow.

**Phase 2: listing metadata.** ST-5, M-1, M-2, M-3 items 1–2, P-1, ST-7. These
also improve the native packages' listing in GNOME Software and KDE Discover.

**Phase 3: sandbox-ready architecture.** ST-2 (the largest task), ST-3, S-3,
and ST-4 as decided. Native packages must behave exactly as before; the full
existing suite is the check.

**Phase 4: the Flatpak.** ST-1. Write the manifest by hand. Build, lint, and
run the ST-1 **Verify** list on COSMIC, then the ST-7 test on GNOME and KDE
inside the Flatpak. Add the CI job. Then the remaining Phase 1 lows (S-6 to
S-9, R-7 to R-9) if not already done.

**Phase 5: submit the app to Flathub.** Follow Flathub's submission guide
(a pull request against the `new-pr` branch of `flathub/flathub`, with the
manifest and a working screenshot link). Prepare the permission
justifications from the ST-1 table for the review thread. After acceptance,
verify the app (`io.github.stldave314` through the GitHub account), which the
listing shows as a checkmark. It then appears in the COSMIC Store through the
Flathub source, and in **Made for COSMIC** through ST-5.

**Phase 6: the applet.** ST-6, once `pop-os/cosmic-panel#694` is fixed. Submit
to `pop-os/cosmic-flatpak`. It then appears in the COSMIC Store's
**Applets** category.

**Every release after that:** the metainfo release entry and screenshot URLs
(already enforced by `validate-metadata.sh`), and a pull request against the
Flathub repository bumping the source tag and regenerating
`cargo-sources.json`. The release workflow can open that pull request itself;
make that a task once the Flathub repository exists.

---

## Checked and sound

Recorded so the next review knows what was covered.

**Processes.** No shell anywhere: hooks, the password command and rclone
command strings go through `shell_words::split` and `Command::new`. `--`
precedes every positional rclone target, `fusermount -u` and `systemd-run`.
Imported rclone remotes must match `stellarshot-[0-9a-f]{8}` and exist in the
private config; REST URLs must be http(s); SFTP host and user are quoted for
rclone's connection-string rules. Hooks, the password command and rclone run
in their own process groups and are group-killed on timeout; output readers
are capped. `rclone serve` must announce a loopback address. The `--run` job
on stdin is capped at 16 MiB, read into zeroizing buffers, after
`PR_SET_DUMPABLE=0`; a truncated write is a parse error, never a partial job.

**Systemd units.** `ExecStart` escapes `\`, `"`, `%` and `$`; newlines and
non-UTF-8 paths are rejected; IDs and UUIDs are `[A-Za-z0-9-]{1,64}`; unit
names all start with `stellarshot-backup-`; files are written atomically; the
binary must not be anywhere another user can write, `/tmp` included.

**Secrets.** `Secret` wraps `secrecy::SecretString` (zeroized, redacted
`Debug`); the only serialization is the child's stdin. The password never
reaches argv or the environment. rustic's REST errors carry URLs reqwest has
already stripped of credentials. Exports clear the password command, redact
REST URLs, are written atomically at 0600, and on import force the schedule
to manual and hooks off. `allow_unlock_prompt` is set once before any thread.
Keyring items are namespaced by `application` and `profile=`/`rest-server=`,
IDs are validated, and removing a backup deletes both kinds.

**Filesystem.** Restore refuses `..`, absolute paths, `/` inside a name, and
empty names in both passes, and in list, mount, diff and search; writing
through a symlink already on disk is refused; setuid and setgid are masked.
Tar downloads skip device nodes, fifos and sockets and copy hard links. Open
Copy uses a 0700 directory and a 0400 file. Locks are OFD locks in a verified
private directory with hashed names; progress, log and state files are
opened `O_NOFOLLOW` at 0600. Settings and state directories are tightened at
every entry point and refused if a symlink or foreign-owned. Delete
everything removes only the six repository entries, only from something that
is a repository, and unlinks symlinks rather than following them. The FUSE
mount is read-only without `allow_other`, callbacks are panic-guarded, and
stale mounts are cleaned at the next start.

**Window.** No `unwrap`/`expect`/`panic` in non-test code of the window, the
applet, config, migration or dialogs. Engine, keyring and D-Bus work runs off
the UI thread. Browse, Deleted files, Compare, Search and History are capped.
All user-facing strings go through `fl!`; `tests/i18n.rs` enforces locale
parity and placeholders.

## Sources

- Flathub requirements: <https://docs.flathub.org/docs/for-app-authors/requirements>
- Flathub quality guidelines: <https://docs.flathub.org/docs/for-app-authors/metainfo-guidelines/quality-guidelines>
- COSMIC Flatpak repository: <https://github.com/pop-os/cosmic-flatpak>
- libcosmic README (provides IDs, applet routing): <https://github.com/pop-os/libcosmic>
- COSMIC Store source (`src/main.rs`): <https://github.com/pop-os/cosmic-store>
- Flatpak 1.18.4 and COSMIC applets: <https://github.com/pop-os/cosmic-panel/issues/694>
- Déjà Dup's Flathub manifest (rclone module, permissions): <https://github.com/flathub/org.gnome.DejaDup>
- Pop!\_OS app sources: <https://system76.com/support/managing-apps/>
- Pika Backup scheduled backups (Monitor, Background portal): <https://help.gnome.org/pika-backup/feature-schedule.html>
