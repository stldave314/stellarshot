// SPDX-License-Identifier: GPL-3.0-only

//! What confirming, closing and answering a dialog does: the dialogs
//! themselves are in `app::dialog`; this is what they act on.

use crate::app::{
    App, Dialog, DialogMessage, EngineError, Job, Message, Operation, Profile, RunnerEvent, Task,
    app, child, engine, event_log, record_event, tasks,
};
use crate::debug::{CONFIG, ENGINE};
use crate::{debug_log, fl};

impl App {
    pub(in crate::app) fn on_dialog(&mut self, message: DialogMessage) -> Task<Message> {
        match message {
            DialogMessage::CancelSignIn => {
                if let Some(cancel) = &self.sign_in_cancel {
                    cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                // Closed when the sign-in reports back (see `Message::Wizard`).
                Task::none()
            }
            DialogMessage::Close => {
                self.dialogs.close();
                Task::none()
            }
            DialogMessage::FinishWizardLater => {
                self.dialogs.close();
                self.go_home();
                Task::none()
            }
            DialogMessage::DiscardWizard => {
                self.dialogs.close();
                self.discard_wizard();
                Task::none()
            }
            DialogMessage::Typed(text) => {
                match self.dialogs.front_mut() {
                    Some(Dialog::DeleteAll { typed, .. }) => *typed = text,
                    Some(Dialog::PasswordCommand { text: field, .. }) => *field = text,
                    _ => {}
                }
                Task::none()
            }
            DialogMessage::NewPassword(text) => {
                if let Some(Dialog::ChangePassword { password, .. }) = self.dialogs.front_mut() {
                    *password = text;
                }
                Task::none()
            }
            DialogMessage::ConfirmPassword(text) => {
                if let Some(Dialog::ChangePassword { confirm, .. }) = self.dialogs.front_mut() {
                    *confirm = text;
                }
                Task::none()
            }
            DialogMessage::Confirm => {
                let Some(dialog) = self.dialogs.front().cloned() else {
                    return Task::none();
                };
                if !dialog.can_confirm() {
                    return Task::none();
                }
                match dialog {
                    Dialog::Error(_) | Dialog::Info(..) | Dialog::SigningIn => {
                        self.dialogs.close();
                        Task::none()
                    }
                    Dialog::Remove { id, .. } => {
                        self.dialogs.close();
                        debug_log!(CONFIG, "removing profile {id}; its data stays");
                        self.remove_profile(&id)
                    }
                    Dialog::DeleteAll {
                        id, name, typed, ..
                    } => {
                        let Some(profile) = self.config.profile(&id).cloned() else {
                            return Task::none();
                        };
                        self.dialogs.update_front(Dialog::DeleteAll {
                            id: id.clone(),
                            name,
                            typed,
                            busy: true,
                        });
                        Task::perform(
                            tasks::blocking(move || delete_repository(&profile)),
                            move |result| {
                                app(Message::Dialog(DialogMessage::Deleted(id.clone(), result)))
                            },
                        )
                    }
                    // Its own two buttons send `FinishWizardLater` and
                    // `DiscardWizard` directly; `Confirm` never legitimately
                    // reaches it. Dismiss rather than do nothing silently.
                    Dialog::WizardCancel => {
                        self.dialogs.close();
                        Task::none()
                    }
                    Dialog::Quit => cosmic::iced::exit(),
                    Dialog::DeleteSnapshot { id, snapshot, .. } => {
                        self.dialogs.close();
                        let effects = self
                            .pages
                            .get_mut(&id)
                            .map(|page| page.delete_snapshot_confirmed(snapshot))
                            .unwrap_or_default();
                        self.run_profile_effects(&id, effects)
                    }
                    Dialog::PasswordCommand { id, text } => {
                        self.dialogs.close();
                        let Some(mut profile) = self.config.profile(&id).cloned() else {
                            return Task::none();
                        };
                        profile.password_command = text.trim().to_owned();
                        self.upsert_profile(profile);
                        Task::none()
                    }
                    Dialog::ChangePassword {
                        id,
                        password,
                        confirm,
                        ..
                    } => {
                        let (Some(profile), Some(secret)) = (
                            self.config.profile(&id).cloned(),
                            self.pages.get(&id).and_then(|page| page.secret().cloned()),
                        ) else {
                            return Task::none();
                        };
                        let repository = match profile.location() {
                            Ok(location) => location,
                            Err(err) => {
                                self.dialogs.close();
                                self.show_error(&fl!("change-password-failed"), &err);
                                return Task::none();
                            }
                        };
                        self.dialogs.update_front(Dialog::ChangePassword {
                            id: id.clone(),
                            password,
                            confirm: confirm.clone(),
                            busy: true,
                        });
                        self.pending_password_change =
                            Some((id.clone(), engine::Secret::new(confirm.clone())));
                        // Through the same `--run` child every other write
                        // goes through, not called in-process: it needs the
                        // cross-process write lock too, so it cannot race a
                        // scheduled backup also touching the repository's
                        // keys, and it needs the child's own diagnostic
                        // logging (see `runner::main`), which an in-process
                        // call never reached.
                        let job = Job {
                            new_password: Some(engine::Secret::new(confirm)),
                            ..Job::new(repository, secret)
                        };
                        Task::run(child::run(Operation::ChangePassword, job), move |event| {
                            app(Message::Dialog(DialogMessage::PasswordChanged(
                                id.clone(),
                                event,
                            )))
                        })
                    }
                }
            }
            DialogMessage::ScheduledRunStarted => {
                self.dialogs.notify(Dialog::Info(
                    fl!("run-as-scheduled-started-title"),
                    fl!("run-as-scheduled-started-body"),
                ));
                Task::none()
            }
            DialogMessage::Failed(context, error) => {
                self.show_error(&context, &error);
                Task::none()
            }
            DialogMessage::Deleted(id, result) => match result {
                Ok(()) => {
                    self.dialogs
                        .close_where(|dialog| matches!(dialog, Dialog::DeleteAll { .. }));
                    debug_log!(ENGINE, "deleted the repository of profile {id}");
                    self.remove_profile(&id)
                }
                Err(err) => {
                    // The busy dialog would otherwise stay in front of the
                    // error, which now waits its turn.
                    self.dialogs
                        .close_where(|dialog| matches!(dialog, Dialog::DeleteAll { .. }));
                    self.show_error(&fl!("delete-repo-failed"), &err);
                    Task::none()
                }
            },
            DialogMessage::PasswordChanged(id, event) => match event {
                child::ChildEvent::Event(RunnerEvent::Done { .. }) => {
                    let Some((pending_id, new_password)) = self.pending_password_change.take()
                    else {
                        return Task::none();
                    };
                    if pending_id != id {
                        return Task::none();
                    }
                    // Only this dialog: anything else showing now (an error
                    // from another backup, say) is not this operation's to
                    // close.
                    self.dialogs
                        .close_where(|dialog| matches!(dialog, Dialog::ChangePassword { .. }));
                    if let Some(page) = self.pages.get_mut(&id) {
                        page.set_secret(new_password.clone());
                    }
                    debug_log!(ENGINE, "changed the password of profile {id}");
                    let logged = record_event(id.clone(), event_log::EventKind::PasswordChanged);
                    let Some(profile) = self.config.profile(&id).cloned() else {
                        return logged;
                    };
                    // The repository itself is already changed at this
                    // point; a keyring failure here does not undo that, so
                    // it is reported separately rather than as this whole
                    // operation having failed.
                    let keyring = Task::perform(
                        async move {
                            if crate::keyring::load(&profile.id).await.is_some() {
                                crate::keyring::store(&profile.id, &profile.name, &new_password)
                                    .await
                            } else {
                                Ok(())
                            }
                        },
                        |result| {
                            app(Message::Dialog(DialogMessage::KeyringUpdateFailed(
                                result.err(),
                            )))
                        },
                    );
                    Task::batch([logged, keyring])
                }
                child::ChildEvent::Event(RunnerEvent::Error { error })
                | child::ChildEvent::Ended(error) => {
                    self.pending_password_change = None;
                    self.dialogs
                        .close_where(|dialog| matches!(dialog, Dialog::ChangePassword { .. }));
                    self.show_error(&fl!("change-password-failed"), &error);
                    Task::none()
                }
                child::ChildEvent::Started(_)
                | child::ChildEvent::Event(RunnerEvent::Progress { .. }) => Task::none(),
            },
            DialogMessage::KeyringUpdateFailed(None) => Task::none(),
            DialogMessage::KeyringUpdateFailed(Some(detail)) => {
                self.show_error(
                    &fl!("change-password-title"),
                    &EngineError::new(engine::ErrorKind::KeyringUnavailable, detail),
                );
                Task::none()
            }
            DialogMessage::PasswordNotRemembered(None) => Task::none(),
            DialogMessage::PasswordNotRemembered(Some(detail)) => {
                self.dialogs.notify(Dialog::Error(fl!(
                    "error-password-not-saved",
                    details = detail
                )));
                Task::none()
            }
        }
    }
}

/// Delete a profile's repository: under its lock, so nothing is writing to
/// it, and only the entries the repository format owns.
fn delete_repository(profile: &Profile) -> Result<(), EngineError> {
    let location = profile.location()?;
    let _lock = engine::lock::acquire(&location)?;
    engine::delete_repository(&location)
}
