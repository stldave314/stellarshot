// SPDX-License-Identifier: GPL-3.0-only

//! Locale parity checks.
//!
//! Fluent falls back silently: a key present in `en` and missing elsewhere
//! renders as English at runtime rather than failing the build, and a mangled
//! `{ $placeholder }` only misbehaves in that one language. Neither is visible
//! without a check like this one.
//!
//! These tests are written so they cannot pass vacuously — if the locale
//! directory or the fallback file goes missing, they fail loudly rather than
//! finding nothing to compare.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The fallback language, as declared in `i18n.toml`.
const FALLBACK: &str = "en";

/// Translation domain: the crate name with dashes replaced, which is what
/// `i18n-embed-fl` looks for.
const DOMAIN: &str = "stellarshot";

fn i18n_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("i18n")
}

/// Every locale directory found under `i18n/`.
fn locales() -> Vec<String> {
    let mut locales: Vec<String> = std::fs::read_dir(i18n_dir())
        .expect("i18n/ must exist")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    locales.sort();
    locales
}

fn locale_file(locale: &str) -> PathBuf {
    i18n_dir().join(locale).join(format!("{DOMAIN}.ftl"))
}

/// Messages in one `.ftl` file, as `key -> set of argument placeholders`.
///
/// This is a deliberately small parser rather than a Fluent dependency: it
/// only has to find message identifiers at the start of a line and `$name`
/// references anywhere in the file, which is enough to catch the two failure
/// modes that matter.
fn parse(path: &Path) -> BTreeMap<String, BTreeSet<String>> {
    let source = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));

    let mut messages: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut current: Option<String> = None;

    for line in source.lines() {
        let trimmed = line.trim_start();

        // Comments and blank lines carry nothing.
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }

        // A message identifier starts in column zero: `key = value`.
        let is_new_message = !line.starts_with(char::is_whitespace)
            && line
                .split_once('=')
                .is_some_and(|(key, _)| is_identifier(key.trim()));

        if is_new_message {
            let (key, _) = line.split_once('=').expect("checked above");
            let key = key.trim().to_string();
            assert!(
                !messages.contains_key(&key),
                "duplicate key `{key}` in {}",
                path.display()
            );
            messages.insert(key.clone(), BTreeSet::new());
            current = Some(key);
        }

        // Placeholders can appear on the identifier line or on any
        // continuation line of a multi-line value.
        if let Some(key) = current.as_ref() {
            let placeholders = extract_placeholders(line);
            messages
                .get_mut(key)
                .expect("current key was inserted")
                .extend(placeholders);
        }
    }

    messages
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && text.starts_with(|c: char| c.is_ascii_alphabetic())
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Every `$name` reference in a line.
fn extract_placeholders(line: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let bytes: Vec<char> = line.chars().collect();

    for (index, c) in bytes.iter().enumerate() {
        if *c != '$' {
            continue;
        }
        let name: String = bytes[index + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == '_' || **c == '-')
            .collect();
        if !name.is_empty() {
            found.insert(name);
        }
    }

    found
}

#[test]
fn the_fallback_locale_exists_and_is_not_empty() {
    // Guards every other test in this file: without this, a missing i18n
    // directory would make them all pass by comparing nothing.
    let path = locale_file(FALLBACK);
    assert!(
        path.is_file(),
        "the fallback locale {} is missing",
        path.display()
    );

    let messages = parse(&path);
    assert!(
        messages.len() > 20,
        "the fallback locale has only {} messages, which suggests it was truncated",
        messages.len()
    );
}

#[test]
fn the_fallback_locale_is_among_the_locales_found() {
    let locales = locales();
    assert!(
        locales.contains(&FALLBACK.to_string()),
        "locales found: {locales:?}"
    );
}

#[test]
fn every_locale_has_exactly_the_fallback_key_set() {
    let fallback = parse(&locale_file(FALLBACK));
    let fallback_keys: BTreeSet<&String> = fallback.keys().collect();

    for locale in locales() {
        if locale == FALLBACK {
            continue;
        }

        let path = locale_file(&locale);
        assert!(
            path.is_file(),
            "locale directory `{locale}` has no {DOMAIN}.ftl"
        );

        let translated = parse(&path);
        let keys: BTreeSet<&String> = translated.keys().collect();

        let missing: Vec<&&String> = fallback_keys.difference(&keys).collect();
        let orphaned: Vec<&&String> = keys.difference(&fallback_keys).collect();

        assert!(
            missing.is_empty(),
            "`{locale}` is missing keys (they will silently show as English): {missing:?}"
        );
        assert!(
            orphaned.is_empty(),
            "`{locale}` has keys that no longer exist in `{FALLBACK}`: {orphaned:?}"
        );
    }
}

#[test]
fn every_locale_keeps_the_same_placeholders() {
    let fallback = parse(&locale_file(FALLBACK));

    for locale in locales() {
        if locale == FALLBACK {
            continue;
        }

        let translated = parse(&locale_file(&locale));

        for (key, expected) in &fallback {
            let Some(actual) = translated.get(key) else {
                // Reported by the key-set test; nothing to add here.
                continue;
            };
            assert_eq!(
                actual, expected,
                "`{key}` in `{locale}` has different placeholders; \
                 a broken one only misbehaves in that language, at runtime"
            );
        }
    }
}

#[test]
fn no_locale_repeats_a_key() {
    // `parse` asserts on duplicates; this makes the intent explicit and covers
    // every locale rather than only the ones the other tests happen to read.
    for locale in locales() {
        let _ = parse(&locale_file(&locale));
    }
}
