// SPDX-License-Identifier: GPL-3.0-only

//! The selected repository: its snapshots, and any backup in progress.

use cosmic::{
    Apply, Element,
    iced::{
        Alignment, Length,
        alignment::{Horizontal, Vertical},
    },
    theme, widget,
};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::config::Repository;
use crate::app::format;
use crate::app::icon_cache::IconCache;
use crate::engine::{EngineError, Location, Phase, ProgressEvent, Secret, SnapshotSummary};
use crate::fl;
use crate::runner::Event;

pub struct Content {
    pub repository: Option<Repository>,
    secret: Option<Secret>,
    snapshots: Option<Vec<SnapshotSummary>>,
    backup: Option<Running>,
}

/// A write running in a child process.
struct Running {
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
}

#[derive(Debug, Clone)]
pub enum Message {
    SetRepository(Repository, Secret),
    SetSnapshots(Result<Vec<SnapshotSummary>, EngineError>),
    ReloadSnapshots,
    Delete(String),
    Select(String),
    BackupStarted,
    Backup(ChildEvent),
    CancelBackup,
    SnapshotsDeleted(ChildEvent),
}

/// Work the application runs on the content view's behalf.
pub enum Task {
    FetchSnapshots(Location, Secret),
    DeleteSnapshots(Location, Secret, Vec<String>),
    /// Something failed; `context` names what was being attempted.
    ShowError(String, EngineError),
}

impl Default for Content {
    fn default() -> Self {
        Self::new()
    }
}

impl Content {
    pub fn new() -> Self {
        Self {
            repository: None,
            secret: None,
            snapshots: None,
            backup: None,
        }
    }

    /// The unlocked repository, if any.
    pub fn unlocked(&self) -> Option<(Location, Secret)> {
        let repository = self.repository.as_ref()?;
        let secret = self.secret.clone()?;
        Some((Location::local(&repository.path), secret))
    }

    pub fn is_backing_up(&self) -> bool {
        self.backup.is_some()
    }

    /// Forget the selected repository, e.g. after it was deleted.
    pub fn clear(&mut self) {
        self.repository = None;
        self.secret = None;
        self.snapshots = None;
    }

    fn fetch(&self) -> Option<Task> {
        self.unlocked()
            .map(|(location, secret)| Task::FetchSnapshots(location, secret))
    }

