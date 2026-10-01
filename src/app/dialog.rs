// SPDX-License-Identifier: GPL-3.0-only

//! The modal dialogs: which one is showing and which wait behind it, what
//! each asks, and how each is drawn. What confirming one *does* stays in
//! `App::on_dialog`, which owns everything they act on.

use cosmic::widget;
use cosmic::{Element, theme};

use super::Message;
use super::child;
use super::config::StellarshotConfig;
use crate::engine::EngineError;
use crate::fl;

/// The command field in the password source dialog, focused as soon as the
/// dialog opens.
pub(super) fn password_command_input_id() -> widget::Id {
    widget::Id::new("password-command-input")
}

/// The first field in the change password dialog, focused as soon as the
/// dialog opens.
pub(super) fn new_password_input_id() -> widget::Id {
    widget::Id::new("new-password-input")
}

/// The "type the name to confirm" field in the "Delete everything" dialog,
/// focused as soon as the dialog opens rather than left for a mouse click or
/// a Tab press to reach.
pub(super) fn delete_all_input_id() -> widget::Id {
    widget::Id::new("delete-all-name")
}

/// A modal dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Dialog {
    /// A localized explanation of something that failed.
    Error(String),
    /// Something finished, and here is what happened.
    Info(String, String),
    /// A browser sign-in is under way. Shown as a modal, since "go to your
    /// browser now" is easy to miss as page text; it is its own kind so the
    /// sign-in finishing closes only this, never a confirmation that is
    /// showing by then.
    SigningIn,
    /// Forget a profile; its data stays.
    Remove { id: String, name: String },
    /// Delete a profile's repository and everything in it.
    DeleteAll {
        id: String,
        name: String,
        typed: String,
        busy: bool,
    },
    /// Cancel was pressed in the wizard: keep the draft to finish later, or
    /// discard it.
    WizardCancel,
    /// Quit was chosen while a backup or other write was in progress
    /// somewhere. `--run` children survive their parent exiting, by
    /// design, so confirming here only asks whether to stop watching.
    Quit,
    /// The trash icon next to one snapshot was pressed. `label` is its
    /// time, already formatted (see `profile::Effect::ConfirmDeleteSnapshot`).
    DeleteSnapshot {
        id: String,
        snapshot: String,
        label: String,
    },
    /// Editing a backup's `password_command`: `text` is the field as typed.
    PasswordCommand { id: String, text: String },
    /// Changing a backup's password: it must already be unlocked, since
    /// changing it needs the repository open.
    ChangePassword {
        id: String,
        password: String,
        confirm: String,
        busy: bool,
    },
}

impl Dialog {
    /// Deleting everything needs the profile's name typed exactly: not
    /// trimmed, not case-folded. A slip of the finger must not qualify.
    pub fn can_confirm(&self) -> bool {
        match self {
            Self::DeleteAll {
                name, typed, busy, ..
            } => !busy && typed == name,
            Self::ChangePassword {
                password,
                confirm,
                busy,
                ..
            } => !busy && !password.is_empty() && password == confirm,
            _ => true,
        }
    }
}

/// The dialog on screen, and the ones waiting behind it.
///
/// Background results (a finished check, a failed schedule, an import) arrive
/// whenever they like, and with one slot each used to overwrite whatever the
/// user was looking at, including a pending Remove or Delete confirmation.
/// Those now wait their turn ([`Dialogs::notify`]); a dialog the user asked
/// for ([`Dialogs::open`]) goes in front and puts the current one back.
#[derive(Default, Debug)]
pub struct Dialogs {
    front: Option<Dialog>,
    queued: std::collections::VecDeque<Dialog>,
}

impl Dialogs {
    /// The dialog to show.
    pub fn front(&self) -> Option<&Dialog> {
        self.front.as_ref()
    }

    pub fn front_mut(&mut self) -> Option<&mut Dialog> {
        self.front.as_mut()
    }

    /// Show `dialog` now, for something the user just asked for. One already
    /// showing comes back after it.
    pub fn open(&mut self, dialog: Dialog) {
        if let Some(previous) = self.front.replace(dialog) {
            self.queued.push_front(previous);
        }
    }

