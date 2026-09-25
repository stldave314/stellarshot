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

use std::env;
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
    Surface(cosmic::surface::Action<Message>),
}

fn refresh() -> Vec<Status> {
    let now = jiff::Timestamp::now().as_second();
    status::all(&StellarshotConfig::config().profiles, now)
}

/// Launch (or, with single-instance active, raise) the main window.
fn open_window() {
    let Ok(exe) = env::current_exe() else {
        return;
    };
    if let Err(err) = Command::new(&exe).spawn() {
        error_log!(UI, "applet: failed to open the window ({exe:?}): {err}");
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
            Message::Open => open_window(),
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
/// without opening the popup.
fn icon_name(statuses: &[Status]) -> &'static str {
    if statuses.iter().any(|status| status.running) {
        "emblem-synchronizing-symbolic"
    } else if statuses
        .iter()
        .any(|status| status.failed || status.overdue)
    {
        "dialog-warning-symbolic"
    } else {
        "io.github.stldave314.Stellarshot-symbolic"
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
            .into();
    }
    let mut column = list_column();
    for status in &state.statuses {
        let warning = (status.failed || status.overdue)
            .then(|| widget::icon::from_name("dialog-warning-symbolic").size(14));
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
