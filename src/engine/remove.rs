// SPDX-License-Identifier: GPL-3.0-only

//! Taking files out of snapshots that already exist: something backed up by
//! mistake (a secret, a huge download) removed from history, and its data
//! erased from the repository.

use std::path::{Component, Path, PathBuf};

use rustic_core::repofile::SnapshotFile;
use rustic_core::{Excludes, IndexedFullStatus, Repository, RewriteOptions, RewriteTreesOptions};
use serde::{Deserialize, Serialize};

use super::backup::literal_glob;
use super::browse::node_at;
use super::error::{EngineError, ErrorKind};
use super::maintenance::{belongs_to_profile, erase_unused, hostname};
use super::repo::Repo;
use crate::debug::ENGINE;
use crate::debug_log;

/// Files and folders to take out of one backup's snapshots, as the
/// snapshots record them: absolute paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveRequest {
    pub paths: Vec<PathBuf>,
    /// Only this snapshot (an ID or unique prefix); every one of the
    /// backup's own if `None`.
    pub snapshot: Option<String>,
    /// The backup's own tag and sources, as [`Repo::forget`] uses them: what
    /// its snapshots are, and so whether anything else uses the repository
    /// too. Another computer's or backup's snapshots are never changed.
    pub tag: String,
    pub sources: Vec<PathBuf>,
}

/// What removing did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveReport {
    /// Snapshots that held at least one of the paths, now saved without
    /// them (under new IDs).
    pub rewritten: u64,
    /// Bytes of data no snapshot needed any more, erased afterwards.
    pub erased: u64,
    /// The repository is shared with another computer or backup, so the
    /// unused data was only marked: rustic deletes it at a clean-up a day or
    /// more later (see [`erase_unused`]).
    pub deferred: bool,
    /// Snapshots that still hold one of the paths and were not changed:
    /// another computer's or backup's, or this backup's others when only
    /// one snapshot was asked for. Their data stays in the repository.
    pub still_held: u64,
}

impl Repo {
    /// Take `request.paths` out of the backup's snapshots (or the one asked
    /// for), then erase every piece of data no snapshot needs any more (see
    /// [`erase_unused`]), so what was removed is actually gone rather than
    /// only unlisted.
    ///
    /// Only snapshots that actually hold one of the paths are rewritten:
    /// rustic would otherwise save a new copy of any snapshot whose recorded
    /// totals it counts differently, changing IDs for nothing. Each one
    /// rewritten keeps its name, notes, tags and pin. The erase runs even
    /// when none holds them any more, so trying again after a failure part
    /// way (rewritten, but not yet erased) finishes the job. Consumes the
    /// repository, since rewriting needs its full index.
    ///
    /// Cannot be undone, and refused by an append-only repository.
    pub fn remove_paths(self, request: &RemoveRequest) -> Result<RemoveReport, EngineError> {
        let globs = request
            .paths
            .iter()
            .map(|path| exclude_glob(path))
            .collect::<Result<Vec<_>, _>>()?;
        if globs.is_empty() {
            return Ok(RemoveReport::default());
        }
        let host = hostname();
        let ours = |snapshot: &SnapshotFile| {
            belongs_to_profile(snapshot, &host, &request.tag, &request.sources)
        };
        let all = self.inner.get_all_snapshots()?;
        // Any snapshot that is not this backup's means something else writes
        // here too, perhaps at this very moment: see `erase_unused`.
        let shared = all.iter().any(|snapshot| !ours(snapshot));
        // One snapshot, if asked for, but only ever one of this backup's own:
        // the restore page lists every snapshot in the repository.
        let wanted: Vec<_> = match &request.snapshot {
            Some(id) => self
                .inner
                .get_snapshots(&[id])?
                .into_iter()
                .filter(|snapshot| ours(snapshot))
                .map(|snapshot| snapshot.id)
                .collect(),
            None => all
                .iter()
                .filter(|snapshot| ours(snapshot))
                .map(|snapshot| snapshot.id)
                .collect(),
        };
        // Kept until the end: the rclone process the repository talks to.
        let _serve = self.serve;
        let repo = self.inner.to_indexed()?;
        let mut holding = Vec::new();
        let mut still_held = 0;
        for snapshot in all {
            if holds_any(&repo, &snapshot, &request.paths)? {
                if wanted.contains(&snapshot.id) {
                    holding.push(snapshot);
                } else {
                    still_held += 1;
                }
            }
        }
        let rewritten = if holding.is_empty() {
            debug_log!(
                ENGINE,
                "remove: no snapshot asked for holds any of the paths"
            );
            0
        } else {
            let tree_options =
                RewriteTreesOptions::default().excludes(Excludes::default().globs(globs));
            let options = RewriteOptions::default().forget(true);
            repo.rewrite_snapshots_and_trees(holding, &options, &tree_options)?
                .len() as u64
        };
        // Looked at again right before deleting, as late as possible: a
        // snapshot another computer saved while this ran means it writes
        // here too. What no check can see is another computer's very first
        // backup still running, since a snapshot is written last and rustic
        // has no lock to look for; the README says so.
        let shared = shared
            || repo
                .get_all_snapshots()?
                .iter()
                .any(|snapshot| !ours(snapshot));
        debug_log!(
            ENGINE,
            "remove: rewrote {rewritten}, {still_held} others still hold them, shared: {shared}"
        );
        let erased = erase_unused(&repo, !shared)?.bytes;
        Ok(RemoveReport {
            rewritten,
            erased,
            deferred: shared,
            still_held,
        })
    }
}

