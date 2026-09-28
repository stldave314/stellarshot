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
use cosmic::iced::{Alignment, Event, Length, Subscription, event, window};
use cosmic::widget::about::About;
use cosmic::widget::menu::{action::MenuAction, key_bind::KeyBind};
use cosmic::widget::{self, nav_bar};
use cosmic::{Application, ApplicationExt, Element, cosmic_config, cosmic_theme, theme};

use crate::app::config::{AppTheme, CONFIG_VERSION, NetworkScope, StellarshotConfig};
use crate::app::key_bind::key_binds;
use crate::app::pages::profile::{self, ProfileState};
use crate::app::pages::restore::{self, RestorePage};
use crate::app::wizard::{Mode, Wizard, place};
use crate::constants::{WINDOW_HEIGHT, WINDOW_WIDTH};
use crate::debug::{CONFIG, ENGINE, UI};
use crate::engine::{self, EngineError, Secret};
use crate::event_log;
use crate::profile::Profile;
use crate::run_state::{self, RunState};
use crate::runner::{Event as RunnerEvent, Job, Operation};
use crate::schedule;
use crate::settings_export;
use crate::web::valid_allow_list_entry;
use crate::web_daemon;
use crate::{debug_log, error_log, fl};

pub mod applet;
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
    /// The Settings page's pattern field, for a global exclusion not yet added.
    global_exclude_pattern_input: String,
    /// Every backup's history, merged and newest first: loaded fresh each
    /// time the History page is opened, so it never needs invalidating.
    history: Option<Vec<(String, event_log::Event)>>,
    /// The Settings page's own draft fields, before they are saved.
    web_password_input: String,
    web_allowed_address_input: String,
    web_port_input: String,
    /// Whether the last save attempt succeeded, shown under the password
    /// field instead of a dialog. Cleared as soon as the field is edited
    /// again.
    web_password_status: Option<Result<(), EngineError>>,
    /// Whether the daemon is running, for Settings' status indicator.
    /// Queried fresh each time Settings is opened, and after every
    /// Start/Stop/Restart: nothing pushes a live update the rest of the
    /// time, so a stale answer only shows while the page is closed.
    web_daemon_status: web_daemon::Status,
}

/// What a sidebar entry leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
enum NavItem {
    /// Every backup at a glance: status, folders and storage locations.
    Home,
    /// Every backup's history in one place, across every profile.
    History,
    Profile(String),
    /// Opens a fresh wizard. Replaced by `Wizard` once one is in progress.
    New,
    /// A wizard is in progress; selecting this shows it again.
    Wizard,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleContextPage(ContextPage),
    CloseContextDrawer,
    LaunchUrl(String),
    CopyToClipboard(String),
    AppTheme(usize),
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
    BackUpSelected,
    Profile(String, profile::Message),
    Wizard(wizard::Message),
    /// A second passed while something shows a running time.
    WaitingTick,
    RestorePage(restore::Message),
    WizardFinished(Box<Result<tasks::Finished, EngineError>>),
    /// Installing or removing a backup's timer failed.
    ScheduleFailed(String),
    Dialog(DialogMessage),
    Noop,
    ExportSettings,
    /// Chose where to save, or canceled.
    ExportChosen(Option<PathBuf>),
    ExportSaved(Result<(), String>),
    ImportSettings,
    /// Chose a file to import, or canceled.
    ImportChosen(Option<PathBuf>),
    ImportRead(Result<settings_export::Export, String>),
    /// The home screen's own way to switch to one backup's page.
    SelectProfile(String),
    GlobalExcludePatternInput(String),
    AddGlobalExcludePattern,
    RemoveGlobalExcludePattern(usize),
    ChooseCacheDir,
    CacheDirChosen(Option<PathBuf>),
    ClearCacheDir,
    NoCache(bool),
    /// Every backup's history, read from disk for the History page.
    HistoryLoaded(Vec<(String, event_log::Event)>),
    WebScope(NetworkScope),
    WebPasswordEnabled(bool),
    WebPasswordInput(String),
    SaveWebPassword,
    WebPasswordSaved(Result<(), String>),
    WebTokenEnabled(bool),
    GenerateWebToken,
    WebPamEnabled(bool),
    WebAllowedAddressInput(String),
    AddWebAllowedAddress,
    RemoveWebAllowedAddress(usize),
    WebPortInput(String),
    SaveWebPort,
    ChooseWebTlsCert,
    WebTlsCertChosen(Option<PathBuf>),
    ClearWebTlsCert,
    ChooseWebTlsKey,
    WebTlsKeyChosen(Option<PathBuf>),
    ClearWebTlsKey,
    WebDaemonStart,
    WebDaemonStop,
    WebDaemonRestart,
    /// A Start/Stop/Restart button's own systemd call finished.
    WebDaemonActed(Result<(), String>),
    WebDaemonStatus(web_daemon::Status),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextPage {
    About,
    Settings,
    Help,
}

impl ContextPage {
    fn title(&self) -> String {
        match self {
            Self::About => fl!("about"),
            Self::Settings => fl!("settings"),
            Self::Help => fl!("help"),
        }
    }
}

/// A modal dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dialog {
    /// A localized explanation of something that failed.
    Error(String),
    /// Something finished, and here is what happened.
    Info(String, String),
    /// A freshly generated API token, shown once, copyable from here since
    /// it can never be shown again after this.
    Token(String),
    /// About to replace the web interface's current API token: anything
    /// still using it will stop working the moment this is confirmed.
    RegenerateToken,
    /// Forget a profile; its data stays.
    Remove { id: String, name: String },
    /// Delete a profile's repository and everything in it.
    DeleteAll {
        id: String,
        name: String,
        typed: String,
        busy: bool,
    },
    /// Cancel was pressed in the wizard: keep the draft to finish later, or
    /// discard it.
    WizardCancel,
    /// Quit was chosen while a backup or other write was in progress
    /// somewhere. `--run` children survive their parent exiting, by
    /// design, so confirming here only asks whether to stop watching.
    Quit,
    /// The trash icon next to one snapshot was pressed. `label` is its
    /// time, already formatted (see `profile::Effect::ConfirmDeleteSnapshot`).
    DeleteSnapshot {
        id: String,
        snapshot: String,
        label: String,
    },
    /// Editing a backup's `password_command`: `text` is the field as typed.
    PasswordCommand { id: String, text: String },
    /// Changing a backup's password: it must already be unlocked, since
    /// changing it needs the repository open.
    ChangePassword {
        id: String,
        password: String,
        confirm: String,
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
            Self::ChangePassword {
                password,
                confirm,
                busy,
                ..
            } => !busy && !password.is_empty() && password == confirm,
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
    /// Show an error from a background task.
    Failed(String, EngineError),
    /// Keep the wizard's draft, only hide it: `Dialog::WizardCancel`'s own
    /// two actions, kept apart from `Confirm`/`Close` since neither means
    /// "just dismiss" here.
    FinishWizardLater,
    DiscardWizard,
    NewPassword(String),
    ConfirmPassword(String),
    /// One line from the `change-password` child; see [`profile::Message::Pinned`]
    /// for why the raw event, not just the outcome, is threaded through.
    PasswordChanged(String, child::ChildEvent),
    /// The repository password changed. `Some` when the keyring entry that
    /// remembered the old one could not be replaced with the new one.
    KeyringUpdateFailed(Option<String>),
    /// A backup or unlock asked to remember its password. `Some` when the
    /// keyring refused it: the repository itself is unaffected, but a
    /// scheduled run will have no password to use later.
    PasswordNotRemembered(Option<String>),
}

/// What launching `stellarshot` again asked for: the desktop entry's two
/// actions, and a scheduled run's failure notification. Doubles as the
/// action `CosmicFlags` forwards to an already-running instance over
/// D-Bus (see the `CosmicFlags` impl below) — its `Display` is the wire
/// name, `parse_activation` reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    NewBackup,
    Restore,
    Profile(String),
}

impl std::fmt::Display for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NewBackup => "new-backup",
            Self::Restore => "restore",
            Self::Profile(_) => "profile",
        })
    }
}

impl Launch {
    /// From the flags this process itself was started with: mirrors
    /// `main.rs`'s own precedence (a new backup, then restore, then a
    /// specific profile) so a single-instance activation asks the running
    /// window for exactly what a fresh launch would have done.
    pub fn from_flags(
        start_wizard: bool,
        start_restore: bool,
        select: &Option<String>,
    ) -> Option<Self> {
        if start_wizard {
            Some(Self::NewBackup)
        } else if start_restore {
            Some(Self::Restore)
        } else {
            select.clone().map(Self::Profile)
        }
    }

    /// The reverse of forwarding one over D-Bus: `action`/`args` arrive as
    /// plain strings either way (D-Bus itself carries nothing else), never
    /// automatically turned back into this enum the way sending it out
    /// was typed.
    fn from_wire(action: &str, args: &[String]) -> Option<Self> {
        match action {
            "new-backup" => Some(Self::NewBackup),
            "restore" => Some(Self::Restore),
            "profile" => args.first().cloned().map(Self::Profile),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: StellarshotConfig,
    /// Open the setup wizard as soon as the window appears.
    pub start_wizard: bool,
    /// Select this backup at start: a notification was clicked.
    pub select: Option<String>,
    /// Open the restore page for the selected backup once it is unlocked.
    pub start_restore: bool,
    /// The same three flags above, folded into one: what `CosmicFlags`
    /// forwards to an already-running instance instead of starting a
    /// second one (see [`App::dbus_activation`]). Kept alongside them,
    /// not derived from `App` at the point `CosmicFlags` needs it, since
    /// `action`/`args` must return references into `self`.
    pub launch: Option<Launch>,
}

/// Required to launch single-instance (see `Cargo.toml`'s comment on the
/// `libcosmic` `single-instance` feature): forwards whichever of
/// `--new-backup`, `--restore` or `--profile <id>` this process itself was
/// started with to an already-running instance, through
/// [`App::dbus_activation`], rather than only raising its window with no
/// information about what was actually asked for.
impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = Launch;
    type Args = Vec<String>;

    fn action(&self) -> Option<&Self::SubCommand> {
        self.launch.as_ref()
    }

    fn args(&self) -> Vec<&str> {
        match &self.launch {
            Some(Launch::Profile(id)) => vec![id.as_str()],
            _ => Vec::new(),
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

impl App {
    fn update_theme(&mut self) -> Task<Message> {
        cosmic::command::set_theme(self.config.app_theme.theme())
    }

    fn settings_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let selected = match self.config.app_theme {
            AppTheme::Dark => 1,
            AppTheme::Light => 2,
            AppTheme::System => 0,
        };
        let cache_dir_label = self
            .config
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
                                        (!self.config.no_cache).then_some(Message::ChooseCacheDir),
                                    ),
                            )
                            .push_maybe(self.config.cache_dir.is_some().then(|| {
                                widget::button::standard(fl!("settings-cache-dir-reset"))
                                    .on_press(Message::ClearCacheDir)
                            })),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("settings-no-cache"))
                    .description(fl!("settings-no-cache-description"))
                    .toggler(self.config.no_cache, Message::NoCache),
            );

