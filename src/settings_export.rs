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
//! defence against a hand-edited or otherwise untrusted export file rather
//! than trusting the exporting installation to have behaved — a
//! `password_command` from an untrusted file would otherwise run whatever
//! it says, unprompted, the first time its backup's schedule fires. An
//! event's detail text (`event_log::EventKind::Failed`) is the same wording
//! already shown in a dialog or a notification; exporting it exposes
//! nothing new.

use serde::{Deserialize, Serialize};

use crate::engine::redact_url;
use crate::event_log::{self, Event};
use crate::profile::{Destination, Profile};

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
}

/// Add every backup from `export` whose ID `existing` does not already have,
/// and merge every backup's history into the store regardless.
pub fn merge(existing: &[Profile], export: &Export) -> Merged {
    let mut profiles = existing.to_vec();
    let mut added = Vec::new();
    let mut counts = Imported::default();
    for profile in &export.profiles {
        if existing.iter().any(|kept| kept.id == profile.id) {
            counts.skipped += 1;
        } else {
            let mut profile = profile.clone();
            // A well-behaved export already cleared this; an untrusted or
            // hand-edited file might not have, and a `password_command`
            // taken on faith would run unprompted the first time this
            // backup's schedule fires.
            profile.password_command.clear();
            profiles.push(profile.clone());
            added.push(profile);
            counts.added += 1;
        }
    }
    for (id, events) in &export.history {
        event_log::merge(id, events);
    }
    Merged {
        profiles,
        added,
        counts,
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
                skipped: 1
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

    #[test]
    fn history_is_merged_for_new_and_existing_backups_alike() {
        let id = uuid::Uuid::new_v4().to_string();
        let existing = [profile(&id)];
        let export = Export {
            version: VERSION,
            profiles: vec![profile(&id)],
            history: vec![(
                id.clone(),
                vec![Event {
                    time: 1,
                    kind: EventKind::BackedUp,
                }],
            )],
        };

        merge(&existing, &export);

        assert_eq!(event_log::load(&id).len(), 1);
    }

    #[test]
    fn unreadable_text_is_reported_not_panicked_on() {
        assert!(Export::from_text("not RON at all").is_err());
    }
}
