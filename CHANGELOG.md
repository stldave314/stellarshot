# Changelog

All notable changes to this project are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security

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
- **A wrong password never re-initialises an existing repository.** Upstream
  initialised whenever opening failed, for any reason.

### Added

- Application ID `io.github.stldave314.Stellarshot`, with a desktop entry,
  AppStream metadata (screenshots, release notes, branding) and a symbolic icon.
- Settings are copied once from the upstream application ID on first launch,
  so existing repositories stay in the sidebar. Newer settings are never
  overwritten.
- Error dialogs. Failures to create a repository, take or delete a snapshot, or
  delete a repository used to be logged and otherwise ignored.
- The About page reads its version, licence and links from the package
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

- Settings subscribe to the application's real configuration type, so a theme
  changed in one window applies to the others.
- `RUST_LOG` is respected rather than overwritten on every launch.
- The file-chooser titles, the password field label, the password dialog title
  and every error are localized. Missing keys were added to German, Swedish and
  Swiss German, and orphaned ones removed.

### Fixed

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
