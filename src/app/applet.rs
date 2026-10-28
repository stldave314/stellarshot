// SPDX-License-Identifier: GPL-3.0-only

//! The panel applet: at a glance, whether a backup is running, what each
//! last did and when, and whether anything needs attention. Clicking it
//! shows that; right-clicking it offers what the window can be asked to do.
//!
//! Read-only on purpose. It reads exactly what the window itself reads —
//! each profile's run history, and whether something currently holds its
//! repository's write lock (see [`crate::status`]) — rather than talking to
//! the window at all, so it works whether or not the window happens to be
//! open. Everything it offers to *do* (open the window, back up, restore)
//! is handed to the window through single-instance activation (see
//! `Cargo.toml`'s comment on the `libcosmic` `single-instance` feature),
//! which has the passwords and shows the progress.

use std::any::TypeId;
use std::process::Command;

use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Length, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget;
use cosmic::{Apply, Element};

use crate::app::config::{CONFIG_VERSION, StellarshotConfig};
use crate::app::format::{self, now};
use crate::constants::{
    APPLET_IDLE_REFRESH as IDLE_REFRESH, APPLET_POPUP_WIDTH, APPLET_REFRESH as REFRESH,
};
use crate::debug::UI;
use crate::event_log::{self, EventKind};
use crate::fl;
use crate::status::{self, Status};
use crate::{debug_log, error_log};

const ID: &str = crate::constants::APPLET_ID;

/// Which popup is open: the status a click shows, or the actions a
/// right-click offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Popup {
    Status,
    Menu,
}

/// Something the window is asked to do, by starting `stellarshot` with the
/// matching flag (which a running window receives instead; see
/// `crate::app::launch::Launch`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Open,
    /// Open with this backup selected.
    Show(String),
    BackUpAll,
    Restore,
    NewBackup,
}

impl Action {
    fn args(&self) -> Vec<&str> {
        match self {
            Self::Open => Vec::new(),
            Self::Show(id) => vec!["--profile", id],
            Self::BackUpAll => vec!["--back-up-all"],
            Self::Restore => vec!["--restore"],
            Self::NewBackup => vec!["--new-backup"],
        }
    }
}

#[derive(Default)]
pub struct Applet {
    core: Core,
    popup: Option<(Id, Popup)>,
    statuses: Vec<Status>,
    /// A refresh is reading the disk right now. Every tick and every config
    /// change asks for one, and on a hung network mount each used to start
    /// another blocking thread behind the stuck ones, answering out of
    /// order; now a request while one is running is dropped.
    refreshing: bool,
    /// The window could not be started, shown in the popup until a later
    /// attempt works.
    open_failed: bool,
}

impl std::fmt::Debug for Applet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Applet")
            .field("popup", &self.popup)
            .field("statuses", &self.statuses.len())
            .field("refreshing", &self.refreshing)
            .finish_non_exhaustive()
    }
}

impl Applet {
    /// Whether to start a refresh now, noting that one is running if so.
    fn begin_refresh(&mut self) -> bool {
        !std::mem::replace(&mut self.refreshing, true)
    }
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    /// The applet was right-clicked: show the actions menu.
    OpenMenu,
    Refresh,
    StatusesLoaded(Vec<Status>),
    Launch(Action),
    /// Whether starting the window worked.
    Opened(bool),
    /// Quit Stellarshot: the window, not this applet, which stays on the
    /// panel showing the status.
    Quit,
    Surface(cosmic::surface::Action<Message>),
}

/// Off the UI thread: reads the config file and every backup's run-state
/// file, and probes each one's repository lock.
async fn refresh() -> Vec<Status> {
    tokio::task::spawn_blocking(|| status::all(&StellarshotConfig::config().profiles, now()))
        .await
        .unwrap_or_default()
}

