// SPDX-License-Identifier: GPL-3.0-only

//! Getting files back: browse a snapshot, find what was deleted, compare two
//! snapshots, and restore a selection with a dry run first.
//!
//! The page keeps one [`Browser`] open while it is shown. Every side effect is
//! returned as an [`Effect`] for the application to run, so the page's logic
//! is testable without a window.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::format;
use crate::engine::{
    Browser, Change, ConflictPolicy, DiffEntry, EngineError, EntryKind, FileVersion, MissingEntry,
    Ownership, ProgressEvent, RestorePreview, RestoreRequest, SnapshotSummary, Target, TreeEntry,
};
use crate::fl;
use crate::runner::Event;

/// How far back "Deleted files" looks by default, in days.
const DELETED_WINDOW_DAYS: i64 = 30;
/// Most search results, deleted files and differences shown at once.
pub const RESULT_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Browse,
    Deleted,
    Compare,
}

/// Where the restore goes, as chosen in the sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetChoice {
    Original,
    Folder(Option<PathBuf>),
}

/// The restore sheet: what, where, what about existing files, and the dry run.
#[derive(Debug, Clone)]
pub struct Sheet {
    /// One request per snapshot the selection comes from.
    requests: Vec<RestoreRequest>,
    target: TargetChoice,
    policy: ConflictPolicy,
    /// Read and verify a file that already looks unchanged, rather than
    /// trusting its size and modification time.
    verify_existing: bool,
    ownership: Ownership,
    preview: Option<Result<RestorePreview, EngineError>>,
    previewing: bool,
}

impl Sheet {
    /// The requests with the chosen target, policy and options applied, if a
    /// target folder has been chosen where one is needed.
    fn finished_requests(&self) -> Option<Vec<RestoreRequest>> {
        let target = match &self.target {
            TargetChoice::Original => Target::Original,
            TargetChoice::Folder(Some(folder)) => Target::Folder(folder.clone()),
            TargetChoice::Folder(None) => return None,
        };
        Some(
            self.requests
                .iter()
                .map(|request| RestoreRequest {
                    target: target.clone(),
                    policy: self.policy,
                    verify_existing: self.verify_existing,
                    ownership: self.ownership,
                    ..request.clone()
                })
                .collect(),
        )
    }
}

/// A restore running in a child process.
struct Running {
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
    /// Requests still to run after this one.
    queue: VecDeque<RestoreRequest>,
}

pub struct RestorePage {
    pub profile_id: String,
    browser: Option<Arc<Browser>>,
    snapshots: Vec<SnapshotSummary>,
    tab: Tab,
    // Browse
    snapshot: Option<usize>,
    dir: PathBuf,
    root: PathBuf,
    entries: Option<Vec<TreeEntry>>,
    search: String,
    results: Option<Vec<TreeEntry>>,
    expanded: Option<PathBuf>,
    versions: Option<Vec<FileVersion>>,
    selection: BTreeSet<PathBuf>,
    // Deleted files
    scope: PathBuf,
    missing: Option<Vec<MissingEntry>>,
    missing_selection: BTreeSet<PathBuf>,
    // Compare
    from: Option<usize>,
    to: Option<usize>,
    diff: Option<Vec<DiffEntry>>,
    diff_selection: BTreeSet<PathBuf>,
    // Restoring
    sheet: Option<Sheet>,
    running: Option<Running>,
    busy: bool,
    labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Arc<Browser>, EngineError>),
    Tab(Tab),
    PickSnapshot(usize),
    Open(PathBuf),
    Up,
    Listed(PathBuf, Result<Vec<TreeEntry>, EngineError>),
    Search(String),
    SearchNow,
    Found(Result<Vec<TreeEntry>, EngineError>),
    ShowVersions(PathBuf),
    VersionsLoaded(PathBuf, Result<Vec<FileVersion>, EngineError>),
    Toggle(PathBuf, bool),
    ChooseScope,
    ScopeChosen(PathBuf),
    FindMissing,
    MissingFound(Result<Vec<MissingEntry>, EngineError>),
    ToggleMissing(PathBuf, bool),
    PickFrom(usize),
    PickTo(usize),
    Compare,
    Compared(Result<Vec<DiffEntry>, EngineError>),
    ToggleDiff(PathBuf, bool),
    /// Open the restore sheet for the current tab's selection.
    RestoreSelection,
    /// Open the restore sheet for one version of one file.
    RestoreVersion(String, PathBuf),
    OpenCopy(String, PathBuf),
    /// Download `path` as it is in the current snapshot: the bool is
    /// whether it is a folder (a `.tar.gz`) rather than a single file.
    Download(PathBuf, bool),
    /// Download one older version of a file, from [`versions_view`].
    DownloadVersion(String, PathBuf),
    TargetOriginal,
    TargetFolder,
    TargetChosen(PathBuf),
    Policy(ConflictPolicy),
    VerifyExisting(bool),
    Ownership(Ownership),
    /// A dry run's answer, with the requests it was worked out for.
    Previewed(Vec<RestoreRequest>, Result<RestorePreview, EngineError>),
    CancelSheet,
    StartRestore,
    Restore(ChildEvent),
    CancelRestore,
    Close,
}

pub enum Effect {
    Load,
    List {
        snapshot: String,
        dir: PathBuf,
    },
    Search {
        snapshot: String,
        query: String,
    },
    Versions(PathBuf),
    Missing {
        scope: PathBuf,
        since: i64,
    },
    Diff {
        from: String,
        to: String,
    },
    Preview(Vec<RestoreRequest>),
    Restore(RestoreRequest),
    OpenCopy {
        snapshot: String,
        path: PathBuf,
    },
    /// Ask where to save `path` from `snapshot`, then write it there.
    Download {
        snapshot: String,
        path: PathBuf,
        is_folder: bool,
    },
    PickScope,
    PickTarget,
    ShowError(String, EngineError),
    /// A restore finished: tell the user where their files are.
    Restored(RestorePreview),
    Close,
}

