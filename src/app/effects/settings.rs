// SPDX-License-Identifier: GPL-3.0-only

//! The Settings page's effects: the file choosers, writing and reading an
//! export, and merging an import into the backups.

use super::super::*;
use crate::app::pages::settings;

impl App {
    pub(in crate::app) fn run_settings_effects(
        &mut self,
        effects: Vec<settings::Effect>,
    ) -> Task<Message> {
        let to_page = |message: settings::Message| app(Message::Settings(message));
        let mut tasks = Vec::new();
        for effect in effects {
            let task = match effect {
                settings::Effect::ThemeChanged => self.update_theme(),
                settings::Effect::ChooseExportPath => Task::perform(
                    tasks::choose_save_path(
                        fl!("settings-export-title"),
                        "stellarshot-settings.ron".to_owned(),
                    ),
                    move |path| to_page(settings::Message::ExportChosen(path)),
                ),
                settings::Effect::WriteExport(path, profiles) => Task::perform(
                    async move {
                        let text = tasks::export_settings(profiles).await?;
                        tasks::write_file(path, text).await
                    },
                    move |result| to_page(settings::Message::ExportSaved(result)),
                ),
                settings::Effect::ChooseImportPath => Task::perform(
                    tasks::choose_import_path(fl!("settings-import-title")),
                    move |path| to_page(settings::Message::ImportChosen(path)),
                ),
                settings::Effect::ReadImport(path) => {
                    Task::perform(tasks::read_export(path), move |result| {
                        to_page(settings::Message::ImportRead(result))
                    })
                }
                settings::Effect::Import(export) => self.import_settings(&export),
                settings::Effect::PickCacheDir => Task::perform(
                    tasks::pick_folder(fl!("settings-cache-dir-title")),
                    move |path| to_page(settings::Message::CacheDirChosen(path)),
                ),
                settings::Effect::Info(title, body) => {
                    self.dialogs.notify(Dialog::Info(title, body));
                    Task::none()
                }
                settings::Effect::Error(text) => {
                    self.dialogs.notify(Dialog::Error(text));
                    Task::none()
                }
            };
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// Add the backups in `export` that are new, install their schedules,
    /// store their history and say what happened.
    fn import_settings(&mut self, export: &settings_export::Export) -> Task<Message> {
        let merged = settings_export::merge(&self.config.profiles, export);
        let history = merged.history;
        if !self.save_profiles(merged.profiles) {
            // `save_profiles` already showed why; claiming success
            // and installing schedules for profiles that were
            // never actually written would only compound it.
            return Task::none();
        }
        let mut body = fl!(
            "settings-import-done-body",
            added = (merged.counts.added as i64),
            skipped = (merged.counts.skipped as i64),
            rejected = (merged.counts.rejected as i64)
        );
        if merged.needs_review {
            body = format!("{body}\n\n{}", fl!("settings-import-hooks-disabled"));
        }
        self.dialogs
            .notify(Dialog::Info(fl!("settings-import-done-title"), body));
        // `save_profiles` already updated `self.config`, so the change
        // notification that follows sees nothing new: show the new backups
        // in the sidebar now.
        self.rebuild_nav(None);
        let activate = self.activate_selected();
        // Each newly added backup's own schedule, exactly as an
        // existing one gets it when the wizard creates or edits it.
        let mut tasks: Vec<Task<Message>> =
            merged.added.into_iter().map(Self::apply_schedule).collect();
        tasks.push(activate);
        // Disk I/O for every backup's history: off the UI thread.
        tasks.push(Task::perform(
            tasks::blocking(move || {
                settings_export::store_history(&history);
                Ok(())
            }),
            |_| app(Message::Noop),
        ));
        Task::batch(tasks)
    }
}