/// Launch (or, with single-instance active, raise) the main window,
/// asking it for `action`.
///
/// The window's binary path is derived from where the applet's own binary
/// is installed (`installed_path`, not a raw `current_exe`), since the
/// applet's own binary is `stellarshot-applet`: spawning it directly would
/// launch another applet instead of the window. Launched through
/// `cosmic::process::spawn`, which double-forks so the applet closing later
/// never leaves the window as a zombie, unlike a plain `Command::spawn`
/// that nothing ever `wait`s on.
async fn open_window(action: Action) -> bool {
    let exe = match crate::exe::installed_path() {
        Ok(exe) => exe.with_file_name("stellarshot"),
        Err(err) => {
            error_log!(UI, "applet: failed to find the window's own binary: {err}");
            return false;
        }
    };
    let mut command = Command::new(&exe);
    command.args(action.args());
    if cosmic::process::spawn(command).await.is_none() {
        error_log!(UI, "applet: failed to open the window ({exe:?})");
        return false;
    }
    true
}

impl Applet {
    fn close(&mut self, id: Id) -> Task<Message> {
        self.popup = None;
        cosmic::task::message(cosmic::Action::App(Message::Surface(destroy_popup(id))))
    }
}

/// Open the `kind` popup over the applet: anchored to `anchor` (the
/// button's own bounds, as a click reports them) if known, otherwise to the
/// applet as a whole.
fn open_popup(
    main_window: Id,
    kind: Popup,
    anchor: Option<Rectangle<i32>>,
) -> cosmic::surface::Action<Message> {
    app_popup::<Applet>(
        |_| cosmic::surface::action::LiveSettings::default(),
        move |state: &mut Applet| {
            let new_id = Id::unique();
            state.popup = Some((new_id, kind));
            let mut popup_settings =
                state
                    .core
                    .applet
                    .get_popup_settings(main_window, new_id, None, None, None);
            if let Some(anchor) = anchor {
                popup_settings.positioner.anchor_rect = anchor;
            }
            popup_settings
        },
        Some(Box::new(|state: &Applet| {
            let content = match state.popup {
                Some((_, Popup::Menu)) => menu_content(state),
                _ => popup_content(state),
            };
            Element::from(state.core.applet.popup_container(content)).map(cosmic::Action::App)
        })),
    )
}

/// Ask a running window to quit, as Ctrl+Q would, through the D-Bus name
/// its single-instance activation already holds (see
/// `crate::app::launch::Launch::Quit`). With no window running there is
/// nothing to quit, and the call simply finds no one there.
async fn quit_window() {
    use crate::app::Launch;
    let path = format!("/{}", crate::app::APP_ID.replace('.', "/"));
    let action = Launch::Quit.to_string();
    let result = async {
        let connection = zbus::Connection::session().await?;
        connection
            .call_method(
                Some(crate::app::APP_ID),
                path.as_str(),
                Some("org.freedesktop.DbusActivation"),
                "ActivateAction",
                &(
                    action.as_str(),
                    Vec::<&str>::new(),
                    std::collections::HashMap::<&str, zbus::zvariant::Value<'_>>::new(),
                ),
            )
            .await?;
        Ok::<(), zbus::Error>(())
    }
    .await;
    match result {
        Ok(()) => debug_log!(UI, "applet: asked the window to quit"),
        Err(err) => debug_log!(UI, "applet: no window to quit ({err})"),
    }
}

