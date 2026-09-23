// SPDX-License-Identifier: GPL-3.0-only

//! One backup profile: is it safe, back it up now, and its snapshots.

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::format::{self, Ago};
use crate::engine::{EngineError, ErrorKind, Phase, ProgressEvent, Secret, SnapshotSummary};
use crate::fl;
use crate::profile::Profile;
use crate::runner::Event;

/// How many snapshots the page shows before "Show all".
const RECENT: usize = 5;

/// A write running in a child process.
pub struct Running {
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
}

/// Everything the page knows beyond the profile's saved settings.
pub struct ProfileState {
    secret: Option<Secret>,
    keyring_checked: bool,
    unlock_password: String,
    unlock_remember: bool,
    unlocking: bool,
    snapshots: Option<Vec<SnapshotSummary>>,
    backup: Option<Running>,
    show_all: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    KeyringLoaded(Option<Secret>),
    UnlockPassword(String),
    UnlockRemember(bool),
    Unlock,
    /// Opening with `secret` produced this snapshot list, or failed.
    Opened(Secret, Result<Vec<SnapshotSummary>, EngineError>),
    SnapshotsLoaded(Result<Vec<SnapshotSummary>, EngineError>),
    BackUpNow,
    Backup(ChildEvent),
    CancelBackup,
    DeleteSnapshot(String),
    SnapshotsDeleted(ChildEvent),
    ShowAll,
    Edit,
    Remove,
    DeleteAll,
}

/// What the page needs the application to do.
pub enum Effect {
    LoadKeyring,
    /// Open the repository with this password; remember it if asked.
    Open {
        secret: Secret,
        remember: bool,
    },
    Fetch(Secret),
    BackUp(Secret),
    DeleteSnapshots(Secret, Vec<String>),
    ShowError(String, EngineError),
    /// A backup finished at this Unix time.
    RecordSuccess(i64),
    Edit,
    Remove,
    DeleteAll,
}

impl Default for ProfileState {
    /// "Remember password" starts checked, as the design asks: scheduled
    /// backups cannot run without a remembered password.
    fn default() -> Self {
        Self {
            secret: None,
            keyring_checked: false,
            unlock_password: String::new(),
            unlock_remember: true,
            unlocking: false,
            snapshots: None,
            backup: None,
            show_all: false,
        }
    }
}

