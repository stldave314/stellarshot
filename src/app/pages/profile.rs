// SPDX-License-Identifier: GPL-3.0-only

//! One backup profile: is it safe, back it up now, its snapshots, and
//! keeping it healthy.

mod snapshots;

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::errors;
use crate::app::format;
use crate::app::pages::{Timing, progress_body, row};
use crate::app::wizard::retention_label;
use crate::constants::PROFILE_RECENT_ROWS as RECENT;
use crate::constants::{NOTICE_ICON_SIZE, PAGE_MAX_WIDTH};
use crate::engine::{
    EngineError, ErrorKind, Phase, ProgressEvent, PruneReport, Secret, SizeEstimate,
    SnapshotDetails, SnapshotSummary, Statistics,
};
use crate::event_log::EventKind;
use crate::fl;
use crate::profile::{Profile, Schedule};
use crate::run_state::{RunState, Stage};
use crate::runner::Event;

/// The unlock card's password field, shared with `app.rs` so an effect
/// there can focus the same field this page's own view sets it on.
pub fn unlock_input_id() -> widget::Id {
    widget::Id::new("unlock-password")
}

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

/// What a [`Work::Modify`] is doing, so its own completion message (which
/// carries only a `ChildEvent`, not what was asked for) can still log the
/// right [`EventKind`].
#[derive(Debug, Clone)]
enum ModifyContext {
    Delete(Vec<String>),
    Pin(String, bool),
    Details(String),
}

/// A write running in a child process. Only one runs at a time.
#[derive(Debug)]
pub struct Running {
    work: Work,
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
    timing: Timing,
    /// Set only for `Work::Modify`.
    context: Option<ModifyContext>,
}

impl Running {
    fn new(work: Work) -> Self {
        Self {
            work,
            handle: None,
            progress: None,
            timing: Timing::new(),
            context: None,
        }
    }

    fn update(&mut self, progress: ProgressEvent) {
        self.timing.observe(self.progress.as_ref(), &progress);
        self.progress = Some(progress);
    }

    /// How much of the current phase is done, once its total is known.
    fn fraction(&self) -> Option<f32> {
        let progress = self.progress.as_ref()?;
        let total = progress.total.filter(|total| *total > 0)?;
        Some(progress.done as f32 / total as f32)
    }
}

/// Progress of a local, filesystem-only size estimate ("how much would this
/// back up"), as it reaches this page. The same walk the setup wizard uses
/// to size a backup before it exists, without the exclusion-arithmetic pass
/// an already-configured backup has no use for.
#[derive(Debug, Clone)]
pub enum SizeEstimateEvent {
    Progress(SizeEstimate),
    Done(SizeEstimate),
    Failed(EngineError),
}

/// How far the page has got toward a password that opens the repository,
/// while it has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Access {
    /// Nothing asked yet: the page has not been shown since it was opened
    /// or last locked.
    #[default]
    Unchecked,
    /// Waiting on the keyring, or the password command: it can take
    /// minutes when the keyring raises its own unlock prompt.
    Fetching,
    /// Opening the repository with the remembered password.
    OpeningSaved,
    /// Opening the repository with the password just typed: the unlock
    /// card stays, so a wrong one leaves focus in its field to try again.
    OpeningTyped,
    /// Nothing remembered, or it did not open the repository: the user
    /// types one.
    Prompt,
}

