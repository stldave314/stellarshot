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
    let Some(settings) = settings.as_ref() else {
        return;
    };
    options.no_cache = settings.disabled;
    if let Some(dir) = &settings.dir {
        options.cache_dir = Some(dir.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests share one process-wide global, so they must not run
    // concurrently with each other; `serial` orders them by hand rather
    // than adding a crate just for that.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[test]
    fn nothing_set_leaves_the_defaults_alone() {
        let _guard = serial();
        *SETTINGS.write().unwrap() = None;
        let mut options = rustic_core::RepositoryOptions::default();

        apply(&mut options);

        assert!(!options.no_cache);
        assert_eq!(options.cache_dir, None);
    }

    #[test]
    fn a_chosen_directory_and_no_cache_both_reach_the_options() {
        let _guard = serial();
        set(Some(PathBuf::from("/mnt/cache")), true);
        let mut options = rustic_core::RepositoryOptions::default();

        apply(&mut options);

        assert!(options.no_cache);
        assert_eq!(options.cache_dir, Some(PathBuf::from("/mnt/cache")));
    }
}
