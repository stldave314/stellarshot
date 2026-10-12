// SPDX-License-Identifier: GPL-3.0-only

//! The Snapshots page of one backup: every snapshot, filtered by what a
//! person called it, and one opened to name it, note it, tag it, pin it,
//! browse it or delete it.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::text_editor;
use cosmic::{Element, theme, widget};

use super::{Effect, Message as PageMessage, ModifyContext, ProfileState, card};
use crate::app::format;
use crate::engine::{SnapshotDetails, SnapshotSummary};
use crate::fl;
use crate::profile::Profile;

/// What happens on the Snapshots page.
#[derive(Debug, Clone)]
pub enum Message {
    /// Show the page.
    Open,
    /// Back to the backup's own page.
    Close,
    Filter(String),
    /// Open one snapshot, by ID.
    Show(String),
    /// Back from one snapshot to the list.
    Back,
    Name(String),
    Notes(text_editor::Action),
    Tags(String),
    Save,
    Revert,
}

/// The page's own state, while it is showing.
#[derive(Debug, Default)]
pub(super) struct Manage {
    filter: String,
    open: Option<Editing>,
}

/// One snapshot opened, with its details as being edited.
#[derive(Debug)]
struct Editing {
    id: String,
    name: String,
    notes: text_editor::Content<cosmic::Renderer>,
    tags: String,
}

impl Editing {
    fn of(snapshot: &SnapshotSummary) -> Self {
        Self {
            id: snapshot.id.clone(),
            name: snapshot.details.name.clone(),
            notes: text_editor::Content::with_text(&snapshot.details.notes),
            tags: snapshot.details.tags.join(", "),
        }
    }

    /// The details as they would be saved.
    fn details(&self) -> SnapshotDetails {
        SnapshotDetails::from_input(&self.name, &self.notes.text(), &self.tags)
    }
}

/// What a snapshot is called in a list: its name, or when it was taken.
pub(super) fn title(snapshot: &SnapshotSummary) -> String {
    if snapshot.details.name.is_empty() {
        format::local_time(snapshot.time)
    } else {
        snapshot.details.name.clone()
    }
}

impl ProfileState {
    pub(super) fn on_manage(&mut self, message: Message) -> Vec<Effect> {
        if let Message::Open = message {
            self.manage = Some(Manage::default());
            return Vec::new();
        }
        let Some(manage) = self.manage.as_mut() else {
            return Vec::new();
        };
        match message {
            Message::Open => {}
            Message::Close => self.manage = None,
            Message::Filter(filter) => manage.filter = filter,
            Message::Show(id) => {
                manage.open = self
                    .snapshots
                    .iter()
                    .flatten()
                    .find(|snapshot| snapshot.id == id)
                    .map(Editing::of);
            }
            Message::Back => manage.open = None,
            Message::Name(name) => {
                if let Some(editing) = manage.open.as_mut() {
                    editing.name = name;
                }
            }
            Message::Notes(action) => {
                if let Some(editing) = manage.open.as_mut() {
                    editing.notes.perform(action);
                }
            }
            Message::Tags(tags) => {
                if let Some(editing) = manage.open.as_mut() {
                    editing.tags = tags;
                }
            }
            Message::Revert => {
                if let Some(editing) = manage.open.as_mut()
                    && let Some(snapshot) = self
                        .snapshots
                        .iter()
                        .flatten()
                        .find(|snapshot| snapshot.id == editing.id)
                {
                    *editing = Editing::of(snapshot);
                }
            }
            Message::Save => {
                let Some(editing) = manage.open.as_ref() else {
                    return Vec::new();
                };
                let (id, details) = (editing.id.clone(), editing.details());
                // Checked here too, not only by the button: an unchanged
                // save would still log a change that never happened.
                let unchanged = self
                    .snapshots
                    .iter()
                    .flatten()
                    .any(|snapshot| snapshot.id == id && snapshot.details == details);
                if unchanged {
                    return Vec::new();
                }
                return self
                    .start_modify(ModifyContext::Details(id.clone()))
                    .map(|secret| Effect::SetDetails(secret, id, details))
                    .into_iter()
                    .collect();
            }
        }
        Vec::new()
    }

