// SPDX-License-Identifier: GPL-3.0-only

//! Importing a Déjà Dup backup.
//!
//! Déjà Dup keeps its settings in GSettings. A Flatpak install stores them in
//! a keyfile inside its sandbox; a native install stores them in dconf, and
//! `dconf dump` prints the same keyfile format. Keys the user never changed
//! are absent, and take Déjà Dup's schema defaults.
//!
//! Only the settings are read. Déjà Dup's settings are never changed, and its
//! password is never read: on a native install it sits in the login keyring
//! where Stellarshot could technically reach it, which is exactly why this
//! module has no keyring code at all. Whether the backup can be imported —
//! whether it is in the restic format — is decided by looking at the
//! destination, because Déjà Dup's `tool = 'unset'` means it decides that the
//! same way.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::profile::{Retention, Schedule};

/// Where the Flatpak build keeps its settings, relative to `$HOME`.
const FLATPAK_KEYFILE: &str = ".var/app/org.gnome.DejaDup/config/glib-2.0/settings/keyfile";

/// The GSettings path Déjà Dup's schema lives at.
const SCHEMA_PATH: &str = "org/gnome/deja-dup";

/// Where a Déjà Dup backup is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    Folder(PathBuf),
    Drive {
        uuid: String,
        folder: PathBuf,
        label: String,
    },
    Sftp {
        host: String,
        user: String,
        port: u16,
        path: String,
    },
    Google {
        folder: String,
    },
    Rclone {
        remote: String,
        folder: String,
    },
    /// A backend Stellarshot cannot use, by Déjà Dup's name for it.
    Unsupported(String),
}

/// A Déjà Dup backup, as Stellarshot would set it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    pub sources: Vec<PathBuf>,
    pub excludes: Vec<PathBuf>,
    pub place: Place,
    /// Déjà Dup recorded a format other than restic.
    pub other_format: bool,
    /// Déjà Dup's automatic backups: off, daily or weekly.
    pub schedule: Schedule,
    /// Déjà Dup's "Keep" setting.
    pub retention: Retention,
}

/// Find and read Déjà Dup's settings: the Flatpak keyfile first, then dconf.
pub fn find() -> Option<Import> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let text = std::fs::read_to_string(home.join(FLATPAK_KEYFILE))
        .ok()
        .or_else(dconf_dump)?;
    let settings = parse(&text);
    if settings.is_empty() {
        return None;
    }
    Some(import(&settings, &UserDirs::load(&home)))
}

fn dconf_dump() -> Option<String> {
    let output = Command::new("dconf")
        .args(["dump", &format!("/{SCHEMA_PATH}/")])
        .output()
        .ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    (output.status.success() && !text.trim().is_empty()).then_some(text)
}

/// Settings by `(section, key)`, sections relative to the schema path: `""`
/// for the main schema, `"google"`, `"local"`, `"drive"`, … for the others.
type Settings = HashMap<(String, String), String>;

/// Parse either keyfile flavour.
fn parse(text: &str) -> Settings {
    let mut settings = Settings::new();
    let mut section = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = normalise_section(name);
            continue;
        }
        let (Some(section), Some((key, value))) = (&section, line.split_once('=')) else {
            continue;
        };
        settings.insert(
            (section.clone(), key.trim().to_owned()),
            value.trim().to_owned(),
        );
    }
    settings
}

/// `[org/gnome/deja-dup/google]` (keyfile) and `[google]` (`dconf dump` of the
/// schema path) both become `google`; `[org/gnome/deja-dup]` and `[/]` become
/// the empty string. Anything else is not Déjà Dup's.
fn normalise_section(name: &str) -> Option<String> {
    if name == "/" || name == SCHEMA_PATH {
        return Some(String::new());
    }
    if let Some(rest) = name.strip_prefix(&format!("{SCHEMA_PATH}/")) {
        return Some(rest.trim_matches('/').to_owned());
    }
    (!name.contains('/') || name.ends_with('/')).then(|| name.trim_matches('/').to_owned())
}

