// SPDX-License-Identifier: GPL-3.0-only

//! One backup profile: is it safe, back it up now, its snapshots, and
//! keeping it healthy.

use std::time::Instant;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::errors;
use crate::app::format::{self, Ago};
use crate::app::wizard::retention_label;
use crate::constants::STALL_NOTICE;
use crate::engine::{
    EngineError, ErrorKind, Phase, ProgressEvent, PruneReport, Secret, SnapshotSummary,
};
use crate::fl;
use crate::profile::{Profile, Schedule};
use crate::run_state::{RunState, Stage};
use crate::runner::Event;

/// How many snapshots the page shows before "Show all".
const RECENT: usize = 5;

/// What a write in a child process is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    Backup,
    Check,
    CleanUp,
}

/// A write running in a child process. Only one runs at a time.
pub struct Running {
    work: Work,
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
    started: Instant,
    /// When the progress last moved: bytes read or bytes uploaded.
    moved: Instant,
}

impl Running {
    fn new(work: Work) -> Self {
        Self {
            work,
            handle: None,
            progress: None,
            started: Instant::now(),
            moved: Instant::now(),
        }
    }

    fn update(&mut self, progress: ProgressEvent) {
        let key = |p: &ProgressEvent| (p.phase, p.done, p.uploaded);
        if self.progress.as_ref().map(key) != Some(key(&progress)) {
            self.moved = Instant::now();
        }
        self.progress = Some(progress);
    }
}

/// Everything the page knows beyond the profile's saved settings.
pub struct ProfileState {
    secret: Option<Secret>,
    keyring_checked: bool,
    unlock_password: String,
    unlock_remember: bool,
    unlocking: bool,
    snapshots: Option<Vec<SnapshotSummary>>,
    work: Option<Running>,
    show_all: bool,
    /// Open the restore page as soon as the backup is unlocked: the
    /// desktop entry's "Restore Files" action.
    pub restore_when_unlocked: bool,
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
    CheckNow,
    Checked(ChildEvent),
    CleanUpNow,
    CleanedUp(ChildEvent),
    EditSchedule,
    DeleteSnapshot(String),
    SnapshotsDeleted(ChildEvent),
    ShowAll,
    Restore,
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
    /// Check the repository for damage.
    Check(Secret),
    /// A check finished: `Ok` if sound, the error if it found damage or
    /// could not run.
    RecordCheck(Result<(), EngineError>),
    /// Forget by the retention policy and prune.
    CleanUp(Secret),
    /// A clean-up finished: snapshots forgotten and space freed.
    CleanedUp {
        forgotten: u64,
        freed: u64,
    },
    OpenRestore(Secret),
    Edit,
    EditSchedule,
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
            work: None,
            show_all: false,
            restore_when_unlocked: false,
        }
    }
}

impl ProfileState {
    pub fn new() -> Self {
        Self::default()
    }

    /// A backup, check or clean-up is running.
    pub fn is_busy(&self) -> bool {
        self.work.is_some()
    }

    /// Start `work` in a child process, if nothing else is running and the
    /// backup is unlocked.
    fn start(&mut self, work: Work) -> Option<Secret> {
        let secret = self.secret.clone().filter(|_| self.work.is_none())?;
        self.work = Some(Running::new(work));
        Some(secret)
    }

    pub fn is_unlocked(&self) -> bool {
        self.secret.is_some()
    }

