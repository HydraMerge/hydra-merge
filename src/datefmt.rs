//! Commit dates as the user wants to read them. Git hands over seconds and the author's UTC
//! offset (`--date=raw`); the text is made here, so a change of format in the settings shows at
//! once without reading the history again.

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// 2023-11-15 15:07 UTC: the day is above 12 and the hour is past noon, so every format shows
/// which number is the day and whether it is a 12 or 24 hour clock.
const SAMPLE: CommitTime = CommitTime {
    secs: 1_700_060_820,
    offset_min: 0,
};

/// The moment of a commit in the author's own time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommitTime {
    /// Seconds since 1970-01-01 UTC.
    pub secs: i64,
    /// Minutes east of UTC.
    pub offset_min: i32,
}

/// How dates are written in the commit list and the commit view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DateFormat {
    /// `Wed, 15 Nov 2023, 15:07`
    #[default]
    Europe,
    /// `15.11.2023 15:07`
    EuropeNumeric,
    /// `Wed, Nov 15, 2023, 3:07 PM`
    Us,
    /// `11/15/2023 3:07 PM`
    UsNumeric,
    /// `2023-11-15 15:07`
    Iso,
}

impl DateFormat {
    pub const ALL: [DateFormat; 5] = [
        DateFormat::Europe,
        DateFormat::EuropeNumeric,
        DateFormat::Us,
        DateFormat::UsNumeric,
        DateFormat::Iso,
    ];

    /// The name in the session file; it never changes.
    pub fn key(self) -> &'static str {
        match self {
            DateFormat::Europe => "europe",
            DateFormat::EuropeNumeric => "europe-numeric",
            DateFormat::Us => "us",
            DateFormat::UsNumeric => "us-numeric",
            DateFormat::Iso => "iso",
        }
    }

    pub fn from_key(key: &str) -> Option<DateFormat> {
        DateFormat::ALL.into_iter().find(|f| f.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            DateFormat::Europe => "Europe",
            DateFormat::EuropeNumeric => "Europe, numeric",
            DateFormat::Us => "US",
            DateFormat::UsNumeric => "US, numeric",
            DateFormat::Iso => "ISO 8601",
        }
    }

    /// The name with a sample date, for the menu: `US (Wed, Nov 15, 2023, 3:07 PM)`.
    pub fn label(self) -> String {
        format!("{} ({})", self.name(), SAMPLE.format(self))
    }
}

impl CommitTime {
    /// `1700000000 +0500`, as `git log --date=raw` prints it.
    pub fn parse_raw(text: &str) -> Option<CommitTime> {
        let (secs, zone) = text.trim().split_once(' ')?;
        let secs: i64 = secs.parse().ok()?;
        let zone = zone.trim().as_bytes();
        let [sign @ (b'+' | b'-'), digits @ ..] = zone else {
            return None;
        };
        if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let number = |pair: &[u8]| i32::from(pair[0] - b'0') * 10 + i32::from(pair[1] - b'0');
        let (hours, minutes) = (number(&digits[..2]), number(&digits[2..]));
        if hours > 23 || minutes > 59 {
            return None;
        }
        let offset = hours * 60 + minutes;
        Some(CommitTime {
            secs,
            offset_min: if *sign == b'-' { -offset } else { offset },
        })
    }

    pub fn format(&self, format: DateFormat) -> String {
        let local = self.secs + i64::from(self.offset_min) * 60;
        let days = local.div_euclid(86_400);
        let second_of_day = local.rem_euclid(86_400);
        let (hour, minute) = (second_of_day / 3600, second_of_day % 3600 / 60);
        let (year, month, day) = civil_from_days(days);
        // 1970-01-01 was a Thursday.
        let weekday = WEEKDAYS[(days + 4).rem_euclid(7) as usize];
        let month_name = MONTHS[(month - 1) as usize];
        let (hour12, half) = match hour {
            0 => (12, "AM"),
            1..=11 => (hour, "AM"),
            12 => (12, "PM"),
            _ => (hour - 12, "PM"),
        };
        match format {
            DateFormat::Europe => {
                format!("{weekday}, {day} {month_name} {year}, {hour:02}:{minute:02}")
            }
            DateFormat::EuropeNumeric => {
                format!("{day:02}.{month:02}.{year} {hour:02}:{minute:02}")
            }
            DateFormat::Us => {
                format!("{weekday}, {month_name} {day}, {year}, {hour12}:{minute:02} {half}")
            }
            DateFormat::UsNumeric => {
                format!("{month:02}/{day:02}/{year} {hour12}:{minute:02} {half}")
            }
            DateFormat::Iso => {
                format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}")
            }
        }
    }
}

