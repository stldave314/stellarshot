// SPDX-License-Identifier: GPL-3.0-only

//! The wizard's "where" step: a folder, a removable drive, an SSH server,
//! Google Drive, or one of the user's rclone remotes.
//!
//! Each kind collects what it needs and produces a [`Destination`]. Nothing is
//! accepted until the destination has been probed, and a probe only counts for
//! the destination it was run against, so editing a field after a check
//! always asks for a new one.

use std::path::PathBuf;
use std::time::Instant;

use cosmic::{Element, theme, widget};

use crate::app::format;
use crate::drives::Drive;
use crate::engine::{EngineError, Probe};
use crate::fl;
use crate::profile::Destination;

/// The kinds of place a backup can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Drive,
    Server,
    Google,
    Remote,
}

/// Default SSH port.
const SSH_PORT: u16 = 22;

/// The machine's name, used for default folder names so two computers
/// backing up to the same drive do not collide.
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "computer".to_owned())
}

/// The folder a new backup gets inside a drive or cloud account.
fn default_folder() -> String {
    format!("Stellarshot/{}", hostname())
}

#[derive(Debug, Clone)]
pub struct Place {
    pub kind: Kind,
    pub folder: Option<PathBuf>,
    pub drives: Vec<Drive>,
    pub drive: Option<usize>,
    pub drive_folder: String,
    pub host: String,
    pub user: String,
    pub port: String,
    pub server_path: String,
    /// The remote a Google sign-in created in Stellarshot's configuration.
    pub google_remote: Option<String>,
    pub signing_in: bool,
    /// A Google API client of the user's own, instead of the one Stellarshot
    /// signs in with by default. Both empty means the default.
    pub google_client_id: String,
    pub google_client_secret: String,
    /// The custom-credentials fields are shown.
    pub google_advanced: bool,
    pub cloud_path: String,
    pub user_remotes: Vec<String>,
    /// The chosen user remote, and the copy of it in Stellarshot's
    /// configuration once made.
    pub user_remote: Option<(usize, Option<String>)>,
    pub remote_path: String,
    pub rclone_available: Option<bool>,
    /// The destination being checked, and since when.
    checking: Option<(Destination, Instant)>,
    probe: Option<(Destination, Result<Probe, EngineError>)>,
    /// A problem to show under the form: a failed sign-in or copy.
    pub problem: Option<String>,
    /// Select the drive with this UUID once drives are listed (an import).
    pub preferred_drive: Option<String>,
    /// Pick the user remote with this name once remotes are listed (an import).
    pub preferred_remote: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Kind(Kind),
    ChooseFolder,
    FolderChosen(PathBuf),
    DrivesListed(Vec<Drive>),
    PickDrive(usize),
    DriveFolder(String),
    Host(String),
    User(String),
    Port(String),
    ServerPath(String),
    SignIn,
    SignedIn(Result<String, EngineError>),
    ToggleGoogleAdvanced,
    GoogleClientId(String),
    GoogleClientSecret(String),
    CloudPath(String),
    RemotesListed(Result<Vec<String>, EngineError>),
    PickRemote(usize),
    RemoteCopied(usize, Result<String, EngineError>),
    RemotePath(String),
    RcloneChecked(bool),
    Check,
    Probed(Destination, Result<Probe, EngineError>),
}

pub enum Effect {
    PickFolder,
    ListDrives,
    CheckRclone,
    ListRemotes,
    /// Sign in to Google Drive, creating the remote `name`. `credentials`
    /// is the user's own client ID and secret, if they gave both; `None`
    /// signs in with Stellarshot's own.
    SignIn {
        name: String,
        credentials: Option<(String, String)>,
    },
    /// Copy the user's remote `from` into Stellarshot's configuration as `to`.
    CopyRemote {
        index: usize,
        from: String,
        to: String,
    },
    Probe(Destination),
}

/// A name for a remote Stellarshot creates in its own configuration.
fn new_remote_name() -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    format!("stellarshot-{}", &id[..8])
}

