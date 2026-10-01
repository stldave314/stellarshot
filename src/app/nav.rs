// SPDX-License-Identifier: GPL-3.0-only

//! The sidebar: its rows, which one is selected, and keeping each row's
//! status in step with the backup it stands for.

use super::*;

/// What a sidebar entry leads to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NavItem {
    /// Every backup at a glance: status, folders and storage locations.
    Home,
    /// Every backup's history in one place, across every profile.
    History,
    Profile(String),
    /// Opens a fresh wizard. Replaced by `Wizard` once one is in progress.
    New,
    /// A wizard is in progress; selecting this shows it again.
    Wizard,
}

impl App {
    /// The profile the sidebar has selected.
    pub(super) fn selected(&self) -> Option<&str> {
        match self.nav.active_data::<NavItem>() {
            Some(NavItem::Profile(id)) => Some(id.as_str()),
            _ => None,
        }
    }

    /// The home screen, rather than a particular backup, is showing.
    pub(super) fn showing_home(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::Home))
    }

    /// The History page is showing.
    pub(super) fn showing_history(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::History))
    }

    /// The wizard, rather than the home screen or a particular backup, is
    /// showing. A wizard can exist (`self.wizard.is_some()`) without this
    /// being true: "finish later" leaves it running but out of view.
    pub(super) fn showing_wizard(&self) -> bool {
        matches!(self.nav.active_data::<NavItem>(), Some(NavItem::Wizard))
    }

    /// Show the wizard: rebuilds the sidebar (so its entry exists) and
    /// selects it. Called whenever a wizard is created or resumed.
    pub(super) fn select_wizard(&mut self) {
        self.rebuild_nav(None);
        let entity = self
            .nav
            .iter()
            .find(|&entity| matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Wizard)));
        if let Some(entity) = entity {
            self.nav.activate(entity);
        }
    }

    /// Leave the wizard running but move the window away from it: "finish
    /// later". Always goes to the home screen, a predictable place to
    /// return from regardless of where the wizard was opened.
    pub(super) fn go_home(&mut self) {
        let entity = self
            .nav
            .iter()
            .find(|&entity| matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Home)));
        if let Some(entity) = entity {
            self.nav.activate(entity);
        }
    }

    /// Stop the wizard's own background work and forget it entirely:
    /// "discard".
    pub(super) fn discard_wizard(&mut self) {
        if let Some(wizard) = &self.wizard {
            wizard.discard();
        }
        self.wizard = None;
        self.wizard_session += 1;
        self.rebuild_nav(None);
        // The wizard's own row was showing and is gone; without this the
        // "New backup" row it leaves behind stays selected over a blank page.
        if self.selected().is_none() && !self.showing_home() && !self.showing_history() {
            self.go_home();
        }
    }

    /// Rebuild the sidebar from the settings, keeping the selection when the
    /// selected profile still exists.
    pub(super) fn rebuild_nav(&mut self, select: Option<&str>) {
        let keep = select
            .map(str::to_owned)
            .or_else(|| self.selected().map(str::to_owned));
        // Only when nothing more specific was asked for: a rebuild while the
        // home screen or the wizard is showing (a backup finished, its
        // schedule changed) must not silently jump the window to the first
        // backup instead.
        let keep_home = select.is_none() && self.showing_home();
        let keep_history = select.is_none() && self.showing_history();
        let keep_wizard = select.is_none() && self.showing_wizard();
        self.nav.clear();
        let home = self
            .nav
            .insert()
            .text(fl!("home"))
            .icon(widget::icon::from_name("go-home-symbolic"))
            .data(NavItem::Home)
            .id();
        let history = self
            .nav
            .insert()
            .text(fl!("history-title"))
            .icon(widget::icon::from_name("emblem-documents-symbolic"))
            .data(NavItem::History)
            .id();
        let mut chosen = if keep_home {
            Some(home)
        } else if keep_history {
            Some(history)
        } else {
            None
        };
        for profile in &self.config.profiles {
            let status = self.backup_status(profile);
            let text = self.nav_row_text(profile, status);
            let id = self
                .nav
                .insert()
                .text(text)
                .icon(widget::icon::from_name(status.icon()))
                .data(NavItem::Profile(profile.id.clone()))
                .id();
            if keep.as_deref() == Some(profile.id.as_str())
                || (chosen.is_none() && !keep_home && !keep_history && !keep_wizard)
            {
                chosen = Some(id);
            }
        }
        if !self.config.profiles.is_empty() {
            // A wizard already in progress is never replaced by a fresh
            // "New backup": there is only ever one at a time, and starting
            // another would silently lose it.
            let (text, icon, item) = if self.wizard.is_some() {
                (
                    fl!("wizard-resume"),
                    "document-edit-symbolic",
                    NavItem::Wizard,
                )
            } else {
                (fl!("new-backup"), "list-add-symbolic", NavItem::New)
            };
            let id = self
                .nav
                .insert()
                .text(text)
                .icon(widget::icon::from_name(icon))
                .data(item)
                .divider_above(true)
                .id();
            if keep_wizard {
                chosen = Some(id);
            }
        }
        if let Some(id) = chosen {
            self.nav.activate(id);
        }
    }

    /// Update one profile's own sidebar row in place — its text (the name,
    /// or a running backup's progress) and icon — without touching any
    /// other row's entity ID, the sidebar's selection, or keyboard focus in
    /// it. `rebuild_nav` clears and reinserts every row, including a fresh
    /// entity ID for each one, which is fine for the profile list or the
    /// wizard's own presence actually changing, but was until now the only
    /// way this page had to reflect anything at all — including a password
    /// keystroke or a progress event arriving every 250ms during a backup,
    /// which dropped whatever had focus in the sidebar each time.
    pub(super) fn refresh_nav_row(&mut self, id: &str) {
        let Some(profile) = self.config.profile(id) else {
            return;
        };
        let status = self.backup_status(profile);
        let text = self.nav_row_text(profile, status);
        let icon = widget::icon::from_name(status.icon());
        let entity = self.nav.iter().find(|&entity| {
            matches!(self.nav.data::<NavItem>(entity), Some(NavItem::Profile(p)) if p == id)
        });
        if let Some(entity) = entity {
            self.nav.text_set(entity, text);
            self.nav.icon_set(entity, icon.into());
        }
    }

    /// `profile`'s state for the sidebar: its run facts, and whether the
    /// window has work running for it right now.
    pub(super) fn backup_status(&self, profile: &Profile) -> run_state::BackupStatus {
        let run = self.runs.get(&profile.id).cloned().unwrap_or_default();
        let running = self
            .pages
            .get(&profile.id)
            .is_some_and(profile::ProfileState::is_busy);
        run_state::status(profile, &run, running)
    }

    /// The sidebar row's text: just the name, unless a backup is running
    /// right now, when the nav row's only way to show progress is its text.
    pub(super) fn nav_row_text(
        &self,
        profile: &Profile,
        status: run_state::BackupStatus,
    ) -> String {
        if status != run_state::BackupStatus::Running {
            return profile.name.clone();
        }
        match self
            .pages
            .get(&profile.id)
            .and_then(profile::ProfileState::progress_fraction)
        {
            Some(fraction) => fl!(
                "nav-running-percent",
                name = profile.name.clone(),
                percent = ((fraction * 100.0).round() as i64)
            ),
            None => fl!("nav-running", name = profile.name.clone()),
        }
    }

    /// Re-read every backup's run state; rebuild the sidebar if a warning
    /// appeared or went away.
    pub(super) fn reload_runs(&mut self) {
        let runs: HashMap<String, RunState> = self
            .config
            .profiles
            .iter()
            .map(|profile| (profile.id.clone(), run_state::load(&profile.id)))
            .collect();
        if runs != self.runs {
            self.runs = runs;
            self.rebuild_nav(None);
        }
    }

    /// Change one backup's run state, and show the change.
    pub(super) fn record_run(&mut self, id: &str, change: impl FnOnce(&mut RunState)) {
        if let Err(err) = run_state::update(id, change) {
            error_log!(CONFIG, "could not record a run of {id}: {err}");
        }
        self.reload_runs();
    }
}
