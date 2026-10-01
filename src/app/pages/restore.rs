// SPDX-License-Identifier: GPL-3.0-only

//! Getting files back: browse a snapshot, find what was deleted, compare two
//! snapshots, and restore a selection with a dry run first.
//!
//! The page keeps one [`Browser`] open while it is shown. Every side effect is
//! returned as an [`Effect`] for the application to run, so the page's logic
//! is testable without a window.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::child::{ChildEvent, ChildHandle};
use crate::app::format;
use crate::constants::DELETED_WINDOW_DAYS;
use crate::constants::RESTORE_RESULT_LIMIT as RESULT_LIMIT;
use crate::constants::{LIST_ICON_SIZE, RESTORE_MAX_WIDTH};
use crate::engine::mount::Mount;
use crate::engine::{
    Browser, Change, ConflictPolicy, DiffEntry, EngineError, EntryKind, FileVersion, GlobalMatch,
    MissingEntry, Ownership, ProgressEvent, RestorePreview, RestoreRequest, SnapshotSummary,
    Target, TreeEntry,
};
use crate::fl;
use crate::runner::Event;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Browse,
    Deleted,
    Compare,
    Search,
}

/// The tab bar's model, which has no `Debug` of its own.
pub struct TabModel(widget::segmented_button::SingleSelectModel);

impl fmt::Debug for TabModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TabModel")
    }
}

impl TabModel {
    fn new() -> Self {
        let tab = |model: widget::segmented_button::ModelBuilder<_>, label: String, tab: Tab| {
            model.insert(move |entity| entity.text(label).data(tab))
        };
        let mut model = widget::segmented_button::SingleSelectModel::builder();
        model = tab(model, fl!("tab-browse"), Tab::Browse);
        model = tab(model, fl!("tab-deleted"), Tab::Deleted);
        model = tab(model, fl!("tab-compare"), Tab::Compare);
        model = tab(model, fl!("tab-search"), Tab::Search);
        let mut model = Self(model.build());
        model.show(Tab::Browse);
        model
    }

    fn tab(&self, entity: widget::segmented_button::Entity) -> Option<Tab> {
        self.0.data::<Tab>(entity).copied()
    }

    /// Mark `tab` as the one showing.
    fn show(&mut self, tab: Tab) {
        let entity = self.0.iter().find(|&e| self.tab(e) == Some(tab));
        if let Some(entity) = entity {
            self.0.activate(entity);
        }
    }
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
#[derive(Debug)]
struct Running {
    handle: Option<ChildHandle>,
    progress: Option<ProgressEvent>,
    /// Requests still to run after this one.
    queue: VecDeque<RestoreRequest>,
    /// What the parts finished so far restored, added up: one restore of
    /// several selections runs as several requests, one after another.
    total: RestorePreview,
}

/// A live FUSE mount of a snapshot. Unmounting happens when the last handle
/// is dropped, so this is dropped through `Effect::Unmount` on a blocking
/// thread rather than here directly: the same reasoning as [`ChildHandle`],
/// whose cheap `Clone` this otherwise mirrors.
#[derive(Clone)]
pub struct MountHandle(Arc<Mount>);

impl fmt::Debug for MountHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MountHandle")
    }
}

impl MountHandle {
    fn point(&self) -> &Path {
        self.0.point()
    }

    pub(crate) fn snapshot(&self) -> &str {
        self.0.snapshot()
    }
}

impl From<Mount> for MountHandle {
    fn from(mount: Mount) -> Self {
        Self(Arc::new(mount))
    }
}

