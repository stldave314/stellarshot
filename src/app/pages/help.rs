// SPDX-License-Identifier: GPL-3.0-only

//! The help drawer.

use cosmic::iced::Alignment;
use cosmic::widget;
use cosmic::{Element, theme};

use crate::fl;
use crate::run_state;

/// What the sidebar's icons mean, and the terms a newcomer may not know.
pub fn view<M: Clone + 'static>() -> Element<'static, M> {
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
