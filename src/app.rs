// SPDX-License-Identifier: GPL-3.0-only

//! The application window: the sidebar of backups, the page for the selected
//! one, the setup wizard, and the dialogs that confirm destructive actions.

use std::any::TypeId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use std::{env, process};

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{Event as KeyEvent, Key, Modifiers};
use cosmic::iced::{Event, Length, Subscription, event, window};
use cosmic::widget::about::About;
use cosmic::widget::menu::{action::MenuAction, key_bind::KeyBind};
use cosmic::widget::{self, nav_bar};
use cosmic::{Application, ApplicationExt, Element, cosmic_config, cosmic_theme};

use crate::app::config::{AppTheme, CONFIG_VERSION, StellarshotConfig};
use crate::app::key_bind::key_binds;
use crate::app::pages::profile::{self, ProfileState};
use crate::app::wizard::{Mode, Wizard, place};
use crate::debug::{CONFIG, ENGINE, UI};
use crate::engine::{self, EngineError};
use crate::profile::Profile;
use crate::runner::{Job, Operation};
use crate::{debug_log, error_log, fl};

pub mod child;
pub mod config;
pub mod errors;
pub mod format;
mod key_bind;
pub mod menu;
pub mod migrate;
pub mod pages;
pub mod portal;
pub mod settings;
pub mod tasks;
pub mod wizard;

/// The application ID: desktop entry, icon, settings and keyring items.
pub const APP_ID: &str = "io.github.stldave314.Stellarshot";

/// How often relative times ("2 hours ago") are refreshed.
const CLOCK_TICK: Duration = Duration::from_secs(30);

pub struct App {
    core: Core,
    nav: nav_bar::Model,
    about: About,
    app_themes: Vec<String>,
    config_handler: Option<cosmic_config::Config>,
    config: StellarshotConfig,
    context_page: ContextPage,
    dialog: Option<Dialog>,
    pages: HashMap<String, ProfileState>,
    wizard: Option<Wizard>,
    key_binds: HashMap<KeyBind, Action>,
    modifiers: Modifiers,
    now: i64,
    /// Déjà Dup settings were found, so the first screen offers an import.
    dejadup: bool,
}

/// What a sidebar entry leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NavItem {
    Profile(String),
    New,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleContextPage(ContextPage),
    CloseContextDrawer,
    LaunchUrl(String),
    AppTheme(usize),
    SystemThemeModeChange,
    /// Settings changed on disk, for example from another window.
    ConfigChanged(StellarshotConfig),
    Key(Modifiers, Key),
    Modifiers(Modifiers),
    WindowClose,
    WindowNew,
    Tick,
    NewBackup,
    OpenExisting,
    ImportDejaDup,
    DejaDupFound(Option<crate::dejadup::Import>),
    BackUpSelected,
    Profile(String, profile::Message),
    Wizard(wizard::Message),
    WizardFinished(Result<tasks::Finished, EngineError>),
    Dialog(DialogMessage),
    Noop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextPage {
    About,
    Settings,
}

impl ContextPage {
    fn title(&self) -> String {
        match self {
            Self::About => fl!("about"),
            Self::Settings => fl!("settings"),
        }
    }
}

/// A modal dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dialog {
    /// A localized explanation of something that failed.
    Error(String),
    /// Forget a profile; its data stays.
    Remove { id: String, name: String },
    /// Delete a profile's repository and everything in it.
    DeleteAll {
        id: String,
        name: String,
        typed: String,
        busy: bool,
    },
}

impl Dialog {
    /// Deleting everything needs the profile's name typed exactly: not
    /// trimmed, not case-folded. A slip of the finger must not qualify.
    pub fn can_confirm(&self) -> bool {
        match self {
            Self::DeleteAll {
                name, typed, busy, ..
            } => !busy && typed == name,
            _ => true,
        }
    }
}

#[derive(Clone, Debug)]
pub enum DialogMessage {
    Close,
    Confirm,
    Typed(String),
    Deleted(String, Result<(), EngineError>),
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: StellarshotConfig,
    /// Open the setup wizard as soon as the window appears.
    pub start_wizard: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    About,
    NewBackup,
    BackUpNow,
    ImportDejaDup,
    Settings,
    WindowClose,
    WindowNew,
}