    fn has_snapshots(&self) -> bool {
        self.snapshots.as_ref().is_some_and(|s| !s.is_empty())
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
        if profile.sources.is_empty() {
            return Vec::new();
        }
        self.start(Work::Backup)
            .map(Effect::BackUp)
            .into_iter()
            .collect()
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
                        if std::mem::take(&mut self.restore_when_unlocked) {
                            return self.update(Message::Restore, profile);
                        }
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
                // Pruning deletes data as it goes and is not stopped halfway.
                if let Some(running) = self.work.as_ref().filter(|w| w.work != Work::CleanUp)
                    && let Some(handle) = &running.handle
                {
                    handle.cancel();
                }
                Vec::new()
            }
            Message::CheckNow => self
                .start(Work::Check)
                .map(Effect::Check)
                .into_iter()
                .collect(),
            Message::Checked(event) => self.on_work(event, |outcome| match outcome {
                Ok(_) => vec![Effect::RecordCheck(Ok(()))],
                Err(error) => vec![Effect::RecordCheck(Err(error))],
            }),
            Message::CleanUpNow => self
                .start(Work::CleanUp)
                .map(Effect::CleanUp)
                .into_iter()
                .collect(),
            Message::CleanedUp(event) => {
                let mut effects = self.on_work(event, |outcome| match outcome {
                    Ok(Event::Done {
                        forgotten, pruned, ..
                    }) => vec![Effect::CleanedUp {
                        forgotten: forgotten.map_or(0, |report| report.removed),
                        freed: pruned.map_or(0, |PruneReport { bytes }| bytes),
                    }],
                    Ok(_) => Vec::new(),
                    Err(error) => vec![Effect::ShowError(fl!("clean-up-failed"), error)],
                });
                effects.extend(self.fetch());
                effects
            }
            Message::EditSchedule => vec![Effect::EditSchedule],
            Message::DeleteSnapshot(id) => match &self.secret {
                Some(secret) if self.work.is_none() => {
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
            Message::Restore => match &self.secret {
                Some(secret) if self.has_snapshots() && self.work.is_none() => {
                    vec![Effect::OpenRestore(secret.clone())]
                }
                _ => Vec::new(),
            },
            Message::Edit => vec![Effect::Edit],
            Message::Remove => vec![Effect::Remove],
            Message::DeleteAll => vec![Effect::DeleteAll],
        }
    }

    fn fetch(&self) -> Vec<Effect> {
        self.secret.clone().map(Effect::Fetch).into_iter().collect()
    }