impl Default for Place {
    fn default() -> Self {
        Self {
            kind: Kind::Folder,
            folder: None,
            drives: Vec::new(),
            drive: None,
            drive_folder: default_folder(),
            host: String::new(),
            user: String::new(),
            port: SSH_PORT.to_string(),
            server_path: String::new(),
            google_remote: None,
            signing_in: false,
            google_client_id: String::new(),
            google_client_secret: String::new(),
            google_advanced: false,
            cloud_path: default_folder(),
            user_remotes: Vec::new(),
            user_remote: None,
            remote_path: default_folder(),
            rclone_available: None,
            checking: None,
            probe: None,
            problem: None,
            preferred_drive: None,
            preferred_remote: None,
        }
    }
}

impl Place {
    /// What to look up when the step is shown.
    pub fn enter(&self) -> Vec<Effect> {
        vec![Effect::ListDrives, Effect::CheckRclone]
    }

    /// The destination the current fields describe, if they are complete.
    pub fn destination(&self) -> Option<Destination> {
        match self.kind {
            Kind::Folder => self.folder.clone().map(Destination::for_folder),
            Kind::Drive => {
                let drive = self.drives.get(self.drive?)?;
                let folder = self.drive_folder.trim().trim_matches('/');
                (!folder.is_empty()).then(|| Destination::Removable {
                    uuid: drive.uuid.clone(),
                    relative_path: PathBuf::from(folder),
                    label: drive.label.clone(),
                })
            }
            Kind::Server => {
                let host = self.host.trim();
                let path = self.server_path.trim();
                let port = self.port.trim().parse::<u16>().ok()?;
                (!host.is_empty() && !path.is_empty()).then(|| Destination::Sftp {
                    host: host.to_owned(),
                    user: self.user.trim().to_owned(),
                    port,
                    path: path.to_owned(),
                })
            }
            Kind::Google => {
                let remote = self.google_remote.clone()?;
                let path = self.cloud_path.trim().trim_matches('/');
                (!path.is_empty()).then(|| Destination::Rclone {
                    remote,
                    path: path.to_owned(),
                    provider: fl!("google-drive"),
                })
            }
            Kind::Remote => {
                let (index, Some(copy)) = self.user_remote.clone()? else {
                    return None;
                };
                let path = self.remote_path.trim().trim_matches('/');
                (!path.is_empty()).then(|| Destination::Rclone {
                    remote: copy,
                    path: path.to_owned(),
                    provider: self.user_remotes.get(index).cloned().unwrap_or_default(),
                })
            }
        }
    }

    /// The probe result, if it is for the destination as it is now.
    pub fn probe(&self) -> Option<&Result<Probe, EngineError>> {
        let current = self.destination()?;
        match &self.probe {
            Some((probed, result)) if *probed == current => Some(result),
            _ => None,
        }
    }

    /// When the check of the destination as it is now began, while it runs.
    pub fn checking_since(&self) -> Option<Instant> {
        let current = self.destination()?;
        match &self.checking {
            Some((checked, since)) if *checked == current => Some(*since),
            _ => None,
        }
    }

