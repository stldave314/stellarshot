// SPDX-License-Identifier: GPL-3.0-only

//! The pages the main area can show.

pub mod empty;
pub mod help;
pub mod history;
pub mod home;
pub mod profile;
pub mod restore;
pub mod settings;

use cosmic::{Element, theme, widget};

/// A description-and-detail row with no interactive control, shared by the
/// home page and a backup's own page.
pub(crate) fn row<M: 'static>(title: String, detail: String) -> Element<'static, M> {
    let spacing = theme::active().cosmic().spacing;
    widget::column::with_capacity(2)
        .spacing(spacing.space_xxxs)
        .padding([spacing.space_xxs, spacing.space_none])
        .push(widget::text::body(title))
        .push(widget::text::caption(detail))
        .into()
}