/// A GVariant string: `'text'`, with `\'` and `\\` escapes.
fn string(value: &str) -> Option<String> {
    let inner = value.trim().strip_prefix('\'')?.strip_suffix('\'')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// A GVariant string array: `['a', 'b']`, or `@as []` for an empty one.
fn string_array(value: &str) -> Option<Vec<String>> {
    let value = value.trim().trim_start_matches("@as").trim();
    let inner = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut items = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in inner.chars() {
        match (c, quoted, escaped) {
            (_, true, true) => {
                current.push(c);
                escaped = false;
            }
            ('\\', true, false) => escaped = true,
            ('\'', _, false) => {
                if quoted {
                    items.push(std::mem::take(&mut current));
                }
                quoted = !quoted;
            }
            (_, true, false) => current.push(c),
            _ => {}
        }
    }
    Some(items)
}

fn get<'a>(settings: &'a Settings, section: &str, key: &str) -> Option<&'a str> {
    settings
        .get(&(section.to_owned(), key.to_owned()))
        .map(String::as_str)
}

/// A whole-number setting, or the schema default.
fn number(settings: &Settings, section: &str, key: &str, default: i64) -> i64 {
    get(settings, section, key)
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

/// A true/false setting, or the schema default.
fn flag(settings: &Settings, section: &str, key: &str, default: bool) -> bool {
    match get(settings, section, key).map(str::trim) {
        Some("true") => true,
        Some("false") => false,
        _ => default,
    }
}

/// A string setting, or the schema default.
fn text(settings: &Settings, section: &str, key: &str, default: &str) -> String {
    get(settings, section, key)
        .and_then(string)
        .unwrap_or_else(|| default.to_owned())
}

/// The XDG user directories Déjà Dup's `$TOKENS` stand for.
pub struct UserDirs {
    home: PathBuf,
    dirs: HashMap<String, PathBuf>,
}

impl UserDirs {
    /// Read `~/.config/user-dirs.dirs`, falling back to the English defaults.
    pub fn load(home: &Path) -> Self {
        let text = std::fs::read_to_string(home.join(".config/user-dirs.dirs")).unwrap_or_default();
        Self::parse(home, &text)
    }

    fn parse(home: &Path, text: &str) -> Self {
        let mut dirs = HashMap::new();
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else {
                continue;
            };
            let Some(name) = key
                .strip_prefix("XDG_")
                .and_then(|k| k.strip_suffix("_DIR"))
            else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            let path = match value.strip_prefix("$HOME") {
                Some(rest) => home.join(rest.trim_start_matches('/')),
                None => PathBuf::from(value),
            };
            dirs.insert(name.to_owned(), path);
        }
        Self {
            home: home.to_path_buf(),
            dirs,
        }
    }

    fn dir(&self, name: &str, fallback: &str) -> PathBuf {
        self.dirs
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.home.join(fallback))
    }

    /// Resolve one entry of Déjà Dup's include or exclude list.
    fn resolve(&self, entry: &str) -> PathBuf {
        let token = |name: &str| -> Option<PathBuf> {
            Some(match name {
                "$HOME" => self.home.clone(),
                "$TRASH" => self.home.join(".local/share/Trash"),
                "$DESKTOP" => self.dir("DESKTOP", "Desktop"),
                "$DOCUMENTS" => self.dir("DOCUMENTS", "Documents"),
                "$DOWNLOAD" => self.dir("DOWNLOAD", "Downloads"),
                "$MUSIC" => self.dir("MUSIC", "Music"),
                "$PICTURES" => self.dir("PICTURES", "Pictures"),
                "$PUBLIC_SHARE" => self.dir("PUBLICSHARE", "Public"),
                "$TEMPLATES" => self.dir("TEMPLATES", "Templates"),
                "$VIDEOS" => self.dir("VIDEOS", "Videos"),
                _ => return None,
            })
        };
        let (head, rest) = entry.split_once('/').unwrap_or((entry, ""));
        match token(head) {
            Some(base) if rest.is_empty() => base,
            Some(base) => base.join(rest),
            None if entry.starts_with('~') => self.home.join(entry.trim_start_matches(['~', '/'])),
            None if Path::new(entry).is_relative() => self.home.join(entry),
            None => PathBuf::from(entry),
        }
    }
}

/// `$HOSTNAME` in Déjà Dup's folder settings.
fn with_hostname(folder: &str) -> String {
    folder.replace("$HOSTNAME", &crate::app::wizard::place::hostname())
}