#[derive(Debug)]
pub struct RestorePage {
    pub profile_id: String,
    browser: Option<Arc<Browser>>,
    snapshots: Vec<SnapshotSummary>,
    tab: Tab,
    tabs: TabModel,
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
    diff: Option<Diff>,
    diff_selection: BTreeSet<PathBuf>,
    /// Folders (relative to the diff's own common root) whose changes are
    /// expanded, rather than collapsed behind a count.
    diff_expanded: BTreeSet<PathBuf>,
    // Search across every snapshot
    global_query: String,
    global_results: Option<Vec<GlobalMatch>>,
    // Restoring
    sheet: Option<Sheet>,
    running: Option<Running>,
    // What each background search is answering, so a result for something
    // the user has since moved on from is recognized and dropped rather than
    // shown under whatever is selected now. `Some` while one is in flight.
    /// Browse search: (snapshot, query).
    searching: Option<(String, String)>,
    /// Deleted files: (scope, since).
    finding_missing: Option<(PathBuf, i64)>,
    /// Compare: (from, to).
    comparing: Option<(String, String)>,
    /// Search across every snapshot: the query.
    global_searching: Option<String>,
    labels: Vec<String>,
    // Mounting
    mounted: Option<MountHandle>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Loaded(Result<Arc<Browser>, EngineError>),
    Tab(widget::segmented_button::Entity),
    PickSnapshot(usize),
    Open(PathBuf),
    Up,
    Listed(String, PathBuf, Result<Vec<TreeEntry>, EngineError>),
    Search(String),
    SearchNow,
    Found(String, String, Result<Vec<TreeEntry>, EngineError>),
    ShowVersions(PathBuf),
    VersionsLoaded(PathBuf, Result<Vec<FileVersion>, EngineError>),
    Toggle(PathBuf, bool),
    ChooseScope,
    ScopeChosen(PathBuf),
    FindMissing,
    MissingFound(PathBuf, i64, Result<Vec<MissingEntry>, EngineError>),
    ToggleMissing(PathBuf, bool),
    PickFrom(usize),
    PickTo(usize),
    Compare,
    Compared(String, String, Result<Vec<DiffEntry>, EngineError>),
    ToggleDiff(PathBuf, bool),
    /// Expand or collapse one folder's changes in the Compare tab.
    ToggleDiffFolder(PathBuf),
    GlobalSearch(String),
    GlobalSearchNow,
    GlobalFound(String, Result<Vec<GlobalMatch>, EngineError>),
    /// Jump to `path`'s folder, in `snapshot`, on the Browse tab.
    JumpToMatch(String, PathBuf),
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
    /// Mount the current snapshot as a read-only folder.
    Mount,
    MountPointChosen(PathBuf),
    Mounted(Result<MountHandle, EngineError>),
    OpenMountedFolder,
    Unmount,
    Close,
}

#[derive(Debug)]
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
    /// Search every snapshot's tree for `query`.
    GlobalSearch {
        query: String,
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
    /// Ask for an empty folder to mount the current snapshot into.
    PickMountPoint,
    /// Mount `snapshot` read-only at `point`.
    Mount {
        snapshot: String,
        point: PathBuf,
    },
    /// Open the mounted folder in the file manager.
    OpenMounted(PathBuf),
    /// Drop a mount (which unmounts it) off the UI thread.
    Unmount(MountHandle),
    Close,
}

impl RestorePage {
    fn show_tab(&mut self, tab: Tab) {
        self.tab = tab;
        self.tabs.show(tab);
    }

    /// `root` is where browsing starts: the profile's first folder.
    pub fn new(profile_id: String, root: PathBuf) -> (Self, Vec<Effect>) {
        let page = Self {
            profile_id,
            browser: None,
            snapshots: Vec::new(),
            tab: Tab::Browse,
            tabs: TabModel::new(),
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
            diff_expanded: BTreeSet::new(),
            global_query: String::new(),
            global_results: None,
            sheet: None,
            running: None,
            searching: None,
            finding_missing: None,
            comparing: None,
            global_searching: None,
            labels: Vec::new(),
            mounted: None,
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
            // Nothing is selected directly here: a match is restored by
            // jumping to it on the Browse tab, which already can.
            Tab::Search => Vec::new(),
        }
    }

