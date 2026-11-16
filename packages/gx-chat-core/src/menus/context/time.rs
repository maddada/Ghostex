//! The two date calls the context rows make.
//!
//! `Date.parse(iso)` turns the ISO stamps gxserver sends into epoch milliseconds, and
//! `new Date(ms).toLocaleString()` formats Codex's `startedAt`. Neither may read a clock or a
//! timezone, so the offset comes from [`crate::ChatContext::utc_offset_minutes`].
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! `toLocaleString()` is the second locale-dependent call in the whole chat brain (the first is
//! `native-accounts.ts`, family e1's). It is reproduced here in the `en-US` form V8 produces by
//! default, which is what the recordings were made under; a user whose locale differs will see a
//! different string than the TypeScript produced. The fix is a formatted value from the host, not
//! a locale database in this crate.

/// `Date.parse(value)`: epoch milliseconds, or `None` for a stamp this parser does not recognise.
///
/// One rule for the whole crate since 2026-09-22 (`crate::jstime`). This copy used to accept
/// trailing junk after the `Z`, where `Date.parse` answers NaN.
pub fn date_parse(value: &str) -> Option<f64> {
    crate::jstime::parse_iso_millis_utc(value)
}

/// The inverse: the civil date of a day count since the epoch.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = month_position + if month_position < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month, day)
}

/// `new Date(ms).toLocaleString()` in V8's default `en-US` form: `M/D/YYYY, h:mm:ss AM/PM`.
pub fn to_locale_string(epoch_ms: f64, utc_offset_minutes: i32) -> String {
    let local_ms = epoch_ms + f64::from(utc_offset_minutes) * 60_000.0;
    let total_millis = local_ms.floor() as i64;
    let days = total_millis.div_euclid(86_400_000);
    let time_of_day = total_millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    let hour24 = time_of_day / 3_600_000;
    let minute = (time_of_day / 60_000) % 60;
    let second = (time_of_day / 1_000) % 60;
    let suffix = if hour24 < 12 { "AM" } else { "PM" };
    let hour12 = match hour24 % 12 {
        0 => 12,
        hour => hour,
    };
    format!("{month}/{day}/{year}, {hour12}:{minute:0>2}:{second:0>2} {suffix}")
}

/// The local clock time of an epoch-milliseconds instant: `h:mm AM/PM`.
pub fn clock_time(epoch_ms: f64, utc_offset_minutes: i32) -> String {
    let local_ms = epoch_ms + f64::from(utc_offset_minutes) * 60_000.0;
    let time_of_day = (local_ms.floor() as i64).rem_euclid(86_400_000);
    let hour24 = time_of_day / 3_600_000;
    let minute = (time_of_day / 60_000) % 60;
    let suffix = if hour24 < 12 { "AM" } else { "PM" };
    let hour12 = match hour24 % 12 {
        0 => 12,
        hour => hour,
    };
    format!("{hour12}:{minute:0>2} {suffix}")
}