    /// Escape on the Snapshots page: back from a snapshot to the list, or
    /// from the list to the backup's own page. `None` when it is not
    /// showing, so Escape means nothing here.
    pub fn escape(&mut self) -> Option<Vec<Effect>> {
        let manage = self.manage.as_ref()?;
        let back = if manage.open.is_some() {
            Message::Back
        } else {
            Message::Close
        };
        Some(self.on_manage(back))
    }

    /// A snapshot that was open is now saved under `new` (it was changed,
    /// so its ID changed): keep it open, as it was just saved.
    pub(super) fn follow(&mut self, old: &str, new: &SnapshotSummary) {
        if let Some(manage) = self.manage.as_mut()
            && let Some(editing) = manage.open.as_mut()
            && editing.id == old
        {
            *editing = Editing::of(new);
        }
    }

    /// Close an open snapshot that is no longer in the list (deleted, or
    /// rewritten under a new ID by removing files from it).
    pub(super) fn forget_missing(&mut self) {
        let snapshots = self.snapshots.as_deref().unwrap_or_default();
        if let Some(manage) = self.manage.as_mut()
            && let Some(editing) = &manage.open
            && !snapshots.iter().any(|snapshot| snapshot.id == editing.id)
        {
            manage.open = None;
        }
    }

    pub(super) fn manage_view<'a>(
        &'a self,
        manage: &'a Manage,
        profile: &'a Profile,
    ) -> Element<'a, PageMessage> {
        let spacing = theme::active().cosmic().spacing;
        let snapshots = self.snapshots.as_deref().unwrap_or_default();
        let open = manage.open.as_ref().and_then(|editing| {
            snapshots
                .iter()
                .find(|snapshot| snapshot.id == editing.id)
                .map(|snapshot| (editing, snapshot))
        });
        let (back, title) = match open {
            Some((_, snapshot)) => (Message::Back, title(snapshot)),
            None => (
                Message::Close,
                fl!("snapshots-title", name = profile.name.as_str()),
            ),
        };
        let header = widget::row::with_capacity(2)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(
                widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                    .tooltip(fl!("back"))
                    .name(fl!("back"))
                    .on_press(PageMessage::Manage(back)),
            )
            .push(widget::text::title3(title));
        let body = if self.is_unlocked() {
            match open {
                Some((editing, snapshot)) => {
                    self.snapshot_view(editing, snapshot, profile.append_only)
                }
                None => self.list_view(manage, snapshots),
            }
        } else {
            self.unlock_card()
        };
        widget::column::with_capacity(2)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .push(header)
            .push(body)
            .into()
    }

    fn list_view<'a>(
        &'a self,
        manage: &'a Manage,
        snapshots: &'a [SnapshotSummary],
    ) -> Element<'a, PageMessage> {
        let spacing = theme::active().cosmic().spacing;
        let query = manage.filter.trim().to_lowercase();
        let mut section = widget::settings::section();
        let mut shown = 0;
        for snapshot in snapshots
            .iter()
            .filter(|snapshot| query.is_empty() || snapshot.details.matches(&query))
        {
            shown += 1;
            let id = snapshot.id.clone();
            let browse = widget::button::icon(widget::icon::from_name("folder-open-symbolic"))
                .padding(spacing.space_xxs)
                .tooltip(fl!("browse-snapshot"))
                .name(fl!("browse-snapshot"))
                .on_press_maybe(
                    self.can_work()
                        .then(|| PageMessage::BrowseSnapshot(id.clone())),
                );
            let details = widget::button::icon(widget::icon::from_name("go-next-symbolic"))
                .padding(spacing.space_xxs)
                .tooltip(fl!("snapshot-open"))
                .name(fl!("snapshot-open"))
                .on_press(PageMessage::Manage(Message::Show(id)));
            section = section.add(
                widget::settings::item::builder(title(snapshot))
                    .description(row_description(snapshot, true))
                    .control(
                        widget::row::with_capacity(2)
                            .spacing(spacing.space_xxs)
                            .push(browse)
                            .push(details),
                    ),
            );
        }
        let list: Element<'a, PageMessage> = if shown == 0 {
            widget::text::body(fl!("snapshots-no-match")).into()
        } else {
            section.into()
        };
        widget::column::with_capacity(3)
            .spacing(spacing.space_s)
            .push(widget::text::caption(fl!("snapshots-explained")))
            .push(
                widget::search_input(fl!("snapshots-filter"), &manage.filter)
                    .on_input(|filter| PageMessage::Manage(Message::Filter(filter)))
                    .width(Length::Fill),
            )
            .push(list)
            .into()
    }

    fn snapshot_view<'a>(
        &'a self,
        editing: &'a Editing,
        snapshot: &'a SnapshotSummary,
        append_only: bool,
    ) -> Element<'a, PageMessage> {
        let spacing = theme::active().cosmic().spacing;
        // rustic refuses to rewrite a snapshot in an append-only repository,
        // and every change here is a rewrite.
        let can_modify = !self.is_busy() && !append_only;
        let changed = editing.details() != snapshot.details;
        let manage = |message: Message| PageMessage::Manage(message);

        let notes = text_editor::text_editor(&editing.notes)
            .placeholder(fl!("snapshot-notes-placeholder"))
            .height(Length::Fixed(96.0))
            .on_action(move |action| manage(Message::Notes(action)));
        let details = card(
            widget::column::with_capacity(6)
                .spacing(spacing.space_xs)
                .push(
                    widget::text_input(fl!("snapshot-name-placeholder"), &editing.name)
                        .label(fl!("snapshot-name"))
                        .on_input(move |name| manage(Message::Name(name))),
                )
                .push(widget::text::heading(fl!("snapshot-notes")))
                .push(notes)
                .push(
                    widget::text_input(fl!("snapshot-tags-placeholder"), &editing.tags)
                        .label(fl!("snapshot-tags"))
                        .on_input(move |tags| manage(Message::Tags(tags))),
                )
                .push(
                    widget::row::with_capacity(3)
                        .spacing(spacing.space_s)
                        .push(widget::space::horizontal())
                        .push(
                            widget::button::standard(fl!("revert"))
                                .on_press_maybe(changed.then(|| manage(Message::Revert))),
                        )
                        .push(widget::button::suggested(fl!("save")).on_press_maybe(
                            (changed && can_modify).then(|| manage(Message::Save)),
                        )),
                ),
        );

        let files = if snapshot.has_file_counts() {
            fl!(
                "snapshot-files-value",
                new = snapshot.files_new,
                changed = snapshot.files_changed,
                unchanged = snapshot.files_unmodified
            )
        } else {
            fl!("snapshot-not-recorded")
        };
        let about = widget::settings::section()
            .title(fl!("snapshot-about"))
            .add(item(
                fl!("snapshot-taken"),
                format::local_time(snapshot.time),
            ))
            .add(item(fl!("snapshot-id"), snapshot.id.clone()))
            .add(item(fl!("snapshot-computer"), snapshot.hostname.clone()))
            .add(item(fl!("snapshot-folders"), snapshot.paths.join("\n")))
            .add(item(fl!("snapshot-files"), files))
            .add_maybe(snapshot.has_file_counts().then(|| {
                item(
                    fl!("snapshot-size"),
                    fl!(
                        "snapshot-size-value",
                        size = format::bytes(snapshot.total_bytes),
                        added = format::bytes(snapshot.data_added)
                    ),
                )
            }));

        let id = snapshot.id.clone();
        let actions = widget::settings::section()
            .title(fl!("snapshot-actions"))
            .add(
                widget::settings::item::builder(fl!("snapshot-browse"))
                    .description(fl!("snapshot-browse-description"))
                    .control(
                        widget::button::standard(fl!("snapshot-browse-button")).on_press_maybe(
                            self.can_work()
                                .then(|| PageMessage::BrowseSnapshot(id.clone())),
                        ),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("snapshot-pinned"))
                    .description(fl!("snapshot-pinned-description"))
                    .control(
                        widget::toggler(snapshot.pinned).on_toggle_maybe(can_modify.then(|| {
                            let id = id.clone();
                            move |pinned| PageMessage::TogglePinned(id.clone(), pinned)
                        })),
                    ),
            )
            .add(
                widget::settings::item::builder(fl!("delete-snapshot-row"))
                    .description(fl!("snapshot-delete-description"))
                    .control(widget::button::destructive(fl!("delete")).on_press_maybe(
                        can_modify.then(|| PageMessage::DeleteSnapshot(id.clone())),
                    )),
            );

        widget::column::with_capacity(3)
            .spacing(spacing.space_m)
            .push(details)
            .push(about)
            .push(actions)
            .into()
    }
}

