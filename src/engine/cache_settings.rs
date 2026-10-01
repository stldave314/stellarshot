// SPDX-License-Identifier: GPL-3.0-only

//! Where rustic keeps its local cache of index data, across every
//! repository this process opens.
//!
//! A machine-wide preference, not a per-backup one, so it lives here as a
//! small global rather than threaded through every call to [`super::open`]
//! and [`super::init`] from the window, the scheduler and each `--run`
//! child process alike. Set once from `StellarshotConfig` when a process
//! starts, and updated when the user changes it in Settings.

use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CacheSettings {
    /// Use this folder instead of rustic's own default cache location.
    dir: Option<PathBuf>,
    /// Do not cache at all.
    disabled: bool,
}

static SETTINGS: RwLock<Option<CacheSettings>> = RwLock::new(None);

/// Set the cache location for every repository this process opens from now
/// on. `dir` and `disabled` are mutually exclusive in the settings UI, but
/// both are accepted here; rustic itself refuses both being set together.
pub fn set(dir: Option<PathBuf>, disabled: bool) {
    let mut settings = SETTINGS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *settings = Some(CacheSettings { dir, disabled });
}

pub(super) fn apply(options: &mut rustic_core::RepositoryOptions) {
    let settings = SETTINGS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    apply_to(settings.as_ref(), options);
}

/// The pure part of [`apply`], taking the settings as an argument so it can
/// be tested with local values rather than by writing the process-wide
/// global — which every other test in the same binary that opens a
/// repository also reads, so a test that left it set (or cleared) would
/// silently change where all of them cached, depending on which ran first.
fn apply_to(settings: Option<&CacheSettings>, options: &mut rustic_core::RepositoryOptions) {
    let Some(settings) = settings else {
        return;
    };
    options.no_cache = settings.disabled;
    if let Some(dir) = &settings.dir {
        options.cache_dir = Some(dir.clone());
    }
}

/// Where a test build keeps rustic's cache when the settings above have
/// not chosen anywhere: under this crate's own `target/`, per process,
/// never the real `~/.cache/rustic`. Every repository a test creates is
/// thrown away with the `TempDir` that held it, but rustic's cache is keyed
/// by repository ID and would otherwise gain one stale entry per test run,
/// forever.
///
/// A location, not `no_cache = true` (which is what a test build used to
/// force, for every repository, in every test): with the cache simply off,
/// no test ever exercised the code path a real backup takes, and a stale
/// index or snapshot cache after a forget, a prune or a password change is
/// exactly the kind of bug that would have hidden behind it. Reached by
/// every `--run` and `--scheduled` child a test spawns too, since
/// `cargo test` builds those with this same feature.
#[cfg(feature = "test-support")]
pub(super) fn test_cache_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/target/test-cache"))
        .join(std::process::id().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_set_leaves_the_defaults_alone() {
        let mut options = rustic_core::RepositoryOptions::default();

        apply_to(None, &mut options);

        assert!(!options.no_cache);
        assert_eq!(options.cache_dir, None);
    }

    #[test]
    fn a_chosen_directory_and_no_cache_both_reach_the_options() {
        let settings = CacheSettings {
            dir: Some(PathBuf::from("/mnt/cache")),
            disabled: true,
        };
        let mut options = rustic_core::RepositoryOptions::default();

        apply_to(Some(&settings), &mut options);

        assert!(options.no_cache);
        assert_eq!(options.cache_dir, Some(PathBuf::from("/mnt/cache")));
    }
}