impl RestorePage {
    /// `root` is where browsing starts: the profile's first folder.
    pub fn new(profile_id: String, root: PathBuf) -> (Self, Vec<Effect>) {
        let page = Self {
            profile_id,
            browser: None,
            snapshots: Vec::new(),
            tab: Tab::Browse,
            snapshot: None,
            dir: root.clone(),
            root: root.clone(),
            entries: None,
            search: String::new(),
            results: None,
            expanded: None,
            versions: None,
            selection: BTreeSet::new(),
            scope: root,
            missing: None,
            missing_selection: BTreeSet::new(),
            from: None,
            to: None,
            diff: None,
            diff_selection: BTreeSet::new(),
            sheet: None,
            running: None,
            busy: false,
            labels: Vec::new(),
        };
        (page, vec![Effect::Load])
    }

    pub fn browser(&self) -> Option<Arc<Browser>> {
        self.browser.clone()
    }

    pub fn is_restoring(&self) -> bool {
        self.running.is_some()
    }

    fn snapshot_id(&self) -> Option<String> {
        self.snapshot
            .and_then(|index| self.snapshots.get(index))
            .map(|snapshot| snapshot.id.clone())
    }

    fn list(&mut self) -> Vec<Effect> {
        let Some(snapshot) = self.snapshot_id() else {
            return Vec::new();
        };
        self.entries = None;
        self.expanded = None;
        vec![Effect::List {
            snapshot,
            dir: self.dir.clone(),
        }]
    }

    fn open_sheet(&mut self, requests: Vec<RestoreRequest>) -> Vec<Effect> {
        if requests.is_empty() || requests.iter().all(|r| r.paths.is_empty()) {
            return Vec::new();
        }
        self.sheet = Some(Sheet {
            requests,
            target: TargetChoice::Original,
            policy: ConflictPolicy::KeepBoth,
            verify_existing: false,
            ownership: Ownership::Preserve,
            preview: None,
            previewing: false,
        });
        self.refresh_preview()
    }

    fn refresh_preview(&mut self) -> Vec<Effect> {
        let Some(sheet) = self.sheet.as_mut() else {
            return Vec::new();
        };
        sheet.preview = None;
        match sheet.finished_requests() {
            Some(requests) => {
                sheet.previewing = true;
                vec![Effect::Preview(requests)]
            }
            None => Vec::new(),
        }
    }

    /// The selection on the current tab, as restore requests.
    fn selection_requests(&self) -> Vec<RestoreRequest> {
        let request = |snapshot: String, paths: Vec<PathBuf>| RestoreRequest {
            snapshot,
            paths,
            target: Target::Original,
            policy: ConflictPolicy::KeepBoth,
            ..RestoreRequest::default()
        };
        match self.tab {
            Tab::Browse => self
                .snapshot_id()
                .map(|id| vec![request(id, self.selection.iter().cloned().collect())])
                .unwrap_or_default(),
            Tab::Deleted => {
                // Each file comes from the newest snapshot that has it.
                let mut by_snapshot: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
                for entry in self.missing.iter().flatten() {
                    if self.missing_selection.contains(&entry.path) {
                        by_snapshot
                            .entry(entry.last_seen.id.clone())
                            .or_default()
                            .push(entry.path.clone());
                    }
                }
                by_snapshot
                    .into_iter()
                    .map(|(id, paths)| request(id, paths))
                    .collect()
            }
            Tab::Compare => {
                // What changed or went away is restored as it was before.
                let from = self.from.and_then(|index| self.snapshots.get(index));
                match from {
                    Some(from) => vec![request(
                        from.id.clone(),
                        self.diff_selection.iter().cloned().collect(),
                    )],
                    None => Vec::new(),
                }
            }
        }
    }

