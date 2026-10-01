// SPDX-License-Identifier: GPL-3.0-only

//! What the window does with each message: the body of
//! `Application::update`.

use cosmic::widget::menu::action::MenuAction;

use crate::app::{
    Action, App, Dialog, DialogMessage, EngineError, Key, Message, NavItem, ProfileState, Task,
    Wizard, app, backup_unreadable_profiles, engine, errors, format, home_dir, place,
    spawn_new_window, tasks, unmount_off_thread, window,
};
use crate::debug::{CONFIG, UI};
use crate::{debug_log, error_log, fl};

impl App {
    pub(in crate::app) fn on_message(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Profile(id, message) => {
                let Some(profile) = self.config.profile(&id).cloned() else {
                    return Task::none();
                };
                let effects = self
                    .pages
                    .entry(id.clone())
                    .or_default()
                    .update(message, &profile);
                // The sidebar shows whether this backup is running, and its
                // progress while it is: every change to it is worth a
                // refresh, not just the ones that finish a run — but not a
                // full `rebuild_nav`, which this page's own messages arrive
                // far too often for (every keystroke, every progress tick).
                // Anything that actually changes the profile list or the
                // wizard's presence goes through its own `rebuild_nav` call
                // elsewhere, once the effect it returns is handled below.
                self.refresh_nav_row(&id);
                return self.run_profile_effects(&id, effects);
            }
            Message::Wizard(session, _) | Message::WizardFinished(session, _)
                if session != self.wizard_session =>
            {
                debug_log!(
                    UI,
                    "ignored a result for wizard {session}; this is {}",
                    self.wizard_session
                );
            }
            Message::Wizard(_, message) => {
                // Sign-in finished, one way or the other: the "switch to
                // your browser" modal has done its job either way, and the
                // wizard page itself already shows the outcome.
                if matches!(
                    message,
                    crate::app::wizard::Message::Place(place::Message::SignedIn(_))
                ) {
                    self.dialogs
                        .close_where(|dialog| matches!(dialog, Dialog::SigningIn));
                    self.sign_in_cancel = None;
                }
                let Some(wizard) = self.wizard.as_mut() else {
                    return Task::none();
                };
                let effects = wizard.update(message);
                return self.run_wizard_effects(effects);
            }
            Message::WizardFinished(_, result) => return self.on_wizard_finished(*result),
            Message::RestorePage(session, message) if session != self.restore_session => {
                debug_log!(UI, "dropped a result for a restore page that is gone");
                return unmount_off_thread(message);
            }
            Message::RestorePage(_, message) => {
                let Some((page, _)) = self.restore.as_mut() else {
                    return unmount_off_thread(message);
                };
                let effects = page.update(message);
                return self.run_restore_effects(effects);
            }
            Message::Dialog(message) => return self.on_dialog(message),
            Message::NewBackup => {
                if self.wizard.is_none() {
                    let (wizard, effects) = Wizard::create(home_dir().as_deref());
                    return self.start_wizard(wizard, effects);
                }
                // Only one wizard at a time: an in-progress one is resumed,
                // never silently replaced.
                self.select_wizard();
            }
            Message::ImportDejaDup => {
                if self.wizard.is_none() {
                    return Task::perform(
                        tasks::blocking(|| Ok(crate::dejadup::find())),
                        |found| app(Message::DejaDupFound(found.ok().flatten())),
                    );
                }
                self.select_wizard();
            }
            Message::DejaDupDetected(found) => self.dejadup = found,
            Message::DejaDupFound(found) => {
                use crate::dejadup::Place;
                match found {
                    None => self.dialogs.notify(Dialog::Error(fl!("dejadup-none"))),
                    Some(import) if import.other_format => {
                        self.dialogs
                            .notify(Dialog::Error(fl!("dejadup-other-format")));
                    }
                    Some(crate::dejadup::Import {
                        place: Place::Unsupported(backend),
                        ..
                    }) => {
                        self.dialogs
                            .notify(Dialog::Error(fl!("dejadup-unsupported", backend = backend)));
                    }
                    Some(import) if self.wizard.is_none() => {
                        let (wizard, effects) = Wizard::import(&import);
                        return self.start_wizard(wizard, effects);
                    }
                    // A wizard appeared while Déjà Dup's settings were being
                    // read (another way to open one was used meanwhile):
                    // resume it rather than replacing it with this import.
                    Some(_) => self.select_wizard(),
                }
            }
            Message::OpenExisting => {
                if self.wizard.is_none() {
                    let (wizard, effects) = Wizard::open();
                    return self.start_wizard(wizard, effects);
                }
                self.select_wizard();
            }
            Message::BackUpSelected => {
                if let Some(id) = self.selected().map(str::to_owned)
                    && let Some(profile) = self.config.profile(&id).cloned()
                {
                    let effects = self.pages.entry(id.clone()).or_default().back_up(&profile);
                    return self.run_profile_effects(&id, effects);
                }
            }
            Message::Tick => {
                self.now = format::now();
                return self.load_runs();
            }
            // Only redraws, so running times count up.
            Message::WaitingTick => {}
            Message::ScheduleFailed(detail) => {
                self.dialogs.notify(Dialog::Error(errors::describe(
                    &fl!("schedule-failed"),
                    &EngineError::new(engine::ErrorKind::Internal, detail),
                )));
            }
            Message::SelectProfile(id) => {
                let entity = self
                    .nav
                    .iter()
                    .find(|&entity| matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Profile(p)) if *p == id));
                if let Some(entity) = entity {
                    self.nav.activate(entity);
                    return self.activate_selected();
                }
            }
            Message::HistoryLoaded(entries) => self.history = Some(entries),
            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }
            Message::CloseContextDrawer => self.core.window.show_context = false,
            Message::WindowClose => {
                // Cleared here, not left for a later close event, so
                // `dbus_activation` sees "no window" right away rather than
                // trying to focus one that is on its way out.
                if let Some(id) = self.core.set_main_window_id(None) {
                    return window::close(id);
                }
            }
            Message::WindowNew => {
                return Task::perform(spawn_new_window(), |()| app(Message::Noop));
            }
            Message::Quit => {
                // Anything that would be cut off: a backup, a restore, a
                // repository being created or opened by the wizard, or a
                // password change or deletion running from a dialog. Only
                // the first used to count.
                // Already asking: a second Ctrl+Q must not stack another.
                if self.dialogs.any(|dialog| matches!(dialog, Dialog::Quit)) {
                    return Task::none();
                }
                // Any busy dialog, not only the one in front: it may be
                // waiting behind another the user opened meanwhile.
                let dialog_busy = self.dialogs.any(Dialog::is_busy);
                if self.pages.values().any(ProfileState::is_busy)
                    || self
                        .restore
                        .as_ref()
                        .is_some_and(|(page, _)| page.is_restoring())
                    || self.wizard.as_ref().is_some_and(Wizard::busy)
                    || dialog_busy
                {
                    self.dialogs.open(Dialog::Quit);
                } else {
                    return cosmic::iced::exit();
                }
            }
            Message::LaunchUrl(url) => {
                if let Err(err) = open::that_detached(&url) {
                    error_log!(UI, "failed to open {url:?}: {err}");
                }
            }
            Message::CopyToClipboard(text) => return cosmic::iced::clipboard::write(text),
            Message::Key(modifiers, key) => {
                // A modal dialog captures the mouse but not the keyboard:
                // while one is open, Escape dismisses it (unless an
                // operation is running from it) and only Quit still works,
                // so a shortcut cannot start a backup or open the wizard
                // behind a "Remove this backup?".
                if let Some(front) = self.dialogs.front() {
                    if key == Key::Named(cosmic::iced::keyboard::key::Named::Escape)
                        && !front.is_busy()
                    {
                        return self.on_message(Message::Dialog(DialogMessage::Close));
                    }
                    let quit = self.key_binds.iter().any(|(key_bind, action)| {
                        *action == Action::Quit && key_bind.matches(modifiers, &key, None)
                    });
                    if quit {
                        return self.on_message(Message::Quit);
                    }
                    return Task::none();
                }
                for (key_bind, action) in &self.key_binds {
                    if key_bind.matches(modifiers, &key, None) {
                        return self.on_message(action.message());
                    }
                }
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Settings(message) => {
                let effects =
                    self.settings
                        .update(message, &mut self.config, self.config_handler.as_ref());
                return self.run_settings_effects(effects);
            }
            Message::ConfigChanged(mut changed) => {
                // A live reload (another window, an import, a hand edit)
                // goes through the same gate as startup: if the profiles
                // key on disk cannot be read *now*, the empty list in
                // `changed` is the derived default standing in for it, not
                // a real removal of every backup. Keep the last list that
                // was actually read, and stop saving, exactly as `init`
                // does — adopting the empty list here would otherwise be
                // written over the file by the next save.
                if let Some(handler) = &self.config_handler
                    && crate::app::config::profiles_unreadable(handler)
                {
                    if !self.profiles_read_only {
                        error_log!(
                            CONFIG,
                            "the profiles list became unreadable on disk; keeping the last one \
                             read and refusing to save until this is resolved"
                        );
                        self.profiles_read_only = true;
                        let path = backup_unreadable_profiles();
                        self.dialogs
                            .notify(Dialog::Error(fl!("error-config-unreadable", path = path)));
                    }
                    changed.profiles = self.config.profiles.clone();
                }
                if changed != self.config {
                    self.config = changed;
                    // State for a backup that no longer exists (removed from
                    // another window, or an import) would otherwise stay in
                    // memory with its password, and a run still marked busy
                    // would keep Quit asking for confirmation forever.
                    self.pages.retain(|id, _| self.config.profile(id).is_some());
                    self.runs.retain(|id, _| self.config.profile(id).is_some());
                    if self
                        .restore
                        .as_ref()
                        .is_some_and(|(page, _)| self.config.profile(&page.profile_id).is_none())
                    {
                        self.restore = None;
                    }
                    self.rebuild_nav(None);
                    return self.update_theme();
                }
            }
            Message::SystemThemeModeChange => return self.update_theme(),
            Message::Noop => {}
            Message::RunsLoaded(runs) => self.apply_runs(runs),
            Message::RestPasswordsMoved(moved) => self.rest_passwords_moved(moved),
        }
        Task::none()
    }
}