    fn selection_count(&self) -> usize {
        match self.tab {
            Tab::Browse => self.selection.len(),
            Tab::Deleted => self.missing_selection.len(),
            Tab::Compare => self.diff_selection.len(),
            Tab::Search => 0,
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Loaded(Ok(browser)) => {
                self.snapshots = browser.snapshots();
                self.labels = self
                    .snapshots
                    .iter()
                    .map(|s| {
                        fl!(
                            "snapshot-label",
                            time = format::local_time(s.time),
                            id = s.short_id()
                        )
                    })
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
            Message::Tab(entity) => {
                if let Some(tab) = self.tabs.tab(entity) {
                    self.show_tab(tab);
                }
                Vec::new()
            }
            Message::PickSnapshot(index) => {
                self.snapshot = Some(index);
                // A selection belongs to one snapshot.
                self.selection.clear();
                self.results = None;
                self.searching = None;
                self.list()
            }
            Message::Open(dir) => {
                self.dir = dir;
                self.results = None;
                // A search still running answers for a view the user left.
                self.searching = None;
                self.list()
            }
            Message::Up => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    self.dir = parent;
                }
                self.list()
            }
            Message::Listed(snapshot, dir, result) => {
                if dir != self.dir || self.snapshot_id().as_deref() != Some(&snapshot) {
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
                    self.searching = None;
                }
                Vec::new()
            }
            Message::SearchNow => match self.snapshot_id() {
                Some(snapshot) if !self.search.trim().is_empty() => {
                    let query = self.search.trim().to_owned();
                    self.searching = Some((snapshot.clone(), query.clone()));
                    vec![Effect::Search { snapshot, query }]
                }
                _ => Vec::new(),
            },
            Message::Found(snapshot, query, result) => {
                if self.searching != Some((snapshot, query)) {
                    return Vec::new();
                }
                self.searching = None;
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
                let since = format::now() - DELETED_WINDOW_DAYS * 86_400;
                self.finding_missing = Some((self.scope.clone(), since));
                self.missing = None;
                self.missing_selection.clear();
                vec![Effect::Missing {
                    scope: self.scope.clone(),
                    since,
                }]
            }
            Message::MissingFound(scope, since, result) => {
                if self.finding_missing != Some((scope, since)) {
                    return Vec::new();
                }
                self.finding_missing = None;
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
                self.comparing = None;
                Vec::new()
            }
            Message::PickTo(index) => {
                self.to = Some(index);
                self.diff = None;
                self.comparing = None;
                Vec::new()
            }
            Message::Compare => {
                let (Some(from), Some(to)) = (
                    self.from.and_then(|i| self.snapshots.get(i)),
                    self.to.and_then(|i| self.snapshots.get(i)),
                ) else {
                    return Vec::new();
                };
                let (from, to) = (from.id.clone(), to.id.clone());
                self.comparing = Some((from.clone(), to.clone()));
                self.diff_selection.clear();
                self.diff_expanded.clear();
                vec![Effect::Diff { from, to }]
            }
            Message::Compared(from, to, result) => {
                if self.comparing != Some((from, to)) {
                    return Vec::new();
                }
                self.comparing = None;
                match result {
                    Ok(diff) => {
                        self.diff = Some(Diff::new(diff));
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
            Message::ToggleDiffFolder(folder) => {
                if !self.diff_expanded.remove(&folder) {
                    self.diff_expanded.insert(folder);
                }
                Vec::new()
            }
            Message::GlobalSearch(text) => {
                self.global_query = text;
                Vec::new()
            }
            Message::GlobalSearchNow => {
                if self.global_query.trim().is_empty() {
                    self.global_results = None;
                    return Vec::new();
                }
                self.global_searching = Some(self.global_query.clone());
                vec![Effect::GlobalSearch {
                    query: self.global_query.clone(),
                }]
            }
            Message::GlobalFound(query, result) => {
                if self.global_searching.as_ref() != Some(&query) {
                    return Vec::new();
                }
                self.global_searching = None;
                match result {
                    Ok(results) => {
                        self.global_results = Some(results);
                        Vec::new()
                    }
                    Err(err) => vec![Effect::ShowError(fl!("browse-failed"), err)],
                }
            }
            Message::JumpToMatch(snapshot, path) => {
                let Some(index) = self.snapshots.iter().position(|s| s.id == snapshot) else {
                    return Vec::new();
                };
                self.show_tab(Tab::Browse);
                self.snapshot = Some(index);
                self.selection.clear();
                self.results = None;
                self.searching = None;
                self.expanded = None;
                self.dir = path
                    .parent()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from("/"));
                self.list()
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
                    total: RestorePreview::default(),
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
            Message::Mount => vec![Effect::PickMountPoint],
            Message::MountPointChosen(point) => self
                .snapshot_id()
                .map(|snapshot| vec![Effect::Mount { snapshot, point }])
                .unwrap_or_default(),
            Message::Mounted(Ok(handle)) => {
                self.mounted = Some(handle);
                Vec::new()
            }
            Message::Mounted(Err(err)) => vec![Effect::ShowError(fl!("mount-failed"), err)],
            Message::OpenMountedFolder => self
                .mounted
                .as_ref()
                .map(|handle| vec![Effect::OpenMounted(handle.point().to_path_buf())])
                .unwrap_or_default(),
            Message::Unmount => self
                .mounted
                .take()
                .map(|handle| vec![Effect::Unmount(handle)])
                .unwrap_or_default(),
            Message::Close => {
                let mut effects: Vec<Effect> = self
                    .mounted
                    .take()
                    .map(|handle| vec![Effect::Unmount(handle)])
                    .unwrap_or_default();
                effects.push(Effect::Close);
                effects
            }
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
                running.total += restored.unwrap_or_default();
                if let Some(next) = running.queue.pop_front() {
                    running.handle = None;
                    running.progress = None;
                    return vec![Effect::Restore(next)];
                }
                let total = running.total;
                self.running = None;
                self.selection.clear();
                self.missing_selection.clear();
                self.diff_selection.clear();
                let mut effects = vec![Effect::Restored(total)];
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
            let tabs =
                widget::segmented_control::horizontal(&self.tabs.0).on_activate(Message::Tab);
            let content = match self.tab {
                Tab::Browse => self.browse_view(),
                Tab::Deleted => self.deleted_view(),
                Tab::Compare => self.compare_view(),
                Tab::Search => self.search_view(),
            };
            let mut column = widget::column::with_capacity(3)
                .spacing(spacing.space_s)
                .push(tabs)
                .push(content);
            // Nothing on the Search tab is ever selected directly: a match
            // is restored by jumping to it on Browse, which already has its
            // own footer.
            if self.tab != Tab::Search {
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
                column = column.push(footer);
            }
            column.into()
        };

        widget::column::with_capacity(2)
            .spacing(spacing.space_m)
            .padding(spacing.space_m)
            .push(header)
            .push(body)
            .apply(widget::container)
            .max_width(RESTORE_MAX_WIDTH)
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
        // Capped the same way a global search's own results already are: a
        // folder with far more than RESULT_LIMIT entries costs RESULT_LIMIT
        // rows, not however many actually live there.
        for entry in entries.iter().take(RESULT_LIMIT) {
            list = list.push(self.entry_row(entry));
            if self.expanded.as_ref() == Some(&entry.path) {
                list = list.push(self.versions_view(&entry.path, &snapshot));
            }
        }
        widget::column::with_capacity(3)
            .spacing(spacing.space_s)
            .push(top)
            .push(self.mount_row())
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    fn mount_row(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        match &self.mounted {
            Some(handle) => widget::row::with_capacity(3)
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center)
                .push(widget::text::caption(fl!(
                    "mount-active",
                    folder = format::path(handle.point())
                )))
                .push(
                    widget::button::standard(fl!("mount-open-folder"))
                        .on_press(Message::OpenMountedFolder),
                )
                .push(widget::button::standard(fl!("unmount")).on_press(Message::Unmount))
                .into(),
            None => widget::button::standard(fl!("restore-mount"))
                .on_press(Message::Mount)
                .into(),
        }
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
            .push(
                widget::checkbox(checked)
                    .name(entry.name.clone())
                    .on_toggle(move |on| Message::Toggle(path.clone(), on)),
            )
            .push(widget::icon::from_name(icon).size(LIST_ICON_SIZE))
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
                widget::button::suggested(fl!("deleted-find")).on_press_maybe(
                    self.finding_missing
                        .is_none()
                        .then_some(Message::FindMissing),
                ),
            );
        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        match &self.missing {
            None if self.finding_missing.is_some() => {
                list = list.push(widget::text::body(fl!("restore-searching")))
            }
            None => list = list.push(widget::text::body(fl!("deleted-intro"))),
            Some(found) if found.is_empty() => {
                list = list.push(widget::text::body(fl!("deleted-none")));
            }
            Some(found) => {
                for entry in found.iter().take(RESULT_LIMIT) {
                    let path = entry.path.clone();
                    let checked = self.missing_selection.contains(&entry.path);
                    list = list.push(
                        widget::row::with_capacity(4)
                            .spacing(spacing.space_s)
                            .align_y(Alignment::Center)
                            .push(
                                widget::checkbox(checked)
                                    .name(format::path(&entry.path))
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
            .push(widget::icon::from_name("go-next-symbolic").size(LIST_ICON_SIZE))
            .push(widget::dropdown(&self.labels, self.to, Message::PickTo))
            .push(
                widget::button::suggested(fl!("compare-button"))
                    .on_press_maybe(self.comparing.is_none().then_some(Message::Compare)),
            );
        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        match &self.diff {
            None if self.comparing.is_some() => {
                list = list.push(widget::text::body(fl!("restore-searching")))
            }
            None => list = list.push(widget::text::body(fl!("compare-intro"))),
            Some(diff) if diff.is_empty() => {
                list = list.push(widget::text::body(fl!("compare-none")))
            }
            Some(diff) => {
                list = list.push(widget::text::caption(fl!(
                    "compare-summary",
                    added = (diff.added as i64),
                    removed = (diff.removed as i64),
                    changed = (diff.changed as i64)
                )));
                for (folder, entries) in diff.groups.iter().take(RESULT_LIMIT) {
                    if entries.len() == 1 {
                        // Nothing to drill into for a folder with only one
                        // change: show it directly, in full, like before.
                        let entry = &entries[0];
                        list = list.push(self.diff_entry_row(entry, &entry.path));
                    } else {
                        let expanded = self.diff_expanded.contains(folder);
                        list = list.push(self.diff_folder_row(folder, entries.len(), expanded));
                        if expanded {
                            for entry in entries.iter().take(RESULT_LIMIT) {
                                let name = entry
                                    .path
                                    .file_name()
                                    .map_or_else(|| entry.path.clone(), PathBuf::from);
                                list = list.push(
                                    widget::row::with_capacity(2)
                                        .push(widget::space::horizontal().width(spacing.space_l))
                                        .push(self.diff_entry_row(entry, &name)),
                                );
                            }
                        }
                    }
                }
            }
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_s)
            .push(top)
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    /// One changed entry, `label` shown in place of its full path: the
    /// entry's own path when it is the only change in its folder, or just
    /// its file name when it is one of several already grouped under a
    /// folder row that names the rest.
    fn diff_entry_row<'a>(&'a self, entry: &'a DiffEntry, label: &Path) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
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
                    .name(format::path(label))
                    .on_toggle(move |on| Message::ToggleDiff(path.clone(), on)),
            );
        }
        row = row
            .push(widget::text::body(sign))
            .push(widget::text::body(format::path(label)).width(Length::Fill));
        if !entry.is_dir {
            row = row.push(widget::text::caption(format::bytes(entry.size)));
        }
        row.into()
    }

    /// A folder with more than one change beneath it: a count instead of
    /// every path at once, expanded on request.
    fn diff_folder_row(&self, folder: &Path, count: usize, expanded: bool) -> Element<'_, Message> {
        let icon = if expanded {
            "go-down-symbolic"
        } else {
            "go-next-symbolic"
        };
        let owned = folder.to_path_buf();
        let label = if folder.as_os_str().is_empty() {
            fl!("compare-folder-root", count = (count as i64))
        } else {
            fl!(
                "compare-folder",
                folder = format::path(folder),
                count = (count as i64)
            )
        };
        widget::button::text(label)
            .leading_icon(widget::icon::from_name(icon))
            .width(Length::Fill)
            .on_press(Message::ToggleDiffFolder(owned))
            .into()
    }

    fn search_view(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let top = widget::search_input(fl!("search-all-placeholder"), &self.global_query)
            .on_input(Message::GlobalSearch)
            .on_submit(|_| Message::GlobalSearchNow)
            .width(Length::Fill);
        let mut list = widget::column::with_capacity(8).spacing(spacing.space_xxxs);
        match &self.global_results {
            None if self.global_searching.is_some() => {
                list = list.push(widget::text::body(fl!("restore-searching")))
            }
            None => list = list.push(widget::text::body(fl!("search-all-intro"))),
            Some(results) if results.is_empty() => {
                list = list.push(widget::text::body(fl!("search-all-none")));
            }
            Some(results) => {
                list = list.push(widget::text::caption(fl!(
                    "search-results",
                    count = (results.len() as i64)
                )));
                for found in results.iter().take(RESULT_LIMIT) {
                    list = list.push(self.global_match_row(found));
                }
            }
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_s)
            .push(top)
            .push(widget::scrollable(list).height(Length::Fill))
            .into()
    }

    fn global_match_row<'a>(&self, found: &'a GlobalMatch) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let icon = match found.kind {
            EntryKind::Directory => "folder-symbolic",
            EntryKind::Symlink => "emblem-symbolic-link",
            _ => "text-x-generic-symbolic",
        };
        let mut chips =
            widget::row::with_capacity(found.snapshots.len()).spacing(spacing.space_xxs);
        for snapshot in &found.snapshots {
            let id = snapshot.id.clone();
            let path = found.path.clone();
            chips = chips.push(
                widget::button::standard(snapshot.short_id().to_owned())
                    .on_press(Message::JumpToMatch(id, path)),
            );
        }
        widget::column::with_capacity(2)
            .spacing(spacing.space_xxxs)
            .push(
                widget::row::with_capacity(3)
                    .spacing(spacing.space_s)
                    .align_y(Alignment::Center)
                    .push(widget::icon::from_name(icon).size(LIST_ICON_SIZE))
                    .push(widget::text::body(format::path(&found.path)).width(Length::Fill))
                    .push(widget::text::caption(format::bytes(found.size))),
            )
            .push(chips)
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
        // Overwriting files where they are is the one restore that destroys
        // something: say how much, on the button itself, in the color that
        // means it.
        let start = match files_replaced_in_place(sheet) {
            Some(count) => {
                widget::button::destructive(fl!("restore-replace-button", count = (count as i64)))
            }
            None => widget::button::suggested(fl!("restore-button")),
        };
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
                    .push(start.on_press_maybe(ready.then_some(Message::StartRestore))),
            )
            .apply(widget::scrollable)
            .into()
    }
}