    /// Follow a running child: record its handle and progress, and when it
    /// ends, clear the work and let `finished` decide what follows from the
    /// final `Done` event or the error.
    fn on_work(
        &mut self,
        event: ChildEvent,
        finished: impl FnOnce(Result<Event, EngineError>) -> Vec<Effect>,
    ) -> Vec<Effect> {
        match event {
            ChildEvent::Started(handle) => {
                if let Some(running) = self.work.as_mut() {
                    running.handle = Some(handle);
                }
                Vec::new()
            }
            ChildEvent::Event(Event::Progress { progress }) => {
                if let Some(running) = self.work.as_mut() {
                    running.update(progress);
                }
                Vec::new()
            }
            ChildEvent::Event(done @ Event::Done { .. }) => {
                self.work = None;
                finished(Ok(done))
            }
            ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                self.work = None;
                finished(Err(error))
            }
        }
    }

    fn on_backup(&mut self, event: ChildEvent) -> Vec<Effect> {
        let fetch = self.fetch();
        self.on_work(event, |outcome| match outcome {
            Ok(Event::Done { report, .. }) => {
                let mut effects = fetch;
                if let Some(report) = report {
                    effects.push(Effect::RecordSuccess(report.snapshot.time));
                }
                effects
            }
            Ok(_) => Vec::new(),
            Err(error) => vec![Effect::ShowError(fl!("snapshot-failed"), error)],
        })
    }

    pub fn view<'a>(
        &'a self,
        profile: &'a Profile,
        run: &RunState,
        now: i64,
    ) -> Element<'a, Message> {
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

        if let Some(banner) = trouble(profile, run, now, self.can_work()) {
            page = page.push(banner);
        }
        page = page.push(self.status_card(profile, now));

        if !self.is_unlocked() {
            page = page.push(self.unlock_card());
        } else if let Some(snapshots) = &self.snapshots {
            page = page.push(self.snapshot_list(snapshots));
        }

        page = page.push(
            widget::settings::section()
                .title(fl!("manage"))
                .add(
                    widget::settings::item::builder(fl!("schedule-row"))
                        .description(format!(
                            "{} · {}",
                            schedule_summary(profile.schedule),
                            retention_label(profile.retention)
                        ))
                        .control(
                            widget::button::standard(fl!("change")).on_press(Message::EditSchedule),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("check-row"))
                        .description(match run.last_check {
                            Some(time) => {
                                fl!("check-row-last", when = format::local_time(time))
                            }
                            None => fl!("check-row-never"),
                        })
                        .control(
                            widget::button::standard(fl!("check-now"))
                                .on_press_maybe(self.can_work().then_some(Message::CheckNow)),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("clean-up-row"))
                        .description(fl!("clean-up-row-description"))
                        .control(
                            widget::button::standard(fl!("clean-up-now"))
                                .on_press_maybe(self.can_work().then_some(Message::CleanUpNow)),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("edit-backup"))
                        .description(fl!("edit-backup-description"))
                        .control(widget::button::standard(fl!("edit")).on_press(Message::Edit)),
                )
                .add(
                    widget::settings::item::builder(fl!("remove-backup"))
                        .description(fl!("remove-backup-description"))
                        .control(
                            widget::button::standard(fl!("remove"))
                                .on_press_maybe((!self.is_busy()).then_some(Message::Remove)),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("delete-backup"))
                        .description(fl!("delete-backup-description"))
                        .control(
                            widget::button::destructive(fl!("delete"))
                                .on_press_maybe((!self.is_busy()).then_some(Message::DeleteAll)),
                        ),
                ),
        );

        widget::scrollable(page.apply(widget::container).max_width(900))
            .height(Length::Fill)
            .into()
    }

    /// Unlocked, and nothing else running.
    fn can_work(&self) -> bool {
        self.is_unlocked() && self.work.is_none()
    }

    fn status_card<'a>(&'a self, profile: &'a Profile, now: i64) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        if let Some(running) = &self.work {
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
            widget::column::with_capacity(4)
                .spacing(spacing.space_xs)
                .push(widget::text::title4(headline))
                .push(widget::text::caption(detail))
                .push(widget::text::caption(schedule_summary(profile.schedule)))
                .push(
                    widget::row::with_capacity(2)
                        .spacing(spacing.space_xs)
                        .push(
                            widget::button::suggested(fl!("back-up-now"))
                                .on_press_maybe(can_back_up.then_some(Message::BackUpNow)),
                        )
                        .push(
                            widget::button::standard(fl!("restore-open")).on_press_maybe(
                                (self.is_unlocked() && self.has_snapshots())
                                    .then_some(Message::Restore),
                            ),
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
                    (!self.is_busy()).then(|| Message::DeleteSnapshot(snapshot.id.clone())),
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

/// How often a backup runs, as a sentence.
pub fn schedule_summary(schedule: Schedule) -> String {
    match schedule {
        Schedule::Manual => fl!("schedule-manual"),
        Schedule::Hourly => fl!("schedule-hourly"),
        Schedule::Daily => fl!("schedule-daily"),
        Schedule::Weekly => fl!("schedule-weekly"),
    }
}

/// A card about a scheduled run that failed, or damage a check found.
fn trouble<'a>(
    profile: &Profile,
    run: &RunState,
    now: i64,
    can_work: bool,
) -> Option<Element<'a, Message>> {
    let spacing = theme::active().cosmic().spacing;
    let (title, body, action) = if run.damaged {
        (
            fl!("damaged-title"),
            fl!("damaged-body"),
            Some((fl!("check-again"), Message::CheckNow)),
        )
    } else {
        let failure = run.current_failure(profile.last_success)?;
        let when = match format::ago(now, failure.time) {
            Ago::JustNow => fl!("failed-just-now"),
            Ago::Minutes(count) => fl!("failed-minutes-ago", count = count),
            Ago::Hours(count) => fl!("failed-hours-ago", count = count),
            Ago::Days(count) => fl!("failed-days-ago", count = count),
        };
        let title = match failure.stage {
            Stage::Backup => fl!("scheduled-backup-failed", when = when),
            Stage::Cleanup => fl!("scheduled-cleanup-failed", when = when),
            Stage::Check => fl!("scheduled-check-failed", when = when),
        };
        let action = match failure.stage {
            Stage::Backup => (fl!("back-up-now"), Message::BackUpNow),
            Stage::Cleanup => (fl!("clean-up-now"), Message::CleanUpNow),
            Stage::Check => (fl!("check-now"), Message::CheckNow),
        };
        (title, errors::explain(&failure.error()), Some(action))
    };
    let mut column = widget::column::with_capacity(3)
        .spacing(spacing.space_xs)
        .push(
            widget::row::with_capacity(2)
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center)
                .push(widget::icon::from_name("dialog-warning-symbolic").size(20))
                .push(widget::text::title4(title)),
        )
        .push(widget::text::body(body));
    if let Some((label, message)) = action {
        column = column
            .push(widget::button::standard(label).on_press_maybe(can_work.then_some(message)));
    }
    Some(card(column))
}

