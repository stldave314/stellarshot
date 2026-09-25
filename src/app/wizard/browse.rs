// SPDX-License-Identifier: GPL-3.0-only

//! Browsing one of the "what" step's included folders by size, to mark
//! subfolders excluded without leaving the wizard: sizes on every row, and
//! a clear mark for whether a folder is going in whole, left out, or
//! partly one and partly the other.
//!
//! Rooted at a chosen source, not anywhere on disk: excluding is the only
//! thing this does (see [`Mark`]), and excluding only means something
//! relative to a folder that is already included. A folder already marked
//! excluded can still be opened to see what is being left out and how
//! large it is, but nothing under it can be re-included from here — there
//! is no way in Stellarshot's own exclude list to say "this whole folder,
//! except this one thing inside it", so offering that control would be a
//! button that quietly does nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cosmic::iced::{Alignment, Length};
use cosmic::{Element, theme, widget};

use crate::app::format;
use crate::engine::DiskEntry;
use crate::fl;

#[derive(Debug, Clone)]
pub enum Message {
    /// Open the browser, rooted at this already-included source.
    Open(PathBuf),
    Close,
    /// Expand or collapse a folder row.
    Toggle(PathBuf),
    /// `path`'s listing is still going; `scanned` is how many entries have
    /// been sized so far, for a running count while a large folder loads.
    Progress(PathBuf, usize),
    /// `path`'s children, sized and sorted largest first.
    Listed(PathBuf, Vec<DiskEntry>),
    Failed(PathBuf, String),
    /// Mark `path` excluded (`true`) or included (`false`). Only offered
    /// for a folder that is not already beneath an excluded ancestor.
    Mark(PathBuf, bool),
}

#[derive(Debug, Clone)]
pub enum Effect {
    /// List `path`'s immediate children with their sizes.
    List(PathBuf),
    /// The user asked to exclude or re-include `path`; the wizard's own
    /// `excludes` list is not this module's to hold, so the change bubbles
    /// up rather than being applied here.
    SetExcluded(PathBuf, bool),
}

#[derive(Debug, Clone, Default)]
struct Node {
    is_dir: bool,
    size: Option<u64>,
    /// Ordered child paths, once listed.
    children: Option<Vec<PathBuf>>,
    loading: bool,
    scanned: usize,
    failed: Option<String>,
}

/// Whether a folder is going into the backup, being left out, or a mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Included,
    Excluded,
    /// Included overall, but something inside it is excluded.
    Partial,
}

#[derive(Debug, Clone, Default)]
pub struct Browse {
    root: Option<PathBuf>,
    nodes: BTreeMap<PathBuf, Node>,
}

impl Browse {
    pub fn is_open(&self) -> bool {
        self.root.is_some()
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        match message {
            Message::Open(root) => {
                self.nodes.clear();
                self.nodes.insert(
                    root.clone(),
                    Node {
                        is_dir: true,
                        loading: true,
                        ..Node::default()
                    },
                );
                let effect = Effect::List(root.clone());
                self.root = Some(root);
                vec![effect]
            }
            Message::Close => {
                self.root = None;
                Vec::new()
            }
            Message::Toggle(path) => {
                let Some(node) = self.nodes.get_mut(&path) else {
                    return Vec::new();
                };
                if node.children.is_some() {
                    node.children = None;
                    Vec::new()
                } else if node.loading {
                    Vec::new()
                } else {
                    node.loading = true;
                    node.failed = None;
                    vec![Effect::List(path)]
                }
            }
            Message::Progress(path, scanned) => {
                if let Some(node) = self.nodes.get_mut(&path) {
                    node.scanned = scanned;
                }
                Vec::new()
            }
            Message::Listed(path, entries) => {
                let order = entries.iter().map(|entry| entry.path.clone()).collect();
                for entry in entries {
                    self.nodes.insert(
                        entry.path,
                        Node {
                            is_dir: entry.is_dir,
                            size: Some(entry.size),
                            ..Node::default()
                        },
                    );
                }
                if let Some(node) = self.nodes.get_mut(&path) {
                    node.loading = false;
                    node.children = Some(order);
                }
                Vec::new()
            }
            Message::Failed(path, detail) => {
                if let Some(node) = self.nodes.get_mut(&path) {
                    node.loading = false;
                    node.failed = Some(detail);
                }
                Vec::new()
            }
            Message::Mark(path, excluded) => vec![Effect::SetExcluded(path, excluded)],
        }
    }

    /// The tree, rooted at the open source, marked against `excludes`.
    /// `excludes` is the wizard's own list, not this module's: a mark is
    /// only ever read from it here, never written except through
    /// [`Effect::SetExcluded`].
    pub fn view<'a>(&'a self, excludes: &'a [PathBuf]) -> Option<Element<'a, Message>> {
        let root = self.root.as_ref()?;
        let spacing = theme::active().cosmic().spacing;
        let mut rows = Vec::new();
        self.push_rows(&mut rows, root, 0, excludes, false);
        Some(
            widget::column::with_capacity(2)
                .spacing(spacing.space_s)
                .push(
                    widget::row::with_capacity(2)
                        .align_y(Alignment::Center)
                        .push(widget::text::heading(format::path(root)).width(Length::Fill))
                        .push(
                            widget::button::standard(fl!("browse-close")).on_press(Message::Close),
                        ),
                )
                .push(
                    widget::scrollable(
                        widget::column::with_children(rows)
                            .spacing(spacing.space_xxs)
                            // The scrollbar draws over the content rather
                            // than reserving its own width, so without this
                            // it sits on top of each row's own checkbox.
                            .padding([0.0, f32::from(spacing.space_m), 0.0, 0.0]),
                    )
                    .height(Length::Fixed(320.0)),
                )
                .into(),
        )
    }

