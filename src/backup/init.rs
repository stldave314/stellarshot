// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use crate::backup::location::{check_init_location, InitCheck};
use crate::debug::ENGINE;
use crate::{debug_log, Error};
use rustic_backend::BackendOptions;
use rustic_core::{ConfigOptions, KeyOptions, Repository, RepositoryOptions};

/// Create a repository at `repository`, or open the one already there.
///
/// Refuses a folder that holds anything other than a repository, so a
/// repository is never written into the middle of someone's files.
pub fn init(repository: &Path, password: &str) -> Result<(), Error> {
    let check = check_init_location(repository)?;
    debug_log!(ENGINE, "init {}: {check:?}", repository.display());
    if check == InitCheck::NotEmpty {
        return Err(Error::LocationNotEmpty(repository.to_path_buf()));
    }

    let location = repository
        .to_str()
        .ok_or_else(|| Error::NonUtf8Path(repository.to_path_buf()))?;
    let backends = BackendOptions::default()
        .repository(location)
        .to_backends()?;
    let repo_opts = RepositoryOptions::default().password(password);

    match check {
        // Opening proves the password is right; a wrong one is an error here,
        // never a reason to initialise over the existing repository.
        InitCheck::ExistingRepository => {
            Repository::new(&repo_opts, backends)?.open()?;
        }
        InitCheck::Empty | InitCheck::NotEmpty => {
            Repository::new(&repo_opts, backends)?
                .init(&KeyOptions::default(), &ConfigOptions::default())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::location::is_repository;
    use tempfile::TempDir;

    #[test]
    fn creates_a_repository_in_a_new_folder() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");

        init(&repo, "password").unwrap();

        assert!(is_repository(&repo));
    }

    #[test]
    fn reopens_an_existing_repository_with_the_right_password() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        init(&repo, "password").unwrap();

        init(&repo, "password").unwrap();
    }

    #[test]
    fn a_wrong_password_never_reinitialises() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("repo");
        init(&repo, "password").unwrap();
        let config_before = std::fs::read(repo.join("config")).unwrap();

        assert!(init(&repo, "wrong").is_err());

        assert_eq!(std::fs::read(repo.join("config")).unwrap(), config_before);
    }

    #[test]
    fn refuses_a_folder_with_other_files() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("notes.txt"), b"mine").unwrap();

        let err = init(tmp.path(), "password").unwrap_err();

        assert!(matches!(err, Error::LocationNotEmpty(_)));
        assert!(!tmp.path().join("config").exists());
    }
}