/// Turn Déjà Dup's settings into a Stellarshot backup.
fn import(settings: &Settings, dirs: &UserDirs) -> Import {
    let list = |key: &str, default: &[&str]| -> Vec<PathBuf> {
        get(settings, "", key)
            .and_then(string_array)
            .unwrap_or_else(|| default.iter().map(|s| (*s).to_owned()).collect())
            .iter()
            .map(|entry| dirs.resolve(entry))
            .collect()
    };
    let sources = list("include-list", &["$HOME"]);
    let excludes = list("exclude-list", &["$TRASH", "$DOWNLOAD"]);
    let tool = text(settings, "", "tool", "unset");
    let backend = text(settings, "", "backend", "auto");

    let place = match backend.as_str() {
        "local" | "file" => {
            let folder = with_hostname(&text(settings, "local", "folder", "$HOSTNAME"));
            Place::Folder(dirs.resolve(&folder))
        }
        "drive" => Place::Drive {
            uuid: text(settings, "drive", "uuid", ""),
            folder: PathBuf::from(with_hostname(&text(
                settings,
                "drive",
                "folder",
                "$HOSTNAME",
            ))),
            label: text(settings, "drive", "name", ""),
        },
        "google" => Place::Google {
            folder: with_hostname(&text(settings, "google", "folder", "$HOSTNAME")),
        },
        "rclone" => Place::Rclone {
            remote: text(settings, "rclone", "remote", ""),
            folder: with_hostname(&text(settings, "rclone", "folder", "$HOSTNAME")),
        },
        "remote" => {
            let uri = text(settings, "remote", "uri", "");
            let folder = with_hostname(&text(settings, "remote", "folder", "$HOSTNAME"));
            sftp(&uri, &folder).unwrap_or(Place::Unsupported(uri))
        }
        other => Place::Unsupported(other.to_owned()),
    };
    // Schema defaults: automatic backups off, every 7 days, kept forever.
    let schedule = match (
        flag(settings, "", "periodic", false),
        number(settings, "", "periodic-period", 7),
    ) {
        (false, _) => Schedule::Manual,
        (true, days) if days < 7 => Schedule::Daily,
        (true, _) => Schedule::Weekly,
    };
    let retention = match number(settings, "", "delete-after", 0) {
        days if days <= 0 => Retention::KeepForever,
        days => Retention::KeepFor {
            days: u32::try_from(days).unwrap_or(u32::MAX),
        },
    };
    Import {
        sources,
        excludes,
        place,
        other_format: matches!(tool.as_str(), "duplicity" | "borg"),
        schedule,
        retention,
    }
}

