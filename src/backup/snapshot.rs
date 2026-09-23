// SPDX-License-Identifier: GPL-3.0-only

use crate::debug::ENGINE;
use crate::{debug_log, Error};
use rustic_backend::BackendOptions;
use rustic_core::{
    repofile::SnapshotFile, BackupOptions, PathList, Repository, RepositoryOptions, SnapshotOptions,
};

pub fn snapshot(repository: &str, password: &str, paths: Vec<&str>) -> Result<(), Error> {
    let backends = BackendOptions::default()
        .repository(repository)
        .to_backends()?;
    let repo_opts = RepositoryOptions::default().password(password);
    let repo = Repository::new(&repo_opts, backends)?
        .open()?
        .to_indexed_ids()?;
    debug_log!(ENGINE, "opened {repository} for backup of {paths:?}");

    let backup_opts = BackupOptions::default();
    let source = PathList::from_strings(paths).sanitize()?;
    let snap = SnapshotOptions::default().to_snapshot()?;

    let snap = repo.backup(&backup_opts, &source, snap)?;
    debug_log!(ENGINE, "created snapshot {}", snap.id);
    Ok(())
}

pub fn fetch(repository: &str, password: &str) -> Result<Vec<SnapshotFile>, Error> {
    let backends = BackendOptions::default()
        .repository(repository)
        .to_backends()?;
    let repo_opts = RepositoryOptions::default().password(password);
    let repo = Repository::new(&repo_opts, backends)?
        .open()?
        .to_indexed_ids()?;

    let snapshots = repo.get_all_snapshots()?;
    debug_log!(ENGINE, "{repository}: {} snapshots", snapshots.len());
    Ok(snapshots)
}

pub fn delete(
    repository: &str,
    password: &str,
    snapshots: Vec<rustic_core::Id>,
) -> Result<(), Error> {
    let backends = BackendOptions::default()
        .repository(repository)
        .to_backends()?;
    let repo_opts = RepositoryOptions::default().password(password);
    let repo = Repository::new(&repo_opts, backends)?
        .open()?
        .to_indexed_ids()?;

    debug_log!(ENGINE, "{repository}: deleting {snapshots:?}");
    repo.delete_snapshots(&snapshots)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn backs_up_a_folder_and_lists_then_deletes_the_snapshot() {
        let repo_dir = TempDir::new().unwrap();
        let source = TempDir::new().unwrap();
        std::fs::write(source.path().join("a file.txt"), b"hello").unwrap();
        std::fs::create_dir(source.path().join("sub")).unwrap();
        std::fs::write(source.path().join("sub/b.txt"), b"world").unwrap();

        let repo_path = repo_dir.path().join("repo");
        crate::backup::init(&repo_path, "password").unwrap();
        let repo = repo_path.to_str().unwrap();

        snapshot(repo, "password", vec![source.path().to_str().unwrap()]).unwrap();

        let snapshots = fetch(repo, "password").unwrap();
        assert_eq!(snapshots.len(), 1);

        delete(repo, "password", vec![snapshots[0].id]).unwrap();
        assert!(fetch(repo, "password").unwrap().is_empty());
    }

    #[test]
    fn a_wrong_password_is_an_error() {
        let repo_dir = TempDir::new().unwrap();
        let repo_path = repo_dir.path().join("repo");
        crate::backup::init(&repo_path, "password").unwrap();

        assert!(fetch(repo_path.to_str().unwrap(), "wrong").is_err());
    }
}