    pub fn update(&mut self, message: Message) -> Vec<Task> {
        let mut tasks = vec![];
        match message {
            Message::SetRepository(repository, secret) => {
                self.snapshots = None;
                self.repository = Some(repository);
                self.secret = Some(secret);
                tasks.extend(self.fetch());
            }
            Message::SetSnapshots(Ok(snapshots)) => self.snapshots = Some(snapshots),
            Message::SetSnapshots(Err(err)) => {
                // A wrong password or a vanished drive: show why, and go back
                // to "nothing selected" rather than an endless spinner.
                self.clear();
                tasks.push(Task::ShowError(fl!("open-repo-failed"), err));
            }
            Message::ReloadSnapshots => tasks.extend(self.fetch()),
            Message::Delete(id) => {
                if let Some((location, secret)) = self.unlocked() {
                    tasks.push(Task::DeleteSnapshots(location, secret, vec![id]));
                }
            }
            // Snapshot details arrive with the restore browser; selecting a
            // snapshot must not crash the app until then.
            Message::Select(id) => {
                crate::debug_log!(crate::debug::UI, "selected snapshot {id}");
            }
            Message::BackupStarted => {
                self.backup = Some(Running {
                    handle: None,
                    progress: None,
                });
            }
            Message::Backup(event) => match event {
                ChildEvent::Started(handle) => {
                    if let Some(running) = self.backup.as_mut() {
                        running.handle = Some(handle);
                    }
                }
                ChildEvent::Event(Event::Progress { progress }) => {
                    if let Some(running) = self.backup.as_mut() {
                        running.progress = Some(progress);
                    }
                }
                ChildEvent::Event(Event::Done { .. }) => {
                    self.backup = None;
                    tasks.extend(self.fetch());
                }
                ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                    self.backup = None;
                    tasks.push(Task::ShowError(fl!("snapshot-failed"), error));
                }
            },
            Message::CancelBackup => {
                if let Some(handle) = self.backup.as_ref().and_then(|b| b.handle.as_ref()) {
                    handle.cancel();
                }
            }
            Message::SnapshotsDeleted(event) => match event {
                ChildEvent::Event(Event::Done { .. }) => tasks.extend(self.fetch()),
                ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                    tasks.push(Task::ShowError(fl!("delete-snapshot-failed"), error));
                    tasks.extend(self.fetch());
                }
                ChildEvent::Started(_) | ChildEvent::Event(Event::Progress { .. }) => {}
            },
        }
        tasks
    }

    pub fn view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let Some(ref repository) = self.repository else {
            return centred(
                "harddisk-symbolic",
                fl!("no-repository-selected"),
                fl!("no-repository-suggestion"),
            );
        };

        widget::column::with_capacity(2)
            .push(self.list_view(repository))
            .spacing(spacing.space_xxs)
            .apply(widget::container)
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }

    fn list_view<'a>(&'a self, repository: &'a Repository) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;

        let Some(ref snapshots) = self.snapshots else {
            return centred(
                "hourglass-symbolic",
                fl!("loading"),
                fl!("loading-snapshots"),
            );
        };

        let mut column = widget::column::with_capacity(3)
            .spacing(spacing.space_xxs)
            .padding(spacing.space_xxs)
            .push(self.repository_header(repository));
        if let Some(running) = &self.backup {
            column = column.push(progress_card(running));
        }

        if snapshots.is_empty() {
            return column
                .push(centred(
                    "box-outline-symbolic",
                    fl!("no-snapshots"),
                    fl!("no-snapshots-suggestion"),
                ))
                .into();
        }

        let mut section = widget::settings::section().title(fl!("snapshots"));
        for snapshot in snapshots {
            let delete_button =
                widget::button::custom(IconCache::get("user-trash-full-symbolic", 18))
                    .padding(spacing.space_xxs)
                    .class(theme::Button::Destructive)
                    .on_press_maybe(
                        (!self.is_backing_up()).then(|| Message::Delete(snapshot.id.clone())),
                    );

            let row = widget::settings::item::builder(format::local_time(snapshot.time))
                .description(fl!(
                    "snapshot-row",
                    id = snapshot.short_id().to_string(),
                    size = format::bytes(snapshot.total_bytes),
                    added = format::bytes(snapshot.data_added)
                ))
                .control(delete_button);
            section = section.add(row);
        }

        column
            .push(section)
            .apply(widget::container)
            .height(Length::Shrink)
            .apply(widget::scrollable)
            .height(Length::Fill)
            .into()
    }

    fn repository_header<'a>(&'a self, repository: &'a Repository) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;

        widget::row::with_capacity(3)
            .align_y(Alignment::Center)
            .spacing(spacing.space_s)
            .push(widget::text::title3(&repository.name).width(Length::Fill))
            .into()
    }
}

/// A backup in progress, with a way to stop it.
fn progress_card(running: &Running) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;
    let (label, fraction, detail) = match &running.progress {
        None => (fl!("progress-starting"), 0.0, String::new()),
        Some(progress) => {
            let label = match progress.phase {
                Phase::Preparing => fl!("progress-preparing"),
                Phase::BackingUp => fl!("progress-backing-up"),
                Phase::Restoring => fl!("progress-restoring"),
                Phase::Checking => fl!("progress-checking"),
            };
            let fraction = progress
                .total
                .filter(|total| *total > 0)
                .map_or(0.0, |total| progress.done as f32 / total as f32);
            let detail = match (progress.bytes, progress.total) {
                (true, Some(total)) => fl!(
                    "progress-amount",
                    done = format::bytes(progress.done),
                    total = format::bytes(total)
                ),
                (true, None) => format::bytes(progress.done),
                (false, _) => String::new(),
            };
            (label, fraction, detail)
        }
    };

    widget::column::with_capacity(4)
        .spacing(spacing.space_xxs)
        .push(widget::text::heading(label))
        .push(widget::progress_bar(0.0..=1.0, fraction))
        .push(widget::text::caption(detail))
        .push(
            widget::button::standard(fl!("cancel"))
                .on_press_maybe(running.handle.as_ref().map(|_| Message::CancelBackup)),
        )
        .apply(widget::container)
        .padding(spacing.space_s)
        .class(theme::Container::Card)
        .width(Length::Fill)
        .into()
}