/// How many existing files the restore in `sheet` would overwrite where they
/// are, if it is that kind of restore and there are any.
fn files_replaced_in_place(sheet: &Sheet) -> Option<u64> {
    match (&sheet.preview, &sheet.target, sheet.policy) {
        (Some(Ok(preview)), TargetChoice::Original, ConflictPolicy::Overwrite)
            if !sheet.previewing && preview.conflicts > 0 =>
        {
            Some(preview.conflicts)
        }
        _ => None,
    }
}

/// The longest path every one of `paths` starts with, component by
/// component: usually a diff's own common source folder, so grouping
/// relative to it does not force a click through a long chain of folders
/// that never actually branch.
fn common_ancestor<'a>(paths: impl Iterator<Item = &'a Path>) -> PathBuf {
    let mut common: Option<Vec<std::path::Component<'a>>> = None;
    for path in paths {
        let components: Vec<_> = path.components().collect();
        common = Some(match common {
            None => components,
            Some(previous) => previous
                .into_iter()
                .zip(components)
                .take_while(|(a, b)| a == b)
                .map(|(a, _)| a)
                .collect(),
        });
    }
    common.unwrap_or_default().into_iter().collect()
}

/// A finished comparison's own added/removed/changed counts and its
/// folder-grouped breakdown, computed once when it arrives rather than
/// recomputed on every render — a full pass over a 50,000-entry diff is not
/// something `view()` should pay for again on every message, including the
/// once-a-second tick, just to redraw the same result.
#[derive(Debug)]
struct Diff {
    added: usize,
    removed: usize,
    changed: usize,
    groups: Vec<(PathBuf, Vec<DiffEntry>)>,
}

