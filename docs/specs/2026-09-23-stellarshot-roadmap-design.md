# Stellarshot: design spec and roadmap to 1.0

- **Date:** 2026-09-23
- **Status:** Approved 2026-09-23
- **Scope:** The overall product and architecture design. Each milestone below gets its own implementation plan. This document is the shared reference those plans build on.

---

## 1. Intent

Stellarshot becomes **a COSMIC-native Déjà Dup**. It's a backup app that a non-technical user can set up once and trust. It goes beyond Déjà Dup in two ways:

1. **Multiple backups.** You can have several independent backup profiles, each with its own folders, destination and schedule, where Déjà Dup has only one.
2. **Complete restore.** You can browse any snapshot, pick files, see every version of a file, find deleted files, compare snapshots, and restore with explicit conflict handling and a dry-run preview.

**What the owner asked for:**
- Create repositories from the main screen, with a button when none exist.
- Choose include and exclude folders when creating one.
- A "Back Up Now" button on the main screen.
- Storage locations like Déjà Dup's, including a complete Google Drive setup flow.
- Multiple repositories.
- A complete restore process.
- Fix delete.
- Import Déjà Dup jobs.
- A roadmap, a new README and a code review, to make this a viable OSS product while keeping the GPL-3.0 license.

**Success at 1.0:**
- A user can install a `.deb` or `.rpm`, create a backup to Google Drive or a USB drive in under two minutes without touching a terminal, get scheduled backups, and restore a single deleted file.
- The app is warning-free, translated and tested end-to-end in CI.

**Non-goals for 1.0:**
- Flatpak packaging.
- A background daemon or D-Bus service.
- Starting a backup when a drive is plugged in.
- Reading duplicity or borg backups.
- Converting Déjà Dup duplicity history.

## 2. Decisions made

| Topic | Decision | Rationale |
|---|---|---|
| Identity | **Hard fork, keep the name "Stellarshot".** App ID `io.github.stldave314.Stellarshot`. The original authors are credited in the README and About page. GPL-3.0 unchanged. | The fork diverges heavily (new rustic API, new config model), so upstreaming is unlikely. |
| Cloud storage | **rclone backend** (`rustic_backend` `rclone` feature). Stellarshot runs the provider sign-in itself. | This is Déjà Dup's current approach. Native Google Drive would need our own OAuth app plus Google's paid restricted-scope security assessment. |
| Passwords | **Secret Service keyring** through `oo7`, with **"Remember password" checked by default**. Stored under `stellarshot/<profile-id>`. | Unattended scheduled backups are impossible without it. |
| Restore scope (1.0) | All four: browse and pick, destination and conflicts, restore missing files, compare and versions. | This is what sets Stellarshot apart from Déjà Dup. |
| Architecture | **Engine/UI split plus a headless `--run` mode.** No daemon. | It contains the rustic 0.2 → 0.13 migration in one module. Scheduled and manual backups share one code path. A daemon can be added later. |
| Write operations | Run in a **child process** (`stellarshot --run …`). | rustic 0.13 has no cancellation API. Killing a child process is the only real way to cancel. |
| Main screen | **Status first** (Déjà Dup style): a big health card, **Back Up Now** and **Restore…**, and a short snapshot list. | Chosen over a snapshot-table-first layout. |
| Size estimate | Estimated by walking **rustic's own `LocalSource` entry stream**, so exclusions inside includes are subtracted exactly. | Déjà Dup's estimate ignores exclusions nested inside includes. Using the same file stream as the backup makes that bug impossible. |
| Déjà Dup import | **Restic-format backups only. Passwords are never imported.** | Owner's decision. Duplicity and borg can't be read by rustic, and reading another app's secret isn't acceptable. |
| Packaging | `.deb` (primary), `.rpm` and tarball from tag-triggered (`v*`) GitHub Actions. `install.sh` is the single build path (it replaces the upstream `justfile`). Flatpak comes after 1.0. | A backup tool needs full filesystem access plus an rclone binary, which works against the Flatpak sandbox. |

## 3. Roadmap

Each milestone gets its own spec → plan → implementation cycle, in this order.