impl MenuAction for Action {
    type Message = Message;
    fn message(&self) -> Self::Message {
        match self {
            Action::About => Message::ToggleContextPage(ContextPage::About),
            Action::NewBackup => Message::NewBackup,
            Action::BackUpNow => Message::BackUpSelected,
            Action::ImportDejaDup => Message::ImportDejaDup,
            Action::Settings => Message::ToggleContextPage(ContextPage::Settings),
            Action::WindowClose => Message::WindowClose,
            Action::WindowNew => Message::WindowNew,
        }
    }
}

/// Wrap an application message for a task.
fn app(message: Message) -> cosmic::Action<Message> {
    cosmic::Action::App(message)
}

impl App {
    fn update_theme(&mut self) -> Task<Message> {
        cosmic::command::set_theme(self.config.app_theme.theme())
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let selected = match self.config.app_theme {
            AppTheme::Dark => 1,
            AppTheme::Light => 2,
            AppTheme::System => 0,
        };
        widget::settings::view_column(vec![
            widget::settings::section()
                .title(fl!("appearance"))
                .add(
                    widget::settings::item::builder(fl!("theme")).control(widget::dropdown(
                        &self.app_themes,
                        Some(selected),
                        Message::AppTheme,
                    )),
                )
                .into(),
        ])
        .into()
    }

    /// The profile the sidebar has selected.
    fn selected(&self) -> Option<&str> {
        match self.nav.active_data::<NavItem>() {
            Some(NavItem::Profile(id)) => Some(id.as_str()),
            _ => None,
        }
    }

    /// Rebuild the sidebar from the settings, keeping the selection when the
    /// selected profile still exists.
    fn rebuild_nav(&mut self, select: Option<&str>) {
        let keep = select
            .map(str::to_owned)
            .or_else(|| self.selected().map(str::to_owned));
        self.nav.clear();
        let mut chosen = None;
        for profile in &self.config.profiles {
            let id = self
                .nav
                .insert()
                .text(profile.name.clone())
                .icon(widget::icon::from_name("drive-harddisk-symbolic"))
                .data(NavItem::Profile(profile.id.clone()))
                .id();
            if keep.as_deref() == Some(profile.id.as_str()) || chosen.is_none() {
                chosen = Some(id);
            }
        }
        if !self.config.profiles.is_empty() {
            self.nav
                .insert()
                .text(fl!("new-backup"))
                .icon(widget::icon::from_name("list-add-symbolic"))
                .data(NavItem::New)
                .divider_above(true);
        }
        if let Some(id) = chosen {
            self.nav.activate(id);
        }
    }

    fn save_profiles(&mut self, profiles: Vec<Profile>) {
        match &self.config_handler {
            Some(handler) => {
                if let Err(err) = self.config.set_profiles(handler, profiles) {
                    error_log!(CONFIG, "failed to save profiles: {err}");
                }
            }
            None => {
                self.config.profiles = profiles;
                error_log!(CONFIG, "failed to save profiles: no config handler");
            }
        }
    }

