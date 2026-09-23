// SPDX-License-Identifier: GPL-3.0-only

use std::any::TypeId;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::{env, process};

use cosmic::app::{message, Core, Task};
use cosmic::iced::alignment::{Horizontal, Vertical};
use cosmic::iced::{event, keyboard::Event as KeyEvent, window, Event, Subscription};
use cosmic::iced_core::keyboard::{Key, Modifiers};
use cosmic::widget::about::About;
use cosmic::widget::menu::{action::MenuAction, key_bind::KeyBind};
use cosmic::widget::segmented_button::{self, EntityMut, SingleSelect};
use cosmic::{cosmic_config, cosmic_theme, iced::Length, ApplicationExt};
use cosmic::{widget, Application, Apply, Element};
use views::content::{self, Content};

use crate::app::config::{AppTheme, Repository, CONFIG_VERSION};
use crate::app::key_bind::key_binds;
use crate::backup;
use crate::backup::location::url_to_path;
use crate::debug::{CONFIG, ENGINE, UI};
use crate::{debug_log, error_log, fl, Error};

use self::icon_cache::IconCache;

pub mod config;
pub mod error;
pub mod icon_cache;
mod key_bind;
pub mod menu;
pub mod migrate;
pub mod settings;
pub mod views;

pub struct App {
    core: Core,
    nav_model: segmented_button::SingleSelectModel,
    about: About,
    content: Content,
    app_themes: Vec<String>,
    config_handler: Option<cosmic_config::Config>,
    config: config::StellarshotConfig,
    context_page: ContextPage,
    dialog_pages: VecDeque<DialogPage>,
    dialog_text_input: widget::Id,
    /// Localized once: `text_input::label` borrows its text for the life of
    /// the view, so it cannot take a temporary.
    password_label: String,
    key_binds: HashMap<KeyBind, Action>,
    modifiers: Modifiers,
}

#[derive(Debug, Clone)]
pub enum Message {
    Content(content::Message),
    DialogCancel,
    DialogComplete,
    DialogUpdate(DialogPage),
    ToggleContextPage(ContextPage),
    LaunchUrl(String),
    AppTheme(usize),
    SystemThemeModeChange,
    /// Settings changed on disk, for example from another window.
    ConfigChanged(config::StellarshotConfig),
    Key(Modifiers, Key),
    Modifiers(Modifiers),
    WindowClose,
    WindowNew,
    Repository(RepositoryAction),
    CreateSnapshot(Vec<PathBuf>),
    RequestFileForRepository,
    OpenCreateRepositoryDialog(PathBuf),
    OpenCreateSnapshotDialog(Vec<PathBuf>),
    ShowError(String),
    DeleteRepositoryDialog,
    RequestFilesForSnapshot,
    OpenPasswordDialog(Repository),
    CloseContextDrawer,
}

#[derive(Debug, Clone)]
pub enum RepositoryAction {
    Init(PathBuf, String),
    Created(Repository),
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DialogPage {
    Password(Repository, String),
    CreateRepository(PathBuf, String),
    CreateSnapshot(Vec<PathBuf>),
    DeleteRepository,
    /// A localized explanation of something that failed.
    Error(String),
}

/// Turn an error into a message a user can act on. Known cases get a
/// localized explanation; anything else keeps the technical detail, since it
/// comes from the backup engine and cannot be translated.
fn describe_error(context: String, error: &Error) -> String {
    match error {
        Error::LocationNotEmpty(path) => {
            fl!("location-not-empty", path = path.display().to_string())
        }
        other => format!(
            "{context}\n\n{}",
            fl!("error-details", details = other.to_string())
        ),
    }
}

/// The paths a file-chooser portal response refers to. Anything that is not a
/// local file is dropped.
fn portal_paths(
    result: ashpd::Result<ashpd::desktop::file_chooser::SelectedFiles>,
) -> Result<Vec<PathBuf>, String> {
    let files = result.map_err(|err| err.to_string())?;
    Ok(files.uris().iter().filter_map(url_to_path).collect())
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: config::StellarshotConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    About,
    CreateRepository,
    CreateSnapshot,
    DeleteRepository,
    Settings,
    WindowClose,
    WindowNew,
}

impl MenuAction for Action {
    type Message = Message;
    fn message(&self) -> Self::Message {
        match self {
            Action::About => Message::ToggleContextPage(ContextPage::About),
            Action::CreateRepository => Message::RequestFileForRepository,
            Action::CreateSnapshot => Message::RequestFilesForSnapshot,
            Action::DeleteRepository => Message::DeleteRepositoryDialog,
            Action::Settings => Message::ToggleContextPage(ContextPage::Settings),
            Action::WindowClose => Message::WindowClose,
            Action::WindowNew => Message::WindowNew,
        }
    }
}

impl App {
    fn update_config(&mut self) -> Task<Message> {
        cosmic::app::command::set_theme(self.config.app_theme.theme())
    }