/// A backup, check or clean-up in progress, with a way to stop it where
/// stopping is safe. The running time counts up every second, and when the
/// figures have not moved for [`STALL_NOTICE`] the card says why they may
/// not, so a slow destination never looks like a hung backup.
fn progress(running: &Running) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;
    let (label, fraction, detail) = match &running.progress {
        None => (fl!("progress-starting"), 0.0, String::new()),
        Some(progress) => {
            let label = match (running.work, progress.phase) {
                (Work::CleanUp, _) => fl!("progress-cleaning-up"),
                (Work::Check, _) | (_, Phase::Checking) => fl!("progress-checking"),
                (_, Phase::Preparing) => fl!("progress-preparing"),
                (_, Phase::BackingUp) => fl!("progress-backing-up"),
                (_, Phase::Restoring) => fl!("progress-restoring"),
            };
            let fraction = progress
                .total
                .filter(|total| *total > 0)
                .map_or(0.0, |total| progress.done as f32 / total as f32);
            let amount = match (progress.bytes, progress.total) {
                (true, Some(total)) => fl!(
                    "progress-amount",
                    done = format::bytes(progress.done),
                    total = format::bytes(total)
                ),
                (true, None) => format::bytes(progress.done),
                (false, _) => String::new(),
            };
            let detail = match progress
                .uploaded
                .filter(|_| progress.phase == Phase::BackingUp)
            {
                Some(uploaded) => fl!(
                    "progress-uploaded",
                    amount = amount,
                    uploaded = format::bytes(uploaded)
                ),
                None => amount,
            };
            (label, fraction, detail)
        }
    };

    let elapsed = fl!(
        "progress-elapsed",
        time = format::duration(running.started.elapsed().as_secs())
    );
    let still = running.moved.elapsed();
    let waiting = (still >= STALL_NOTICE).then(|| {
        let seconds = format::duration(still.as_secs());
        match running.progress.as_ref().map(|progress| progress.phase) {
            None | Some(Phase::Preparing) => fl!("progress-waiting-preparing", time = seconds),
            Some(_) => fl!("progress-waiting", time = seconds),
        }
    });

    widget::column::with_capacity(6)
        .spacing(spacing.space_xs)
        .push(widget::text::title4(label))
        .push(widget::progress_bar::determinate_linear(fraction))
        .push(widget::text::caption(detail))
        .push(widget::text::caption(elapsed))
        .push_maybe(waiting.map(widget::text::caption))
        .push(if running.work == Work::CleanUp {
            // Pruning deletes data as it goes; it is left to finish.
            widget::text::caption(fl!("clean-up-cannot-stop")).into()
        } else {
            Element::from(
                widget::button::standard(fl!("cancel"))
                    .on_press_maybe(running.handle.as_ref().map(|_| Message::CancelBackup)),
            )
        })
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
                restored: None,
                forgotten: None,
                pruned: None,
            })),
            &profile(),
        );

        assert!(!state.is_busy());
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

        assert!(!state.is_busy());
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

    #[test]
    fn restore_when_unlocked_opens_the_restore_page_once() {
        let mut state = ProfileState::new();
        state.restore_when_unlocked = true;

        let effects = state.update(
            Message::Opened(Secret::new("pw"), Ok(vec![summary(10)])),
            &profile(),
        );

        assert!(matches!(effects.as_slice(), [Effect::OpenRestore(_)]));
        assert!(!state.restore_when_unlocked, "only the first unlock");
    }
}
