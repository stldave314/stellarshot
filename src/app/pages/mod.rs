// SPDX-License-Identifier: GPL-3.0-only

//! The pages the main area can show.

pub mod empty;
pub mod help;
pub mod history;
pub mod home;
pub mod profile;
pub mod restore;
pub mod settings;

use std::time::Instant;

use cosmic::{Element, theme, widget};

use crate::app::format;
use crate::constants::STALL_NOTICE;
use crate::engine::{Phase, ProgressEvent};
use crate::fl;

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

/// When a running operation started and when its figures last moved, for
/// its progress card's running time and stall notice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    started: Instant,
    moved: Instant,
}

impl Timing {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            started: now,
            moved: now,
        }
    }

    /// `next` arrived after `previous`: the figures moved if it says
    /// anything new (bytes read or uploaded, or a new phase).
    pub(crate) fn observe(&mut self, previous: Option<&ProgressEvent>, next: &ProgressEvent) {
        let key = |p: &ProgressEvent| (p.phase, p.done, p.uploaded);
        if previous.map(key) != Some(key(next)) {
            self.moved = Instant::now();
        }
    }
}

/// The figures of a progress card, shared by a backup's page and the restore
/// page: `label`, a bar (animated while the total is not known yet, rather
/// than an empty 0% that reads as stalled), how much is done and uploaded,
/// the running time, and, once the figures have stood still for
/// [`STALL_NOTICE`], what may be holding them up; then `action`, the
/// caller's own Cancel (or why there is none).
pub(crate) fn progress_body<'a, M: Clone + 'static>(
    label: String,
    progress: Option<&ProgressEvent>,
    timing: &Timing,
    action: Element<'a, M>,
) -> Element<'a, M> {
    let spacing = theme::active().cosmic().spacing;
    let fraction = progress.and_then(|progress| {
        let total = progress.total.filter(|total| *total > 0)?;
        Some(progress.done as f32 / total as f32)
    });
    let detail = progress.map_or_else(String::new, |progress| {
        let amount = match (progress.bytes, progress.total) {
            (true, Some(total)) => fl!(
                "progress-amount",
                done = format::bytes(progress.done),
                total = format::bytes(total)
            ),
            (true, None) => format::bytes(progress.done),
            (false, _) => String::new(),
        };
        match progress
            .uploaded
            .filter(|_| progress.phase == Phase::BackingUp)
        {
            Some(uploaded) => fl!(
                "progress-uploaded",
                amount = amount,
                uploaded = format::bytes(uploaded)
            ),
            None => amount,
        }
    });
    let elapsed = fl!(
        "progress-elapsed",
        time = format::duration(timing.started.elapsed().as_secs())
    );
    let still = timing.moved.elapsed();
    let waiting = (still >= STALL_NOTICE).then(|| {
        let seconds = format::duration(still.as_secs());
        match progress.map(|progress| progress.phase) {
            None | Some(Phase::Preparing) => fl!("progress-waiting-preparing", time = seconds),
            Some(_) => fl!("progress-waiting", time = seconds),
        }
    });
    let bar: Element<'a, M> = match fraction {
        Some(fraction) => widget::progress_bar::determinate_linear(fraction).into(),
        None => widget::progress_bar::indeterminate_linear().into(),
    };
    widget::column::with_capacity(6)
        .spacing(spacing.space_xs)
        .push(widget::text::title4(label))
        .push(bar)
        .push(widget::text::caption(detail))
        .push(widget::text::caption(elapsed))
        .push_maybe(waiting.map(widget::text::caption))
        .push(action)
        .into()
}
