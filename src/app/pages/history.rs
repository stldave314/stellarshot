// SPDX-License-Identifier: GPL-3.0-only

//! Every backup's history in one place: what happened, when, and to which
//! backup, across every profile rather than one at a time.

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::Message;
use crate::app::format;
use crate::constants::HISTORY_LIMIT as LIMIT;
use crate::constants::PAGE_MAX_WIDTH;
use crate::event_log::{self, Event, Source};
use crate::fl;
use crate::profile::Profile;

pub fn view<'a>(entries: &'a [(String, Event)], profiles: &'a [Profile]) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;

    let mut page = widget::column::with_capacity(3)
        .spacing(spacing.space_m)
        .padding(spacing.space_m)
        .push(widget::text::title3(fl!("history-title")));

    if entries.is_empty() {
        return page.push(widget::text::body(fl!("history-empty"))).into();
    }

    if entries.len() > LIMIT {
        page = page.push(widget::text::caption(fl!(
            "history-truncated",
            shown = (LIMIT as i64),
            total = (entries.len() as i64)
        )));
    }

    let mut list =
        widget::column::with_capacity(LIMIT.min(entries.len())).spacing(spacing.space_xxxs);
    for (profile_id, event) in entries.iter().take(LIMIT) {
        let name = profiles
            .iter()
            .find(|profile| &profile.id == profile_id)
            .map_or_else(
                || fl!("history-unknown-backup"),
                |profile| profile.name.clone(),
            );
        list = list.push(row(name, event));
    }
    page.push(widget::scrollable(list).height(Length::Fill))
        .apply(widget::container)
        .max_width(PAGE_MAX_WIDTH)
        .into()
}

fn row<'a>(profile_name: String, event: &'a Event) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    let mut line = widget::row::with_capacity(4)
        .spacing(spacing.space_s)
        .align_y(Alignment::Center)
        .push(widget::text::caption(format::local_time(event.time)))
        .push(widget::text::body(profile_name))
        .push(
            widget::container(widget::text::body(event_log::describe(&event.kind)))
                .width(Length::Fill),
        );
    if event.source == Source::Other {
        line = line.push(widget::text::caption(fl!("history-via-other")));
    }
    line.into()
}
