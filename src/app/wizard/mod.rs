// SPDX-License-Identifier: GPL-3.0-only

//! The setup wizard: what to back up, where to, when, and the password.
//!
//! The same wizard creates a backup, opens an existing one, and edits what an
//! existing profile covers or when it runs; each mode shows only the steps it
//! needs. All state
//! lives here and every side effect is returned as an [`Effect`] for the
//! application to run, so the step logic is testable without a window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::format;
use crate::engine::{BackupRequest, EngineError, ExclusionBreakdown, Probe, Secret, SizeEstimate};
use crate::fl;
use crate::profile::{Destination, Profile, Retention, Schedule, default_excludes};

pub mod place;

/// What the wizard is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Set up a new backup and create its repository.
    Create,
    /// Add a repository that already exists.
    Open,
    /// Change what an existing profile backs up.
    Edit { profile_id: String },
    /// Change when an existing profile runs and what it keeps.
    Schedule { profile_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    What,
    Where,
    When,
    Secure,
}

impl Mode {
    fn steps(&self) -> &'static [Step] {
        match self {
            Self::Create => &[Step::What, Step::Where, Step::When, Step::Secure],
            Self::Open => &[Step::Where, Step::When, Step::Secure],
            Self::Edit { .. } => &[Step::What],
            Self::Schedule { .. } => &[Step::When],
        }
    }

    fn edits(&self) -> bool {
        matches!(self, Self::Edit { .. } | Self::Schedule { .. })
    }
}

/// How often automatic backups run, in the order the list shows them.
const FREQUENCIES: [Schedule; 3] = [Schedule::Hourly, Schedule::Daily, Schedule::Weekly];

/// What to keep, in the order the list shows them: Déjà Dup's choices, with
/// Smart first.
const KEEP_CHOICES: [Retention; 5] = [
    Retention::Smart,
    Retention::KeepFor { days: 90 },
    Retention::KeepFor { days: 182 },
    Retention::KeepFor { days: 365 },
    Retention::KeepForever,
];

/// The size estimate as the wizard shows it.
#[derive(Debug, Default)]
pub struct EstimateView {
    /// Incremented whenever the lists change; results for an older generation
    /// are stale and ignored.
    pub generation: u64,
    pub running: bool,
    pub total: Option<SizeEstimate>,
    /// What the exclusions take out, once the total is known.
    pub breakdown: Option<ExclusionBreakdown>,
    /// Size of each excluded folder that sits inside an included one.
    pub exclude_sizes: HashMap<PathBuf, u64>,
}

/// Progress of a size estimate, as it reaches the wizard.
#[derive(Debug, Clone)]
pub enum EstimateEvent {
    Progress(SizeEstimate),
    Done(SizeEstimate),
    /// What the exclusions take out, with the sizes of these folders.
    Breakdown(Vec<PathBuf>, ExclusionBreakdown),
    Failed(String),
}

pub struct Wizard {
    pub mode: Mode,
    pub step: Step,
    pub name: String,
    /// The name was typed (or came with the backup), so the destination no
    /// longer suggests one.
    name_chosen: bool,
    pub sources: Vec<PathBuf>,
    pub excludes: Vec<PathBuf>,
    pub patterns: Vec<String>,
    pub pattern_input: String,
    pub one_file_system: bool,
    /// Leave out any folder containing a `CACHEDIR.TAG` file.
    pub exclude_caches: bool,
    /// Honour each project's own `.gitignore`.
    pub git_ignore: bool,
    /// Write no snapshot when nothing changed since the last one.
    pub skip_if_unchanged: bool,
    pub place: place::Place,
    /// The profile being edited. Finishing changes only the fields the
    /// mode's steps show, so nothing else about it can be lost.
    base: Option<Profile>,
    /// Back up automatically. `Manual` when off.
    pub schedule: Schedule,
    /// The frequency to use when automatic backups are turned on.
    frequency: Schedule,
    pub retention: Retention,
    /// Free up space automatically; `None` until the user chooses, which
    /// means the default for the destination.
    pub prune: Option<bool>,
    /// rustic's own append-only mode. Only offered when creating a new
    /// backup: rustic's `config` command, the only way to change it,
    /// refuses every other change to an append-only repository, and
    /// Stellarshot offers no way to run the one change it still allows
    /// (turning append-only back off), so from here there is no way back.
    pub append_only: bool,
    frequency_labels: Vec<String>,
    keep_labels: Vec<String>,
    pub password: String,
    pub confirm: String,
    pub password_hidden: bool,
    pub remember: bool,
    pub estimate: EstimateView,
    cancel: Arc<AtomicBool>,
    /// Finishing: the repository is being created or opened, since when.
    busy_since: Option<Instant>,
    /// Next was pressed on the "where" step before the destination had been
    /// checked: move on by itself once the check succeeds.
    advance_when_checked: bool,
    /// Opening a backup Déjà Dup made.
    pub importing: bool,
}

#[derive(Debug, Clone)]
pub enum Message {
    AddSources,
    SourcesChosen(Vec<PathBuf>),
    RemoveSource(usize),
    AddExcludes,
    ExcludesChosen(Vec<PathBuf>),
    RemoveExclude(usize),
    PatternInput(String),
    AddPattern,
    RemovePattern(usize),
    OneFileSystem(bool),
    ExcludeCaches(bool),
    GitIgnore(bool),
    SkipIfUnchanged(bool),
    Estimate(u64, EstimateEvent),
    Name(String),
    Place(place::Message),
    Password(String),
    Confirm(String),
    TogglePasswordVisible,
    Remember(bool),
    Automatic(bool),
    Frequency(usize),
    Keep(usize),
    Prune(bool),
    AppendOnly(bool),
    Back,
    Next,
    Cancel,
    /// The repository was created or opened (or that failed).
    Finished(Result<(), EngineError>),
}

/// Everything the wizard needs the application to do.
pub enum Effect {
    PickFolders {
        excludes: bool,
    },
    Place(place::Effect),
    /// Walk what `request` covers, and size `exclude_folders`, reporting under
    /// `generation`.
    Estimate {
        generation: u64,
        request: BackupRequest,
        exclude_folders: Vec<PathBuf>,
        cancel: Arc<AtomicBool>,
    },
    Finish(Box<Finish>),
    Close,
    /// Cancel was pressed: ask the application to offer "finish later" or
    /// "discard", rather than deciding here. Nothing is stopped yet — a
    /// running estimate keeps going until [`Wizard::discard`] is actually
    /// called, since "finish later" leaves it as it was.
    ConfirmCancel,
}