| # | Milestone | Scope |
|---|---|---|
| **M0** | **Fork foundation** | Rebrand the app ID, `.desktop`, metainfo, URLs and icons to `io.github.stldave314.Stellarshot`. Credit the original authors. Add the `debug_log!` module and the `release-build` feature. Add `src/constants.rs`. CI runs check, clippy `-D warnings` and test. Add tag-triggered `.deb`/`.rpm`/tarball packaging. Write a new README, ROADMAP.md and CONTRIBUTING.md. Keep the GPL-3.0 license. |
| **M1** | **Engine migration** | Move rustic from 0.2 to 0.13 behind `src/engine/`. Add the two execution lanes (§5), the `--run` child mode, the per-profile lock, progress reporting and typed errors. Replace today's tests with temp-dir repository tests. |
| **M2** | **Profiles and main screen** | Config v2 `Profile` with v1 migration (§4). A status-first main screen and the empty state. A create wizard with a live size estimate, local destination only at this stage. Back Up Now with Cancel. Keyring passwords. **Fix delete** (§9). |
| **M3** | **Storage locations and import** | Removable drive (identified by UUID), SFTP, **Google Drive through rclone** with in-app sign-in, OneDrive, and any rclone remote. Detect existing repositories. **Déjà Dup import** (§7). |
| **M4** | **Restore** | The restore page with Browse, Deleted files and Compare tabs. Per-file versions. The restore dialog with destination, conflict policy and a dry-run summary (§6.3). |
| **M5** | **Automation** | systemd user-timer schedules. Retention (`forget`, and `prune` per §5.3). Desktop notifications. A periodic `check`, with its interval (`CHECK_INTERVAL`, default 30 days) in `constants.rs`. |
| **1.0** | **Hardening** | All locales complete, accessibility pass, screenshots, and end-to-end "restore actually works" tests in CI. |
| Post-1.0 | | Flatpak. cosmic-files "Revert to previous version" integration. Back up when a drive is plugged in. A D-Bus daemon. Converting duplicity history. |

## 4. Data model and configuration

Profiles are user settings with a UI, so they live in **`cosmic-config`**. `CONFIG_VERSION` goes from 1 to 2.

```rust
Profile {
    id: Uuid,                        // stable key for keyring, systemd unit, lock file, logs
    name: String,
    destination: Destination,
    sources: Vec<PathBuf>,           // include folders
    excludes: Vec<PathBuf>,          // → rustic Excludes.globs as "!/abs/path"
    exclude_patterns: Vec<String>,   // advanced globs: "*.tmp", "node_modules"
    one_file_system: bool,           // default true; used by BOTH the estimate and the backup
    schedule: Schedule,              // Manual | Hourly | Daily | Weekly
    retention: Retention,            // KeepForever | Smart { daily: 7, weekly: 4, monthly: 12 } | Custom(KeepSpec)
    last_success: Option<DateTime>,  // cache for the main screen only; the repository is the source of truth
}

Destination =
  | Local     { path: PathBuf }
  | Removable { volume_uuid: String, relative_path: PathBuf, label: String } // resolved at run time
  | Sftp      { host: String, user: String, path: String }
  | Rclone    { remote: String, path: String }  // e.g. remote "stellarshot-gdrive-<id>"
```

- **v1 migration.** Each v1 `Repository { name, path }` becomes a `Local` profile with no sources, and the user is asked to add folders. Migration runs once, on first launch after upgrading.
- **Defaults for a new profile:**
  - Include `$HOME`.
  - Exclude `~/.cache`, `~/.local/share/Trash` and `~/Downloads`.
  - `one_file_system = true`, `Schedule::Daily`, `Retention::Smart`.
- **`Custom(KeepSpec)`** covers keep-last/daily/weekly/monthly/yearly and `keep_within`, which maps onto rustic's `KeepOptions`. `keep_within` is what Déjà Dup's `delete-after` imports into. *(Changed in M5: `Custom(KeepSpec)` became `KeepFor { days }`, Déjà Dup's "Keep at least…" choice. Arbitrary rules remain available through `rustic forget`. `keep_within` is measured from the newest snapshot, so a backup that has not run for a while keeps its history. Run facts such as `last_check` live in cosmic-config's state store, one key per profile, so a scheduled run never rewrites the profile list.)*
- **Passwords never go in the config.** They live only in the keyring, under `stellarshot/<profile-id>`.
- **rclone configuration is our own.** It lives at `~/.config/stellarshot/rclone.conf` and is passed through `RCLONE_CONFIG`. We never read or write `~/.config/rclone/rclone.conf`. The Déjà Dup importer *copies* a remote section into ours when needed.
- **Tuning values** (progress interval, size-scan debounce, rclone timeouts, the default exclude list, the check interval) go in `src/constants.rs`, never in runtime config.
- **Removable drives** are identified by volume UUID plus a relative path, not by mount point. A second drive with the same label turns `/media/<user>/Backup` into `/media/<user>/Backup1`.

