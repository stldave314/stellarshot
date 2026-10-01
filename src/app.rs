// SPDX-License-Identifier: GPL-3.0-only

//! The application window: the sidebar of backups, the page for the selected
//! one, the setup wizard, and the dialogs that confirm destructive actions.

use std::any::TypeId;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::{env, process};

use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::{Event as KeyEvent, Key, Modifiers};
use cosmic::iced::{Event, Length, Subscription, event, window};
use cosmic::widget::about::About;
use cosmic::widget::menu::{action::MenuAction, key_bind::KeyBind};
use cosmic::widget::{self, nav_bar};
use cosmic::{Application, ApplicationExt, Element, cosmic_config, cosmic_theme};

use crate::app::config::{CONFIG_VERSION, StellarshotConfig, profiles_key_path};
use crate::app::key_bind::key_binds;
use crate::app::pages::profile::{self, ProfileState};
use crate::app::pages::restore::{self, RestorePage};
use crate::app::wizard::{Mode, Wizard, place};
use crate::constants::{
    OPEN_COPY_MAX_AGE, OPEN_COPY_MAX_BYTES, RESTORE_RESULT_LIMIT, WINDOW_CLOCK_TICK, WINDOW_HEIGHT,
    WINDOW_WIDTH,
};
use crate::debug::{CONFIG, UI};
use crate::engine::{self, EngineError, Secret};
use crate::event_log;
use crate::profile::{Destination, Profile};
use crate::run_state::{self, RunState};
use crate::runner::{Event as RunnerEvent, Job, Operation};
use crate::settings_export;
use crate::timers;
use crate::{debug_log, error_log, fl};

pub mod applet;
pub mod child;
mod dialog;
mod effects;
mod key_bind;
mod launch;
pub mod menu;
pub mod migrate;
mod nav;
pub mod pages;
pub mod portal;
mod profiles;
pub mod startup;
pub mod tasks;
mod update;
pub mod wizard;

pub use dialog::{Dialog, DialogMessage, Dialogs};
use dialog::{delete_all_input_id, new_password_input_id, password_command_input_id};
pub use launch::{Flags, Launch};
use nav::NavItem;

pub use crate::constants::APP_ID;
// Shared with the code that runs without a window (`--run`, `--scheduled`,
// the applet), so they live outside `app`; reachable here as before.
pub use crate::core::{config, errors, format};