impl Diff {
    fn new(entries: Vec<DiffEntry>) -> Self {
        let (added, removed, changed) =
            entries
                .iter()
                .fold((0, 0, 0), |(a, r, c), entry| match entry.change {
                    Change::Added => (a + 1, r, c),
                    Change::Removed => (a, r + 1, c),
                    Change::Modified => (a, r, c + 1),
                });
        let groups = group_diff(&entries)
            .into_iter()
            .map(|(folder, refs)| (folder, refs.into_iter().cloned().collect()))
            .collect();
        Self {
            added,
            removed,
            changed,
            groups,
        }
    }

    fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }
}

/// `entries` grouped by the folder each directly sits in, relative to their
/// own common root, oldest folder path first. A folder with just one change
/// is not worth drilling into on its own; [`compare_view`] shows those
/// directly instead of as a one-entry group.
fn group_diff(entries: &[DiffEntry]) -> Vec<(PathBuf, Vec<&DiffEntry>)> {
    let root = common_ancestor(entries.iter().map(|entry| entry.path.as_path()));
    let mut groups: BTreeMap<PathBuf, Vec<&DiffEntry>> = BTreeMap::new();
    for entry in entries {
        let relative = entry.path.strip_prefix(&root).unwrap_or(&entry.path);
        let folder = relative
            .parent()
            .map_or_else(PathBuf::new, Path::to_path_buf);
        groups.entry(folder).or_default().push(entry);
    }
    groups.into_iter().collect()
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
    fn only_overwriting_in_place_with_conflicts_is_marked_as_replacing() {
        let mut sheet = Sheet {
            requests: Vec::new(),
            target: TargetChoice::Original,
            policy: ConflictPolicy::Overwrite,
            verify_existing: false,
            ownership: Ownership::Preserve,
            preview: Some(Ok(RestorePreview {
                conflicts: 4,
                ..RestorePreview::default()
            })),
            previewing: false,
        };
        assert_eq!(files_replaced_in_place(&sheet), Some(4));

        sheet.policy = ConflictPolicy::KeepBoth;
        assert_eq!(files_replaced_in_place(&sheet), None, "nothing is lost");
        sheet.policy = ConflictPolicy::Overwrite;
        sheet.target = TargetChoice::Folder(Some("/tmp/out".into()));
        assert_eq!(files_replaced_in_place(&sheet), None, "a separate folder");
        sheet.target = TargetChoice::Original;
        sheet.preview = Some(Ok(RestorePreview::default()));
        assert_eq!(files_replaced_in_place(&sheet), None, "no conflicts");
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
                restored: Some(RestorePreview {
                    files: 3,
                    bytes: 100,
                    unchanged: 1,
                    conflicts: 2,
                }),
                forgotten: None,
                pruned: None,
                pinned: None,
            }))
        };
        let second = page.update(done());
        assert!(matches!(second.as_slice(), [Effect::Restore(r)] if r.snapshot == "bbbbbbbb"));
        assert!(page.is_restoring());

        let finished = page.update(done());
        assert!(
            matches!(
                finished.as_slice(),
                [Effect::Restored(RestorePreview {
                    files: 6,
                    bytes: 200,
                    unchanged: 2,
                    conflicts: 4,
                })]
            ),
            "both parts' counts are reported, not just the last one's"
        );
        assert!(!page.is_restoring());
    }

    /// What the window feeds back when a restore cannot even start (its
    /// drive was unplugged): the page must not stay "restoring".
    #[test]
    fn a_restore_that_never_started_leaves_the_page_free() {
        let mut page = page_with_snapshots();
        page.running = Some(Running {
            handle: None,
            progress: None,
            total: RestorePreview::default(),
            queue: VecDeque::new(),
        });

        let effects = page.update(Message::Restore(ChildEvent::Ended(EngineError::new(
            crate::engine::ErrorKind::DestinationUnavailable,
            "",
        ))));

        assert!(!page.is_restoring());
        assert!(matches!(effects.as_slice(), [Effect::ShowError(..)]));
    }

    #[test]
    fn canceling_drops_the_rest_of_the_queue() {
        let mut page = page_with_snapshots();
        page.running = Some(Running {
            handle: None,
            progress: None,
            total: RestorePreview::default(),
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
        page.update(Message::Listed(
            "bbbbbbbb".into(),
            "/home/alex".into(),
            Ok(Vec::new()),
        ));
        assert!(
            page.entries.is_none(),
            "the answer for a folder we left is dropped"
        );
    }

    #[test]
    fn a_listing_for_another_snapshot_is_ignored() {
        let mut page = page_with_snapshots();
        page.update(Message::PickSnapshot(1));
        page.update(Message::Listed(
            "bbbbbbbb".into(),
            "/home/alex".into(),
            Ok(Vec::new()),
        ));
        assert!(
            page.entries.is_none(),
            "the old snapshot's folder is not shown under the new one"
        );
    }

    fn entry(path: &str) -> TreeEntry {
        TreeEntry {
            name: path.rsplit('/').next().unwrap_or_default().to_owned(),
            path: path.into(),
            kind: EntryKind::File,
            size: 1,
            modified: None,
        }
    }

    #[test]
    fn a_search_answered_for_another_snapshot_is_dropped() {
        let mut page = page_with_snapshots();
        page.search = "notes".into();
        page.update(Message::SearchNow);
        page.update(Message::PickSnapshot(1));

        page.update(Message::Found(
            "bbbbbbbb".into(),
            "notes".into(),
            Ok(vec![entry("/home/alex/notes.txt")]),
        ));

        assert!(
            page.results.is_none(),
            "snapshot b's hits are not shown under a"
        );
    }

    #[test]
    fn a_search_answered_for_an_older_query_is_dropped() {
        let mut page = page_with_snapshots();
        page.search = "old".into();
        page.update(Message::SearchNow);
        page.search = "new".into();
        page.update(Message::SearchNow);

        page.update(Message::Found(
            "bbbbbbbb".into(),
            "old".into(),
            Ok(vec![entry("/home/alex/old.txt")]),
        ));
        assert!(page.results.is_none());
        assert!(page.searching.is_some(), "still waiting for the newer one");

        page.update(Message::Found(
            "bbbbbbbb".into(),
            "new".into(),
            Ok(vec![entry("/home/alex/new.txt")]),
        ));
        assert_eq!(page.results.as_ref().map(Vec::len), Some(1));
        assert!(page.searching.is_none());
    }

    #[test]
    fn a_deleted_files_answer_for_another_folder_is_dropped() {
        let mut page = page_with_snapshots();
        page.update(Message::FindMissing);
        let (scope, since) = page.finding_missing.clone().unwrap();
        page.update(Message::ScopeChosen("/home/alex/other".into()));

        page.update(Message::MissingFound(scope, since, Ok(Vec::new())));

        assert!(
            page.missing.is_none(),
            "the first folder's list is not shown"
        );
        assert!(page.finding_missing.is_some());
    }

    #[test]
    fn a_comparison_answered_after_the_pick_changed_is_dropped() {
        let mut page = page_with_snapshots();
        page.update(Message::Compare);
        page.update(Message::PickFrom(0));

        page.update(Message::Compared(
            "aaaaaaaa".into(),
            "bbbbbbbb".into(),
            Ok(Vec::new()),
        ));

        assert!(page.diff.is_none());
        assert!(page.comparing.is_none(), "and Compare can be pressed again");
    }

    #[test]
    fn a_search_answered_after_the_field_was_cleared_or_a_folder_opened_is_dropped() {
        for leave in [
            Message::Search(String::new()),
            Message::Open("/home/alex/Documents".into()),
        ] {
            let mut page = page_with_snapshots();
            page.search = "notes".into();
            page.update(Message::SearchNow);
            page.update(leave);

            page.update(Message::Found(
                "bbbbbbbb".into(),
                "notes".into(),
                Ok(vec![entry("/home/alex/notes.txt")]),
            ));

            assert!(page.results.is_none());
        }
    }

    #[test]
    fn a_global_search_answered_for_an_older_query_is_dropped() {
        let mut page = page_with_snapshots();
        page.global_query = "old".into();
        page.update(Message::GlobalSearchNow);
        page.global_query = "new".into();
        page.update(Message::GlobalSearchNow);

        page.update(Message::GlobalFound("old".into(), Ok(Vec::new())));

        assert!(page.global_results.is_none());
        assert!(page.global_searching.is_some());
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

        let effects = page.update(Message::Listed(
            "bbbbbbbb".into(),
            "/home/alex/gone".into(),
            failed(),
        ));
        assert!(
            matches!(effects.as_slice(), [Effect::List { dir, .. }] if dir == Path::new("/home/alex")),
            "first back to where browsing started"
        );

        let effects = page.update(Message::Listed(
            "bbbbbbbb".into(),
            "/home/alex".into(),
            failed(),
        ));
        assert!(
            matches!(effects.as_slice(), [Effect::List { dir, .. }] if dir == Path::new("/")),
            "then to the top"
        );

        let effects = page.update(Message::Listed("bbbbbbbb".into(), "/".into(), failed()));
        assert!(
            matches!(effects.as_slice(), [Effect::ShowError(..)]),
            "and only then an error, never a loop"
        );
    }

    fn diff_entry(path: &str, change: Change) -> DiffEntry {
        DiffEntry {
            path: path.into(),
            change,
            is_dir: false,
            size: 1,
        }
    }

    #[test]
    fn common_ancestor_finds_the_longest_shared_prefix() {
        let paths = [
            PathBuf::from("/home/alex/project/src/main.rs"),
            PathBuf::from("/home/alex/project/src/lib.rs"),
            PathBuf::from("/home/alex/project/README.md"),
        ];
        assert_eq!(
            common_ancestor(paths.iter().map(PathBuf::as_path)),
            PathBuf::from("/home/alex/project")
        );
    }

    #[test]
    fn common_ancestor_of_one_path_is_its_own_parent_chain() {
        let paths = [PathBuf::from("/home/alex/only.txt")];
        assert_eq!(
            common_ancestor(paths.iter().map(PathBuf::as_path)),
            PathBuf::from("/home/alex/only.txt")
        );
    }

    #[test]
    fn group_diff_puts_every_entry_directly_in_the_common_root_together() {
        let entries = vec![
            diff_entry("/home/alex/a.txt", Change::Added),
            diff_entry("/home/alex/b.txt", Change::Modified),
        ];

        let groups = group_diff(&entries);

        assert_eq!(groups.len(), 1, "both entries share one, empty, group key");
        let (folder, group_entries) = &groups[0];
        assert!(folder.as_os_str().is_empty());
        assert_eq!(group_entries.len(), 2);
    }

    #[test]
    fn group_diff_separates_entries_in_different_subfolders() {
        let entries = vec![
            diff_entry("/home/alex/project/src/main.rs", Change::Modified),
            diff_entry("/home/alex/project/docs/readme.md", Change::Modified),
        ];

        let groups = group_diff(&entries);

        assert_eq!(groups.len(), 2, "src and docs are separate groups");
        assert!(groups.iter().any(|(folder, _)| folder == Path::new("src")));
        assert!(groups.iter().any(|(folder, _)| folder == Path::new("docs")));
    }

    #[test]
    fn diff_counts_and_groups_are_computed_once_not_on_every_render() {
        let entries = vec![
            diff_entry("/home/alex/a.txt", Change::Added),
            diff_entry("/home/alex/b.txt", Change::Removed),
            diff_entry("/home/alex/project/src/main.rs", Change::Modified),
            diff_entry("/home/alex/project/src/lib.rs", Change::Modified),
        ];

        let diff = Diff::new(entries);

        assert!(!diff.is_empty());
        assert_eq!((diff.added, diff.removed, diff.changed), (1, 1, 2));
        assert_eq!(diff.groups.len(), 2, "one root group, one src group");
    }

    #[test]
    fn an_empty_diff_reports_itself_empty() {
        let diff = Diff::new(Vec::new());
        assert!(diff.is_empty());
        assert_eq!((diff.added, diff.removed, diff.changed), (0, 0, 0));
    }

    #[test]
    fn a_folders_diff_expansion_toggles() {
        let mut page = page_with_snapshots();
        let folder = PathBuf::from("src");

        page.update(Message::ToggleDiffFolder(folder.clone()));
        assert!(page.diff_expanded.contains(&folder));

        page.update(Message::ToggleDiffFolder(folder.clone()));
        assert!(!page.diff_expanded.contains(&folder));
    }

    #[test]
    fn comparing_again_clears_the_previous_expansion() {
        let mut page = page_with_snapshots();
        page.diff_expanded.insert(PathBuf::from("src"));

        page.update(Message::Compare);

        assert!(page.diff_expanded.is_empty());
    }

    #[test]
    fn jump_to_match_switches_to_browse_at_the_matched_snapshot_and_folder() {
        let mut page = page_with_snapshots();
        page.tab = Tab::Search;

        let effects = page.update(Message::JumpToMatch(
            "aaaaaaaa".into(),
            "/home/alex/project/report.txt".into(),
        ));

        assert_eq!(page.tab, Tab::Browse);
        assert_eq!(page.snapshot, Some(1), "aaaaaaaa is the second snapshot");
        assert_eq!(page.dir, PathBuf::from("/home/alex/project"));
        assert!(matches!(effects.as_slice(), [Effect::List { .. }]));
    }

    #[test]
    fn jump_to_an_unknown_snapshot_does_nothing() {
        let mut page = page_with_snapshots();
        page.tab = Tab::Search;

        let effects = page.update(Message::JumpToMatch("nope".into(), "/x".into()));

        assert_eq!(page.tab, Tab::Search, "nothing to jump to");
        assert!(effects.is_empty());
    }

    #[test]
    fn an_empty_global_search_clears_any_previous_results_without_a_fetch() {
        let mut page = page_with_snapshots();
        page.global_results = Some(vec![]);
        page.global_query = "  ".into();

        let effects = page.update(Message::GlobalSearchNow);

        assert!(page.global_results.is_none());
        assert!(effects.is_empty());
    }

    #[test]
    fn a_global_search_asks_for_the_current_query() {
        let mut page = page_with_snapshots();
        page.update(Message::GlobalSearch("report".into()));

        let effects = page.update(Message::GlobalSearchNow);

        assert!(
            matches!(effects.as_slice(), [Effect::GlobalSearch { query }] if query == "report")
        );
    }
}
