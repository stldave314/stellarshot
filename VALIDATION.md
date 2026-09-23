# Validation

How Stellarshot is checked, what each check proves, and how to run it.

The rule every check here follows: **a check that cannot fail is not a check.**
Each one is written so that a missing fixture, an empty input or a skipped step
produces a failure, not a green result. Security-relevant behaviour is proven
against real files and real binaries, never by reading the code.

## Running everything locally

```sh
cargo fmt --all --check
cargo clippy --all-targets --all-features      # must report zero warnings
cargo test --all-features
./scripts/verify-release-build.sh
./scripts/validate-metadata.sh                  # needs desktop-file-utils and appstream
./install.sh package                            # needs cargo-deb and cargo-generate-rpm
```

CI runs the same commands on every push and pull request
(`.github/workflows/ci.yml`), with `RUSTFLAGS=-D warnings`. A tag runs them
again before publishing (`.github/workflows/release.yml`).

## Automated checks

### Repository safety (`src/engine/location.rs`, `src/engine/tests.rs`)

These guard against the upstream behaviour that could have deleted a home
directory. They run against real directories in temporary folders.

| Test | What it proves |
| --- | --- |
| `delete_repository_leaves_foreign_files` | Deleting a repository removes every repository entry, leaves a neighbouring `Documents/report.odt` byte-for-byte intact, keeps the folder, and reports what it left |
| `delete_repository_removes_the_folder_when_nothing_else_is_there` | A folder holding only a repository is removed completely |
| `delete_refuses_a_folder_that_is_not_a_repository` | A folder without `config` and `keys` is refused, and nothing in it is touched |
| `delete_does_not_follow_a_symlinked_entry` | A `snapshots` entry that is a symlink to another folder is removed as a link; the target's files survive |
| `init_refuses_non_empty_non_repository` | A folder with the user's own files is classified as unusable |
| `init_refuses_existing_and_non_empty_locations` | The engine's `init` refuses both an existing repository and a folder of the user's files, and writes nothing into the latter |
| `url_with_space_becomes_real_path` (`src/app/portal.rs`) | `file:///tmp/My%20Backups` becomes `/tmp/My Backups`; the test also asserts that `Url::path()` would have kept `%20`, so it fails if the premise stops being true |

### The engine (`src/engine/tests.rs`)

Real repositories in temporary folders, with a fixture tree containing names
with spaces, unicode and `%`, a symlink, an empty folder, a 64 KiB binary file
and a mode-0600 file.

| Test | What it proves |
| --- | --- |
| `round_trip_preserves_tree` | Back up, restore, and compare every entry: content, symlink target, permissions, empty folders. **Cannot pass vacuously:** it first asserts the fixture has at least ten entries |
| `excluded_folder_is_not_in_the_snapshot` | An excluded folder inside a source is absent after restore, and the rest is present |
| `exclude_pattern_applies_at_any_depth` | `node_modules` and `*.tmp` are excluded wherever they occur |
| `wrong_password_is_reported_as_such` | A wrong password is `WrongPassword`, not a generic failure |
| `open_non_repository_is_not_a_repository` / `unreachable_location_is_reported_as_unavailable` | A folder of other files, and a path whose parent does not exist (an unplugged drive), are told apart |
| `second_backup_is_incremental` | After changing one small file, the unchanged 64 KiB file is not stored again |
| `snapshots_are_listed_newest_first_and_can_be_deleted` | Ordering, short IDs, and deleting the right snapshot |
| `check_passes_on_a_sound_repository` / `check_reports_a_damaged_repository` | The integrity check passes a sound repository **and fails** one whose index was removed, so it is known to detect damage |
| `backup_reports_progress_ending_at_the_total` | Progress reports every byte of every regular file |
| `throttle_sends_first_and_final` (`progress.rs`) | 1,000 rapid increments produce few reports, and the last one is exact |

### The `--run` child process (`tests/runner.rs`, `tests/child.rs`)

These run the real `stellarshot` binary, the way the window does.