    fn settings(&self) -> Element<'_, Message> {
        let app_theme_selected = match self.config.app_theme {
            AppTheme::Dark => 1,
            AppTheme::Light => 2,
            AppTheme::System => 0,
        };
        widget::settings::view_column(vec![widget::settings::section()
            .title(fl!("appearance"))
            .add(
                widget::settings::item::builder(fl!("theme")).control(widget::dropdown(
                    &self.app_themes,
                    Some(app_theme_selected),
                    Message::AppTheme,
                )),
            )
            .into()])
        .into()
    }

    fn create_nav_item(
        &mut self,
        repository: Repository,
        icon: &'static str,
    ) -> EntityMut<'_, SingleSelect> {
        self.nav_model
            .insert()
            .icon(IconCache::get(icon, 18))
            .text(repository.name.clone())
            .data(repository.clone())
    }
}

impl Application for App {
    type Executor = cosmic::executor::Default;

    type Flags = Flags;

    type Message = Message;

    const APP_ID: &'static str = "io.github.stldave314.Stellarshot";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn header_start(&self) -> Vec<Element<'_, Self::Message>> {
        vec![menu::menu_bar(&self.key_binds)]
    }

    fn nav_model(&self) -> Option<&widget::nav_bar::Model> {
        Some(&self.nav_model)
    }

    fn init(core: Core, flags: Self::Flags) -> (Self, Task<Self::Message>) {
        let nav_model = segmented_button::ModelBuilder::default().build();
        let about = About::default()
            .name(fl!("stellarshot"))
            .icon(Self::APP_ID)
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
            nav_model,
            about,
            content: Content::new(),
            app_themes: vec![fl!("match-desktop"), fl!("dark"), fl!("light")],
            context_page: ContextPage::Settings,
            config_handler: flags.config_handler,
            config: flags.config,
            dialog_pages: VecDeque::new(),
            dialog_text_input: widget::Id::unique(),
            password_label: fl!("password"),
            key_binds: key_binds(),
            modifiers: Modifiers::empty(),
        };

        let repositories = app.config.repositories.clone();
        for repository in repositories {
            app.create_nav_item(repository, "harddisk-symbolic");
        }