/// Everything the page knows beyond the profile's saved settings.
#[derive(Debug)]
pub struct ProfileState {
    secret: Option<Secret>,
    /// Only meaningful while `secret` is `None`.
    access: Access,
    unlock_password: String,
    unlock_remember: bool,
    /// Whether the unlock field has already been focused once, the first
    /// time this page activates while still locked — a keyboard-only user
    /// landing here otherwise has to Tab or click into it themselves before
    /// they can type a password at all.
    unlock_focused: bool,
    snapshots: Option<Vec<SnapshotSummary>>,
    work: Option<Running>,
    /// The Snapshots page, while it is showing in place of this one.
    manage: Option<snapshots::Manage>,
    /// The explanation of what a snapshot is, shown under "Recent
    /// snapshots" once its info button is pressed.
    explain_snapshots: bool,
    /// When this backup's timer will next run, from systemd. `None` until
    /// asked, or if the backup is not scheduled.
    next_run: Option<i64>,
    /// The repository's statistics, once calculated: reads every index file
    /// and lists the destination, so it waits for the user to ask.
    statistics: Option<Result<Statistics, EngineError>>,
    calculating_statistics: bool,
    /// A one-off local size estimate, started by pressing "Estimate Size":
    /// no unlock or repository access needed, so it works even on a locked
    /// backup, unlike statistics above.
    estimate: Option<Result<SizeEstimate, EngineError>>,
    estimating: bool,
    /// What has happened to this backup, oldest first. Loaded once when the
    /// page opens; every later entry is added here directly, since whatever
    /// adds one already knows what it is.
    history: Vec<crate::event_log::Event>,
    /// The saved history cannot be read, so nothing new reaches it.
    history_unreadable: bool,
    history_loaded: bool,
    /// Open the restore page as soon as the backup is unlocked: the
    /// desktop entry's "Restore Files" action.
    pub restore_when_unlocked: bool,
    /// Start a backup as soon as the backup is unlocked: the panel
    /// applet's "Back Up All Now".
    pub back_up_when_unlocked: bool,
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
    /// Replace a status that could not be read.
    ResetStatus,
    CleanUpNow,
    CleanedUp(ChildEvent),
    EditSchedule,
    /// Start the scheduled run now, the way its timer would.
    RunAsScheduled,
    EditHooks,
    EditPasswordCommand,
    ChangePassword,
    DeleteSnapshot(String),
    SnapshotsDeleted(ChildEvent),
    TogglePinned(String, bool),
    Pinned(ChildEvent),
    DetailsSaved(ChildEvent),
    /// Something on the Snapshots page.
    Manage(snapshots::Message),
    /// Show or hide what a snapshot is, under "Recent snapshots".
    ExplainSnapshots,
    Restore,
    /// Open the restore page at this snapshot rather than the newest.
    BrowseSnapshot(String),
    Edit,
    Remove,
    DeleteAll,
    /// systemd answered when this backup's timer will next run.
    NextRunLoaded(Option<i64>),
    CalculateStatistics,
    StatisticsLoaded(Result<Statistics, EngineError>),
    EstimateSize,
    Estimate(SizeEstimateEvent),
    /// The history, and whether it is there but cannot be read.
    HistoryLoaded(Vec<crate::event_log::Event>, bool),
    /// The snapshots were changed from another page: reload them, and
    /// record what happened, if anything is to be.
    ChangedElsewhere(Option<EventKind>),
}

/// What the page needs the application to do.
#[derive(Debug)]
pub enum Effect {
    LoadKeyring,
    /// Move keyboard focus into the unlock field: no password was found in
    /// the keyring, so the card asking for one is about to show.
    FocusUnlock,
    /// Open the repository with this password; remember it if asked.
    Open {
        secret: Secret,
        remember: bool,
    },
    Fetch(Secret),
    BackUp(Secret),
    DeleteSnapshots(Secret, Vec<String>),
    /// The trash icon was pressed: ask for confirmation before
    /// `DeleteSnapshots` actually runs. `label` is the snapshot's own time,
    /// already formatted, so the confirmation dialog needs no lookup of
    /// its own.
    ConfirmDeleteSnapshot {
        id: String,
        label: String,
    },
    SetPinned(Secret, String, bool),
    /// Give a snapshot a new name, notes and tags.
    SetDetails(Secret, String, SnapshotDetails),
    ShowError(String, EngineError),
    /// A backup finished, taking this snapshot.
    RecordSuccess(SnapshotSummary),
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
    /// Open the restore page: at this snapshot if one is given, otherwise
    /// the newest.
    OpenRestore(Secret, Option<String>),
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
    /// Walk this backup's own sources locally and total their size: no
    /// unlock or repository access needed.
    EstimateSize(Arc<AtomicBool>),
    /// Add an entry to this backup's history.
    LogEvent(EventKind),
    /// Replace this backup's unreadable status with a fresh one.
    ResetStatus,
    /// Start this backup's scheduled run now, through systemd.
    RunAsScheduled,
    /// Read this backup's history from disk: once, when the page opens.
    FetchHistory,
}