    fn selection_count(&self) -> usize {
        match self.tab {
            Tab::Browse => self.selection.len(),
            Tab::Deleted => self.missing_selection.len(),
            Tab::Compare => self.diff_selection.len(),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Loaded(Ok(browser)) => {
                self.snapshots = browser.snapshots();
                self.labels = self
                    .snapshots
                    .iter()
                    .map(|s| format!("{} · {}", format::local_time(s.time), s.short_id()))
                    .collect();
                self.browser = Some(browser);
                if self.snapshots.is_empty() {
                    return Vec::new();
                }
                self.snapshot = Some(0);
                self.from = (self.snapshots.len() > 1).then_some(1);
                self.to = Some(0);
                self.list()
            }
            Message::Loaded(Err(err)) => vec![
                Effect::ShowError(fl!("open-repo-failed"), err),
                Effect::Close,
            ],
            Message::Tab(tab) => {
                self.tab = tab;
                Vec::new()
            }
            Message::PickSnapshot(index) => {
                self.snapshot = Some(index);
                // A selection belongs to one snapshot.
                self.selection.clear();
                self.results = None;
                self.list()
            }
            Message::Open(dir) => {
                self.dir = dir;
                self.results = None;
                self.list()
            }
            Message::Up => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    self.dir = parent;
                }
                self.list()
            }
            Message::Listed(dir, result) => {
                if dir != self.dir {
                    return Vec::new();
                }
                match result {
                    Ok(entries) => {
                        self.entries = Some(entries);
                        Vec::new()
                    }
                    // Another snapshot may not have this folder: fall back to
                    // where browsing started, then to the top, before calling
                    // it an error.
                    Err(err) if self.dir == Path::new("/") => {
                        self.entries = Some(Vec::new());
                        vec![Effect::ShowError(fl!("browse-failed"), err)]
                    }
                    Err(_) if self.dir != self.root => {
                        self.dir = self.root.clone();
                        self.list()
                    }
                    Err(_) => {
                        self.dir = PathBuf::from("/");
                        self.list()
                    }
                }
            }
            Message::Search(text) => {
                self.search = text;
                if self.search.is_empty() {
                    self.results = None;
                }
                Vec::new()
            }
            Message::SearchNow => match self.snapshot_id() {
                Some(snapshot) if !self.search.trim().is_empty() => {
                    self.busy = true;
                    vec![Effect::Search {
                        snapshot,
                        query: self.search.trim().to_owned(),
                    }]
                }
                _ => Vec::new(),
            },
            Message::Found(result) => {
                self.busy = false;
                match result {
                    Ok(found) => {
                        self.results = Some(found);
                        Vec::new()
                    }
                    Err(err) => vec![Effect::ShowError(fl!("browse-failed"), err)],
                }
            }
            Message::ShowVersions(path) => {
                if self.expanded.as_ref() == Some(&path) {
                    self.expanded = None;
                    return Vec::new();
                }
                self.expanded = Some(path.clone());
                self.versions = None;
                vec![Effect::Versions(path)]
            }
            Message::VersionsLoaded(path, result) => {
                if self.expanded.as_ref() != Some(&path) {
                    return Vec::new();
                }
                match result {
                    Ok(versions) => {
                        self.versions = Some(versions);
                        Vec::new()
                    }
                    Err(err) => vec![Effect::ShowError(fl!("browse-failed"), err)],
                }
            }
            Message::Toggle(path, on) => {
                if on {
                    self.selection.insert(path);
                } else {
                    self.selection.remove(&path);
                }
                Vec::new()
            }
            Message::ChooseScope => vec![Effect::PickScope],
            Message::ScopeChosen(scope) => {
                self.scope = scope;
                self.update(Message::FindMissing)
            }
            Message::FindMissing => {
                self.busy = true;
                self.missing = None;
                self.missing_selection.clear();
                vec![Effect::Missing {
                    scope: self.scope.clone(),
                    since: format::now() - DELETED_WINDOW_DAYS * 86_400,
                }]
            }
            Message::MissingFound(result) => {
                self.busy = false;
                match result {
                    Ok(found) => {
                        self.missing = Some(found);
                        Vec::new()
                    }
                    Err(err) => vec![Effect::ShowError(fl!("browse-failed"), err)],
                }
            }
            Message::ToggleMissing(path, on) => {
                if on {
                    self.missing_selection.insert(path);
                } else {
                    self.missing_selection.remove(&path);
                }
                Vec::new()
            }
            Message::PickFrom(index) => {
                self.from = Some(index);
                self.diff = None;
                Vec::new()
            }
            Message::PickTo(index) => {
                self.to = Some(index);
                self.diff = None;
                Vec::new()
            }
            Message::Compare => {
                let (Some(from), Some(to)) = (
                    self.from.and_then(|i| self.snapshots.get(i)),
                    self.to.and_then(|i| self.snapshots.get(i)),
                ) else {
                    return Vec::new();
                };
                self.busy = true;
                self.diff_selection.clear();
                vec![Effect::Diff {
                    from: from.id.clone(),
                    to: to.id.clone(),
                }]
            }
            Message::Compared(result) => {
                self.busy = false;
                match result {
                    Ok(diff) => {
                        self.diff = Some(diff);
                        Vec::new()
                    }
                    Err(err) => vec![Effect::ShowError(fl!("browse-failed"), err)],
                }
            }
            Message::ToggleDiff(path, on) => {
                if on {
                    self.diff_selection.insert(path);
                } else {
                    self.diff_selection.remove(&path);
                }
                Vec::new()
            }
            Message::RestoreSelection => {
                let requests = self.selection_requests();
                self.open_sheet(requests)
            }
            Message::RestoreVersion(snapshot, path) => self.open_sheet(vec![RestoreRequest {
                snapshot,
                paths: vec![path],
                target: Target::Original,
                policy: ConflictPolicy::KeepBoth,
                ..RestoreRequest::default()
            }]),
            Message::OpenCopy(snapshot, path) => vec![Effect::OpenCopy { snapshot, path }],
            Message::Download(path, is_folder) => self
                .snapshot_id()
                .map(|snapshot| {
                    vec![Effect::Download {
                        snapshot,
                        path,
                        is_folder,
                    }]
                })
                .unwrap_or_default(),
            Message::DownloadVersion(snapshot, path) => vec![Effect::Download {
                snapshot,
                path,
                is_folder: false,
            }],
            Message::TargetOriginal => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.target = TargetChoice::Original;
                }
                self.refresh_preview()
            }
            Message::TargetFolder => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.target = TargetChoice::Folder(None);
                    sheet.preview = None;
                }
                vec![Effect::PickTarget]
            }
            Message::TargetChosen(folder) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.target = TargetChoice::Folder(Some(folder));
                }
                self.refresh_preview()
            }
            Message::Policy(policy) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.policy = policy;
                }
                self.refresh_preview()
            }
            Message::VerifyExisting(on) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.verify_existing = on;
                }
                self.refresh_preview()
            }
            Message::Ownership(ownership) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.ownership = ownership;
                }
                self.refresh_preview()
            }
            Message::Previewed(asked, result) => {
                // An answer for choices the user has since changed is stale;
                // the dry run for the current ones is still on its way.
                if let Some(sheet) = self.sheet.as_mut()
                    && sheet.finished_requests().as_ref() == Some(&asked)
                {
                    sheet.previewing = false;
                    sheet.preview = Some(result);
                }
                Vec::new()
            }
            Message::CancelSheet => {
                self.sheet = None;
                Vec::new()
            }
            Message::StartRestore => {
                // Nothing is written until the dry run has said what will
                // happen, for exactly these choices.
                let Some(requests) = self
                    .sheet
                    .as_ref()
                    .filter(|sheet| !sheet.previewing && matches!(sheet.preview, Some(Ok(_))))
                    .and_then(Sheet::finished_requests)
                else {
                    return Vec::new();
                };
                self.sheet = None;
                let mut queue: VecDeque<RestoreRequest> = requests.into();
                let Some(first) = queue.pop_front() else {
                    return Vec::new();
                };
                self.running = Some(Running {
                    handle: None,
                    progress: None,
                    queue,
                });
                vec![Effect::Restore(first)]
            }
            Message::Restore(event) => self.on_restore(event),
            Message::CancelRestore => {
                if let Some(running) = self.running.as_mut() {
                    running.queue.clear();
                    if let Some(handle) = &running.handle {
                        handle.cancel();
                    }
                }
                Vec::new()
            }
            Message::Close => vec![Effect::Close],
        }
    }

    fn on_restore(&mut self, event: ChildEvent) -> Vec<Effect> {
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        match event {
            ChildEvent::Started(handle) => {
                running.handle = Some(handle);
                Vec::new()
            }
            ChildEvent::Event(Event::Progress { progress }) => {
                running.progress = Some(progress);
                Vec::new()
            }
            ChildEvent::Event(Event::Done { restored, .. }) => {
                if let Some(next) = running.queue.pop_front() {
                    running.handle = None;
                    running.progress = None;
                    return vec![Effect::Restore(next)];
                }
                self.running = None;
                self.selection.clear();
                self.missing_selection.clear();
                self.diff_selection.clear();
                let mut effects = vec![Effect::Restored(restored.unwrap_or_default())];
                // What was missing may not be any more.
                if self.tab == Tab::Deleted && self.missing.is_some() {
                    effects.extend(self.update(Message::FindMissing));
                }
                effects
            }
            ChildEvent::Event(Event::Error { error }) | ChildEvent::Ended(error) => {
                self.running = None;
                vec![Effect::ShowError(fl!("restore-failed"), error)]
            }
        }
    }

    pub fn view<'a>(&'a self, profile_name: &'a str) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let header = widget::row::with_capacity(3)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(
                widget::button::icon(widget::icon::from_name("go-previous-symbolic"))
                    .tooltip(fl!("back"))
                    .name(fl!("back"))
                    .on_press_maybe((!self.is_restoring()).then_some(Message::Close)),
            )
            .push(widget::text::title3(fl!(
                "restore-title",
                name = profile_name
            )));

        let body: Element<'a, Message> = if let Some(running) = &self.running {
            progress(running)
        } else if let Some(sheet) = &self.sheet {
            self.sheet_view(sheet)
        } else if self.browser.is_none() {
            widget::text::body(fl!("restore-loading")).into()
        } else if self.snapshots.is_empty() {
            widget::text::body(fl!("no-snapshots-yet")).into()
        } else {
            let tabs = widget::row::with_capacity(3)
                .spacing(spacing.space_xs)
                .push(tab_button(fl!("tab-browse"), Tab::Browse, self.tab))
                .push(tab_button(fl!("tab-deleted"), Tab::Deleted, self.tab))
                .push(tab_button(fl!("tab-compare"), Tab::Compare, self.tab));
            let content = match self.tab {
                Tab::Browse => self.browse_view(),
                Tab::Deleted => self.deleted_view(),
                Tab::Compare => self.compare_view(),
            };
            let count = self.selection_count();
            let footer = widget::row::with_capacity(3)
                .spacing(spacing.space_s)
                .align_y(Alignment::Center)
                .push(widget::text::body(fl!(
                    "selected-count",
                    count = (count as i64)
                )))
                .push(widget::space::horizontal())
                .push(
                    widget::button::suggested(fl!("restore-button"))
                        .on_press_maybe((count > 0).then_some(Message::RestoreSelection)),
                );
            widget::column::with_capacity(3)
                .spacing(spacing.space_s)
                .push(tabs)
                .push(content)
                .push(footer)
                .into()
        };

        widget::column::with_capacity(2)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .push(header)
            .push(body)
            .apply(widget::container)
            .max_width(960)
            .apply(widget::container)
            .center_x(Length::Fill)
            .into()
    }

    fn browse_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let top = widget::row::with_capacity(2)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(widget::dropdown(
                &self.labels,
                self.snapshot,
                Message::PickSnapshot,
            ))
            .push(
                widget::search_input(fl!("search-placeholder"), &self.search)
                    .on_input(Message::Search)
                    .on_submit(|_| Message::SearchNow)
                    .width(Length::Fill),
            );

        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        let entries: &[TreeEntry] = match (&self.results, &self.entries) {
            (Some(results), _) => {
                list = list.push(widget::text::caption(fl!(
                    "search-results",
                    count = (results.len() as i64)
                )));
                results
            }
            (None, Some(entries)) => {
                let crumbs = widget::row::with_capacity(2)
                    .spacing(spacing.space_xs)
                    .align_y(Alignment::Center)
                    .push(
                        widget::button::icon(widget::icon::from_name("go-up-symbolic"))
                            .tooltip(fl!("folder-up"))
                            .name(fl!("folder-up"))
                            .on_press_maybe((self.dir != Path::new("/")).then_some(Message::Up)),
                    )
                    .push(widget::text::body(format::path(&self.dir)));
                list = list.push(crumbs);
                entries
            }
            (None, None) => return widget::text::body(fl!("restore-loading")).into(),
        };
        if entries.is_empty() {
            list = list.push(widget::text::body(fl!("folder-empty")));
        }
        let snapshot = self.snapshot_id().unwrap_or_default();
        for entry in entries {
            list = list.push(self.entry_row(entry));
            if self.expanded.as_ref() == Some(&entry.path) {
                list = list.push(self.versions_view(&entry.path, &snapshot));
            }
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_s)
            .push(top)
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    fn entry_row<'a>(&'a self, entry: &'a TreeEntry) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let path = entry.path.clone();
        let checked = self.selection.contains(&entry.path);
        let icon = match entry.kind {
            EntryKind::Directory => "folder-symbolic",
            EntryKind::Symlink => "emblem-symbolic-link",
            _ => "text-x-generic-symbolic",
        };
        let name: Element<'a, Message> = if entry.kind == EntryKind::Directory {
            widget::button::link(entry.name.clone())
                .on_press(Message::Open(entry.path.clone()))
                .into()
        } else {
            widget::button::text(entry.name.clone())
                .on_press(Message::ShowVersions(entry.path.clone()))
                .into()
        };
        let detail = match entry.kind {
            EntryKind::Directory => String::new(),
            _ => format::bytes(entry.size),
        };
        let modified = entry.modified.map(format::local_time).unwrap_or_default();
        let is_folder = entry.kind == EntryKind::Directory;
        let download_path = entry.path.clone();
        widget::row::with_capacity(6)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(widget::checkbox(checked).on_toggle(move |on| Message::Toggle(path.clone(), on)))
            .push(widget::icon::from_name(icon).size(16))
            .push(widget::container(name).width(Length::Fill))
            .push(widget::text::caption(detail))
            .push(widget::text::caption(modified))
            .push(
                widget::button::icon(widget::icon::from_name("document-save-symbolic"))
                    .tooltip(fl!("download"))
                    .name(fl!("download"))
                    .on_press(Message::Download(download_path, is_folder)),
            )
            .into()
    }

    fn versions_view<'a>(&'a self, path: &'a Path, _snapshot: &str) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let Some(versions) = &self.versions else {
            return widget::text::caption(fl!("restore-loading")).into();
        };
        let mut column = widget::column::with_capacity(versions.len() + 1)
            .spacing(spacing.space_xxxs)
            .padding([0, 0, 0, spacing.space_xl]);
        column = column.push(widget::text::caption(fl!("versions-title")));
        let mut hidden: i64 = 0;
        for version in versions {
            if version.same_as_newer {
                hidden += 1;
                continue;
            }
            if hidden > 0 {
                column = column.push(widget::text::caption(fl!("versions-same", count = hidden)));
                hidden = 0;
            }
            let id = version.snapshot.id.clone();
            column = column.push(
                widget::row::with_capacity(5)
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center)
                    .push(
                        widget::text::body(format::local_time(version.snapshot.time))
                            .width(Length::Fill),
                    )
                    .push(widget::text::caption(format::bytes(version.size)))
                    .push(
                        widget::button::standard(fl!("open-copy"))
                            .on_press(Message::OpenCopy(id.clone(), path.to_path_buf())),
                    )
                    .push(
                        widget::button::icon(widget::icon::from_name("document-save-symbolic"))
                            .tooltip(fl!("download"))
                            .name(fl!("download"))
                            .on_press(Message::DownloadVersion(id.clone(), path.to_path_buf())),
                    )
                    .push(
                        widget::button::standard(fl!("restore-this-version"))
                            .on_press(Message::RestoreVersion(id, path.to_path_buf())),
                    ),
            );
        }
        if hidden > 0 {
            column = column.push(widget::text::caption(fl!("versions-same", count = hidden)));
        }
        column.into()
    }

    fn deleted_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let top = widget::row::with_capacity(3)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(
                widget::text::body(fl!(
                    "deleted-scope",
                    folder = format::path(&self.scope),
                    days = DELETED_WINDOW_DAYS
                ))
                .width(Length::Fill),
            )
            .push(
                widget::button::standard(fl!("deleted-change-folder"))
                    .on_press(Message::ChooseScope),
            )
            .push(
                widget::button::suggested(fl!("deleted-find"))
                    .on_press_maybe((!self.busy).then_some(Message::FindMissing)),
            );
        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        match &self.missing {
            None if self.busy => list = list.push(widget::text::body(fl!("restore-searching"))),
            None => list = list.push(widget::text::body(fl!("deleted-intro"))),
            Some(found) if found.is_empty() => {
                list = list.push(widget::text::body(fl!("deleted-none")));
            }
            Some(found) => {
                for entry in found {
                    let path = entry.path.clone();
                    let checked = self.missing_selection.contains(&entry.path);
                    list = list.push(
                        widget::row::with_capacity(4)
                            .spacing(spacing.space_s)
                            .align_y(Alignment::Center)
                            .push(
                                widget::checkbox(checked)
                                    .on_toggle(move |on| Message::ToggleMissing(path.clone(), on)),
                            )
                            .push(widget::text::body(format::path(&entry.path)).width(Length::Fill))
                            .push(widget::text::caption(format::bytes(entry.size)))
                            .push(widget::text::caption(fl!(
                                "deleted-last-seen",
                                when = format::local_time(entry.last_seen.time)
                            ))),
                    );
                }
            }
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_s)
            .push(top)
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    fn compare_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let top = widget::row::with_capacity(4)
            .spacing(spacing.space_s)
            .align_y(Alignment::Center)
            .push(widget::dropdown(&self.labels, self.from, Message::PickFrom))
            .push(widget::text::body("→"))
            .push(widget::dropdown(&self.labels, self.to, Message::PickTo))
            .push(
                widget::button::suggested(fl!("compare-button"))
                    .on_press_maybe((!self.busy).then_some(Message::Compare)),
            );
        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        match &self.diff {
            None if self.busy => list = list.push(widget::text::body(fl!("restore-searching"))),
            None => list = list.push(widget::text::body(fl!("compare-intro"))),
            Some(diff) if diff.is_empty() => {
                list = list.push(widget::text::body(fl!("compare-none")))
            }
            Some(diff) => {
                let (added, removed, changed) =
                    diff.iter().fold((0, 0, 0), |(a, r, c), d| match d.change {
                        Change::Added => (a + 1, r, c),
                        Change::Removed => (a, r + 1, c),
                        Change::Modified => (a, r, c + 1),
                    });
                list = list.push(widget::text::caption(fl!(
                    "compare-summary",
                    added = (added as i64),
                    removed = (removed as i64),
                    changed = (changed as i64)
                )));
                for entry in diff.iter().take(RESULT_LIMIT) {
                    let sign = match entry.change {
                        Change::Added => "+",
                        Change::Removed => "−",
                        Change::Modified => "~",
                    };
                    let mut row = widget::row::with_capacity(4)
                        .spacing(spacing.space_s)
                        .align_y(Alignment::Center);
                    // Only what existed in the older snapshot can be restored from it.
                    if entry.change != Change::Added {
                        let path = entry.path.clone();
                        let checked = self.diff_selection.contains(&entry.path);
                        row = row.push(
                            widget::checkbox(checked)
                                .on_toggle(move |on| Message::ToggleDiff(path.clone(), on)),
                        );
                    }
                    row = row
                        .push(widget::text::body(sign))
                        .push(widget::text::body(format::path(&entry.path)).width(Length::Fill));
                    if !entry.is_dir {
                        row = row.push(widget::text::caption(format::bytes(entry.size)));
                    }
                    list = list.push(row);
                }
            }
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_s)
            .push(top)
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    fn sheet_view<'a>(&'a self, sheet: &'a Sheet) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let count: usize = sheet.requests.iter().map(|r| r.paths.len()).sum();
        let original = sheet.target == TargetChoice::Original;
        let folder_label = match &sheet.target {
            TargetChoice::Folder(Some(folder)) => {
                fl!("restore-to-folder-chosen", folder = format::path(folder))
            }
            _ => fl!("restore-to-folder"),
        };
        let target = widget::settings::section()
            .title(fl!("restore-to"))
            .add(
                widget::settings::item::builder(fl!("restore-to-original")).radio(
                    true,
                    Some(original),
                    |_| Message::TargetOriginal,
                ),
            )
            .add(widget::settings::item::builder(folder_label).radio(
                false,
                Some(original),
                |_| Message::TargetFolder,
            ));
        let policy_item = |title: String, description: String, policy: ConflictPolicy| {
            widget::settings::item::builder(title)
                .description(description)
                .radio(policy, Some(sheet.policy), Message::Policy)
        };
        let policy = widget::settings::section()
            .title(fl!("restore-existing"))
            .add(policy_item(
                fl!("policy-keep-both"),
                fl!("policy-keep-both-description"),
                ConflictPolicy::KeepBoth,
            ))
            .add(policy_item(
                fl!("policy-overwrite"),
                fl!("policy-overwrite-description"),
                ConflictPolicy::Overwrite,
            ))
            .add(policy_item(
                fl!("policy-skip"),
                fl!("policy-skip-description"),
                ConflictPolicy::Skip,
            ));
        let ownership_item = |title: String, ownership: Ownership| {
            widget::settings::item::builder(title).radio(
                ownership,
                Some(sheet.ownership),
                Message::Ownership,
            )
        };
        let advanced = widget::settings::section()
            .title(fl!("restore-advanced"))
            .add(
                widget::settings::item::builder(fl!("restore-verify-existing"))
                    .description(fl!("restore-verify-existing-description"))
                    .toggler(sheet.verify_existing, Message::VerifyExisting),
            )
            .add(ownership_item(
                fl!("restore-ownership-preserve"),
                Ownership::Preserve,
            ))
            .add(ownership_item(
                fl!("restore-ownership-numeric"),
                Ownership::Numeric,
            ))
            .add(ownership_item(
                fl!("restore-ownership-none"),
                Ownership::None,
            ));

        let summary: Element<'a, Message> = match (&sheet.preview, sheet.previewing) {
            (_, true) => widget::text::body(fl!("restore-previewing")).into(),
            (None, false) => widget::text::body(fl!("restore-choose-folder")).into(),
            (Some(Err(err)), false) => widget::text::body(crate::app::errors::describe(
                &fl!("restore-preview-failed"),
                err,
            ))
            .into(),
            (Some(Ok(preview)), false) => {
                let conflicts = match sheet.policy {
                    ConflictPolicy::KeepBoth => {
                        fl!("preview-kept", count = (preview.conflicts as i64))
                    }
                    ConflictPolicy::Overwrite => {
                        fl!("preview-replaced", count = (preview.conflicts as i64))
                    }
                    ConflictPolicy::Skip => {
                        fl!("preview-skipped", count = (preview.conflicts as i64))
                    }
                };
                widget::column::with_capacity(3)
                    .push(widget::text::body(fl!(
                        "preview-restore",
                        count = (preview.files as i64),
                        size = format::bytes(preview.bytes)
                    )))
                    .push(widget::text::body(conflicts))
                    .push(widget::text::body(fl!(
                        "preview-unchanged",
                        count = (preview.unchanged as i64)
                    )))
                    .into()
            }
        };
        let ready = matches!(sheet.preview, Some(Ok(_))) && !sheet.previewing;
        widget::column::with_capacity(6)
            .spacing(spacing.space_m)
            .push(widget::text::title4(fl!(
                "restore-sheet-title",
                count = (count as i64)
            )))
            .push(target)
            .push(policy)
            .push(advanced)
            .push(
                widget::container(summary)
                    .padding(spacing.space_s)
                    .class(theme::Container::Card)
                    .width(Length::Fill),
            )
            .push(
                widget::row::with_capacity(3)
                    .spacing(spacing.space_s)
                    .push(widget::button::standard(fl!("cancel")).on_press(Message::CancelSheet))
                    .push(widget::space::horizontal())
                    .push(
                        widget::button::suggested(fl!("restore-button"))
                            .on_press_maybe(ready.then_some(Message::StartRestore)),
                    ),
            )
            .apply(widget::scrollable)
            .into()
    }
}