/// What to do when the wizard completes.
pub struct Finish {
    pub mode: Mode,
    pub profile: Profile,
    /// Absent when editing: the repository is not touched.
    pub secret: Option<Secret>,
    pub remember: bool,
}

impl Wizard {
    fn new(mode: Mode) -> Self {
        let step = mode.steps()[0];
        Self {
            mode,
            step,
            name: String::new(),
            name_chosen: false,
            sources: Vec::new(),
            excludes: Vec::new(),
            patterns: Vec::new(),
            pattern_input: String::new(),
            one_file_system: true,
            exclude_caches: false,
            git_ignore: false,
            skip_if_unchanged: false,
            place: place::Place::default(),
            base: None,
            // A new backup runs daily and keeps a smart history, as the
            // design asks.
            schedule: Schedule::Daily,
            frequency: Schedule::Daily,
            retention: Retention::Smart,
            prune: None,
            append_only: false,
            frequency_labels: vec![
                fl!("frequency-hourly"),
                fl!("frequency-daily"),
                fl!("frequency-weekly"),
            ],
            keep_labels: Vec::new(),
            password: String::new(),
            confirm: String::new(),
            password_hidden: true,
            remember: true,
            estimate: EstimateView::default(),
            cancel: Arc::new(AtomicBool::new(false)),
            busy_since: None,
            advance_when_checked: false,
            importing: false,
        }
    }

    /// A new backup of `home`, with the usual folders left out.
    pub fn create(home: Option<&Path>) -> (Self, Vec<Effect>) {
        let mut wizard = Self::new(Mode::Create);
        if let Some(home) = home {
            wizard.sources = vec![home.to_path_buf()];
            wizard.excludes = default_excludes(home);
        }
        let effects = wizard.restart_estimate();
        (wizard, effects)
    }

    pub fn open() -> (Self, Vec<Effect>) {
        let wizard = Self::new(Mode::Open);
        let effects = wizard.place_effects(wizard.place.enter());
        (wizard, effects)
    }

    /// Open a Déjà Dup backup: its folders, exclusions and destination come
    /// from Déjà Dup's settings; the password does not.
    pub fn import(import: &crate::dejadup::Import) -> (Self, Vec<Effect>) {
        use crate::dejadup::Place as From;
        let mut wizard = Self::new(Mode::Open);
        wizard.importing = true;
        wizard.name = fl!("dejadup-name");
        wizard.name_chosen = true;
        wizard.sources = import.sources.clone();
        wizard.excludes = import.excludes.clone();
        wizard.set_schedule(import.schedule);
        wizard.retention = import.retention;
        let mut effects = wizard.place.enter();
        match &import.place {
            From::Folder(path) => {
                wizard.place.kind = place::Kind::Folder;
                effects.extend(
                    wizard
                        .place
                        .update(place::Message::FolderChosen(path.clone())),
                );
            }
            From::Drive { uuid, folder, .. } => {
                wizard.place.kind = place::Kind::Drive;
                wizard.place.preferred_drive = Some(uuid.clone());
                wizard.place.drive_folder = folder.display().to_string();
            }
            From::Sftp {
                host,
                user,
                port,
                path,
            } => {
                wizard.place.kind = place::Kind::Server;
                wizard.place.host = host.clone();
                wizard.place.user = user.clone();
                wizard.place.port = port.to_string();
                wizard.place.server_path = path.clone();
            }
            From::Google { folder } => {
                wizard.place.kind = place::Kind::Google;
                wizard.place.cloud_path = folder.clone();
            }
            From::Rclone { remote, folder } => {
                wizard.place.kind = place::Kind::Remote;
                wizard.place.preferred_remote = Some(remote.clone());
                wizard.place.remote_path = folder.clone();
                effects.push(place::Effect::ListRemotes);
            }
            From::Unsupported(_) => {}
        }
        let effects = wizard.place_effects(effects);
        (wizard, effects)
    }

    fn place_effects(&self, effects: Vec<place::Effect>) -> Vec<Effect> {
        effects.into_iter().map(Effect::Place).collect()
    }

    /// Start from an existing profile, in a mode that edits it.
    fn editing(mode: Mode, profile: &Profile) -> Self {
        let mut wizard = Self::new(mode);
        wizard.name = profile.name.clone();
        wizard.name_chosen = true;
        wizard.sources = profile.sources.clone();
        wizard.excludes = profile.excludes.clone();
        wizard.patterns = profile.exclude_patterns.clone();
        wizard.one_file_system = profile.one_file_system;
        wizard.exclude_caches = profile.exclude_caches;
        wizard.git_ignore = profile.git_ignore;
        wizard.skip_if_unchanged = profile.skip_if_unchanged;
        wizard.set_schedule(profile.schedule);
        wizard.retention = profile.retention;
        wizard.prune = profile.prune;
        wizard.base = Some(profile.clone());
        wizard
    }

    pub fn edit(profile: &Profile) -> (Self, Vec<Effect>) {
        let mut wizard = Self::editing(
            Mode::Edit {
                profile_id: profile.id.clone(),
            },
            profile,
        );
        let effects = wizard.restart_estimate();
        (wizard, effects)
    }

    /// Change when `profile` runs and what it keeps.
    pub fn schedule(profile: &Profile) -> (Self, Vec<Effect>) {
        let mode = Mode::Schedule {
            profile_id: profile.id.clone(),
        };
        let mut wizard = Self::editing(mode, profile);
        wizard.keep_labels = wizard.keep_labels();
        (wizard, Vec::new())
    }

    fn set_schedule(&mut self, schedule: Schedule) {
        self.schedule = schedule;
        if schedule != Schedule::Manual {
            self.frequency = schedule;
        }
    }

    /// Where the backup will be, as far as is known yet.
    fn destination(&self) -> Option<Destination> {
        self.place
            .destination()
            .or_else(|| self.base.as_ref().map(|base| base.destination.clone()))
    }

    /// Whether freeing space is on: the user's choice, or else the default
    /// for the destination.
    pub fn prune_enabled(&self) -> bool {
        self.prune.unwrap_or_else(|| {
            self.destination().is_some_and(|destination| {
                Profile::new(String::new(), destination, Vec::new()).prune_enabled()
            })
        })
    }

    /// The labels for [`KEEP_CHOICES`], plus the current choice if it is a
    /// period the list does not have (an imported Déjà Dup setting).
    fn keep_labels(&self) -> Vec<String> {
        let mut labels: Vec<String> = KEEP_CHOICES.iter().map(|r| retention_label(*r)).collect();
        if !KEEP_CHOICES.contains(&self.retention) {
            labels.push(retention_label(self.retention));
        }
        labels
    }