| Test | What it proves |
| --- | --- |
| `runner_backs_up_and_reports_done` | A job on stdin produces progress events, a final `done` with a report, exit status 0, and one snapshot |
| `runner_reports_wrong_password` | Exit status 1, an error event of kind `WrongPassword`, and no snapshot |
| `runner_rejects_an_unknown_operation` | Exit status 2 for an operation that does not exist |
| `second_writer_reports_locked` | With the lock held, a backup reports `Locked` and **writes nothing** |
| `killed_backup_leaves_a_sound_repository` | 48 MiB of incompressible data; the child is killed with SIGKILL as soon as data is being stored. Afterwards there is no snapshot, `check` passes, and the next backup succeeds. **Cannot pass vacuously:** it fails if the backup finishes before it can be killed |
| `backup_finishes_after_the_window_goes_away` | Closing the reading end of the pipe mid-backup does not stop it; the snapshot is recorded |
| `a_backup_streams_started_progress_and_done` | The window's stream yields `Started`, progress, then `Done` |
| `cancelling_a_backup_ends_it_as_cancelled_without_a_snapshot` | Cancel from the window's handle ends the stream as `Cancelled`, with no snapshot left behind |
| `a_missing_executable_is_reported_not_hung` | If the child cannot start, the stream ends with an error instead of waiting forever |

### The window's state (`src/app/views/content.rs`)

`update()` is tested without rendering: a finished backup reloads the list; a
failed, locked or cancelled backup is reported and the progress card cleared;
a wrong password returns to "nothing selected" and forgets the password;
deleting needs an unlocked repository.

### Settings migration (`src/app/migrate.rs`)

| Test | What it proves |
| --- | --- |
| `migrates_old_config_once` | Old settings are copied, and a second launch does not copy them again over a later change |
| `does_not_overwrite_existing_new_config` | Settings that already exist under the new ID always win |
| `no_old_config_is_a_no_op` | Nothing is created when there is nothing to migrate |

### Locale parity (`tests/i18n.rs`)

Fluent falls back silently: a key missing from one locale shows as English at
runtime, and a mangled `{ $placeholder }` misbehaves only in that language.

- Every locale has exactly the English key set: nothing missing, nothing
  orphaned.
- Every message has the same placeholders in every locale.
- No locale repeats a key.
- **Cannot pass vacuously:** it fails if the English file is missing or has
  suspiciously few messages, rather than comparing nothing.

### Release builds carry no debug logging (`scripts/verify-release-build.sh`)

Builds the binary twice with the developer switch forced on: once without and
once with `--features release-build`. It then checks whether the debug log path
survives into each binary.

- **Cannot pass vacuously:** it fails if the path is missing from the build
  *without* the feature. In that state it could not detect a regression.
- It restores `src/debug.rs` on exit, even on failure.
- The M0 release was also checked on the packaged binary: the `.deb`'s
  `/usr/bin/stellarshot` does not contain the path.

### Packaging metadata (`scripts/validate-metadata.sh`)

- `desktop-file-validate` on the desktop entry, where any finding except a hint
  fails.
- `appstreamcli validate` on the metainfo.
- Checks `appstreamcli` does not make:
  - at least one screenshot, every screenshot captioned, exactly one default;
  - the newest `<release>` equals the version in `Cargo.toml`;
  - the component ID, launchable, desktop `Icon=` and installed icon files all
    agree on the application ID;
  - developer, content rating, source link and branding are present.
- **Cannot pass vacuously:** a missing metainfo, desktop entry or `Cargo.toml`
  is a failure, not a skip.

One pedantic finding is accepted: the application ID contains uppercase
letters (`Stellarshot`), which AppStream discourages. That follows the COSMIC
convention (`com.system76.CosmicFiles`).

### Packages (`./install.sh package`, CI)

CI builds the `.deb`, `.rpm` and tarball on every push and prints the `.deb`'s
control data and file list, so a packaging break shows up before a release is
tagged.

## Manual checks

Things a test cannot reach yet, and how they were confirmed.

| Check | How | Last confirmed |
| --- | --- | --- |
| The `.deb` contains the binary, desktop entry, both icons, metainfo and README, with `Recommends: rclone` and the no-reply maintainer address | `dpkg-deb --info` and `--contents` on the built package | M0 |
| The shipped binary has no debug log path | `dpkg-deb -x`, then `strings usr/bin/stellarshot` | M0 |
| The window starts in a COSMIC Wayland session and exits cleanly | Launched for six seconds; no output on stderr, no process left behind | M1 |
| Settings migrate on a real account | First launch copied `~/.config/cosmic/com.github.cosmic-utils.Stellarshot/v1/repositories` to the new ID | M1 |

## Adding a check

- Name the failure it prevents, in the test name or a comment.
- Make it fail when its fixture or input is missing.
- For anything that deletes, writes or grants access, test against real files,
  and assert on what must **survive** as well as what must change.
- Add it to this document.
