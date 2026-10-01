// SPDX-License-Identifier: GPL-3.0-only

//! The wizard's effects: the steps' own requests, the destination checks, the
//! folder browser, and what happens when it finishes.

use super::super::*;

impl App {
    pub(in crate::app) fn run_wizard_effects(
        &mut self,
        effects: Vec<wizard::Effect>,
    ) -> Task<Message> {
        let session = self.wizard_session;
        let mut tasks = Vec::new();
        for effect in effects {
            tasks.push(match effect {
                wizard::Effect::PickFolders { excludes } => {
                    let title = if excludes {
                        fl!("wizard-pick-excludes")
                    } else {
                        fl!("wizard-pick-sources")
                    };
                    Task::perform(tasks::pick_folders(title), move |paths| {
                        app(Message::Wizard(
                            session,
                            if excludes {
                                wizard::Message::ExcludesChosen(paths)
                            } else {
                                wizard::Message::SourcesChosen(paths)
                            },
                        ))
                    })
                }
                wizard::Effect::Place(effect) => self.run_place_effect(effect),
                wizard::Effect::Browse(effect) => self.run_browse_effect(effect),
                wizard::Effect::Estimate {
                    generation,
                    request,
                    exclude_folders,
                    cancel,
                } => Task::run(
                    tasks::estimate(request, exclude_folders, cancel),
                    move |event| {
                        app(Message::Wizard(
                            session,
                            wizard::Message::Estimate(generation, event),
                        ))
                    },
                ),
                wizard::Effect::Finish(finish) => {
                    self.wizard_remember = finish.remember && finish.secret.is_some();
                    Task::perform(
                        tasks::finish(finish.mode, finish.profile, finish.secret),
                        move |result| app(Message::WizardFinished(session, Box::new(result))),
                    )
                }
                wizard::Effect::Close => {
                    self.wizard = None;
                    self.wizard_session += 1;
                    Task::none()
                }
                wizard::Effect::ConfirmCancel => {
                    self.dialogs.open(Dialog::WizardCancel);
                    Task::none()
                }
            });
        }
        Task::batch(tasks)
    }

    pub(in crate::app) fn run_place_effect(&mut self, effect: place::Effect) -> Task<Message> {
        let session = self.wizard_session;
        let to_wizard = move |message: place::Message| {
            app(Message::Wizard(session, wizard::Message::Place(message)))
        };
        match effect {
            place::Effect::PickFolder => {
                Task::perform(tasks::pick_folder(fl!("select-repo-folder")), move |path| {
                    path.map_or(app(Message::Noop), |path| {
                        to_wizard(place::Message::FolderChosen(path))
                    })
                })
            }
            place::Effect::ListDrives => Task::perform(
                tasks::blocking(|| {
                    crate::drives::mounted_drives()
                        .map_err(|err| EngineError::new(engine::ErrorKind::Io, err))
                }),
                move |drives| {
                    to_wizard(match drives {
                        Ok(drives) => place::Message::DrivesListed(drives),
                        Err(_) => place::Message::DrivesUnreadable,
                    })
                },
            ),
            place::Effect::CheckRclone => Task::perform(
                tasks::blocking(|| Ok(engine::rclone::available())),
                move |available| {
                    to_wizard(place::Message::RcloneChecked(available.unwrap_or(false)))
                },
            ),
            place::Effect::ListRemotes => Task::perform(
                tasks::blocking(engine::rclone::user_remotes),
                move |remotes| to_wizard(place::Message::RemotesListed(remotes)),
            ),
            place::Effect::SignIn { name, credentials } => {
                // Easy to miss as a line of page text below a button that
                // just disappeared: a modal makes "go to your browser now"
                // impossible to scroll past. Cleared again once sign-in
                // finishes, successfully or not; the page itself already
                // shows that outcome.
                self.dialogs.open(Dialog::SigningIn);
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                self.sign_in_cancel = Some(cancel.clone());
                Task::perform(
                    tasks::blocking(move || {
                        let config = engine::rclone::config_path();
                        let params = ["scope=drive"];
                        let credentials = credentials
                            .as_ref()
                            .map(|(id, secret)| (id.as_str(), secret.as_str()));
                        engine::rclone::sign_in(
                            &config,
                            &name,
                            "drive",
                            &params,
                            credentials,
                            &cancel,
                        )
                        .map(|()| name)
                    }),
                    move |result| to_wizard(place::Message::SignedIn(result)),
                )
            }
            place::Effect::CopyRemote { index, from, to } => Task::perform(
                tasks::blocking(move || {
                    let config = engine::rclone::config_path();
                    engine::rclone::copy_user_remote(&config, &from, &to).map(|()| to)
                }),
                move |result| to_wizard(place::Message::RemoteCopied(index, result)),
            ),
            place::Effect::Probe(destination) => {
                let probed = destination.clone();
                Task::perform(tasks::probe(destination), move |result| {
                    to_wizard(place::Message::Probed(probed.clone(), result))
                })
            }
        }
    }

