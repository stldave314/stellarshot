# M0: Fork foundation, implementation plan

**Goal:** Turn the upstream checkout into an independently shippable project. That means its own identity, a safe Delete, warning-free CI, tag-triggered `.deb`/`.rpm`/tarball releases, developer logging that releases can't carry, and complete documentation. No feature work yet.

**Architecture:** The app's behavior stays the same apart from the Delete safety fix. M0 adds the scaffolding every later milestone relies on:
- `src/debug.rs` (the `debug_log!` / `error_log!` macros)
- `src/constants.rs` (tuning values)
- `install.sh` (the one build/install/package path)
- `scripts/` (release-strip and metadata validators)
- `tests/i18n.rs` (locale parity)
- Two workflows: CI and Release

**Tech stack:** Rust 1.95 (edition 2021 for now; M1 moves to 2024 alongside the rustic bump), libcosmic (git), rustic_core 0.2 (unchanged until M1), cargo-deb, cargo-generate-rpm, GitHub Actions.

**Spec:** `docs/specs/2026-09-23-stellarshot-roadmap-design.md`

## Global constraints

- App ID: `io.github.stldave314.Stellarshot`. The binary name stays `stellarshot`.
- License stays **GPL-3.0-only**. SPDX headers are `GPL-3.0-only`, and the Cargo `license` becomes the valid SPDX id `GPL-3.0-only`.
- There's no personal email anywhere. Where a maintainer email is required, use `stldave314@users.noreply.github.com`.
- Package metadata (name, version, repository URL) comes from `env!("CARGO_PKG_*")`, never hard-coded.
- Zero warnings from `cargo check` and `cargo clippy --all-targets` with `RUSTFLAGS=-D warnings`.
- Every user-facing string goes through `fl!`. Every locale has the same key set as `en`.
- Every packaging target passes `--features release-build`.
- Nothing is committed that names the tooling used to produce it.
- Local heavy commands run capped: `systemd-run --user --scope -q -p MemoryMax=4G -p CPUQuota=300% nice -n 10 cargo … -j 4`.

## Review focus

1. **Deleting a repository whose path is `$HOME`, `/`, or a folder with other content.** Only the repository's own entries (`config`, `keys/`, `data/`, `index/`, `snapshots/`, `locks/`) may be removed, and the folder itself only if it's then empty. Test: `delete_repository_leaves_foreign_files`.
2. **Creating a repository in a non-empty folder that isn't a repository**, for example picking `~`. It must be refused with a message, not initialized in place. Test: `init_refuses_non_empty_non_repository`.
3. **A user who ran upstream Stellarshot.** Their repository list must survive the app-ID change, copied once from the old cosmic-config directory without overwriting a newer one. Tests: `migrates_old_config_once`, `does_not_overwrite_existing_new_config`.
4. **Paths with spaces.** The file chooser returns percent-encoded URLs, and `Url::path()` keeps `%20`. Use `to_file_path()`. Test: `url_with_space_becomes_real_path`.
5. **A locale drifting from `en`.** It silently shows English at runtime. Test: `tests/i18n.rs`, which can't pass vacuously.

---

### Task 1: Rebrand identity and the About page

**Files:**
- Rename: `res/com.github.cosmic-utils.Stellarshot.{desktop,metainfo.xml}` → `res/io.github.stldave314.Stellarshot.{desktop,metainfo.xml}`
- Rename: `res/icons/hicolor/scalable/apps/com.github.cosmic-utils.Stellarshot.svg` → `…/io.github.stldave314.Stellarshot.svg`. Add `…-symbolic.svg` (a copy of the bundled `harddisk-symbolic.svg` shape).
- Rename: `res/icons/com.github.cosmic-utils.Stellarshot.Source.svg` → `res/icons/io.github.stldave314.Stellarshot.Source.svg`
- Modify: `Cargo.toml` (`license`, `description`, `repository`, `homepage`, `rust-version`, `keywords`, `categories`, `authors` omitted)
- Modify: `src/app.rs` (`APP_ID`, the About builder)

The About page reads its version, license and links from the manifest:

```rust
let about = About::default()
    .name(fl!("stellarshot"))
    .icon(Self::APP_ID)
    .version(env!("CARGO_PKG_VERSION"))
    .license(env!("CARGO_PKG_LICENSE"))
    .links([
        (fl!("repository"), env!("CARGO_PKG_REPOSITORY")),
        (fl!("support"), concat!(env!("CARGO_PKG_REPOSITORY"), "/issues")),
    ])
    .developers([("Aaron Honeycutt", ""), ("Eduardo Flores", "")])
    .comments(fl!("about-credits"));
```

`about-credits` = "Based on Stellarshot by the cosmic-utils project." It's added to every locale.

Metainfo:
- A single `homepage`, `bugtracker` and `vcs-browser` (the empty duplicate `<url>` tags are removed).
- `<developer id="io.github.stldave314"><name>stldave314</name></developer>`, with no `update_contact`.
- `<branding>`, `<releases>` with `0.1.0`, and two `<screenshot>` entries with captions (dark default, light).
- `<provides><binary>stellarshot</binary></provides>`. The false `text/plain` MIME claim is dropped.

