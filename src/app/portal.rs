// SPDX-License-Identifier: GPL-3.0-only

//! Results from the XDG file-chooser portal.

use std::path::PathBuf;

use ashpd::desktop::file_chooser::SelectedFiles;
use ashpd::url::Url;

/// Convert a `file://` URL from the file-chooser portal into a real path.
///
/// `Url::path()` keeps percent escapes, so a folder called `My Backups` comes
/// back as `My%20Backups` — a different directory. `to_file_path` decodes it.
pub fn url_to_path(url: &Url) -> Option<PathBuf> {
    url.to_file_path().ok()
}

/// The local paths a portal response refers to. Anything that is not a local
/// file is dropped.
pub fn selected_paths(result: ashpd::Result<SelectedFiles>) -> Result<Vec<PathBuf>, String> {
    let files = result.map_err(|err| err.to_string())?;
    Ok(files.uris().iter().filter_map(url_to_path).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_with_space_becomes_real_path() {
        let url = Url::parse("file:///tmp/My%20Backups").unwrap();
        assert_eq!(url_to_path(&url), Some(PathBuf::from("/tmp/My Backups")));
        // What upstream stored instead:
        assert_eq!(url.path(), "/tmp/My%20Backups");
    }

    #[test]
    fn non_file_urls_are_rejected() {
        let url = Url::parse("https://example.com/backup").unwrap();
        assert_eq!(url_to_path(&url), None);
    }
}