    fn request(&self) -> BackupRequest {
        BackupRequest {
            sources: self.sources.clone(),
            excludes: self.excludes.clone(),
            exclude_patterns: self.patterns.clone(),
            exclude_caches: self.exclude_caches,
            git_ignore: self.git_ignore,
            one_file_system: self.one_file_system,
            ..BackupRequest::default()
        }
    }

    /// Whether `exclude` takes anything out, i.e. sits inside a source.
    pub fn exclude_applies(&self, exclude: &Path) -> bool {
        self.sources
            .iter()
            .any(|source| exclude.starts_with(source))
    }

    /// Cancel any running estimate and start a new one for the current lists.
    fn restart_estimate(&mut self) -> Vec<Effect> {
        self.cancel.store(true, Ordering::Relaxed);
        self.cancel = Arc::new(AtomicBool::new(false));
        self.estimate.generation += 1;
        self.estimate.running = true;
        self.estimate.breakdown = None;
        self.estimate.exclude_sizes.clear();
        let exclude_folders = self
            .excludes
            .iter()
            .filter(|exclude| self.exclude_applies(exclude))
            .cloned()
            .collect();
        vec![Effect::Estimate {
            generation: self.estimate.generation,
            request: self.request(),
            exclude_folders,
            cancel: self.cancel.clone(),
        }]
    }

    fn position(&self) -> usize {
        self.mode
            .steps()
            .iter()
            .position(|step| *step == self.step)
            .unwrap_or(0)
    }

    fn is_last_step(&self) -> bool {
        self.position() + 1 == self.mode.steps().len()
    }

    /// Finishing: the repository is being created or opened.
    pub fn busy(&self) -> bool {
        self.busy_since.is_some()
    }

    /// Something is under way that the wizard shows a running time for.
    pub fn waiting(&self) -> bool {
        self.busy() || self.place.checking_since().is_some()
    }