    fn push_rows<'a>(
        &'a self,
        rows: &mut Vec<Element<'a, Message>>,
        path: &'a Path,
        depth: u16,
        excludes: &'a [PathBuf],
        under_excluded_ancestor: bool,
    ) {
        let Some(node) = self.nodes.get(path) else {
            return;
        };
        let mark = mark_of(path, excludes);
        let is_excluded_here = mark == Mark::Excluded;
        rows.push(self.row(path, node, depth, mark, excludes, under_excluded_ancestor));
        if let Some(children) = &node.children {
            for child in children {
                self.push_rows(
                    rows,
                    child,
                    depth + 1,
                    excludes,
                    under_excluded_ancestor || is_excluded_here,
                );
            }
        } else if node.loading {
            rows.push(
                widget::row::with_capacity(1)
                    .padding([0.0, 0.0, 0.0, indent(depth + 1)])
                    .push(widget::text::caption(fl!(
                        "browse-scanning",
                        count = (node.scanned as i64)
                    )))
                    .into(),
            );
        } else if let Some(detail) = &node.failed {
            rows.push(
                widget::row::with_capacity(1)
                    .padding([0.0, 0.0, 0.0, indent(depth + 1)])
                    .push(widget::text::caption(detail.clone()))
                    .into(),
            );
        }
    }

    /// How many bytes under `path` are excluded, based on what has been
    /// sized so far. A folder's own listed size is real disk usage,
    /// unadjusted for what `excludes` takes out of it; this is what to
    /// subtract from it to show what the backup would actually store.
    /// Undercounts an excluded path this session never sized (never
    /// expanded down to) — the common case, a folder like `.cache` right
    /// under the source, is sized the moment its parent is opened, so this
    /// still catches most of what matters without a separate size fetch
    /// for every entry in the exclude list.
    fn excluded_bytes(&self, path: &Path, excludes: &[PathBuf]) -> u64 {
        excludes
            .iter()
            .filter(|excluded| excluded.starts_with(path))
            .filter_map(|excluded| self.nodes.get(excluded).and_then(|node| node.size))
            .sum()
    }

    fn row<'a>(
        &self,
        path: &'a Path,
        node: &Node,
        depth: u16,
        mark: Mark,
        excludes: &[PathBuf],
        under_excluded_ancestor: bool,
    ) -> Element<'a, Message> {
        let spacing = theme::active().cosmic().spacing;
        let name = if depth == 0 {
            format::path(path)
        } else {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let mut row = widget::row::with_capacity(4)
            .spacing(spacing.space_xxs)
            .align_y(Alignment::Center)
            .padding([0.0, 0.0, 0.0, indent(depth)]);
        if node.is_dir {
            let expanded = node.children.is_some();
            row = row.push(
                widget::button::icon(widget::icon::from_name(if expanded {
                    "go-down-symbolic"
                } else {
                    "go-next-symbolic"
                }))
                .padding(spacing.space_xxxs)
                .on_press(Message::Toggle(path.to_path_buf())),
            );
        } else {
            row = row.push(widget::Space::new().width(Length::Fixed(24.0)));
        }
        row = row.push(widget::text::body(name).width(Length::Fill));
        if let Some(size) = node.size {
            let reduced = node.is_dir.then(|| self.excluded_bytes(path, excludes));
            let text = match reduced {
                Some(reduced) if reduced > 0 => fl!(
                    "browse-size-reduced",
                    size = format::bytes(size.saturating_sub(reduced)),
                    total = format::bytes(size)
                ),
                _ => format::bytes(size),
            };
            row = row.push(widget::text::caption(text));
        }
        // A row already under an excluded ancestor has nothing meaningful
        // left to toggle: the ancestor's own mark already decides it, and
        // Stellarshot's exclude list cannot re-include one thing inside an
        // excluded folder while leaving the rest of it out. Shown as an
        // unchecked, non-interactive checkbox rather than dropped entirely,
        // so every row in the tree reads the same way at a glance.
        if node.is_dir {
            let checked = mark != Mark::Excluded;
            let mut checkbox = widget::checkbox(checked);
            if mark == Mark::Partial {
                checkbox = checkbox.label(fl!("browse-mark-partial"));
            }
            if !under_excluded_ancestor {
                let owned = path.to_path_buf();
                checkbox = checkbox.on_toggle(move |on| Message::Mark(owned.clone(), !on));
            }
            row = row.push(checkbox);
        }
        row.into()
    }
}

fn indent(depth: u16) -> f32 {
    f32::from(depth) * 20.0
}

