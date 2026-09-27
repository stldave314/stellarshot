// SPDX-License-Identifier: GPL-3.0-only

//! Exporting and importing Stellarshot's own settings: every backup and its
//! history, never a password or a command that prints one.
//!
//! A [`Profile`] never holds a password itself — it lives in the keyring,
//! or nowhere. Two fields come close, and both are stripped by
//! `Export::collect`, the same as if they were a password, since an export
//! is more likely to be shared or copied somewhere less careful than the
//! settings themselves: `password_command`, a command that prints one; and
//! a REST destination's own URL, which can carry HTTP basic auth
//! (`http://user:pass@host/repo/`). `merge` (import) clears
//! `password_command` again on whatever it adds, as a second line of
//! defense against a hand-edited or otherwise untrusted export file rather
//! than trusting the exporting installation to have behaved — a
//! `password_command` from an untrusted file would otherwise run whatever
//! it says, unprompted, the first time its backup's schedule fires. An
//! event's detail text (`event_log::EventKind::Failed`) is the same wording
//! already shown in a dialog or a notification; exporting it exposes
//! nothing new.

use serde::{Deserialize, Serialize};

use crate::engine::redact_url;
use crate::event_log::{self, Event};
use crate::profile::{self, Destination, Profile, Schedule};

/// The file format's version, bumped only if a change could not otherwise
/// be read by an older Stellarshot.
const VERSION: u32 = 1;

/// Everything a settings export holds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Export {
    version: u32,
    pub profiles: Vec<Profile>,
    /// One backup's history per entry, for whichever profiles had any.
    #[serde(default)]
    pub history: Vec<(String, Vec<Event>)>,
}

impl Export {
    /// Everything currently in `profiles`, with each one's history.
    pub fn collect(profiles: &[Profile]) -> Self {
        let history = profiles
            .iter()
            .map(|profile| (profile.id.clone(), event_log::load(&profile.id)))
            .filter(|(_, events)| !events.is_empty())
            .collect();
        let profiles = profiles
            .iter()
            .cloned()
            .map(|mut profile| {
                profile.password_command.clear();
                if let Destination::Rest { url } = &mut profile.destination {
                    *url = redact_url(url);
                }
                profile
            })
            .collect();
        Self {
            version: VERSION,
            profiles,
            history,
        }
    }

    /// As RON text, the format the rest of Stellarshot's own settings use.
    pub fn to_text(&self) -> Result<String, String> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())
            .map_err(|err| err.to_string())
    }

    pub fn from_text(text: &str) -> Result<Self, String> {
        ron::from_str(text).map_err(|err| err.to_string())
    }
}

/// What importing found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Imported {
    /// Backups added: their ID was not already in use.
    pub added: usize,
    /// Backups left alone: a backup with that ID already exists. Its
    /// history is still merged in, in case the export has entries this
    /// installation does not.
    pub skipped: usize,
    /// Backups refused outright: an ID that is not safe to use in a unit
    /// name and command line, or an rclone remote outside the shape
    /// Stellarshot itself creates (see [`profile::valid_rclone_remote`]).
    /// Neither the profile nor its history is kept.
    pub rejected: usize,
}

/// What merging an export produced.
pub struct Merged {
    /// The full profile list to save: the export never replaces an
    /// existing backup's settings, only adds ones that are new.
    pub profiles: Vec<Profile>,
    /// The profiles that were actually new, in case the caller needs to
    /// do something with only those (a new profile's timer needs
    /// installing; an existing one's is left exactly as it was).
    pub added: Vec<Profile>,
    pub counts: Imported,
    /// Whether any added profile had a schedule or an enabled hook before
    /// this import turned them off (see [`merge`]'s own doc comment). The
    /// caller shows a review hint when this is set.
    pub needs_review: bool,
}

