// SPDX-License-Identifier: GPL-3.0-only

//! The Settings page: theme, exporting and importing every backup's settings,
//! where rustic keeps its cache, and exclusions that apply to every backup.
//!
//! Follows the other pages' pattern: [`SettingsPage::update`] changes what is
//! its own and the saved settings, and returns [`Effect`]s for what needs the
//! rest of the window (a dialog, a file chooser, the profile list).

use std::path::PathBuf;

use cosmic::iced::{Alignment, Length};
use cosmic::{Element, cosmic_config, theme, widget};

use crate::app::config::{AppTheme, StellarshotConfig};
use crate::app::{errors, format};
use crate::debug::CONFIG;
use crate::engine::{self, EngineError};
use crate::profile::Profile;
use crate::{error_log, fl, settings_export};

#[derive(Debug, Clone)]
pub enum Message {
    AppTheme(usize),
    Export,
    /// Chose where to save, or canceled.
    ExportChosen(Option<PathBuf>),
    ExportSaved(Result<(), String>),
    Import,
    /// Chose a file to import, or canceled.
    ImportChosen(Option<PathBuf>),
    ImportRead(Result<settings_export::Export, String>),
    PatternInput(String),
    AddPattern,
    RemovePattern(usize),
    ChooseCacheDir,
    CacheDirChosen(Option<PathBuf>),
    ClearCacheDir,
    NoCache(bool),
}

/// What the page needs the window to do.
#[derive(Debug)]
pub enum Effect {
    /// The theme setting changed: apply it.
    ThemeChanged,
    ChooseExportPath,
    WriteExport(PathBuf, Vec<Profile>),
    ChooseImportPath,
    ReadImport(PathBuf),
    /// Add the backups in this file that are new (see
    /// [`settings_export::merge`]).
    Import(settings_export::Export),
    PickCacheDir,
    Info(String, String),
    Error(String),
}

#[derive(Debug)]
pub struct SettingsPage {
    /// The theme names for the dropdown, in [`AppTheme::ALL`] order.
    themes: Vec<String>,
    /// The pattern field, for a global exclusion not yet added.
    pattern_input: String,
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self {
            themes: vec![fl!("match-desktop"), fl!("dark"), fl!("light")],
            pattern_input: String::new(),
        }
    }
}

/// Save one setting, reporting (not failing) if it cannot be written: the new
/// value still applies for this run.
/// Nothing if `result` saved, else the error to show: a setting that
/// silently reverts at the next launch looks like it worked.
fn saved<T, E: std::fmt::Display>(what: &str, result: Result<T, E>) -> Vec<Effect> {
    match result {
        Ok(_) => Vec::new(),
        Err(err) => {
            error_log!(CONFIG, "failed to save {what}: {err}");
            vec![Effect::Error(errors::describe(
                &fl!("error-settings-not-saved"),
                &EngineError::new(engine::ErrorKind::Io, err.to_string()),
            ))]
        }
    }
}