/// Whether `path` is fully included, fully excluded, or a mix, given the
/// wizard's own `excludes` list. A path is only ever "included" here in the
/// sense of "not excluded" — whether it is under a source at all is up to
/// the caller, which only ever asks about paths already known to be under
/// the open browser's root source.
fn mark_of(path: &Path, excludes: &[PathBuf]) -> Mark {
    if excludes.iter().any(|excluded| path == excluded) {
        return Mark::Excluded;
    }
    if excludes
        .iter()
        .any(|excluded| excluded.starts_with(path) && excluded != path)
    {
        return Mark::Partial;
    }
    Mark::Included
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, is_dir: bool, size: u64) -> DiskEntry {
        DiskEntry {
            name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            path: PathBuf::from(path),
            is_dir,
            size,
        }
    }

    #[test]
    fn opening_lists_the_root() {
        let mut browse = Browse::default();
        let effects = browse.update(Message::Open(PathBuf::from("/home/alex")));
        assert!(
            matches!(effects.as_slice(), [Effect::List(path)] if path == Path::new("/home/alex"))
        );
        assert!(browse.is_open());
    }

    #[test]
    fn toggling_an_unlisted_folder_requests_its_children() {
        let mut browse = Browse::default();
        browse.update(Message::Open(PathBuf::from("/home/alex")));
        browse.update(Message::Listed(
            PathBuf::from("/home/alex"),
            vec![entry("/home/alex/Documents", true, 100)],
        ));

        let effects = browse.update(Message::Toggle(PathBuf::from("/home/alex/Documents")));

        assert!(matches!(
            effects.as_slice(),
            [Effect::List(path)] if path == Path::new("/home/alex/Documents")
        ));
    }

    #[test]
    fn toggling_an_expanded_folder_collapses_it_without_a_new_request() {
        let mut browse = Browse::default();
        browse.update(Message::Open(PathBuf::from("/home/alex")));
        browse.update(Message::Listed(PathBuf::from("/home/alex"), vec![]));

        let effects = browse.update(Message::Toggle(PathBuf::from("/home/alex")));

        assert!(effects.is_empty());
        assert!(browse.nodes[Path::new("/home/alex")].children.is_none());
    }

    #[test]
    fn excluded_bytes_sums_every_known_excluded_descendant() {
        let mut browse = Browse::default();
        browse.update(Message::Open(PathBuf::from("/home/alex")));
        browse.update(Message::Listed(
            PathBuf::from("/home/alex"),
            vec![
                entry("/home/alex/Documents", true, 1000),
                entry("/home/alex/.cache", true, 300),
            ],
        ));

        let excludes = [PathBuf::from("/home/alex/.cache")];

        assert_eq!(
            browse.excluded_bytes(Path::new("/home/alex"), &excludes),
            300,
            "a folder right under the root is sized the moment the root is opened"
        );
        assert_eq!(
            browse.excluded_bytes(Path::new("/home/alex/Documents"), &excludes),
            0,
            "nothing excluded under this one"
        );
    }

    #[test]
    fn excluded_bytes_is_zero_for_an_exclude_never_sized_this_session() {
        let mut browse = Browse::default();
        browse.update(Message::Open(PathBuf::from("/home/alex")));
        browse.update(Message::Listed(PathBuf::from("/home/alex"), vec![]));

        // Deep enough that opening the root alone never loaded it.
        let excludes = [PathBuf::from("/home/alex/Projects/target")];

        assert_eq!(
            browse.excluded_bytes(Path::new("/home/alex"), &excludes),
            0,
            "undercounts rather than guesses at a size it never actually read"
        );
    }

    #[test]
    fn marking_bubbles_up_rather_than_being_applied_here() {
        let mut browse = Browse::default();
        let effects = browse.update(Message::Mark(PathBuf::from("/home/alex/.cache"), true));
        assert!(matches!(
            effects.as_slice(),
            [Effect::SetExcluded(path, true)] if path == Path::new("/home/alex/.cache")
        ));
    }

    #[test]
    fn mark_of_a_plain_folder_is_included() {
        assert_eq!(
            mark_of(Path::new("/home/alex/Documents"), &[]),
            Mark::Included
        );
    }

    #[test]
    fn mark_of_a_listed_exclude_is_excluded() {
        let excludes = [PathBuf::from("/home/alex/.cache")];
        assert_eq!(
            mark_of(Path::new("/home/alex/.cache"), &excludes),
            Mark::Excluded
        );
    }

    #[test]
    fn mark_of_an_ancestor_of_an_exclude_is_partial() {
        let excludes = [PathBuf::from("/home/alex/Projects/target")];
        assert_eq!(
            mark_of(Path::new("/home/alex/Projects"), &excludes),
            Mark::Partial
        );
        assert_eq!(mark_of(Path::new("/home/alex"), &excludes), Mark::Partial);
    }

    #[test]
    fn mark_of_an_unrelated_folder_is_included() {
        let excludes = [PathBuf::from("/home/alex/.cache")];
        assert_eq!(
            mark_of(Path::new("/home/alex/Documents"), &excludes),
            Mark::Included
        );
    }
}
