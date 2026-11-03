//! ISO-8601 times for registry rows (ms precision, `Z`), and arithmetic on them.

use chrono::{DateTime, SecondsFormat, TimeZone, Utc};

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub(super) fn iso_from_millis(millis: i64) -> String {
    Utc.timestamp_millis_opt(millis)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Now plus `seconds`.
pub(super) fn iso_in(seconds: i64) -> String {
    iso_from_millis(Utc::now().timestamp_millis() + seconds * 1000)
}

pub(super) fn parse_millis(iso: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|time| time.with_timezone(&Utc).timestamp_millis())
}

/// `iso` is in the past.
pub(super) fn is_past(iso: &str) -> bool {
    parse_millis(iso).is_some_and(|millis| millis <= Utc::now().timestamp_millis())
}

/// Seconds from `iso` to now (0 when unreadable or in the future).
pub(super) fn seconds_since(iso: &str) -> u64 {
    parse_millis(iso)
        .map(|millis| ((Utc::now().timestamp_millis() - millis).max(0) / 1000) as u64)
        .unwrap_or(0)
}

/// `YYYY-MM` now (UTC).
pub(super) fn this_month() -> String {
    now_iso().chars().take(7).collect()
}

/// The first instant of this month (UTC).
pub(super) fn month_start_iso() -> String {
    format!("{}-01T00:00:00.000Z", this_month())
}
