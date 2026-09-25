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
    EngineError, ErrorKind, Phase, ProgressEvent, PruneReport, Secret, SnapshotSummary, Statistics,
};
use crate::event_log::EventKind;
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
    /// Pinning, unpinning or deleting a single snapshot: quick, but still a
    /// repository write, so it takes the same single-flight slot as the
    /// others rather than being clickable again mid-flight.
    Modify,
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

    /// How much of the current phase is done, once its total is known.
    fn fraction(&self) -> Option<f32> {
        let progress = self.progress.as_ref()?;
        let total = progress.total.filter(|total| *total > 0)?;
        Some(progress.done as f32 / total as f32)
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
    /// When this backup's timer will next run, from systemd. `None` until
    /// asked, or if the backup is not scheduled.
    next_run: Option<i64>,
    /// The repository's statistics, once calculated: reads every index file
    /// and lists the destination, so it waits for the user to ask.
    statistics: Option<Result<Statistics, EngineError>>,
    calculating_statistics: bool,
    /// What has happened to this backup, oldest first. Loaded once when the
    /// page opens; every later entry is added here directly, since whatever
    /// adds one already knows what it is.
    history: Vec<crate::event_log::Event>,
    history_loaded: bool,
    /// Open the restore page as soon as the backup is unlocked: the
    /// desktop entry's "Restore Files" action.
    pub restore_when_unlocked: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    KeyringLoaded(Result<Option<Secret>, EngineError>),
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
    EditHooks,
    EditPasswordCommand,
    ChangePassword,
    DeleteSnapshot(String),
    SnapshotsDeleted(ChildEvent),
    TogglePinned(String, bool),
    Pinned(ChildEvent),
    ShowAll,
    Restore,
    Edit,
    Remove,
    DeleteAll,
    /// systemd answered when this backup's timer will next run.
    NextRunLoaded(Option<i64>),
    CalculateStatistics,
    StatisticsLoaded(Result<Statistics, EngineError>),
    HistoryLoaded(Vec<crate::event_log::Event>),
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
    SetPinned(Secret, String, bool),
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
    EditHooks,
    EditPasswordCommand,
    ChangePassword,
    Remove,
    DeleteAll,
    /// Ask systemd when this backup's timer will next run.
    FetchNextRun,
    /// Read the repository's statistics: it needs the password.
    FetchStatistics(Secret),
    /// Add an entry to this backup's history.
    LogEvent(EventKind),
    /// Read this backup's history from disk: once, when the page opens.
    FetchHistory,
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
            next_run: None,
            statistics: None,
            calculating_statistics: false,
            history: Vec::new(),
            history_loaded: false,
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

    /// The password this backup is unlocked with right now, if it is.
    pub fn secret(&self) -> Option<&Secret> {
        self.secret.as_ref()
    }

    /// Replace the unlocked password in memory, after changing it: the
    /// repository itself was already reopened with the new one, so this
    /// only keeps the window's own copy from going stale for whatever
    /// operation comes next in the same session.
    pub fn set_secret(&mut self, secret: Secret) {
        self.secret = Some(secret);
    }

    /// How much of the running work is done, once its total is known: for
    /// the sidebar, which can only show progress as a number.
    pub fn progress_fraction(&self) -> Option<f32> {
        self.work.as_ref()?.fraction()
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

    /// Mirror any `LogEvent` in `effects` into the page's own copy of the
    /// history, so it shows up without waiting for a round trip back from
    /// the store the effect writes to.
    fn track_history(&mut self, effects: &[Effect]) {
        let now = format::now();
        for effect in effects {
            if let Effect::LogEvent(kind) = effect {
                self.history.push(crate::event_log::Event {
                    time: now,
                    kind: kind.clone(),
                });
            }
        }
    }

    /// Called when the page is shown: look for a remembered password once.
    pub fn activate(&mut self, profile: &Profile) -> Vec<Effect> {
        let mut effects = Vec::new();
        if !self.keyring_checked && self.secret.is_none() {
            self.keyring_checked = true;
            effects.push(Effect::LoadKeyring);
        }
        if profile.schedule != Schedule::Manual {
            effects.push(Effect::FetchNextRun);
        }
        if !self.history_loaded {
            self.history_loaded = true;
            effects.push(Effect::FetchHistory);
        }
        effects
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
            Message::KeyringLoaded(Ok(Some(secret))) => {
                self.unlocking = true;
                vec![Effect::Open {
                    secret,
                    remember: false,
                }]
            }
            Message::KeyringLoaded(Ok(None)) => Vec::new(),
            Message::KeyringLoaded(Err(error)) => {
                vec![Effect::ShowError(fl!("password-command-failed"), error)]
            }
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
            Message::Checked(event) => {
                let effects = self.on_work(event, |outcome| match outcome {
                    Ok(_) => vec![
                        Effect::RecordCheck(Ok(())),
                        Effect::LogEvent(EventKind::Checked { damaged: false }),
                    ],
                    Err(error) => {
                        let damaged = error.kind == ErrorKind::RepositoryDamaged;
                        let mut effects = vec![Effect::LogEvent(if damaged {
                            EventKind::Checked { damaged: true }
                        } else {
                            EventKind::Failed {
                                stage: crate::run_state::Stage::Check,
                                kind: error.kind,
                                detail: error.detail.clone(),
                            }
                        })];
                        effects.push(Effect::RecordCheck(Err(error)));
                        effects
                    }
                });
                self.track_history(&effects);
                effects
            }
            Message::CleanUpNow => self
                .start(Work::CleanUp)
                .map(Effect::CleanUp)
                .into_iter()
                .collect(),
            Message::CleanedUp(event) => {
                let mut effects = self.on_work(event, |outcome| match outcome {
                    Ok(Event::Done {
                        forgotten, pruned, ..
                    }) => {
                        let forgotten = forgotten.map_or(0, |report| report.removed);
                        let freed = pruned.map_or(0, |PruneReport { bytes }| bytes);
                        vec![
                            Effect::CleanedUp { forgotten, freed },
                            Effect::LogEvent(EventKind::CleanedUp { forgotten, freed }),
                        ]
                    }
                    Ok(_) => Vec::new(),
                    Err(error) => vec![
                        Effect::LogEvent(EventKind::Failed {
                            stage: crate::run_state::Stage::Cleanup,
                            kind: error.kind,
                            detail: error.detail.clone(),
                        }),
                        Effect::ShowError(fl!("clean-up-failed"), error),
                    ],
                });
                self.track_history(&effects);
                effects.extend(self.fetch());
                effects
            }
            Message::EditSchedule => vec![Effect::EditSchedule],
            Message::EditHooks => vec![Effect::EditHooks],
            Message::EditPasswordCommand => vec![Effect::EditPasswordCommand],
            Message::ChangePassword => vec![Effect::ChangePassword],
            Message::DeleteSnapshot(id) => self
                .start(Work::Modify)
                .map(|secret| Effect::DeleteSnapshots(secret, vec![id]))
                .into_iter()
                .collect(),
            Message::SnapshotsDeleted(event) => {
                let secret = self.secret.clone();
                self.on_work(event, move |outcome| {
                    let mut effects = match outcome {
                        Ok(_) => Vec::new(),
                        Err(error) => vec![Effect::ShowError(fl!("delete-snapshot-failed"), error)],
                    };
                    effects.extend(secret.map(Effect::Fetch));
                    effects
                })
            }
            Message::TogglePinned(id, pinned) => self
                .start(Work::Modify)
                .map(|secret| Effect::SetPinned(secret, id, pinned))
                .into_iter()
                .collect(),
            Message::Pinned(event) => {
                let secret = self.secret.clone();
                self.on_work(event, move |outcome| {
                    let mut effects = match outcome {
                        Ok(_) => Vec::new(),
                        Err(error) => vec![Effect::ShowError(fl!("pin-snapshot-failed"), error)],
                    };
                    effects.extend(secret.map(Effect::Fetch));
                    effects
                })
            }
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
            Message::NextRunLoaded(next_run) => {
                self.next_run = next_run;
                Vec::new()
            }
            Message::CalculateStatistics => match &self.secret {
                Some(secret) if !self.calculating_statistics => {
                    self.calculating_statistics = true;
                    vec![Effect::FetchStatistics(secret.clone())]
                }
                _ => Vec::new(),
            },
            Message::StatisticsLoaded(result) => {
                self.calculating_statistics = false;
                self.statistics = Some(result);
                Vec::new()
            }
            Message::HistoryLoaded(mut history) => {
                // Whatever this page logged itself while the read was in
                // flight goes after it, but only what the read cannot
                // already have on disk: an event a moment after everything
                // just loaded, never one at or before it.
                let since = history.last().map_or(i64::MIN, |event| event.time);
                history.extend(self.history.drain(..).filter(|event| event.time > since));
                self.history = history;
                Vec::new()
            }
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
        let effects = self.on_work(event, |outcome| match outcome {
            Ok(Event::Done { report, .. }) => {
                let mut effects = fetch;
                if let Some(report) = report {
                    effects.push(Effect::RecordSuccess(report.snapshot.time));
                    effects.push(Effect::LogEvent(EventKind::BackedUp));
                }
                effects
            }
            Ok(_) => Vec::new(),
            Err(error) if error.kind == ErrorKind::Canceled => {
                vec![Effect::ShowError(fl!("snapshot-failed"), error)]
            }
            Err(error) => vec![
                Effect::LogEvent(EventKind::Failed {
                    stage: crate::run_state::Stage::Backup,
                    kind: error.kind,
                    detail: error.detail.clone(),
                }),
                Effect::ShowError(fl!("snapshot-failed"), error),
            ],
        });
        self.track_history(&effects);
        effects
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
            page = page.push(self.snapshot_list(snapshots, profile.append_only));
        }

        page = page.push(self.summary_section(profile, run));
        page = page.push(self.statistics_section());
        if let Some(history) = self.history_section() {
            page = page.push(history);
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
                    widget::settings::item::builder(fl!("hooks-row"))
                        .description(hooks_summary(&profile.hooks))
                        .control(
                            widget::button::standard(fl!("change")).on_press(Message::EditHooks),
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
                            widget::button::standard(fl!("clean-up-now")).on_press_maybe(
                                (self.can_work() && !profile.append_only)
                                    .then_some(Message::CleanUpNow),
                            ),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("edit-backup"))
                        .description(fl!("edit-backup-description"))
                        .control(widget::button::standard(fl!("edit")).on_press(Message::Edit)),
                )
                .add(
                    widget::settings::item::builder(fl!("password-source-row"))
                        .description(if profile.password_command.is_empty() {
                            fl!("password-source-keyring")
                        } else {
                            fl!("password-source-command")
                        })
                        .control(
                            widget::button::standard(fl!("change"))
                                .on_press(Message::EditPasswordCommand),
                        ),
                )
                .add(
                    widget::settings::item::builder(fl!("change-password-row"))
                        .description(fl!("change-password-row-description"))
                        .control(
                            widget::button::standard(fl!("change")).on_press_maybe(
                                (self.secret.is_some() && !self.is_busy())
                                    .then_some(Message::ChangePassword),
                            ),
                        ),
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
                .push_maybe(self.next_run.map(|time| {
                    widget::text::caption(fl!("next-run", time = format::local_time(time)))
                }))
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

    fn snapshot_list<'a>(
        &'a self,
        snapshots: &'a [SnapshotSummary],
        append_only: bool,
    ) -> Element<'a, Message> {
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
            let pin_label = if snapshot.pinned {
                fl!("unpin-snapshot")
            } else {
                fl!("pin-snapshot")
            };
            let pinned = snapshot.pinned;
            let id = snapshot.id.clone();
            // rustic itself refuses both against an append-only repository
            // (pinning rewrites the snapshot, which needs the same
            // forget-the-old-one step as an ordinary delete); offering
            // either here would just be a button that always fails.
            let can_modify = !self.is_busy() && !append_only;
            let pin = widget::button::icon(widget::icon::from_name("pin-symbolic"))
                .padding(spacing.space_xxs)
                .selected(pinned)
                .tooltip(pin_label.clone())
                .name(pin_label)
                .on_press_maybe(can_modify.then(|| Message::TogglePinned(id.clone(), !pinned)));
            let delete = widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                .padding(spacing.space_xxs)
                .tooltip(fl!("delete-snapshot-row"))
                .name(fl!("delete-snapshot-row"))
                .on_press_maybe(can_modify.then(|| Message::DeleteSnapshot(id.clone())));
            section = section.add(
                widget::settings::item::builder(format::local_time(snapshot.time))
                    .description(fl!(
                        "snapshot-row",
                        id = snapshot.short_id().to_string(),
                        size = format::bytes(snapshot.total_bytes),
                        added = format::bytes(snapshot.data_added)
                    ))
                    .control(
                        widget::row::with_capacity(2)
                            .spacing(spacing.space_xxs)
                            .push(pin)
                            .push(delete),
                    ),
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

impl ProfileState {
    /// The folders this backup covers, and space freed by past clean-ups:
    /// what the status card does not already say.
    fn summary_section<'a>(&'a self, profile: &'a Profile, run: &RunState) -> Element<'a, Message> {
        let join = |paths: &[std::path::PathBuf]| -> String {
            if paths.is_empty() {
                fl!("summary-none")
            } else {
                paths
                    .iter()
                    .map(|path| format::path(path))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        };
        let mut section = widget::settings::section()
            .title(fl!("summary-title"))
            .add(row(fl!("summary-included"), join(&profile.sources)))
            .add(row(fl!("summary-excluded"), join(&profile.excludes)));
        if run.total_freed > 0 {
            section = section.add(row(fl!("summary-freed"), format::bytes(run.total_freed)));
        }
        section.into()
    }

    /// The most recent entries in this backup's history, newest first.
    /// `None` with nothing to show yet.
    fn history_section(&self) -> Option<Element<'_, Message>> {
        if self.history.is_empty() {
            return None;
        }
        let mut section = widget::settings::section().title(fl!("history-title"));
        for event in self.history.iter().rev().take(RECENT) {
            section = section.add(row(
                format::local_time(event.time),
                describe_event(&event.kind),
            ));
        }
        Some(section.into())
    }

    /// The repository's real size, compression ratio and reclaimable space,
    /// calculated on request since it reads every index file.
    fn statistics_section(&self) -> Element<'_, Message> {
        let mut section = widget::settings::section().title(fl!("statistics-title"));
        section = match (&self.statistics, self.calculating_statistics) {
            (_, true) => section.add(widget::text::caption(fl!("statistics-calculating"))),
            (None, false) => section.add(
                widget::settings::item::builder(fl!("statistics-description")).control(
                    widget::button::standard(fl!("statistics-calculate")).on_press_maybe(
                        (self.is_unlocked() && !self.is_busy())
                            .then_some(Message::CalculateStatistics),
                    ),
                ),
            ),
            (Some(Err(error)), false) => section.add(widget::text::caption(errors::explain(error))),
            (Some(Ok(stats)), false) => {
                let ratio = stats.compression_ratio().map_or_else(
                    || fl!("statistics-no-ratio"),
                    |ratio| format!("{ratio:.1}×"),
                );
                section = section
                    .add(row(
                        fl!("statistics-stored"),
                        format::bytes(stats.stored_bytes),
                    ))
                    .add(row(fl!("statistics-ratio"), ratio));
                if stats.reclaimable_bytes > 0 {
                    section = section.add(row(
                        fl!("statistics-reclaimable"),
                        format::bytes(stats.reclaimable_bytes),
                    ));
                }
                section.add(
                    widget::button::standard(fl!("statistics-calculate")).on_press_maybe(
                        (self.is_unlocked() && !self.is_busy())
                            .then_some(Message::CalculateStatistics),
                    ),
                )
            }
        };
        section.into()
    }
}

/// A history entry, as a sentence.
fn describe_event(kind: &EventKind) -> String {
    match kind {
        EventKind::BackedUp => fl!("event-backed-up"),
        EventKind::Failed {
            stage,
            kind,
            detail,
        } => {
            let stage = match stage {
                crate::run_state::Stage::Backup => fl!("event-stage-backup"),
                crate::run_state::Stage::Check => fl!("event-stage-check"),
                crate::run_state::Stage::Cleanup => fl!("event-stage-cleanup"),
            };
            let error = EngineError::new(*kind, detail.clone());
            fl!(
                "event-failed",
                stage = stage,
                reason = errors::explain(&error)
            )
        }
        EventKind::Skipped { kind } => {
            let error = EngineError::new(*kind, String::new());
            fl!("event-skipped", reason = errors::explain(&error))
        }
        EventKind::Checked { damaged: false } => fl!("event-checked-sound"),
        EventKind::Checked { damaged: true } => fl!("event-checked-damaged"),
        EventKind::CleanedUp { forgotten, freed } => fl!(
            "event-cleaned-up",
            count = (*forgotten as i64),
            size = format::bytes(*freed)
        ),
    }
}

fn row(title: String, detail: String) -> Element<'static, Message> {
    let spacing = theme::active().cosmic().spacing;
    widget::column::with_capacity(2)
        .spacing(spacing.space_xxxs)
        .padding([spacing.space_xxs, spacing.space_none])
        .push(widget::text::body(title))
        .push(widget::text::caption(detail))
        .into()
}

/// How often a backup runs, as a sentence.
pub fn schedule_summary(schedule: Schedule) -> String {
    match schedule {
        Schedule::Manual => fl!("schedule-manual"),
        Schedule::Hourly => fl!("schedule-hourly"),
        Schedule::Daily => fl!("schedule-daily"),
        Schedule::Weekly => fl!("schedule-weekly"),
        Schedule::OnConnect => fl!("schedule-on-connect"),
    }
}

/// How many hooks are set up, as a sentence.
fn hooks_summary(hooks: &[crate::profile::Hook]) -> String {
    let enabled = hooks.iter().filter(|hook| hook.enabled).count();
    if enabled == 0 {
        fl!("hooks-row-none")
    } else {
        fl!("hooks-row-count", count = (enabled as i64))
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
        None => (fl!("progress-starting"), None, String::new()),
        Some(progress) => {
            let label = match (running.work, progress.phase) {
                (Work::CleanUp, _) => fl!("progress-cleaning-up"),
                (Work::Check, _) | (_, Phase::Checking) => fl!("progress-checking"),
                (_, Phase::Preparing) => fl!("progress-preparing"),
                (_, Phase::BackingUp) => fl!("progress-backing-up"),
                (_, Phase::Restoring) => fl!("progress-restoring"),
            };
            let fraction = running.fraction();
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

    // The total is not known yet while rustic is still walking the sources
    // (or, for an upload-bound backup, while it waits on the destination):
    // an animated bar says something is happening, rather than sitting at
    // an empty 0%, which reads as stalled.
    let bar: Element<'_, Message> = match fraction {
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
            pinned: false,
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
        let effects = state.activate(&profile());
        assert!(
            effects.iter().any(|e| matches!(e, Effect::LoadKeyring)),
            "did not load the keyring"
        );
        let effects = state.activate(&profile());
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::LoadKeyring)),
            "the keyring is not asked for twice"
        );
    }

    #[test]
    fn a_scheduled_backup_asks_for_its_next_run() {
        let mut state = ProfileState::new();
        let mut scheduled = profile();
        scheduled.schedule = Schedule::Daily;
        let effects = state.activate(&scheduled);
        assert!(effects.iter().any(|e| matches!(e, Effect::FetchNextRun)));

        // A manual backup has no timer to ask about.
        let mut state = ProfileState::new();
        let effects = state.activate(&profile());
        assert!(!effects.iter().any(|e| matches!(e, Effect::FetchNextRun)));
    }

    #[test]
    fn statistics_are_calculated_once_per_press_while_unlocked() {
        let mut locked = ProfileState::new();
        assert!(
            locked
                .update(Message::CalculateStatistics, &profile())
                .is_empty(),
            "needs the password"
        );

        let mut state = unlocked();
        assert!(matches!(
            state
                .update(Message::CalculateStatistics, &profile())
                .as_slice(),
            [Effect::FetchStatistics(_)]
        ));
        assert!(
            state
                .update(Message::CalculateStatistics, &profile())
                .is_empty(),
            "a second press does not start another calculation"
        );

        let stats = Statistics {
            stored_bytes: 100,
            original_bytes: 300,
            packed_bytes: 100,
            reclaimable_bytes: 10,
        };
        state.update(Message::StatisticsLoaded(Ok(stats)), &profile());
        assert!(
            matches!(
                state
                    .update(Message::CalculateStatistics, &profile())
                    .as_slice(),
                [Effect::FetchStatistics(_)]
            ),
            "a finished calculation can be run again"
        );
    }

    #[test]
    fn a_remembered_password_opens_without_asking_to_store_it_again() {
        let mut state = ProfileState::new();
        let effects = state.update(
            Message::KeyringLoaded(Ok(Some(Secret::new("pw")))),
            &profile(),
        );
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
                pinned: None,
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
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::LogEvent(EventKind::BackedUp)))
        );
        assert!(
            matches!(state.history.as_slice(), [event] if event.kind == EventKind::BackedUp),
            "logged in the page's own copy too, without waiting for a round trip"
        );
    }

    #[test]
    fn a_late_history_load_does_not_duplicate_what_was_logged_while_it_ran() {
        let mut state = unlocked();
        // Something happens after the load was asked for but before it comes
        // back: the local record must survive the merge.
        state.history.push(crate::event_log::Event {
            time: 500,
            kind: EventKind::BackedUp,
        });

        let loaded = vec![crate::event_log::Event {
            time: 100,
            kind: EventKind::Checked { damaged: false },
        }];
        state.update(Message::HistoryLoaded(loaded), &profile());

        assert_eq!(state.history.len(), 2, "nothing lost, nothing duplicated");
        assert_eq!(state.history[0].time, 100);
        assert_eq!(state.history[1].time, 500);
    }

    #[test]
    fn a_cancelled_backup_is_reported_and_cleared() {
        let mut state = unlocked();
        state.back_up(&profile());

        let effects = state.update(
            Message::Backup(ChildEvent::Ended(EngineError::new(ErrorKind::Canceled, ""))),
            &profile(),
        );

        assert!(!state.is_busy());
        assert!(
            matches!(effects.as_slice(), [Effect::ShowError(_, err)] if err.kind == ErrorKind::Canceled)
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
    fn toggling_pinned_asks_the_application_to_set_it() {
        let mut state = unlocked();
        let effects = state.update(Message::TogglePinned("abc".into(), true), &profile());
        assert!(matches!(
            effects.as_slice(),
            [Effect::SetPinned(_, id, true)] if id == "abc"
        ));
    }

    #[test]
    fn toggling_pinned_waits_for_a_running_backup() {
        let mut state = unlocked();
        state.back_up(&profile());
        assert!(
            state
                .update(Message::TogglePinned("abc".into(), true), &profile())
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
