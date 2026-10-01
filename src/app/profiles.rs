// SPDX-License-Identifier: GPL-3.0-only

//! The list of backups: saving it, adding and removing one, and what has
//! to follow each change (its schedule, its keyring items, its state).

use crate::app::{
    App, Destination, DialogMessage, EngineError, Message, Profile, Secret, StellarshotConfig,
    Task, app, engine, event_log, run_state, tasks, timers,
};
use crate::debug::CONFIG;
use crate::{error_log, fl};

impl App {
    /// Put `secret` in the keyring for the backup `profile_id`, and report
    /// it if the keyring refuses: the repository itself is unaffected, but a
    /// scheduled run would have no password to use.
    pub(in crate::app) fn remember_task(
        profile_id: String,
        name: String,
        secret: Secret,
    ) -> Task<Message> {
        Task::perform(
            async move { crate::keyring::store(&profile_id, &name, &secret).await },
            |result| {
                app(Message::Dialog(DialogMessage::PasswordNotRemembered(
                    result.err(),
                )))
            },
        )
    }

    pub(in crate::app) fn apply_schedule(profile: Profile) -> Task<Message> {
        Task::perform(
            tasks::blocking(move || {
                timers::apply(&profile)
                    .map_err(|err| EngineError::new(engine::ErrorKind::Internal, err))
            }),
            |result| match result {
                Ok(()) => app(Message::Noop),
                Err(err) => app(Message::ScheduleFailed(err.detail)),
            },
        )
    }

    /// Writes `profiles` to disk, keeping `self.config.profiles` in sync
    /// only when it actually succeeds. Returns whether it did; every
    /// caller must skip whatever it was about to do on the strength of
    /// this save (installing a schedule, opening the profile it
    /// just "added") when this comes back `false`,
    /// since none of that reflects what is actually on disk.
    ///
    /// Refuses outright, without writing anything, while
    /// `self.profiles_read_only` is set: the file this would overwrite
    /// could not be read as it was found (see `App::init`'s handling of
    /// `Flags::profiles_unreadable`), and writing the in-memory list over
    /// it would make that loss permanent instead of leaving the original
    /// bytes for a later Stellarshot version, or the user by hand, to
    /// recover.
    pub(in crate::app) fn save_profiles(&mut self, profiles: Vec<Profile>) -> bool {
        if self.profiles_read_only {
            error_log!(
                CONFIG,
                "refusing to save profiles: the profiles list could not be read at startup"
            );
            self.show_error(
                &fl!("error-settings-not-saved"),
                &EngineError::new(
                    engine::ErrorKind::Internal,
                    "the profiles file could not be read at startup",
                ),
            );
            return false;
        }
        let result = if let Some(handler) = &self.config_handler {
            self.config.set_profiles(handler, profiles).map_err(|err| {
                // `set_profiles` already assigned `self.config.profiles`
                // to the new value before this write failed (the derived
                // setter's own doing, not fixable here): reloading from
                // disk undoes that, rather than leaving the window
                // showing a list that was never actually saved.
                self.config = StellarshotConfig::config();
                err.to_string()
            })
        } else {
            self.config = StellarshotConfig::config();
            Err("no config handler".to_owned())
        };
        match result {
            Ok(_) => true,
            Err(detail) => {
                error_log!(CONFIG, "failed to save profiles: {detail}");
                self.show_error(
                    &fl!("error-settings-not-saved"),
                    &EngineError::new(engine::ErrorKind::Io, detail),
                );
                false
            }
        }
    }

    /// Replace a profile by ID, or add it: saves `profile`, whether it is new
    /// or replacing one with the same ID.
    ///
    /// Returns `false` when the save itself failed: the caller must not
    /// treat the profile as added or updated (no schedule, no opening it),
    /// since none of that would match what is actually on disk.
    pub(in crate::app) fn upsert_profile(&mut self, profile: Profile) -> bool {
        let mut profiles = self.config.profiles.clone();
        match profiles
            .iter_mut()
            .find(|existing| existing.id == profile.id)
        {
            Some(existing) => *existing = profile,
            None => profiles.push(profile),
        }
        self.save_profiles(profiles)
    }

    pub(in crate::app) fn remove_profile(&mut self, id: &str) -> Task<Message> {
        let profiles = self
            .config
            .profiles
            .iter()
            .filter(|p| p.id != id)
            .cloned()
            .collect();
        if !self.save_profiles(profiles) {
            // Nothing on disk actually changed; undo the parts of this
            // that assumed it had.
            return Task::none();
        }
        self.pages.remove(id);
        self.runs.remove(id);
        self.rebuild_nav(None);
        let id = id.to_owned();
        let unschedule = {
            let id = id.clone();
            Task::perform(
                tasks::blocking(move || {
                    // Profile IDs are never reused, so what is left behind
                    // is never read again; failing to remove it is logged,
                    // not shown.
                    for result in [run_state::remove(&id), event_log::remove(&id)] {
                        if let Err(err) = result {
                            error_log!(CONFIG, "could not forget the state of {id}: {err}");
                        }
                    }
                    timers::remove(&id)
                        .map_err(|err| EngineError::new(engine::ErrorKind::Internal, err))
                }),
                |result| match result {
                    Ok(()) => app(Message::Noop),
                    Err(err) => app(Message::ScheduleFailed(err.detail)),
                },
            )
        };
        // A password left in the keyring when removing the backup promised
        // to forget it is worth saying so: nothing else will ever clear it.
        let forget = Task::perform(
            async move {
                let rest = crate::profile::forget_rest_password(&id).await;
                crate::keyring::forget(&id).await.and(rest)
            },
            |result| match result {
                Ok(()) => app(Message::Noop),
                Err(detail) => app(Message::Dialog(DialogMessage::Failed(
                    fl!("remove-keyring-failed"),
                    EngineError::new(engine::ErrorKind::Internal, detail),
                ))),
            },
        );
        Task::batch([unschedule, forget, self.activate_selected()])
    }

    /// See [`crate::profile::secure_rest_passwords`].
    pub(in crate::app) fn secure_rest_passwords(&self) -> Task<Message> {
        let profiles: Vec<Profile> = self
            .config
            .profiles
            .iter()
            .filter(|profile| matches!(profile.destination, Destination::Rest { .. }))
            .cloned()
            .collect();
        if profiles.is_empty() {
            return Task::none();
        }
        Task::perform(crate::profile::secure_rest_passwords(profiles), |moved| {
            app(Message::RestPasswordsMoved(moved))
        })
    }

    /// Save the addresses whose password the keyring now holds, unless the
    /// address changed meanwhile.
    pub(in crate::app) fn rest_passwords_moved(&mut self, moved: Vec<(String, String)>) {
        if moved.is_empty() || self.profiles_read_only {
            return;
        }
        let mut profiles = self.config.profiles.clone();
        let mut changed = false;
        for (id, without) in moved {
            if let Some(profile) = profiles.iter_mut().find(|profile| profile.id == id)
                && let Destination::Rest { url } = &mut profile.destination
                && crate::profile::split_rest_password(url)
                    .is_some_and(|(stripped, _)| stripped == without)
            {
                *url = without;
                changed = true;
            }
        }
        if changed {
            self.save_profiles(profiles);
        }
    }
}