impl Default for ProfileState {
    /// "Remember password" starts checked, as the design asks: scheduled
    /// backups cannot run without a remembered password.
    fn default() -> Self {
        Self {
            secret: None,
            access: Access::Unchecked,
            unlock_password: String::new(),
            unlock_remember: true,
            unlock_focused: false,
            snapshots: None,
            work: None,
            manage: None,
            explain_snapshots: false,
            next_run: None,
            statistics: None,
            calculating_statistics: false,
            estimate: None,
            estimating: false,
            history: Vec::new(),
            history_unreadable: false,
            history_loaded: false,
            restore_when_unlocked: false,
            back_up_when_unlocked: false,
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

    /// Start a [`Work::Modify`], remembering `context` so its completion can
    /// log what was actually asked for.
    fn start_modify(&mut self, context: ModifyContext) -> Option<Secret> {
        let secret = self.start(Work::Modify)?;
        if let Some(running) = self.work.as_mut() {
            running.context = Some(context);
        }
        Some(secret)
    }

    /// Actually delete `id`, after the app-level confirmation dialog
    /// `Message::DeleteSnapshot` opens has itself been confirmed.
    pub fn delete_snapshot_confirmed(&mut self, id: String) -> Vec<Effect> {
        let ids = vec![id];
        self.start_modify(ModifyContext::Delete(ids.clone()))
            .map(|secret| Effect::DeleteSnapshots(secret, ids))
            .into_iter()
            .collect()
    }

    pub fn is_unlocked(&self) -> bool {
        self.secret.is_some()
    }

    /// Forget the password and what it unlocked, as if the page had never
    /// been opened: for a window closed with nothing to watch over. Opening
    /// the page again asks the keyring, or the user, once more. Does nothing
    /// while work is running, which still needs it.
    pub fn lock(&mut self) {
        if self.is_busy() {
            return;
        }
        self.secret = None;
        self.snapshots = None;
        self.access = Access::Unchecked;
    }

    fn has_snapshots(&self) -> bool {
        self.snapshots.as_ref().is_some_and(|s| !s.is_empty())
    }

    fn open_restore(&self, snapshot: Option<String>) -> Vec<Effect> {
        match &self.secret {
            Some(secret) if self.has_snapshots() && self.work.is_none() => {
                vec![Effect::OpenRestore(secret.clone(), snapshot)]
            }
            _ => Vec::new(),
        }
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
                    source: crate::event_log::Source::Desktop,
                });
            }
        }
    }

    /// Called when the page is shown: look for a remembered password once.
    pub fn activate(&mut self, profile: &Profile) -> Vec<Effect> {
        let mut effects = Vec::new();
        if self.access == Access::Unchecked && self.secret.is_none() {
            self.access = Access::Fetching;
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

    /// Show the unlock card, focusing its field the first time it appears.
    fn prompt(&mut self) -> Vec<Effect> {
        self.access = Access::Prompt;
        if std::mem::replace(&mut self.unlock_focused, true) {
            Vec::new()
        } else {
            vec![Effect::FocusUnlock]
        }
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
                self.access = Access::OpeningSaved;
                vec![Effect::Open {
                    secret,
                    remember: false,
                }]
            }
            // No password remembered: only now does the unlock card show.
            // A keyring that cannot be reached is, for unlocking by hand, the
            // same as one with nothing remembered: ask for the password.
            Message::KeyringLoaded(
                Ok(None)
                | Err(EngineError {
                    kind: ErrorKind::KeyringUnavailable,
                    ..
                }),
            ) => self.prompt(),
            Message::KeyringLoaded(Err(error)) => {
                let mut effects = self.prompt();
                effects.push(Effect::ShowError(fl!("password-command-failed"), error));
                effects
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
                if self.unlock_password.is_empty() || self.access != Access::Prompt {
                    return Vec::new();
                }
                self.access = Access::OpeningTyped;
                vec![Effect::Open {
                    secret: Secret::new(std::mem::take(&mut self.unlock_password)),
                    remember: self.unlock_remember,
                }]
            }
            Message::Opened(secret, result) => match result {
                Ok(snapshots) => {
                    // Should the password stop working later, it is the
                    // user's to retype: the keyring already gave its own.
                    self.access = Access::Prompt;
                    self.secret = Some(secret);
                    self.snapshots = Some(snapshots);
                    // Restore first: it only opens a page, while a backup
                    // takes the work slot that opening it would wait on.
                    let mut effects = Vec::new();
                    if std::mem::take(&mut self.restore_when_unlocked) {
                        effects = self.update(Message::Restore, profile);
                    }
                    if std::mem::take(&mut self.back_up_when_unlocked) {
                        effects.extend(self.back_up(profile));
                    }
                    effects
                }
                Err(err) => {
                    self.secret = None;
                    let mut effects = self.prompt();
                    effects.push(Effect::ShowError(fl!("open-repo-failed"), err));
                    effects
                }
            },
            Message::SnapshotsLoaded(Ok(snapshots)) => {
                self.snapshots = Some(snapshots);
                self.forget_missing();
                Vec::new()
            }
            Message::SnapshotsLoaded(Err(err)) => {
                let mut effects = Vec::new();
                if err.kind == ErrorKind::WrongPassword {
                    self.secret = None;
                    effects = self.prompt();
                }
                effects.push(Effect::ShowError(fl!("open-repo-failed"), err));
                effects
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
            Message::ResetStatus => vec![Effect::ResetStatus],
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
            Message::RunAsScheduled => vec![Effect::RunAsScheduled],
            Message::EditHooks => vec![Effect::EditHooks],
            Message::EditPasswordCommand => vec![Effect::EditPasswordCommand],
            Message::ChangePassword => vec![Effect::ChangePassword],
            Message::DeleteSnapshot(id) => {
                // Only asks; the trash icon must not delete on one click,
                // unlike every other destructive action in the app. The
                // actual delete runs from `delete_snapshot_confirmed`,
                // called only once the app-level confirmation dialog this
                // effect opens is itself confirmed. Still checked here too,
                // not just by the button being disabled in `view`: nothing
                // is worth confirming toward an action that would just be
                // dropped anyway.
                if self.is_busy() {
                    return Vec::new();
                }
                let label = self
                    .snapshots
                    .as_ref()
                    .and_then(|snapshots| snapshots.iter().find(|s| s.id == id))
                    .map(|snapshot| format::local_time(snapshot.time))
                    .unwrap_or_default();
                vec![Effect::ConfirmDeleteSnapshot { id, label }]
            }
            Message::SnapshotsDeleted(event) => {
                let secret = self.secret.clone();
                let context = self
                    .work
                    .as_ref()
                    .and_then(|running| running.context.clone());
                let effects = self.on_work(event, move |outcome| {
                    let mut effects = match outcome {
                        Ok(_) => match context {
                            Some(ModifyContext::Delete(ids)) => ids
                                .into_iter()
                                .map(|snapshot| {
                                    Effect::LogEvent(EventKind::SnapshotDeleted { snapshot })
                                })
                                .collect(),
                            _ => Vec::new(),
                        },
                        Err(error) => vec![Effect::ShowError(fl!("delete-snapshot-failed"), error)],
                    };
                    effects.extend(secret.map(Effect::Fetch));
                    effects
                });
                self.track_history(&effects);
                effects
            }
            Message::TogglePinned(id, pinned) => self
                .start_modify(ModifyContext::Pin(id.clone(), pinned))
                .map(|secret| Effect::SetPinned(secret, id, pinned))
                .into_iter()
                .collect(),
            Message::Pinned(event) => {
                let secret = self.secret.clone();
                let context = self
                    .work
                    .as_ref()
                    .and_then(|running| running.context.clone());
                if let (
                    Some(ModifyContext::Pin(old, _)),
                    ChildEvent::Event(Event::Done {
                        pinned: Some(new), ..
                    }),
                ) = (&context, &event)
                {
                    self.follow(old, new);
                }
                let effects = self.on_work(event, move |outcome| {
                    let mut effects = match outcome {
                        Ok(_) => match context {
                            Some(ModifyContext::Pin(snapshot, pinned)) => {
                                vec![Effect::LogEvent(EventKind::Pinned { snapshot, pinned })]
                            }
                            _ => Vec::new(),
                        },
                        Err(error) => vec![Effect::ShowError(fl!("pin-snapshot-failed"), error)],
                    };
                    effects.extend(secret.map(Effect::Fetch));
                    effects
                });
                self.track_history(&effects);
                effects
            }
            Message::DetailsSaved(event) => {
                let secret = self.secret.clone();
                let context = self
                    .work
                    .as_ref()
                    .and_then(|running| running.context.clone());
                if let (
                    Some(ModifyContext::Details(old)),
                    ChildEvent::Event(Event::Done {
                        detailed: Some(new),
                        ..
                    }),
                ) = (&context, &event)
                {
                    self.follow(old, new);
                }
                let effects = self.on_work(event, move |outcome| {
                    let mut effects = match outcome {
                        Ok(_) => match context {
                            Some(ModifyContext::Details(snapshot)) => {
                                vec![Effect::LogEvent(EventKind::DetailsChanged { snapshot })]
                            }
                            _ => Vec::new(),
                        },
                        Err(error) => {
                            vec![Effect::ShowError(fl!("snapshot-details-failed"), error)]
                        }
                    };
                    effects.extend(secret.map(Effect::Fetch));
                    effects
                });
                self.track_history(&effects);
                effects
            }
            Message::Manage(message) => self.on_manage(message),
            Message::ExplainSnapshots => {
                self.explain_snapshots = !self.explain_snapshots;
                Vec::new()
            }
            Message::ChangedElsewhere(logged) => {
                let mut effects: Vec<Effect> = logged.map(Effect::LogEvent).into_iter().collect();
                self.track_history(&effects);
                effects.extend(self.fetch());
                effects
            }
            Message::Restore => self.open_restore(None),
            Message::BrowseSnapshot(id) => self.open_restore(Some(id)),
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
            Message::EstimateSize => {
                if self.estimating || profile.sources.is_empty() {
                    Vec::new()
                } else {
                    self.estimating = true;
                    self.estimate = None;
                    vec![Effect::EstimateSize(Arc::new(AtomicBool::new(false)))]
                }
            }
            Message::Estimate(event) => {
                match event {
                    SizeEstimateEvent::Progress(total) => self.estimate = Some(Ok(total)),
                    SizeEstimateEvent::Done(total) => {
                        self.estimate = Some(Ok(total));
                        self.estimating = false;
                    }
                    SizeEstimateEvent::Failed(err) => {
                        self.estimate = Some(Err(err));
                        self.estimating = false;
                    }
                }
                Vec::new()
            }
            Message::HistoryLoaded(mut history, unreadable) => {
                self.history_unreadable = unreadable;
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
                    effects.push(Effect::RecordSuccess(report.snapshot.clone()));
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
        if let Some(manage) = &self.manage {
            return widget::scrollable(
                self.manage_view(manage, profile)
                    .apply(widget::container)
                    .max_width(PAGE_MAX_WIDTH),
            )
            .height(Length::Fill)
            .into();
        }
        let spacing = theme::active().cosmic().spacing;
        let sandboxed = crate::sandbox::sandboxed();
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

        if let Some(banner) = trouble(profile, run, self.history_unreadable, now, self.can_work()) {
            page = page.push(banner);
        }
        page = page.push(self.status_card(profile, now));

        if !self.is_unlocked() {
            page = page.push(match self.access {
                Access::Prompt | Access::OpeningTyped => self.unlock_card(),
                Access::Unchecked | Access::Fetching => busy_card(fl!("unlock-fetching")),
                Access::OpeningSaved => busy_card(fl!("unlock-opening")),
            });
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
                        .description(fl!(
                            "schedule-retention-summary",
                            schedule = schedule_summary(profile.schedule),
                            retention = retention_label(profile.retention)
                        ))
                        .control(
                            widget::button::standard(fl!("change")).on_press(Message::EditSchedule),
                        ),
                )
                .add_maybe((profile.schedule != Schedule::Manual).then(|| {
                    widget::settings::item::builder(fl!("run-as-scheduled-row"))
                        .description(fl!("run-as-scheduled-description"))
                        .control(
                            widget::button::standard(fl!("run-as-scheduled"))
                                .on_press_maybe(self.can_work().then_some(Message::RunAsScheduled)),
                        )
                }))
                // Hooks and a password command run programs, which inside a
                // sandbox are the runtime's, not the host's.
                .add(
                    widget::settings::item::builder(fl!("hooks-row"))
                        .description(if sandboxed {
                            fl!("sandbox-unavailable")
                        } else {
                            hooks_summary(&profile.hooks)
                        })
                        .control(
                            widget::button::standard(fl!("change"))
                                .on_press_maybe((!sandboxed).then_some(Message::EditHooks)),
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
                        .description(if sandboxed {
                            fl!("sandbox-unavailable")
                        } else if profile.password_command.is_empty() {
                            fl!("password-source-keyring")
                        } else {
                            fl!("password-source-command")
                        })
                        .control(
                            widget::button::standard(fl!("change")).on_press_maybe(
                                (!sandboxed).then_some(Message::EditPasswordCommand),
                            ),
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

        widget::scrollable(page.apply(widget::container).max_width(PAGE_MAX_WIDTH))
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
            .and_then(|snapshots| {
                crate::engine::newest_settled(snapshots, now).map(|snapshot| snapshot.time)
            })
            .or(profile.last_success);
        let headline = match last {
            None => fl!("never-backed-up"),
            Some(time) => format::backed_up_ago(now, time),
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
        // No unlock needed: this only walks the source folders on disk, the
        // same as the setup wizard's own live estimate.
        let can_estimate = !self.estimating && !profile.sources.is_empty();

        card(
            widget::column::with_capacity(5)
                .spacing(spacing.space_xs)
                .push(widget::text::title4(headline))
                .push(widget::text::caption(detail))
                .push(widget::text::caption(schedule_summary(profile.schedule)))
                .push_maybe(self.next_run.map(|time| {
                    widget::text::caption(fl!("next-run", time = format::local_time(time)))
                }))
                .push(
                    widget::row::with_capacity(3)
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
                        )
                        .push(
                            widget::button::standard(fl!("estimate-size"))
                                .on_press_maybe(can_estimate.then_some(Message::EstimateSize)),
                        ),
                )
                .push_maybe(self.estimate_line()),
        )
    }

    /// The result of pressing "Estimate Size": a count in progress, the
    /// total once known, or why it could not be counted.
    fn estimate_line(&self) -> Option<Element<'_, Message>> {
        if self.estimating {
            return Some(widget::text::caption(fl!("wizard-estimate-counting")).into());
        }
        match self.estimate.as_ref()? {
            Ok(total) => {
                let files = total.files as i64;
                Some(
                    widget::text::caption(fl!(
                        "wizard-estimate",
                        size = format::bytes(total.bytes),
                        files = files
                    ))
                    .into(),
                )
            }
            Err(err) => Some(widget::text::caption(errors::explain(err)).into()),
        }
    }

    fn unlock_card(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let unlock = (!self.unlock_password.is_empty() && self.access == Access::Prompt)
            .then_some(Message::Unlock);
        card(
            widget::column::with_capacity(4)
                .spacing(spacing.space_xs)
                .push(widget::text::title4(fl!("unlock-title")))
                .push(
                    widget::secure_input(fl!("password"), &self.unlock_password, None, true)
                        .id(unlock_input_id())
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
        let shown = RECENT.min(snapshots.len());
        // The title, with an info button that shows what a snapshot is
        // beneath it, outside the list itself.
        let explain = widget::button::icon(widget::icon::from_name("dialog-information-symbolic"))
            .padding(spacing.space_xxs)
            .selected(self.explain_snapshots)
            .tooltip(fl!("snapshots-explain"))
            .name(fl!("snapshots-explain"))
            .on_press(Message::ExplainSnapshots);
        let header = widget::column::with_capacity(2)
            .spacing(spacing.space_xxs)
            .push(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xxs)
                    .align_y(Alignment::Center)
                    .push(widget::text::heading(fl!("recent-snapshots")))
                    .push(explain),
            )
            .push_maybe(
                self.explain_snapshots
                    .then(|| widget::text::caption(fl!("snapshots-explained"))),
            );
        let mut section = widget::settings::section().header(header);
        // Browsing needs the repository to itself, as the Restore button does.
        let can_browse = self.can_work();
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
            let browse = widget::button::icon(widget::icon::from_name("folder-open-symbolic"))
                .padding(spacing.space_xxs)
                .tooltip(fl!("browse-snapshot"))
                .name(fl!("browse-snapshot"))
                .on_press_maybe(can_browse.then(|| Message::BrowseSnapshot(id.clone())));
            section = section.add(
                widget::settings::item::builder(snapshots::title(snapshot))
                    .description(snapshots::row_description(snapshot, false))
                    .control(
                        widget::row::with_capacity(3)
                            .spacing(spacing.space_xxs)
                            .push(browse)
                            .push(pin)
                            .push(delete),
                    ),
            );
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_xs)
            .push(section)
            .push(
                widget::button::standard(fl!("manage-snapshots"))
                    .on_press(Message::Manage(snapshots::Message::Open)),
            )
            .into()
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
                format::list(paths.iter().map(|path| format::path(path)))
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
                crate::event_log::describe(&event.kind),
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
                    |ratio| fl!("compression-ratio", ratio = format!("{ratio:.1}")),
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

pub use crate::core::format::schedule_summary;

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
    history_unreadable: bool,
    now: i64,
    can_work: bool,
) -> Option<Element<'a, Message>> {
    let spacing = theme::active().cosmic().spacing;
    let (title, body, action) = if run.unreadable || history_unreadable {
        (
            fl!("status-unreadable-title"),
            fl!("status-unreadable-body"),
            Some((fl!("reset-status"), Message::ResetStatus)),
        )
    } else if run.damaged {
        (
            fl!("damaged-title"),
            fl!("damaged-body"),
            Some((fl!("check-again"), Message::CheckNow)),
        )
    } else {
        let failure = run.current_failure(profile.last_success, now)?;
        let when = format::failed_ago(now, failure.time);
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
                .push(widget::icon::from_name("dialog-warning-symbolic").size(NOTICE_ICON_SIZE))
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
    let label = match (running.work, running.progress.as_ref().map(|p| p.phase)) {
        (_, None) => fl!("progress-starting"),
        (Work::CleanUp, _) => fl!("progress-cleaning-up"),
        (Work::Check, _) | (_, Some(Phase::Checking)) => fl!("progress-checking"),
        (_, Some(Phase::Preparing)) => fl!("progress-preparing"),
        (_, Some(Phase::BackingUp)) => fl!("progress-backing-up"),
        (_, Some(Phase::Restoring)) => fl!("progress-restoring"),
    };
    let action: Element<'_, Message> = if running.work == Work::CleanUp {
        // Pruning deletes data as it goes; it is left to finish.
        widget::text::caption(fl!("clean-up-cannot-stop")).into()
    } else {
        widget::button::standard(fl!("cancel"))
            .on_press_maybe(running.handle.as_ref().map(|_| Message::CancelBackup))
            .into()
    };
    progress_body(label, running.progress.as_ref(), &running.timing, action)
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

/// What the unlock card shows while the page is getting or trying a
/// password on its own: nothing to type into, so nothing typed is lost
/// when the remembered password opens the backup a moment later.
fn busy_card<'a>(label: String) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;
    card(
        widget::column::with_capacity(2)
            .spacing(spacing.space_xs)
            .push(widget::text::title4(label))
            .push(widget::progress_bar::indeterminate_linear()),
    )
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
            time_subsec_ns: 0,
            paths: vec!["/home/dave".into()],
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
    fn the_unlock_field_is_focused_once_when_no_password_is_remembered() {
        let mut state = ProfileState::new();
        let effects = state.update(Message::KeyringLoaded(Ok(None)), &profile());
        assert!(
            effects.iter().any(|e| matches!(e, Effect::FocusUnlock)),
            "did not focus the unlock field"
        );
        let effects = state.update(Message::KeyringLoaded(Ok(None)), &profile());
        assert!(
            !effects.iter().any(|e| matches!(e, Effect::FocusUnlock)),
            "the unlock field is not refocused every time the keyring is re-checked"
        );
    }

    #[test]
    fn no_password_can_be_typed_while_the_keyring_is_asked() {
        let mut state = ProfileState::new();
        state.activate(&profile());
        state.update(Message::UnlockPassword("typed".into()), &profile());
        let effects = state.update(Message::Unlock, &profile());
        assert!(effects.is_empty(), "unlocked by hand mid-lookup");
        assert_eq!(state.access, Access::Fetching);
    }

    #[test]
    fn a_remembered_password_opens_without_showing_the_prompt() {
        let mut state = ProfileState::new();
        state.activate(&profile());
        state.update(
            Message::KeyringLoaded(Ok(Some(Secret::new("pw")))),
            &profile(),
        );
        assert_eq!(state.access, Access::OpeningSaved);
        state.update(
            Message::Opened(Secret::new("pw"), Ok(vec![summary(10)])),
            &profile(),
        );
        assert!(state.is_unlocked());
    }

    #[test]
    fn a_remembered_password_that_fails_falls_back_to_the_prompt() {
        let mut state = ProfileState::new();
        state.activate(&profile());
        state.update(
            Message::KeyringLoaded(Ok(Some(Secret::new("stale")))),
            &profile(),
        );
        let effects = state.update(
            Message::Opened(
                Secret::new("stale"),
                Err(EngineError::new(ErrorKind::WrongPassword, "")),
            ),
            &profile(),
        );
        assert_eq!(state.access, Access::Prompt);
        assert!(effects.iter().any(|e| matches!(e, Effect::FocusUnlock)));
        assert!(effects.iter().any(|e| matches!(e, Effect::ShowError(..))));
    }

    #[test]
    fn a_failed_password_command_still_shows_the_prompt() {
        let mut state = ProfileState::new();
        state.activate(&profile());
        let effects = state.update(
            Message::KeyringLoaded(Err(EngineError::new(ErrorKind::Io, "exit 1"))),
            &profile(),
        );
        assert_eq!(state.access, Access::Prompt);
        assert!(effects.iter().any(|e| matches!(e, Effect::ShowError(..))));
    }

    #[test]
    fn locking_asks_the_keyring_again() {
        let mut state = unlocked();
        state.lock();
        let effects = state.activate(&profile());
        assert!(effects.iter().any(|e| matches!(e, Effect::LoadKeyring)));
        assert_eq!(state.access, Access::Fetching);
    }

    #[test]
    fn back_up_when_unlocked_starts_a_backup_once_opened() {
        let mut state = ProfileState::new();
        state.back_up_when_unlocked = true;
        state.activate(&profile());
        state.update(
            Message::KeyringLoaded(Ok(Some(Secret::new("pw")))),
            &profile(),
        );
        let effects = state.update(
            Message::Opened(Secret::new("pw"), Ok(vec![summary(10)])),
            &profile(),
        );
        assert!(effects.iter().any(|e| matches!(e, Effect::BackUp(_))));
        assert!(state.is_busy());
        assert!(!state.back_up_when_unlocked, "only once");
    }

    #[test]
    fn a_remembered_password_never_needs_the_unlock_field_focused() {
        let mut state = ProfileState::new();
        let effects = state.update(
            Message::KeyringLoaded(Ok(Some(Secret::new("pw")))),
            &profile(),
        );
        assert!(!effects.iter().any(|e| matches!(e, Effect::FocusUnlock)));
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
    fn size_is_estimated_once_per_press_and_works_while_locked() {
        // Unlike statistics, no unlock is needed: this only walks the
        // source folders on disk.
        let mut state = ProfileState::new();
        assert!(matches!(
            state.update(Message::EstimateSize, &profile()).as_slice(),
            [Effect::EstimateSize(_)]
        ));
        assert!(
            state.update(Message::EstimateSize, &profile()).is_empty(),
            "a second press does not start another estimate"
        );

        let total = SizeEstimate {
            files: 3,
            bytes: 1024,
            per_source: vec![1024],
        };
        state.update(
            Message::Estimate(SizeEstimateEvent::Done(total)),
            &profile(),
        );
        assert!(
            matches!(
                state.update(Message::EstimateSize, &profile()).as_slice(),
                [Effect::EstimateSize(_)]
            ),
            "a finished estimate can be run again"
        );

        // A backup with nothing to back up yet has nothing to estimate.
        let mut empty_sources = profile();
        empty_sources.sources.clear();
        let mut state = ProfileState::new();
        assert!(
            state
                .update(Message::EstimateSize, &empty_sources)
                .is_empty()
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
        state.update(Message::KeyringLoaded(Ok(None)), &profile());
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
        state.update(Message::KeyringLoaded(Ok(None)), &profile());
        state.update(Message::UnlockPassword("wrong".into()), &profile());
        state.update(Message::Unlock, &profile());
        assert_eq!(state.access, Access::OpeningTyped);
        let effects = state.update(
            Message::Opened(
                Secret::new("wrong"),
                Err(EngineError::new(ErrorKind::WrongPassword, "")),
            ),
            &profile(),
        );
        assert!(!state.is_unlocked());
        assert_eq!(state.access, Access::Prompt);
        // The field kept its focus: the card never left the screen.
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
                report: Some(Box::new(BackupReport {
                    snapshot: summary(99),
                })),
                restored: None,
                forgotten: None,
                pruned: None,
                pinned: None,
                detailed: None,
                removed: None,
            })),
            &profile(),
        );

        assert!(!state.is_busy());
        assert!(effects.iter().any(|e| matches!(e, Effect::Fetch(_))));
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::RecordSuccess(s) if s.time == 99))
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
            source: crate::event_log::Source::Desktop,
        });

        let loaded = vec![crate::event_log::Event {
            time: 100,
            kind: EventKind::Checked { damaged: false },
            source: crate::event_log::Source::Desktop,
        }];
        state.update(Message::HistoryLoaded(loaded, false), &profile());

        assert_eq!(state.history.len(), 2, "nothing lost, nothing duplicated");
        assert_eq!(state.history[0].time, 100);
        assert_eq!(state.history[1].time, 500);
    }

    #[test]
    fn a_canceled_backup_is_reported_and_cleared() {
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
    fn what_a_snapshot_is_shows_and_hides_on_its_info_button() {
        let mut state = ProfileState::new();
        assert!(!state.explain_snapshots, "hidden until asked for");
        state.update(Message::ExplainSnapshots, &profile());
        assert!(state.explain_snapshots);
        state.update(Message::ExplainSnapshots, &profile());
        assert!(!state.explain_snapshots);
    }

    #[test]
    fn browsing_a_snapshot_opens_the_restore_page_at_it() {
        let mut state = unlocked();

        let effects = state.update(Message::BrowseSnapshot("abc123".into()), &profile());

        assert!(matches!(
            effects.as_slice(),
            [Effect::OpenRestore(_, Some(id))] if id == "abc123"
        ));
    }

    #[test]
    fn a_snapshot_is_not_browsed_while_a_backup_runs() {
        let mut state = unlocked();
        state.back_up(&profile());

        assert!(
            state
                .update(Message::BrowseSnapshot("abc123".into()), &profile())
                .is_empty()
        );
    }

    #[test]
    fn deleting_a_snapshot_asks_for_confirmation_before_touching_anything() {
        let mut state = unlocked();

        let effects = state.update(Message::DeleteSnapshot("abc123".into()), &profile());

        assert!(matches!(
            effects.as_slice(),
            [Effect::ConfirmDeleteSnapshot { id, .. }] if id == "abc123"
        ));
        assert!(
            !state.is_busy(),
            "asking must not itself start deleting anything"
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

    fn done() -> ChildEvent {
        ChildEvent::Event(Event::Done {
            report: None,
            restored: None,
            forgotten: None,
            pruned: None,
            pinned: None,
            detailed: None,
            removed: None,
        })
    }

    #[test]
    fn a_finished_snapshot_deletion_logs_which_one() {
        let mut state = unlocked();
        state.delete_snapshot_confirmed("abc123".into());

        let effects = state.update(Message::SnapshotsDeleted(done()), &profile());

        assert!(effects.iter().any(|e| matches!(e, Effect::Fetch(_))));
        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::LogEvent(EventKind::SnapshotDeleted { snapshot }) if snapshot == "abc123"
        )));
    }

    #[test]
    fn a_failed_snapshot_deletion_logs_nothing() {
        let mut state = unlocked();
        state.delete_snapshot_confirmed("abc123".into());

        let effects = state.update(
            Message::SnapshotsDeleted(ChildEvent::Ended(EngineError::new(ErrorKind::Io, ""))),
            &profile(),
        );

        assert!(!effects.iter().any(|e| matches!(e, Effect::LogEvent(_))));
    }

    /// What the window feeds back when a delete or pin cannot even start
    /// (its drive was unplugged): the page must not stay busy.
    #[test]
    fn a_delete_or_pin_that_never_started_leaves_the_page_free() {
        let gone = || ChildEvent::Ended(EngineError::new(ErrorKind::DestinationUnavailable, ""));

        let mut state = unlocked();
        state.delete_snapshot_confirmed("abc123".into());
        assert!(state.is_busy());
        let effects = state.update(Message::SnapshotsDeleted(gone()), &profile());
        assert!(!state.is_busy());
        assert!(effects.iter().any(|e| matches!(e, Effect::ShowError(..))));

        state.update(Message::TogglePinned("abc123".into(), true), &profile());
        assert!(state.is_busy());
        state.update(Message::Pinned(gone()), &profile());
        assert!(!state.is_busy());
    }

    #[test]
    fn a_finished_pin_change_logs_which_way_it_went() {
        let mut state = unlocked();
        state.update(Message::TogglePinned("abc123".into(), true), &profile());

        let effects = state.update(Message::Pinned(done()), &profile());

        assert!(effects.iter().any(|e| matches!(
            e,
            Effect::LogEvent(EventKind::Pinned { snapshot, pinned: true }) if snapshot == "abc123"
        )));
    }

    #[test]
    fn restore_when_unlocked_opens_the_restore_page_once() {
        let mut state = ProfileState::new();
        state.restore_when_unlocked = true;

        let effects = state.update(
            Message::Opened(Secret::new("pw"), Ok(vec![summary(10)])),
            &profile(),
        );

        assert!(matches!(effects.as_slice(), [Effect::OpenRestore(_, None)]));
        assert!(!state.restore_when_unlocked, "only the first unlock");
    }
}