impl cosmic::Application for Applet {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Message>) {
        crate::debug::init(crate::debug::Role::Applet);
        crate::paths::tighten_app_dirs();
        crate::core::localization::init();
        let applet = Self {
            core,
            ..Default::default()
        };
        let mut applet = applet;
        applet.refreshing = true;
        (
            applet,
            Task::perform(refresh(), |statuses| {
                cosmic::Action::App(Message::StatusesLoaded(statuses))
            }),
        )
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        let interval = if self.popup.is_some() {
            REFRESH
        } else {
            IDLE_REFRESH
        };
        struct ConfigSubscription;
        Subscription::batch([
            cosmic::iced::time::every(interval).map(|_| Message::Refresh),
            // A profile added, removed or edited elsewhere refreshes right
            // away rather than waiting for the next tick above, which — now
            // that the popup being closed slows that tick to a minute — could
            // otherwise leave the list visibly stale for a while.
            cosmic_config::config_subscription::<_, StellarshotConfig>(
                TypeId::of::<ConfigSubscription>(),
                crate::app::APP_ID.into(),
                CONFIG_VERSION,
            )
            .map(|_| Message::Refresh),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup.is_some_and(|(open, _)| open == id) {
                    self.popup = None;
                }
            }
            Message::OpenMenu => {
                let mut tasks = Vec::new();
                if let Some((id, _)) = self.popup {
                    tasks.push(self.close(id));
                }
                if let Some(main_window) = self.core.main_window_id() {
                    tasks.push(cosmic::task::message(cosmic::Action::App(
                        Message::Surface(open_popup(main_window, Popup::Menu, None)),
                    )));
                }
                return Task::batch(tasks);
            }
            Message::Refresh => {
                if self.begin_refresh() {
                    return Task::perform(refresh(), |statuses| {
                        cosmic::Action::App(Message::StatusesLoaded(statuses))
                    });
                }
            }
            Message::StatusesLoaded(statuses) => {
                self.refreshing = false;
                self.statuses = statuses;
            }
            Message::Launch(action) => {
                debug_log!(UI, "applet: asking the window for {action:?}");
                let launch = Task::perform(open_window(action), |opened| {
                    cosmic::Action::App(Message::Opened(opened))
                });
                // The menu has done its job; the status stays up until
                // clicked away, so a failure to start can still be shown.
                return match self.popup {
                    Some((id, Popup::Menu)) => Task::batch([self.close(id), launch]),
                    _ => launch,
                };
            }
            Message::Opened(opened) => {
                self.open_failed = !opened;
                return cosmic::task::message(cosmic::Action::App(Message::Refresh));
            }
            // The window decides: it asks first if a backup is running.
            Message::Quit => {
                let quit = Task::perform(quit_window(), |()| cosmic::Action::App(Message::Refresh));
                return match self.popup {
                    Some((id, _)) => Task::batch([self.close(id), quit]),
                    None => quit,
                };
            }
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Surface(action));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let have_popup = self.popup.map(|(id, _)| id);
        let main_window = self.core.main_window_id();
        let btn = self
            .core
            .applet
            .icon_button(icon_name(&self.statuses))
            .on_press_with_rectangle(move |offset, bounds| {
                if let Some(id) = have_popup {
                    Message::Surface(destroy_popup(id))
                } else if let Some(main_window) = main_window {
                    let anchor = Rectangle {
                        x: pixels(bounds.x - offset.x),
                        y: pixels(bounds.y - offset.y),
                        width: pixels(bounds.width),
                        height: pixels(bounds.height),
                    };
                    Message::Surface(open_popup(main_window, Popup::Status, Some(anchor)))
                } else {
                    // No window to anchor a popup to (not yet, or no more):
                    // nothing to open.
                    Message::Refresh
                }
            });
        let tooltip = self.core.applet.applet_tooltip::<Message>(
            btn,
            fl!("applet-tooltip"),
            self.popup.is_some(),
            Message::Surface,
            None,
        );
        widget::mouse_area(tooltip)
            .on_right_press(Message::OpenMenu)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        cosmic::widget::text("").into()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

/// A standard, always-installed icon name reflecting the busiest or most
/// urgent thing across every backup, so a glance at the panel is enough
/// without opening the popup. The app's own icon when nothing needs
/// attention; otherwise whichever of [`crate::run_state::BackupStatus`]'s own
/// icons matches the single worst status among them (the same precedence
/// [`Status::backup_status`] uses), rather than one generic warning icon
/// that could not previously tell a failure from simply being overdue —
/// let alone ever show real damage, which this used to have no way to see
/// at all.
fn icon_name(statuses: &[Status]) -> &'static str {
    use crate::run_state::BackupStatus;
    let rank = |status: BackupStatus| match status {
        BackupStatus::Running => 4,
        BackupStatus::Damaged => 3,
        BackupStatus::Failed => 2,
        BackupStatus::Overdue => 1,
        BackupStatus::UpToDate => 0,
    };
    match statuses
        .iter()
        .map(Status::backup_status)
        .max_by_key(|&status| rank(status))
    {
        Some(BackupStatus::UpToDate) | None => "io.github.stldave314.Stellarshot-symbolic",
        Some(status) => status.icon(),
    }
}

