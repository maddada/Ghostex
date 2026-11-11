//! The few date words the Beads view shows: "4m" since a change, "Until Mon, Nov 17", "Due Fri".
//! The host passes the clock and the time zone offset; the core reads neither.

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The clock the view is drawn at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BeadsClock {
    pub now_ms: i64,
    /// Local time minus UTC.
    pub offset_ms: i64,
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// bd's timestamps: `YYYY-MM-DDTHH:MM:SS`, optional fraction, then `Z` or `±HH:MM`. A bare
/// date reads as midnight UTC.
pub fn parse_bd_time_ms(text: &str) -> Option<i64> {
    let text = text.trim();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let slice = text.get(range)?;
        slice
            .bytes()
            .all(|byte| byte.is_ascii_digit())
            .then(|| slice.parse().ok())
            .flatten()
    };
    let year = number(0..4)?;
    let month = number(5..7)?;
    let day = number(8..10)?;
    let date_ms = days_from_civil(year, month, day) * DAY_MS;
    if text.len() == 10 {
        return Some(date_ms);
    }
    let hour = number(11..13)?;
    let minute = number(14..16)?;
    let second = number(17..19)?;
    let mut rest = &text[19..];
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        rest = &fraction[digits..];
    }
    let offset_ms = match rest {
        "" | "Z" | "z" => 0,
        offset => {
            let sign = match offset.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let hours: i64 = offset.get(1..3)?.parse().ok()?;
            let minutes: i64 = offset.get(4..6)?.parse().ok()?;
            sign * (hours * HOUR_MS + minutes * MINUTE_MS)
        }
    };
    Some(date_ms + hour * HOUR_MS + minute * MINUTE_MS + second * 1_000 - offset_ms)
}

/// "now", "4m", "3h", "2d", "3w", "5mo", "1y".
pub fn relative_age(time_ms: i64, clock: &BeadsClock) -> String {
    let age = (clock.now_ms - time_ms).max(0);
    if age < MINUTE_MS {
        "now".to_string()
    } else if age < HOUR_MS {
        format!("{}m", age / MINUTE_MS)
    } else if age < DAY_MS {
        format!("{}h", age / HOUR_MS)
    } else if age < 14 * DAY_MS {
        format!("{}d", age / DAY_MS)
    } else if age < 60 * DAY_MS {
        format!("{}w", age / (7 * DAY_MS))
    } else if age < 365 * DAY_MS {
        format!("{}mo", age / (30 * DAY_MS))
    } else {
        format!("{}y", age / (365 * DAY_MS))
    }
}

/// "4 min ago", "2 h ago", "3 days ago", for the sync line.
pub fn relative_ago(time_ms: i64, clock: &BeadsClock) -> String {
    let age = (clock.now_ms - time_ms).max(0);
    if age < MINUTE_MS {
        "just now".to_string()
    } else if age < HOUR_MS {
        format!("{} min ago", age / MINUTE_MS)
    } else if age < DAY_MS {
        format!("{} h ago", age / HOUR_MS)
    } else {
        let days = age / DAY_MS;
        if days == 1 {
            "yesterday".to_string()
        } else {
            format!("{days} days ago")
        }
    }
}

/// A future date as the row says it: the weekday within a week ("Fri"), else "Mon, Nov 17".
pub fn short_date(time_ms: i64, clock: &BeadsClock, always_month: bool) -> String {
    let local_days = (time_ms + clock.offset_ms).div_euclid(DAY_MS);
    let today = (clock.now_ms + clock.offset_ms).div_euclid(DAY_MS);
    let weekday = WEEKDAYS[local_days.rem_euclid(7) as usize];
    let (_, month, day) = civil_from_days(local_days);
    if local_days == today {
        return "today".to_string();
    }
    if local_days == today + 1 {
        return "tomorrow".to_string();
    }
    if !always_month && local_days > today && local_days - today < 7 {
        return weekday.to_string();
    }
    format!("{weekday}, {} {day}", MONTHS[(month - 1) as usize])
}

/// The local calendar date `days` after today, as bd's `--until` takes it (`YYYY-MM-DD`).
pub fn local_date_after(days: i64, clock: &BeadsClock) -> String {
    let local_days = (clock.now_ms + clock.offset_ms).div_euclid(DAY_MS) + days;
    let (year, month, day) = civil_from_days(local_days);
    format!("{year:04}-{month:02}-{day:02}")
}