    /// Show `dialog` once nothing else is showing, for something that
    /// happened in the background.
    pub fn notify(&mut self, dialog: Dialog) {
        if self.front.is_none() {
            self.front = Some(dialog);
        } else {
            self.queued.push_back(dialog);
        }
    }

    /// Dismiss the dialog on screen; the next waiting one takes its place.
    /// Replace the dialog on screen with a changed version of itself (a
    /// button now busy, a field cleared): nothing is put back.
    pub fn update_front(&mut self, dialog: Dialog) {
        self.front = Some(dialog);
    }

    pub fn close(&mut self) {
        self.front = self.queued.pop_front();
    }

    /// Close the first dialog `is` accepts, wherever it is: on screen (the
    /// next waiting one then takes its place) or waiting behind another. For
    /// an operation finishing: its own dialog may have been pushed back by
    /// one the user opened meanwhile (Quit), and must still go, or it would
    /// reappear busy with nothing left to end it. Anything else showing is
    /// left alone.
    pub fn close_where(&mut self, is: impl Fn(&Dialog) -> bool) {
        if self.front.as_ref().is_some_and(&is) {
            self.close();
        } else if let Some(index) = self.queued.iter().position(&is) {
            self.queued.remove(index);
        }
    }

    /// Whether any dialog, on screen or waiting, is one `is` accepts.
    pub fn any(&self, is: impl Fn(&Dialog) -> bool) -> bool {
        self.front.iter().chain(self.queued.iter()).any(is)
    }
}

#[derive(Clone, Debug)]
pub enum DialogMessage {
    Close,
    /// Stop a cloud sign-in that is waiting on the browser.
    CancelSignIn,
    Confirm,
    Typed(String),
    Deleted(String, Result<(), EngineError>),
    /// Show an error from a background task.
    Failed(String, EngineError),
    /// Keep the wizard's draft, only hide it: `Dialog::WizardCancel`'s own
    /// two actions, kept apart from `Confirm`/`Close` since neither means
    /// "just dismiss" here.
    FinishWizardLater,
    DiscardWizard,
    NewPassword(String),
    ConfirmPassword(String),
    /// One line from the `change-password` child; see [`profile::Message::Pinned`]
    /// for why the raw event, not just the outcome, is threaded through.
    PasswordChanged(String, child::ChildEvent),
    /// The repository password changed. `Some` when the keyring entry that
    /// remembered the old one could not be replaced with the new one.
    KeyringUpdateFailed(Option<String>),
    /// A backup or unlock asked to remember its password. `Some` when the
    /// keyring refused it: the repository itself is unaffected, but a
    /// scheduled run will have no password to use later.
    PasswordNotRemembered(Option<String>),
}