/// What a click shows: every backup, what it last did and when, and the
/// two things most worth doing from here.
fn popup_content(state: &Applet) -> Element<'_, Message> {
    let spacing = cosmic::theme::active().cosmic().spacing;
    let now = now();
    let mut column = widget::column::with_capacity(state.statuses.len() + 6)
        .padding([spacing.space_xs, 0])
        .width(Length::Fixed(APPLET_POPUP_WIDTH));
    if state.statuses.is_empty() {
        column = column.push(cosmic::applet::padded_control(widget::text::body(fl!(
            "applet-none"
        ))));
    }
    for status in &state.statuses {
        column = column.push(
            cosmic::applet::menu_button(backup_entry(status, now))
                .on_press(Message::Launch(Action::Show(status.profile_id.clone()))),
        );
    }
    if let Some(notice) = open_failed_notice(state) {
        column = column.push(cosmic::applet::padded_control(notice));
    }
    column = column.push(divider());
    if !state.statuses.is_empty() {
        column = column.push(menu_item(fl!("applet-back-up-all"), Action::BackUpAll));
    }
    column
        .push(menu_item(fl!("applet-open"), Action::Open))
        .push(divider())
        .push(menu_quit())
        .into()
}

/// What a right-click shows: only the actions.
fn menu_content(state: &Applet) -> Element<'_, Message> {
    let spacing = cosmic::theme::active().cosmic().spacing;
    let have_backups = !state.statuses.is_empty();
    widget::column::with_capacity(7)
        .padding([spacing.space_xs, 0])
        .width(Length::Fixed(APPLET_POPUP_WIDTH))
        .push(menu_item(fl!("applet-open"), Action::Open))
        .push_maybe(have_backups.then(|| menu_item(fl!("applet-back-up-all"), Action::BackUpAll)))
        .push_maybe(have_backups.then(|| menu_item(fl!("applet-restore"), Action::Restore)))
        .push(menu_item(fl!("applet-new-backup"), Action::NewBackup))
        .push(divider())
        .push(menu_quit())
        .into()
}

/// One backup in the popup: its name and status, when it last backed up and
/// what that took in, and whatever has happened since.
fn backup_entry(status: &Status, now: i64) -> Element<'_, Message> {
    let spacing = cosmic::theme::active().cosmic().spacing;
    let backup_status = status.backup_status();
    let mut details = widget::column::with_capacity(4)
        .spacing(spacing.space_xxxs)
        .width(Length::Fill)
        .push(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .push(widget::text::body(status.name.clone()).width(Length::Fill))
                // Said in words too: the icon alone tells a screen reader
                // nothing.
                .push(widget::text::caption(backup_status.label())),
        );
    for line in entry_lines(status, now) {
        details = details.push(widget::text::caption(line));
    }
    widget::row::with_capacity(2)
        .spacing(spacing.space_s)
        .align_y(Alignment::Start)
        .push(widget::icon::from_name(backup_status.icon()).size(16))
        .push(details)
        .into()
}

