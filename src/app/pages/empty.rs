// SPDX-License-Identifier: GPL-3.0-only

//! The first screen: nothing is backed up yet.

use cosmic::iced::{Alignment, Length};
use cosmic::{Element, theme, widget};

use crate::app::Message;
use crate::fl;

/// The application's own icon, bundled so it shows even before the app is
/// installed and its icon is in the theme.
fn app_icon() -> widget::icon::Handle {
    widget::icon::from_svg_bytes(include_bytes!(
        "../../../res/icons/hicolor/scalable/apps/io.github.stldave314.Stellarshot.svg"
    ))
}

pub fn view<'a>() -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    widget::container(
        widget::column::with_capacity(5)
            .spacing(spacing.space_s)
            .align_x(Alignment::Center)
            .max_width(460)
            .push(widget::icon(app_icon()).size(96))
            .push(widget::text::title2(fl!("empty-title")))
            .push(widget::text::body(fl!("empty-body")).align_x(Alignment::Center))
            .push(widget::button::suggested(fl!("create-backup")).on_press(Message::NewBackup))
            .push(widget::button::link(fl!("open-existing")).on_press(Message::OpenExisting)),
    )
    .center(Length::Fill)
    .into()
}
