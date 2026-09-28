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

use std::process::Command;
use std::time::Duration;

use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::iced::window::Id;
use cosmic::iced::{Alignment, Rectangle, Subscription};
use cosmic::surface::action::{app_popup, destroy_popup};
use cosmic::widget::{self, list_column, settings};

use crate::app::config::StellarshotConfig;
use crate::debug::UI;
use crate::error_log;
use crate::fl;
use crate::run_state;
use crate::status::{self, Status};

const ID: &str = "io.github.stldave314.Stellarshot.Applet";
/// How often the applet re-reads every backup's status while its popup is
/// open. Cheap (a lock probe and a couple of small config reads per
/// backup), so this can be frequent without it mattering.
const REFRESH: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Applet {
    core: Core,
    popup: Option<Id>,
    statuses: Vec<Status>,
}

#[derive(Clone, Debug)]
pub enum Message {
    PopupClosed(Id),
    TogglePopup,
    Refresh,
    Open,
    Quit,
    Surface(cosmic::surface::Action<Message>),
}

fn refresh() -> Vec<Status> {
    let now = jiff::Timestamp::now().as_second();
    status::all(&StellarshotConfig::config().profiles, now)
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
async fn open_window() {
    let exe = match crate::exe::installed_path() {
        Ok(exe) => exe.with_file_name("stellarshot"),
        Err(err) => {
            error_log!(UI, "applet: failed to find the window's own binary: {err}");
            return;
        }
    };
    if cosmic::process::spawn(Command::new(&exe)).await.is_none() {
        error_log!(UI, "applet: failed to open the window ({exe:?})");
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
        crate::core::localization::init();
        let applet = Self {
            core,
            statuses: refresh(),
            ..Default::default()
        };
        (applet, Task::none())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> Subscription<Message> {
        cosmic::iced::time::every(REFRESH).map(|_| Message::Refresh)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                }
            }
            Message::Refresh => self.statuses = refresh(),
            Message::Open => {
                return Task::perform(open_window(), |()| cosmic::Action::App(Message::Refresh));
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
        let btn = self
            .core
            .applet
            .icon_button(icon_name(&self.statuses))
            .on_press_with_rectangle(move |offset, bounds| {
                if let Some(id) = have_popup {
                    Message::Surface(destroy_popup(id))
                } else {
                    Message::Surface(app_popup::<Applet>(
                        |_| Default::default(),
                        move |state: &mut Applet| {
                            let new_id = Id::unique();
                            state.popup = Some(new_id);
                            let mut popup_settings = state.core.applet.get_popup_settings(
                                state.core.main_window_id().unwrap(),
                                new_id,
                                None,
                                None,
                                None,
                            );
                            popup_settings.positioner.anchor_rect = Rectangle {
                                x: (bounds.x - offset.x) as i32,
                                y: (bounds.y - offset.y) as i32,
                                width: bounds.width as i32,
                                height: bounds.height as i32,
                            };
                            popup_settings
                        },
                        Some(Box::new(|state: &Applet| {
                            Element::from(state.core.applet.popup_container(popup_content(state)))
                                .map(cosmic::Action::App)
                        })),
                    ))
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
        return list_column()
            .add(settings::item(
                fl!("applet-none"),
                widget::button::standard(fl!("applet-open")).on_press(Message::Open),
            ))
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
                .push(widget::text::caption(status_text(status)))
                .push_maybe(warning),
        ));
    }
    column
        .add(widget::button::standard(fl!("applet-open")).on_press(Message::Open))
        .add(widget::button::standard(fl!("quit")).on_press(Message::Quit))
        .into()
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