/// The lines under a backup's name: when it last backed up, that backup's
/// figures if they were recorded, and the newest history entry if it is
/// something other than that backup.
fn entry_lines(status: &Status, now: i64) -> Vec<String> {
    let mut lines = Vec::with_capacity(3);
    lines.push(match status.last_success {
        Some(time) => format::backed_up_ago(now, time),
        None => fl!("never-backed-up"),
    });
    if let Some(last) = &status.last_backup {
        lines.push(fl!(
            "applet-backup-figures",
            size = format::bytes(last.total_bytes),
            new = (last.files_new as i64),
            changed = (last.files_changed as i64),
            added = format::bytes(last.data_added)
        ));
    }
    if let Some(event) = &status.last_event
        && event.kind != EventKind::BackedUp
    {
        lines.push(fl!(
            "applet-last-event",
            what = event_log::describe(&event.kind),
            when = format::local_time(event.time)
        ));
    }
    lines
}

fn menu_item<'a>(label: String, action: Action) -> Element<'a, Message> {
    cosmic::applet::menu_button(widget::text::body(label))
        .on_press(Message::Launch(action))
        .into()
}

fn menu_quit<'a>() -> Element<'a, Message> {
    cosmic::applet::menu_button(widget::text::body(fl!("applet-quit")))
        .on_press(Message::Quit)
        .into()
}

fn divider<'a>() -> Element<'a, Message> {
    cosmic::applet::padded_control(widget::divider::horizontal::default())
        .padding([
            cosmic::theme::active().cosmic().spacing.space_xxs,
            cosmic::theme::active().cosmic().spacing.space_s,
        ])
        .into()
}

/// Say so, when the last try to open the window did not work.
fn open_failed_notice(state: &Applet) -> Option<Element<'_, Message>> {
    state
        .open_failed
        .then(|| widget::text::caption(fl!("applet-open-failed")).into())
}

/// A position or size on screen in whole pixels, as the popup's anchor
/// takes it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a position on screen, nowhere near i32's range"
)]
fn pixels(value: f32) -> i32 {
    value as i32
}

/// Both popups side by side in an ordinary window, over the real statuses:
/// for screenshots (`examples/applet_preview.rs`), since a panel's popups
/// cannot be captured on their own. The buttons do nothing here.
#[doc(hidden)]
pub fn preview() -> cosmic::iced::Result {
    let settings = cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(830.0, 470.0))
        .client_decorations(false);
    cosmic::app::run::<Preview>(settings, ())
}

struct Preview {
    core: Core,
    applet: Applet,
}

impl cosmic::Application for Preview {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(mut core: Core, _flags: Self::Flags) -> (Self, Task<Message>) {
        crate::core::localization::init();
        core.window.show_headerbar = false;
        let statuses = status::all(&StellarshotConfig::config().profiles, now());
        let applet = Applet {
            statuses,
            ..Applet::default()
        };
        (Self { core, applet }, Task::none())
    }

    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let spacing = cosmic::theme::active().cosmic().spacing;
        let popup = |content| {
            widget::container(content)
                .class(cosmic::theme::Container::Dropdown)
                .padding(spacing.space_xxxs)
        };
        widget::row::with_capacity(2)
            .spacing(spacing.space_l)
            .padding(spacing.space_l)
            .align_y(Alignment::Start)
            .push(popup(popup_content(&self.applet)))
            .push(popup(menu_content(&self.applet)))
            .apply(widget::container)
            .width(Length::Fill)
            .height(Length::Fill)
            .class(cosmic::theme::Container::Background)
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::run_state;

    fn status(name: &str) -> Status {
        Status {
            profile_id: name.to_owned(),
            name: name.to_owned(),
            running: false,
            last_success: None,
            failed: false,
            overdue: false,
            damaged: false,
            last_backup: None,
            last_event: None,
        }
    }

    #[test]
    fn each_action_asks_the_window_with_its_own_flag() {
        assert_eq!(Action::Open.args(), Vec::<&str>::new());
        assert_eq!(Action::Show("id".into()).args(), ["--profile", "id"]);
        assert_eq!(Action::BackUpAll.args(), ["--back-up-all"]);
        assert_eq!(Action::Restore.args(), ["--restore"]);
        assert_eq!(Action::NewBackup.args(), ["--new-backup"]);
    }

