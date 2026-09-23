// SPDX-License-Identifier: GPL-3.0-only

//! Human-readable sizes and times.

use jiff::{Timestamp, tz::TimeZone};

/// A byte count in decimal (SI) units, as the COSMIC and GNOME file managers
/// show sizes: `1.2 GB` is 1,200,000,000 bytes.
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 7] = ["B", "kB", "MB", "GB", "TB", "PB", "EB"];
    if count < 1000 {
        return format!("{count} B");
    }
    let mut value = count as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

/// A Unix time as a local date and time, `2026-09-23 14:02`.
pub fn local_time(seconds: i64) -> String {
    match Timestamp::from_second(seconds) {
        Ok(time) => time
            .to_zoned(TimeZone::system())
            .strftime("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => seconds.to_string(),
    }
}

/// A path as the file manager shows it: `~/Documents` rather than
/// `/home/alex/Documents`.
pub fn path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    shorten(path, home.as_deref())
}

fn shorten(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// How long ago something happened, in the largest whole unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ago {
    JustNow,
    Minutes(i64),
    Hours(i64),
    Days(i64),
}

/// How long before `now` the Unix time `then` was. A time in the future
/// (a clock that moved) reads as "just now" rather than a negative age.
pub fn ago(now: i64, then: i64) -> Ago {
    let seconds = (now - then).max(0);
    match seconds {
        0..60 => Ago::JustNow,
        60..3600 => Ago::Minutes(seconds / 60),
        3600..86_400 => Ago::Hours(seconds / 3600),
        _ => Ago::Days(seconds / 86_400),
    }
}

/// The current Unix time.
pub fn now() -> i64 {
    jiff::Timestamp::now().as_second()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_use_decimal_units() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(999), "999 B");
        assert_eq!(bytes(1000), "1.0 kB");
        assert_eq!(bytes(1_234_567), "1.2 MB");
        assert_eq!(bytes(212_000_000_000), "212 GB");
        assert_eq!(bytes(u64::MAX), "18 EB");
    }

    #[test]
    fn paths_under_home_are_shortened() {
        let home = std::path::Path::new("/home/alex");
        assert_eq!(shorten(std::path::Path::new("/home/alex"), Some(home)), "~");
        assert_eq!(
            shorten(std::path::Path::new("/home/alex/.cache"), Some(home)),
            "~/.cache"
        );
        assert_eq!(
            shorten(std::path::Path::new("/home/alexandra/x"), Some(home)),
            "/home/alexandra/x",
            "a sibling with a longer name is not inside"
        );
        assert_eq!(
            shorten(std::path::Path::new("/media/usb"), None),
            "/media/usb"
        );
    }

    #[test]
    fn ages_use_the_largest_whole_unit() {
        assert_eq!(ago(1000, 1000), Ago::JustNow);
        assert_eq!(
            ago(1000, 2000),
            Ago::JustNow,
            "a future time is not negative"
        );
        assert_eq!(ago(3600, 0), Ago::Hours(1));
        assert_eq!(ago(3599, 0), Ago::Minutes(59));
        assert_eq!(ago(86_400 * 3 + 5, 0), Ago::Days(3));
    }

    #[test]
    fn times_render_as_date_and_minute() {
        let rendered = local_time(1_790_000_000);
        assert_eq!(rendered.len(), "2026-09-21 12:53".len());
        assert!(rendered.starts_with("2026-09-2"));
    }
}