    pub(in crate::app) fn run_browse_effect(
        &mut self,
        effect: wizard::browse::Effect,
    ) -> Task<Message> {
        let session = self.wizard_session;
        match effect {
            wizard::browse::Effect::List(dir, cancel) => {
                Task::run(tasks::browse_folder(dir, cancel), move |message| {
                    app(Message::Wizard(session, wizard::Message::Browse(message)))
                })
            }
            // Applied directly in `Wizard::update`, not here: see its own
            // comment for why.
            wizard::browse::Effect::SetExcluded(..) => Task::none(),
        }
    }

    pub(in crate::app) fn on_wizard_finished(
        &mut self,
        result: Result<tasks::Finished, EngineError>,
    ) -> Task<Message> {
        let remember = std::mem::take(&mut self.wizard_remember);
        // An edit is applied to the backup as it is now, not to the copy
        // the wizard opened with: see `Wizard::apply_to`. `Err(())` means
        // the backup was removed while the wizard was open.
        let edited = match (&result, &self.wizard) {
            (Ok(finished), Some(wizard)) if finished.mode.edits() => Some(
                self.config
                    .profile(&finished.profile.id)
                    .cloned()
                    .map(|mut current| {
                        wizard.apply_to(&mut current);
                        current
                    })
                    .ok_or(()),
            ),
            _ => None,
        };
        let outcome = result.as_ref().map(drop).map_err(Clone::clone);
        let close = match self.wizard.as_mut() {
            Some(wizard) => {
                let effects = wizard.update(wizard::Message::Finished(outcome));
                self.run_wizard_effects(effects)
            }
            None => Task::none(),
        };
        let finished = match result {
            Ok(finished) => finished,
            Err(err) => {
                self.show_error(&fl!("create-repo-failed"), &err);
                return close;
            }
        };

        let profile = match edited {
            Some(Ok(current)) => current,
            Some(Err(())) => {
                self.show_error(
                    &fl!("edit-backup-removed"),
                    &EngineError::new(engine::ErrorKind::NotFound, finished.profile.name.clone()),
                );
                // The wizard has closed; its sidebar row must go too.
                self.rebuild_nav(None);
                self.go_home();
                return close;
            }
            None => finished.profile,
        };
        let id = profile.id.clone();
        if !self.upsert_profile(profile.clone()) {
            // Already reported by `save_profiles` itself; opening the
            // repository this wizard just created (or reached) is fine on
            // its own, but nothing downstream of "this profile now exists"
            // — its schedule, or a first backup starting on a repository
            // Stellarshot cannot actually find again next launch — should
            // run against a save that never happened.
            self.rebuild_nav(None);
            self.go_home();
            return close;
        }
        self.rebuild_nav(Some(&id));
        let scheduled = Self::apply_schedule(profile.clone());
        // Only now that the repository opened with this password.
        let remember_task = match (&finished.secret, remember) {
            (Some(secret), true) => {
                App::remember_task(id.clone(), profile.name.clone(), secret.clone())
            }
            _ => Task::none(),
        };

        let mut effects = Vec::new();
        if let Some(secret) = finished.secret {
            let state = self.pages.entry(id.clone()).or_default();
            state.update(
                profile::Message::Opened(secret, Ok(finished.snapshots)),
                &profile,
            );
            if finished.mode == Mode::Create {
                effects = state.back_up(&profile);
            }
        }
        Task::batch([
            close,
            scheduled,
            remember_task,
            self.secure_rest_passwords(),
            self.run_profile_effects(&id, effects),
        ])
    }
}
