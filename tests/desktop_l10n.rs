// SPDX-License-Identifier: GPL-3.0-only
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "tests and demos state their expectations by panicking"
)]

//! The desktop entries and the store listing's summary say what the locale
//! files say, in every locale, so a translation changed in one place cannot
//! drift from the other. The Fluent keys are the source; `res/` carries
//! copies a launcher or software center can read without running anything.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every locale but the fallback, whose text is the unlocalized key.
fn translations() -> Vec<String> {
    let mut locales: Vec<String> = std::fs::read_dir(root().join("i18n"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|locale| locale != "en")
        .collect();
    locales.sort();
    assert!(!locales.is_empty(), "no translations found to check");
    locales
}

/// `key`'s single-line value in `locale`'s file.
fn fluent(locale: &str, key: &str) -> String {
    let path = root().join("i18n").join(locale).join("stellarshot.ftl");
    let text = std::fs::read_to_string(&path).unwrap();
    let prefix = format!("{key} = ");
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("{} has no {key}", path.display()))
        .to_owned()
}

/// `field` in `section` of the desktop entry at `path`.
fn desktop(path: &Path, section: &str, field: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut current = "";
    for line in text.lines() {
        if line.starts_with('[') {
            current = line;
        } else if current == section
            && let Some(value) = line.strip_prefix(&format!("{field}="))
        {
            return Some(value.to_owned());
        }
    }
    None
}

fn check_field(path: &Path, section: &str, field: &str, key: &str) {
    assert_eq!(
        desktop(path, section, field).as_deref(),
        Some(fluent("en", key).as_str()),
        "{} {section} {field} must match {key} in en",
        path.display()
    );
    for locale in translations() {
        assert_eq!(
            desktop(path, section, &format!("{field}[{locale}]")),
            Some(fluent(&locale, key)),
            "{} {section} {field}[{locale}] must match {key} in {locale}",
            path.display()
        );
    }
}

#[test]
fn the_app_entry_matches_the_locale_files() {
    let path = root().join("res/io.github.stldave314.Stellarshot.desktop");
    check_field(&path, "[Desktop Entry]", "Comment", "desktop-comment");
    check_field(&path, "[Desktop Entry]", "Keywords", "desktop-keywords");
    check_field(
        &path,
        "[Desktop Action new-backup]",
        "Name",
        "desktop-new-backup",
    );
    check_field(&path, "[Desktop Action restore]", "Name", "desktop-restore");
}

#[test]
fn the_applet_entry_matches_the_locale_files() {
    let path = root().join("res/io.github.stldave314.Stellarshot.Applet.desktop");
    check_field(&path, "[Desktop Entry]", "Comment", "applet-comment");
}

#[test]
fn the_store_summary_matches_the_locale_files() {
    let path = root().join("res/io.github.stldave314.Stellarshot.metainfo.xml");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains(&format!(
            "<summary>{}</summary>",
            fluent("en", "metainfo-summary")
        )),
        "the summary must match metainfo-summary in en"
    );
    for locale in translations() {
        let expected = format!(
            "<summary xml:lang=\"{locale}\">{}</summary>",
            fluent(&locale, "metainfo-summary")
        );
        assert!(text.contains(&expected), "missing or stale: {expected}");
    }
}
