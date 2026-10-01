// SPDX-License-Identifier: GPL-3.0-only

//! A backup page's effects.

use super::super::*;

impl App {
    pub(in crate::app) fn run_profile_effects(
        &mut self,
        id: &str,
        effects: Vec<profile::Effect>,
    ) -> Task<Message> {
        let Some(profile) = self.config.profile(id).cloned() else {
            return Task::none();
        };
        let mut tasks = Vec::new();
        for effect in effects {
            let id = id.to_owned();
            let task = match effect {
                profile::Effect::LoadKeyring => {
                    let profile = profile.clone();
                    Task::perform(async move { profile.password().await }, move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::KeyringLoaded(result),
                        ))
                    })
                }
                profile::Effect::FocusUnlock => {
                    cosmic::iced::widget::operation::focus(profile::unlock_input_id())
                }
                profile::Effect::Open { secret, remember } => {
                    let used = secret.clone();
                    let (profile_id, name) = (profile.id.clone(), profile.name.clone());
                    // Remembered only once the password has actually opened
                    // the repository: storing it alongside the attempt saved
                    // a mistyped one too, and could replace a good one
                    // already there when the open failed for another reason
                    // (an unplugged drive).
                    cosmic::iced::Task::perform(
                        tasks::open(profile.clone(), secret.clone()),
                        |result| result,
                    )
                    .then(move |result| {
                        let remember_task = if remember && result.is_ok() {
                            App::remember_task(profile_id.clone(), name.clone(), used.clone())
                        } else {
                            Task::none()
                        };
                        let opened = Task::done(app(Message::Profile(
                            id.clone(),
                            profile::Message::Opened(used.clone(), result),
                        )));
                        Task::batch([remember_task, opened])
                    })
                }
                profile::Effect::Fetch(secret) => {
                    Task::perform(tasks::open(profile.clone(), secret), move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::SnapshotsLoaded(result),
                        ))
                    })
                }
                profile::Effect::BackUp(secret) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        // An unplugged drive: end the backup before it starts,
                        // through the same path a failed backup takes.
                        Err(err) => {
                            tasks.push(self.end_before_start(
                                &id,
                                &profile,
                                profile::Message::Backup,
                                err,
                            ));
                            continue;
                        }
                    };
                    let job = Job {
                        request: Some(profile.backup_request(&self.config.global_exclude_patterns)),
                        hooks: profile.hooks.clone(),
                        ..Job::new(repository, secret)
                    };
                    Task::run(child::run(Operation::Backup, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Backup(event),
                        ))
                    })
                }
                profile::Effect::DeleteSnapshots(secret, ids) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            tasks.push(self.end_before_start(
                                &id,
                                &profile,
                                profile::Message::SnapshotsDeleted,
                                err,
                            ));
                            continue;
                        }
                    };
                    let job = Job {
                        ids,
                        ..Job::new(repository, secret)
                    };
                    Task::run(child::run(Operation::DeleteSnapshots, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::SnapshotsDeleted(event),
                        ))
                    })
                }
                profile::Effect::ConfirmDeleteSnapshot {
                    id: snapshot,
                    label,
                } => {
                    self.dialogs.open(Dialog::DeleteSnapshot {
                        id: id.clone(),
                        snapshot,
                        label,
                    });
                    Task::none()
                }
                profile::Effect::SetPinned(secret, snapshot_id, pinned) => {
                    let repository = match profile.location() {
                        Ok(location) => location,
                        Err(err) => {
                            tasks.push(self.end_before_start(
                                &id,
                                &profile,
                                profile::Message::Pinned,
                                err,
                            ));
                            continue;
                        }
                    };
                    let job = Job {
                        ids: vec![snapshot_id],
                        pinned: Some(pinned),
                        ..Job::new(repository, secret)
                    };
                    Task::run(child::run(Operation::SetPinned, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Pinned(event),
                        ))
                    })
                }
                profile::Effect::ShowError(context, error) => {
                    self.show_error(&context, &error);
                    Task::none()
                }
                profile::Effect::RecordSuccess(time) => {
                    let mut updated = profile.clone();
                    updated.last_success = Some(time);
                    self.upsert_profile(updated);
                    Task::none()
                }
                // A restore page already open (perhaps restoring right now)
                // is shown as it is, never replaced: its running restore
                // would lose the page that reports and records it.
                profile::Effect::OpenRestore(_) if self.restore.is_some() => Task::none(),
                profile::Effect::OpenRestore(secret) => {
                    let root = profile
                        .sources
                        .first()
                        .cloned()
                        .unwrap_or_else(|| PathBuf::from("/"));
                    let (page, effects) = RestorePage::new(profile.id.clone(), root);
                    self.restore_session += 1;
                    self.restore = Some((page, secret));
                    self.run_restore_effects(effects)
                }
                profile::Effect::Edit => self.edit_wizard(Wizard::edit, &profile),
                profile::Effect::EditSchedule => self.edit_wizard(Wizard::schedule, &profile),
                profile::Effect::EditHooks => self.edit_wizard(Wizard::hooks, &profile),
                profile::Effect::EditPasswordCommand => {
                    self.dialogs.open(Dialog::PasswordCommand {
                        id: id.clone(),
                        text: profile.password_command.clone(),
                    });
                    cosmic::iced::widget::operation::focus(password_command_input_id())
                }
                profile::Effect::ChangePassword => {
                    self.dialogs.open(Dialog::ChangePassword {
                        id: id.clone(),
                        password: String::new(),
                        confirm: String::new(),
                        busy: false,
                    });
                    cosmic::iced::widget::operation::focus(new_password_input_id())
                }
                profile::Effect::LogEvent(kind) => record_event(id.clone(), kind),
                profile::Effect::FetchHistory => {
                    Task::perform(tasks::history(id.clone()), move |history| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::HistoryLoaded(history),
                        ))
                    })
                }
                profile::Effect::FetchStatistics(secret) => {
                    Task::perform(tasks::statistics(profile.clone(), secret), move |result| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::StatisticsLoaded(result),
                        ))
                    })
                }
                profile::Effect::EstimateSize(cancel) => {
                    let request = profile.backup_request(&self.config.global_exclude_patterns);
                    Task::run(tasks::estimate_size(request, cancel), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Estimate(event),
                        ))
                    })
                }
                profile::Effect::FetchNextRun => {
                    let profile_id = id.clone();
                    Task::perform(
                        async move { schedule::next_run(&profile_id).await },
                        move |next_run| {
                            app(Message::Profile(
                                id.clone(),
                                profile::Message::NextRunLoaded(next_run),
                            ))
                        },
                    )
                }
                profile::Effect::Check(secret) => {
                    let job = match profile.location() {
                        Ok(location) => Job::new(location, secret),
                        Err(err) => {
                            tasks.push(self.end_before_start(
                                &id,
                                &profile,
                                profile::Message::Checked,
                                err,
                            ));
                            continue;
                        }
                    };
                    Task::run(child::run(Operation::Check, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::Checked(event),
                        ))
                    })
                }
                profile::Effect::RecordCheck(result) => {
                    // A check that could not run (an unplugged drive, a
                    // cancel) says nothing about damage and is not recorded.
                    let damaged = match &result {
                        Ok(()) => false,
                        Err(err) if err.kind == engine::ErrorKind::RepositoryDamaged => true,
                        Err(err) => {
                            self.show_error(&fl!("check-failed"), err);
                            continue;
                        }
                    };
                    let now = format::now();
                    let recorded = self.record_run(&id, move |run| {
                        run.last_check = Some(now);
                        run.damaged = damaged;
                        if !damaged
                            && run
                                .failure
                                .as_ref()
                                .is_some_and(|f| f.stage == run_state::Stage::Check)
                        {
                            run.failure = None;
                        }
                    });
                    match result {
                        Ok(()) => {
                            self.dialogs.notify(Dialog::Info(
                                fl!("check-passed-title"),
                                fl!("check-passed-body"),
                            ));
                        }
                        Err(err) => self.show_error(&fl!("check-failed"), &err),
                    }
                    recorded
                }
                profile::Effect::CleanUp(secret) => {
                    let job = match profile.location() {
                        Ok(location) => Job {
                            keep: profile.retention.keep_rules(),
                            prune: !self.runs.get(&id).is_some_and(|run| run.damaged),
                            profile_tag: engine::profile_tag(&profile.id),
                            profile_sources: profile.sources.clone(),
                            ..Job::new(location, secret)
                        },
                        Err(err) => {
                            tasks.push(self.end_before_start(
                                &id,
                                &profile,
                                profile::Message::CleanedUp,
                                err,
                            ));
                            continue;
                        }
                    };
                    Task::run(child::run(Operation::Maintain, job), move |event| {
                        app(Message::Profile(
                            id.clone(),
                            profile::Message::CleanedUp(event),
                        ))
                    })
                }
                profile::Effect::CleanedUp { forgotten, freed } => {
                    let recorded = self.record_run(&id, move |run| {
                        if run
                            .failure
                            .as_ref()
                            .is_some_and(|f| f.stage == run_state::Stage::Cleanup)
                        {
                            run.failure = None;
                        }
                        run.total_freed += freed;
                    });
                    self.dialogs.notify(Dialog::Info(
                        fl!("clean-up-done-title"),
                        fl!(
                            "clean-up-done-body",
                            count = (forgotten as i64),
                            size = format::bytes(freed)
                        ),
                    ));
                    recorded
                }
                profile::Effect::Remove => {
                    self.dialogs.open(Dialog::Remove {
                        id,
                        name: profile.name.clone(),
                    });
                    Task::none()
                }
                profile::Effect::DeleteAll => {
                    self.dialogs.open(Dialog::DeleteAll {
                        id,
                        name: profile.name.clone(),
                        typed: String::new(),
                        busy: false,
                    });
                    cosmic::iced::widget::operation::focus(delete_all_input_id())
                }
            };
            tasks.push(task);
        }
        Task::batch(tasks)
    }

    /// A job could not even start because its location failed (an unplugged
    /// drive, say): end it through the same path a failed run takes, so the
    /// page shows the same thing.
    fn end_before_start(
        &mut self,
        id: &str,
        profile: &Profile,
        wrap: fn(child::ChildEvent) -> profile::Message,
        error: EngineError,
    ) -> Task<Message> {
        let ended = wrap(child::ChildEvent::Ended(error));
        let effects = self
            .pages
            .entry(id.to_owned())
            .or_default()
            .update(ended, profile);
        self.run_profile_effects(id, effects)
    }

    /// Open the wizard `make` builds for `profile`, or, if one is already in
    /// progress ("finish later" from elsewhere), resume that one rather than
    /// losing it to this edit.
    fn edit_wizard(
        &mut self,
        make: fn(&Profile) -> (Wizard, Vec<wizard::Effect>),
        profile: &Profile,
    ) -> Task<Message> {
        if self.wizard.is_none() {
            let (wizard, effects) = make(profile);
            self.start_wizard(wizard, effects)
        } else {
            self.select_wizard();
            Task::none()
        }
    }
}