/// Add every backup from `export` whose ID `existing` does not already have,
/// and merge every backup's history into the store regardless.
///
/// An export is untrusted: it may have been hand-edited, or produced by
/// another installation that behaved differently, so nothing in it runs
/// unprompted just because it says to.
///
/// - A profile ID that is not safe to use in a systemd unit name and command
///   line is refused outright, and so is a `Destination::Rclone` whose
///   remote is not one Stellarshot itself could have created; either could
///   otherwise run something the moment its schedule fires or its backup is
///   opened. Refused profiles keep no history either, since nothing here
///   can confirm it belongs to this installation.
/// - Every added profile's schedule is reset to manual and every one of its
///   hooks is disabled (kept, not dropped, so the user can review them),
///   since an imported hook is an arbitrary command that would otherwise run
///   the first time its schedule fires.
pub fn merge(existing: &[Profile], export: &Export) -> Merged {
    let mut profiles = existing.to_vec();
    let mut added = Vec::new();
    let mut counts = Imported::default();
    let mut needs_review = false;
    let mut kept_ids: std::collections::HashSet<String> =
        existing.iter().map(|p| p.id.clone()).collect();
    for profile in &export.profiles {
        if existing.iter().any(|kept| kept.id == profile.id) {
            counts.skipped += 1;
            continue;
        }
        if !profile::valid_id(&profile.id) || !remote_is_safe(&profile.destination) {
            counts.rejected += 1;
            continue;
        }
        let mut profile = profile.clone();
        // A well-behaved export already cleared this; an untrusted or
        // hand-edited file might not have, and a `password_command` taken
        // on faith would run unprompted the first time this backup's
        // schedule fires.
        profile.password_command.clear();
        if profile.schedule != Schedule::Manual {
            needs_review = true;
            profile.schedule = Schedule::Manual;
        }
        for hook in &mut profile.hooks {
            if hook.enabled {
                needs_review = true;
                hook.enabled = false;
            }
        }
        kept_ids.insert(profile.id.clone());
        profiles.push(profile.clone());
        added.push(profile);
        counts.added += 1;
    }
    for (id, events) in &export.history {
        if kept_ids.contains(id) {
            event_log::merge(id, events);
        }
    }
    Merged {
        profiles,
        added,
        counts,
        needs_review,
    }
}