## 5. Engine and execution model

### 5.1 Engine API

`src/engine/` is a **synchronous** library with plain types. No `rustic_core` types appear outside it, so it can be tested without any async runtime.

```rust
engine::open(&Profile, &Secret) -> Result<Repo, EngineError>
engine::init(&Profile, &Secret) -> Result<Repo, EngineError>
engine::probe(&Destination) -> Result<Probe, EngineError>          // Empty | ResticRepo | Other
engine::estimate(&Profile, &CancelFlag, &dyn SizeSink) -> SizeEstimate

Repo::snapshots() -> Vec<SnapshotSummary>
Repo::backup(&Profile, &dyn ProgressSink) -> BackupReport          // includes warnings
Repo::browse(snap, dir: &Path) -> Vec<TreeEntry>                   // lazy, one directory at a time
Repo::search(snap, query) -> impl Iterator<Item = TreeEntry>
Repo::versions(path) -> Vec<FileVersion>                           // identical content collapsed
Repo::missing(scope: &Path, since) -> Vec<MissingEntry>
Repo::diff(a, b) -> impl Iterator<Item = DiffEntry>                // skips subtrees with equal tree IDs
Repo::plan_restore(&Selection, dest, ConflictPolicy) -> RestorePlan  // prepare_restore(dry_run = true)
Repo::restore(RestorePlan, &dyn ProgressSink) -> RestoreReport
Repo::forget_prune(&Retention) -> MaintenanceReport
Repo::check() -> CheckReport
Repo::delete_all()                                                 // "Delete repository and all data"
```

Files: `engine/{mod,repo,backup,estimate,restore,browse,diff,maintenance,progress,lock,error}.rs`.

What we checked in rustic 0.13:
- `Excludes { globs, iglobs }` exists.
- `ProgressBars` / `RusticProgress` are synchronous traits we can implement.
- `KeepOptions` exists.
- `prepare_restore(opts, node_streamer, dest, dry_run)` exists.
- `LocalSource` is built on the `ignore` crate and supports `one_file_system`.
- **There's no library diff.** It's built by walking two trees, as the rustic CLI does.
- **There's no cancellation API and no repository locking.**

### 5.2 Two lanes

| Lane | Operations | How it runs |
|---|---|---|
| **Read** | snapshots, browse, search, versions, missing, diff, plan_restore, estimate | In-process `tokio::task::spawn_blocking`. Results come back through `Task::perform`. Several can run at once. `estimate` checks a `CancelFlag` between entries. |
| **Write** | backup, restore, forget/prune, check, delete_all | A child process, `stellarshot --run <op> <profile-id> [args]`. It writes JSON progress lines to stdout and ends with `{"result": …}` or `{"error": kind, "detail": …}`. The UI consumes the lines as a stream through `Task::run`. |

- **Cancel sends SIGTERM to the child.** It's safe for backup: the snapshot file is written last, so an interrupted backup leaves only unreferenced packs, and the next prune removes them. **Prune can't be cancelled** once it starts deleting, and the UI shows this.
- **The systemd timer runs the same `--run backup`**, so scheduled and manual backups share one code path.
- **Fixed as a side effect:** today `app.rs` calls blocking rustic functions inside `async` blocks, which stalls tokio workers. And its `for command in commands { return … }` only runs the first command.

### 5.3 Locking, progress files and pruning

- Every write takes an **`flock` on `$XDG_RUNTIME_DIR/stellarshot/<location-key>.lock`**, where the key is derived from the repository's location. This covers the UI, the timer and a second UI instance. *(Changed in M1 from a profile-ID key: two profiles pointing at one repository must share a lock.)*
- A child holding the lock also writes throttled progress to **`<location-key>.progress`** next to it. If the UI finds the lock held by a process it didn't start, such as a timer run, it shows "Backup in progress" and **follows that file**. It never starts a second write.
- Because rustic doesn't lock, prune is dangerous when another machine shares the destination.
  - `forget` (removing snapshot records under the retention policy) always runs after a scheduled backup.
  - **Automatic `prune`** (actually deleting unreferenced data) is **on by default for `Local` and `Removable`** destinations and **off by default for `Sftp` and `Rclone`** destinations. Those can be shared, and the user turns prune on per profile.
  - Manual "Clean up now" is always available.

### 5.4 Progress

