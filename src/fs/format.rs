//! Human-readable sizes, counts and dates, as shown in the file list and
//! status bar.

use std::time::SystemTime;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

const UNITS: [&str; 6] = ["KB", "MB", "GB", "TB", "PB", "EB"];

/// Formats a byte count the way the Size column shows it.
///
/// Uses 1024-based units, like Windows Explorer. Values under 100 get one
/// decimal ("48.3 KB"), larger ones none ("812 KB").
pub fn size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    // Round first so 1023.96 KB becomes "1.0 MB" rather than "1024 KB".
    while unit < UNITS.len() - 1 && round_for_display(value) >= 1024.0 {
        value /= 1024.0;
        unit += 1;
    }
    if round_for_display(value) < 100.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

fn round_for_display(value: f64) -> f64 {
    if value < 99.95 {
        (value * 10.0).round() / 10.0
    } else {
        value.round()
    }
}

/// Formats an exact byte count with thousands separators: "1,214 bytes".
pub fn exact_size(bytes: u64) -> String {
    let unit = if bytes == 1 { "byte" } else { "bytes" };
    format!("{} {unit}", count(bytes))
}

/// Formats a number with comma thousands separators: "12,400".
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// How dates appear in the Modified column and elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DateFormat {
    /// 2026-10-14 09:41
    #[default]
    Iso,
    /// 14-10-2026 09:41
    DayFirst,
    /// 10/14/2026 9:41 AM
    MonthFirst,
}

impl DateFormat {
    pub const ALL: [DateFormat; 3] = [Self::Iso, Self::DayFirst, Self::MonthFirst];

    fn pattern(self) -> &'static str {
        match self {
            Self::Iso => "%Y-%m-%d %H:%M",
            Self::DayFirst => "%d-%m-%Y %H:%M",
            Self::MonthFirst => "%m/%d/%Y %-I:%M %p",
        }
    }
}

/// Formats a timestamp in local time, e.g. "2026-10-13 09:41".
pub fn date(time: SystemTime, format: DateFormat) -> String {
    DateTime::<Local>::from(time)
        .format(format.pattern())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_under_one_kilobyte_are_in_bytes() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(214), "214 B");
        assert_eq!(size(1023), "1023 B");
    }

    #[test]
    fn sizes_match_the_design() {
        assert_eq!(size(1_214), "1.2 KB");
        assert_eq!(size(1_843), "1.8 KB");
        assert_eq!(size(49_459), "48.3 KB");
        assert_eq!(size(831_488), "812 KB");
        assert_eq!(size(2_516_582), "2.4 MB");
        assert_eq!(size(19_503_513), "18.6 MB");
    }

    #[test]
    fn sizes_switch_unit_at_the_rounding_boundary() {
        assert_eq!(size(1024), "1.0 KB");
        assert_eq!(size(102_349), "100 KB");
        assert_eq!(size(1_048_575), "1.0 MB");
        assert_eq!(size(1 << 30), "1.0 GB");
        assert_eq!(size(u64::MAX), "16.0 EB");
    }

    #[test]
    fn exact_sizes_have_separators() {
        assert_eq!(exact_size(1), "1 byte");
        assert_eq!(exact_size(1_214), "1,214 bytes");
        assert_eq!(exact_size(0), "0 bytes");
    }

    #[test]
    fn counts_have_separators() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(4_812), "4,812");
        assert_eq!(count(1_234_567), "1,234,567");
    }

    #[test]
    fn date_formats() {
        use chrono::TimeZone;
        let t: SystemTime = Local
            .with_ymd_and_hms(2026, 10, 14, 9, 41, 0)
            .unwrap()
            .into();
        assert_eq!(date(t, DateFormat::Iso), "2026-10-14 09:41");
        assert_eq!(date(t, DateFormat::DayFirst), "14-10-2026 09:41");
        assert_eq!(date(t, DateFormat::MonthFirst), "10/14/2026 9:41 AM");
    }

    #[test]
    fn dates_use_iso_order() {
        let formatted = date(SystemTime::now(), DateFormat::Iso);
        assert_eq!(formatted.len(), "2026-10-13 09:41".len());
        assert_eq!(&formatted[4..5], "-");
        assert_eq!(&formatted[10..11], " ");
    }
}
