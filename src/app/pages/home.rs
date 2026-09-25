// SPDX-License-Identifier: GPL-3.0-only

//! An overview of every backup, every folder it covers on this computer,
//! and every storage location it uses, at a glance.

use std::collections::HashMap;
use std::path::PathBuf;

use cosmic::iced::{Alignment, Length};
use cosmic::{Apply, Element, theme, widget};

use crate::app::Message;
use crate::app::format::{self, Ago};
use crate::app::pages::profile::ProfileState;
use crate::fl;
use crate::profile::Profile;
use crate::run_state::{self, RunState};

pub fn view<'a>(
    profiles: &'a [Profile],
    runs: &HashMap<String, RunState>,
    pages: &HashMap<String, ProfileState>,
    now: i64,
) -> Element<'a, Message> {
    let spacing = theme::active().cosmic().spacing;

    let mut backups = widget::settings::section().title(fl!("home-backups-title"));
    for profile in profiles {
        let run = runs.get(&profile.id).cloned().unwrap_or_default();
        let running = pages.get(&profile.id).is_some_and(ProfileState::is_busy);
        let status = run_state::status(profile, &run, running);
        // "Up to date" reads oddly next to "Not backed up yet" for a manual
        // backup that has simply never run: say only the one that is true.
        let detail = match profile.last_success {
            None => fl!("never-backed-up"),
            Some(time) => {
                let last = match format::ago(now, time) {
                    Ago::JustNow => fl!("backed-up-just-now"),
                    Ago::Minutes(count) => fl!("backed-up-minutes-ago", count = count),
                    Ago::Hours(count) => fl!("backed-up-hours-ago", count = count),
                    Ago::Days(count) => fl!("backed-up-days-ago", count = count),
                };
                fl!("home-backup-detail", status = status.label(), last = last)
            }
        };
        backups = backups.add(
            widget::row::with_capacity(3)
                .spacing(spacing.space_xs)
                .align_y(Alignment::Center)
                .push(widget::icon::from_name(status.icon()).size(16))
                .push(
                    widget::column::with_capacity(2)
                        .push(widget::text::body(profile.name.clone()))
                        .push(widget::text::caption(detail))
                        .width(Length::Fill),
                )
                .push(
                    widget::button::standard(fl!("home-view"))
                        .on_press(Message::SelectProfile(profile.id.clone())),
                ),
        );
    }

    let mut page = widget::column::with_capacity(4)
        .spacing(spacing.space_m)
        .padding(spacing.space_m)
        .push(widget::text::title3(fl!("home")))
        .push(backups);

    let folders = local_folders(profiles);
    if !folders.is_empty() {
        let mut section = widget::settings::section().title(fl!("home-folders-title"));
        for (path, names) in folders {
            section = section.add(row(format::path(&path), names.join(", ")));
        }
        page = page.push(section);
    }

    let locations = storage_locations(profiles);
    if !locations.is_empty() {
        let mut section = widget::settings::section().title(fl!("home-locations-title"));
        for location in locations {
            section = section.add(
                widget::row::with_capacity(2)
                    .spacing(spacing.space_xs)
                    .align_y(Alignment::Center)
                    .push(widget::icon::from_name(location.icon).size(16))
                    .push(
                        widget::column::with_capacity(2)
                            .push(widget::text::body(location.description))
                            .push(widget::text::caption(fl!(
                                "home-location-detail",
                                kind = location.kind,
                                backups = location.used_by.join(", ")
                            ))),
                    ),
            );
        }
        page = page.push(section);
    }

    widget::scrollable(page.apply(widget::container).max_width(900))
        .height(Length::Fill)
        .into()
}

/// A description-and-detail row with no interactive control, matching
/// `pages::profile`'s own.
fn row(title: String, detail: String) -> Element<'static, Message> {
    let spacing = theme::active().cosmic().spacing;
    widget::column::with_capacity(2)
        .spacing(spacing.space_xxxs)
        .padding([spacing.space_xxs, spacing.space_none])
        .push(widget::text::body(title))
        .push(widget::text::caption(detail))
        .into()
}

/// Every folder backed up on this computer, with the name of each backup
/// that includes it, in the order the backups are given. Two backups
/// covering the very same folder appear once, naming both.
fn local_folders(profiles: &[Profile]) -> Vec<(PathBuf, Vec<String>)> {
    let mut by_path: Vec<(PathBuf, Vec<String>)> = Vec::new();
    for profile in profiles {
        for source in &profile.sources {
            match by_path.iter_mut().find(|(path, _)| path == source) {
                Some((_, names)) => names.push(profile.name.clone()),
                None => by_path.push((source.clone(), vec![profile.name.clone()])),
            }
        }
    }
    by_path
}

/// One storage location, and every backup that keeps a repository there.
struct Location {
    description: String,
    kind: String,
    icon: &'static str,
    used_by: Vec<String>,
}

/// Every storage location in use, with the name of each backup that keeps a
/// repository there. Two backups sharing a destination (the same folder, the
/// same server and path, …) appear once.
fn storage_locations(profiles: &[Profile]) -> Vec<Location> {
    let mut by_place: Vec<Location> = Vec::new();
    for profile in profiles {
        let description = profile.destination.describe();
        match by_place
            .iter_mut()
            .find(|place| place.description == description)
        {
            Some(place) => place.used_by.push(profile.name.clone()),
            None => by_place.push(Location {
                description,
                kind: profile.destination.kind_label(),
                icon: profile.destination.icon(),
                used_by: vec![profile.name.clone()],
            }),
        }
    }
    by_place
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Destination;

    fn profile(name: &str, sources: Vec<&str>, destination: Destination) -> Profile {
        Profile::new(
            name.into(),
            destination,
            sources.into_iter().map(PathBuf::from).collect(),
        )
    }

    #[test]
    fn a_folder_shared_by_two_backups_is_listed_once_naming_both() {
        let local = Destination::Local {
            path: "/backup/a".into(),
        };
        let profiles = [
            profile("Home", vec!["/home/alex", "/srv/shared"], local.clone()),
            profile("Projects", vec!["/srv/shared"], local),
        ];

        let folders = local_folders(&profiles);

        assert_eq!(folders.len(), 2, "two distinct folders, not three rows");
        let shared = folders
            .iter()
            .find(|(path, _)| path == &PathBuf::from("/srv/shared"))
            .unwrap();
        assert_eq!(shared.1, vec!["Home".to_owned(), "Projects".to_owned()]);
    }

    #[test]
    fn two_backups_to_the_same_place_share_one_location_row() {
        let same = Destination::Local {
            path: "/mnt/backup".into(),
        };
        let profiles = [
            profile("Home", vec!["/home/alex"], same.clone()),
            profile("Documents", vec!["/home/alex/Documents"], same),
            profile(
                "Elsewhere",
                vec!["/home/alex"],
                Destination::Local {
                    path: "/mnt/other".into(),
                },
            ),
        ];

        let locations = storage_locations(&profiles);

        assert_eq!(locations.len(), 2);
        let shared = locations
            .iter()
            .find(|place| place.description.contains("/mnt/backup"))
            .unwrap();
        assert_eq!(
            shared.used_by,
            vec!["Home".to_owned(), "Documents".to_owned()]
        );
    }
}