    /// Stop any running estimate. Called when the wizard is truly discarded,
    /// as opposed to merely hidden while the user looks at something else
    /// ("finish later"), which leaves it running.
    pub fn discard(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// What the "where" step needs to find at the destination.
    fn wanted(&self) -> Probe {
        match self.mode {
            Mode::Open => Probe::Repository,
            _ => Probe::Empty,
        }
    }

    /// The destination has been checked and is what this step needs.
    fn place_accepted(&self) -> bool {
        matches!(self.place.probe(), Some(Ok(probe)) if *probe == self.wanted())
    }

    /// Whether Next can be pressed. On the "where" step that is as soon as a
    /// destination is described: Next checks it, and moves on when the
    /// check succeeds. A check that found the wrong thing there blocks it; a
    /// check that failed can be tried again with Next.
    pub fn can_advance(&self) -> bool {
        if self.busy() {
            return false;
        }
        match self.step {
            Step::What => !self.sources.is_empty(),
            Step::Where => {
                !self.name.trim().is_empty()
                    && self.place.destination().is_some()
                    && self.place.checking_since().is_none()
                    && match self.place.probe() {
                        None | Some(Err(_)) => true,
                        Some(Ok(_)) => self.place_accepted(),
                    }
            }
            Step::When => true,
            Step::Secure => match self.mode {
                Mode::Create => !self.password.is_empty() && self.password == self.confirm,
                _ => !self.password.is_empty(),
            },
        }
    }

    /// Go to the next step, or finish on the last.
    fn advance(&mut self) -> Vec<Effect> {
        if self.is_last_step() {
            return self.finish();
        }
        self.step = self.mode.steps()[self.position() + 1];
        match self.step {
            Step::Where => self.place_effects(self.place.enter()),
            Step::When => {
                self.keep_labels = self.keep_labels();
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn finish(&mut self) -> Vec<Effect> {
        let Some(destination) = self.destination() else {
            return Vec::new();
        };
        let mut profile = match &self.base {
            Some(base) => base.clone(),
            None => {
                let mut profile = Profile::new(
                    self.name.trim().to_owned(),
                    destination,
                    self.sources.clone(),
                );
                profile.bandwidth_limit = self.place.bandwidth_limit.trim().to_owned();
                profile
            }
        };
        let steps = self.mode.steps();
        // A new profile takes everything the wizard holds, including folders
        // an import filled in without showing the What step; an edited one
        // only what its steps showed.
        let new = self.base.is_none();
        if new || steps.contains(&Step::What) {
            profile.sources = self.sources.clone();
            profile.excludes = self.excludes.clone();
            profile.exclude_patterns = self.patterns.clone();
            profile.one_file_system = self.one_file_system;
            profile.exclude_caches = self.exclude_caches;
            profile.git_ignore = self.git_ignore;
            profile.skip_if_unchanged = self.skip_if_unchanged;
        }
        if new || steps.contains(&Step::When) {
            profile.schedule = self.schedule;
            profile.retention = self.retention;
            profile.prune = self.prune;
        }
        // Create-time only: Stellarshot exposes no way to change this once
        // set (see `Wizard::append_only`'s own doc comment), so it is never
        // revisited through `Step::When` on an existing profile. `self.append_only`
        // stays at its default `false` for `Mode::Open`, which shows no
        // toggle for it; `tasks::finish` overwrites this with the truth
        // read back from the repository itself once it is opened, so an
        // existing append-only repository is still recognised as one.
        if new {
            profile.append_only = self.append_only;
        }
        let secret = (!self.mode.edits()).then(|| Secret::new(self.password.clone()));
        self.busy_since = Some(Instant::now());
        vec![Effect::Finish(Box::new(Finish {
            mode: self.mode.clone(),
            profile,
            secret,
            remember: self.remember,
        }))]
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::AddSources => vec![Effect::PickFolders { excludes: false }],
            Message::AddExcludes => vec![Effect::PickFolders { excludes: true }],
            Message::SourcesChosen(paths) => {
                for path in paths {
                    if !self.sources.contains(&path) {
                        self.sources.push(path);
                    }
                }
                self.restart_estimate()
            }
            Message::ExcludesChosen(paths) => {
                for path in paths {
                    if !self.excludes.contains(&path) {
                        self.excludes.push(path);
                    }
                }
                self.restart_estimate()
            }
            Message::RemoveSource(index) => {
                if index < self.sources.len() {
                    self.sources.remove(index);
                }
                self.restart_estimate()
            }
            Message::RemoveExclude(index) => {
                if index < self.excludes.len() {
                    self.excludes.remove(index);
                }
                self.restart_estimate()
            }
            Message::PatternInput(text) => {
                self.pattern_input = text;
                Vec::new()
            }
            Message::AddPattern => {
                let pattern = self.pattern_input.trim().to_owned();
                self.pattern_input.clear();
                if pattern.is_empty() || self.patterns.contains(&pattern) {
                    return Vec::new();
                }
                self.patterns.push(pattern);
                self.restart_estimate()
            }
            Message::RemovePattern(index) => {
                if index < self.patterns.len() {
                    self.patterns.remove(index);
                }
                self.restart_estimate()
            }
            Message::OneFileSystem(on) => {
                self.one_file_system = on;
                self.restart_estimate()
            }
            Message::ExcludeCaches(on) => {
                self.exclude_caches = on;
                Vec::new()
            }
            Message::GitIgnore(on) => {
                self.git_ignore = on;
                Vec::new()
            }
            Message::SkipIfUnchanged(on) => {
                self.skip_if_unchanged = on;
                Vec::new()
            }
            Message::Estimate(generation, event) => {
                if generation == self.estimate.generation {
                    match event {
                        EstimateEvent::Progress(total) => self.estimate.total = Some(total),
                        EstimateEvent::Done(total) => self.estimate.total = Some(total),
                        EstimateEvent::Breakdown(folders, breakdown) => {
                            self.estimate.exclude_sizes = folders
                                .into_iter()
                                .zip(breakdown.per_folder.iter().copied())
                                .collect();
                            self.estimate.breakdown = Some(breakdown);
                            self.estimate.running = false;
                        }
                        EstimateEvent::Failed(_) => self.estimate.running = false,
                    }
                }
                Vec::new()
            }
            Message::Name(name) => {
                self.name_chosen = !name.trim().is_empty();
                self.name = name;
                Vec::new()
            }
            Message::Place(message) => {
                let effects = self.place.update(message);
                // Follows the destination, including a host name as it is
                // typed, until a name of its own is typed.
                if !self.name_chosen
                    && let Some(name) = self.place.suggested_name()
                {
                    self.name = name;
                }
                let mut effects = self.place_effects(effects);
                if self.advance_when_checked && self.place.checking_since().is_none() {
                    // The check Next started has finished, or the
                    // destination changed under it.
                    self.advance_when_checked = false;
                    if self.step == Step::Where && self.place_accepted() {
                        effects.extend(self.advance());
                    }
                }
                effects
            }
            Message::Password(password) => {
                self.password = password;
                Vec::new()
            }
            Message::Confirm(confirm) => {
                self.confirm = confirm;
                Vec::new()
            }
            Message::TogglePasswordVisible => {
                self.password_hidden = !self.password_hidden;
                Vec::new()
            }
            Message::Remember(remember) => {
                self.remember = remember;
                Vec::new()
            }
            Message::Automatic(on) => {
                self.schedule = if on { self.frequency } else { Schedule::Manual };
                Vec::new()
            }
            Message::Frequency(index) => {
                if let Some(frequency) = FREQUENCIES.get(index) {
                    self.set_schedule(*frequency);
                }
                Vec::new()
            }
            Message::Keep(index) => {
                if let Some(retention) = KEEP_CHOICES.get(index) {
                    self.retention = *retention;
                }
                Vec::new()
            }
            Message::Prune(on) => {
                self.prune = Some(on);
                Vec::new()
            }
            Message::AppendOnly(on) => {
                self.append_only = on;
                Vec::new()
            }
            Message::Back => {
                let position = self.position();
                if position > 0 {
                    self.step = self.mode.steps()[position - 1];
                }
                Vec::new()
            }
            Message::Next => {
                if !self.can_advance() {
                    return Vec::new();
                }
                if self.step == Step::Where && !self.place_accepted() {
                    self.advance_when_checked = true;
                    let effects = self.place.check();
                    return self.place_effects(effects);
                }
                self.advance()
            }
            Message::Cancel => vec![Effect::ConfirmCancel],
            Message::Finished(result) => {
                self.busy_since = None;
                match result {
                    Ok(()) => {
                        self.cancel.store(true, Ordering::Relaxed);
                        vec![Effect::Close]
                    }
                    // The application shows the error; the wizard stays open
                    // so nothing typed is lost.
                    Err(_) => Vec::new(),
                }
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let steps = self.mode.steps();
        let title = match self.mode {
            Mode::Create => fl!("wizard-create-title"),
            Mode::Open if self.importing => fl!("dejadup-title"),
            Mode::Open => fl!("wizard-open-title"),
            Mode::Edit { .. } => fl!("wizard-edit-title", name = self.name.clone()),
            Mode::Schedule { .. } => fl!("wizard-schedule-title", name = self.name.clone()),
        };
        let current = (self.position() + 1) as i64;
        let total = steps.len() as i64;
        let step_label = fl!("wizard-step", current = current, total = total);

        let body = match self.step {
            Step::What => self.what_view(),
            Step::Where => self.where_view(),
            Step::When => self.when_view(),
            Step::Secure => self.secure_view(),
        };

        let time = |since: Instant| format::duration(since.elapsed().as_secs());
        let next_label = match (self.busy_since, self.is_last_step(), &self.mode) {
            (Some(since), _, Mode::Create) => fl!("wizard-creating", time = time(since)),
            (Some(since), _, Mode::Open) => fl!("wizard-opening", time = time(since)),
            (Some(_), _, _) => fl!("wizard-saving"),
            (None, _, _) if self.step == Step::Where && self.advance_when_checked => {
                match self.place.checking_since() {
                    Some(since) => fl!("place-checking-for", time = time(since)),
                    None => fl!("next"),
                }
            }
            (None, false, _) => fl!("next"),
            (None, true, Mode::Create) => fl!("wizard-finish-create"),
            (None, true, Mode::Open) => fl!("wizard-finish-open"),
            (None, true, Mode::Edit { .. } | Mode::Schedule { .. }) => fl!("save"),
        };
        // Creating a repository on cloud storage takes a while; say so where
        // the button that started it is.
        let busy_note = match (self.busy(), &self.mode) {
            (true, Mode::Create) => Some(fl!("wizard-creating-note")),
            (true, Mode::Open) => Some(fl!("wizard-opening-note")),
            _ => None,
        };
        let mut footer = widget::row::with_capacity(5)
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
            .push(widget::button::standard(fl!("cancel")).on_press(Message::Cancel))
            .push_maybe(busy_note.map(widget::text::caption))
            .push(widget::space::horizontal());
        if self.position() > 0 {
            footer = footer.push(widget::button::standard(fl!("back")).on_press(Message::Back));
        }
        footer = footer.push(
            widget::button::suggested(next_label)
                .on_press_maybe(self.can_advance().then_some(Message::Next)),
        );

        widget::column::with_capacity(4)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .push(
                widget::column::with_capacity(2)
                    .push(widget::text::title3(title))
                    .push(widget::text::caption(step_label)),
            )
            .push(
                // Room inside the scrolled area: on the left for the focus
                // ring a field draws just outside itself, which the edge of
                // the area would otherwise cut off, and on the right for the
                // scrollbar, which would otherwise cover every row.
                widget::container(body)
                    .padding([0, spacing.space_s, 0, spacing.space_xxxs])
                    .apply(widget::scrollable)
                    .height(Length::Fill),
            )
            .push(footer)
            .apply(widget::container)
            .max_width(760)
            .apply(widget::container)
            .center_x(Length::Fill)
            .into()
    }

    fn what_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        // Each include shows everything it holds once that is known, as the
        // arithmetic under the estimate counts it; until then, what the walk
        // has found so far.
        let per_source = match &self.estimate.breakdown {
            Some(breakdown) => Some(&breakdown.per_source),
            None => self.estimate.total.as_ref().map(|total| &total.per_source),
        };

        let mut include = widget::settings::section().title(fl!("wizard-include"));
        for (index, source) in self.sources.iter().enumerate() {
            let size = per_source
                .and_then(|sizes| sizes.get(index))
                .map(|bytes| format::bytes(*bytes))
                .unwrap_or_default();
            include = include.add(path_row(source, size, Message::RemoveSource(index)));
        }
        include = include
            .add(widget::button::text(fl!("wizard-add-folders")).on_press(Message::AddSources));

        let mut exclude = widget::settings::section().title(fl!("wizard-exclude"));
        for (index, path) in self.excludes.iter().enumerate() {
            let size = if self.exclude_applies(path) {
                // Nothing to show for a folder that holds nothing (or does
                // not exist): "−0 B" reads like an error.
                self.estimate
                    .exclude_sizes
                    .get(path)
                    .filter(|bytes| **bytes > 0)
                    .map(|bytes| format!("−{}", format::bytes(*bytes)))
                    .unwrap_or_default()
            } else {
                fl!("wizard-exclude-outside")
            };
            exclude = exclude.add(path_row(path, size, Message::RemoveExclude(index)));
        }
        exclude = exclude
            .add(widget::button::text(fl!("wizard-add-folders")).on_press(Message::AddExcludes));

        let by_patterns = match (&self.estimate.breakdown, &self.estimate.total) {
            (Some(breakdown), Some(total)) if !self.patterns.is_empty() => {
                Some(breakdown.by_patterns(total.bytes)).filter(|bytes| *bytes > 0)
            }
            _ => None,
        };
        let mut advanced = widget::settings::section().title(fl!("wizard-advanced"));
        if let Some(bytes) = by_patterns {
            advanced = advanced.add(widget::text::caption(fl!(
                "wizard-patterns-remove",
                size = format::bytes(bytes)
            )));
        }
        for (index, pattern) in self.patterns.iter().enumerate() {
            advanced = advanced.add(
                widget::settings::item::builder(pattern.clone()).control(
                    widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                        .tooltip(fl!("remove"))
                        .name(fl!("remove"))
                        .on_press(Message::RemovePattern(index)),
                ),
            );
        }
        advanced = advanced
            .add(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xs)
                    .align_y(Alignment::Center)
                    .push(
                        widget::text_input(fl!("wizard-pattern-placeholder"), &self.pattern_input)
                            .on_input(Message::PatternInput)
                            .on_submit(|_| Message::AddPattern)
                            .width(Length::Fill),
                    )
                    .push(widget::button::standard(fl!("add")).on_press(Message::AddPattern)),
            )
            .add(
                widget::settings::item::builder(fl!("wizard-one-file-system"))
                    .description(fl!("wizard-one-file-system-description"))
                    .toggler(self.one_file_system, Message::OneFileSystem),
            )
            .add(
                widget::settings::item::builder(fl!("wizard-exclude-caches"))
                    .description(fl!("wizard-exclude-caches-description"))
                    .toggler(self.exclude_caches, Message::ExcludeCaches),
            )
            .add(
                widget::settings::item::builder(fl!("wizard-git-ignore"))
                    .description(fl!("wizard-git-ignore-description"))
                    .toggler(self.git_ignore, Message::GitIgnore),
            )
            .add(
                widget::settings::item::builder(fl!("wizard-skip-if-unchanged"))
                    .description(fl!("wizard-skip-if-unchanged-description"))
                    .toggler(self.skip_if_unchanged, Message::SkipIfUnchanged),
            );

        widget::column::with_capacity(5)
            .spacing(spacing.space_m)
            .push(widget::text::body(fl!("wizard-what-intro")))
            .push(self.estimate_card())
            .push(include)
            .push(exclude)
            .push(advanced)
            .into()
    }

    fn estimate_card(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let total = self.estimate.total.clone().unwrap_or_default();
        let files = total.files as i64;
        let headline = fl!(
            "wizard-estimate",
            size = format::bytes(total.bytes),
            files = files
        );
        // "45 GB included − 12 GB excluded = 33 GB", once both walks are in.
        let arithmetic = self.estimate.breakdown.as_ref().map(|breakdown| {
            let excluded = breakdown.excluded(total.bytes);
            if excluded == 0 {
                fl!("wizard-estimate-nothing-excluded")
            } else {
                fl!(
                    "wizard-estimate-arithmetic",
                    included = format::bytes(breakdown.included),
                    excluded = format::bytes(excluded),
                    total = format::bytes(total.bytes)
                )
            }
        });
        let status = match (&self.estimate.total, self.estimate.running) {
            (None, true) => fl!("wizard-estimate-counting"),
            (Some(_), true) if self.estimate.breakdown.is_none() => {
                fl!("wizard-estimate-adding-up")
            }
            _ => fl!("wizard-estimate-note"),
        };
        widget::column::with_capacity(4)
            .spacing(spacing.space_xxs)
            .push(widget::text::caption(fl!("wizard-estimate-label")))
            .push(widget::text::title4(headline))
            .push_maybe(arithmetic.map(widget::text::body))
            .push(widget::text::caption(status))
            .apply(widget::container)
            .padding(spacing.space_s)
            .class(theme::Container::Card)
            .width(Length::Fill)
            .into()
    }

    fn where_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        widget::column::with_capacity(3)
            .spacing(spacing.space_m)
            .push(widget::text::body(fl!("wizard-where-intro")))
            .push(
                self.place
                    .view(self.mode == Mode::Open, self.importing)
                    .map(Message::Place),
            )
            .push(
                widget::settings::section().title(fl!("wizard-name")).add(
                    widget::text_input(fl!("wizard-name-placeholder"), &self.name)
                        .on_input(Message::Name),
                ),
            )
            .into()
    }

    fn when_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let automatic = self.schedule != Schedule::Manual;
        let mut when = widget::settings::section().add(
            widget::settings::item::builder(fl!("wizard-automatic"))
                .description(fl!("wizard-automatic-description"))
                .toggler(automatic, Message::Automatic),
        );
        if automatic {
            let selected = FREQUENCIES.iter().position(|f| *f == self.frequency);
            when =
                when.add(
                    widget::settings::item::builder(fl!("wizard-frequency")).control(
                        widget::dropdown(&self.frequency_labels, selected, Message::Frequency),
                    ),
                );
        }

        let selected = KEEP_CHOICES
            .iter()
            .position(|r| *r == self.retention)
            .unwrap_or(KEEP_CHOICES.len());
        let mut keep = widget::settings::section().title(fl!("wizard-keep")).add(
            widget::settings::item::builder(fl!("wizard-keep-label"))
                .description(retention_description(self.retention))
                .control(widget::dropdown(
                    &self.keep_labels,
                    Some(selected),
                    Message::Keep,
                )),
        );
        if self.retention != Retention::KeepForever {
            keep = keep.add(
                widget::settings::item::builder(fl!("wizard-prune"))
                    .description(fl!("wizard-prune-description"))
                    .toggler(self.prune_enabled(), Message::Prune),
            );
        }
        if self.mode == Mode::Create {
            keep = keep.add(
                widget::settings::item::builder(fl!("wizard-append-only"))
                    .description(fl!("wizard-append-only-description"))
                    .toggler(self.append_only, Message::AppendOnly),
            );
        }

        widget::column::with_capacity(3)
            .spacing(spacing.space_m)
            .push(widget::text::body(fl!("wizard-when-intro")))
            .push(when)
            .push(keep)
            .into()
    }

    fn secure_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let mut fields = widget::column::with_capacity(3)
            .spacing(spacing.space_xs)
            .push(
                widget::secure_input(
                    fl!("password"),
                    &self.password,
                    Some(Message::TogglePasswordVisible),
                    self.password_hidden,
                )
                .label(fl!("password"))
                .on_input(Message::Password),
            );
        let intro = if self.mode == Mode::Create {
            fields = fields.push(
                widget::secure_input(
                    fl!("wizard-confirm"),
                    &self.confirm,
                    Some(Message::TogglePasswordVisible),
                    self.password_hidden,
                )
                .label(fl!("wizard-confirm"))
                .on_input(Message::Confirm)
                .on_submit(|_| Message::Next),
            );
            if !self.confirm.is_empty() && self.password != self.confirm {
                fields = fields.push(widget::text::caption(fl!("wizard-mismatch")));
            }
            fl!("wizard-secure-intro")
        } else {
            fl!("wizard-open-intro")
        };

        widget::column::with_capacity(4)
            .spacing(spacing.space_m)
            .push(widget::text::body(intro))
            .push(fields)
            .push(
                widget::settings::section().add(
                    widget::settings::item::builder(fl!("remember-password"))
                        .description(fl!("remember-password-description"))
                        .toggler(self.remember, Message::Remember),
                ),
            )
            .push_maybe(
                (self.schedule != Schedule::Manual && !self.remember)
                    .then(|| widget::text::caption(fl!("wizard-remember-for-schedule"))),
            )
            .push(
                widget::text::body(fl!("wizard-password-warning"))
                    .apply(widget::container)
                    .padding(spacing.space_s)
                    .class(theme::Container::Card),
            )
            .into()
    }
}