/// (year, month 1-12, day 1-31) of a day counted from 1970-01-01. Howard Hinnant's
/// `civil_from_days`, exact for the whole proleptic Gregorian calendar.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2023-11-14 22:13:20 UTC is 2023-11-15 03:13 in +0500.
    fn at(raw: &str) -> CommitTime {
        CommitTime::parse_raw(raw).expect("valid raw date")
    }

    #[test]
    fn every_format_writes_the_year() {
        let time = at("1700000000 +0500");
        let text = |f| time.format(f);
        assert_eq!(text(DateFormat::Europe), "Wed, 15 Nov 2023, 03:13");
        assert_eq!(text(DateFormat::EuropeNumeric), "15.11.2023 03:13");
        assert_eq!(text(DateFormat::Us), "Wed, Nov 15, 2023, 3:13 AM");
        assert_eq!(text(DateFormat::UsNumeric), "11/15/2023 3:13 AM");
        assert_eq!(text(DateFormat::Iso), "2023-11-15 03:13");
    }

    #[test]
    fn the_authors_offset_decides_the_day() {
        assert_eq!(
            at("1700000000 +0000").format(DateFormat::Iso),
            "2023-11-14 22:13"
        );
        assert_eq!(
            at("1700000000 -0330").format(DateFormat::Iso),
            "2023-11-14 18:43"
        );
        assert_eq!(
            at("0 -0330").format(DateFormat::Europe),
            "Wed, 31 Dec 1969, 20:30"
        );
    }

    #[test]
    fn leap_day_and_year_ends() {
        let leap = at("1709208000 +0000");
        assert_eq!(leap.format(DateFormat::Europe), "Thu, 29 Feb 2024, 12:00");
        assert_eq!(
            at("1704067199 +0000").format(DateFormat::Iso),
            "2023-12-31 23:59"
        );
        assert_eq!(
            at("1704067200 +0000").format(DateFormat::Iso),
            "2024-01-01 00:00"
        );
    }

    #[test]
    fn twelve_hour_clock_has_no_hour_zero() {
        let us = |raw| at(raw).format(DateFormat::UsNumeric);
        assert_eq!(us("1699920300 +0000"), "11/14/2023 12:05 AM");
        assert_eq!(us("1699963200 +0000"), "11/14/2023 12:00 PM");
        assert_eq!(us("1699968600 +0000"), "11/14/2023 1:30 PM");
    }

    #[test]
    fn raw_dates_that_git_does_not_print_are_refused() {
        for bad in [
            "",
            "x +0000",
            "1700000000",
            "1700000000 0500",
            "1700000000 +05",
            "1700000000 +2400",
            "1700000000 +0560",
            "1700000000 +05a0",
        ] {
            assert_eq!(CommitTime::parse_raw(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn keys_are_stable_and_round_trip() {
        for format in DateFormat::ALL {
            assert_eq!(DateFormat::from_key(format.key()), Some(format));
        }
        assert_eq!(DateFormat::from_key("klingon"), None);
        assert_eq!(DateFormat::default(), DateFormat::Europe);
    }

    #[test]
    fn menu_labels_show_a_sample() {
        assert_eq!(DateFormat::Us.label(), "US (Wed, Nov 15, 2023, 3:07 PM)");
        assert_eq!(
            DateFormat::Europe.label(),
            "Europe (Wed, 15 Nov 2023, 15:07)"
        );
    }
}