impl SettingsPage {
    pub fn update(
        &mut self,
        message: Message,
        config: &mut StellarshotConfig,
        handler: Option<&cosmic_config::Config>,
    ) -> Vec<Effect> {
        match message {
            Message::AppTheme(index) => {
                let mut effects = vec![Effect::ThemeChanged];
                if let Some(handler) = handler {
                    effects.extend(saved(
                        "the theme",
                        config.set_app_theme(handler, AppTheme::from_index(index)),
                    ));
                }
                effects
            }
            Message::Export => vec![Effect::ChooseExportPath],
            Message::ExportChosen(Some(path)) => {
                vec![Effect::WriteExport(path, config.profiles.clone())]
            }
            Message::ExportChosen(None) => Vec::new(),
            Message::ExportSaved(Ok(())) => vec![Effect::Info(
                fl!("settings-export-done-title"),
                fl!("settings-export-done-body"),
            )],
            Message::ExportSaved(Err(detail)) => vec![Effect::Error(errors::describe(
                &fl!("settings-export-failed"),
                &EngineError::new(engine::ErrorKind::Io, detail),
            ))],
            Message::Import => vec![Effect::ChooseImportPath],
            Message::ImportChosen(Some(path)) => vec![Effect::ReadImport(path)],
            Message::ImportChosen(None) => Vec::new(),
            Message::ImportRead(Ok(export)) => vec![Effect::Import(export)],
            Message::ImportRead(Err(detail)) => vec![Effect::Error(errors::describe(
                &fl!("settings-import-failed"),
                &EngineError::new(engine::ErrorKind::Io, detail),
            ))],
            Message::PatternInput(text) => {
                self.pattern_input = text;
                Vec::new()
            }
            Message::AddPattern => {
                let mut effects = Vec::new();
                let pattern = self.pattern_input.trim().to_owned();
                if !pattern.is_empty() && !config.global_exclude_patterns.contains(&pattern) {
                    let mut patterns = config.global_exclude_patterns.clone();
                    patterns.push(pattern);
                    if let Some(handler) = handler {
                        effects = saved(
                            "the global exclusions",
                            config.set_global_exclude_patterns(handler, patterns),
                        );
                    }
                }
                self.pattern_input.clear();
                effects
            }
            Message::RemovePattern(index) => {
                let mut patterns = config.global_exclude_patterns.clone();
                if index < patterns.len()
                    && let Some(handler) = handler
                {
                    patterns.remove(index);
                    return saved(
                        "the global exclusions",
                        config.set_global_exclude_patterns(handler, patterns),
                    );
                }
                Vec::new()
            }
            Message::ChooseCacheDir => vec![Effect::PickCacheDir],
            Message::CacheDirChosen(Some(path)) => {
                engine::cache_settings::set(Some(path.clone()), config.no_cache);
                match handler {
                    Some(handler) => saved(
                        "the cache location",
                        config.set_cache_dir(handler, Some(path)),
                    ),
                    None => Vec::new(),
                }
            }
            Message::CacheDirChosen(None) => Vec::new(),
            Message::ClearCacheDir => {
                engine::cache_settings::set(None, config.no_cache);
                match handler {
                    Some(handler) => {
                        saved("the cache location", config.set_cache_dir(handler, None))
                    }
                    None => Vec::new(),
                }
            }
            Message::NoCache(no_cache) => {
                engine::cache_settings::set(config.cache_dir.clone(), no_cache);
                match handler {
                    Some(handler) => {
                        saved("the cache setting", config.set_no_cache(handler, no_cache))
                    }
                    None => Vec::new(),
                }
            }
        }
    }

    pub fn view<'a>(&'a self, config: &'a StellarshotConfig) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let cache_dir_label = config
            .cache_dir
            .as_ref()
            .map(|path| format::path(path))
            .unwrap_or_else(|| fl!("settings-cache-dir-default"));
        let cache = widget::settings::section()
            .title(fl!("settings-cache-title"))
            .add(
                widget::settings::item::builder(fl!("settings-cache-dir"))
                    .description(cache_dir_label)
                    .control(
                        widget::row::with_capacity(2)
                            .spacing(spacing.space_xs)
                            .push(
                                widget::button::standard(fl!("settings-cache-dir-choose"))
                                    .on_press_maybe(
                                        (!config.no_cache).then_some(Message::ChooseCacheDir),
                                    ),
                            )
                            .push_maybe(config.cache_dir.is_some().then(|| {
                                widget::button::standard(fl!("settings-cache-dir-reset"))
                                    .on_press(Message::ClearCacheDir)
                            })),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-no-cache"))
                    .description(fl!("settings-no-cache-description"))
                    .toggler(config.no_cache, Message::NoCache),
            );

        let mut global_excludes = widget::settings::section()
            .title(fl!("settings-global-excludes-title"))
            .add(widget::text::body(fl!(
                "settings-global-excludes-description"
            )));
        for (index, pattern) in config.global_exclude_patterns.iter().enumerate() {
            global_excludes = global_excludes.add(
                widget::settings::item::builder(pattern.clone()).control(
                    widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                        .tooltip(fl!("remove"))
                        .name(fl!("remove-item", item = pattern.clone()))
                        .on_press(Message::RemovePattern(index)),
                ),
            );
        }
        global_excludes = global_excludes.add(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center)
                .push(
                    widget::text_input(fl!("wizard-pattern-placeholder"), &self.pattern_input)
                        .on_input(Message::PatternInput)
                        .on_submit(|_| Message::AddPattern)
                        .width(Length::Fill),
                )
                .push(widget::button::standard(fl!("add")).on_press(Message::AddPattern)),
        );

