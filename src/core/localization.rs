// SPDX-License-Identifier: GPL-3.0-only

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader
        .load_fallback_language(&Localizations)
        .expect("i18n/en must be present and valid; it is embedded at build time");
    loader
});

/// Switch to the desktop's languages.
///
/// Without this only the English fallback is ever loaded, whatever the
/// desktop's language, which is how upstream shipped: every translation was
/// present and none was used.
pub fn init() {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    let requested = i18n_embed::DesktopLanguageRequester::requested_languages();
    if let Err(err) = localizer.select(&requested) {
        crate::error_log!(crate::debug::CONFIG, "could not select a language: {err}");
    }
}

#[macro_export]
macro_rules! fl {
    ($message_id:literal) => {{
        i18n_embed_fl::fl!($crate::core::localization::LANGUAGE_LOADER, $message_id)
    }};

    ($message_id:literal, $($args:expr),*) => {{
        i18n_embed_fl::fl!($crate::core::localization::LANGUAGE_LOADER, $message_id, $($args), *)
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_requested_language_is_actually_used() {
        // Select German explicitly and read a string that differs from
        // English: proves the loader switches, which upstream never did.
        let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
        localizer
            .select(&["de".parse().unwrap()])
            .expect("German is embedded");
        assert_eq!(LANGUAGE_LOADER.get("delete"), "Löschen");
        localizer.select(&["en".parse().unwrap()]).unwrap();
        assert_eq!(LANGUAGE_LOADER.get("delete"), "Delete");
    }
}