    #[test]
    fn a_backup_shows_its_figures_and_what_happened_since() {
        crate::core::localization::init();
        let now = 1_000_000;
        let backed_up = Status {
            last_success: Some(now - 7200),
            last_backup: Some(run_state::LastBackup {
                time: now - 7200,
                files_new: 3,
                files_changed: 10,
                data_added: 2048,
                total_bytes: 4096,
            }),
            last_event: Some(event_log::Event {
                time: now - 3600,
                kind: EventKind::Checked { damaged: false },
                source: event_log::Source::Desktop,
            }),
            ..status("a")
        };
        let lines = entry_lines(&backed_up, now);
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(lines[0], format::backed_up_ago(now, now - 7200));
        assert!(
            lines[1].contains('3') && lines[1].contains("10"),
            "{lines:?}"
        );
        assert!(
            // Fluent wraps each argument in bidi isolation marks.
            lines[2].contains(&event_log::describe(&EventKind::Checked { damaged: false })),
            "{lines:?}"
        );

        let just_backed_up = Status {
            last_event: Some(event_log::Event {
                time: now - 7200,
                kind: EventKind::BackedUp,
                source: event_log::Source::Desktop,
            }),
            ..backed_up
        };
        assert_eq!(
            entry_lines(&just_backed_up, now).len(),
            2,
            "the backup itself is not said twice"
        );
        assert_eq!(entry_lines(&status("new"), now), [fl!("never-backed-up")]);
    }

    #[test]
    fn a_refresh_is_dropped_while_one_is_running() {
        let mut applet = Applet::default();

        assert!(applet.begin_refresh(), "the first one starts");
        assert!(!applet.begin_refresh(), "one while it runs does not");
        applet.refreshing = false;
        assert!(applet.begin_refresh(), "after it finishes, the next does");
    }

    #[test]
    fn nothing_wrong_shows_the_apps_own_icon() {
        assert_eq!(
            icon_name(&[status("a"), status("b")]),
            "io.github.stldave314.Stellarshot-symbolic"
        );
        assert_eq!(icon_name(&[]), "io.github.stldave314.Stellarshot-symbolic");
    }

    #[test]
    fn each_status_gets_its_own_distinct_icon_not_one_generic_warning() {
        let overdue = Status {
            overdue: true,
            ..status("a")
        };
        let failed = Status {
            failed: true,
            ..status("a")
        };
        let damaged = Status {
            damaged: true,
            ..status("a")
        };
        let running = Status {
            running: true,
            ..status("a")
        };
        assert_eq!(
            icon_name(&[overdue]),
            run_state::BackupStatus::Overdue.icon()
        );
        assert_eq!(icon_name(&[failed]), run_state::BackupStatus::Failed.icon());
        assert_eq!(
            icon_name(&[damaged]),
            run_state::BackupStatus::Damaged.icon()
        );
        assert_eq!(
            icon_name(&[running]),
            run_state::BackupStatus::Running.icon()
        );
        // Distinct from each other: the whole point of this fix.
        assert_ne!(
            run_state::BackupStatus::Overdue.icon(),
            run_state::BackupStatus::Failed.icon()
        );
        assert_ne!(
            run_state::BackupStatus::Failed.icon(),
            run_state::BackupStatus::Damaged.icon()
        );
    }

    #[test]
    fn running_outranks_a_damaged_backup_elsewhere_in_the_list() {
        let running = Status {
            running: true,
            ..status("a")
        };
        let damaged = Status {
            damaged: true,
            ..status("b")
        };
        assert_eq!(
            icon_name(&[damaged, running]),
            run_state::BackupStatus::Running.icon()
        );
    }

    #[test]
    fn damage_outranks_an_overdue_backup_elsewhere_in_the_list() {
        let overdue = Status {
            overdue: true,
            ..status("a")
        };
        let damaged = Status {
            damaged: true,
            ..status("b")
        };
        assert_eq!(
            icon_name(&[overdue, damaged]),
            run_state::BackupStatus::Damaged.icon()
        );
    }
}