/// An icon, a title and a hint, centred in the available space.
fn centred<'a>(icon: &'static str, title: String, hint: String) -> Element<'a, Message> {
    widget::container(
        widget::column::with_children(vec![
            IconCache::get(icon, 56).into(),
            widget::text::title1(title).into(),
            widget::text(hint).into(),
        ])
        .spacing(10)
        .align_x(Alignment::Center),
    )
    .align_y(Vertical::Center)
    .align_x(Horizontal::Center)
    .height(Length::Fill)
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{BackupReport, ErrorKind};
    use std::path::PathBuf;

    fn unlocked() -> Content {
        let mut content = Content::new();
        let repository = Repository {
            name: "home".into(),
            path: PathBuf::from("/backups/home"),
        };
        let tasks = content.update(Message::SetRepository(repository, Secret::new("pw")));
        assert!(matches!(tasks.as_slice(), [Task::FetchSnapshots(..)]));
        content
    }

    fn summary() -> SnapshotSummary {
        SnapshotSummary {
            id: "0123456789abcdef".into(),
            time: 0,
            paths: vec!["/home".into()],
            hostname: "host".into(),
            files_new: 1,
            files_changed: 0,
            files_unmodified: 0,
            data_added: 5,
            total_bytes: 5,
        }
    }

    #[test]
    fn a_finished_backup_reloads_the_snapshot_list() {
        let mut content = unlocked();
        content.update(Message::BackupStarted);
        assert!(content.is_backing_up());

        let tasks = content.update(Message::Backup(ChildEvent::Event(Event::Done {
            report: Some(BackupReport {
                snapshot: summary(),
            }),
        })));

        assert!(!content.is_backing_up());
        assert!(matches!(tasks.as_slice(), [Task::FetchSnapshots(..)]));
    }

    #[test]
    fn a_failed_backup_is_reported_and_cleared() {
        let mut content = unlocked();
        content.update(Message::BackupStarted);

        let error = EngineError::new(ErrorKind::Locked, "");
        let tasks = content.update(Message::Backup(ChildEvent::Event(Event::Error { error })));

        assert!(!content.is_backing_up());
        assert!(
            matches!(tasks.as_slice(), [Task::ShowError(_, err)] if err.kind == ErrorKind::Locked)
        );
    }

    #[test]
    fn a_killed_backup_is_reported_as_cancelled() {
        let mut content = unlocked();
        content.update(Message::BackupStarted);

        let tasks = content.update(Message::Backup(ChildEvent::Ended(EngineError::new(
            ErrorKind::Cancelled,
            "",
        ))));

        assert!(!content.is_backing_up());
        assert!(
            matches!(tasks.as_slice(), [Task::ShowError(_, err)] if err.kind == ErrorKind::Cancelled)
        );
    }

    #[test]
    fn a_wrong_password_returns_to_nothing_selected() {
        let mut content = unlocked();

        let tasks = content.update(Message::SetSnapshots(Err(EngineError::new(
            ErrorKind::WrongPassword,
            "",
        ))));

        assert!(content.repository.is_none());
        assert!(
            content.unlocked().is_none(),
            "the rejected password is forgotten"
        );
        assert!(matches!(tasks.as_slice(), [Task::ShowError(..)]));
    }

    #[test]
    fn deleting_needs_an_unlocked_repository() {
        let mut locked = Content::new();
        assert!(locked.update(Message::Delete("abc".into())).is_empty());

        let mut content = unlocked();
        let tasks = content.update(Message::Delete("abc".into()));
        assert!(matches!(tasks.as_slice(), [Task::DeleteSnapshots(_, _, ids)] if ids == &["abc"]));
    }
}
