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
    fn times_render_as_date_and_minute() {
        let rendered = local_time(1_790_000_000);
        assert_eq!(rendered.len(), "2026-09-21 12:53".len());
        assert!(rendered.starts_with("2026-09-2"));
    }
}