Desktop entry:
- `Exec=stellarshot` (it doesn't accept files, so `%F` and `MimeType=` are dropped).
- `Categories=Utility;Archiving;`
- `Icon=io.github.stldave314.Stellarshot`

**Verification:** `scripts/validate-metadata.sh` (Task 5) passes.

### Task 2: Debug logging and constants modules

**Files:**
- Create: `src/debug.rs`. This is the standard module:
  - `DEVELOPER_LOGGING = false`, `ENABLED = DEVELOPER_LOGGING && !cfg!(feature = "release-build")`, `PATH = "/tmp/stellarshot-debug.log"`.
  - Categories `ENGINE`, `UI`, `CONFIG`, `RCLONE`, `SCHED`, `IMPORT`, `LOCK`.
  - The `debug_log!` / `error_log!` macros.
  - Truncated once per launch, with elapsed-time prefixes.
- Create: `src/constants.rs`. It starts with the only tuning values M0 uses: `WINDOW_WIDTH = 800.0`, `WINDOW_HEIGHT = 800.0`, `WINDOW_MIN_WIDTH = 400.0`, `WINDOW_MIN_HEIGHT = 180.0`. It grows per milestone and never holds unused constants.
- Modify: `Cargo.toml` `[features] release-build = []`
- Modify: every `println!`, `log::error!`, `eprintln!` in `src/` → `debug_log!` for diagnostics, `error_log!` for genuine errors.
- Modify: `src/app/settings.rs`, which no longer overwrites `RUST_LOG` (that also removes the `unsafe` `set_var`, which is `unsafe` in edition 2024).

**Test (in `src/debug.rs`):** `enabled_is_off_in_this_build`, which asserts `!ENABLED` when `DEVELOPER_LOGGING` is false. The strip guarantee itself is proven by `scripts/verify-release-build.sh` (Task 5).

### Task 3: Safe repository creation and deletion (review focus 1, 2, 4)

**Files:**
- Create: `src/backup/location.rs`

```rust
/// Entries rustic creates at the root of a repository. Nothing else is ever deleted.
pub const REPOSITORY_ENTRIES: &[&str] = &["config", "keys", "data", "index", "snapshots", "locks"];

pub enum InitCheck { Empty, ExistingRepository, NotEmpty }

/// Classify a folder before a repository is created in it.
pub fn check_init_location(path: &Path) -> std::io::Result<InitCheck>;

/// Remove only the repository's own entries, then the folder if it is left empty.
/// Returns the entries that were left in place (anything that is not ours).
pub fn delete_repository(path: &Path) -> std::io::Result<Vec<PathBuf>>;

/// `file://` URL from the portal → real path, decoding percent escapes.
pub fn url_to_path(url: &Url) -> Option<PathBuf>;
```

- Modify: `src/backup/init.rs`. It refuses `InitCheck::NotEmpty` with `Error::LocationNotEmpty(path)`.
- Modify: `src/app.rs`:
  - `DeleteRepository` calls `delete_repository` and reports errors in a dialog (`fl!("delete-failed")`). It no longer calls `remove_dir_all`.
  - The portal URL goes through `url_to_path`.
  - A failed init removes the placeholder nav item and shows the error.
- Modify: `src/app/views/content.rs`. `ReloadSnapshots` uses the stored password, not the literal `"password"`. `Select` no longer `todo!()`s.

**Tests (in `location.rs`, using `tempfile::TempDir`):**
- `init_refuses_non_empty_non_repository`: a folder containing `notes.txt` → `NotEmpty`.
- `init_accepts_empty_and_existing`: empty → `Empty`, folder with a `config` file plus `keys/` → `ExistingRepository`.
- `delete_repository_leaves_foreign_files`: the repository entries plus `Documents/report.odt` → the entries are gone, `Documents/report.odt` survives, the folder survives, and the returned list contains `Documents`.
- `delete_repository_removes_empty_folder`: only repository entries → the folder is gone.
- `url_with_space_becomes_real_path`: `file:///tmp/My%20Backups` → `/tmp/My Backups`.

### Task 4: Config migration from the upstream app ID (review focus 3)

**Files:**
- Create: `src/app/migrate.rs`

```rust
pub const OLD_APP_ID: &str = "com.github.cosmic-utils.Stellarshot";

/// Copy `<config_root>/cosmic/<OLD_APP_ID>/v<version>/*` into the new ID's directory,
/// once, and only if the new directory does not exist yet.
pub fn migrate_app_id(config_root: &Path, new_app_id: &str, version: u64) -> std::io::Result<bool>;
```

It's called from `settings::init()` with `dirs::config_dir()` (the `XDG_CONFIG_HOME` resolution cosmic-config uses) before the config is read.

**Tests (temp dir as `config_root`):** `migrates_old_config_once`, `does_not_overwrite_existing_new_config`, `no_old_config_is_a_no_op`.

### Task 5: install.sh, packaging metadata and validation scripts

**Files:**
- Create: `install.sh`, adapted from the standard script:
  - Commands: `build`, `install`, `uninstall`, `deb`, `rpm`, `tarball`, `package`.
  - `FEATURES=release-build` on every target. `cargo deb -- --features release-build`.
- Delete: `justfile`. `install.sh` becomes the single build path, and spec §2 is updated to match.
- Modify: `Cargo.toml` `[package.metadata.deb]`:
  - `maintainer = "stldave314 <stldave314@users.noreply.github.com>"`
  - `depends = "$auto"` and `recommends = "rclone"` (rclone becomes required in M3)
  - Assets: binary, desktop entry, both icons, metainfo, README.
  - Plus `[package.metadata.generate-rpm]` with the same assets.
- Modify: `Cargo.toml` `[profile.release]`: `lto = "thin"`, `strip = true`, `codegen-units = 4`.
- Create: `scripts/verify-release-build.sh`. It builds with `DEVELOPER_LOGGING` forced on, with and without `release-build`. The log path must be present without the feature and absent with it, and the script fails if it's absent from both.
- Create: `scripts/validate-metadata.sh`:
  - `desktop-file-validate`, then `appstreamcli validate --no-net`.
  - Screenshots are present, each with a caption, and there's exactly one default.
  - The newest `<release>` equals the Cargo version.
  - The IDs agree across the metainfo, desktop entry and icons.
  - Missing files are a failure, not a skip.

**Verification:** `./install.sh package` produces `dist/stellarshot_0.1.0-1_amd64.deb`, `dist/stellarshot-0.1.0-1.x86_64.rpm` and `dist/stellarshot-0.1.0-x86_64.tar.gz`. `dpkg-deb -c` lists every asset, and `scripts/verify-release-build.sh` prints `RESULT: release-build strips developer logging`.

### Task 6: Tests and locale parity (review focus 5)

**Files:**
- Modify: `src/backup/init.rs` and `src/backup/snapshot.rs` tests. They use `tempfile::TempDir` for the repository and a fixture tree written into another temp dir, not `/tmp/test` and `/etc`.
- Create: `tests/i18n.rs`. It's the standard parity test with `DOMAIN = "stellarshot"`: the fallback exists and isn't empty, key sets match, placeholders match, and there are no duplicate keys.
- Modify: `i18n/{de,gsw,sv,bg}/stellarshot.ftl`. Add the missing keys (translated), remove the orphaned `git-description`, and add `about-credits`, `delete-failed`, `location-not-empty` everywhere.
- Add dev-dependency: `tempfile`.

### Task 7: CI and release workflows

**Files:**
- Delete: `.github/workflows/build.yml`
- Create: `.github/workflows/ci.yml`:
  - Runs on push to main, PRs, and manual dispatch, with `RUSTFLAGS=-D warnings`.
  - `test` job: build deps, `cargo fmt --check`, `cargo clippy --all-targets --all-features`, `cargo test --all-features`, a release build, then `scripts/verify-release-build.sh`.
  - `metadata` job: `scripts/validate-metadata.sh`.
- Create: `.github/workflows/release.yml`:
  - Triggered by a `v*` tag or manual dispatch with a tag.
  - Test, verify the strip, run `./install.sh package`, then publish `dist/*.deb`, `*.rpm`, `*.tar.gz` with `softprops/action-gh-release@v2`.

**Verification:** after the push, `gh run watch` shows CI green.

### Task 8: Documentation

**Files:**
- Rewrite: `README.md`, following the standard layout:
  - The problem it solves and what it does.
  - Screenshots (`docs/screenshots/`) and a status line.
  - Requirements, installing, using it, settings, troubleshooting, known limitations, how it works, building, the tests that can't pass vacuously, contributing, license and credits.
  - Links to ROADMAP, CHANGELOG, SECURITY, VALIDATION and CONTRIBUTING.
- Create: `ROADMAP.md` (spec §3 in public form), `CHANGELOG.md` (Keep a Changelog, `Unreleased`), `SECURITY.md` (threat model, reporting, what's protected and what isn't), `VALIDATION.md` (every automated and manual check, what it proves, how to run it), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
- Move: `res/screenshots/*` → `docs/screenshots/`. The metainfo screenshot URLs point at the raw files there.

### Task 9: Verify, commit, push

- [ ] `cargo fmt --all`
- [ ] Capped: `cargo clippy --all-targets --all-features -j 4` → zero warnings
- [ ] Capped: `cargo test --all-features -j 4` → all pass
- [ ] `scripts/verify-release-build.sh`, `scripts/validate-metadata.sh`, `./install.sh package`
- [ ] Grep the tree for personal email and tooling references
- [ ] Commit on `main` (`feat:` for the identity and safety changes, `build:`/`docs:` for the rest), push, and watch CI to green