fn tab_button<'a>(label: String, tab: Tab, current: Tab) -> Element<'a, Message> {
    if tab == current {
        widget::button::suggested(label)
            .on_press(Message::Tab(tab))
            .into()
    } else {
        widget::button::standard(label)
            .on_press(Message::Tab(tab))
            .into()
    }
}

fn progress(running: &Running) -> Element<'_, Message> {
    let spacing = theme::active().cosmic().spacing;
    let (fraction, detail) = match &running.progress {
        Some(progress) => {
            let fraction = progress
                .total
                .filter(|total| *total > 0)
                .map_or(0.0, |total| progress.done as f32 / total as f32);
            let detail = progress.total.map_or_else(
                || format::bytes(progress.done),
                |total| {
                    fl!(
                        "progress-amount",
                        done = format::bytes(progress.done),
                        total = format::bytes(total)
                    )
                },
            );
            (fraction, detail)
        }
        None => (0.0, String::new()),
    };
    widget::column::with_capacity(4)
        .spacing(spacing.space_xs)
        .push(widget::text::title4(fl!("progress-restoring")))
        .push(widget::progress_bar::determinate_linear(fraction))
        .push(widget::text::caption(detail))
        .push(
            widget::button::standard(fl!("cancel"))
                .on_press_maybe(running.handle.as_ref().map(|_| Message::CancelRestore)),
        )
        .apply(widget::container)
        .padding(spacing.space_m)
        .class(theme::Container::Card)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: &str, time: i64) -> SnapshotSummary {
        SnapshotSummary {
            id: id.into(),
            time,
            paths: vec!["/home/alex".into()],
            hostname: "host".into(),
            files_new: 0,
            files_changed: 0,
            files_unmodified: 0,
            data_added: 0,
            total_bytes: 0,
            pinned: false,
        }
    }

    fn page_with_snapshots() -> RestorePage {
        let (mut page, _) = RestorePage::new("p".into(), "/home/alex".into());
        page.snapshots = vec![summary("bbbbbbbb", 20), summary("aaaaaaaa", 10)];
        page.snapshot = Some(0);
        page.from = Some(1);
        page.to = Some(0);
        page
    }

    fn missing(path: &str, snapshot: &str) -> MissingEntry {
        MissingEntry {
            path: path.into(),
            size: 1,
            last_seen: summary(snapshot, 0),
        }
    }

    #[test]
    fn the_sheet_starts_with_keep_both_and_the_original_place() {
        let mut page = page_with_snapshots();
        page.update(Message::Toggle("/home/alex/a.txt".into(), true));

        let effects = page.update(Message::RestoreSelection);

        let sheet = page.sheet.as_ref().expect("the sheet is open");
        assert_eq!(sheet.policy, ConflictPolicy::KeepBoth, "the safe default");
        assert_eq!(sheet.target, TargetChoice::Original);
        match effects.as_slice() {
            [Effect::Preview(requests)] => {
                assert_eq!(requests[0].snapshot, "bbbbbbbb");
                assert_eq!(requests[0].paths, vec![PathBuf::from("/home/alex/a.txt")]);
            }
            _ => panic!("a dry run starts straight away"),
        }
    }

    #[test]
    fn restoring_needs_a_preview_first() {
        let mut page = page_with_snapshots();
        page.update(Message::Toggle("/home/alex/a.txt".into(), true));
        let asked = preview_requests(page.update(Message::RestoreSelection));

        assert!(
            page.update(Message::StartRestore).is_empty(),
            "no preview yet"
        );

        page.update(Message::Previewed(asked, Ok(RestorePreview::default())));
        let effects = page.update(Message::StartRestore);
        assert!(matches!(effects.as_slice(), [Effect::Restore(_)]));
        assert!(page.is_restoring());
    }

    #[test]
    fn a_preview_for_earlier_choices_is_ignored() {
        let mut page = page_with_snapshots();
        page.update(Message::Toggle("/home/alex/a.txt".into(), true));
        let keep_both = preview_requests(page.update(Message::RestoreSelection));
        let overwrite = preview_requests(page.update(Message::Policy(ConflictPolicy::Overwrite)));

        page.update(Message::Previewed(keep_both, Ok(RestorePreview::default())));
        assert!(
            page.update(Message::StartRestore).is_empty(),
            "the Keep both dry run says nothing about Overwrite"
        );

        page.update(Message::Previewed(overwrite, Ok(RestorePreview::default())));
        match page.update(Message::StartRestore).as_slice() {
            [Effect::Restore(request)] => assert_eq!(request.policy, ConflictPolicy::Overwrite),
            _ => panic!("the matching dry run lets the restore start"),
        }
    }

    fn preview_requests(effects: Vec<Effect>) -> Vec<RestoreRequest> {
        match effects.as_slice() {
            [Effect::Preview(requests)] => requests.clone(),
            _ => panic!("expected a dry run"),
        }
    }

    #[test]
    fn choosing_a_folder_waits_for_the_folder() {
        let mut page = page_with_snapshots();
        page.update(Message::Toggle("/home/alex/a.txt".into(), true));
        page.update(Message::RestoreSelection);

        let effects = page.update(Message::TargetFolder);
        assert!(matches!(effects.as_slice(), [Effect::PickTarget]));
        assert!(page.update(Message::StartRestore).is_empty());

        let effects = page.update(Message::TargetChosen("/tmp/out".into()));
        match effects.as_slice() {
            [Effect::Preview(requests)] => {
                assert_eq!(requests[0].target, Target::Folder("/tmp/out".into()));
            }
            _ => panic!("expected a new dry run"),
        }
    }

    #[test]
    fn deleted_files_restore_from_their_own_snapshots() {
        let mut page = page_with_snapshots();
        page.tab = Tab::Deleted;
        page.missing = Some(vec![
            missing("/home/alex/a.txt", "bbbbbbbb"),
            missing("/home/alex/b.txt", "aaaaaaaa"),
        ]);
        page.update(Message::ToggleMissing("/home/alex/a.txt".into(), true));
        page.update(Message::ToggleMissing("/home/alex/b.txt".into(), true));

        let requests = page.selection_requests();

        assert_eq!(requests.len(), 2, "one per snapshot");
        assert!(requests.iter().any(
            |r| r.snapshot == "aaaaaaaa" && r.paths == vec![PathBuf::from("/home/alex/b.txt")]
        ));
    }

    #[test]
    fn a_multi_part_restore_runs_every_part_in_turn() {
        let mut page = page_with_snapshots();
        page.sheet = Some(Sheet {
            requests: vec![
                RestoreRequest {
                    snapshot: "aaaaaaaa".into(),
                    paths: vec!["/a".into()],
                    target: Target::Original,
                    policy: ConflictPolicy::KeepBoth,
                    ..RestoreRequest::default()
                },
                RestoreRequest {
                    snapshot: "bbbbbbbb".into(),
                    paths: vec!["/b".into()],
                    target: Target::Original,
                    policy: ConflictPolicy::KeepBoth,
                    ..RestoreRequest::default()
                },
            ],
            target: TargetChoice::Original,
            policy: ConflictPolicy::KeepBoth,
            verify_existing: false,
            ownership: Ownership::Preserve,
            preview: Some(Ok(RestorePreview::default())),
            previewing: false,
        });

        let first = page.update(Message::StartRestore);
        assert!(matches!(first.as_slice(), [Effect::Restore(r)] if r.snapshot == "aaaaaaaa"));

        let done = || {
            Message::Restore(ChildEvent::Event(Event::Done {
                report: None,
                restored: Some(RestorePreview::default()),
                forgotten: None,
                pruned: None,
                pinned: None,
            }))
        };
        let second = page.update(done());
        assert!(matches!(second.as_slice(), [Effect::Restore(r)] if r.snapshot == "bbbbbbbb"));
        assert!(page.is_restoring());

        let finished = page.update(done());
        assert!(matches!(finished.as_slice(), [Effect::Restored(_)]));
        assert!(!page.is_restoring());
    }

    #[test]
    fn cancelling_drops_the_rest_of_the_queue() {
        let mut page = page_with_snapshots();
        page.running = Some(Running {
            handle: None,
            progress: None,
            queue: VecDeque::from(vec![RestoreRequest {
                snapshot: "aaaaaaaa".into(),
                paths: vec!["/b".into()],
                target: Target::Original,
                policy: ConflictPolicy::Skip,
                ..RestoreRequest::default()
            }]),
        });

        page.update(Message::CancelRestore);
        let effects = page.update(Message::Restore(ChildEvent::Ended(EngineError::new(
            crate::engine::ErrorKind::Canceled,
            "",
        ))));

        assert!(!page.is_restoring());
        assert!(matches!(effects.as_slice(), [Effect::ShowError(..)]));
    }

    #[test]
    fn compare_restores_from_the_older_snapshot() {
        let mut page = page_with_snapshots();
        page.tab = Tab::Compare;
        page.update(Message::ToggleDiff("/home/alex/changed.txt".into(), true));

        let requests = page.selection_requests();

        assert_eq!(
            requests[0].snapshot, "aaaaaaaa",
            "the version from before the change"
        );
    }

    #[test]
    fn a_stale_listing_is_ignored() {
        let mut page = page_with_snapshots();
        page.update(Message::Open("/home/alex/Documents".into()));
        page.update(Message::Listed("/home/alex".into(), Ok(Vec::new())));
        assert!(
            page.entries.is_none(),
            "the answer for a folder we left is dropped"
        );
    }

    #[test]
    fn a_folder_missing_from_a_snapshot_falls_back_then_gives_up() {
        let mut page = page_with_snapshots();
        page.dir = "/home/alex/gone".into();
        let failed = || {
            Err(EngineError::new(
                crate::engine::ErrorKind::Internal,
                "not here",
            ))
        };

        let effects = page.update(Message::Listed("/home/alex/gone".into(), failed()));
        assert!(
            matches!(effects.as_slice(), [Effect::List { dir, .. }] if dir == Path::new("/home/alex")),
            "first back to where browsing started"
        );

        let effects = page.update(Message::Listed("/home/alex".into(), failed()));
        assert!(
            matches!(effects.as_slice(), [Effect::List { dir, .. }] if dir == Path::new("/")),
            "then to the top"
        );

        let effects = page.update(Message::Listed("/".into(), failed()));
        assert!(
            matches!(effects.as_slice(), [Effect::ShowError(..)]),
            "and only then an error, never a loop"
        );
    }
}
