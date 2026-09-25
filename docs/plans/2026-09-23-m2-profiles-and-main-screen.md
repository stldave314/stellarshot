# M2: Backup profiles and the main screen, implementation plan

**Goal:** Replace "repositories you type a password into every time" with **backup profiles**. The main screen is status-first, the empty state has a button to start, there's a three-step setup wizard with a live size estimate, passwords are remembered in the keyring, and "remove" and "delete everything" are separate actions.

**Architecture:**
- **UI toolkit:** libcosmic moves to 1.0 (current `main`), and the app is rebuilt on it. `src/app.rs` keeps the application shell (nav bar, menus, dialogs, settings). Each page gets its own module:
  - `app/pages/profile.rs`: the status card and snapshot list
  - `app/pages/empty.rs`: the first-launch screen
  - `app/wizard.rs`: the setup wizard
- **Profiles:** `src/profile.rs` holds the `Profile` type the config stores. It turns a profile into a `Location` and `BackupRequest` for the engine, so the engine stays profile-agnostic.
- **Estimate:** `src/engine/estimate.rs` walks rustic's own `LocalSource` entry stream, the same filtered file list the backup reads.
- **Keyring:** `src/keyring.rs` wraps `oo7` (Secret Service), with one item per profile.

**Spec:** §4 (data model), §6.1 (main screen), §6.2 (wizard), §9 (delete)

## Global constraints

The M0 and M1 constraints still apply. In addition:
- `CONFIG_VERSION` = 2. v1 `Repository { name, path }` entries become `Local` profiles once, and are never re-imported.
- The keyring item attributes are `{ "application": APP_ID, "profile": <profile id> }`, with the label "Stellarshot backup password: <name>".
- The file chooser is libcosmic's `dialog::file_chooser` (`xdg-portal`); the direct `ashpd` dependency is removed.

## Deviations from the spec, decided here

- **The wizard has three steps in M2 (what, where, secure).** "When" arrives with M5, where schedules and retention actually run. A schedule the app can't yet honor would be a setting that lies. Profiles store `schedule: Manual` and `retention: KeepForever` until then.
- **Local folders only.** Removable drives, SFTP and cloud storage are M3.
- **Restore…** isn't shown on the status card until M4 provides it.
- **The estimate restarts when the lists change instead of debouncing.** The previous scan is canceled at once through its flag, which is simpler, and just as cheap, as a timer.

## Review focus

1. **A repository inside a folder being backed up**, for example backing up `~` to `~/Backups/home`. Its folder must be excluded automatically, or the backup copies itself. Test: `repository_inside_a_source_is_excluded`.
2. **Excludes nested in includes, overlapping includes, and hard links in the estimate.** The total must equal what the backup reports. Test: `estimate_matches_the_backup`.
3. **A v1 config with repositories.** Each becomes a profile exactly once, and a second launch adds nothing. Tests: `v1_repositories_become_profiles`, `profiles_are_not_duplicated`.
4. **The keyring unavailable or locked.** The password prompt still works and the app never crashes. The UI treats keyring errors as "not remembered". Test: `keyring_round_trip` (a real Secret Service, required in CI through gnome-keyring).
5. **Delete everything on a mistyped name.** The button stays disabled until the name matches exactly. Test: `delete_requires_the_exact_name`.

---

### Task 1: libcosmic 1.0 and dependency refresh
`cargo update`, then i18n-embed 0.16, i18n-embed-fl 0.10, rust-embed 8.11, `LazyLock` instead of `once_cell`, and the portal through libcosmic. Port the shell to `cosmic::app::Task`/`cosmic::Action`.

### Task 2: Profiles and config v2 (`src/profile.rs`, `src/app/config.rs`, `src/app/migrate.rs`)
```rust
pub struct Profile { pub id: String, pub name: String, pub destination: Destination,
    pub sources: Vec<PathBuf>, pub excludes: Vec<PathBuf>, pub exclude_patterns: Vec<String>,
    pub one_file_system: bool, pub schedule: Schedule, pub retention: Retention,
    pub last_success: Option<i64> }
pub enum Destination { Local { path: PathBuf } }
impl Profile { pub fn location(&self) -> Location; pub fn backup_request(&self) -> BackupRequest; }
pub fn profiles_from_v1(ron: &str) -> Vec<Profile>;
```
Tests: `repository_inside_a_source_is_excluded`, `v1_repositories_become_profiles`, `profiles_are_not_duplicated`, `default_excludes_are_home_relative`.

### Task 3: Size estimate (`src/engine/estimate.rs`)
```rust
pub struct SizeEstimate { pub files: u64, pub bytes: u64 }
pub fn estimate(request: &BackupRequest, cancel: &AtomicBool, progress: &mut dyn FnMut(SizeEstimate)) -> Result<Option<SizeEstimate>, EngineError>;
pub fn folder_size(path: &Path, cancel: &AtomicBool) -> Option<u64>;
```
Hard links are counted once by (device, inode). `None` means canceled. Tests: `estimate_matches_the_backup` and `estimate_respects_cancel`.

### Task 4: Keyring (`src/keyring.rs`)
`store(profile, name, secret)`, `load(profile) -> Option<Secret>`, `forget(profile)`, all async. Test `tests/keyring.rs::keyring_round_trip` fails, not skips, when no Secret Service answers. CI runs it inside `dbus-run-session` with an unlocked gnome-keyring.

### Task 5: Main screen and pages
- Nav bar: one item per profile, plus "＋ New backup".
- Profile page:
  - Status card: last backup (relative time), destination, snapshot count, **Back Up Now**, a progress card with Cancel, and a ⋯ menu with Remove from Stellarshot and Delete backup and all data.
  - Recent snapshots (5, then "Show all").
  - An inline unlock row when no password is remembered: a password field and "Remember password".
- Empty state: "Keep your files safe", **Create a Backup…**, and "Open an existing backup".

### Task 6: Wizard (`src/app/wizard.rs`)
- **What:**
  - Include and exclude rows with sizes; "Add folders…" uses `open_folders()`.
  - An exclude outside every include is labeled.
  - Patterns go under Advanced.
  - The live estimate uses the total from Task 3.
- **Where:** "Choose folder…", then `probe`. Empty means create, Repository means open, NotEmpty means refuse.
- **Secure:**
  - Password and confirmation (create) or the password alone (open), plus Remember.
  - On finish, create or open the repository, save the profile, store the secret if Remember is checked, and start the first backup.
  - When opening an existing repository, the What step is pre-filled from its latest snapshot's paths.
- Pure-state tests for step validation: `cannot_leave_what_without_a_source`, `create_needs_matching_passwords`, `not_empty_location_blocks_next`.

### Task 7: Delete split
- "Remove from Stellarshot" removes the profile and its keyring item. The data stays.
- "Delete backup and all data" needs the profile's name typed exactly, then locks, calls `delete_repository`, and removes the profile.
- Test: `delete_requires_the_exact_name`.

### Task 8: Verify, document, screenshots, commit, push
- **Screenshots:** capture only Stellarshot's own window, never the whole screen. The window runs under Xwayland (`WAYLAND_DISPLAY` unset) with a throwaway `XDG_CONFIG_HOME` and a demo repository. `xdotool` finds the window ID, and ImageMagick `import -window <id>` captures just that window. If window-only capture isn't possible, the README keeps the old screenshots and says so.