/// The modal for `dialog`, as the window shows it.
pub(super) fn view<'a>(dialog: &'a Dialog, config: &StellarshotConfig) -> Element<'a, Message> {
    let confirm = dialog
        .can_confirm()
        .then_some(Message::Dialog(DialogMessage::Confirm));
    let cancel =
        widget::button::standard(fl!("cancel")).on_press(Message::Dialog(DialogMessage::Close));
    let built = match dialog {
        Dialog::Error(message) => widget::dialog()
            .title(fl!("error-title"))
            .body(message.as_str())
            .primary_action(
                widget::button::suggested(fl!("ok"))
                    .on_press(Message::Dialog(DialogMessage::Close)),
            ),
        Dialog::Info(title, message) => widget::dialog()
            .title(title.as_str())
            .body(message.as_str())
            .primary_action(
                widget::button::suggested(fl!("ok"))
                    .on_press(Message::Dialog(DialogMessage::Close)),
            ),
        Dialog::SigningIn => widget::dialog()
            .title(fl!("place-signing-in-title"))
            .body(fl!("place-signing-in-body"))
            .primary_action(
                widget::button::standard(fl!("place-sign-in-cancel"))
                    .on_press(Message::Dialog(DialogMessage::CancelSignIn)),
            ),
        Dialog::Remove { name, .. } => widget::dialog()
            .title(fl!("remove-title", name = name.clone()))
            .body(fl!("remove-body"))
            .primary_action(widget::button::destructive(fl!("remove")).on_press_maybe(confirm))
            .secondary_action(cancel),
        Dialog::DeleteAll { name, typed, .. } => widget::dialog()
            .title(fl!("delete-title", name = name.clone()))
            .body(fl!("delete-body", name = name.clone()))
            .control(
                widget::text_input(name.as_str(), typed.as_str())
                    .id(delete_all_input_id())
                    .on_input(|text| Message::Dialog(DialogMessage::Typed(text)))
                    .on_submit(|_| Message::Dialog(DialogMessage::Confirm)),
            )
            .primary_action(widget::button::destructive(fl!("delete")).on_press_maybe(confirm))
            .secondary_action(cancel),
        Dialog::WizardCancel => widget::dialog()
            .title(fl!("wizard-cancel-title"))
            .body(fl!("wizard-cancel-body"))
            .primary_action(
                widget::button::suggested(fl!("wizard-finish-later"))
                    .on_press(Message::Dialog(DialogMessage::FinishWizardLater)),
            )
            .secondary_action(
                widget::button::destructive(fl!("wizard-discard"))
                    .on_press(Message::Dialog(DialogMessage::DiscardWizard)),
            )
            .tertiary_action(
                widget::button::standard(fl!("wizard-keep-editing"))
                    .on_press(Message::Dialog(DialogMessage::Close)),
            ),
        Dialog::Quit => widget::dialog()
            .title(fl!("quit-confirm-title"))
            .body(fl!("quit-confirm-body"))
            .primary_action(widget::button::destructive(fl!("quit")).on_press_maybe(confirm))
            .secondary_action(cancel),
        Dialog::DeleteSnapshot { label, .. } => widget::dialog()
            .title(fl!("delete-snapshot-title"))
            .body(fl!("delete-snapshot-body", time = label.clone()))
            .primary_action(widget::button::destructive(fl!("delete")).on_press_maybe(confirm))
            .secondary_action(cancel),
        Dialog::PasswordCommand { text, .. } => widget::dialog()
            .title(fl!("password-source-title"))
            .body(fl!("password-source-body"))
            .control(
                widget::text_input(fl!("password-source-placeholder"), text.as_str())
                    .id(password_command_input_id())
                    .on_input(|text| Message::Dialog(DialogMessage::Typed(text)))
                    .on_submit(|_| Message::Dialog(DialogMessage::Confirm)),
            )
            .primary_action(widget::button::suggested(fl!("save")).on_press_maybe(confirm))
            .secondary_action(cancel),
        Dialog::ChangePassword {
            id,
            password,
            confirm: confirm_password,
            busy,
        } => {
            let confirm_action = confirm;
            let mismatch = !confirm_password.is_empty() && password != confirm_password;
            let uses_password_command = config
                .profile(id)
                .is_some_and(|profile| !profile.password_command.is_empty());
            let mut fields = widget::column::with_capacity(3)
                .spacing(theme::active().cosmic().spacing.space_xs)
                .push(
                    widget::secure_input(fl!("password"), password.as_str(), None, true)
                        .id(new_password_input_id())
                        .label(fl!("password"))
                        .on_input(|text| Message::Dialog(DialogMessage::NewPassword(text))),
                )
                .push(
                    widget::secure_input(
                        fl!("wizard-confirm"),
                        confirm_password.as_str(),
                        None,
                        true,
                    )
                    .label(fl!("wizard-confirm"))
                    .on_input(|text| Message::Dialog(DialogMessage::ConfirmPassword(text)))
                    .on_submit(|_| Message::Dialog(DialogMessage::Confirm)),
                );
            if mismatch {
                fields = fields.push(widget::text::caption(fl!("wizard-mismatch")));
            }
            let mut body = fl!("change-password-body");
            if uses_password_command {
                body = format!("{body}\n\n{}", fl!("change-password-command-note"));
            }
            widget::dialog()
                .title(fl!("change-password-title"))
                .body(body)
                .control(fields)
                .primary_action(
                    widget::button::suggested(fl!("save")).on_press_maybe(confirm_action),
                )
                // Not while the change is running: the repository's
                // password changes whether or not this closes, and the
                // new one is applied when it finishes.
                .secondary_action(
                    widget::button::standard(fl!("cancel"))
                        .on_press_maybe((!*busy).then_some(Message::Dialog(DialogMessage::Close))),
                )
        }
    };
    built.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remove() -> Dialog {
        Dialog::Remove {
            id: "a".into(),
            name: "Home".into(),
        }
    }

    #[test]
    fn a_background_error_waits_behind_a_pending_confirmation() {
        let mut dialogs = Dialogs::default();
        dialogs.open(remove());

        dialogs.notify(Dialog::Error("the backup failed".into()));

        assert!(
            matches!(dialogs.front(), Some(Dialog::Remove { .. })),
            "the confirmation is still what the user sees"
        );
        dialogs.close();
        assert!(matches!(dialogs.front(), Some(Dialog::Error(_))));
        dialogs.close();
        assert!(dialogs.front().is_none());
    }

    #[test]
    fn a_dialog_the_user_asks_for_goes_in_front_and_the_old_one_returns() {
        let mut dialogs = Dialogs::default();
        dialogs.notify(Dialog::Error("earlier".into()));

        dialogs.open(Dialog::Quit);

        assert!(matches!(dialogs.front(), Some(Dialog::Quit)));
        dialogs.close();
        assert!(matches!(dialogs.front(), Some(Dialog::Error(_))));
    }

    #[test]
    fn finishing_an_operation_closes_only_its_own_dialog() {
        let mut dialogs = Dialogs::default();
        dialogs.open(remove());

        dialogs.close_where(|dialog| matches!(dialog, Dialog::SigningIn));
        assert!(
            matches!(dialogs.front(), Some(Dialog::Remove { .. })),
            "a sign-in finishing must not dismiss an unrelated confirmation"
        );

        dialogs.open(Dialog::SigningIn);
        dialogs.close_where(|dialog| matches!(dialog, Dialog::SigningIn));
        assert!(matches!(dialogs.front(), Some(Dialog::Remove { .. })));
    }

    /// A busy dialog pushed back behind Quit must still close when its
    /// operation finishes, or it reappears with nothing left to end it.
    #[test]
    fn an_operation_closes_its_own_dialog_even_behind_another() {
        let mut dialogs = Dialogs::default();
        dialogs.open(Dialog::ChangePassword {
            id: "a".into(),
            password: "x".into(),
            confirm: "x".into(),
            busy: true,
        });
        dialogs.open(Dialog::Quit);
        assert!(dialogs.any(|d| matches!(d, Dialog::ChangePassword { busy: true, .. })));

        dialogs.close_where(|d| matches!(d, Dialog::ChangePassword { .. }));
        assert!(matches!(dialogs.front(), Some(Dialog::Quit)), "Quit stays");
        dialogs.close();
        assert!(dialogs.front().is_none(), "nothing busy comes back");
    }

    #[test]
    fn delete_requires_the_exact_name() {
        let dialog = |typed: &str| Dialog::DeleteAll {
            id: "x".into(),
            name: "Home".into(),
            typed: typed.into(),
            busy: false,
        };
        assert!(!dialog("").can_confirm());
        assert!(!dialog("home").can_confirm(), "case matters");
        assert!(!dialog("Home ").can_confirm(), "no trailing space");
        assert!(!dialog("Hom").can_confirm());
        assert!(dialog("Home").can_confirm());

        let busy = Dialog::DeleteAll {
            id: "x".into(),
            name: "Home".into(),
            typed: "Home".into(),
            busy: true,
        };
        assert!(!busy.can_confirm(), "not twice while the first delete runs");
    }

    #[test]
    fn other_dialogs_confirm_freely() {
        assert!(Dialog::Error("x".into()).can_confirm());
        assert!(
            Dialog::Remove {
                id: "x".into(),
                name: "Home".into()
            }
            .can_confirm()
        );
        assert!(Dialog::Quit.can_confirm());
    }
}