/// What a retention choice is called in the list.
pub fn retention_label(retention: Retention) -> String {
    match retention {
        Retention::Smart => fl!("keep-smart"),
        Retention::KeepForever => fl!("keep-forever"),
        Retention::KeepFor { days: 90 } => fl!("keep-3-months"),
        Retention::KeepFor { days: 182 } => fl!("keep-6-months"),
        Retention::KeepFor { days: 365 } => fl!("keep-1-year"),
        Retention::KeepFor { days } => fl!("keep-days", days = (days as i64)),
    }
}

/// A sentence saying what a retention choice does.
pub fn retention_description(retention: Retention) -> String {
    match retention {
        Retention::Smart => fl!("keep-smart-description"),
        Retention::KeepForever => fl!("keep-forever-description"),
        Retention::KeepFor { days } => fl!("keep-for-description", days = (days as i64)),
    }
}

/// A path with its size and a remove button.
fn path_row(path: &Path, detail: String, remove: Message) -> Element<'_, Message> {
    widget::settings::item::builder(format::path(path))
        .description(detail)
        .control(
            widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
                .tooltip(fl!("remove"))
                .name(fl!("remove"))
                .on_press(remove),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ErrorKind;

    fn finish_effect(effects: &[Effect]) -> Option<&Finish> {
        effects.iter().find_map(|effect| match effect {
            Effect::Finish(finish) => Some(finish.as_ref()),
            _ => None,
        })
    }

    #[test]
    fn create_starts_with_home_and_the_default_excludes() {
        let (wizard, effects) = Wizard::create(Some(Path::new("/home/dave")));
        assert_eq!(wizard.sources, vec![PathBuf::from("/home/dave")]);
        assert!(
            wizard
                .excludes
                .contains(&PathBuf::from("/home/dave/.cache"))
        );
        assert!(matches!(effects.as_slice(), [Effect::Estimate { .. }]));
    }

    #[test]
    fn cannot_leave_what_without_a_source() {
        let (mut wizard, _) = Wizard::create(None);
        assert!(!wizard.can_advance());
        wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::What);

        wizard.update(Message::SourcesChosen(vec!["/data".into()]));
        assert!(wizard.can_advance());
    }

    /// Choose a folder in the "where" step and deliver its probe result.
    fn place_folder(wizard: &mut Wizard, path: &str, probe: Result<Probe, EngineError>) {
        wizard.update(Message::Place(place::Message::FolderChosen(path.into())));
        let destination = wizard.place.destination().expect("a folder was chosen");
        wizard.update(Message::Place(place::Message::Probed(destination, probe)));
    }

    #[test]
    fn not_empty_location_blocks_next() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        let effects = wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::Where);
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::Place(place::Effect::ListDrives))),
            "entering the step looks for drives"
        );

        place_folder(&mut wizard, "/home/dave", Ok(Probe::NotEmpty));
        assert!(!wizard.can_advance(), "a folder of other files is refused");

        place_folder(&mut wizard, "/srv/backups/laptop", Ok(Probe::Empty));
        assert!(wizard.can_advance());
        assert_eq!(wizard.name, "laptop", "the name follows the place chosen");
    }

    #[test]
    fn the_suggested_name_follows_typing_until_a_name_is_typed() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/alex")));
        wizard.update(Message::Next);
        wizard.update(Message::Place(place::Message::Kind(place::Kind::Server)));
        for host in ["n", "na", "nas.local"] {
            wizard.update(Message::Place(place::Message::Host(host.into())));
        }
        assert_eq!(wizard.name, "nas.local", "not frozen at the first letter");

        wizard.update(Message::Name("Home to the NAS".into()));
        wizard.update(Message::Place(place::Message::Host("nas2.local".into())));
        assert_eq!(wizard.name, "Home to the NAS", "a typed name is kept");
    }

    #[test]
    fn a_stale_probe_result_is_ignored() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        wizard.update(Message::Place(place::Message::FolderChosen("/b".into())));
        wizard.update(Message::Place(place::Message::Probed(
            Destination::Local { path: "/a".into() },
            Ok(Probe::Empty),
        )));
        assert!(wizard.place.probe().is_none());
    }

    #[test]
    fn create_needs_matching_passwords() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        wizard.update(Message::Next);
        place_folder(&mut wizard, "/srv/backups/laptop", Ok(Probe::Empty));
        wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::When);
        wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::Secure);

        wizard.update(Message::Password("secret".into()));
        wizard.update(Message::Confirm("secrte".into()));
        assert!(!wizard.can_advance());

        wizard.update(Message::Confirm("secret".into()));
        let effects = wizard.update(Message::Next);
        let finish = finish_effect(&effects).expect("finishing");
        assert_eq!(finish.mode, Mode::Create);
        assert_eq!(finish.secret.as_ref().map(Secret::expose), Some("secret"));
        assert!(finish.remember, "remembering is on by default");
        assert_eq!(
            finish.profile.destination,
            Destination::Local {
                path: "/srv/backups/laptop".into()
            }
        );
        assert!(wizard.busy());
    }

    #[test]
    fn open_needs_an_existing_repository() {
        let (mut wizard, effects) = Wizard::open();
        assert_eq!(wizard.step, Step::Where);
        assert!(!effects.is_empty(), "opening looks for drives and rclone");
        place_folder(&mut wizard, "/srv/backups/laptop", Ok(Probe::Empty));
        assert!(
            !wizard.can_advance(),
            "an empty folder holds nothing to open"
        );

        place_folder(&mut wizard, "/srv/backups/laptop", Ok(Probe::Repository));
        assert!(wizard.can_advance());
    }

    #[test]
    fn an_unreachable_destination_is_checked_again_by_next() {
        let (mut wizard, _) = Wizard::open();
        place_folder(
            &mut wizard,
            "/gone",
            Err(EngineError::new(ErrorKind::DestinationUnavailable, "/gone")),
        );
        assert!(wizard.can_advance(), "Next tries again");

        let effects = wizard.update(Message::Next);
        assert!(
            matches!(effects.as_slice(), [Effect::Place(place::Effect::Probe(_))]),
            "Next checks again rather than moving on"
        );
        assert_eq!(wizard.step, Step::Where);
        assert!(!wizard.can_advance(), "not while the check runs");
        let destination = wizard.place.destination().unwrap();
        wizard.update(Message::Place(place::Message::Probed(
            destination,
            Err(EngineError::new(ErrorKind::TimedOut, "60")),
        )));
        assert_eq!(wizard.step, Step::Where, "a failed check stays put");
    }

    /// Fill in an SFTP destination, which is not checked until asked.
    fn describe_server(wizard: &mut Wizard) {
        for message in [
            place::Message::Kind(place::Kind::Server),
            place::Message::Host("nas.local".into()),
            place::Message::ServerPath("backups/laptop".into()),
        ] {
            wizard.update(Message::Place(message));
        }
    }

    #[test]
    fn one_press_of_next_checks_the_destination_and_moves_on() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/alex")));
        wizard.update(Message::Next);
        describe_server(&mut wizard);
        assert!(wizard.place.probe().is_none(), "not checked yet");
        assert!(wizard.can_advance(), "Next can be pressed straight away");

        let effects = wizard.update(Message::Next);
        assert!(matches!(
            effects.as_slice(),
            [Effect::Place(place::Effect::Probe(_))]
        ));
        assert!(wizard.waiting(), "the check shows its running time");
        assert!(
            !wizard.can_advance(),
            "a second press does not start another"
        );

        let destination = wizard.place.destination().unwrap();
        wizard.update(Message::Place(place::Message::Probed(
            destination,
            Ok(Probe::Empty),
        )));
        assert_eq!(wizard.step, Step::When, "moved on without another press");
        assert!(!wizard.waiting());
    }

    #[test]
    fn a_check_that_finds_other_files_does_not_move_on() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/alex")));
        wizard.update(Message::Next);
        describe_server(&mut wizard);
        wizard.update(Message::Next);
        let destination = wizard.place.destination().unwrap();
        wizard.update(Message::Place(place::Message::Probed(
            destination,
            Ok(Probe::NotEmpty),
        )));
        assert_eq!(wizard.step, Step::Where);
        assert!(!wizard.can_advance());
    }

    #[test]
    fn a_check_for_an_edited_destination_does_not_move_on() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/alex")));
        wizard.update(Message::Next);
        describe_server(&mut wizard);
        wizard.update(Message::Next);
        let checked = wizard.place.destination().unwrap();
        wizard.update(Message::Place(place::Message::ServerPath(
            "backups/other".into(),
        )));
        wizard.update(Message::Place(place::Message::Probed(
            checked,
            Ok(Probe::Empty),
        )));
        assert_eq!(wizard.step, Step::Where, "the new path is still unchecked");
    }

    #[test]
    fn editing_keeps_the_profile_id_and_needs_no_password() {
        let mut profile = Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/media/usb/backup".into(),
            },
            vec!["/home/dave".into()],
        );
        profile.id = "keep-me".into();
        let (mut wizard, _) = Wizard::edit(&profile);

        let effects = wizard.update(Message::Next);

        let finish = finish_effect(&effects).expect("saving");
        assert_eq!(finish.profile.id, "keep-me");
        assert!(finish.secret.is_none());
        assert_eq!(
            finish.profile.destination, profile.destination,
            "the destination is kept"
        );
    }

    #[test]
    fn a_dejadup_import_keeps_its_folders_but_asks_for_the_password() {
        let import = crate::dejadup::Import {
            sources: vec!["/home/alex".into()],
            excludes: vec!["/home/alex/.cache".into()],
            place: crate::dejadup::Place::Drive {
                uuid: "1111-AAAA".into(),
                folder: "laptop".into(),
                label: "Backup".into(),
            },
            other_format: false,
            schedule: Schedule::Weekly,
            retention: Retention::KeepFor { days: 182 },
        };
        let (mut wizard, _) = Wizard::import(&import);
        assert_eq!(wizard.mode, Mode::Open);
        assert!(wizard.password.is_empty(), "the password is never imported");
        assert_eq!(wizard.schedule, Schedule::Weekly, "Déjà Dup's schedule");
        assert_eq!(wizard.retention, Retention::KeepFor { days: 182 });

        // The drive with the recorded UUID is selected once drives are known.
        wizard.update(Message::Place(place::Message::DrivesListed(vec![
            crate::drives::Drive {
                uuid: "other".into(),
                label: "Other".into(),
                mount_point: "/media/alex/Other".into(),
            },
            crate::drives::Drive {
                uuid: "1111-AAAA".into(),
                label: "Backup".into(),
                mount_point: "/media/alex/Backup".into(),
            },
        ])));
        match wizard.place.destination().unwrap() {
            Destination::Removable {
                uuid,
                relative_path,
                ..
            } => {
                assert_eq!(uuid, "1111-AAAA");
                assert_eq!(relative_path, PathBuf::from("laptop"));
            }
            other => panic!("expected the drive, got {other:?}"),
        }

        let destination = wizard.place.destination().unwrap();
        wizard.update(Message::Place(place::Message::Probed(
            destination,
            Ok(Probe::Repository),
        )));
        wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::When);
        wizard.update(Message::Next);
        wizard.update(Message::Password("typed by the user".into()));
        let effects = wizard.update(Message::Next);
        let finish = finish_effect(&effects).expect("finishing");
        assert_eq!(finish.profile.schedule, Schedule::Weekly);
        assert_eq!(finish.profile.retention, Retention::KeepFor { days: 182 });
        assert_eq!(
            finish.profile.excludes,
            vec![PathBuf::from("/home/alex/.cache")]
        );
    }

    #[test]
    fn estimates_from_an_older_generation_are_ignored() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        let old = wizard.estimate.generation;
        wizard.update(Message::SourcesChosen(vec!["/data".into()]));

        let stale = SizeEstimate {
            files: 1,
            bytes: 1,
            per_source: vec![1],
        };
        wizard.update(Message::Estimate(old, EstimateEvent::Done(stale)));

        assert!(wizard.estimate.total.is_none());
        assert!(wizard.estimate.running);
    }

    #[test]
    fn excludes_outside_every_source_do_not_apply() {
        let (wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        assert!(wizard.exclude_applies(Path::new("/home/dave/.cache")));
        assert!(!wizard.exclude_applies(Path::new("/var/tmp")));
    }

    fn scheduled_profile() -> Profile {
        let mut profile = Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/mnt/backup/home".into(),
            },
            vec!["/home/alex".into()],
        );
        profile.excludes = vec!["/home/alex/.cache".into()];
        profile.schedule = Schedule::Daily;
        profile.retention = Retention::Smart;
        profile.last_success = Some(42);
        profile
    }

    #[test]
    fn a_new_backup_runs_daily_and_keeps_a_smart_history() {
        let (wizard, _) = Wizard::create(Some(Path::new("/home/alex")));
        assert_eq!(wizard.schedule, Schedule::Daily);
        assert_eq!(wizard.retention, Retention::Smart);
        assert_eq!(
            wizard.prune, None,
            "the destination decides until the user does"
        );
    }

    #[test]
    fn editing_the_schedule_changes_nothing_else() {
        let profile = scheduled_profile();
        let (mut wizard, _) = Wizard::schedule(&profile);
        assert_eq!(wizard.step, Step::When);

        wizard.update(Message::Automatic(false));
        let forever = KEEP_CHOICES
            .iter()
            .position(|r| *r == Retention::KeepForever)
            .unwrap();
        wizard.update(Message::Keep(forever));
        let effects = wizard.update(Message::Next);

        let finish = finish_effect(&effects).expect("saving finishes");
        assert!(finish.secret.is_none(), "the repository is not touched");
        let saved = &finish.profile;
        assert_eq!(saved.schedule, Schedule::Manual);
        assert_eq!(saved.retention, Retention::KeepForever);
        assert_eq!(
            (
                &saved.id,
                &saved.name,
                &saved.sources,
                &saved.excludes,
                &saved.destination
            ),
            (
                &profile.id,
                &profile.name,
                &profile.sources,
                &profile.excludes,
                &profile.destination
            )
        );
        assert_eq!(saved.last_success, Some(42));
    }

    #[test]
    fn editing_the_folders_keeps_the_schedule() {
        let profile = scheduled_profile();
        let (mut wizard, _) = Wizard::edit(&profile);
        wizard.update(Message::SourcesChosen(vec!["/srv/projects".into()]));
        let effects = wizard.update(Message::Next);

        let saved = &finish_effect(&effects).expect("saving finishes").profile;
        assert_eq!(saved.sources.len(), 2);
        assert_eq!(saved.schedule, Schedule::Daily);
        assert_eq!(saved.retention, Retention::Smart);
    }

    #[test]
    fn turning_automatic_backups_back_on_restores_the_frequency() {
        let (mut wizard, _) = Wizard::schedule(&scheduled_profile());
        wizard.update(Message::Frequency(2));
        assert_eq!(wizard.schedule, Schedule::Weekly);
        wizard.update(Message::Automatic(false));
        assert_eq!(wizard.schedule, Schedule::Manual);
        wizard.update(Message::Automatic(true));
        assert_eq!(wizard.schedule, Schedule::Weekly);
    }

    #[test]
    fn freeing_space_follows_the_destination_until_chosen() {
        let (mut wizard, _) = Wizard::schedule(&scheduled_profile());
        assert!(
            wizard.prune_enabled(),
            "a local folder frees space by default"
        );
        wizard.update(Message::Prune(false));
        assert!(!wizard.prune_enabled());
    }
}