    /// Replace a profile by ID, or add it.
    fn upsert_profile(&mut self, profile: Profile) {
        let mut profiles = self.config.profiles.clone();
        match profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(existing) => *existing = profile,
            None => profiles.push(profile),
        }
        self.save_profiles(profiles);
    }

    fn remove_profile(&mut self, id: &str) -> Task<Message> {
        let profiles = self
            .config
            .profiles
            .iter()
            .filter(|p| p.id != id)
            .cloned()
            .collect();
        self.save_profiles(profiles);
        self.pages.remove(id);
        self.rebuild_nav(None);
        let id = id.to_owned();
        let forget = Task::perform(async move { crate::keyring::forget(&id).await }, |_| {
            app(Message::Noop)
        });
        Task::batch([forget, self.activate_selected()])
    }

    /// Show the selected profile, looking for its password if needed.
    fn activate_selected(&mut self) -> Task<Message> {
        let Some(id) = self.selected().map(str::to_owned) else {
            return Task::none();
        };
        let effects = self.pages.entry(id.clone()).or_default().activate();
        self.run_profile_effects(&id, effects)
    }

    fn show_error(&mut self, context: &str, error: &EngineError) {
        self.dialog = Some(Dialog::Error(errors::describe(context, error)));
    }

    fn start_wizard(&mut self, wizard: Wizard, effects: Vec<wizard::Effect>) -> Task<Message> {
        self.wizard = Some(wizard);
        self.run_wizard_effects(effects)
    }

    fn run_wizard_effects(&mut self, effects: Vec<wizard::Effect>) -> Task<Message> {
        let mut tasks = Vec::new();
        for effect in effects {
            tasks.push(match effect {
                wizard::Effect::PickFolders { excludes } => {
                    let title = if excludes {
                        fl!("wizard-pick-excludes")
                    } else {
                        fl!("wizard-pick-sources")
                    };
                    Task::perform(tasks::pick_folders(title), move |paths| {
                        app(Message::Wizard(if excludes {
                            wizard::Message::ExcludesChosen(paths)
                        } else {
                            wizard::Message::SourcesChosen(paths)
                        }))
                    })
                }
                wizard::Effect::Place(effect) => self.run_place_effect(effect),
                wizard::Effect::Estimate {
                    generation,
                    request,
                    exclude_folders,
                    cancel,
                } => Task::run(
                    tasks::estimate(request, exclude_folders, cancel),
                    move |event| {
                        app(Message::Wizard(wizard::Message::Estimate(
                            generation, event,
                        )))
                    },
                ),
                wizard::Effect::Finish(finish) => Task::perform(
                    tasks::finish(finish.mode, finish.profile, finish.secret, finish.remember),
                    |result| app(Message::WizardFinished(result)),
                ),
                wizard::Effect::Close => {
                    self.wizard = None;
                    Task::none()
                }
            });
        }
        Task::batch(tasks)
    }

    fn run_place_effect(&mut self, effect: place::Effect) -> Task<Message> {
        let to_wizard =
            |message: place::Message| app(Message::Wizard(wizard::Message::Place(message)));
        match effect {
            place::Effect::PickFolder => {
                Task::perform(tasks::pick_folder(fl!("select-repo-folder")), move |path| {
                    path.map_or(app(Message::Noop), |path| {
                        to_wizard(place::Message::FolderChosen(path))
                    })
                })
            }
            place::Effect::ListDrives => Task::perform(
                tasks::blocking(|| Ok(crate::drives::mounted_drives())),
                move |drives| to_wizard(place::Message::DrivesListed(drives.unwrap_or_default())),
            ),
            place::Effect::CheckRclone => Task::perform(
                tasks::blocking(|| Ok(engine::rclone::available())),
                move |available| {
                    to_wizard(place::Message::RcloneChecked(available.unwrap_or(false)))
                },
            ),
            place::Effect::ListRemotes => Task::perform(
                tasks::blocking(engine::rclone::user_remotes),
                move |remotes| to_wizard(place::Message::RemotesListed(remotes)),
            ),
            place::Effect::SignIn { name } => Task::perform(
                tasks::blocking(move || {
                    let config = engine::rclone::config_path();
                    engine::rclone::sign_in(&config, &name, "drive", &["scope=drive"])
                        .map(|()| name)
                }),
                move |result| to_wizard(place::Message::SignedIn(result)),
            ),
            place::Effect::CopyRemote { index, from, to } => Task::perform(
                tasks::blocking(move || {
                    let config = engine::rclone::config_path();
                    engine::rclone::copy_user_remote(&config, &from, &to).map(|()| to)
                }),
                move |result| to_wizard(place::Message::RemoteCopied(index, result)),
            ),
            place::Effect::Probe(destination) => {
                let probed = destination.clone();
                Task::perform(tasks::probe(destination), move |result| {
                    to_wizard(place::Message::Probed(probed.clone(), result))
                })
            }
        }
    }

    fn run_profile_effects(&mut self, id: &str, effects: Vec<profile::Effect>) -> Task<Message> {
        let Some(profile) = self.config.profile(id).cloned() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        for effect in effects {
            let id = id.to_owned();
            let task = match effect {
                profile::Effect::LoadKeyring => {
                    let key = id.clone();
                    Task::perform(
                        async move { crate::keyring::load(&key).await },
                        move |secret| {
                            app(Message::Profile(
                                id.clone(),
                                profile::Message::KeyringLoaded(secret),
                            ))
                        },
                    )
                }
                profile::Effect::Open { secret, remember } => {
                    let used = secret.clone();
                    Task::perform(
                        tasks::open(profile.clone(), secret, remember),
                        move |result| {
                            app(Message::Profile(
                                id.clone(),
                                profile::Message::Opened(used.clone(), result),
                            ))
                        },
                    )
                }
                profile::Effect::Fetch(secret) => {
                    Task::perform(tasks::snapshots(profile.clone(), secret), move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::SnapshotsLoaded(result),
                        ))
                    })
                }
                profile::Effect::BackUp(secret) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        // An unplugged drive: end the backup before it starts,
                        // through the same path a failed backup takes.
                        Err(err) => {
                            let ended = profile::Message::Backup(child::ChildEvent::Ended(err));
                            let effects = self
                                .pages
                                .entry(id.clone())
                                .or_default()
                                .update(ended, &profile);
                            tasks.push(self.run_profile_effects(&id, effects));
                            continue;
                        }
                    };
                    let job = Job {
                        repository,
                        password: secret,
                        request: Some(profile.backup_request()),
                        snapshot: None,
                        destination: None,
                        ids: Vec::new(),
                    };
                    Task::run(child::run(Operation::Backup, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Backup(event),
                        ))
                    })
                }
                profile::Effect::DeleteSnapshots(secret, ids) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            self.show_error(&fl!("delete-snapshot-failed"), &err);
                            continue;
                        }
                    };
                    let job = Job {
                        repository,
                        password: secret,
                        request: None,
                        snapshot: None,
                        destination: None,
                        ids,
                    };
                    Task::run(child::run(Operation::DeleteSnapshots, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::SnapshotsDeleted(event),
                        ))
                    })
                }
                profile::Effect::ShowError(context, error) => {
                    self.show_error(&context, &error);
                    Task::none()
                }
                profile::Effect::RecordSuccess(time) => {
                    let mut updated = profile.clone();
                    updated.last_success = Some(time);
                    self.upsert_profile(updated);
                    Task::none()
                }
                profile::Effect::Edit => {
                    let (wizard, effects) = Wizard::edit(&profile);
                    self.start_wizard(wizard, effects)
                }
                profile::Effect::Remove => {
                    self.dialog = Some(Dialog::Remove {
                        id,
                        name: profile.name.clone(),
                    });
                    Task::none()
                }
                profile::Effect::DeleteAll => {
                    self.dialog = Some(Dialog::DeleteAll {
                        id,
                        name: profile.name.clone(),
                        typed: String::new(),
                        busy: false,
                    });
                    Task::none()
                }
            };
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    fn on_wizard_finished(
        &mut self,
        result: Result<tasks::Finished, EngineError>,
    ) -> Task<Message> {
        let outcome = result.as_ref().map(drop).map_err(Clone::clone);
        let close = match self.wizard.as_mut() {
            Some(wizard) => {
                let effects = wizard.update(wizard::Message::Finished(outcome));
                self.run_wizard_effects(effects)
            }
            None => Task::none(),
        };
        let finished = match result {
            Ok(finished) => finished,
            Err(err) => {
                self.show_error(&fl!("create-repo-failed"), &err);
                return close;
            }
        };

        let mut profile = finished.profile;
        if let Mode::Edit { .. } = finished.mode {
            // Editing changes only what is backed up; the rest stays.
            if let Some(existing) = self.config.profile(&profile.id) {
                profile.last_success = existing.last_success;
                profile.destination = existing.destination.clone();
                profile.name = existing.name.clone();
            }
        }
        let id = profile.id.clone();
        self.upsert_profile(profile.clone());
        self.rebuild_nav(Some(&id));

        let mut effects = Vec::new();
        if let Some(secret) = finished.secret {
            let state = self.pages.entry(id.clone()).or_default();
            state.update(
                profile::Message::Opened(secret, Ok(finished.snapshots)),
                &profile,
            );
            if finished.mode == Mode::Create {
                effects = state.back_up(&profile);
            }
        }
        Task::batch([close, self.run_profile_effects(&id, effects)])
    }

    fn on_dialog(&mut self, message: DialogMessage) -> Task<Message> {
        match message {
            DialogMessage::Close => {
                self.dialog = None;
                Task::none()
            }
            DialogMessage::Typed(text) => {
                if let Some(Dialog::DeleteAll { typed, .. }) = &mut self.dialog {
                    *typed = text;
                }
                Task::none()
            }
            DialogMessage::Confirm => {
                let Some(dialog) = self.dialog.clone() else {
                    return Task::none();
                };
                if !dialog.can_confirm() {
                    return Task::none();
                }
                match dialog {
                    Dialog::Error(_) => {
                        self.dialog = None;
                        Task::none()
                    }
                    Dialog::Remove { id, .. } => {
                        self.dialog = None;
                        debug_log!(CONFIG, "removing profile {id}; its data stays");
                        self.remove_profile(&id)
                    }
                    Dialog::DeleteAll {
                        id, name, typed, ..
                    } => {
                        let Some(profile) = self.config.profile(&id).cloned() else {
                            return Task::none();
                        };
                        self.dialog = Some(Dialog::DeleteAll {
                            id: id.clone(),
                            name,
                            typed,
                            busy: true,
                        });
                        Task::perform(
                            tasks::blocking(move || delete_repository(&profile)),
                            move |result| {
                                app(Message::Dialog(DialogMessage::Deleted(id.clone(), result)))
                            },
                        )
                    }
                }
            }
            DialogMessage::Deleted(id, result) => match result {
                Ok(()) => {
                    self.dialog = None;
                    debug_log!(ENGINE, "deleted the repository of profile {id}");
                    self.remove_profile(&id)
                }
                Err(err) => {
                    self.show_error(&fl!("delete-repo-failed"), &err);
                    Task::none()
                }
            },
        }
    }
}

