# Contributing

Thanks for looking. Bug reports, translations and code are all welcome.

Please read the [Code of Conduct](CODE_OF_CONDUCT.md). Security problems go
through [SECURITY.md](SECURITY.md), not the issue tracker.

## Reporting a bug

Please include:

- What you expected, and what happened instead.
- Your distribution and COSMIC version, and the Stellarshot version from the
  About page.
- Where the repository is: a local folder, a USB drive, a network mount.

**Never include a repository password, and do not attach repository files.**
They are encrypted, but they are still your data.

If something is misbehaving rather than obviously broken, a debug log helps:
set `DEVELOPER_LOGGING` to `true` in `src/debug.rs`, rebuild *without*
`--features release-build`, reproduce, and attach
`/tmp/stellarshot-debug.log`. Look through it first: it contains file paths.

## Building and testing

```sh
cargo build
cargo test
cargo clippy --all-targets --all-features
```

Both `cargo check` and `cargo clippy` must be warning-free before a change
lands; CI runs them with `RUSTFLAGS=-D warnings`. Warnings from dependencies are
out of scope. [VALIDATION.md](VALIDATION.md) describes every check and how to
run it.

On a machine with limited memory, limit parallel jobs: `cargo build -j 4`, or
`CARGO_JOBS=4 ./install.sh build`.

## Translations

This is the easiest way to help and the most visible.

1. Copy `i18n/en/stellarshot.ftl` to `i18n/<locale>/stellarshot.ftl`.
2. Translate the values. Leave the keys alone.
3. Keep every `{ $placeholder }` intact and spelled the same. A mangled
   placeholder only misbehaves in that one language, at runtime.
4. Run `cargo test --test i18n`.

`tests/i18n.rs` reports missing keys, keys that no longer exist, duplicates,
and placeholder mismatches. It is not advisory: CI runs it.

**When you change a translatable string, change it in every locale in the same
commit.** Fluent falls back silently, so a key added only to `en` shows up as
stray English elsewhere rather than as a build error.

## Code

A few conventions this codebase holds to. Each one exists because the
alternative caused a real problem.

**Never delete what you did not create.** Anything that removes files removes
only entries it can prove are its own, and does not follow symlinks. Upstream
Stellarshot deleted a repository with `remove_dir_all` on its folder; see
`src/engine/location.rs` for why that could have taken a home directory.

**Only the engine talks to rustic.** No `rustic_core` type may appear outside
`src/engine/`. Everything else works with the engine's plain types, so a
rustic upgrade is contained in one module.

**Writes run in a child process.** Anything that writes to a repository goes
through `stellarshot --run` and holds the repository lock. rustic cannot be
interrupted; a process can.

**Prove behaviour against real files.** Tests that delete, write or restore
work on real directories in temporary folders, and assert on what must survive
as well as what must change.

**A check that cannot fail is not a check.** `tests/i18n.rs` fails if the
fallback locale is missing; `scripts/verify-release-build.sh` fails if the build
without the feature does not contain the log path. Hold new tests to the same
standard.

**User-facing strings go through `fl!`.** No hardcoded English in the UI,
including error messages. Where an error's detail comes from the backup engine
and cannot be translated, show a localized explanation with the detail beneath
it.

**Settings versus constants.** Anything a user would reasonably change belongs
in `cosmic-config`, with a UI control. Implementation tuning values belong in
`src/constants.rs` as compile-time constants. Do not add a second runtime
configuration file.

**Package metadata comes from the manifest.** Version, licence and links are
read with `env!("CARGO_PKG_*")`, so they cannot drift from `Cargo.toml`.

**Debug logging goes to a file, behind `debug_log!`.** Scheduled backups run
under systemd, where stderr is rarely read. Genuine errors go to stderr as well,
via `error_log!`.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`,
`docs:`, `chore:`, `build:`, `refactor:`. Only `feat:` and `fix:` bump the
version.

Write a body explaining *why*, not what; the diff already says what.

Keep the README current in the same commit as the change. If a feature is added
or observable behaviour changes, the feature list, usage, settings and
troubleshooting sections need to match, and so does [CHANGELOG.md](CHANGELOG.md).

## Licence

By contributing you agree that your work is licensed under GPL-3.0-only, the
same as the project.