/// Whether `snapshot` has any of `paths`. Only a path that is not there
/// counts as "no": any other failure to read the snapshot's folders is
/// reported, rather than taken to mean there is nothing to remove.
fn holds_any(
    repo: &Repository<IndexedFullStatus>,
    snapshot: &SnapshotFile,
    paths: &[PathBuf],
) -> Result<bool, EngineError> {
    for path in paths {
        match node_at(repo, snapshot, path) {
            Ok(_) => return Ok(true),
            Err(err) if err.kind == ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }
    }
    Ok(false)
}

/// An exclude pattern for rustic's rewrite that matches exactly `path` and
/// nothing else.
///
/// rustic's patterns are overrides in gitignore syntax, where a pattern is
/// a **whitelist** unless it starts with `!`: a pattern without the `!`
/// would remove everything *except* the path. Every pattern made here is
/// `!` and the absolute path, whose leading slash anchors it at the
/// snapshot's root, written through [`literal_glob`] so a file named `a*b`
/// never takes `aXb` with it.
pub(super) fn exclude_glob(path: &Path) -> Result<String, EngineError> {
    let unsafe_path = || EngineError::new(ErrorKind::UnsafePath, path.display().to_string());
    if !path.is_absolute() {
        return Err(unsafe_path());
    }
    let mut named = false;
    for component in path.components() {
        match component {
            Component::RootDir => {}
            Component::Normal(_) => named = true,
            _ => return Err(unsafe_path()),
        }
    }
    // Only `/` itself: that would be everything.
    if !named {
        return Err(unsafe_path());
    }
    Ok(format!("!{}", literal_glob(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_excludes_and_is_anchored() {
        assert_eq!(
            exclude_glob(Path::new("/home/alex/notes.txt")).unwrap(),
            "!/home/alex/notes.txt"
        );
    }

    #[test]
    fn pattern_characters_in_a_name_are_matched_literally() {
        assert_eq!(
            exclude_glob(Path::new("/d/a*b?[c]{d,e}!\\ ")).unwrap(),
            r"!/d/a\*b\?\[c\]\{d,e\}\!\\\ "
        );
    }

    #[test]
    fn the_root_a_relative_path_and_dot_dot_are_refused() {
        for path in ["/", "relative/file", "/home/../etc"] {
            let err = exclude_glob(Path::new(path)).unwrap_err();
            assert_eq!(err.kind, ErrorKind::UnsafePath, "{path}");
        }
    }
}