`ProgressSink` adapts rustic's `ProgressBars`. Updates are throttled to `PROGRESS_INTERVAL` from `constants.rs`, so per-blob `inc()` calls don't flood the UI. Each progress event carries: phase, files done/total, bytes done/total, current path and ETA.

## 6. User interface

### 6.1 Main screen (status first)

- **Nav bar:** one entry per profile, with a health dot and the time since the last backup. **"＋ New backup"** sits at the bottom.
  - Green means succeeded within the schedule.
  - Amber means overdue, or completed with warnings.
  - Red means the last run failed, or a check found damage.
- **Profile page:** a status card ("✓ Last backup 2 hours ago", next run, destination, snapshot count, size) with **Back Up Now**, **Restore…** and a **⋯** menu (Edit, Check, Remove from Stellarshot, Delete repository and all data). Below it are the three most recent snapshots and "Show all".
- **While running:** the card turns into a progress card with a bar, the current file, counts, ETA and **Cancel**. This works the same for timer-started runs (§5.3).
- **Empty state (no profiles):** "Keep your files safe", then **Create a Backup…**, with the links "Open an existing repository" and, if detected, "Import from Déjà Dup".

### 6.2 Create wizard

It's a full page in the content area with four steps: **what, where, when, secure**. Every step has usable defaults.