/// A read-only row in "About this snapshot".
fn item<'a>(label: String, value: String) -> Element<'a, PageMessage> {
    widget::settings::item::builder(label)
        .control(widget::text::body(value))
        .into()
}

/// What a snapshot's row says under its title: when it was taken (if the
/// title is its name), its ID, what changed, and with `tags`, its tags and
/// whether it is pinned.
pub(super) fn row_description(snapshot: &SnapshotSummary, tags: bool) -> String {
    let mut parts = Vec::with_capacity(4);
    if !snapshot.details.name.is_empty() {
        parts.push(format::local_time(snapshot.time));
    }
    // Without the counts there is no size to show either: both come from
    // the same record, which some snapshots never got.
    parts.push(if snapshot.has_file_counts() {
        fl!(
            "snapshot-row",
            id = snapshot.short_id().to_string(),
            size = format::bytes(snapshot.total_bytes),
            new = snapshot.files_new,
            changed = snapshot.files_changed,
            added = format::bytes(snapshot.data_added)
        )
    } else {
        snapshot.short_id().to_string()
    });
    if tags {
        if snapshot.pinned {
            parts.push(fl!("snapshot-pinned"));
        }
        if !snapshot.details.tags.is_empty() {
            parts.push(fl!(
                "snapshot-tag-list",
                tags = snapshot.details.tags.join(", ")
            ));
        }
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::child::ChildEvent;
    use crate::engine::Secret;
    use crate::event_log::EventKind;
    use crate::profile::Destination;
    use crate::runner::Event;

    fn profile() -> Profile {
        Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/backups/home".into(),
            },
            vec!["/home/alex".into()],
        )
    }

    fn snapshot(id: &str) -> SnapshotSummary {
        SnapshotSummary {
            id: id.into(),
            time: 10,
            time_subsec_ns: 0,
            paths: vec!["/home/alex".into()],
            hostname: "host".into(),
            files_new: 1,
            files_changed: 0,
            files_unmodified: 0,
            data_added: 5,
            total_bytes: 5,
            pinned: false,
            details: SnapshotDetails::default(),
        }
    }

    /// Unlocked, with snapshot `aaaa` open on the Snapshots page.
    fn opened() -> ProfileState {
        let mut state = ProfileState::new();
        state.update(
            PageMessage::Opened(Secret::new("pw"), Ok(vec![snapshot("aaaa")])),
            &profile(),
        );
        state.update(PageMessage::Manage(Message::Open), &profile());
        state.update(
            PageMessage::Manage(Message::Show("aaaa".into())),
            &profile(),
        );
        state
    }

    fn open_id(state: &ProfileState) -> Option<&str> {
        state
            .manage
            .as_ref()?
            .open
            .as_ref()
            .map(|editing| editing.id.as_str())
    }

    #[test]
    fn saving_sends_the_tidied_details() {
        let mut state = opened();
        state.update(
            PageMessage::Manage(Message::Name(" Before ".into())),
            &profile(),
        );
        state.update(
            PageMessage::Manage(Message::Tags("b, a, a".into())),
            &profile(),
        );

        let effects = state.update(PageMessage::Manage(Message::Save), &profile());

        assert!(matches!(
            effects.as_slice(),
            [Effect::SetDetails(_, id, details)]
                if id == "aaaa" && details.name == "Before" && details.tags == ["a", "b"]
        ));
        assert!(state.is_busy());
    }

    #[test]
    fn an_unchanged_snapshot_is_not_saved() {
        let mut state = opened();
        assert!(
            state
                .update(PageMessage::Manage(Message::Save), &profile())
                .is_empty()
        );
        assert!(!state.is_busy());
    }

    #[test]
    fn a_saved_snapshot_stays_open_under_its_new_id_and_is_logged() {
        let mut state = opened();
        state.update(
            PageMessage::Manage(Message::Name("Named".into())),
            &profile(),
        );
        state.update(PageMessage::Manage(Message::Save), &profile());
        let mut saved = snapshot("bbbb");
        saved.details.name = "Named".into();

        let effects = state.update(
            PageMessage::DetailsSaved(ChildEvent::Event(Event::Done {
                report: None,
                restored: None,
                forgotten: None,
                pruned: None,
                pinned: None,
                detailed: Some(Box::new(saved)),
                removed: None,
            })),
            &profile(),
        );

        assert_eq!(open_id(&state), Some("bbbb"));
        assert!(effects.iter().any(|effect| matches!(
            effect,
            Effect::LogEvent(EventKind::DetailsChanged { snapshot }) if snapshot == "aaaa"
        )));
        assert!(
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Fetch(_)))
        );
    }

    #[test]
    fn a_snapshot_gone_from_the_list_is_closed() {
        let mut state = opened();
        state.update(
            PageMessage::SnapshotsLoaded(Ok(vec![snapshot("cccc")])),
            &profile(),
        );
        assert_eq!(open_id(&state), None);
        assert!(state.manage.is_some(), "the list itself stays open");
    }

    #[test]
    fn reverting_puts_back_what_is_saved() {
        let mut state = opened();
        state.update(
            PageMessage::Manage(Message::Name("Typed".into())),
            &profile(),
        );
        state.update(PageMessage::Manage(Message::Revert), &profile());
        let editing = state.manage.as_ref().unwrap().open.as_ref().unwrap();
        assert_eq!(editing.name, "");
    }

    #[test]
    fn a_change_made_elsewhere_is_logged_and_reloaded() {
        let mut state = opened();
        let effects = state.update(
            PageMessage::ChangedElsewhere(Some(EventKind::FilesRemoved {
                snapshots: 2,
                erased: 9,
                deferred: false,
            })),
            &profile(),
        );
        assert!(matches!(
            effects.as_slice(),
            [
                Effect::LogEvent(EventKind::FilesRemoved { .. }),
                Effect::Fetch(_)
            ]
        ));
    }

    #[test]
    fn escape_steps_back_one_level_at_a_time() {
        let mut state = opened();
        assert!(state.escape().is_some());
        assert_eq!(open_id(&state), None, "first back to the list");
        assert!(state.manage.is_some());
        assert!(state.escape().is_some());
        assert!(state.manage.is_none(), "then back to the backup's page");
        assert!(state.escape().is_none(), "and then it means nothing here");
    }

    #[test]
    fn the_filter_matches_name_notes_and_tags_ignoring_case() {
        let details = SnapshotDetails::from_input("Before Upgrade", "GPU driver", "Keep");
        for query in ["before", "gpu", "keep"] {
            assert!(details.matches(query), "{query}");
        }
        assert!(!details.matches("after"));
    }
}
