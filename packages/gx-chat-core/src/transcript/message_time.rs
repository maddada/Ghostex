//! The timestamp label and tooltip under a message.
//!
//! Ported from `packages/shared/session-chat-presentation/message-time.ts`.
//!
//! The TypeScript read the host's timezone through `Date`'s local accessors. The core cannot, so
//! the boundary is computed from [`ChatContext::utc_offset_minutes`] instead
//! (`docs/2026-09-21/rust-chat/FAMILIES.md`, family b's open question, answered). One offset is
//! applied to both stamps, which is also what the rounded day difference below was already written
//! to survive.

use serde_json::{json, Value};

use crate::state::ChatContext;

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const MS_PER_DAY: i64 = 86_400_000;

/// A local calendar date and time of day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    pub year: i64,
    /// 1 to 12, the way a calendar writes it; `Date.getMonth()` is this minus one.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// Days since the epoch in local time, which is what the day boundary compares.
    pub local_day: i64,
}

/// Civil date from days since 1970-01-01, Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// The local wall clock for an epoch stamp under `utc_offset_minutes`.
pub fn civil(epoch_ms: i64, utc_offset_minutes: i32) -> Civil {
    let local = epoch_ms + i64::from(utc_offset_minutes) * 60_000;
    let local_day = local.div_euclid(MS_PER_DAY);
    let time_of_day = local.rem_euclid(MS_PER_DAY);
    let (year, month, day) = civil_from_days(local_day);
    Civil {
        year,
        month,
        day,
        hour: (time_of_day / 3_600_000) as u32,
        minute: (time_of_day / 60_000 % 60) as u32,
        local_day,
    }
}

fn clock_time(at: Civil) -> String {
    let hour = if at.hour.is_multiple_of(12) {
        12
    } else {
        at.hour % 12
    };
    let meridiem = if at.hour < 12 { "AM" } else { "PM" };
    format!("{hour}:{:02} {meridiem}", at.minute)
}

/// The local clock time of an epoch stamp, `6:20 PM`.
pub(crate) fn clock_label(epoch_ms: i64, context: &ChatContext) -> String {
    clock_time(civil(epoch_ms, context.utc_offset_minutes))
}

fn ordinal_suffix(day: u32) -> &'static str {
    if (11..=13).contains(&(day % 100)) {
        return "th";
    }
    match day % 10 {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    }
}

/// How long ago `at_ms` was, as the Chat Resources list words it: "now" under a minute, "4m" and
/// "2h" within the day, "yesterday" for the day before, then "3d". Elapsed time, not calendar days,
/// so it needs no timezone and both renderers can recompute it as the clock moves.
///
/// CDXC:SessionChat 2026-10-28 DECISION:
/// User, of the Chat Resources rows: "i wanna see how long ago the question was sent to me". The row's right end says this instead of the clock time (the hover still has the full date and time), and the open list keeps it current. The phone mirrors this in ChatPills.tsx (`ageLabel`).
pub fn age_label(now_ms: i64, at_ms: i64) -> String {
    let minutes = (now_ms - at_ms).max(0) / 60_000;
    match minutes {
        0 => "now".to_string(),
        1..=59 => format!("{minutes}m"),
        60..=1439 => format!("{}h", minutes / 60),
        1440..=2879 => "yesterday".to_string(),
        _ => format!("{}d", minutes / 1440),
    }
}

/// CDXC:SessionChat 2026-09-19 WHY:
/// The label under a message is a day-aware timestamp: today `5:48 AM`, yesterday
/// `yesterday at 5:48 AM`, older `26/07 5:48 AM`, with the year once it differs; the hover title is
/// `5:48 AM, 26th July 2026`. Both renderers read the same function so the two transcripts cannot
/// word a stamp differently.
pub fn message_time(timestamp: Option<i64>, context: &ChatContext) -> Value {
    let Some(timestamp) = timestamp else {
        return Value::Null;
    };
    let at = civil(timestamp, context.utc_offset_minutes);
    let today = civil(context.now_millis(), context.utc_offset_minutes);
    let time = clock_time(at);
    // Rounded in the TypeScript so a 23- or 25-hour DST day still counts as one calendar day; with
    // one offset for both stamps the rounding is exact and this is the plain day difference.
    let day_difference = today.local_day - at.local_day;
    let day_month = format!("{:02}/{:02}", at.day, at.month);
    let label = if day_difference <= 0 {
        time.clone()
    } else if day_difference == 1 {
        format!("yesterday at {time}")
    } else if at.year == today.year {
        format!("{day_month} {time}")
    } else {
        format!("{day_month}/{} {time}", at.year)
    };
    json!({
        "label": label,
        "title": format!(
            "{time}, {}{} {} {}",
            at.day,
            ordinal_suffix(at.day),
            MONTHS[(at.month - 1) as usize],
            at.year
        ),
    })
}

#[cfg(test)]
mod age_tests {
    use super::age_label;

    #[test]
    fn the_age_reads_now_minutes_hours_yesterday_then_days() {
        let at = 1_000_000_000_000;
        let ago = |ms: i64| age_label(at + ms, at);
        assert_eq!(ago(0), "now");
        assert_eq!(ago(59_999), "now");
        assert_eq!(ago(60_000), "1m");
        assert_eq!(ago(4 * 60_000 + 30_000), "4m");
        assert_eq!(ago(59 * 60_000), "59m");
        assert_eq!(ago(60 * 60_000), "1h");
        assert_eq!(ago(23 * 3_600_000 + 59 * 60_000), "23h");
        assert_eq!(ago(24 * 3_600_000), "yesterday");
        assert_eq!(ago(47 * 3_600_000), "yesterday");
        assert_eq!(ago(48 * 3_600_000), "2d");
        assert_eq!(ago(10 * 86_400_000), "10d");
        assert_eq!(
            age_label(at - 5_000, at),
            "now",
            "a clock behind the stamp reads now"
        );
    }
}