impl ProfileState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_backing_up(&self) -> bool {
        self.backup.is_some()
    }

    pub fn is_unlocked(&self) -> bool {
        self.secret.is_some()
    }

    /// Called when the page is shown: look for a remembered password once.
    pub fn activate(&mut self) -> Vec<Effect> {
        if self.keyring_checked || self.secret.is_some() {
            return Vec::new();
        }
        self.keyring_checked = true;
        vec![Effect::LoadKeyring]
    }

    /// Start a backup, if one can start.
    pub fn back_up(&mut self, profile: &Profile) -> Vec<Effect> {
        match &self.secret {
            Some(secret) if self.backup.is_none() && !profile.sources.is_empty() => {
                self.backup = Some(Running {
                    handle: None,
                    progress: None,
                });
                vec![Effect::BackUp(secret.clone())]
            }
            _ => Vec::new(),
        }
    }

    pub fn update(&mut self, message: Message, profile: &Profile) -> Vec<Effect> {
        match message {
            Message::KeyringLoaded(Some(secret)) => {
                self.unlocking = true;
                vec![Effect::Open {
                    secret,
                    remember: false,
                }]
            }
            Message::KeyringLoaded(None) => Vec::new(),
            Message::UnlockPassword(password) => {
                self.unlock_password = password;
                Vec::new()
            }
            Message::UnlockRemember(remember) => {
                self.unlock_remember = remember;
                Vec::new()
            }
            Message::Unlock => {
                if self.unlock_password.is_empty() || self.unlocking {
                    return Vec::new();
                }
                self.unlocking = true;
                vec![Effect::Open {
                    secret: Secret::new(std::mem::take(&mut self.unlock_password)),
                    remember: self.unlock_remember,
                }]
            }
            Message::Opened(secret, result) => {
                self.unlocking = false;
                match result {
                    Ok(snapshots) => {
                        self.secret = Some(secret);
                        self.snapshots = Some(snapshots);
                        Vec::new()
                    }
                    Err(err) => {
                        self.secret = None;
                        vec![Effect::ShowError(fl!("open-repo-failed"), err)]
                    }
                }
            }
            Message::SnapshotsLoaded(Ok(snapshots)) => {
                self.snapshots = Some(snapshots);
                Vec::new()
            }
            Message::SnapshotsLoaded(Err(err)) => {
                if err.kind == ErrorKind::WrongPassword {
                    self.secret = None;
                }
                vec![Effect::ShowError(fl!("open-repo-failed"), err)]
            }
            Message::BackUpNow => self.back_up(profile),
            Message::Backup(event) => self.on_backup(event),
            Message::CancelBackup => {
                if let Some(handle) = self.backup.as_ref().and_then(|b| b.handle.as_ref()) {
                    handle.cancel();
                }
                Vec::new()
            }
            Message::DeleteSnapshot(id) => match &self.secret {
                Some(secret) if self.backup.is_none() => {
                    vec![Effect::DeleteSnapshots(secret.clone(), vec![id])]
                }
                _ => Vec::new(),
            },
            Message::SnapshotsDeleted(event) => match event {
                ChildEvent::Event(Event::Done { .. }) => self.fetch(),
                ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                    let mut effects = vec![Effect::ShowError(fl!("delete-snapshot-failed"), error)];
                    effects.extend(self.fetch());
                    effects
                }
                ChildEvent::Started(_) | ChildEvent::Event(Event::Progress { .. }) => Vec::new(),
            },
            Message::ShowAll => {
                self.show_all = true;
                Vec::new()
            }
            Message::Edit => vec![Effect::Edit],
            Message::Remove => vec![Effect::Remove],
            Message::DeleteAll => vec![Effect::DeleteAll],
        }
    }

    fn fetch(&self) -> Vec<Effect> {
        self.secret.clone().map(Effect::Fetch).into_iter().collect()
    }

    fn on_backup(&mut self, event: ChildEvent) -> Vec<Effect> {
        match event {
            ChildEvent::Started(handle) => {
                if let Some(running) = self.backup.as_mut() {
                    running.handle = Some(handle);
                }
                Vec::new()
            }
            ChildEvent::Event(Event::Progress { progress }) => {
                if let Some(running) = self.backup.as_mut() {
                    running.progress = Some(progress);
                }
                Vec::new()
            }
            ChildEvent::Event(Event::Done { report }) => {
                self.backup = None;
                let mut effects = self.fetch();
                if let Some(report) = report {
                    effects.push(Effect::RecordSuccess(report.snapshot.time));
                }
                effects
            }
            ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                self.backup = None;
                vec![Effect::ShowError(fl!("snapshot-failed"), error)]
            }
        }
    }

    pub fn view<'a>(&'a self, profile: &'a Profile, now: i64) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let mut page = widget::column::with_capacity(6)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .push(widget::text::title3(&profile.name));

        if profile.sources.is_empty() {
            page = page.push(card(
                widget::column::with_capacity(3)
                    .spacing(spacing.space_xs)
                    .push(widget::text::title4(fl!("choose-what-title")))
                    .push(widget::text::body(fl!("choose-what-body")))
                    .push(
                        widget::button::suggested(fl!("choose-what-button"))
                            .on_press(Message::Edit),
                    ),
            ));
        }

        page = page.push(self.status_card(profile, now));

        if !self.is_unlocked() {
            page = page.push(self.unlock_card());
        } else if let Some(snapshots) = &self.snapshots {
            page = page.push(self.snapshot_list(snapshots));
        }

        page =
            page.push(
                widget::settings::section()
                    .title(fl!("manage"))
                    .add(
                        widget::settings::item::builder(fl!("edit-backup"))
                            .description(fl!("edit-backup-description"))
                            .control(widget::button::standard(fl!("edit")).on_press(Message::Edit)),
                    )
                    .add(
                        widget::settings::item::builder(fl!("remove-backup"))
                            .description(fl!("remove-backup-description"))
                            .control(widget::button::standard(fl!("remove")).on_press_maybe(
                                (!self.is_backing_up()).then_some(Message::Remove),
                            )),
                    )
                    .add(
                        widget::settings::item::builder(fl!("delete-backup"))
                            .description(fl!("delete-backup-description"))
                            .control(widget::button::destructive(fl!("delete")).on_press_maybe(
                                (!self.is_backing_up()).then_some(Message::DeleteAll),
                            )),
                    ),
            );

        widget::scrollable(page.apply(widget::container).max_width(900))
            .height(Length::Fill)
            .into()
    }

    fn status_card<'a>(&'a self, profile: &'a Profile, now: i64) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        if let Some(running) = &self.backup {
            return card(progress(running));
        }

        let last = self
            .snapshots
            .as_ref()
            .and_then(|snapshots| snapshots.first().map(|snapshot| snapshot.time))
            .or(profile.last_success);
        let headline = match last {
            None => fl!("never-backed-up"),
            Some(time) => match format::ago(now, time) {
                Ago::JustNow => fl!("backed-up-just-now"),
                Ago::Minutes(count) => fl!("backed-up-minutes-ago", count = count),
                Ago::Hours(count) => fl!("backed-up-hours-ago", count = count),
                Ago::Days(count) => fl!("backed-up-days-ago", count = count),
            },
        };
        let detail = match &self.snapshots {
            Some(snapshots) => {
                let count = snapshots.len() as i64;
                fl!(
                    "status-detail",
                    destination = profile.destination.describe(),
                    count = count
                )
            }
            None => profile.destination.describe(),
        };
        let can_back_up = self.is_unlocked() && !profile.sources.is_empty();

        card(
            widget::column::with_capacity(3)
                .spacing(spacing.space_xs)
                .push(widget::text::title4(headline))
                .push(widget::text::caption(detail))
                .push(
                    widget::row::with_capacity(1).push(
                        widget::button::suggested(fl!("back-up-now"))
                            .on_press_maybe(can_back_up.then_some(Message::BackUpNow)),
                    ),
                ),
        )
    }

    fn unlock_card(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let unlock =
            (!self.unlock_password.is_empty() && !self.unlocking).then_some(Message::Unlock);
        card(
            widget::column::with_capacity(4)
                .spacing(spacing.space_xs)
                .push(widget::text::title4(fl!("unlock-title")))
                .push(
                    widget::secure_input(fl!("password"), &self.unlock_password, None, true)
                        .on_input(Message::UnlockPassword)
                        .on_submit(|_| Message::Unlock),
                )
                .push(
                    widget::settings::section().add(
                        widget::settings::item::builder(fl!("remember-password"))
                            .toggler(self.unlock_remember, Message::UnlockRemember),
                    ),
                )
                .push(widget::button::suggested(fl!("unlock")).on_press_maybe(unlock)),
        )
    }

    fn snapshot_list<'a>(&'a self, snapshots: &'a [SnapshotSummary]) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        if snapshots.is_empty() {
            return widget::text::body(fl!("no-snapshots-yet")).into();
        }
        let shown = if self.show_all {
            snapshots.len()
        } else {
            RECENT.min(snapshots.len())
        };
        let mut section = widget::settings::section().title(fl!("recent-snapshots"));
        for snapshot in &snapshots[..shown] {
            let delete = widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                .padding(spacing.space_xxs)
                .on_press_maybe(
                    (!self.is_backing_up()).then(|| Message::DeleteSnapshot(snapshot.id.clone())),
                );
            section = section.add(
                widget::settings::item::builder(format::local_time(snapshot.time))
                    .description(fl!(
                        "snapshot-row",
                        id = snapshot.short_id().to_string(),
                        size = format::bytes(snapshot.total_bytes),
                        added = format::bytes(snapshot.data_added)
                    ))
                    .control(delete),
            );
        }
        let mut column = widget::column::with_capacity(2)
            .spacing(spacing.space_xs)
            .push(section);
        if shown < snapshots.len() {
            let count = snapshots.len() as i64;
            column = column.push(
                widget::button::text(fl!("show-all-snapshots", count = count))
                    .on_press(Message::ShowAll),
            );
        }
        column.into()
    }
}