1. **What.**
   - Include and exclude lists. "Add folders…" opens the XDG file-chooser portal with multi-select. Glob patterns are under Advanced.
   - Each include row shows its size. Each exclude row shows how much it removes from inside the includes.
   - An exclude outside every include is dimmed and labeled *"not inside an included folder"*.
   - The **Estimated backup size** shows files and bytes, with the arithmetic underneath. It's computed by `engine::estimate`, which iterates `LocalSource::entries()`: the **same filtered stream the backup reads**. Files are counted exactly as the backup counts them (a hard-linked file once per name, overlapping sources once), symlinks aren't followed, and `one_file_system` is respected. *(Changed in M2 from "hard links once by inode": the estimate must equal the backup's own total, and rustic counts each name.)*
   - The estimate updates live, restarts after `SIZE_SCAN_DEBOUNCE` when the lists change, and never blocks Next. A note says the first backup is usually smaller after compression and deduplication.
2. **Where.**
   - Options: removable drive (detected drives listed, with a **too-small warning** from the estimate), Google Drive (**Sign in with Google…**, then a folder field), SFTP, local folder, and "Other cloud (rclone remote)" behind an expander.
   - After a destination is chosen, `engine::probe` runs. If it finds an existing restic repository, step 4 switches to "Existing backup found. Enter its password" and step 1 is pre-filled from the latest snapshot's paths.
3. **When.** Name, "Back up automatically" with Hourly/Daily/Weekly, and Keep: Smart (recommended) / Forever / Custom.
4. **Secure.**
   - Password and confirmation with a strength meter, and "Remember password (keyring)" checked.
   - A warning that a lost password makes backups unrecoverable, and a note that scheduled backups need Remember.
   - The final button is **Create & Back Up Now**.

"Open an existing repository" uses the same wizard, starting at step 2.

### 6.3 Restore page

It's opened from **Restore…** and has three tabs. All three feed one Restore dialog.

- **Browse:**
  - Snapshot column: recent snapshots individually, older ones grouped weekly and monthly.
  - File column: breadcrumb, search within the snapshot, and tri-state checkboxes.
  - Versions column: versions of the selected file with identical content collapsed ("same in 3 more"), plus **Open copy** (extracted to a temp folder, opened read-only) and **Restore this version…**
  - The bottom bar shows the selection count and size, and **Restore…**
- **Deleted files:** files that are in snapshots but missing from disk, within a folder scope the user picks and a time window (default 30 days). Each item restores from the last snapshot that contained it.
- **Compare:** two snapshot pickers, counts of added, changed and removed, and collapsible folders. Any row can go to Restore… or Versions.
- **Restore dialog:**
  - **Restore to:** the original location, or another folder (keeping the folder structure).
  - **If a file already exists:** Overwrite / **Keep both** (default; the restored copy gets a " (Sep 21)" suffix) / Skip.
  - A **dry-run summary** from `prepare_restore(dry_run = true)` shows files to restore, conflicts and already-identical files, before anything is written.
  - How the policies are implemented: we supply the `(path, node)` stream that `prepare_restore` consumes. **Skip** filters out existing paths. **Keep both** rewrites conflicting paths. **Overwrite** passes the stream through unchanged, and rustic's own logic leaves identical files alone.
  - rustic's `delete: true` option is **never exposed**.

## 7. Déjà Dup import

- **Where the settings are.** Stellarshot reads `~/.var/app/org.gnome.DejaDup/config/glib-2.0/settings/keyfile` (Flatpak) and the output of `dconf dump /org/gnome/deja-dup/` (native install). Both use the same keyfile format, so one parser handles both. Missing keys take the **schema defaults**, which were checked against Déjà Dup 50.2's `org.gnome.DejaDup.gschema.xml`. For example, `include-list` defaults to `['$HOME']`, `periodic` to `false` and `tool` to `'unset'`.
- **Import pre-fills the create wizard.** There's no separate UI. The user reviews every step.
- **Only restic-format backups can be imported:**
  - `tool = 'restic'`: known up front.
  - `tool = 'unset'`: decided by `engine::probe` after the destination step. For cloud backends that means after sign-in.
  - Duplicity, borg or unknown: the importer stops with *"This Déjà Dup backup uses the duplicity format, which Stellarshot can't read. Create a new backup instead."* Nothing is imported.
- **Passwords are never imported.** This is a rule, not a limitation. On native installs Déjà Dup's password *is* readable from the login keyring, and we never query it. The import module has **no keyring dependency**. The user enters the password in step 4.
- **Mapping:**

| Déjà Dup | Stellarshot |
|---|---|
| `include-list`, `exclude-list` | `sources`, `excludes`. `$HOME`, `$TRASH`, `$DOWNLOAD`, `$DESKTOP`, `$DOCUMENTS`, `$MUSIC`, `$PICTURES`, `$PUBLIC_SHARE`, `$TEMPLATES`, `$VIDEOS` are resolved through XDG user dirs. `~` is expanded. |
| `backend=local` + `local/folder` | `Local` (a relative folder is resolved against `$HOME`) |
| `backend=drive` + `drive/{uuid,folder,name}` | `Removable` |
| `backend=remote` + `remote/{uri,folder}` | `sftp://` → `Sftp`. Other schemes are refused with "not supported yet". |
| `backend=google` / `microsoft` + `folder` | `Rclone` (Drive / OneDrive). Needs a fresh sign-in, because Déjà Dup's OAuth token isn't ours. |
| `backend=rclone` + `rclone/{remote,folder}` | `Rclone`. The remote's section is copied into our `rclone.conf`. |
| `periodic` + `periodic-period` | Off → Manual. 1 → Daily. Any other value → Weekly. |
| `delete-after` | 0 → KeepForever. N → `Custom(KeepSpec { keep_within: N days, .. })` |

- **Déjà Dup's settings are never modified.** If its `periodic` is on, the last wizard screen suggests turning off automatic backups in Déjà Dup so the two don't both run.
- **To verify in M3:** where Déjà Dup's restic tool puts the repository inside `folder` (the root or a subfolder). `probe` checks both.

## 8. Error handling

The engine returns `EngineError` (thiserror). The child process reports errors in its final JSON line, and the UI maps them back to the same enum. Every message goes through `fl!`.

| Kind | Message (gist) | Action |
|---|---|---|
| `WrongPassword` | Password incorrect | Re-prompt |
| `DestinationUnavailable` | Insert "Backup SSD" / can't reach server | Retry. **Scheduled runs skip quietly** and retry at the next slot. |
| `AuthExpired` | Google Drive sign-in expired | Sign in again |
| `DestinationFull` | Destination is full | Open retention settings |
| `Locked` | A backup is already running | Follow its progress |
| `CompletedWithWarnings` | Backed up, but N files couldn't be read | Show list (amber, not red) |
| `RepositoryDamaged` | Check found problems | Details. **Automatic prune is disabled** until resolved. |
| `NotARepository` / `AlreadyExists` | Location has no backup / already has one | Route to the right wizard branch |
| `RcloneMissing` | rclone isn't installed | Install hint |
| `Internal` | Unexpected error | Copy details |

- **Scheduled-run failures** raise a desktop notification (`org.freedesktop.Notifications`). Clicking it opens the profile.
- **Genuine errors go to stderr as well as the debug log.** The debug log is for diagnostics only.
- **Debug logging** follows the standard `debug_log!` module:
  - `ENABLED = DEVELOPER_LOGGING && !cfg!(feature = "release-build")`, plus a `PATH` constant.
  - Truncated on each launch, with elapsed-time prefixes.
  - Categories: `ENGINE`, `UI`, `CONFIG`, `RCLONE`, `SCHED`, `IMPORT`, `LOCK`.
- **To verify in M1:** rustic reports unreadable files as `warn!` log events, not return values. Collecting them for `CompletedWithWarnings` means a `tracing` layer scoped to the job. Confirm this captures backup-time warnings before relying on it.

## 9. Fixing delete (M2)

Three suspected causes were found by reading `app.rs`. **Each must be reproduced before it's fixed:**

1. **Delete does nothing unless a repository is open.** The `DeleteRepository` branch checks `self.content.repository`, which is only set after the password is entered.
2. **Failures are swallowed.** `if let Ok(_) = remove_dir_all(…)` discards errors.
3. **Paths are percent-encoded.** They come from `Url::path()`, so `My Backups` is stored as `My%20Backups`. The fix is `Url::to_file_path()`.

The new design splits the action in two:
- **Remove from Stellarshot:** forgets the profile, its keyring entry and its timer. The data stays.
- **Delete repository and all data:** asks for confirmation by typing the profile name, then runs `delete_all` through the write lane, with errors reported.

## 10. Testing

All tests use temp directories. The current tests that write to `/tmp/test` and `/etc` are removed.

- **Round trip:**
  - Back up a fixture tree containing spaces, unicode, `%` characters, symlinks, hard links, empty dirs, sparse files and restrictive permissions.
  - Restore it and compare content and metadata byte-for-byte.
- **Conflict policies:** Overwrite, Keep both and Skip, each against a destination that already contains files. Check that the dry-run counts equal the actual outcome.
- **Kill safety:** SIGKILL `--run backup` partway through. `check` must pass and the next backup must succeed.
- **Locking:** start two `--run backup` processes at once. The second must report `Locked`.
- **Estimate accuracy (regression test for the Déjà Dup bug):**
  - Use a fixture with excludes nested inside includes, overlapping includes and hard links.
  - `estimate` must **equal** the byte total the backup reports.
- **Diff and versions:** known changes between two fixture snapshots, with identical versions collapsed.
- **Import:**
  - Fixture keyfiles in both the Flatpak and `dconf dump` formats.
  - Schema defaults applied, `$TOKENS` resolved.
  - Duplicity and borg are refused.
  - No keyring crate is reachable from the import module (enforced by module structure).
- **Config:** v1 → v2 migration.
- **rclone:** the rclone path is exercised against rclone's `:local:` backend. CI never uses real cloud credentials.
- **UI:** `update()` state-transition tests (message → state) without rendering.
- **CI jobs:**
  - `cargo check`, `cargo clippy -- -D warnings`, `cargo test`.
  - A locale parity script: same keys as `en`, no duplicates or orphans, placeholders intact.
  - A **release-strip check**: the debug-log path must be absent from the `release-build` binary *and present* in the default build. If it's missing from both, the check fails, so it can't pass by skipping.
- **Local runs** are resource-capped: `cargo -j 4` under `systemd-run --user --scope -p MemoryMax=4G`.

## 11. Code review summary: what changes and why

| Finding | Where | Resolution |
|---|---|---|
| rustic 0.2 pinned; current is 0.13 | `Cargo.toml` | M1 engine migration |
| Blocking rustic calls inside `async` blocks | `app.rs` `Task::perform` sites | Read lane uses `spawn_blocking`, write lane uses a child process |
| Only the first returned command runs | `app.rs` (`for command in commands { return … }`) | Collect into `Task::batch` |
| Delete silently fails | `app.rs` `DeleteRepository` | §9 |
| Paths are percent-encoded | `Url::path()` call sites | `Url::to_file_path()` |
| Errors go to `println!`/`log`, not the UI | throughout | Typed `EngineError` plus localized UI messages (§8) |
| Password asked on every click | `on_nav_select` | Keyring (§2) |
| Tests write to `/tmp/test` and `/etc` | `src/backup/*` tests | Temp-dir tests (§10) |
| `update()` is a 736-line mix of dialogs, config, portal and rustic calls | `app.rs` | UI talks only to `engine`. Each page gets its own view module. |
| Upstream app ID and URLs | `APP_ID`, `res/*.desktop`, metainfo | M0 rebrand |

## 12. Deliverables that follow this spec

1. **M0 implementation plan**, written next. It includes the new README, ROADMAP.md (§3 in public-facing form) and CONTRIBUTING.md.
2. Per-milestone plans for M1–M5, each written when its predecessor is merged.