pub struct App {
    core: Core,
    nav: nav_bar::Model,
    about: About,
    settings: pages::settings::SettingsPage,
    config_handler: Option<cosmic_config::Config>,
    config: StellarshotConfig,
    context_page: ContextPage,
    dialogs: Dialogs,
    pages: HashMap<String, ProfileState>,
    wizard: Option<Wizard>,
    /// Which wizard is current: bumped every time one starts or is
    /// discarded. Every background result a wizard asks for carries the
    /// number it was asked under, and is ignored if it no longer matches —
    /// otherwise a late result (a finished repository creation, a size
    /// estimate) from a discarded wizard would act on the next one, or on
    /// none at all: starting a backup the user had discarded.
    wizard_session: u64,
    /// Which restore page is current: bumped each time one opens, so a
    /// result that finishes after the page was closed and another opened
    /// (for this backup or another) is dropped, not shown on the new one.
    restore_session: u64,
    /// Set to stop a cloud sign-in that is waiting on the browser.
    sign_in_cancel: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// The password a change is in progress for, and which backup: kept
    /// here rather than read back out of the dialog when the child
    /// finishes. The dialog can be gone by then (closed, or replaced by an
    /// unrelated error), but the repository's password has changed either
    /// way, and this session must switch to the new one or every later
    /// operation in it would use the old one.
    pending_password_change: Option<(String, engine::Secret)>,
    /// Whether the wizard that is creating or opening a repository asked
    /// for its password to be remembered. Acted on only once that has
    /// worked: storing it alongside the attempt saved every mistyped
    /// password too, and left an orphaned keyring item for each failed
    /// attempt, since a failed Create leaves no backup to ever clean one up.
    wizard_remember: bool,
    /// The restore page, and the password of the backup it is for.
    restore: Option<(RestorePage, Secret)>,
    key_binds: HashMap<KeyBind, Action>,
    modifiers: Modifiers,
    now: i64,
    /// Déjà Dup settings were found, so the first screen offers an import.
    dejadup: bool,
    /// What happened when each backup last ran on its own, by profile ID.
    /// Written by scheduled runs; re-read on every clock tick.
    runs: HashMap<String, RunState>,
    /// Every backup's history, merged and newest first: loaded fresh each
    /// time the History page is opened, so it never needs invalidating.
    history: Option<Vec<(String, event_log::Event)>>,
    /// The `profiles` key could not be read at startup (see
    /// `Flags::profiles_unreadable`), so `self.config.profiles` may be an
    /// empty stand-in for a list this binary simply could not parse,
    /// rather than a real absence of backups. While this is set: no
    /// timer is added or removed to match it (`init` skips its own
    /// `timers::reconcile` call), and `save_profiles` refuses to write
    /// anything, so the original file survives for a later Stellarshot
    /// version, or the user, to recover — rather than the in-memory empty
    /// list being written over it the next time anything would normally
    /// save.
    profiles_read_only: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleContextPage(ContextPage),
    CloseContextDrawer,
    LaunchUrl(String),
    CopyToClipboard(String),
    SystemThemeModeChange,
    /// Settings changed on disk, for example from another window.
    ConfigChanged(StellarshotConfig),
    Key(Modifiers, Key),
    Modifiers(Modifiers),
    WindowClose,
    WindowNew,
    /// Quit outright, rather than minimizing to the panel. Confirms first
    /// if a backup or other write is in progress anywhere.
    Quit,
    Tick,
    NewBackup,
    OpenExisting,
    ImportDejaDup,
    DejaDupFound(Option<crate::dejadup::Import>),
    /// Whether Déjà Dup has settings to import, found off the UI thread at startup.
    DejaDupDetected(bool),
    BackUpSelected,
    Profile(String, profile::Message),
    /// A message for the wizard that was open when it was produced: the
    /// number says which one (see `App::wizard_session`). A result from a
    /// wizard that has since been discarded or replaced is dropped rather
    /// than handed to whichever one exists now.
    Wizard(u64, wizard::Message),
    /// A second passed while something shows a running time.
    WaitingTick,
    /// A message for the restore page that was open when it was produced; see
    /// `App::restore_session`.
    RestorePage(u64, restore::Message),
    WizardFinished(u64, Box<Result<tasks::Finished, EngineError>>),
    /// Installing or removing a backup's timer failed.
    ScheduleFailed(String),
    Dialog(DialogMessage),
    Noop,
    /// REST server passwords moved from the settings to the keyring: each
    /// backup's ID and its address without the password.
    RestPasswordsMoved(Vec<(String, String)>),
    /// Every backup's run state, read off the window's thread.
    RunsLoaded(HashMap<String, RunState>),
    Settings(pages::settings::Message),
    /// The home screen's own way to switch to one backup's page.
    SelectProfile(String),
    /// Every backup's history, read from disk for the History page.
    HistoryLoaded(Vec<(String, event_log::Event)>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextPage {
    About,
    Settings,
    Help,
}

impl ContextPage {
    fn title(self) -> String {
        match self {
            Self::About => fl!("about"),
            Self::Settings => fl!("settings"),
            Self::Help => fl!("help"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    About,
    NewBackup,
    BackUpNow,
    ImportDejaDup,
    Settings,
    Help,
    WindowClose,
    WindowNew,
    Quit,
}

impl MenuAction for Action {
    type Message = Message;
    fn message(&self) -> Self::Message {
        match self {
            Action::About => Message::ToggleContextPage(ContextPage::About),
            Action::Help => Message::ToggleContextPage(ContextPage::Help),
            Action::NewBackup => Message::NewBackup,
            Action::BackUpNow => Message::BackUpSelected,
            Action::ImportDejaDup => Message::ImportDejaDup,
            Action::Settings => Message::ToggleContextPage(ContextPage::Settings),
            Action::WindowClose => Message::WindowClose,
            Action::WindowNew => Message::WindowNew,
            Action::Quit => Message::Quit,
        }
    }
}

/// Wrap an application message for a task.
fn app(message: Message) -> cosmic::Action<Message> {
    cosmic::Action::App(message)
}

/// Copies the `profiles` key's own file to a sibling tagged with the
/// current time, before anything is ever saved over it — see
/// `App::profiles_read_only`. Returns the path shown in the startup
/// warning: the backup copy's path when the copy succeeded, the original
/// file's path otherwise (still worth knowing, even though nothing new
/// was written there), or a fixed fallback if even the original path
/// could not be found — which in practice cannot happen here, since
/// reaching this function at all already required `Config::new` (which
/// resolves that same path internally) to have succeeded.
fn backup_unreadable_profiles() -> String {
    let Some(original) = profiles_key_path() else {
        return fl!("settings-directory");
    };
    if !original.exists() {
        // Nothing to copy; still tell the user which file to look at.
        return original.display().to_string();
    }
    let backup = original.with_file_name(format!("profiles.unreadable-{}", format::now()));
    match std::fs::copy(&original, &backup) {
        Ok(_) => {
            error_log!(
                CONFIG,
                "profiles could not be read; copied {} to {} before anything can be saved",
                original.display(),
                backup.display()
            );
            backup.display().to_string()
        }
        Err(err) => {
            error_log!(
                CONFIG,
                "profiles could not be read, and the safety copy to {} also failed: {err}",
                backup.display()
            );
            original.display().to_string()
        }
    }
}

impl App {
    fn update_theme(&mut self) -> Task<Message> {
        cosmic::command::set_theme(self.config.app_theme.theme())
    }

    /// Install, update or remove `profile`'s timer, off the UI thread.
    /// Show the selected profile, looking for its password if needed.
    fn activate_selected(&mut self) -> Task<Message> {
        let Some(id) = self.selected().map(str::to_owned) else {
            return Task::none();
        };
        let Some(profile) = self.config.profile(&id).cloned() else {
            return Task::none();
        };
        let effects = self.pages.entry(id.clone()).or_default().activate(&profile);
        self.run_profile_effects(&id, effects)
    }

    /// Read every backup's history from disk, off the UI thread: reloaded
    /// every time the History page is opened rather than cached, since
    /// nothing about the page's own state can go stale that way.
    fn load_history(&self) -> Task<Message> {
        let profile_ids: Vec<String> = self
            .config
            .profiles
            .iter()
            .map(|profile| profile.id.clone())
            .collect();
        Task::perform(
            tasks::blocking(move || Ok(event_log::load_all(&profile_ids))),
            |result: Result<Vec<(String, event_log::Event)>, EngineError>| {
                app(Message::HistoryLoaded(result.unwrap_or_default()))
            },
        )
    }

    /// Something on screen shows a running time.
    fn waiting(&self) -> bool {
        self.wizard.as_ref().is_some_and(Wizard::waiting)
            || self.pages.values().any(ProfileState::is_busy)
            // A restore's running time counts up too.
            || self
                .restore
                .as_ref()
                .is_some_and(|(page, _)| page.is_restoring())
    }

    fn show_error(&mut self, context: &str, error: &EngineError) {
        self.dialogs
            .notify(Dialog::Error(errors::describe(context, error)));
    }

    fn start_wizard(&mut self, wizard: Wizard, effects: Vec<wizard::Effect>) -> Task<Message> {
        self.wizard = Some(wizard);
        self.wizard_session += 1;
        self.select_wizard();
        self.run_wizard_effects(effects)
    }
}

/// Launch a genuinely new window rather than reactivating this one.
///
/// `crate::exe::installed_path`, not a raw `current_exe`, so this still
/// finds the right binary if it was replaced on disk while this process
/// kept running (a package upgrade). Launched through `cosmic::process::spawn`,
/// which double-forks so the new window is never left as a zombie once this
/// process exits, unlike a plain `Command::spawn` that nothing here `wait`s
/// on.
async fn spawn_new_window() {
    let exe = match crate::exe::installed_path() {
        Ok(exe) => exe,
        Err(err) => {
            error_log!(UI, "failed to find this app's own executable: {err}");
            return;
        }
    };
    // Single-instance activation (see `Cargo.toml`'s comment on the
    // `libcosmic` `single-instance` feature) is what lets the applet reopen
    // a window closed to the panel, but it would also swallow an explicit
    // "new window" into just refocusing this one. Opt this one launch out.
    let mut command = process::Command::new(&exe);
    command.env("COSMIC_SINGLE_INSTANCE", "false");
    if cosmic::process::spawn(command).await.is_none() {
        error_log!(UI, "failed to execute {exe:?}");
    }
}

/// The user's home folder, the default thing to back up.
fn home_dir() -> Option<PathBuf> {
    crate::paths::home_dir()
}

/// Add `kind` to backup `id`'s history, off the window's thread: it reads
/// and writes the state store under a lock other processes share.
fn record_event(id: String, kind: event_log::EventKind) -> Task<Message> {
    Task::perform(
        tasks::blocking(move || {
            event_log::record(&id, format::now(), kind, event_log::Source::Desktop);
            Ok(())
        }),
        |_| cosmic::Action::App(Message::Noop),
    )
}

/// A snapshot mounted for a restore page that has since closed: unmounting
/// waits for the mount's session to end, so the handle is dropped on a
/// blocking thread, never on the window's.
fn unmount_off_thread(message: restore::Message) -> Task<Message> {
    let restore::Message::Mounted(Ok(handle)) = message else {
        return Task::none();
    };
    Task::perform(
        tasks::blocking(move || {
            drop(handle);
            Ok(())
        }),
        |_| cosmic::Action::App(Message::Noop),
    )
}

/// Remove "Open a copy" folders left in the runtime folder for more than a
/// day, off the UI thread.
fn remove_old_open_copies() -> Task<Message> {
    Task::perform(
        tasks::blocking(|| {
            engine::lock::remove_stale_open_copies(OPEN_COPY_MAX_AGE);
            // And any rclone a crashed run or window left connected.
            engine::stop_orphan_rclones();
            // And any snapshot it left mounted, now answering nothing.
            engine::mount::unmount_dead();
            Ok(())
        }),
        |_| cosmic::Action::App(Message::Noop),
    )
}

impl fmt::Debug for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("App")
            .field("profiles", &self.config.profiles.len())
            .field("wizard", &self.wizard.is_some())
            .field("restore", &self.restore.is_some())
            .finish_non_exhaustive()
    }
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

    /// Closing the window minimizes to the panel applet rather than
    /// quitting: [`get_app_settings`] turns off the default
    /// close-quits-the-app behavior, so this is reached instead. The
    /// process (and any backup in progress) keeps running with no window
    /// open; the applet, or launching `stellarshot` again, reopens it
    /// through [`Self::dbus_activation`].
    ///
    /// [`get_app_settings`]: crate::app::startup::get_app_settings
    fn on_close_requested(&self, _id: window::Id) -> Option<Self::Message> {
        Some(Message::WindowClose)
    }

    /// Launching `stellarshot` again while this one is already running
    /// (single-instance; see `Cargo.toml`'s comment on the `libcosmic`
    /// `single-instance` feature) reaches this instead of a second window.
    /// Reopens the window if it was closed to the panel, or brings it to
    /// the front if it was merely behind something else — and, if the new
    /// launch asked for something specific (`--new-backup`, `--restore`,
    /// or a notification's `--profile <id>`), does that too, the same as
    /// [`Self::init`] would have for a fresh start.
    fn dbus_activation(&mut self, msg: cosmic::dbus_activation::Message) -> Task<Self::Message> {
        let raise = if let Some(id) = self.core.main_window_id() {
            Task::batch([window::gain_focus(id), window::minimize(id, false)])
        } else {
            let (id, open) = window::open(window::Settings {
                size: cosmic::iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT),
                // `cosmic::app::Settings` sets this for the window the
                // app starts with (client-side decorations, the default
                // for a COSMIC app); a bare `window::Settings::default()`
                // does not, so this reopened window got both the
                // compositor's own title bar and the app's own — a
                // double one.
                decorations: false,
                ..window::Settings::default()
            });
            self.core.set_main_window_id(Some(id));
            open.map(|_| cosmic::Action::App(Message::Noop))
        };
        // D-Bus carries only plain strings either way, whatever `Flags`'s
        // own `CosmicFlags::SubCommand`/`Args` are typed as on the sending
        // side (see `Launch`'s own doc comment) — never automatically
        // turned back into `Launch` the way sending it out was typed.
        let launch = match msg.msg {
            cosmic::dbus_activation::Details::ActivateAction { action, args } => {
                Launch::from_wire(&action, &args)
            }
            cosmic::dbus_activation::Details::Activate
            | cosmic::dbus_activation::Details::Open { .. } => None,
        };
        let requested = match launch {
            Some(Launch::NewBackup) => self.update(Message::NewBackup),
            Some(Launch::Restore) => match self.selected().map(str::to_owned) {
                // Already unlocked (the window was open and in use): no
                // future unlock event will ever come along to notice
                // `restore_when_unlocked`, so switch to it directly, the
                // same message the Restore tab itself sends.
                Some(id) if self.pages.get(&id).is_some_and(ProfileState::is_unlocked) => {
                    self.update(Message::Profile(id, profile::Message::Restore))
                }
                Some(id) => {
                    self.pages.entry(id).or_default().restore_when_unlocked = true;
                    Task::none()
                }
                None => Task::none(),
            },
            Some(Launch::Profile(id)) => {
                self.rebuild_nav(Some(&id));
                self.activate_selected()
            }
            None => Task::none(),
        };
        Task::batch([raise, requested])
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu::menu_bar(&self.key_binds)]
    }

    fn header_end(&self) -> Vec<Element<'_, Self::Message>> {
        vec![
            widget::button::icon(widget::icon::from_name("preferences-system-symbolic"))
                .tooltip(fl!("settings"))
                .name(fl!("settings"))
                .on_press(Message::ToggleContextPage(ContextPage::Settings))
                .into(),
        ]
    }