        widget::settings::view_column(vec![
            widget::settings::section()
                .title(fl!("appearance"))
                .add(
                    widget::settings::item::builder(fl!("theme")).control(widget::dropdown(
                        &self.themes,
                        Some(config.app_theme.index()),
                        Message::AppTheme,
                    )),
                )
                .into(),
            widget::settings::section()
                .title(fl!("settings-backup-title"))
                .add(
                    widget::settings::item::builder(fl!("settings-export"))
                        .description(fl!("settings-export-description"))
                        .control(
                            widget::button::standard(fl!("settings-export-button"))
                                .on_press(Message::Export),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("settings-import"))
                        .description(fl!("settings-import-description"))
                        .control(
                            widget::button::standard(fl!("settings-import-button"))
                                .on_press(Message::Import),
                        ),
                )
                .into(),
            cache.into(),
            global_excludes.into(),
        ])
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::APP_ID;
    use crate::app::config::CONFIG_VERSION;

    fn handler(dir: &std::path::Path) -> cosmic_config::Config {
        cosmic_config::Config::with_custom_path(APP_ID, CONFIG_VERSION, dir.to_owned()).unwrap()
    }

    #[test]
    fn a_pattern_is_added_once_and_the_field_cleared() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(dir.path());
        let mut config = StellarshotConfig::default();
        let mut page = SettingsPage::default();

        for _ in 0..2 {
            page.update(
                Message::PatternInput("*.tmp".into()),
                &mut config,
                Some(&handler),
            );
            page.update(Message::AddPattern, &mut config, Some(&handler));
        }

        assert_eq!(config.global_exclude_patterns, vec!["*.tmp".to_owned()]);
        assert!(page.pattern_input.is_empty());

        page.update(Message::RemovePattern(5), &mut config, Some(&handler));
        assert_eq!(
            config.global_exclude_patterns.len(),
            1,
            "out of range is ignored"
        );
        page.update(Message::RemovePattern(0), &mut config, Some(&handler));
        assert!(config.global_exclude_patterns.is_empty());
    }

    #[test]
    fn changing_the_theme_saves_it_and_asks_for_it_to_be_applied() {
        let dir = tempfile::tempdir().unwrap();
        let handler = handler(dir.path());
        let mut config = StellarshotConfig::default();
        let mut page = SettingsPage::default();

        let effects = page.update(Message::AppTheme(1), &mut config, Some(&handler));

        assert!(matches!(effects.as_slice(), [Effect::ThemeChanged]));
        assert_eq!(config.app_theme, AppTheme::Dark);
    }

    #[test]
    fn exporting_and_importing_hand_the_work_to_the_window() {
        let mut config = StellarshotConfig::default();
        let mut page = SettingsPage::default();

        assert!(matches!(
            page.update(Message::Export, &mut config, None).as_slice(),
            [Effect::ChooseExportPath]
        ));
        assert!(matches!(
            page.update(Message::ExportChosen(None), &mut config, None)
                .as_slice(),
            []
        ));
        assert!(matches!(
            page.update(
                Message::ExportChosen(Some("/tmp/x.ron".into())),
                &mut config,
                None
            )
            .as_slice(),
            [Effect::WriteExport(..)]
        ));
        assert!(matches!(
            page.update(Message::ImportRead(Err("bad".into())), &mut config, None)
                .as_slice(),
            [Effect::Error(_)]
        ));
    }
}