        let mut global_excludes = widget::settings::section()
            .title(fl!("settings-global-excludes-title"))
            .add(widget::text::body(fl!(
                "settings-global-excludes-description"
            )));
        for (index, pattern) in self.config.global_exclude_patterns.iter().enumerate() {
            global_excludes = global_excludes.add(
                widget::settings::item::builder(pattern.clone()).control(
                    widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                        .tooltip(fl!("remove"))
                        .name(fl!("remove"))
                        .on_press(Message::RemoveGlobalExcludePattern(index)),
                ),
            );
        }
        global_excludes = global_excludes.add(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center)
                .push(
                    widget::text_input(
                        fl!("wizard-pattern-placeholder"),
                        &self.global_exclude_pattern_input,
                    )
                    .on_input(Message::GlobalExcludePatternInput)
                    .on_submit(|_| Message::AddGlobalExcludePattern)
                    .width(Length::Fill),
                )
                .push(
                    widget::button::standard(fl!("add")).on_press(Message::AddGlobalExcludePattern),
                ),
        );

        let scope_item = |title: String, description: String, scope: NetworkScope| {
            widget::settings::item::builder(title)
                .description(description)
                .radio(scope, Some(self.config.web.scope), Message::WebScope)
        };
        let web = widget::settings::section()
            .title(fl!("settings-web-title"))
            .add(scope_item(
                fl!("web-scope-off"),
                fl!("web-scope-off-description"),
                NetworkScope::Off,
            ))
            .add(scope_item(
                fl!("web-scope-localhost"),
                fl!("web-scope-localhost-description"),
                NetworkScope::Localhost,
            ))
            .add(scope_item(
                fl!("web-scope-lan"),
                fl!("web-scope-lan-description"),
                NetworkScope::Lan,
            ))
            .add({
                // Starts holding the port actually in effect (reset whenever
                // Settings opens — see `Message::ToggleContextPage`), so
                // this is never empty behind a placeholder that only looks
                // pre-filled. Checked on every keystroke, not just on Save:
                // an invalid value disables Save and says why underneath,
                // instead of a save that silently could not have worked.
                let port_valid = parse_port(&self.web_port_input).is_some();
                let mut field = widget::column::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(
                        widget::row::with_capacity(2)
                            .spacing(spacing.space_xs)
                            .push(
                                widget::text_input("", &self.web_port_input)
                                    .on_input(Message::WebPortInput)
                                    .on_submit(|_| Message::SaveWebPort)
                                    .width(Length::Fixed(120.0)),
                            )
                            .push(
                                widget::button::standard(fl!("save"))
                                    .on_press_maybe(port_valid.then_some(Message::SaveWebPort)),
                            ),
                    );
                if !port_valid {
                    field = field.push(widget::text::caption(fl!("web-port-invalid")));
                }
                widget::settings::item::builder(fl!("web-port"))
                    .description(fl!("web-port-description"))
                    .control(field)
            })
            .add_maybe(
                web_address(self.config.web.scope, self.config.web.port).map(|url| {
                    let row: Element<'_, Message> = widget::row::with_capacity(2)
                        .spacing(spacing.space_xxs)
                        .push(widget::text::body(fl!("web-address-label")))
                        .push(widget::button::link(url.clone()).on_press(Message::LaunchUrl(url)))
                        .into();
                    row
                }),
            )
            .add(
                widget::settings::item::builder(fl!("web-auth-password"))
                    .description(fl!("web-auth-password-description"))
                    .toggler(
                        self.config.web.password_enabled,
                        Message::WebPasswordEnabled,
                    ),
            )
            .add_maybe(self.config.web.password_enabled.then(|| {
                // Checked on every keystroke, the same as the port field
                // above: a caption under the field says how many more
                // characters are needed, live, instead of a dialog only
                // after Save is pressed.
                let count = self.web_password_input.chars().count();
                let long_enough = password_long_enough(&self.web_password_input);
                let can_save = !self.web_password_input.is_empty() && long_enough;
                let mut field = widget::column::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .push(
                        widget::row::with_capacity(2)
                            .spacing(spacing.space_xs)
                            .push(
                                widget::secure_input(
                                    fl!("web-password-placeholder"),
                                    &self.web_password_input,
                                    None,
                                    true,
                                )
                                .on_input(Message::WebPasswordInput)
                                .on_submit(|_| Message::SaveWebPassword)
                                .width(Length::Fill),
                            )
                            .push(
                                widget::button::standard(fl!("save"))
                                    .on_press_maybe(can_save.then_some(Message::SaveWebPassword)),
                            ),
                    );
                if !self.web_password_input.is_empty() && !long_enough {
                    let count = count as i64;
                    let minimum = crate::constants::WEB_PASSWORD_MIN_LENGTH as i64;
                    field = field.push(widget::text::caption(fl!(
                        "web-password-length",
                        count = count,
                        minimum = minimum
                    )));
                }
                match &self.web_password_status {
                    Some(Ok(())) => {
                        field = field.push(widget::text::caption(fl!("web-password-saved-body")));
                    }
                    Some(Err(err)) => {
                        field = field.push(widget::text::caption(errors::describe(
                            &fl!("web-password-failed"),
                            err,
                        )));
                    }
                    None => {}
                }
                widget::settings::item::builder(fl!("web-password-set")).control(field)
            }))
            .add(
                widget::settings::item::builder(fl!("web-auth-token"))
                    .description(fl!("web-auth-token-description"))
                    .toggler(self.config.web.token_enabled, Message::WebTokenEnabled),
            )
            .add_maybe(self.config.web.token_enabled.then(|| {
                let status = if self.config.web.token_hash.is_some() {
                    fl!("web-token-exists")
                } else {
                    fl!("web-token-none")
                };
                widget::settings::item::builder(fl!("web-token-generate"))
                    .description(status)
                    .control(
                        widget::button::standard(fl!("web-token-generate-button"))
                            .on_press(Message::GenerateWebToken),
                    )
            }))
            .add(
                widget::settings::item::builder(fl!("web-auth-pam"))
                    .description(fl!("web-auth-pam-description"))
                    .toggler(self.config.web.pam_enabled, Message::WebPamEnabled),
            );

        let mut web_allowed = widget::settings::section()
            .title(fl!("web-allowed-title"))
            .add(widget::text::body(fl!("web-allowed-description")));
        for (index, address) in self.config.web.allowed_addresses.iter().enumerate() {
            web_allowed = web_allowed.add(
                widget::settings::item::builder(address.clone()).control(
                    widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                        .tooltip(fl!("remove"))
                        .name(fl!("remove"))
                        .on_press(Message::RemoveWebAllowedAddress(index)),
                ),
            );
        }
        {
            // Checked as it is typed, not only once it is already saved and
            // an address that could never match anything (a typo, the wrong
            // shape) has silently locked out whoever just added it — the
            // same shape of bug the port and password fields above had.
            let text = self.web_allowed_address_input.trim();
            let valid = text.is_empty() || valid_allow_list_entry(text);
            let mut add_row = widget::column::with_capacity(2)
                .spacing(spacing.space_xxs)
                .push(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xs)
                        .align_y(Alignment::Center)
                        .push(
                            widget::text_input(
                                fl!("web-allowed-placeholder"),
                                &self.web_allowed_address_input,
                            )
                            .on_input(Message::WebAllowedAddressInput)
                            .on_submit(|_| Message::AddWebAllowedAddress)
                            .width(Length::Fill),
                        )
                        .push(widget::button::standard(fl!("add")).on_press_maybe(
                            (!text.is_empty() && valid).then_some(Message::AddWebAllowedAddress),
                        )),
                );
            if !valid {
                add_row = add_row.push(widget::text::caption(fl!("web-allowed-invalid")));
            }
            web_allowed = web_allowed.add(add_row);
        }

        let choose_or_reset = |chosen: bool, choose: Message, reset: Message| {
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .push(widget::button::standard(fl!("settings-cache-dir-choose")).on_press(choose))
                .push_maybe(chosen.then(|| {
                    widget::button::standard(fl!("settings-cache-dir-reset")).on_press(reset)
                }))
        };
        let web_tls = widget::settings::section()
            .title(fl!("web-tls-title"))
            .add(widget::text::body(fl!("web-tls-description")))
            .add(
                widget::settings::item::builder(fl!("web-tls-cert"))
                    .description(
                        self.config
                            .web
                            .tls_cert_path
                            .as_ref()
                            .map(|path| format::path(path))
                            .unwrap_or_else(|| fl!("web-tls-default")),
                    )
                    .control(choose_or_reset(
                        self.config.web.tls_cert_path.is_some(),
                        Message::ChooseWebTlsCert,
                        Message::ClearWebTlsCert,
                    )),
            )
            .add(
                widget::settings::item::builder(fl!("web-tls-key"))
                    .description(
                        self.config
                            .web
                            .tls_key_path
                            .as_ref()
                            .map(|path| format::path(path))
                            .unwrap_or_else(|| fl!("web-tls-default")),
                    )
                    .control(choose_or_reset(
                        self.config.web.tls_key_path.is_some(),
                        Message::ChooseWebTlsKey,
                        Message::ClearWebTlsKey,
                    )),
            );

        let daemon_status = match self.web_daemon_status {
            web_daemon::Status::Active => fl!("web-daemon-status-active"),
            web_daemon::Status::Inactive => fl!("web-daemon-status-inactive"),
            web_daemon::Status::Failed => fl!("web-daemon-status-failed"),
            web_daemon::Status::Unknown => fl!("web-daemon-status-unknown"),
        };
        let web_daemon_section = widget::settings::section()
            .title(fl!("web-daemon-title"))
            .add(
                widget::settings::item::builder(fl!("web-daemon-status"))
                    .description(daemon_status)
                    .control(
                        widget::row::with_capacity(3)
                            .spacing(spacing.space_xs)
                            .push_maybe(
                                (self.web_daemon_status != web_daemon::Status::Active).then(|| {
                                    widget::button::standard(fl!("web-daemon-start"))
                                        .on_press(Message::WebDaemonStart)
                                }),
                            )
                            .push_maybe(
                                (self.web_daemon_status == web_daemon::Status::Active).then(|| {
                                    widget::button::standard(fl!("web-daemon-stop"))
                                        .on_press(Message::WebDaemonStop)
                                }),
                            )
                            .push(
                                widget::button::standard(fl!("web-daemon-restart"))
                                    .on_press(Message::WebDaemonRestart),
                            ),
                    ),
            )
            .add(
                widget::button::link(fl!("web-docs-link"))
                    .on_press(Message::LaunchUrl(WEB_DOCS_URL.to_owned())),
            );

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
            widget::settings::section()
                .title(fl!("settings-backup-title"))
                .add(
                    widget::settings::item::builder(fl!("settings-export"))
                        .description(fl!("settings-export-description"))
                        .control(
                            widget::button::standard(fl!("settings-export-button"))
                                .on_press(Message::ExportSettings),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("settings-import"))
                        .description(fl!("settings-import-description"))
                        .control(
                            widget::button::standard(fl!("settings-import-button"))
                                .on_press(Message::ImportSettings),
                        ),
                )
                .into(),
            cache.into(),
            global_excludes.into(),
            web.into(),
            web_tls.into(),
            web_daemon_section.into(),
            web_allowed.into(),
        ])
        .into()
    }

    /// What the sidebar's icons mean, and the terms a newcomer may not know.
    fn help_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let mut icons = widget::settings::section().title(fl!("help-icons-title"));
        for status in run_state::BackupStatus::legend() {
            icons = icons.add(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xs)
                    .align_y(Alignment::Center)
                    .padding([spacing.space_xxs, spacing.space_none])
                    .push(widget::icon::from_name(status.icon()).size(16))
                    .push(widget::text::body(status.label())),
            );
        }
        let mut terms = widget::settings::section().title(fl!("help-terms-title"));
        for (term, description) in [
            (fl!("term-repository"), fl!("term-repository-description")),
            (fl!("term-snapshot"), fl!("term-snapshot-description")),
            (fl!("term-rclone"), fl!("term-rclone-description")),
            (fl!("term-prune"), fl!("term-prune-description")),
            (fl!("term-keep"), fl!("term-keep-description")),
        ] {
            terms = terms.add(
                widget::column::with_capacity(2)
                    .spacing(spacing.space_xxxs)
                    .padding([spacing.space_xxs, spacing.space_none])
                    .push(widget::text::body(term))
                    .push(widget::text::caption(description)),
            );
        }
        widget::settings::view_column(vec![icons.into(), terms.into()]).into()
    }

    /// The profile the sidebar has selected.
    fn selected(&self) -> Option<&str> {
        match self.nav.active_data::<NavItem>() {
            Some(NavItem::Profile(id)) => Some(id.as_str()),
            _ => None,
        }
    }

    /// The home screen, rather than a particular backup, is showing.
    fn showing_home(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::Home))
    }

    /// The History page is showing.
    fn showing_history(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::History))
    }

    /// The wizard, rather than the home screen or a particular backup, is
    /// showing. A wizard can exist (`self.wizard.is_some()`) without this
    /// being true: "finish later" leaves it running but out of view.
    fn showing_wizard(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::Wizard))
    }

    /// Show the wizard: rebuilds the sidebar (so its entry exists) and
    /// selects it. Called whenever a wizard is created or resumed.
    fn select_wizard(&mut self) {
        self.rebuild_nav(None);
        let entity = self
            .nav
            .iter()
            .find(|&entity| matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Wizard)));
        if let Some(entity) = entity {
            self.nav.activate(entity);
        }
    }

    /// Leave the wizard running but move the window away from it: "finish
    /// later". Always goes to the home screen, a predictable place to
    /// return from regardless of where the wizard was opened.
    fn go_home(&mut self) {
        let entity = self
            .nav
            .iter()
            .find(|&entity| matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Home)));
        if let Some(entity) = entity {
            self.nav.activate(entity);
        }
    }

    /// Stop the wizard's own background work and forget it entirely:
    /// "discard".
    fn discard_wizard(&mut self) {
        if let Some(wizard) = &self.wizard {
            wizard.discard();
        }
        self.wizard = None;
        self.rebuild_nav(None);
    }

    /// Rebuild the sidebar from the settings, keeping the selection when the
    /// selected profile still exists.
    fn rebuild_nav(&mut self, select: Option<&str>) {
        let keep = select
            .map(str::to_owned)
            .or_else(|| self.selected().map(str::to_owned));
        // Only when nothing more specific was asked for: a rebuild while the
        // home screen or the wizard is showing (a backup finished, its
        // schedule changed) must not silently jump the window to the first
        // backup instead.
        let keep_home = select.is_none() && self.showing_home();
        let keep_history = select.is_none() && self.showing_history();
        let keep_wizard = select.is_none() && self.showing_wizard();
        self.nav.clear();
        let home = self
            .nav
            .insert()
            .text(fl!("home"))
            .icon(widget::icon::from_name("go-home-symbolic"))
            .data(NavItem::Home)
            .id();
        let history = self
            .nav
            .insert()
            .text(fl!("history-title"))
            .icon(widget::icon::from_name("emblem-documents-symbolic"))
            .data(NavItem::History)
            .id();
        let mut chosen = if keep_home {
            Some(home)
        } else if keep_history {
            Some(history)
        } else {
            None
        };
        for profile in &self.config.profiles {
            let status = self.backup_status(profile);
            let text = self.nav_row_text(profile, status);
            let id = self
                .nav
                .insert()
                .text(text)
                .icon(widget::icon::from_name(status.icon()))
                .data(NavItem::Profile(profile.id.clone()))
                .id();
            if keep.as_deref() == Some(profile.id.as_str())
                || (chosen.is_none() && !keep_home && !keep_history && !keep_wizard)
            {
                chosen = Some(id);
            }
        }
        if !self.config.profiles.is_empty() {
            // A wizard already in progress is never replaced by a fresh
            // "New backup": there is only ever one at a time, and starting
            // another would silently lose it.
            let (text, icon, item) = if self.wizard.is_some() {
                (
                    fl!("wizard-resume"),
                    "document-edit-symbolic",
                    NavItem::Wizard,
                )
            } else {
                (fl!("new-backup"), "list-add-symbolic", NavItem::New)
            };
            let id = self
                .nav
                .insert()
                .text(text)
                .icon(widget::icon::from_name(icon))
                .data(item)
                .divider_above(true)
                .id();
            if keep_wizard {
                chosen = Some(id);
            }
        }
        if let Some(id) = chosen {
            self.nav.activate(id);
        }
    }

    /// Update one profile's own sidebar row in place — its text (the name,
    /// or a running backup's progress) and icon — without touching any
    /// other row's entity ID, the sidebar's selection, or keyboard focus in
    /// it. `rebuild_nav` clears and reinserts every row, including a fresh
    /// entity ID for each one, which is fine for the profile list or the
    /// wizard's own presence actually changing, but was until now the only
    /// way this page had to reflect anything at all — including a password
    /// keystroke or a progress event arriving every 250ms during a backup,
    /// which dropped whatever had focus in the sidebar each time.
    fn refresh_nav_row(&mut self, id: &str) {
        let Some(profile) = self.config.profile(id) else {
            return;
        };
        let status = self.backup_status(profile);
        let text = self.nav_row_text(profile, status);
        let icon = widget::icon::from_name(status.icon());
        let entity = self.nav.iter().find(|&entity| {
            matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Profile(p)) if p == id)
        });
        if let Some(entity) = entity {
            self.nav.text_set(entity, text);
            self.nav.icon_set(entity, icon.into());
        }
    }

    /// `profile`'s state for the sidebar: its run facts, and whether the
    /// window has work running for it right now.
    fn backup_status(&self, profile: &Profile) -> run_state::BackupStatus {
        let run = self.runs.get(&profile.id).cloned().unwrap_or_default();
        let running = self
            .pages
            .get(&profile.id)
            .is_some_and(profile::ProfileState::is_busy);
        run_state::status(profile, &run, running)
    }

    /// The sidebar row's text: just the name, unless a backup is running
    /// right now, when the nav row's only way to show progress is its text.
    fn nav_row_text(&self, profile: &Profile, status: run_state::BackupStatus) -> String {
        if status != run_state::BackupStatus::Running {
            return profile.name.clone();
        }
        match self
            .pages
            .get(&profile.id)
            .and_then(profile::ProfileState::progress_fraction)
        {
            Some(fraction) => fl!(
                "nav-running-percent",
                name = profile.name.clone(),
                percent = ((fraction * 100.0).round() as i64)
            ),
            None => fl!("nav-running", name = profile.name.clone()),
        }
    }

    /// Re-read every backup's run state; rebuild the sidebar if a warning
    /// appeared or went away.
    fn reload_runs(&mut self) {
        let runs: HashMap<String, RunState> = self
            .config
            .profiles
            .iter()
            .map(|profile| (profile.id.clone(), run_state::load(&profile.id)))
            .collect();
        if runs != self.runs {
            self.runs = runs;
            self.rebuild_nav(None);
        }
    }

    /// Change one backup's run state, and show the change.
    fn record_run(&mut self, id: &str, change: impl FnOnce(&mut RunState)) {
        if let Err(err) = run_state::update(id, change) {
            error_log!(CONFIG, "could not record a run of {id}: {err}");
        }
        self.reload_runs();
    }

    /// Install, update or remove `profile`'s timer, off the UI thread.
    fn apply_schedule(profile: Profile) -> Task<Message> {
        Task::perform(
            tasks::blocking(move || {
                schedule::apply(&profile)
                    .map_err(|err| EngineError::new(engine::ErrorKind::Internal, err))
            }),
            |result| match result {
                Ok(()) => app(Message::Noop),
                Err(err) => app(Message::ScheduleFailed(err.detail)),
            },
        )
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
        self.runs.remove(id);
        self.rebuild_nav(None);
        let id = id.to_owned();
        let unschedule = {
            let id = id.clone();
            Task::perform(
                tasks::blocking(move || {
                    let _ = run_state::save(&id, &RunState::default());
                    schedule::remove(&id)
                        .map_err(|err| EngineError::new(engine::ErrorKind::Internal, err))
                }),
                |result| match result {
                    Ok(()) => app(Message::Noop),
                    Err(err) => app(Message::ScheduleFailed(err.detail)),
                },
            )
        };
        let forget = Task::perform(async move { crate::keyring::forget(&id).await }, |_| {
            app(Message::Noop)
        });
        Task::batch([unschedule, forget, self.activate_selected()])
    }

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

    /// Change the web interface's settings and save them, in one step: every
    /// field lives in one `WebConfig`, so `set_web` is the only setter
    /// `CosmicConfigEntry` generates for any part of it.
    fn update_web(&mut self, mutate: impl FnOnce(&mut config::WebConfig)) {
        let mut web = self.config.web.clone();
        mutate(&mut web);
        if let Some(handler) = &self.config_handler
            && let Err(err) = self.config.set_web(handler, web)
        {
            error_log!(CONFIG, "failed to save the web interface settings: {err}");
        }
    }

    /// Run a `web_daemon` systemd call (blocking: it shells out to
    /// `systemctl`) off the UI thread, reporting what happened through
    /// [`Message::WebDaemonActed`].
    fn web_daemon_task(&self, call: fn() -> Result<(), String>) -> Task<Message> {
        Task::perform(
            async move {
                tokio::task::spawn_blocking(call)
                    .await
                    .unwrap_or_else(|err| Err(err.to_string()))
            },
            |result| app(Message::WebDaemonActed(result)),
        )
    }

    /// Ask systemd how the daemon is doing, off the UI thread (it is a D-Bus
    /// round trip, not truly blocking, but still not something `view` should
    /// wait on).
    fn web_daemon_status_task() -> Task<Message> {
        Task::perform(web_daemon::status(), |status| {
            app(Message::WebDaemonStatus(status))
        })
    }

    /// Auth, the allow-list and the network scope are all read once, at
    /// startup — a change to any of them (a regenerated token, a removed
    /// allow-list entry, a disabled password) otherwise has no effect until
    /// the daemon is restarted by hand, which the config doc comment
    /// wrongly claimed happened "immediately". Restarting here closes that
    /// gap for whichever of them just changed, without disturbing anything
    /// if the daemon is not even running (`self.web_daemon_status` is a
    /// cached last-known status, not a fresh D-Bus round trip, so this
    /// restarts based on what Settings was last told, not a guaranteed
    /// truth — the same staleness the status indicator itself already has).
    /// A backup being added, edited or removed still does not reach the
    /// daemon this way; that is a larger, separate gap (see WEB-1's own
    /// status note).
    fn restart_web_daemon_if_active(&self) -> Task<Message> {
        if self.web_daemon_status == web_daemon::Status::Active {
            self.web_daemon_task(web_daemon::restart)
        } else {
            Task::none()
        }
    }

    /// Something on screen shows a running time.
    fn waiting(&self) -> bool {
        self.wizard.as_ref().is_some_and(Wizard::waiting)
            || self.pages.values().any(ProfileState::is_busy)
    }

    fn show_error(&mut self, context: &str, error: &EngineError) {
        self.dialog = Some(Dialog::Error(errors::describe(context, error)));
    }

    fn start_wizard(&mut self, wizard: Wizard, effects: Vec<wizard::Effect>) -> Task<Message> {
        self.wizard = Some(wizard);
        self.select_wizard();
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
                wizard::Effect::Browse(effect) => self.run_browse_effect(effect),
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
                wizard::Effect::Finish(finish) => {
                    let remember_task = match (finish.remember, &finish.secret) {
                        (true, Some(secret)) => {
                            let id = finish.profile.id.clone();
                            let name = finish.profile.name.clone();
                            let secret = secret.clone();
                            Task::perform(
                                async move { crate::keyring::store(&id, &name, &secret).await },
                                |result| {
                                    app(Message::Dialog(DialogMessage::PasswordNotRemembered(
                                        result.err(),
                                    )))
                                },
                            )
                        }
                        _ => Task::none(),
                    };
                    Task::batch([
                        remember_task,
                        Task::perform(
                            tasks::finish(finish.mode, finish.profile, finish.secret),
                            |result| app(Message::WizardFinished(Box::new(result))),
                        ),
                    ])
                }
                wizard::Effect::Close => {
                    self.wizard = None;
                    Task::none()
                }
                wizard::Effect::ConfirmCancel => {
                    self.dialog = Some(Dialog::WizardCancel);
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
            place::Effect::SignIn { name, credentials } => {
                // Easy to miss as a line of page text below a button that
                // just disappeared: a modal makes "go to your browser now"
                // impossible to scroll past. Cleared again once sign-in
                // finishes, successfully or not; the page itself already
                // shows that outcome.
                self.dialog = Some(Dialog::Info(
                    fl!("place-signing-in-title"),
                    fl!("place-signing-in-body"),
                ));
                Task::perform(
                    tasks::blocking(move || {
                        let config = engine::rclone::config_path();
                        let params = ["scope=drive"];
                        let credentials = credentials
                            .as_ref()
                            .map(|(id, secret)| (id.as_str(), secret.as_str()));
                        engine::rclone::sign_in(&config, &name, "drive", &params, credentials)
                            .map(|()| name)
                    }),
                    move |result| to_wizard(place::Message::SignedIn(result)),
                )
            }
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

    fn run_browse_effect(&mut self, effect: wizard::browse::Effect) -> Task<Message> {
        match effect {
            wizard::browse::Effect::List(dir) => Task::run(
                tasks::browse_folder(
                    dir,
                    std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                ),
                |message| app(Message::Wizard(wizard::Message::Browse(message))),
            ),
            // Applied directly in `Wizard::update`, not here: see its own
            // comment for why.
            wizard::browse::Effect::SetExcluded(..) => Task::none(),
        }
    }

    fn run_restore_effects(&mut self, effects: Vec<restore::Effect>) -> Task<Message> {
        let Some((page, secret)) = &self.restore else {
            return Task::none();
        };
        let Some(profile) = self.config.profile(&page.profile_id).cloned() else {
            return Task::none();
        };
        let secret = secret.clone();
        let browser = page.browser();
        let to_page = |message: restore::Message| app(Message::RestorePage(message));
        let mut tasks = Vec::new();
        for effect in effects {
            let browser = browser.clone();
            let task = match effect {
                restore::Effect::Load => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    Task::perform(
                        tasks::blocking(move || {
                            let location = profile.location()?;
                            Ok(std::sync::Arc::new(
                                engine::open(&location, &secret)?.browse()?,
                            ))
                        }),
                        move |result| to_page(restore::Message::Loaded(result)),
                    )
                }
                restore::Effect::List { snapshot, dir } => {
                    let listed = dir.clone();
                    Task::perform(
                        tasks::blocking(move || browsing(browser)?.list(&snapshot, &dir)),
                        move |result| to_page(restore::Message::Listed(listed.clone(), result)),
                    )
                }
                restore::Effect::Search { snapshot, query } => Task::perform(
                    tasks::blocking(move || {
                        browsing(browser)?.search(&snapshot, &query, restore::RESULT_LIMIT)
                    }),
                    move |result| to_page(restore::Message::Found(result)),
                ),
                restore::Effect::Versions(path) => {
                    let asked = path.clone();
                    Task::perform(
                        tasks::blocking(move || browsing(browser)?.versions(&path)),
                        move |result| {
                            to_page(restore::Message::VersionsLoaded(asked.clone(), result))
                        },
                    )
                }
                restore::Effect::Missing { scope, since } => Task::perform(
                    tasks::blocking(move || {
                        browsing(browser)?.missing(&scope, since, restore::RESULT_LIMIT)
                    }),
                    move |result| to_page(restore::Message::MissingFound(result)),
                ),
                restore::Effect::Diff { from, to } => Task::perform(
                    tasks::blocking(move || browsing(browser)?.diff(&from, &to)),
                    move |result| to_page(restore::Message::Compared(result)),
                ),
                restore::Effect::GlobalSearch { query } => Task::perform(
                    tasks::blocking(move || {
                        browsing(browser)?.search_all(&query, restore::RESULT_LIMIT)
                    }),
                    move |result| to_page(restore::Message::GlobalFound(result)),
                ),
                restore::Effect::Preview(requests) => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    let asked = requests.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            let location = profile.location()?;
                            let mut total = engine::RestorePreview::default();
                            for request in &requests {
                                let part =
                                    engine::open(&location, &secret)?.preview_restore(request)?;
                                total.files += part.files;
                                total.bytes += part.bytes;
                                total.unchanged += part.unchanged;
                                total.conflicts += part.conflicts;
                            }
                            Ok(total)
                        }),
                        move |result| to_page(restore::Message::Previewed(asked.clone(), result)),
                    )
                }
                restore::Effect::Restore(request) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            self.show_error(&fl!("restore-failed"), &err);
                            continue;
                        }
                    };
                    let job = Job {
                        restore: Some(request),
                        ..Job::new(repository, secret.clone())
                    };
                    Task::run(child::run(Operation::Restore, job), move |event| {
                        to_page(restore::Message::Restore(event))
                    })
                }
                restore::Effect::OpenCopy { snapshot, path } => {
                    let (profile, secret) = (profile.clone(), secret.clone());
                    Task::perform(
                        tasks::blocking(move || open_copy(&profile, &secret, &snapshot, &path)),
                        |result| match result {
                            Ok(()) => app(Message::Noop),
                            Err(err) => app(Message::Dialog(DialogMessage::Failed(
                                fl!("open-copy-failed"),
                                err,
                            ))),
                        },
                    )
                }
                restore::Effect::Download {
                    snapshot,
                    path,
                    is_folder,
                } => {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let file_name = if is_folder {
                        format!("{name}.tar.gz")
                    } else {
                        name.into_owned()
                    };
                    Task::perform(
                        async move {
                            let destination =
                                tasks::choose_save_path(fl!("download-title"), file_name).await?;
                            Some(
                                tasks::blocking(move || {
                                    let browser = browsing(browser)?;
                                    if is_folder {
                                        browser.archive_folder(&snapshot, &path, &destination)
                                    } else {
                                        browser.dump_file(&snapshot, &path, &destination)
                                    }
                                })
                                .await,
                            )
                        },
                        |result| match result {
                            None | Some(Ok(())) => app(Message::Noop),
                            Some(Err(err)) => app(Message::Dialog(DialogMessage::Failed(
                                fl!("download-failed"),
                                err,
                            ))),
                        },
                    )
                }
                restore::Effect::PickScope => Task::perform(
                    tasks::pick_folder(fl!("select-scope-folder")),
                    move |path| {
                        path.map_or(app(Message::Noop), |path| {
                            to_page(restore::Message::ScopeChosen(path))
                        })
                    },
                ),
                restore::Effect::PickTarget => Task::perform(
                    tasks::pick_folder(fl!("select-restore-folder")),
                    move |path| {
                        path.map_or(to_page(restore::Message::TargetOriginal), |path| {
                            to_page(restore::Message::TargetChosen(path))
                        })
                    },
                ),
                restore::Effect::PickMountPoint => Task::perform(
                    tasks::pick_folder(fl!("select-mount-folder")),
                    move |path| match path {
                        Some(point) => to_page(restore::Message::MountPointChosen(point)),
                        None => app(Message::Noop),
                    },
                ),
                restore::Effect::Mount { snapshot, point } => {
                    let profile_id = profile.id.clone();
                    let logged_snapshot = snapshot.clone();
                    Task::perform(
                        tasks::blocking(move || {
                            let browser = browsing(browser)?;
                            Ok(engine::mount::mount(browser, snapshot, &point)?.into())
                        }),
                        move |result: Result<restore::MountHandle, EngineError>| {
                            if result.is_ok() {
                                event_log::record(
                                    &profile_id,
                                    format::now(),
                                    event_log::EventKind::Mounted {
                                        snapshot: logged_snapshot.clone(),
                                    },
                                    event_log::Source::Desktop,
                                );
                            }
                            to_page(restore::Message::Mounted(result))
                        },
                    )
                }
                restore::Effect::OpenMounted(path) => {
                    if let Err(err) = open::that_detached(&path) {
                        error_log!(UI, "failed to open mounted folder {path:?}: {err}");
                    }
                    Task::none()
                }
                restore::Effect::Unmount(handle) => {
                    let profile_id = profile.id.clone();
                    let snapshot = handle.snapshot().to_owned();
                    Task::perform(
                        tasks::blocking(move || {
                            drop(handle);
                            Ok(())
                        }),
                        move |_: Result<(), EngineError>| {
                            event_log::record(
                                &profile_id,
                                format::now(),
                                event_log::EventKind::Unmounted {
                                    snapshot: snapshot.clone(),
                                },
                                event_log::Source::Desktop,
                            );
                            app(Message::Noop)
                        },
                    )
                }
                restore::Effect::ShowError(context, error) => {
                    self.show_error(&context, &error);
                    Task::none()
                }
                restore::Effect::Restored(done) => {
                    event_log::record(
                        &profile.id,
                        format::now(),
                        event_log::EventKind::Restored {
                            files: done.files,
                            bytes: done.bytes,
                        },
                        event_log::Source::Desktop,
                    );
                    self.dialog = Some(Dialog::Info(
                        fl!("restore-done-title"),
                        fl!(
                            "restore-done-body",
                            count = (done.files as i64),
                            size = format::bytes(done.bytes),
                            conflicts = (done.conflicts as i64)
                        ),
                    ));
                    Task::none()
                }
                restore::Effect::Close => {
                    self.restore = None;
                    Task::none()
                }
            };
            tasks.push(task);
        }
        Task::batch(tasks)
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
                    let profile = profile.clone();
                    Task::perform(async move { profile.password().await }, move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::KeyringLoaded(result),
                        ))
                    })
                }
                profile::Effect::Open { secret, remember } => {
                    let used = secret.clone();
                    let remember_task = if remember {
                        let profile_id = profile.id.clone();
                        let name = profile.name.clone();
                        let secret = secret.clone();
                        Task::perform(
                            async move { crate::keyring::store(&profile_id, &name, &secret).await },
                            |result| {
                                app(Message::Dialog(DialogMessage::PasswordNotRemembered(
                                    result.err(),
                                )))
                            },
                        )
                    } else {
                        Task::none()
                    };
                    Task::batch([
                        remember_task,
                        Task::perform(tasks::open(profile.clone(), secret), move |result| {
                            app(Message::Profile(
                                id.clone(),
                                profile::Message::Opened(used.clone(), result),
                            ))
                        }),
                    ])
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
                        request: Some(profile.backup_request(&self.config.global_exclude_patterns)),
                        hooks: profile.hooks.clone(),
                        ..Job::new(repository, secret)
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
                        ids,
                        ..Job::new(repository, secret)
                    };
                    Task::run(child::run(Operation::DeleteSnapshots, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::SnapshotsDeleted(event),
                        ))
                    })
                }
                profile::Effect::ConfirmDeleteSnapshot {
                    id: snapshot,
                    label,
                } => {
                    self.dialog = Some(Dialog::DeleteSnapshot {
                        id: id.clone(),
                        snapshot,
                        label,
                    });
                    Task::none()
                }
                profile::Effect::SetPinned(secret, snapshot_id, pinned) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            self.show_error(&fl!("pin-snapshot-failed"), &err);
                            continue;
                        }
                    };
                    let job = Job {
                        ids: vec![snapshot_id],
                        pinned: Some(pinned),
                        ..Job::new(repository, secret)
                    };
                    Task::run(child::run(Operation::SetPinned, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Pinned(event),
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
                profile::Effect::OpenRestore(secret) => {
                    let root = profile
                        .sources
                        .first()
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from("/"));
                    let (page, effects) = RestorePage::new(profile.id.clone(), root);
                    self.restore = Some((page, secret));
                    self.run_restore_effects(effects)
                }
                profile::Effect::Edit => {
                    if self.wizard.is_none() {
                        let (wizard, effects) = Wizard::edit(&profile);
                        self.start_wizard(wizard, effects)
                    } else {
                        // A different wizard is already in progress
                        // ("finish later" from elsewhere): resume that one
                        // rather than losing it to this edit.
                        self.select_wizard();
                        Task::none()
                    }
                }
                profile::Effect::EditSchedule => {
                    if self.wizard.is_none() {
                        let (wizard, effects) = Wizard::schedule(&profile);
                        self.start_wizard(wizard, effects)
                    } else {
                        self.select_wizard();
                        Task::none()
                    }
                }
                profile::Effect::EditHooks => {
                    if self.wizard.is_none() {
                        let (wizard, effects) = Wizard::hooks(&profile);
                        self.start_wizard(wizard, effects)
                    } else {
                        self.select_wizard();
                        Task::none()
                    }
                }
                profile::Effect::EditPasswordCommand => {
                    self.dialog = Some(Dialog::PasswordCommand {
                        id: id.clone(),
                        text: profile.password_command.clone(),
                    });
                    Task::none()
                }
                profile::Effect::ChangePassword => {
                    self.dialog = Some(Dialog::ChangePassword {
                        id: id.clone(),
                        password: String::new(),
                        confirm: String::new(),
                        busy: false,
                    });
                    Task::none()
                }
                profile::Effect::LogEvent(kind) => {
                    event_log::record(&id, format::now(), kind, event_log::Source::Desktop);
                    Task::none()
                }
                profile::Effect::FetchHistory => {
                    Task::perform(tasks::history(id.clone()), move |history| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::HistoryLoaded(history),
                        ))
                    })
                }
                profile::Effect::FetchStatistics(secret) => {
                    Task::perform(tasks::statistics(profile.clone(), secret), move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::StatisticsLoaded(result),
                        ))
                    })
                }
                profile::Effect::EstimateSize(cancel) => {
                    let request = profile.backup_request(&self.config.global_exclude_patterns);
                    Task::run(tasks::estimate_size(request, cancel), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Estimate(event),
                        ))
                    })
                }
                profile::Effect::FetchNextRun => {
                    let profile_id = id.clone();
                    Task::perform(
                        async move { schedule::next_run(&profile_id).await },
                        move |next_run| {
                            app(Message::Profile(
                                id.clone(),
                                profile::Message::NextRunLoaded(next_run),
                            ))
                        },
                    )
                }
                profile::Effect::Check(secret) => {
                    let job = match profile.location() {
                        Ok(location) => Job::new(location, secret),
                        Err(err) => {
                            let ended = profile::Message::Checked(child::ChildEvent::Ended(err));
                            let effects = self
                                .pages
                                .entry(id.clone())
                                .or_default()
                                .update(ended, &profile);
                            tasks.push(self.run_profile_effects(&id, effects));
                            continue;
                        }
                    };
                    Task::run(child::run(Operation::Check, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Checked(event),
                        ))
                    })
                }
                profile::Effect::RecordCheck(result) => {
                    // A check that could not run (an unplugged drive, a
                    // cancel) says nothing about damage and is not recorded.
                    let damaged = match &result {
                        Ok(()) => false,
                        Err(err) if err.kind == engine::ErrorKind::RepositoryDamaged => true,
                        Err(err) => {
                            self.show_error(&fl!("check-failed"), err);
                            continue;
                        }
                    };
                    let now = format::now();
                    self.record_run(&id, |run| {
                        run.last_check = Some(now);
                        run.damaged = damaged;
                        if !damaged
                            && run
                                .failure
                                .as_ref()
                                .is_some_and(|f| f.stage == run_state::Stage::Check)
                        {
                            run.failure = None;
                        }
                    });
                    match result {
                        Ok(()) => {
                            self.dialog = Some(Dialog::Info(
                                fl!("check-passed-title"),
                                fl!("check-passed-body"),
                            ));
                        }
                        Err(err) => self.show_error(&fl!("check-failed"), &err),
                    }
                    Task::none()
                }
                profile::Effect::CleanUp(secret) => {
                    let job = match profile.location() {
                        Ok(location) => Job {
                            keep: profile.retention.keep_rules(),
                            prune: !self.runs.get(&id).is_some_and(|run| run.damaged),
                            profile_tag: engine::profile_tag(&profile.id),
                            profile_sources: profile.sources.clone(),
                            ..Job::new(location, secret)
                        },
                        Err(err) => {
                            let ended = profile::Message::CleanedUp(child::ChildEvent::Ended(err));
                            let effects = self
                                .pages
                                .entry(id.clone())
                                .or_default()
                                .update(ended, &profile);
                            tasks.push(self.run_profile_effects(&id, effects));
                            continue;
                        }
                    };
                    Task::run(child::run(Operation::Maintain, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::CleanedUp(event),
                        ))
                    })
                }
                profile::Effect::CleanedUp { forgotten, freed } => {
                    self.record_run(&id, |run| {
                        if run
                            .failure
                            .as_ref()
                            .is_some_and(|f| f.stage == run_state::Stage::Cleanup)
                        {
                            run.failure = None;
                        }
                        run.total_freed += freed;
                    });
                    self.dialog = Some(Dialog::Info(
                        fl!("clean-up-done-title"),
                        fl!(
                            "clean-up-done-body",
                            count = (forgotten as i64),
                            size = format::bytes(freed)
                        ),
                    ));
                    Task::none()
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
        if let Mode::Edit { .. } | Mode::Schedule { .. } = finished.mode {
            // The wizard started from the saved profile and changed only its
            // own fields; a backup may have finished since it opened.
            if let Some(existing) = self.config.profile(&profile.id) {
                profile.last_success = existing.last_success;
            }
        }
        let id = profile.id.clone();
        self.upsert_profile(profile.clone());
        self.rebuild_nav(Some(&id));
        let scheduled = Self::apply_schedule(profile.clone());

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
        Task::batch([close, scheduled, self.run_profile_effects(&id, effects)])
    }

    fn on_dialog(&mut self, message: DialogMessage) -> Task<Message> {
        match message {
            DialogMessage::Close => {
                self.dialog = None;
                Task::none()
            }
            DialogMessage::FinishWizardLater => {
                self.dialog = None;
                self.go_home();
                Task::none()
            }
            DialogMessage::DiscardWizard => {
                self.dialog = None;
                self.discard_wizard();
                Task::none()
            }
            DialogMessage::Typed(text) => {
                match &mut self.dialog {
                    Some(Dialog::DeleteAll { typed, .. }) => *typed = text,
                    Some(Dialog::PasswordCommand { text: field, .. }) => *field = text,
                    _ => {}
                }
                Task::none()
            }
            DialogMessage::NewPassword(text) => {
                if let Some(Dialog::ChangePassword { password, .. }) = &mut self.dialog {
                    *password = text;
                }
                Task::none()
            }
            DialogMessage::ConfirmPassword(text) => {
                if let Some(Dialog::ChangePassword { confirm, .. }) = &mut self.dialog {
                    *confirm = text;
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
                    Dialog::Error(_) | Dialog::Info(..) | Dialog::Token(_) => {
                        self.dialog = None;
                        Task::none()
                    }
                    Dialog::RegenerateToken => {
                        let token = crate::web_token::generate();
                        self.update_web(|web| web.token_hash = Some(token.hash));
                        self.dialog = Some(Dialog::Token(token.raw));
                        self.restart_web_daemon_if_active()
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
                    // Its own two buttons send `FinishWizardLater` and
                    // `DiscardWizard` directly; `Confirm` never legitimately
                    // reaches it. Dismiss rather than do nothing silently.
                    Dialog::WizardCancel => {
                        self.dialog = None;
                        Task::none()
                    }
                    Dialog::Quit => cosmic::iced::exit(),
                    Dialog::DeleteSnapshot { id, snapshot, .. } => {
                        self.dialog = None;
                        let effects = self
                            .pages
                            .get_mut(&id)
                            .map(|page| page.delete_snapshot_confirmed(snapshot))
                            .unwrap_or_default();
                        self.run_profile_effects(&id, effects)
                    }
                    Dialog::PasswordCommand { id, text } => {
                        self.dialog = None;
                        if let Some(mut profile) = self.config.profile(&id).cloned() {
                            profile.password_command = text.trim().to_owned();
                            self.upsert_profile(profile);
                        }
                        Task::none()
                    }
                    Dialog::ChangePassword {
                        id,
                        password,
                        confirm,
                        ..
                    } => {
                        let (Some(profile), Some(secret)) = (
                            self.config.profile(&id).cloned(),
                            self.pages.get(&id).and_then(|page| page.secret().cloned()),
                        ) else {
                            return Task::none();
                        };
                        let repository = match profile.location() {
                            Ok(location) => location,
                            Err(err) => {
                                self.dialog = None;
                                self.show_error(&fl!("change-password-failed"), &err);
                                return Task::none();
                            }
                        };
                        self.dialog = Some(Dialog::ChangePassword {
                            id: id.clone(),
                            password,
                            confirm: confirm.clone(),
                            busy: true,
                        });
                        // Through the same `--run` child every other write
                        // goes through, not called in-process: it needs the
                        // cross-process write lock too, so it cannot race a
                        // scheduled backup also touching the repository's
                        // keys, and it needs the child's own diagnostic
                        // logging (see `runner::main`), which an in-process
                        // call never reached.
                        let job = Job {
                            new_password: Some(engine::Secret::new(confirm)),
                            ..Job::new(repository, secret)
                        };
                        Task::run(child::run(Operation::ChangePassword, job), move |event| {
                            app(Message::Dialog(DialogMessage::PasswordChanged(
                                id.clone(),
                                event,
                            )))
                        })
                    }
                }
            }
            DialogMessage::Failed(context, error) => {
                self.show_error(&context, &error);
                Task::none()
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
            DialogMessage::PasswordChanged(id, event) => match event {
                child::ChildEvent::Event(RunnerEvent::Done { .. }) => {
                    let Some(Dialog::ChangePassword { confirm, .. }) = self.dialog.take() else {
                        return Task::none();
                    };
                    let new_password = engine::Secret::new(confirm);
                    if let Some(page) = self.pages.get_mut(&id) {
                        page.set_secret(new_password.clone());
                    }
                    debug_log!(ENGINE, "changed the password of profile {id}");
                    event_log::record(
                        &id,
                        format::now(),
                        event_log::EventKind::PasswordChanged,
                        event_log::Source::Desktop,
                    );
                    let Some(profile) = self.config.profile(&id).cloned() else {
                        return Task::none();
                    };
                    // The repository itself is already changed at this
                    // point; a keyring failure here does not undo that, so
                    // it is reported separately rather than as this whole
                    // operation having failed.
                    Task::perform(
                        async move {
                            if crate::keyring::load(&profile.id).await.is_some() {
                                crate::keyring::store(&profile.id, &profile.name, &new_password)
                                    .await
                            } else {
                                Ok(())
                            }
                        },
                        |result| {
                            app(Message::Dialog(DialogMessage::KeyringUpdateFailed(
                                result.err(),
                            )))
                        },
                    )
                }
                child::ChildEvent::Event(RunnerEvent::Error { error })
                | child::ChildEvent::Ended(error) => {
                    self.dialog = None;
                    self.show_error(&fl!("change-password-failed"), &error);
                    Task::none()
                }
                child::ChildEvent::Started(_)
                | child::ChildEvent::Event(RunnerEvent::Progress { .. }) => Task::none(),
            },
            DialogMessage::KeyringUpdateFailed(None) => Task::none(),
            DialogMessage::KeyringUpdateFailed(Some(detail)) => {
                self.show_error(
                    &fl!("change-password-title"),
                    &EngineError::new(engine::ErrorKind::KeyringUnavailable, detail),
                );
                Task::none()
            }
            DialogMessage::PasswordNotRemembered(None) => Task::none(),
            DialogMessage::PasswordNotRemembered(Some(detail)) => {
                self.dialog = Some(Dialog::Error(fl!(
                    "error-password-not-saved",
                    details = detail
                )));
                Task::none()
            }
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

/// The browser the restore page opened, or an error if it has not finished
/// opening.
fn browsing(
    browser: Option<std::sync::Arc<engine::Browser>>,
) -> Result<std::sync::Arc<engine::Browser>, EngineError> {
    browser
        .ok_or_else(|| EngineError::new(engine::ErrorKind::Internal, "the backup is still opening"))
}

/// Restore one file from `snapshot` into a private temporary folder, make it
/// read-only, and open it with the default application: a way to look at an
/// old version without touching the current one.
fn open_copy(
    profile: &Profile,
    secret: &engine::Secret,
    snapshot: &str,
    path: &std::path::Path,
) -> Result<(), EngineError> {
    use std::os::unix::fs::PermissionsExt;
    let folder =
        engine::lock::runtime_dir().join(format!("open-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&folder)?;
    std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
    let request = engine::RestoreRequest {
        snapshot: snapshot.to_owned(),
        paths: vec![path.to_path_buf()],
        target: engine::Target::Folder(folder.clone()),
        policy: engine::ConflictPolicy::Overwrite,
        ..engine::RestoreRequest::default()
    };
    engine::open(&profile.location()?, secret)?
        .restore(&request, std::sync::Arc::new(engine::NoProgress))?;
    let copy = folder.join(path.file_name().unwrap_or_default());
    // A snapshot's node can claim to be a symlink (see SEC-2 in the review
    // plan): for a repository shared with someone else, that is not
    // necessarily this process's own doing. `symlink_metadata` (unlike
    // `metadata`) does not follow it, so this refuses to chmod or open
    // whatever it points at — `~/.ssh`, say — instead of trusting that
    // "restored into a private folder this process just created" also means
    // "definitely a plain file".
    if !std::fs::symlink_metadata(&copy)?.is_file() {
        return Err(EngineError::new(
            engine::ErrorKind::Internal,
            format!("{} did not restore as a plain file", copy.display()),
        ));
    }
    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o400))?;
    open::that_detached(&copy)?;
    Ok(())
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
    env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Documentation for the web interface, its API and its daemon, linked from
/// Settings rather than duplicated there.
const WEB_DOCS_URL: &str = concat!(
    env!("CARGO_PKG_REPOSITORY"),
    "/blob/main/docs/web-interface.md"
);

/// A valid port number typed into the web interface's port field: not empty,
/// not out of `u16` range, and not `0` (not a real port to listen on, even
/// though `0_u16` parses fine on its own).
fn parse_port(text: &str) -> Option<u16> {
    text.trim().parse::<u16>().ok().filter(|&port| port != 0)
}

/// Whether `password` meets Settings' own minimum for the web interface's
/// shared password (OWASP ASVS 5.0 §6.2). Counted in characters, not bytes,
/// so a password using non-ASCII characters is not penalized for it.
fn password_long_enough(password: &str) -> bool {
    password.chars().count() >= crate::constants::WEB_PASSWORD_MIN_LENGTH
}

/// Where the web interface will be reachable at `scope`, or `None` when it
/// is off. Always `https`: the daemon serves TLS unconditionally, a
/// self-signed certificate by default. `Lan` uses this machine's mDNS name
/// (`.local`, resolved by `avahi`/`systemd-resolved` on the same network)
/// rather than an actual IP address, since a machine can have several and the
/// address alone would not say which one to use.
fn web_address(scope: NetworkScope, port: u16) -> Option<String> {
    match scope {
        NetworkScope::Off => None,
        NetworkScope::Localhost => Some(format!("https://127.0.0.1:{port}")),
        NetworkScope::Lan => Some(format!(
            "https://{}.local:{port}",
            gethostname::gethostname().to_string_lossy(),
        )),
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
    /// [`get_app_settings`]: crate::app::settings::get_app_settings
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
        let raise = match self.core.main_window_id() {
            Some(id) => Task::batch([window::gain_focus(id), window::minimize(id, false)]),
            None => {
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
            }
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
            app_themes: vec![fl!("match-desktop"), fl!("dark"), fl!("light")],
            context_page: ContextPage::Settings,
            config_handler: flags.config_handler,
            config: flags.config,
            dialog: None,
            pages: HashMap::new(),
            wizard: None,
            restore: None,
            key_binds: key_binds(),
            modifiers: Modifiers::empty(),
            now: format::now(),
            dejadup: crate::dejadup::find().is_some(),
            runs: HashMap::new(),
            global_exclude_pattern_input: String::new(),
            history: None,
            web_password_input: String::new(),
            web_allowed_address_input: String::new(),
            web_port_input: String::new(),
            web_password_status: None,
            web_daemon_status: web_daemon::Status::default(),
        };
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
        let activate = app.activate_selected();
        // Timers follow the settings, which may have changed while the
        // window was closed, or the program may have moved.
        let profiles = app.config.profiles.clone();
        let reconcile = Task::perform(
            tasks::blocking(move || Ok(schedule::reconcile(&profiles))),
            |errors: Result<Vec<String>, EngineError>| match errors
                .ok()
                .and_then(|e| e.into_iter().next())
            {
                Some(err) => cosmic::Action::App(Message::ScheduleFailed(err)),
                None => cosmic::Action::App(Message::Noop),
            },
        );
        let wizard = if flags_start_wizard {
            app.update(Message::NewBackup)
        } else {
            Task::none()
        };
        debug_log!(UI, "started with {} profiles", app.config.profiles.len());
        (app, Task::batch([title_task, activate, reconcile, wizard]))
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
            ContextPage::Help => cosmic::app::context_drawer::context_drawer(
                self.help_view(),
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
            Dialog::Info(title, message) => widget::dialog()
                .title(title.as_str())
                .body(message.as_str())
                .primary_action(
                    widget::button::suggested(fl!("ok"))
                        .on_press(Message::Dialog(DialogMessage::Close)),
                ),
            Dialog::Token(token) => widget::dialog()
                .title(fl!("web-token-title"))
                .body(fl!("web-token-body"))
                .control(
                    widget::row::with_capacity(2)
                        .spacing(theme::active().cosmic().spacing.space_xs)
                        .push(widget::text_input("", token.as_str()).width(Length::Fill))
                        .push(
                            widget::button::standard(fl!("web-token-copy"))
                                .on_press(Message::CopyToClipboard(token.clone())),
                        ),
                )
                .primary_action(
                    widget::button::suggested(fl!("ok"))
                        .on_press(Message::Dialog(DialogMessage::Close)),
                ),
            Dialog::RegenerateToken => widget::dialog()
                .title(fl!("web-token-regenerate-title"))
                .body(fl!("web-token-regenerate-body"))
                .primary_action(
                    widget::button::destructive(fl!("web-token-regenerate-confirm"))
                        .on_press_maybe(confirm),
                )
                .secondary_action(cancel),
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
            Dialog::WizardCancel => widget::dialog()
                .title(fl!("wizard-cancel-title"))
                .body(fl!("wizard-cancel-body"))
                .primary_action(
                    widget::button::suggested(fl!("wizard-finish-later"))
                        .on_press(Message::Dialog(DialogMessage::FinishWizardLater)),
                )
                .secondary_action(
                    widget::button::destructive(fl!("wizard-discard"))
                        .on_press(Message::Dialog(DialogMessage::DiscardWizard)),
                ),
            Dialog::Quit => widget::dialog()
                .title(fl!("quit-confirm-title"))
                .body(fl!("quit-confirm-body"))
                .primary_action(widget::button::destructive(fl!("quit")).on_press_maybe(confirm))
                .secondary_action(cancel),
            Dialog::DeleteSnapshot { label, .. } => widget::dialog()
                .title(fl!("delete-snapshot-title"))
                .body(fl!("delete-snapshot-body", time = label.clone()))
                .primary_action(widget::button::destructive(fl!("delete")).on_press_maybe(confirm))
                .secondary_action(cancel),
            Dialog::PasswordCommand { text, .. } => widget::dialog()
                .title(fl!("password-source-title"))
                .body(fl!("password-source-body"))
                .control(
                    widget::text_input(fl!("password-source-placeholder"), text.as_str())
                        .on_input(|text| Message::Dialog(DialogMessage::Typed(text))),
                )
                .primary_action(widget::button::suggested(fl!("save")).on_press_maybe(confirm))
                .secondary_action(cancel),
            Dialog::ChangePassword {
                id,
                password,
                confirm: confirm_password,
                ..
            } => {
                let confirm_action = confirm;
                let mismatch = !confirm_password.is_empty() && password != confirm_password;
                let uses_password_command = self
                    .config
                    .profile(id)
                    .is_some_and(|profile| !profile.password_command.is_empty());
                let mut fields = widget::column::with_capacity(3)
                    .spacing(theme::active().cosmic().spacing.space_xs)
                    .push(
                        widget::secure_input(fl!("password"), password.as_str(), None, true)
                            .label(fl!("password"))
                            .on_input(|text| Message::Dialog(DialogMessage::NewPassword(text))),
                    )
                    .push(
                        widget::secure_input(
                            fl!("wizard-confirm"),
                            confirm_password.as_str(),
                            None,
                            true,
                        )
                        .label(fl!("wizard-confirm"))
                        .on_input(|text| Message::Dialog(DialogMessage::ConfirmPassword(text))),
                    );
                if mismatch {
                    fields = fields.push(widget::text::caption(fl!("wizard-mismatch")));
                }
                let mut body = fl!("change-password-body");
                if uses_password_command {
                    body = format!("{body}\n\n{}", fl!("change-password-command-note"));
                }
                widget::dialog()
                    .title(fl!("change-password-title"))
                    .body(body)
                    .control(fields)
                    .primary_action(
                        widget::button::suggested(fl!("save")).on_press_maybe(confirm_action),
                    )
                    .secondary_action(cancel)
            }
        };
        Some(built.into())
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
            return wizard.view().map(Message::Wizard);
        }
        if let Some((page, _)) = &self.restore {
            let name = self
                .config
                .profile(&page.profile_id)
                .map(|profile| profile.name.as_str())
                .unwrap_or_default();
            return page.view(name).map(Message::RestorePage);
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
            cosmic::iced::time::every(CLOCK_TICK).map(|_| Message::Tick),
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
            Message::Wizard(message) => {
                // Sign-in finished, one way or the other: the "switch to
                // your browser" modal has done its job either way, and the
                // wizard page itself already shows the outcome.
                if matches!(
                    message,
                    crate::app::wizard::Message::Place(place::Message::SignedIn(_))
                ) {
                    self.dialog = None;
                }
                let Some(wizard) = self.wizard.as_mut() else {
                    return Task::none();
                };
                let effects = wizard.update(message);
                return self.run_wizard_effects(effects);
            }
            Message::WizardFinished(result) => return self.on_wizard_finished(*result),
            Message::RestorePage(message) => {
                let Some((page, _)) = self.restore.as_mut() else {
                    return Task::none();
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
                self.reload_runs();
            }
            // Only redraws, so running times count up.
            Message::WaitingTick => {}
            Message::ScheduleFailed(detail) => {
                self.dialog = Some(Dialog::Error(errors::describe(
                    &fl!("schedule-failed"),
                    &EngineError::new(engine::ErrorKind::Internal, detail),
                )));
            }
            Message::ExportSettings => {
                return Task::perform(
                    tasks::choose_save_path(
                        fl!("settings-export-title"),
                        "stellarshot-settings.ron".to_owned(),
                    ),
                    |path| app(Message::ExportChosen(path)),
                );
            }
            Message::ExportChosen(Some(path)) => {
                let profiles = self.config.profiles.clone();
                return Task::perform(
                    async move {
                        let text = tasks::export_settings(profiles).await?;
                        tasks::write_file(path, text).await
                    },
                    |result| app(Message::ExportSaved(result)),
                );
            }
            Message::ExportChosen(None) => {}
            Message::ExportSaved(Ok(())) => {
                self.dialog = Some(Dialog::Info(
                    fl!("settings-export-done-title"),
                    fl!("settings-export-done-body"),
                ));
            }
            Message::ExportSaved(Err(detail)) => {
                self.dialog = Some(Dialog::Error(errors::describe(
                    &fl!("settings-export-failed"),
                    &EngineError::new(engine::ErrorKind::Io, detail),
                )));
            }
            Message::ImportSettings => {
                return Task::perform(
                    tasks::choose_import_path(fl!("settings-import-title")),
                    |path| app(Message::ImportChosen(path)),
                );
            }
            Message::ImportChosen(Some(path)) => {
                return Task::perform(tasks::read_export(path), |result| {
                    app(Message::ImportRead(result))
                });
            }
            Message::ImportChosen(None) => {}
            Message::ImportRead(Ok(export)) => {
                let merged = settings_export::merge(&self.config.profiles, &export);
                self.save_profiles(merged.profiles);
                let mut body = fl!(
                    "settings-import-done-body",
                    added = (merged.counts.added as i64),
                    skipped = (merged.counts.skipped as i64),
                    rejected = (merged.counts.rejected as i64)
                );
                if merged.needs_review {
                    body = format!("{body}\n\n{}", fl!("settings-import-hooks-disabled"));
                }
                self.dialog = Some(Dialog::Info(fl!("settings-import-done-title"), body));
                // Each newly added backup's own schedule, exactly as an
                // existing one gets it when the wizard creates or edits it.
                return Task::batch(
                    merged
                        .added
                        .into_iter()
                        .map(Self::apply_schedule)
                        .collect::<Vec<_>>(),
                );
            }
            Message::ImportRead(Err(detail)) => {
                self.dialog = Some(Dialog::Error(errors::describe(
                    &fl!("settings-import-failed"),
                    &EngineError::new(engine::ErrorKind::Io, detail),
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
            Message::GlobalExcludePatternInput(text) => {
                self.global_exclude_pattern_input = text;
            }
            Message::AddGlobalExcludePattern => {
                let pattern = self.global_exclude_pattern_input.trim().to_owned();
                if !pattern.is_empty() && !self.config.global_exclude_patterns.contains(&pattern) {
                    let mut patterns = self.config.global_exclude_patterns.clone();
                    patterns.push(pattern);
                    if let Some(handler) = &self.config_handler
                        && let Err(err) = self.config.set_global_exclude_patterns(handler, patterns)
                    {
                        error_log!(CONFIG, "failed to save the global exclusions: {err}");
                    }
                }
                self.global_exclude_pattern_input.clear();
            }
            Message::RemoveGlobalExcludePattern(index) => {
                let mut patterns = self.config.global_exclude_patterns.clone();
                if index < patterns.len() {
                    patterns.remove(index);
                    if let Some(handler) = &self.config_handler
                        && let Err(err) = self.config.set_global_exclude_patterns(handler, patterns)
                    {
                        error_log!(CONFIG, "failed to save the global exclusions: {err}");
                    }
                }
            }
            Message::ChooseCacheDir => {
                return Task::perform(
                    tasks::pick_folder(fl!("settings-cache-dir-title")),
                    |path| app(Message::CacheDirChosen(path)),
                );
            }
            Message::CacheDirChosen(Some(path)) => {
                engine::cache_settings::set(Some(path.clone()), self.config.no_cache);
                if let Some(handler) = &self.config_handler
                    && let Err(err) = self.config.set_cache_dir(handler, Some(path))
                {
                    error_log!(CONFIG, "failed to save the cache location: {err}");
                }
            }
            Message::CacheDirChosen(None) => {}
            Message::ClearCacheDir => {
                engine::cache_settings::set(None, self.config.no_cache);
                if let Some(handler) = &self.config_handler
                    && let Err(err) = self.config.set_cache_dir(handler, None)
                {
                    error_log!(CONFIG, "failed to save the cache location: {err}");
                }
            }
            Message::NoCache(no_cache) => {
                engine::cache_settings::set(self.config.cache_dir.clone(), no_cache);
                if let Some(handler) = &self.config_handler
                    && let Err(err) = self.config.set_no_cache(handler, no_cache)
                {
                    error_log!(CONFIG, "failed to save the cache setting: {err}");
                }
            }
            Message::HistoryLoaded(entries) => self.history = Some(entries),
            Message::WebScope(scope) => {
                let was_off = self.config.web.scope == NetworkScope::Off;
                self.update_web(|web| web.scope = scope);
                // Off really means off, right away, not "on until the next
                // restart" — and turning it on should not need a separate,
                // easy-to-miss trip to Start after choosing a scope in the
                // very settings that imply it. Switching between Localhost
                // and Lan while already on still needs the explicit Restart
                // button: it only changes which address is bound, not
                // whether anything is listening at all.
                return match (was_off, scope == NetworkScope::Off) {
                    (true, false) => self.web_daemon_task(web_daemon::start),
                    (false, true) => self.web_daemon_task(web_daemon::stop),
                    _ => Task::none(),
                };
            }
            Message::WebPasswordEnabled(enabled) => {
                self.update_web(|web| web.password_enabled = enabled);
                return self.restart_web_daemon_if_active();
            }
            Message::WebPasswordInput(text) => {
                self.web_password_input = text;
                self.web_password_status = None;
            }
            // Unreachable through the Save button while too short (it is
            // disabled — see `settings_view`'s `can_save`), but `on_submit`
            // still fires on Enter; quietly do nothing rather than show a
            // dialog for what the field's own caption already explains.
            Message::SaveWebPassword => {
                if !password_long_enough(&self.web_password_input) {
                    return Task::none();
                }
                let secret = Secret::new(std::mem::take(&mut self.web_password_input));
                return Task::perform(
                    async move { crate::keyring::store_web_password(&secret).await },
                    |result| app(Message::WebPasswordSaved(result)),
                );
            }
            Message::WebPasswordSaved(Ok(())) => {
                self.web_password_status = Some(Ok(()));
                return self.restart_web_daemon_if_active();
            }
            Message::WebPasswordSaved(Err(detail)) => {
                self.web_password_status = Some(Err(EngineError::new(
                    engine::ErrorKind::KeyringUnavailable,
                    detail,
                )));
            }
            Message::WebTokenEnabled(enabled) => {
                self.update_web(|web| web.token_enabled = enabled);
                return self.restart_web_daemon_if_active();
            }
            // A token already in use is asked about first: generating a new
            // one invalidates it immediately, breaking whatever already
            // relies on it with no warning. Nothing to lose the first time,
            // so that case skips straight to generating one.
            Message::GenerateWebToken => {
                if self.config.web.token_hash.is_some() {
                    self.dialog = Some(Dialog::RegenerateToken);
                } else {
                    let token = crate::web_token::generate();
                    self.update_web(|web| web.token_hash = Some(token.hash));
                    self.dialog = Some(Dialog::Token(token.raw));
                    return self.restart_web_daemon_if_active();
                }
            }
            Message::WebPamEnabled(enabled) => self.update_web(|web| web.pam_enabled = enabled),
            Message::WebAllowedAddressInput(text) => self.web_allowed_address_input = text,
            // Unreachable through the Add button while invalid (it is
            // disabled — see `settings_view`'s `valid`), but `on_submit`
            // still fires on Enter; quietly do nothing rather than save an
            // entry that could never match anything.
            Message::AddWebAllowedAddress => {
                let address = self.web_allowed_address_input.trim().to_owned();
                if valid_allow_list_entry(&address) {
                    self.web_allowed_address_input.clear();
                    self.update_web(|web| {
                        if !web.allowed_addresses.contains(&address) {
                            web.allowed_addresses.push(address);
                        }
                    });
                    return self.restart_web_daemon_if_active();
                }
            }
            Message::RemoveWebAllowedAddress(index) => {
                self.update_web(|web| {
                    if index < web.allowed_addresses.len() {
                        web.allowed_addresses.remove(index);
                    }
                });
                return self.restart_web_daemon_if_active();
            }
            Message::WebPortInput(text) => self.web_port_input = text,
            // Unreachable through the Save button while invalid (it is
            // disabled — see `settings_view`'s `port_valid`), but `on_submit`
            // still fires on Enter; quietly do nothing rather than show a
            // dialog for what the field's own caption already explains.
            Message::SaveWebPort => {
                if let Some(port) = parse_port(&self.web_port_input) {
                    self.web_port_input = port.to_string();
                    self.update_web(|web| web.port = port);
                }
            }
            Message::ChooseWebTlsCert => {
                return Task::perform(tasks::pick_file(fl!("web-tls-cert-title")), |path| {
                    app(Message::WebTlsCertChosen(path))
                });
            }
            Message::WebTlsCertChosen(Some(path)) => {
                self.update_web(|web| web.tls_cert_path = Some(path));
            }
            Message::WebTlsCertChosen(None) => {}
            Message::ClearWebTlsCert => self.update_web(|web| web.tls_cert_path = None),
            Message::ChooseWebTlsKey => {
                return Task::perform(tasks::pick_file(fl!("web-tls-key-title")), |path| {
                    app(Message::WebTlsKeyChosen(path))
                });
            }
            Message::WebTlsKeyChosen(Some(path)) => {
                self.update_web(|web| web.tls_key_path = Some(path));
            }
            Message::WebTlsKeyChosen(None) => {}
            Message::ClearWebTlsKey => self.update_web(|web| web.tls_key_path = None),
            Message::WebDaemonStart => return self.web_daemon_task(web_daemon::start),
            Message::WebDaemonStop => return self.web_daemon_task(web_daemon::stop),
            Message::WebDaemonRestart => return self.web_daemon_task(web_daemon::restart),
            Message::WebDaemonActed(Err(detail)) => {
                self.show_error(
                    &fl!("web-daemon-action-failed"),
                    &EngineError::new(engine::ErrorKind::Internal, detail),
                );
                return Self::web_daemon_status_task();
            }
            Message::WebDaemonActed(Ok(())) => return Self::web_daemon_status_task(),
            Message::WebDaemonStatus(status) => self.web_daemon_status = status,
            Message::ToggleContextPage(context_page) => {
                if self.context_page == context_page {
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
                if self.context_page == ContextPage::Settings && self.core.window.show_context {
                    // The field starts holding the port actually in effect,
                    // not empty behind a placeholder that only looks
                    // pre-filled — saving without touching it must keep the
                    // current port, not fail with nothing to explain why.
                    self.web_port_input = self.config.web.port.to_string();
                    return Self::web_daemon_status_task();
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
                if self.pages.values().any(ProfileState::is_busy) {
                    self.dialog = Some(Dialog::Quit);
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

    /// The round trip `Launch::from_flags` and `CosmicFlags` build on: what
    /// launching `stellarshot` again with each flag combination asks an
    /// already-running instance to do, and back, exactly as it would arrive
    /// over D-Bus (plain strings, `Details::ActivateAction`'s own shape —
    /// see `Launch::from_wire`'s doc comment).
    #[test]
    fn a_launch_survives_the_round_trip_to_wire_strings_and_back() {
        let cases = [
            (true, false, &None, Some(Launch::NewBackup)),
            (false, true, &None, Some(Launch::Restore)),
            (
                false,
                false,
                &Some("home".to_owned()),
                Some(Launch::Profile("home".to_owned())),
            ),
            (false, false, &None, None),
        ];
        for (start_wizard, start_restore, select, expected) in cases {
            let launch = Launch::from_flags(start_wizard, start_restore, select);
            assert_eq!(launch, expected);
            let Some(launch) = launch else { continue };
            let action = launch.to_string();
            let args: Vec<String> = match &launch {
                Launch::Profile(id) => vec![id.clone()],
                Launch::NewBackup | Launch::Restore => Vec::new(),
            };
            assert_eq!(
                Launch::from_wire(&action, &args),
                Some(launch),
                "must read back its own wire form"
            );
        }
    }

    #[test]
    fn new_backup_takes_precedence_over_restore_and_profile() {
        // Mirrors `main.rs`'s own precedence: if a caller somehow sets more
        // than one, a new backup wins, then restore, then a profile ID —
        // the same order `main` checks them in.
        assert_eq!(
            Launch::from_flags(true, true, &Some("x".to_owned())),
            Some(Launch::NewBackup)
        );
        assert_eq!(
            Launch::from_flags(false, true, &Some("x".to_owned())),
            Some(Launch::Restore)
        );
    }

    #[test]
    fn an_unrecognized_action_is_not_a_launch() {
        assert_eq!(Launch::from_wire("something-else", &[]), None);
        assert_eq!(Launch::from_wire("profile", &[]), None, "no id, no launch");
    }

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
    fn the_web_interface_off_has_no_address_to_show() {
        assert_eq!(web_address(NetworkScope::Off, 8737), None);
    }

    #[test]
    fn localhost_scope_points_at_the_loopback_address() {
        assert_eq!(
            web_address(NetworkScope::Localhost, 8737),
            Some("https://127.0.0.1:8737".to_owned())
        );
    }

    #[test]
    fn lan_scope_points_at_this_machine_s_mdns_name() {
        let url = web_address(NetworkScope::Lan, 8737).unwrap();
        assert!(url.starts_with("https://"));
        assert!(url.ends_with(".local:8737"));
    }

    #[test]
    fn a_chosen_port_is_reflected_in_the_shown_address() {
        assert_eq!(
            web_address(NetworkScope::Localhost, 9000),
            Some("https://127.0.0.1:9000".to_owned())
        );
    }

    #[test]
    fn a_password_shorter_than_the_minimum_is_rejected() {
        assert!(!password_long_enough("short"));
        assert!(password_long_enough("twelve-chars"));
        assert!(password_long_enough(&"a".repeat(40)));
    }

    #[test]
    fn password_length_is_counted_in_characters_not_bytes() {
        // Twelve é's: 24 UTF-8 bytes, but 12 characters — long enough.
        assert!(password_long_enough(&"é".repeat(12)));
    }

    #[test]
    fn a_plain_port_number_parses() {
        assert_eq!(parse_port("9000"), Some(9000));
        assert_eq!(
            parse_port("  9000  "),
            Some(9000),
            "surrounding space is trimmed"
        );
        assert_eq!(parse_port("1"), Some(1));
        assert_eq!(parse_port("65535"), Some(65535));
    }

    #[test]
    fn zero_empty_and_out_of_range_ports_are_rejected() {
        assert_eq!(parse_port("0"), None, "not a port anything can listen on");
        assert_eq!(parse_port(""), None);
        assert_eq!(parse_port("not a number"), None);
        assert_eq!(parse_port("65536"), None, "one past the top of u16");
        assert_eq!(parse_port("-1"), None);
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
        assert!(Dialog::Quit.can_confirm());
        assert!(Dialog::Token("abc123".into()).can_confirm());
        assert!(Dialog::RegenerateToken.can_confirm());
    }
}