    fn nav_model(&self) -> Option<&nav_bar::Model> {
        // No sidebar until there is a list for the wizard to sit beside: the
        // empty state (setting up the very first backup) and the restore
        // page fill the window instead. Once at least one backup exists,
        // the wizard shows beside the list rather than over it, and stays
        // running if the user looks at something else.
        (!self.config.profiles.is_empty() && self.restore.is_none()).then_some(&self.nav)
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let flags_start_wizard = flags.start_wizard;
        let flags_start_restore = flags.start_restore;
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
            // libcosmic links every developer by email; the GitHub no-reply
            // address, never a personal one.
            .developers([
                ("stldave314", "stldave314@users.noreply.github.com"),
                ("Aaron Honeycutt", "aaronhoneycutt@proton.me"),
                ("Eduardo Flores", "edfloreshz@proton.me"),
            ]);
        let mut app = App {
            core,
            nav: nav_bar::Model::default(),
            about,
            settings: pages::settings::SettingsPage::default(),
            context_page: ContextPage::Settings,
            config_handler: flags.config_handler,
            config: flags.config,
            dialogs: Dialogs::default(),
            pages: HashMap::new(),
            wizard: None,
            wizard_session: 0,
            restore_session: 0,
            sign_in_cancel: None,
            pending_password_change: None,
            wizard_remember: false,
            restore: None,
            key_binds: key_binds(),
            modifiers: Modifiers::empty(),
            now: format::now(),
            // Filled in by `Message::DejaDupDetected`: finding it runs `dconf`,
            // which must not hold up the first frame.
            dejadup: false,
            runs: HashMap::new(),
            history: None,
            profiles_read_only: flags.profiles_unreadable,
        };
        if app.profiles_read_only {
            let path = backup_unreadable_profiles();
            app.dialogs
                .notify(Dialog::Error(fl!("error-config-unreadable", path = path)));
        }
        app.reload_runs();
        app.rebuild_nav(flags.select.as_deref());

        let title = fl!("stellarshot");
        app.set_header_title(title.clone());
        let title_task = match app.core.main_window_id() {
            Some(id) => app.set_window_title(title, id),
            None => Task::none(),
        };
        if flags_start_restore && let Some(id) = app.selected().map(str::to_owned) {
            app.pages.entry(id).or_default().restore_when_unlocked = true;
        }
        // A REST server backup cannot be opened before its password is read.
        let secure = app.secure_rest_passwords();
        let activate = secure.chain(app.activate_selected());
        // Timers follow the settings, which may have changed while the
        // window was closed, or the program may have moved. Skipped
        // entirely while `profiles_read_only` is set: reconciling against
        // an empty stand-in for a list this binary could not actually
        // parse would remove every real timer, not just ones genuinely no
        // longer scheduled.
        let reconcile = if app.profiles_read_only {
            Task::none()
        } else {
            let profiles = app.config.profiles.clone();
            Task::perform(
                tasks::blocking(move || Ok(timers::reconcile(&profiles))),
                |errors: Result<Vec<String>, EngineError>| match errors
                    .ok()
                    .and_then(|e| e.into_iter().next())
                {
                    Some(err) => cosmic::Action::App(Message::ScheduleFailed(err)),
                    None => cosmic::Action::App(Message::Noop),
                },
            )
        };
        let wizard = if flags_start_wizard {
            app.update(Message::NewBackup)
        } else {
            Task::none()
        };
        debug_log!(UI, "started with {} profiles", app.config.profiles.len());
        let detect_dejadup = Task::perform(
            tasks::blocking(|| Ok(crate::dejadup::find().is_some())),
            |found| cosmic::Action::App(Message::DejaDupDetected(found.unwrap_or(false))),
        );
        (
            app,
            Task::batch([
                title_task,
                activate,
                reconcile,
                wizard,
                detect_dejadup,
                remove_old_open_copies(),
            ]),
        )
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
                self.settings.view(&self.config).map(Message::Settings),
                Message::CloseContextDrawer,
            )
            .title(title),
            ContextPage::Help => cosmic::app::context_drawer::context_drawer(
                pages::help::view(),
                Message::CloseContextDrawer,
            )
            .title(title),
        })
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        Some(dialog::view(self.dialogs.front()?, &self.config))
    }

    fn on_nav_select(&mut self, id: nav_bar::Id) -> Task<Self::Message> {
        if let Some(NavItem::New) = self.nav.data::<NavItem>(id) {
            // "New backup" opens the wizard; the selection stays where it was.
            return self.update(Message::NewBackup);
        }
        self.nav.activate(id);
        if matches!(self.nav.data::<NavItem>(id), Some(NavItem::History)) {
            return self.load_history();
        }
        self.activate_selected()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        // With no backup yet there is no list for the wizard to sit beside,
        // so it still fills the window (there is also no sidebar to switch
        // away with in that case: see `nav_model`). Otherwise it only shows
        // while its own sidebar entry is selected; "finish later" leaves it
        // running underneath whatever is chosen instead.
        if let Some(wizard) = &self.wizard
            && (self.config.profiles.is_empty() || self.showing_wizard())
        {
            let session = self.wizard_session;
            return wizard
                .view()
                .map(move |message| Message::Wizard(session, message));
        }
        if let Some((page, _)) = &self.restore {
            let name = self
                .config
                .profile(&page.profile_id)
                .map(|profile| profile.name.as_str())
                .unwrap_or_default();
            let session = self.restore_session;
            return page
                .view(name)
                .map(move |message| Message::RestorePage(session, message));
        }
        if self.config.profiles.is_empty() {
            return pages::empty::view(self.dejadup);
        }
        if self.showing_home() {
            return pages::home::view(&self.config.profiles, &self.runs, &self.pages, self.now);
        }
        if self.showing_history() {
            let entries = self.history.as_deref().unwrap_or_default();
            return pages::history::view(entries, &self.config.profiles);
        }
        let Some(id) = self.selected() else {
            return widget::space::horizontal().width(Length::Fill).into();
        };
        match (self.config.profile(id), self.pages.get(id)) {
            (Some(profile), Some(state)) => {
                let run = self.runs.get(id).cloned().unwrap_or_default();
                let id = id.to_owned();
                state
                    .view(profile, &run, self.now)
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
            cosmic::iced::time::every(WINDOW_CLOCK_TICK).map(|_| Message::Tick),
            // Running times count up only while something is running.
            if self.waiting() {
                cosmic::iced::time::every(crate::constants::WAITING_TICK)
                    .map(|_| Message::WaitingTick)
            } else {
                Subscription::none()
            },
        ])
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        self.on_message(message)
    }
}