/// Whether `destination` is safe to accept from an untrusted export: only
/// `Destination::Rclone` needs a check here, since its `remote` is used
/// verbatim as `"{remote}:{path}"`, the last argument to `rclone serve
/// restic` (see [`profile::valid_rclone_remote`]'s own doc comment). Every
/// other destination kind is built from structured fields rustic or rclone
/// receive as separate, already-escaped arguments.
fn remote_is_safe(destination: &Destination) -> bool {
    match destination {
        Destination::Rclone { remote, .. } => profile::valid_rclone_remote(remote),
        Destination::Local { .. }
        | Destination::Removable { .. }
        | Destination::Sftp { .. }
        | Destination::Rest { .. } => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_log::EventKind;

    fn profile(id: &str) -> Profile {
        let mut profile = Profile::new(
            "Home".into(),
            Destination::Local {
                path: "/backup".into(),
            },
            vec!["/home/alex".into()],
        );
        profile.id = id.to_owned();
        profile
    }

    #[test]
    fn a_profile_round_trips_through_the_text_form() {
        let export = Export::collect(&[profile("keep-me")]);

        let text = export.to_text().unwrap();
        let read_back = Export::from_text(&text).unwrap();

        assert_eq!(read_back, export);
        assert!(!text.contains("password"), "nothing password-shaped in it");
    }

    #[test]
    fn a_rest_destinations_credentials_are_stripped_on_export() {
        let mut with_credentials = profile("rest-backup");
        with_credentials.destination = Destination::Rest {
            url: "http://alex:s3cret@nas:8000/repo/".into(),
        };

        let export = Export::collect(&[with_credentials]);

        let text = export.to_text().unwrap();
        assert!(
            !text.contains("s3cret"),
            "a REST URL's password must not survive an export: {text}"
        );
        let Destination::Rest { url } = &export.profiles[0].destination else {
            panic!("expected a Rest destination");
        };
        assert!(
            url.contains("nas:8000/repo/"),
            "the rest of the URL is still useful: {url}"
        );
    }

    #[test]
    fn a_password_command_from_an_untrusted_export_is_cleared_on_import() {
        // Simulates a hand-edited or otherwise untrusted export file, not
        // one this version of Stellarshot actually produced (which already
        // clears this field itself) — `merge` must not take it on faith.
        let mut smuggled = profile("smuggled-command");
        smuggled.password_command = "sh -c 'evil'".into();
        let export = Export {
            version: 1,
            profiles: vec![smuggled],
            history: Vec::new(),
        };

        let merged = merge(&[], &export);

        assert_eq!(merged.profiles[0].password_command, "");
        assert_eq!(merged.added[0].password_command, "");
    }

    #[test]
    fn an_imported_backups_schedule_and_hooks_start_off_and_flag_for_review() {
        use crate::profile::{Hook, HookTiming};

        let mut smuggled = profile("smuggled-schedule");
        smuggled.schedule = Schedule::Hourly;
        smuggled.hooks = vec![Hook {
            name: "run at first fire".into(),
            command: "sh -c 'touch /tmp/pwn'".into(),
            timing: HookTiming::Before,
            enabled: true,
        }];
        let export = Export {
            version: 1,
            profiles: vec![smuggled],
            history: Vec::new(),
        };

        let merged = merge(&[], &export);

        assert_eq!(merged.added[0].schedule, Schedule::Manual);
        assert!(
            !merged.added[0].hooks[0].enabled,
            "an imported hook must not run until reviewed"
        );
        assert!(merged.needs_review);
    }

    #[test]
    fn an_rclone_remote_outside_the_wizards_shape_is_rejected_on_import() {
        let mut attack = profile("rclone-attack");
        attack.destination = Destination::Rclone {
            remote: ":sftp,host=h,ssh=\"touch /tmp/pwn\"".into(),
            path: "backups".into(),
            provider: "Custom".into(),
        };
        let export = Export {
            version: 1,
            profiles: vec![attack],
            history: Vec::new(),
        };

        let merged = merge(&[], &export);

        assert_eq!(
            merged.counts,
            Imported {
                added: 0,
                skipped: 0,
                rejected: 1
            }
        );
        assert!(merged.profiles.is_empty());
    }

    #[test]
    fn an_unsafe_profile_id_is_rejected_and_its_history_is_not_merged() {
        let bad_id = profile("../x");
        let export = Export {
            version: 1,
            profiles: vec![bad_id],
            history: vec![(
                "../x".into(),
                vec![Event {
                    time: 1,
                    kind: EventKind::BackedUp,
                    source: event_log::Source::Desktop,
                }],
            )],
        };

        let merged = merge(&[], &export);

        assert_eq!(merged.counts.rejected, 1);
        assert!(merged.profiles.is_empty());
        assert!(event_log::load("../x").is_empty());
    }

    #[test]
    fn a_new_backup_is_added_and_an_existing_one_is_left_alone() {
        let existing = [profile("already-here")];
        let export = Export {
            version: VERSION,
            profiles: vec![
                {
                    let mut changed = profile("already-here");
                    changed.name = "Would overwrite if not skipped".into();
                    changed
                },
                profile("new-to-this-computer"),
            ],
            history: Vec::new(),
        };

        let merged = merge(&existing, &export);

        assert_eq!(
            merged.counts,
            Imported {
                added: 1,
                skipped: 1,
                rejected: 0
            }
        );
        assert_eq!(merged.profiles.len(), 2);
        assert_eq!(
            merged.profiles[0].name, "Home",
            "the existing backup's own settings are kept, not overwritten"
        );
        assert!(
            merged
                .profiles
                .iter()
                .any(|p| p.id == "new-to-this-computer")
        );
        assert_eq!(merged.added.len(), 1);
        assert_eq!(merged.added[0].id, "new-to-this-computer");
    }

    // `history_is_merged_for_new_and_existing_backups_alike` lives in
    // `tests/settings_export_history.rs` instead of here: merging history
    // writes into the real `event_log` state store (there is no test-only
    // namespace for it), so it needs a redirected `XDG_STATE_HOME` — a
    // process-wide setting this crate's own test binary cannot give it
    // without risking every other unit test that also touches state
    // running concurrently in the same process.

    #[test]
    fn unreadable_text_is_reported_not_panicked_on() {
        assert!(Export::from_text("not RON at all").is_err());
    }
}
