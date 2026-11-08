//! Wall times of an automation schedule, resolved in the schedule's own zone.

use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, Utc};
use jiff::tz::{AmbiguousOffset, TimeZone};
use serde_json::Value;

/// CDXC:Automations 2026-11-08 WHY:
/// A daily, weekly or cron schedule names its zone in `timezone`: `local` (the normalizer's default) is this computer's zone, anything else is an IANA name such as the `Asia/Dubai` the desktop writes. An unknown name falls back to this computer's zone rather than to UTC, which ran "daily at 9:00" at 9:00 UTC (13:00 in UTC+4) before this.
pub(super) fn schedule_zone(schedule: &Value) -> TimeZone {
    match schedule
        .get("timezone")
        .and_then(Value::as_str)
        .map(str::trim)
    {
        None | Some("") | Some("local") => TimeZone::system(),
        Some(name) => TimeZone::get(name).unwrap_or_else(|_| TimeZone::system()),
    }
}

/// `now` as a wall clock in `zone`, whose date and time pick the next candidate day or minute.
pub(super) fn wall_now(zone: &TimeZone, now: DateTime<Utc>) -> Option<DateTime<FixedOffset>> {
    let instant = jiff::Timestamp::from_second(now.timestamp()).ok()?;
    let offset = FixedOffset::east_opt(zone.to_offset(instant).seconds())?;
    Some(now.with_timezone(&offset))
}

/// CDXC:Automations 2026-11-08 DECISION:
/// The instant `date` at `hour:minute` names in `zone`. On a spring-forward day a wall time inside the gap runs at the first valid instant after it (the moment the clocks jump), and on a fall-back day a wall time that happens twice runs once, at the earlier instant.
pub(super) fn wall_time_instant(
    zone: &TimeZone,
    date: NaiveDate,
    hour: u32,
    minute: u32,
) -> Option<DateTime<Utc>> {
    let wall = jiff::civil::date(
        i16::try_from(date.year()).ok()?,
        i8::try_from(date.month()).ok()?,
        i8::try_from(date.day()).ok()?,
    )
    .at(i8::try_from(hour).ok()?, i8::try_from(minute).ok()?, 0, 0);
    let instant = match zone.to_ambiguous_timestamp(wall).offset() {
        AmbiguousOffset::Unambiguous { offset } => offset.to_timestamp(wall).ok()?,
        AmbiguousOffset::Fold { before, .. } => before.to_timestamp(wall).ok()?,
        AmbiguousOffset::Gap { after, .. } => {
            // Read with the later offset, the gap's wall time lands just before the jump.
            let before_jump = after.to_timestamp(wall).ok()?;
            zone.following(before_jump).next()?.timestamp()
        }
    };
    DateTime::from_timestamp(instant.as_second(), 0)
}