    /// A name for the backup, from what was chosen.
    pub fn suggested_name(&self) -> Option<String> {
        match self.kind {
            Kind::Folder => self
                .folder
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned()),
            Kind::Drive => self
                .drive
                .and_then(|index| self.drives.get(index))
                .map(|drive| drive.label.clone()),
            Kind::Server => (!self.host.trim().is_empty()).then(|| self.host.trim().to_owned()),
            Kind::Google => Some(fl!("google-drive")),
            Kind::Remote => self
                .user_remote
                .as_ref()
                .and_then(|(index, _)| self.user_remotes.get(*index).cloned()),
        }
    }

    /// Whether this kind needs rclone.
    fn needs_rclone(&self) -> bool {
        matches!(self.kind, Kind::Server | Kind::Google | Kind::Remote)
    }

    /// Check the destination as it is now, unless that is already under way.
    pub fn check(&mut self) -> Vec<Effect> {
        if self.checking_since().is_some() {
            return Vec::new();
        }
        match self.destination() {
            Some(destination) => {
                self.checking = Some((destination.clone(), Instant::now()));
                vec![Effect::Probe(destination)]
            }
            None => Vec::new(),
        }
    }

    pub fn update(&mut self, message: Message) -> Vec<Effect> {
        self.problem = None;
        match message {
            Message::Kind(kind) => {
                self.kind = kind;
                if kind == Kind::Remote && self.user_remotes.is_empty() {
                    return vec![Effect::ListRemotes];
                }
                Vec::new()
            }
            Message::ChooseFolder => vec![Effect::PickFolder],
            Message::FolderChosen(path) => {
                self.folder = Some(path);
                self.check()
            }
            Message::DrivesListed(drives) => {
                self.drives = drives;
                self.drive = None;
                let preferred = self
                    .preferred_drive
                    .as_ref()
                    .and_then(|uuid| self.drives.iter().position(|drive| &drive.uuid == uuid));
                match preferred {
                    Some(index) => self.update(Message::PickDrive(index)),
                    None => Vec::new(),
                }
            }
            Message::PickDrive(index) => {
                self.drive = Some(index);
                self.check()
            }
            Message::DriveFolder(folder) => {
                self.drive_folder = folder;
                Vec::new()
            }
            Message::Host(host) => {
                self.host = host;
                Vec::new()
            }
            Message::User(user) => {
                self.user = user;
                Vec::new()
            }
            Message::Port(port) => {
                self.port = port;
                Vec::new()
            }
            Message::ServerPath(path) => {
                self.server_path = path;
                Vec::new()
            }
            Message::SignIn => {
                if self.signing_in {
                    return Vec::new();
                }
                self.signing_in = true;
                let id = self.google_client_id.trim();
                let secret = self.google_client_secret.trim();
                let credentials = (!id.is_empty() && !secret.is_empty())
                    .then(|| (id.to_owned(), secret.to_owned()));
                vec![Effect::SignIn {
                    name: new_remote_name(),
                    credentials,
                }]
            }
            Message::ToggleGoogleAdvanced => {
                self.google_advanced = !self.google_advanced;
                Vec::new()
            }
            Message::GoogleClientId(value) => {
                self.google_client_id = value;
                Vec::new()
            }
            Message::GoogleClientSecret(value) => {
                self.google_client_secret = value;
                Vec::new()
            }
            Message::SignedIn(result) => {
                self.signing_in = false;
                match result {
                    Ok(remote) => {
                        self.google_remote = Some(remote);
                        self.check()
                    }
                    Err(err) => {
                        self.problem = Some(err.detail);
                        Vec::new()
                    }
                }
            }
            Message::CloudPath(path) => {
                self.cloud_path = path;
                Vec::new()
            }
            Message::RemotesListed(result) => {
                match result {
                    Ok(remotes) => self.user_remotes = remotes,
                    Err(err) => self.problem = Some(err.detail),
                }
                let preferred = self
                    .preferred_remote
                    .take()
                    .and_then(|name| self.user_remotes.iter().position(|remote| *remote == name));
                match preferred {
                    Some(index) => self.update(Message::PickRemote(index)),
                    None => Vec::new(),
                }
            }
            Message::PickRemote(index) => {
                let Some(from) = self.user_remotes.get(index).cloned() else {
                    return Vec::new();
                };
                self.user_remote = Some((index, None));
                vec![Effect::CopyRemote {
                    index,
                    from,
                    to: new_remote_name(),
                }]
            }
            Message::RemoteCopied(index, result) => {
                match result {
                    Ok(copy) if matches!(self.user_remote, Some((chosen, _)) if chosen == index) => {
                        self.user_remote = Some((index, Some(copy)));
                    }
                    Ok(_) => {}
                    Err(err) => self.problem = Some(err.detail),
                }
                Vec::new()
            }
            Message::RemotePath(path) => {
                self.remote_path = path;
                Vec::new()
            }
            Message::RcloneChecked(available) => {
                self.rclone_available = Some(available);
                Vec::new()
            }
            Message::Check => self.check(),
            Message::Probed(destination, result) => {
                if self
                    .checking
                    .as_ref()
                    .is_some_and(|(checked, _)| *checked == destination)
                {
                    self.checking = None;
                }
                if self.destination().as_ref() == Some(&destination) {
                    self.probe = Some((destination, result));
                }
                Vec::new()
            }
        }
    }

    pub fn view(&self, opening: bool, importing: bool) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let selected = Some(self.kind);
        let option = |kind: Kind, title: String, description: String| {
            widget::settings::item::builder(title)
                .description(description)
                .radio(kind, selected, Message::Kind)
        };
        let kinds = widget::settings::section()
            .title(fl!("wizard-where-title"))
            .add(option(
                Kind::Folder,
                fl!("place-folder"),
                fl!("place-folder-description"),
            ))
            .add(option(
                Kind::Drive,
                fl!("place-drive"),
                fl!("place-drive-description"),
            ))
            .add(option(
                Kind::Server,
                fl!("place-server"),
                fl!("place-server-description"),
            ))
            .add(option(
                Kind::Google,
                fl!("google-drive"),
                fl!("place-google-description"),
            ))
            .add(option(
                Kind::Remote,
                fl!("place-remote"),
                fl!("place-remote-description"),
            ));

        let mut column = widget::column::with_capacity(4)
            .spacing(spacing.space_m)
            .push(kinds);

        if self.needs_rclone() && self.rclone_available == Some(false) {
            column = column.push(widget::text::body(fl!("place-rclone-missing")));
            return column.into();
        }
        column = column.push(self.form());

        let verdict: Option<String> = match self.probe() {
            None => self.checking_since().map(|since| {
                fl!(
                    "place-checking-for",
                    time = format::duration(since.elapsed().as_secs())
                )
            }),
            Some(Ok(Probe::Empty)) if opening => Some(fl!("wizard-where-no-repository")),
            Some(Ok(Probe::Empty)) => Some(fl!("wizard-where-new")),
            Some(Ok(Probe::Repository)) if opening => Some(fl!("wizard-where-found")),
            Some(Ok(Probe::Repository)) => Some(fl!("wizard-where-existing")),
            // Déjà Dup's older duplicity format leaves files that are not a
            // restic repository.
            Some(Ok(Probe::NotEmpty)) if importing => Some(fl!("dejadup-other-format")),
            Some(Ok(Probe::NotEmpty)) => Some(fl!("wizard-where-not-empty")),
            Some(Err(err)) => Some(crate::app::errors::describe(
                &fl!("place-check-failed"),
                err,
            )),
        };
        if let Some(verdict) = verdict {
            column = column.push(widget::text::body(verdict));
        }
        if let Some(problem) = &self.problem {
            column = column.push(widget::text::body(problem.as_str()));
        }
        column.into()
    }

    /// The fields for the chosen kind.
    fn form(&self) -> Element<'_, Message> {
        let spacing = theme::active().cosmic().spacing;
        let check = widget::button::standard(fl!("place-check")).on_press_maybe(
            (self.destination().is_some() && self.checking_since().is_none())
                .then_some(Message::Check),
        );
        match self.kind {
            Kind::Folder => {
                let folder = self
                    .folder
                    .as_ref()
                    .map(|path| format::path(path))
                    .unwrap_or_else(|| fl!("wizard-no-folder"));
                widget::settings::section()
                    .add(
                        widget::settings::item::builder(folder).control(
                            widget::button::standard(fl!("wizard-choose-folder"))
                                .on_press(Message::ChooseFolder),
                        ),
                    )
                    .into()
            }
            Kind::Drive => {
                if self.drives.is_empty() {
                    return widget::text::body(fl!("place-no-drives")).into();
                }
                let mut section = widget::settings::section();
                for (index, drive) in self.drives.iter().enumerate() {
                    section = section.add(
                        widget::settings::item::builder(drive.label.clone())
                            .description(format::path(&drive.mount_point))
                            .radio(index, self.drive, Message::PickDrive),
                    );
                }
                widget::column::with_capacity(3)
                    .spacing(spacing.space_xs)
                    .push(section)
                    .push(
                        widget::text_input(fl!("place-folder-on-drive"), &self.drive_folder)
                            .label(fl!("place-folder-on-drive"))
                            .on_input(Message::DriveFolder),
                    )
                    .push(check)
                    .into()
            }
            Kind::Server => widget::column::with_capacity(6)
                .spacing(spacing.space_xs)
                .push(
                    widget::text_input("nas.local", &self.host)
                        .label(fl!("place-host"))
                        .on_input(Message::Host),
                )
                .push(
                    widget::text_input(fl!("place-user-placeholder"), &self.user)
                        .label(fl!("place-user"))
                        .on_input(Message::User),
                )
                .push(
                    widget::text_input("22", &self.port)
                        .label(fl!("place-port"))
                        .on_input(Message::Port),
                )
                .push(
                    widget::text_input("backups/laptop", &self.server_path)
                        .label(fl!("place-server-path"))
                        .on_input(Message::ServerPath),
                )
                .push(widget::text::caption(fl!("place-server-note")))
                .push(check)
                .into(),
            Kind::Google => {
                let mut column = widget::column::with_capacity(4).spacing(spacing.space_xs);
                column = match (&self.google_remote, self.signing_in) {
                    (_, true) => column.push(widget::text::body(fl!("place-signing-in"))),
                    (None, false) => {
                        let mut section = column
                            .push(widget::text::body(fl!("place-google-intro")))
                            .push(
                                widget::button::suggested(fl!("place-sign-in"))
                                    .on_press(Message::SignIn),
                            )
                            .push(
                                widget::button::link(fl!("place-google-advanced"))
                                    .on_press(Message::ToggleGoogleAdvanced),
                            );
                        if self.google_advanced {
                            section = section
                                .push(widget::text::caption(fl!(
                                    "place-google-advanced-description"
                                )))
                                .push(
                                    widget::text_input(
                                        fl!("place-google-client-id"),
                                        &self.google_client_id,
                                    )
                                    .label(fl!("place-google-client-id"))
                                    .on_input(Message::GoogleClientId),
                                )
                                .push(
                                    widget::text_input(
                                        fl!("place-google-client-secret"),
                                        &self.google_client_secret,
                                    )
                                    .label(fl!("place-google-client-secret"))
                                    .on_input(Message::GoogleClientSecret),
                                );
                        }
                        section
                    }
                    (Some(_), false) => column.push(widget::text::body(fl!("place-signed-in"))),
                };
                if self.google_remote.is_some() {
                    column = column
                        .push(
                            widget::text_input(fl!("place-cloud-folder"), &self.cloud_path)
                                .label(fl!("place-cloud-folder"))
                                .on_input(Message::CloudPath),
                        )
                        .push(check);
                }
                column.into()
            }
            Kind::Remote => {
                if self.user_remotes.is_empty() {
                    return widget::text::body(fl!("place-no-remotes")).into();
                }
                let chosen = self.user_remote.as_ref().map(|(index, _)| *index);
                let mut section = widget::settings::section();
                for (index, remote) in self.user_remotes.iter().enumerate() {
                    section = section.add(widget::settings::item::builder(remote.clone()).radio(
                        index,
                        chosen,
                        Message::PickRemote,
                    ));
                }
                widget::column::with_capacity(3)
                    .spacing(spacing.space_xs)
                    .push(section)
                    .push(
                        widget::text_input(fl!("place-cloud-folder"), &self.remote_path)
                            .label(fl!("place-cloud-folder"))
                            .on_input(Message::RemotePath),
                    )
                    .push(check)
                    .into()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive() -> Drive {
        Drive {
            uuid: "1111-AAAA".into(),
            label: "Backup".into(),
            mount_point: "/media/alex/Backup".into(),
        }
    }

    #[test]
    fn a_drive_destination_is_remembered_by_uuid() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Drive));
        place.update(Message::DrivesListed(vec![drive()]));
        let effects = place.update(Message::PickDrive(0));

        assert!(matches!(effects.as_slice(), [Effect::Probe(_)]));
        match place.destination().unwrap() {
            Destination::Removable {
                uuid,
                relative_path,
                label,
            } => {
                assert_eq!(uuid, "1111-AAAA");
                assert_eq!(label, "Backup");
                assert!(relative_path.starts_with("Stellarshot"));
            }
            other => panic!("expected a removable destination, got {other:?}"),
        }
    }

    #[test]
    fn a_probe_only_counts_for_the_destination_it_checked() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Server));
        place.update(Message::Host("nas.local".into()));
        place.update(Message::ServerPath("backups".into()));
        let checked = place.destination().unwrap();
        place.update(Message::Probed(checked, Ok(Probe::Empty)));
        assert!(place.probe().is_some());

        // Editing a field invalidates the check.
        place.update(Message::ServerPath("backups/laptop".into()));
        assert!(place.probe().is_none());
    }

    #[test]
    fn editing_during_a_check_does_not_leave_it_checking() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Server));
        place.update(Message::Host("nas.local".into()));
        place.update(Message::ServerPath("backups".into()));
        let first = place.destination().unwrap();
        assert!(matches!(
            place.update(Message::Check).as_slice(),
            [Effect::Probe(_)]
        ));
        assert!(place.checking_since().is_some());

        place.update(Message::ServerPath("backups/laptop".into()));
        assert!(
            place.checking_since().is_none(),
            "the new path is not being checked"
        );
        place.update(Message::Probed(first, Ok(Probe::Empty)));
        assert!(
            matches!(place.update(Message::Check).as_slice(), [Effect::Probe(_)]),
            "the new path can be checked"
        );
    }

    #[test]
    fn a_server_needs_a_host_a_path_and_a_valid_port() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Server));
        place.update(Message::Host("nas.local".into()));
        assert!(place.destination().is_none(), "no path yet");
        place.update(Message::ServerPath("backups".into()));
        assert!(place.destination().is_some());
        place.update(Message::Port("not a port".into()));
        assert!(place.destination().is_none());
    }

    #[test]
    fn google_needs_a_sign_in_first() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Google));
        assert!(place.destination().is_none());

        let effects = place.update(Message::SignIn);
        let Some(Effect::SignIn { name, credentials }) = effects.first() else {
            panic!("expected a sign-in");
        };
        assert!(name.starts_with("stellarshot-"));
        assert_eq!(*credentials, None, "no custom credentials were given");
        assert!(
            place.update(Message::SignIn).is_empty(),
            "one sign-in at a time"
        );

        place.update(Message::SignedIn(Ok(name.clone())));
        match place.destination().unwrap() {
            Destination::Rclone { remote, path, .. } => {
                assert_eq!(&remote, name);
                assert!(path.starts_with("Stellarshot/"));
            }
            other => panic!("expected an rclone destination, got {other:?}"),
        }
    }

    #[test]
    fn a_google_sign_in_carries_a_complete_pair_of_custom_credentials() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Google));

        // Only one of the two given is not enough to use them: rclone needs
        // both or neither.
        place.update(Message::GoogleClientId("my-client-id".into()));
        let effects = place.update(Message::SignIn);
        let Some(Effect::SignIn { credentials, .. }) = effects.first() else {
            panic!("expected a sign-in");
        };
        assert_eq!(*credentials, None, "a client ID alone is not enough");

        place.signing_in = false;
        place.update(Message::GoogleClientSecret("my-client-secret".into()));
        let effects = place.update(Message::SignIn);
        let Some(Effect::SignIn { credentials, .. }) = effects.first() else {
            panic!("expected a sign-in");
        };
        assert_eq!(
            *credentials,
            Some(("my-client-id".into(), "my-client-secret".into()))
        );
    }

    #[test]
    fn a_user_remote_is_used_through_stellarshots_own_copy() {
        let mut place = Place::default();
        place.update(Message::Kind(Kind::Remote));
        place.update(Message::RemotesListed(Ok(vec!["onedrive".into()])));
        let effects = place.update(Message::PickRemote(0));
        let Some(Effect::CopyRemote { from, to, .. }) = effects.first() else {
            panic!("expected a copy");
        };
        assert_eq!(from, "onedrive");
        assert!(place.destination().is_none(), "not usable until copied");

        place.update(Message::RemoteCopied(0, Ok(to.clone())));
        match place.destination().unwrap() {
            Destination::Rclone {
                remote, provider, ..
            } => {
                assert_eq!(&remote, to, "never the user's own remote");
                assert_eq!(provider, "onedrive");
            }
            other => panic!("expected an rclone destination, got {other:?}"),
        }
    }
}
