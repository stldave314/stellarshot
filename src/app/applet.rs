// SPDX-License-Identifier: GPL-3.0-only

//! The panel applet: at a glance, whether a backup is running, when each
//! last succeeded, and whether anything needs attention.
//!
//! Read-only on purpose. It reads exactly what the window itself reads —
//! each profile's run history, and whether something currently holds its
//! repository's write lock (see [`crate::status`]) — rather than talking to
//! the window at all, so it works whether or not the window happens to be
//! open. Opening the window (through single-instance activation; see
//! `Cargo.toml`'s comment on the `libcosmic` `single-instance` feature) is
//! the only thing it hands off rather than doing itself.

use std::any::TypeId;
use std::process::Command;

use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::cosmic_config;
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, list_column, settings};

use crate::app::config::{CONFIG_VERSION, StellarshotConfig};
use crate::app::format::now;
use crate::constants::{APPLET_IDLE_REFRESH as IDLE_REFRESH, APPLET_REFRESH as REFRESH};
use crate::debug::UI;
use crate::error_log;
use crate::fl;
use crate::run_state;
use crate::status::{self, Status};

const ID: &str = "io.github.stldave314.Stellarshot.Applet";

#[derive(Default)]
pub struct Applet {
    core: Core,
    popup: Option<Id>,
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
    TogglePopup,
    Refresh,
    StatusesLoaded(Vec<Status>),
    Open,
    /// Whether starting the window worked.
    Opened(bool),
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

/// Launch (or, with single-instance active, raise) the main window.
///
/// The window's binary path is derived from where the applet's own binary
/// is installed (`installed_path`, not a raw `current_exe`), since the
/// applet's own binary is `stellarshot-applet`: spawning it directly would
/// launch another applet instead of the window. Launched through
/// `cosmic::process::spawn`, which double-forks so the applet closing later
/// never leaves the window as a zombie, unlike a plain `Command::spawn`
/// that nothing ever `wait`s on.
async fn open_window() -> bool {
    let exe = match crate::exe::installed_path() {
        Ok(exe) => exe.with_file_name("stellarshot"),
        Err(err) => {
            error_log!(UI, "applet: failed to find the window's own binary: {err}");
            return false;
        }
    };
    if cosmic::process::spawn(Command::new(&exe)).await.is_none() {
        error_log!(UI, "applet: failed to open the window ({exe:?})");
        return false;
    }
    true
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
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
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
            Message::Open => {
                return Task::perform(open_window(), |opened| {
                    cosmic::Action::App(Message::Opened(opened))
                });
            }
            Message::Opened(opened) => {
                self.open_failed = !opened;
                return cosmic::task::message(cosmic::Action::App(Message::Refresh));
            }
            // Quitting the applet itself never touches a running backup:
            // that runs in the window's own `--run` child, unaffected by
            // whether the panel widget that reads its status is up.
            Message::Quit => return cosmic::iced::exit(),
            Message::TogglePopup => {}
            Message::Surface(action) => {
                return cosmic::task::message(cosmic::Action::Surface(action));
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let have_popup = self.popup;
        let main_window = self.core.main_window_id();
        let btn = self
            .core
            .applet
            .icon_button(icon_name(&self.statuses))
            .on_press_with_rectangle(move |offset, bounds| {
                if let Some(id) = have_popup {
                    Message::Surface(destroy_popup(id))
                } else if let Some(main_window) = main_window {
                    Message::Surface(app_popup::<Applet>(
                        |_| cosmic::surface::action::LiveSettings::default(),
                        move |state: &mut Applet| {
                            let new_id = Id::unique();
                            state.popup = Some(new_id);
                            let mut popup_settings = state.core.applet.get_popup_settings(
                                main_window,
                                new_id,
                                None,
                                None,
                                None,
                            );
                            popup_settings.positioner.anchor_rect = Rectangle {
                                x: pixels(bounds.x - offset.x),
                                y: pixels(bounds.y - offset.y),
                                width: pixels(bounds.width),
                                height: pixels(bounds.height),
                            };
                            popup_settings
                        },
                        Some(Box::new(|state: &Applet| {
                            Element::from(state.core.applet.popup_container(popup_content(state)))
                                .map(cosmic::Action::App)
                        })),
                    ))
                } else {
                    // No window to anchor a popup to (not yet, or no more):
                    // nothing to open.
                    Message::Refresh
                }
            });
        Element::from(self.core.applet.applet_tooltip::<Message>(
            btn,
            fl!("applet-tooltip"),
            self.popup.is_some(),
            Message::Surface,
            None,
        ))
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
/// attention; otherwise whichever of [`run_state::BackupStatus`]'s own
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

fn popup_content(state: &Applet) -> Element<'_, Message> {
    let spacing = cosmic::theme::active().cosmic().spacing;
    if state.statuses.is_empty() {
        let mut column = list_column().add(settings::item(
            fl!("applet-none"),
            widget::button::standard(fl!("applet-open")).on_press(Message::Open),
        ));
        if let Some(notice) = open_failed_notice(state) {
            column = column.add(notice);
        }
        return column
            .add(widget::button::standard(fl!("quit")).on_press(Message::Quit))
            .into();
    }
    let mut column = list_column();
    for status in &state.statuses {
        let backup_status = status.backup_status();
        let warning = matches!(
            backup_status,
            run_state::BackupStatus::Damaged
                | run_state::BackupStatus::Failed
                | run_state::BackupStatus::Overdue
        )
        .then(|| widget::icon::from_name(backup_status.icon()).size(14));
        column = column.add(settings::item(
            status.name.clone(),
            widget::row::with_capacity(2)
                .spacing(spacing.space_xxs)
                .align_y(Alignment::Center)
                // Said in words too: the icon alone tells a screen reader
                // nothing.
                .push_maybe(
                    warning
                        .is_some()
                        .then(|| widget::text::caption(backup_status.label())),
                )
                .push(widget::text::caption(status_text(status)))
                .push_maybe(warning),
        ));
    }
    if let Some(notice) = open_failed_notice(state) {
        column = column.add(notice);
    }
    column
        .add(widget::button::standard(fl!("applet-open")).on_press(Message::Open))
        .add(widget::button::standard(fl!("quit")).on_press(Message::Quit))
        .into()
}

/// Say so, when the last try to open the window did not work.
fn open_failed_notice(state: &Applet) -> Option<Element<'_, Message>> {
    state
        .open_failed
        .then(|| widget::text::caption(fl!("applet-open-failed")).into())
}

fn status_text(status: &Status) -> String {
    if status.running {
        return fl!("status-running");
    }
    match status.last_success {
        Some(time) => crate::app::format::local_time(time),
        None => fl!("never-backed-up"),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn status(name: &str) -> Status {
        Status {
            profile_id: name.to_owned(),
            name: name.to_owned(),
            running: false,
            last_success: None,
            failed: false,
            overdue: false,
            damaged: false,
        }
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