/// A backup in progress, with a way to stop it.
fn progress(running: &Running) -> Element<'_, Message> {
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
        .spacing(spacing.space_xs)
        .push(widget::text::title4(label))
        .push(widget::progress_bar::determinate_linear(fraction))
        .push(widget::text::caption(detail))
        .push(
            widget::button::standard(fl!("cancel"))
                .on_press_maybe(running.handle.as_ref().map(|_| Message::CancelBackup)),
        )
        .into()
}

fn card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    widget::container(content)
        .padding(spacing.space_m)
        .class(theme::Container::Card)
        .width(Length::Fill)
        .align_y(Alignment::Start)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::BackupReport;
    use crate::profile::Destination;

    fn profile() -> Profile {
        Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/backups/home".into(),
            },
            vec!["/home/dave".into()],
        )
    }

    fn summary(time: i64) -> SnapshotSummary {
        SnapshotSummary {
            id: "0123456789abcdef".into(),
            time,
            paths: vec!["/home/dave".into()],
            hostname: "host".into(),
            files_new: 1,
            files_changed: 0,
            files_unmodified: 0,
            data_added: 5,
            total_bytes: 5,
        }
    }

    fn unlocked() -> ProfileState {
        let mut state = ProfileState::new();
        state.update(
            Message::Opened(Secret::new("pw"), Ok(vec![summary(10)])),
            &profile(),
        );
        state
    }

    #[test]
    fn the_keyring_is_consulted_once() {
        let mut state = ProfileState::new();
        assert!(matches!(state.activate().as_slice(), [Effect::LoadKeyring]));
        assert!(state.activate().is_empty());
    }

    #[test]
    fn a_remembered_password_opens_without_asking_to_store_it_again() {
        let mut state = ProfileState::new();
        let effects = state.update(Message::KeyringLoaded(Some(Secret::new("pw"))), &profile());
        assert!(matches!(
            effects.as_slice(),
            [Effect::Open {
                remember: false,
                ..
            }]
        ));
    }

    #[test]
    fn unlocking_uses_the_typed_password_and_clears_the_field() {
        let mut state = ProfileState::new();
        state.update(Message::UnlockPassword("typed".into()), &profile());

        let effects = state.update(Message::Unlock, &profile());

        match effects.as_slice() {
            [Effect::Open { secret, remember }] => {
                assert_eq!(secret.expose(), "typed");
                assert!(remember, "remember is on by default");
            }
            _ => panic!("expected an open effect"),
        }
        assert!(
            state.unlock_password.is_empty(),
            "the field does not keep the password"
        );
    }

    #[test]
    fn a_wrong_password_stays_locked_and_is_reported() {
        let mut state = ProfileState::new();
        let effects = state.update(
            Message::Opened(
                Secret::new("wrong"),
                Err(EngineError::new(ErrorKind::WrongPassword, "")),
            ),
            &profile(),
        );
        assert!(!state.is_unlocked());
        assert!(matches!(effects.as_slice(), [Effect::ShowError(..)]));
    }

    #[test]
    fn back_up_now_needs_a_password_and_sources() {
        let mut locked = ProfileState::new();
        assert!(locked.back_up(&profile()).is_empty());

        let mut state = unlocked();
        let mut empty = profile();
        empty.sources.clear();
        assert!(state.back_up(&empty).is_empty(), "nothing to back up");

        assert!(matches!(
            state.back_up(&profile()).as_slice(),
            [Effect::BackUp(_)]
        ));
        assert!(state.back_up(&profile()).is_empty(), "one backup at a time");
    }

    #[test]
    fn a_finished_backup_records_success_and_reloads() {
        let mut state = unlocked();
        state.back_up(&profile());

        let effects = state.update(
            Message::Backup(ChildEvent::Event(Event::Done {
                report: Some(BackupReport {
                    snapshot: summary(99),
                }),
            })),
            &profile(),
        );

        assert!(!state.is_backing_up());
        assert!(effects.iter().any(|e| matches!(e, Effect::Fetch(_))));
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::RecordSuccess(99)))
        );
    }

    #[test]
    fn a_cancelled_backup_is_reported_and_cleared() {
        let mut state = unlocked();
        state.back_up(&profile());

        let effects = state.update(
            Message::Backup(ChildEvent::Ended(EngineError::new(
                ErrorKind::Cancelled,
                "",
            ))),
            &profile(),
        );

        assert!(!state.is_backing_up());
        assert!(
            matches!(effects.as_slice(), [Effect::ShowError(_, err)] if err.kind == ErrorKind::Cancelled)
        );
    }

    #[test]
    fn deleting_a_snapshot_waits_for_a_running_backup() {
        let mut state = unlocked();
        state.back_up(&profile());
        assert!(
            state
                .update(Message::DeleteSnapshot("abc".into()), &profile())
                .is_empty()
        );
    }
}