/// `sftp://user@host:port/path` plus Déjà Dup's folder.
fn sftp(uri: &str, folder: &str) -> Option<Place> {
    let rest = uri
        .strip_prefix("sftp://")
        .or_else(|| uri.strip_prefix("ssh://"))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (user, host_port) = authority
        .rsplit_once('@')
        .map_or(("", authority), |(user, host)| (user, host));
    let (host, port) = match host_port.rsplit_once(':') {
        Some((host, port)) => (host, port.parse().ok()?),
        None => (host_port, 22),
    };
    let path = [path.trim_matches('/'), folder.trim_matches('/')]
        .iter()
        .filter(|part| !part.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join("/");
    Some(Place::Sftp {
        host: host.to_owned(),
        user: user.to_owned(),
        port,
        path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Modelled on a real Déjà Dup 50 Flatpak keyfile: Google Drive, a long
    /// exclude list, and no `include-list`, `tool` or `periodic` keys, so
    /// those take their schema defaults.
    const FLATPAK: &str = r#"[org/gnome/deja-dup]
periodic-timestamp='2026-09-22T22:59:29.766968-05'
backend='google'
periodic-period=7
delete-after=0
window-height=997
exclude-list=['$TRASH', '$DOWNLOAD', '/home/alex/Projects', '/home/alex/.cache', '/home/alex/.cargo']
last-backup='2026-09-23T11:16:00.299886-05'

[org/gnome/deja-dup/google]
folder='laptop-backup'

[org/gnome/deja-dup/local]
folder='laptop'
"#;

    /// What `dconf dump /org/gnome/deja-dup/` prints for a native install.
    const DCONF: &str = r#"[/]
backend='drive'
include-list=['$HOME', '/srv/projects']
exclude-list=@as []
tool='restic'

[drive]
uuid='1111-AAAA'
folder='$HOSTNAME'
name='Backup SSD'
"#;

    fn home() -> PathBuf {
        PathBuf::from("/home/alex")
    }

    fn dirs() -> UserDirs {
        UserDirs::parse(
            &home(),
            "XDG_DOWNLOAD_DIR=\"$HOME/Téléchargements\"\nXDG_PICTURES_DIR=\"$HOME/Pictures\"\n",
        )
    }

    #[test]
    fn a_flatpak_keyfile_imports() {
        let import = import(&parse(FLATPAK), &dirs());

        assert_eq!(
            import.place,
            Place::Google {
                folder: "laptop-backup".into()
            }
        );
        assert!(
            import
                .excludes
                .contains(&PathBuf::from("/home/alex/.cargo"))
        );
        assert!(!import.other_format, "an unset tool is decided by probing");
    }

    #[test]
    fn schedule_and_keep_come_across() {
        let defaults = import(&parse(FLATPAK), &dirs());
        assert_eq!(
            defaults.schedule,
            Schedule::Manual,
            "`periodic` defaults to false in Déjà Dup's schema"
        );
        assert_eq!(
            defaults.retention,
            Retention::KeepForever,
            "0 means forever"
        );

        let daily = import(
            &parse("[/]\nperiodic=true\nperiodic-period=1\ndelete-after=182\n"),
            &dirs(),
        );
        assert_eq!(daily.schedule, Schedule::Daily);
        assert_eq!(daily.retention, Retention::KeepFor { days: 182 });

        let weekly = import(&parse("[/]\nperiodic=true\n"), &dirs());
        assert_eq!(weekly.schedule, Schedule::Weekly, "every 7 days by default");
    }

    #[test]
    fn missing_keys_take_the_schema_defaults() {
        let import = import(&parse(FLATPAK), &dirs());

        assert_eq!(
            import.sources,
            vec![home()],
            "include-list defaults to $HOME"
        );
        assert!(
            import
                .excludes
                .contains(&PathBuf::from("/home/alex/.local/share/Trash"))
        );
    }

    #[test]
    fn tokens_follow_the_users_own_folder_names() {
        let import = import(&parse(FLATPAK), &dirs());
        assert!(
            import
                .excludes
                .contains(&PathBuf::from("/home/alex/Téléchargements")),
            "$DOWNLOAD comes from user-dirs.dirs, not a hard-coded English name"
        );
    }

    #[test]
    fn a_dconf_dump_imports() {
        let import = import(&parse(DCONF), &dirs());

        assert_eq!(import.sources, vec![home(), PathBuf::from("/srv/projects")]);
        assert!(import.excludes.is_empty(), "@as [] is an empty list");
        match import.place {
            Place::Drive {
                uuid,
                folder,
                label,
            } => {
                assert_eq!(uuid, "1111-AAAA");
                assert_eq!(label, "Backup SSD");
                assert!(!folder.to_string_lossy().contains("$HOSTNAME"));
            }
            other => panic!("expected a drive, got {other:?}"),
        }
    }

    #[test]
    fn a_relative_local_folder_is_under_home() {
        let settings = parse(
            "[org/gnome/deja-dup]\nbackend='local'\n\n[org/gnome/deja-dup/local]\nfolder='laptop'\n",
        );
        assert_eq!(
            import(&settings, &dirs()).place,
            Place::Folder(PathBuf::from("/home/alex/laptop"))
        );
    }

    #[test]
    fn sftp_uris_become_servers() {
        assert_eq!(
            sftp("sftp://alex@nas.local:2222/backups", "laptop"),
            Some(Place::Sftp {
                host: "nas.local".into(),
                user: "alex".into(),
                port: 2222,
                path: "backups/laptop".into(),
            })
        );
        assert_eq!(
            sftp("smb://nas/share", "x"),
            None,
            "other schemes are not SFTP"
        );
    }

    #[test]
    fn other_formats_and_backends_are_flagged() {
        let duplicity = parse("[org/gnome/deja-dup]\ntool='duplicity'\nbackend='local'\n");
        assert!(import(&duplicity, &dirs()).other_format);

        let s3 = parse("[org/gnome/deja-dup]\nbackend='s3'\n");
        assert_eq!(import(&s3, &dirs()).place, Place::Unsupported("s3".into()));
    }

    #[test]
    fn strings_unescape() {
        assert_eq!(string(r"'it\'s'"), Some("it's".into()));
        assert_eq!(
            string_array(r"['a b', 'c\'d']"),
            Some(vec!["a b".into(), "c'd".into()])
        );
    }
}