        (app, Task::none())
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
                Message::LaunchUrl,
                Message::CloseContextDrawer,
            )
            .title(title),
            ContextPage::Settings => cosmic::app::context_drawer::context_drawer(
                self.settings(),
                Message::CloseContextDrawer,
            )
            .title(title),
        })
    }

    fn dialog(&self) -> Option<Element<'_, Message>> {
        let dialog_page = self.dialog_pages.front()?;

        let spacing = cosmic::theme::active().cosmic().spacing;

        let dialog = match dialog_page {
            DialogPage::CreateRepository(directory, password) => widget::dialog()
                .title(fl!("create-repo"))
                .primary_action(
                    widget::button::suggested(fl!("save"))
                        .on_press_maybe(Some(Message::DialogComplete)),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                )
                .control(
                    widget::column::with_children(vec![
                        widget::text::body(fl!(
                            "repo-location-value",
                            path = directory.display().to_string()
                        ))
                        .into(),
                        widget::text_input("", password)
                            .password()
                            .label(self.password_label.as_str())
                            .id(self.dialog_text_input.clone())
                            .on_input(move |password| {
                                Message::DialogUpdate(DialogPage::CreateRepository(
                                    directory.clone(),
                                    password,
                                ))
                            })
                            .on_submit(Message::DialogComplete)
                            .into(),
                    ])
                    .spacing(spacing.space_xxs),
                ),
            DialogPage::CreateSnapshot(files) => widget::dialog()
                .title(fl!("create-snapshot"))
                .body(fl!("snapshot-description"))
                .control(
                    widget::column::with_children(
                        files
                            .iter()
                            .map(|file| widget::text::body(file.display().to_string()).into())
                            .collect(),
                    )
                    .spacing(spacing.space_xxs),
                )
                .primary_action(
                    widget::button::suggested(fl!("create"))
                        .on_press_maybe(Some(Message::DialogComplete)),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                ),
            DialogPage::Password(repository, password) => widget::dialog()
                .title(fl!("password-for", name = repository.name.clone()))
                .primary_action(
                    widget::button::suggested(fl!("ok"))
                        .on_press_maybe(Some(Message::DialogComplete)),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                )
                .control(
                    widget::text_input("", password)
                        .password()
                        .label(self.password_label.as_str())
                        .id(self.dialog_text_input.clone())
                        .on_input(move |password| {
                            Message::DialogUpdate(DialogPage::Password(
                                repository.clone(),
                                password,
                            ))
                        })
                        .on_submit(Message::DialogComplete),
                ),
            DialogPage::DeleteRepository => widget::dialog()
                .title(fl!("delete-repository"))
                .body(fl!("delete-repository-description"))
                .primary_action(
                    widget::button::suggested(fl!("delete"))
                        .on_press_maybe(Some(Message::DialogComplete)),
                )
                .secondary_action(
                    widget::button::standard(fl!("cancel")).on_press(Message::DialogCancel),
                ),
            DialogPage::Error(message) => widget::dialog()
                .title(fl!("error-title"))
                .body(message.as_str())
                .primary_action(
                    widget::button::suggested(fl!("ok")).on_press(Message::DialogCancel),
                ),
        };

        Some(dialog.into())
    }

    fn on_nav_select(&mut self, entity: widget::nav_bar::Id) -> Task<Self::Message> {
        let mut commands = vec![];
        self.nav_model.activate(entity);

        if let Some(repository) = self.nav_model.data::<Repository>(entity) {
            debug_log!(UI, "selected repository {}", repository.path.display());
            let name = repository.name.clone();
            commands.push(self.update(Message::OpenPasswordDialog(repository.clone())));
            let window_title = format!("{} - {}", name, fl!("stellarshot"));
            if let Some(win_id) = self.core.main_window_id() {
                commands.push(self.set_window_title(window_title, win_id));
            }
        }

        Task::batch(commands)
    }

    fn view(&self) -> Element<'_, Self::Message> {
        widget::container(self.content.view().map(Message::Content))
            .apply(widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Center)
            .align_y(Vertical::Center)
            .into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        struct ConfigSubscription;
        struct ThemeSubscription;

        let subscriptions = vec![
            event::listen_with(|event, status, _win_id| match event {
                Event::Keyboard(KeyEvent::KeyPressed { key, modifiers, .. }) => match status {
                    event::Status::Ignored => Some(Message::Key(modifiers, key)),
                    event::Status::Captured => None,
                },
                Event::Keyboard(KeyEvent::ModifiersChanged(modifiers)) => {
                    Some(Message::Modifiers(modifiers))
                }
                _ => None,
            }),
            cosmic_config::config_subscription::<_, config::StellarshotConfig>(
                TypeId::of::<ConfigSubscription>(),
                Self::APP_ID.into(),
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
            .map(|update| {
                if !update.errors.is_empty() {
                    debug_log!(
                        CONFIG,
                        "errors loading theme mode {:?}: {:?}",
                        update.keys,
                        update.errors
                    );
                }
                Message::SystemThemeModeChange
            }),
        ];

        Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        macro_rules! config_set {
            ($name: ident, $value: expr) => {
                match &self.config_handler {
                    Some(config_handler) => {
                        match paste::paste! { self.config.[<set_ $name>](config_handler, $value) } {
                            Ok(_) => {}
                            Err(err) => {
                                error_log!(
                                    CONFIG,
                                    "failed to save config {:?}: {}",
                                    stringify!($name),
                                    err
                                );
                            }
                        }
                    }
                    None => {
                        self.config.$name = $value;
                        error_log!(
                            CONFIG,
                            "failed to save config {:?}: no config handler",
                            stringify!($name)
                        );
                    }
                }
            };
        }

        match message {
            Message::Content(message) => {
                // Every task the content view asks for runs; returning from
                // inside the loop used to drop all but the first.
                let tasks =
                    self.content
                        .update(message)
                        .into_iter()
                        .map(|task| match task {
                            content::Task::FetchSnapshots(repository, password) => Task::perform(
                                async move { Content::snapshots(&repository, &password) },
                                |result| {
                                    message::app(Message::Content(content::Message::SetSnapshots(
                                        result,
                                    )))
                                },
                            ),
                            content::Task::DeleteSnapshots(repository, password, snapshots) => {
                                Task::perform(
                                    async move {
                                        backup::snapshot::delete(&repository, &password, snapshots)
                                    },
                                    |result| match result {
                                        Ok(()) => message::app(Message::Content(
                                            content::Message::ReloadSnapshots,
                                        )),
                                        Err(err) => message::app(Message::ShowError(
                                            describe_error(fl!("delete-snapshot-failed"), &err),
                                        )),
                                    },
                                )
                            }
                        });
                return Task::batch(tasks);
            }
            Message::ToggleContextPage(context_page) => {
                //TODO: ensure context menus are closed
                if self.context_page == context_page {
                    self.core.window.show_context = !self.core.window.show_context;
                } else {
                    self.context_page = context_page;
                    self.core.window.show_context = true;
                }
            }
            Message::RequestFileForRepository => {
                let title = fl!("select-repo-folder");
                return Task::perform(
                    async move {
                        match ashpd::desktop::file_chooser::SelectedFiles::open_file()
                            .title(title.as_str())
                            .directory(true)
                            .multiple(false)
                            .send()
                            .await
                        {
                            Ok(request) => portal_paths(request.response()),
                            Err(err) => Err(err.to_string()),
                        }
                    },
                    |result| match result {
                        Ok(paths) => match paths.into_iter().next() {
                            Some(path) => message::app(Message::OpenCreateRepositoryDialog(path)),
                            // Cancelled, or a non-local location was picked.
                            None => cosmic::app::Message::None,
                        },
                        Err(err) => {
                            debug_log!(UI, "file chooser for repository: {err}");
                            cosmic::app::Message::None
                        }
                    },
                );
            }
            Message::RequestFilesForSnapshot => {
                let title = fl!("select-snapshot-files");
                return Task::perform(
                    async move {
                        match ashpd::desktop::file_chooser::SelectedFiles::open_file()
                            .title(title.as_str())
                            .directory(false)
                            .multiple(true)
                            .send()
                            .await
                        {
                            Ok(request) => portal_paths(request.response()),
                            Err(err) => Err(err.to_string()),
                        }
                    },
                    |result| match result {
                        Ok(paths) if !paths.is_empty() => {
                            message::app(Message::OpenCreateSnapshotDialog(paths))
                        }
                        Ok(_) => cosmic::app::Message::None,
                        Err(err) => {
                            debug_log!(UI, "file chooser for snapshot: {err}");
                            cosmic::app::Message::None
                        }
                    },
                );
            }
            Message::ShowError(message) => {
                self.dialog_pages.push_back(DialogPage::Error(message));
            }
            Message::OpenCreateRepositoryDialog(path) => {
                self.dialog_pages
                    .push_back(DialogPage::CreateRepository(path, String::new()));
                return widget::text_input::focus(self.dialog_text_input.clone());
            }
            Message::OpenCreateSnapshotDialog(files) => {
                self.dialog_pages
                    .push_back(DialogPage::CreateSnapshot(files));
            }
            Message::OpenPasswordDialog(repository) => {
                let Some(current_repository) = &self.content.repository else {
                    self.dialog_pages
                        .push_back(DialogPage::Password(repository, String::new()));
                    return Task::none();
                };
                if &repository != current_repository {
                    self.dialog_pages
                        .push_back(DialogPage::Password(repository, String::new()));
                }
            }
            Message::Repository(state) => match state {
                RepositoryAction::Init(path, password) => {
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let repository = Repository {
                        name,
                        path: path.clone(),
                    };
                    // The sidebar entry is only added once the repository
                    // exists, so a refused or failed creation leaves nothing
                    // behind.
                    return Task::perform(
                        async move { crate::backup::init(&path, &password) },
                        move |result| match result {
                            Ok(()) => message::app(Message::Repository(RepositoryAction::Created(
                                repository.clone(),
                            ))),
                            Err(err) => message::app(Message::ShowError(describe_error(
                                fl!("create-repo-failed"),
                                &err,
                            ))),
                        },
                    );
                }
                RepositoryAction::Created(repository) => {
                    debug_log!(CONFIG, "adding repository {}", repository.path.display());
                    self.create_nav_item(repository.clone(), "harddisk-symbolic");
                    let mut repositories = self.config.repositories.clone();
                    if !repositories.iter().any(|r| r.path == repository.path) {
                        repositories.push(repository);
                    }
                    config_set!(repositories, repositories);
                }
            },
            Message::DeleteRepositoryDialog => {
                // Delete acts on the repository selected in the sidebar, which
                // need not have been unlocked with its password first.
                if self.nav_model.active_data::<Repository>().is_some() {
                    self.dialog_pages.push_back(DialogPage::DeleteRepository);
                }
            }
            Message::CreateSnapshot(files) => {
                if let Some(repository) = &self.content.repository {
                    let Some(path) = repository.path.to_str() else {
                        return self.update(Message::ShowError(fl!(
                            "error-details",
                            details = Error::NonUtf8Path(repository.path.clone()).to_string()
                        )));
                    };
                    let sources: Vec<&str> = files.iter().filter_map(|f| f.to_str()).collect();
                    match crate::backup::snapshot(path, &self.content.password, sources) {
                        Ok(()) => {
                            return self.update(Message::Content(content::Message::ReloadSnapshots))
                        }
                        Err(err) => {
                            error_log!(ENGINE, "failed to create snapshot: {err}");
                            return self.update(Message::ShowError(describe_error(
                                fl!("snapshot-failed"),
                                &err,
                            )));
                        }
                    }
                }
            }
            Message::DialogCancel => {
                self.dialog_pages.pop_front();
            }
            Message::DialogComplete => {
                if let Some(dialog_page) = self.dialog_pages.pop_front() {
                    match dialog_page {
                        DialogPage::CreateRepository(path, password) => {
                            return self.update(Message::Repository(RepositoryAction::Init(
                                path, password,
                            )));
                        }
                        DialogPage::CreateSnapshot(files) => {
                            return self.update(Message::CreateSnapshot(files));
                        }
                        DialogPage::Password(repository, password) => {
                            return self.update(Message::Content(content::Message::SetRepository(
                                repository, password,
                            )));
                        }
                        DialogPage::DeleteRepository => {
                            let entity = self.nav_model.active();
                            let Some(repository) =
                                self.nav_model.data::<Repository>(entity).cloned()
                            else {
                                return Task::none();
                            };
                            // Only the repository's own entries are removed;
                            // anything else in the folder is left alone.
                            match backup::location::delete_repository(&repository.path) {
                                Ok(remaining) => {
                                    debug_log!(
                                        ENGINE,
                                        "deleted repository {}; left {} other entries",
                                        repository.path.display(),
                                        remaining.len()
                                    );
                                    let repositories = self
                                        .config
                                        .repositories
                                        .iter()
                                        .filter(|r| r.path != repository.path)
                                        .cloned()
                                        .collect();
                                    config_set!(repositories, repositories);
                                    self.nav_model.remove(entity);
                                    if self.content.repository.as_ref() == Some(&repository) {
                                        self.content.repository = None;
                                    }
                                }
                                Err(err) => {
                                    error_log!(ENGINE, "failed to delete repository: {err}");
                                    return self.update(Message::ShowError(describe_error(
                                        fl!("delete-repo-failed"),
                                        &Error::Io(err),
                                    )));
                                }
                            }
                        }
                        DialogPage::Error(_) => {}
                    }
                }
            }
            Message::DialogUpdate(dialog_page) => {
                if let Some(front) = self.dialog_pages.front_mut() {
                    *front = dialog_page;
                }
            }
            Message::WindowClose => {
                if let Some(win_id) = self.core.main_window_id() {
                    return window::close(win_id);
                }
            }
            Message::WindowNew => match env::current_exe() {
                Ok(exe) => match process::Command::new(&exe).spawn() {
                    Ok(_child) => {}
                    Err(err) => {
                        error_log!(UI, "failed to execute {:?}: {}", exe, err);
                    }
                },
                Err(err) => {
                    error_log!(UI, "failed to get current executable path: {}", err);
                }
            },
            Message::LaunchUrl(url) => match open::that_detached(&url) {
                Ok(()) => {}
                Err(err) => {
                    error_log!(UI, "failed to open {:?}: {}", url, err);
                }
            },
            Message::Key(modifiers, key) => {
                for (key_bind, action) in self.key_binds.iter() {
                    if key_bind.matches(modifiers, &key) {
                        return self.update(action.message());
                    }
                }
            }
            Message::Modifiers(modifiers) => {
                self.modifiers = modifiers;
            }
            Message::AppTheme(index) => {
                let app_theme = match index {
                    1 => AppTheme::Dark,
                    2 => AppTheme::Light,
                    _ => AppTheme::System,
                };
                config_set!(app_theme, app_theme);
                return self.update_config();
            }
            Message::ConfigChanged(config) => {
                if config != self.config {
                    self.config = config;
                    return self.update_config();
                }
            }
            Message::SystemThemeModeChange => {
                return self.update_config();
            }
            Message::CloseContextDrawer => {
                self.core.window.show_context = !self.core.window.show_context
            }
        }

        Task::none()
    }
}
