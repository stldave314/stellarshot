// SPDX-License-Identifier: GPL-3.0-only

//! The setup wizard: what to back up, where to, and the password.
//!
//! The same wizard creates a backup, opens an existing one, and edits what an
//! existing profile covers; each mode shows only the steps it needs. All state
//! lives here and every side effect is returned as an [`Effect`] for the
//! application to run, so the step logic is testable without a window.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::format;
use crate::engine::{BackupRequest, EngineError, Probe, Secret, SizeEstimate};
use crate::fl;
use crate::profile::{Destination, Profile, default_excludes};

/// What the wizard is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Set up a new backup and create its repository.
    Create,
    /// Add a repository that already exists.
    Open,
    /// Change what an existing profile backs up.
    Edit { profile_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    What,
    Where,
    Secure,
}

impl Mode {
    fn steps(&self) -> &'static [Step] {
        match self {
            Self::Create => &[Step::What, Step::Where, Step::Secure],
            Self::Open => &[Step::Where, Step::Secure],
            Self::Edit { .. } => &[Step::What],
        }
    }
}

/// The size estimate as the wizard shows it.
#[derive(Debug, Default)]
pub struct EstimateView {
    /// Incremented whenever the lists change; results for an older generation
    /// are stale and ignored.
    pub generation: u64,
    pub running: bool,
    pub total: Option<SizeEstimate>,
    /// Size of each excluded folder that sits inside an included one.
    pub exclude_sizes: HashMap<PathBuf, u64>,
}

/// Progress of a size estimate, as it reaches the wizard.
#[derive(Debug, Clone)]
pub enum EstimateEvent {
    Progress(SizeEstimate),
    Done(SizeEstimate),
    ExcludeSize(PathBuf, u64),
    Failed(String),
}

pub struct Wizard {
    pub mode: Mode,
    pub step: Step,
    pub name: String,
    pub sources: Vec<PathBuf>,
    pub excludes: Vec<PathBuf>,
    pub patterns: Vec<String>,
    pub pattern_input: String,
    pub one_file_system: bool,
    pub destination: Option<PathBuf>,
    pub probe: Option<Result<Probe, EngineError>>,
    pub password: String,
    pub confirm: String,
    pub password_hidden: bool,
    pub remember: bool,
    pub estimate: EstimateView,
    cancel: Arc<AtomicBool>,
    /// Finishing: the repository is being created or opened.
    pub busy: bool,
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
    Estimate(u64, EstimateEvent),
    Name(String),
    ChooseDestination,
    DestinationChosen(PathBuf),
    Probed(PathBuf, Result<Probe, EngineError>),
    Password(String),
    Confirm(String),
    TogglePasswordVisible,
    Remember(bool),
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
    PickDestination,
    /// Walk what `request` covers, and size `exclude_folders`, reporting under
    /// `generation`.
    Estimate {
        generation: u64,
        request: BackupRequest,
        exclude_folders: Vec<PathBuf>,
        cancel: Arc<AtomicBool>,
    },
    Probe(PathBuf),
    Finish(Finish),
    Close,
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
            sources: Vec::new(),
            excludes: Vec::new(),
            patterns: Vec::new(),
            pattern_input: String::new(),
            one_file_system: true,
            destination: None,
            probe: None,
            password: String::new(),
            confirm: String::new(),
            password_hidden: true,
            remember: true,
            estimate: EstimateView::default(),
            cancel: Arc::new(AtomicBool::new(false)),
            busy: false,
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

    pub fn open() -> Self {
        Self::new(Mode::Open)
    }

    pub fn edit(profile: &Profile) -> (Self, Vec<Effect>) {
        let mut wizard = Self::new(Mode::Edit {
            profile_id: profile.id.clone(),
        });
        wizard.name = profile.name.clone();
        wizard.sources = profile.sources.clone();
        wizard.excludes = profile.excludes.clone();
        wizard.patterns = profile.exclude_patterns.clone();
        wizard.one_file_system = profile.one_file_system;
        let Destination::Local { path } = &profile.destination;
        wizard.destination = Some(path.clone());
        let effects = wizard.restart_estimate();
        (wizard, effects)
    }

