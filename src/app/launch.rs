// SPDX-License-Identifier: GPL-3.0-only

//! What launching `stellarshot` asked for, and the flags the window starts
//! with, including the ones forwarded to an already-running instance.

use cosmic::cosmic_config;

use super::config::StellarshotConfig;

/// What launching `stellarshot` again asked for: the desktop entry's two
/// actions, and a scheduled run's failure notification. Doubles as the
/// action `CosmicFlags` forwards to an already-running instance over
/// D-Bus (see the `CosmicFlags` impl below) — its `Display` is the wire
/// name, `parse_activation` reads it back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Launch {
    NewBackup,
    Restore,
    Profile(String),
}

impl std::fmt::Display for Launch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NewBackup => "new-backup",
            Self::Restore => "restore",
            Self::Profile(_) => "profile",
        })
    }
}

impl Launch {
    /// From the flags this process itself was started with: mirrors
    /// `main.rs`'s own precedence (a new backup, then restore, then a
    /// specific profile) so a single-instance activation asks the running
    /// window for exactly what a fresh launch would have done.
    pub fn from_flags(
        start_wizard: bool,
        start_restore: bool,
        select: &Option<String>,
    ) -> Option<Self> {
        if start_wizard {
            Some(Self::NewBackup)
        } else if start_restore {
            Some(Self::Restore)
        } else {
            select.clone().map(Self::Profile)
        }
    }

    /// The reverse of forwarding one over D-Bus: `action`/`args` arrive as
    /// plain strings either way (D-Bus itself carries nothing else), never
    /// automatically turned back into this enum the way sending it out
    /// was typed.
    pub(super) fn from_wire(action: &str, args: &[String]) -> Option<Self> {
        match action {
            "new-backup" => Some(Self::NewBackup),
            "restore" => Some(Self::Restore),
            "profile" => args.first().cloned().map(Self::Profile),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Flags {
    pub config_handler: Option<cosmic_config::Config>,
    pub config: StellarshotConfig,
    /// Whether the `profiles` key specifically could not be read at
    /// startup: see `App::profiles_read_only`.
    pub profiles_unreadable: bool,
    /// Open the setup wizard as soon as the window appears.
    pub start_wizard: bool,
    /// Select this backup at start: a notification was clicked.
    pub select: Option<String>,
    /// Open the restore page for the selected backup once it is unlocked.
    pub start_restore: bool,
    /// The same three flags above, folded into one: what `CosmicFlags`
    /// forwards to an already-running instance instead of starting a
    /// second one (see [`super::App::dbus_activation`]). Kept alongside them,
    /// not derived from `App` at the point `CosmicFlags` needs it, since
    /// `action`/`args` must return references into `self`.
    pub launch: Option<Launch>,
}

/// Required to launch single-instance (see `Cargo.toml`'s comment on the
/// `libcosmic` `single-instance` feature): forwards whichever of
/// `--new-backup`, `--restore` or `--profile <id>` this process itself was
/// started with to an already-running instance, through
/// [`super::App::dbus_activation`], rather than only raising its window with no
/// information about what was actually asked for.
impl cosmic::app::CosmicFlags for Flags {
    type SubCommand = Launch;
    type Args = Vec<String>;

    fn action(&self) -> Option<&Self::SubCommand> {
        self.launch.as_ref()
    }

    fn args(&self) -> Vec<&str> {
        match &self.launch {
            Some(Launch::Profile(id)) => vec![id.as_str()],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The round trip `Launch::from_flags` and `CosmicFlags` build on: what
    /// launching `stellarshot` again with each flag combination asks an
    /// already-running instance to do, and back, exactly as it would arrive
    /// over D-Bus (plain strings, `Details::ActivateAction`'s own shape —
    /// see `Launch::from_wire`'s doc comment).
    #[test]
    fn a_launch_survives_the_round_trip_to_wire_strings_and_back() {
        let cases = [
            (true, false, &None, Some(Launch::NewBackup)),
            (false, true, &None, Some(Launch::Restore)),
            (
                false,
                false,
                &Some("home".to_owned()),
                Some(Launch::Profile("home".to_owned())),
            ),
            (false, false, &None, None),
        ];
        for (start_wizard, start_restore, select, expected) in cases {
            let launch = Launch::from_flags(start_wizard, start_restore, select);
            assert_eq!(launch, expected);
            let Some(launch) = launch else { continue };
            let action = launch.to_string();
            let args: Vec<String> = match &launch {
                Launch::Profile(id) => vec![id.clone()],
                Launch::NewBackup | Launch::Restore => Vec::new(),
            };
            assert_eq!(
                Launch::from_wire(&action, &args),
                Some(launch),
                "must read back its own wire form"
            );
        }
    }

    #[test]
    fn new_backup_takes_precedence_over_restore_and_profile() {
        // Mirrors `main.rs`'s own precedence: if a caller somehow sets more
        // than one, a new backup wins, then restore, then a profile ID —
        // the same order `main` checks them in.
        assert_eq!(
            Launch::from_flags(true, true, &Some("x".to_owned())),
            Some(Launch::NewBackup)
        );
        assert_eq!(
            Launch::from_flags(false, true, &Some("x".to_owned())),
            Some(Launch::Restore)
        );
    }

    #[test]
    fn an_unrecognized_action_is_not_a_launch() {
        assert_eq!(Launch::from_wire("something-else", &[]), None);
        assert_eq!(Launch::from_wire("profile", &[]), None, "no id, no launch");
    }
}