/// Delete a profile's repository: under its lock, so nothing is writing to
/// it, and only the entries the repository format owns.
fn delete_repository(profile: &Profile) -> Result<(), EngineError> {
    let location = profile.location()?;
    let _lock = engine::lock::acquire(&location)?;
    engine::delete_repository(&location)
}

/// The user's home folder, the default thing to back up.
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu::menu_bar(&self.key_binds)]
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        // No sidebar until there is something to put in it: the empty state
        // and the wizard fill the window instead.
        (!self.config.profiles.is_empty() && self.wizard.is_none()).then_some(&self.nav)
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let flags_start_wizard = flags.start_wizard;
        let about = About::default()
            .name(fl!("stellarshot"))
            .icon(widget::icon::from_name(APP_ID))
            .version(env!("CARGO_PKG_VERSION"))
            .author(fl!("about-author"))
            .comments(fl!("about-credits"))
            .license(env!("CARGO_PKG_LICENSE"))
            .links([
                (fl!("repository"), env!("CARGO_PKG_REPOSITORY")),
                (
                    fl!("support"),
                    concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"),
                ),
            ])
            .developers([
                ("Aaron Honeycutt", "aaronhoneycutt@proton.me"),
                ("Eduardo Flores", "edfloreshz@proton.me"),
            ]);
        let mut app = App {
            core,
            nav: nav_bar::Model::default(),
            about,
            app_themes: vec![fl!("match-desktop"), fl!("dark"), fl!("light")],
            context_page: ContextPage::Settings,
            config_handler: flags.config_handler,
            config: flags.config,
            dialog: None,
            pages: HashMap::new(),
            wizard: None,
            key_binds: key_binds(),
            modifiers: Modifiers::empty(),
            now: format::now(),
            dejadup: crate::dejadup::find().is_some(),
        };
        app.rebuild_nav(None);

        let title = fl!("stellarshot");
        app.set_header_title(title.clone());
        let title_task = match app.core.main_window_id() {
            Some(id) => app.set_window_title(title, id),
            None => Task::none(),
        };
        let activate = app.activate_selected();
        let wizard = if flags_start_wizard {
            app.update(Message::NewBackup)
        } else {
            Task::none()
        };
        debug_log!(UI, "started with {} profiles", app.config.profiles.len());
        (app, Task::batch([title_task, activate, wizard]))
    }

    fn context_drawer(
        &self,
    ) -> Option<cosmic::app::context_drawer::ContextDrawer<'_, Self::Message>> {
        if !self.core.window.show_context {
            return None;
        }
        let title = self.context_page.title();
        Some(match self.context_page {
            ContextPage::About => cosmic::app::context_drawer::about(
                &self.about,
                |url| Message::LaunchUrl(url.to_owned()),
                Message::CloseContextDrawer,
            )
            .title(title),
            ContextPage::Settings => cosmic::app::context_drawer::context_drawer(
                self.settings_view(),
                Message::CloseContextDrawer,
            )
            .title(title),
        })
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        let dialog = self.dialog.as_ref()?;
        let confirm = dialog
            .can_confirm()
            .then_some(Message::Dialog(DialogMessage::Confirm));
        let cancel =
            widget::button::standard(fl!("cancel")).on_press(Message::Dialog(DialogMessage::Close));
        let built = match dialog {
            Dialog::Error(message) => widget::dialog()
                .title(fl!("error-title"))
                .body(message.as_str())
                .primary_action(
                    widget::button::suggested(fl!("ok"))
                        .on_press(Message::Dialog(DialogMessage::Close)),
                ),
            Dialog::Remove { name, .. } => widget::dialog()
                .title(fl!("remove-title", name = name.clone()))
                .body(fl!("remove-body"))
                .primary_action(widget::button::suggested(fl!("remove")).on_press_maybe(confirm))
                .secondary_action(cancel),
            Dialog::DeleteAll { name, typed, .. } => widget::dialog()
                .title(fl!("delete-title", name = name.clone()))
                .body(fl!("delete-body", name = name.clone()))
                .control(
                    widget::text_input(name.as_str(), typed.as_str())
                        .on_input(|text| Message::Dialog(DialogMessage::Typed(text))),
                )
                .primary_action(widget::button::destructive(fl!("delete")).on_press_maybe(confirm))
                .secondary_action(cancel),
        };
        Some(built.into())
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        if let Some(NavItem::New) = self.nav.data::<NavItem>(id) {
            // "New backup" opens the wizard; the selection stays where it was.
            return self.update(Message::NewBackup);
        }
        self.nav.activate(id);
        self.activate_selected()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        if let Some(wizard) = &self.wizard {
            return wizard.view().map(Message::Wizard);
        }
        if self.config.profiles.is_empty() {
            return pages::empty::view(self.dejadup);
        }
        let Some(id) = self.selected() else {
            return widget::space::horizontal().width(Length::Fill).into();
        };
        match (self.config.profile(id), self.pages.get(id)) {
            (Some(profile), Some(state)) => {
                let id = id.to_owned();
                state
                    .view(profile, self.now)
                    .map(move |message| Message::Profile(id.clone(), message))
            }
            _ => widget::space::horizontal().width(Length::Fill).into(),
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        struct ConfigSubscription;
        struct ThemeSubscription;

        Subscription::batch([
            event::listen_with(|event, status, _window| match event {
                Event::Keyboard(KeyEvent::KeyPressed { key, modifiers, .. }) => match status {
                    event::Status::Ignored => Some(Message::Key(modifiers, key)),
                    event::Status::Captured => None,
                },
                Event::Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                    Some(Message::Modifiers(modifiers))
                }
                _ => None,
            }),
            cosmic_config::config_subscription::<_, StellarshotConfig>(
                TypeId::of::<ConfigSubscription>(),
                APP_ID.into(),
                CONFIG_VERSION,
            )
            .map(|update| {
                if !update.errors.is_empty() {
                    debug_log!(
                        CONFIG,
                        "errors loading config {:?}: {:?}",
                        update.keys,
                        update.errors
                    );
                }
                Message::ConfigChanged(update.config)
            }),
            cosmic_config::config_subscription::<_, cosmic_theme::ThemeMode>(
                TypeId::of::<ThemeSubscription>(),
                cosmic_theme::THEME_MODE_ID.into(),
                cosmic_theme::ThemeMode::version(),
            )
            .map(|_| Message::SystemThemeModeChange),
            cosmic::iced::time::every(CLOCK_TICK).map(|_| Message::Tick),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
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
                return self.run_profile_effects(&id, effects);
            }
            Message::Wizard(message) => {
                let Some(wizard) = self.wizard.as_mut() else {
                    return Task::none();
                };
                let effects = wizard.update(message);
                return self.run_wizard_effects(effects);
            }
            Message::WizardFinished(result) => return self.on_wizard_finished(result),
            Message::Dialog(message) => return self.on_dialog(message),
            Message::NewBackup => {
                if self.wizard.is_none() {
                    let (wizard, effects) = Wizard::create(home_dir().as_deref());
                    return self.start_wizard(wizard, effects);
                }
            }
            Message::ImportDejaDup => {
                if self.wizard.is_none() {
                    return Task::perform(
                        tasks::blocking(|| Ok(crate::dejadup::find())),
                        |found| app(Message::DejaDupFound(found.ok().flatten())),
                    );
                }
            }
            Message::DejaDupFound(found) => {
                use crate::dejadup::Place;
                match found {
                    None => self.dialog = Some(Dialog::Error(fl!("dejadup-none"))),
                    Some(import) if import.other_format => {
                        self.dialog = Some(Dialog::Error(fl!("dejadup-other-format")));
                    }
                    Some(crate::dejadup::Import {
                        place: Place::Unsupported(backend),
                        ..
                    }) => {
                        self.dialog =
                            Some(Dialog::Error(fl!("dejadup-unsupported", backend = backend)));
                    }
                    Some(import) => {
                        let (wizard, effects) = Wizard::import(&import);
                        return self.start_wizard(wizard, effects);
                    }
                }
            }
            Message::OpenExisting => {
                if self.wizard.is_none() {
                    let (wizard, effects) = Wizard::open();
                    return self.start_wizard(wizard, effects);
                }
            }
            Message::BackUpSelected => {
                if let Some(id) = self.selected().map(str::to_owned)
                    && let Some(profile) = self.config.profile(&id).cloned()
                {
                    let effects = self.pages.entry(id.clone()).or_default().back_up(&profile);
                    return self.run_profile_effects(&id, effects);
                }
            }
            Message::Tick => self.now = format::now(),
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
                if let Some(id) = self.core.main_window_id() {
                    return window::close(id);
                }
            }
            Message::WindowNew => match env::current_exe() {
                Ok(exe) => {
                    if let Err(err) = process::Command::new(&exe).spawn() {
                        error_log!(UI, "failed to execute {exe:?}: {err}");
                    }
                }
                Err(err) => error_log!(UI, "failed to get the current executable: {err}"),
            },
            Message::LaunchUrl(url) => {
                if let Err(err) = open::that_detached(&url) {
                    error_log!(UI, "failed to open {url:?}: {err}");
                }
            }
            Message::Key(modifiers, key) => {
                for (key_bind, action) in &self.key_binds {
                    if key_bind.matches(modifiers, &key, None) {
                        return self.update(action.message());
                    }
                }
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::AppTheme(index) => {
                let theme = match index {
                    1 => AppTheme::Dark,
                    2 => AppTheme::Light,
                    _ => AppTheme::System,
                };
                if let Some(handler) = &self.config_handler
                    && let Err(err) = self.config.set_app_theme(handler, theme)
                {
                    error_log!(CONFIG, "failed to save the theme: {err}");
                }
                return self.update_theme();
            }
            Message::ConfigChanged(config) => {
                if config != self.config {
                    self.config = config;
                    self.rebuild_nav(None);
                    return self.update_theme();
                }
            }
            Message::SystemThemeModeChange => return self.update_theme(),
            Message::Noop => {}
        }
        Task::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_requires_the_exact_name() {
        let dialog = |typed: &str| Dialog::DeleteAll {
            id: "x".into(),
            name: "Home".into(),
            typed: typed.into(),
            busy: false,
        };
        assert!(!dialog("").can_confirm());
        assert!(!dialog("home").can_confirm(), "case matters");
        assert!(!dialog("Home ").can_confirm(), "no trailing space");
        assert!(!dialog("Hom").can_confirm());
        assert!(dialog("Home").can_confirm());

        let busy = Dialog::DeleteAll {
            id: "x".into(),
            name: "Home".into(),
            typed: "Home".into(),
            busy: true,
        };
        assert!(!busy.can_confirm(), "not twice while the first delete runs");
    }

    #[test]
    fn other_dialogs_confirm_freely() {
        assert!(Dialog::Error("x".into()).can_confirm());
        assert!(
            Dialog::Remove {
                id: "x".into(),
                name: "Home".into()
            }
            .can_confirm()
        );
    }
}