    fn request(&self) -> BackupRequest {
        BackupRequest {
            sources: self.sources.clone(),
            excludes: self.excludes.clone(),
            exclude_patterns: self.patterns.clone(),
            one_file_system: self.one_file_system,
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

    /// Whether the current step is complete enough to move on.
    pub fn can_advance(&self) -> bool {
        if self.busy {
            return false;
        }
        match self.step {
            Step::What => !self.sources.is_empty(),
            Step::Where => {
                let wanted = match self.mode {
                    Mode::Open => Probe::Repository,
                    _ => Probe::Empty,
                };
                !self.name.trim().is_empty()
                    && matches!(&self.probe, Some(Ok(probe)) if *probe == wanted)
            }
            Step::Secure => match self.mode {
                Mode::Create => !self.password.is_empty() && self.password == self.confirm,
                _ => !self.password.is_empty(),
            },
        }
    }

    fn finish(&mut self) -> Vec<Effect> {
        let Some(destination) = self.destination.clone() else {
            return Vec::new();
        };
        let mut profile = Profile::new(
            self.name.trim().to_owned(),
            Destination::Local { path: destination },
            self.sources.clone(),
        );
        profile.excludes = self.excludes.clone();
        profile.exclude_patterns = self.patterns.clone();
        profile.one_file_system = self.one_file_system;
        let secret = match self.mode {
            Mode::Edit { ref profile_id } => {
                profile.id = profile_id.clone();
                None
            }
            _ => Some(Secret::new(self.password.clone())),
        };
        self.busy = true;
        vec![Effect::Finish(Finish {
            mode: self.mode.clone(),
            profile,
            secret,
            remember: self.remember,
        })]
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
            Message::Estimate(generation, event) => {
                if generation == self.estimate.generation {
                    match event {
                        EstimateEvent::Progress(total) => self.estimate.total = Some(total),
                        EstimateEvent::Done(total) => {
                            self.estimate.total = Some(total);
                            self.estimate.running = false;
                        }
                        EstimateEvent::ExcludeSize(path, bytes) => {
                            self.estimate.exclude_sizes.insert(path, bytes);
                        }
                        EstimateEvent::Failed(_) => self.estimate.running = false,
                    }
                }
                Vec::new()
            }
            Message::Name(name) => {
                self.name = name;
                Vec::new()
            }
            Message::ChooseDestination => vec![Effect::PickDestination],
            Message::DestinationChosen(path) => {
                if self.name.trim().is_empty() {
                    self.name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default();
                }
                self.destination = Some(path.clone());
                self.probe = None;
                vec![Effect::Probe(path)]
            }
            Message::Probed(path, result) => {
                if self.destination.as_ref() == Some(&path) {
                    self.probe = Some(result);
                }
                Vec::new()
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
                if self.is_last_step() {
                    return self.finish();
                }
                self.step = self.mode.steps()[self.position() + 1];
                Vec::new()
            }
            Message::Cancel => {
                self.cancel.store(true, Ordering::Relaxed);
                vec![Effect::Close]
            }
            Message::Finished(result) => {
                self.busy = false;
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
            Mode::Open => fl!("wizard-open-title"),
            Mode::Edit { .. } => fl!("wizard-edit-title", name = self.name.clone()),
        };
        let current = (self.position() + 1) as i64;
        let total = steps.len() as i64;
        let step_label = fl!("wizard-step", current = current, total = total);

        let body = match self.step {
            Step::What => self.what_view(),
            Step::Where => self.where_view(),
            Step::Secure => self.secure_view(),
        };

        let next_label = match (self.is_last_step(), &self.mode) {
            (false, _) => fl!("next"),
            (true, Mode::Create) => fl!("wizard-finish-create"),
            (true, Mode::Open) => fl!("wizard-finish-open"),
            (true, Mode::Edit { .. }) => fl!("save"),
        };
        let mut footer = widget::row::with_capacity(4)
            .spacing(spacing.space_xs)
            .align_y(Alignment::Center)
            .push(widget::button::standard(fl!("cancel")).on_press(Message::Cancel))
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
            .push(widget::scrollable(body).height(Length::Fill))
            .push(footer)
            .apply(widget::container)
            .max_width(760)
            .apply(widget::container)
            .center_x(Length::Fill)
            .into()
    }

    fn what_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let per_source = self.estimate.total.as_ref().map(|total| &total.per_source);

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

        let mut advanced = widget::settings::section().title(fl!("wizard-advanced"));
        for (index, pattern) in self.patterns.iter().enumerate() {
            advanced = advanced.add(
                widget::settings::item::builder(pattern.clone()).control(
                    widget::button::icon(widget::icon::from_name("edit-delete-symbolic"))
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
        let status = if self.estimate.running {
            fl!("wizard-estimate-counting")
        } else {
            fl!("wizard-estimate-note")
        };
        widget::column::with_capacity(3)
            .spacing(spacing.space_xxs)
            .push(widget::text::caption(fl!("wizard-estimate-label")))
            .push(widget::text::title4(headline))
            .push(widget::text::caption(status))
            .apply(widget::container)
            .padding(spacing.space_s)
            .class(theme::Container::Card)
            .width(Length::Fill)
            .into()
    }

    fn where_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let folder = self
            .destination
            .as_ref()
            .map(|path| format::path(path))
            .unwrap_or_else(|| fl!("wizard-no-folder"));
        let verdict: Option<String> = match (&self.probe, &self.mode) {
            (None, _) => None,
            (Some(Ok(Probe::Empty)), Mode::Open) => Some(fl!("wizard-where-no-repository")),
            (Some(Ok(Probe::Empty)), _) => Some(fl!("wizard-where-new")),
            (Some(Ok(Probe::Repository)), Mode::Open) => Some(fl!("wizard-where-found")),
            (Some(Ok(Probe::Repository)), _) => Some(fl!("wizard-where-existing")),
            (Some(Ok(Probe::NotEmpty)), _) => Some(fl!("wizard-where-not-empty")),
            (Some(Err(err)), _) => Some(err.detail.clone()),
        };

        let mut location = widget::settings::section()
            .title(fl!("wizard-where-title"))
            .add(
                widget::settings::item::builder(folder).control(
                    widget::button::standard(fl!("wizard-choose-folder"))
                        .on_press(Message::ChooseDestination),
                ),
            );
        if let Some(verdict) = verdict {
            location = location.add(widget::text::body(verdict));
        }

        widget::column::with_capacity(3)
            .spacing(spacing.space_m)
            .push(widget::text::body(fl!("wizard-where-intro")))
            .push(location)
            .push(
                widget::settings::section().title(fl!("wizard-name")).add(
                    widget::text_input(fl!("wizard-name-placeholder"), &self.name)
                        .on_input(Message::Name),
                ),
            )
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
            .push(
                widget::text::body(fl!("wizard-password-warning"))
                    .apply(widget::container)
                    .padding(spacing.space_s)
                    .class(theme::Container::Card),
            )
            .into()
    }
}

/// A path with its size and a remove button.
fn path_row(path: &Path, detail: String, remove: Message) -> Element<'_, Message> {
    widget::settings::item::builder(format::path(path))
        .description(detail)
        .control(
            widget::button::icon(widget::icon::from_name("edit-delete-symbolic")).on_press(remove),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ErrorKind;

    fn finish_effect(effects: &[Effect]) -> Option<&Finish> {
        effects.iter().find_map(|effect| match effect {
            Effect::Finish(finish) => Some(finish),
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

    #[test]
    fn not_empty_location_blocks_next() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        wizard.update(Message::Next);
        assert_eq!(wizard.step, Step::Where);

        wizard.update(Message::DestinationChosen("/home/dave".into()));
        wizard.update(Message::Probed("/home/dave".into(), Ok(Probe::NotEmpty)));
        assert!(!wizard.can_advance(), "a folder of other files is refused");

        wizard.update(Message::DestinationChosen("/media/usb/backup".into()));
        wizard.update(Message::Probed(
            "/media/usb/backup".into(),
            Ok(Probe::Empty),
        ));
        assert!(wizard.can_advance());
        assert_eq!(
            wizard.name, "dave",
            "the name defaults to the first folder chosen"
        );
    }

    #[test]
    fn a_stale_probe_result_is_ignored() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        wizard.update(Message::DestinationChosen("/b".into()));
        wizard.update(Message::Probed("/a".into(), Ok(Probe::Empty)));
        assert!(wizard.probe.is_none());
    }

    #[test]
    fn create_needs_matching_passwords() {
        let (mut wizard, _) = Wizard::create(Some(Path::new("/home/dave")));
        wizard.update(Message::Next);
        wizard.update(Message::DestinationChosen("/media/usb/backup".into()));
        wizard.update(Message::Probed(
            "/media/usb/backup".into(),
            Ok(Probe::Empty),
        ));
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
        assert!(wizard.busy);
    }

    #[test]
    fn open_needs_an_existing_repository() {
        let mut wizard = Wizard::open();
        assert_eq!(wizard.step, Step::Where);
        wizard.update(Message::DestinationChosen("/media/usb/backup".into()));
        wizard.update(Message::Probed(
            "/media/usb/backup".into(),
            Ok(Probe::Empty),
        ));
        assert!(
            !wizard.can_advance(),
            "an empty folder holds nothing to open"
        );

        wizard.update(Message::Probed(
            "/media/usb/backup".into(),
            Ok(Probe::Repository),
        ));
        assert!(wizard.can_advance());
    }

    #[test]
    fn an_unreachable_destination_blocks_next() {
        let mut wizard = Wizard::open();
        wizard.update(Message::DestinationChosen("/gone".into()));
        wizard.update(Message::Probed(
            "/gone".into(),
            Err(EngineError::new(ErrorKind::DestinationUnavailable, "/gone")),
        ));
        assert!(!wizard.can_advance());
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
}
