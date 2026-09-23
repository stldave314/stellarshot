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

### Repository safety (`src/backup/location.rs`, `src/backup/init.rs`)

These guard against the upstream behaviour that could have deleted a home
directory. They run against real directories in temporary folders.

| Test | What it proves |
| --- | --- |
| `delete_repository_leaves_foreign_files` | Deleting a repository removes every repository entry, leaves a neighbouring `Documents/report.odt` byte-for-byte intact, keeps the folder, and reports what it left |
| `delete_repository_removes_the_folder_when_nothing_else_is_there` | A folder holding only a repository is removed completely |
| `delete_refuses_a_folder_that_is_not_a_repository` | A folder without `config` and `keys` is refused, and nothing in it is touched |
| `delete_does_not_follow_a_symlinked_entry` | A `snapshots` entry that is a symlink to another folder is removed as a link; the target's files survive |
| `init_refuses_non_empty_non_repository` | A folder with the user's own files is classified as unusable |
| `refuses_a_folder_with_other_files` | `init` itself refuses that folder and writes nothing into it |
| `a_wrong_password_never_reinitialises` | A wrong password on an existing repository is an error, and the repository's `config` is unchanged afterwards |
| `url_with_space_becomes_real_path` | `file:///tmp/My%20Backups` becomes `/tmp/My Backups`; the test also asserts that `Url::path()` would have kept `%20`, so it fails if the premise stops being true |

### Backup round trip (`src/backup/snapshot.rs`)

`backs_up_a_folder_and_lists_then_deletes_the_snapshot` creates a real
repository, backs up a folder containing a name with a space and a
subdirectory, lists exactly one snapshot, deletes it, and lists none.

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

## Adding a check

- Name the failure it prevents, in the test name or a comment.
- Make it fail when its fixture or input is missing.
- For anything that deletes, writes or grants access, test against real files,
  and assert on what must **survive** as well as what must change.
- Add it to this document.
