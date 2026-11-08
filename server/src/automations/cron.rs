//! Five-field cron expressions (`minute hour day-of-month month day-of-week`) on a schedule's own
//! clock: numbers, `*`, lists (`1,15`), ranges (`1-5`) and steps (`*/15`, `0-30/10`).
//!
//! CDXC:Automations 2026-11-09 WHY:
//! A GBot turns plain words into a schedule, and "every month on the 1st at 9" or "at 9, 13 and 17 on weekdays" are ordinary asks that `daily` and `weekly` can't say. The scheduler read only `M H * * D` and `*/N * * * *` before; every expression it read then still runs at exactly the same times (the `*/N` minute step keeps its own path in schedule.rs). When both day-of-month and day-of-week are restricted, a day matching either runs, as cron reads it.

use chrono::{Datelike, Utc};
use jiff::tz::TimeZone;

use super::schedule_zone::{wall_now, wall_time_instant};

/// A parsed expression; `None` for a day field means `*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CronFields {
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Option<Vec<u32>>,
    pub months: Option<Vec<u32>>,
    /// 0 = Sunday (7 is read as Sunday too).
    pub weekdays: Option<Vec<u32>>,
    /// The minute field was `*/N` and every other field `*`.
    pub minute_step: Option<u32>,
}

/// How far ahead a next run is looked for (a 29 February asks for up to four years).
const SEARCH_DAYS: i64 = 4 * 366 + 1;

fn field(text: &str, low: u32, high: u32) -> Option<(Vec<u32>, bool)> {
    if text == "*" {
        return Some(((low..=high).collect(), true));
    }
    let mut values = Vec::new();
    for item in text.split(',') {
        let (range, step) = match item.split_once('/') {
            Some((range, step)) => (range, step.parse::<u32>().ok().filter(|step| *step > 0)?),
            None => (item, 1),
        };
        let (from, to) = if range == "*" {
            (low, high)
        } else if let Some((from, to)) = range.split_once('-') {
            (from.parse::<u32>().ok()?, to.parse::<u32>().ok()?)
        } else {
            let value = range.parse::<u32>().ok()?;
            (value, if item.contains('/') { high } else { value })
        };
        if from < low || to > high || from > to {
            return None;
        }
        values.extend((from..=to).step_by(step as usize));
    }
    values.sort_unstable();
    values.dedup();
    (!values.is_empty()).then_some((values, false))
}

pub(super) fn parse_cron(expression: &str) -> Option<CronFields> {
    let parts: Vec<&str> = expression.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let (minutes, _) = field(parts[0], 0, 59)?;
    let (hours, _) = field(parts[1], 0, 23)?;
    let (days_of_month, any_day) = field(parts[2], 1, 31)?;
    let (months, any_month) = field(parts[3], 1, 12)?;
    let (weekdays, any_weekday) = field(parts[4], 0, 7)?;
    let mut weekdays: Vec<u32> = weekdays.into_iter().map(|day| day % 7).collect();
    weekdays.sort_unstable();
    weekdays.dedup();
    let minute_step = parts[0]
        .strip_prefix("*/")
        .and_then(|step| step.parse::<u32>().ok())
        .filter(|step| *step > 0 && parts[1..].iter().all(|part| *part == "*"));
    Some(CronFields {
        minutes,
        hours,
        days_of_month: (!any_day).then_some(days_of_month),
        months: (!any_month).then_some(months),
        weekdays: (!any_weekday).then_some(weekdays),
        minute_step,
    })
}

impl CronFields {
    fn day_matches(&self, date: chrono::NaiveDate) -> bool {
        if self
            .months
            .as_ref()
            .is_some_and(|months| !months.contains(&date.month()))
        {
            return false;
        }
        let by_month_day = self
            .days_of_month
            .as_ref()
            .map(|days| days.contains(&date.day()));
        let by_weekday = self
            .weekdays
            .as_ref()
            .map(|days| days.contains(&date.weekday().num_days_from_sunday()));
        match (by_month_day, by_weekday) {
            (Some(a), Some(b)) => a || b,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => true,
        }
    }

    /// The first time after `now` on the zone's clock.
    pub(super) fn next_after(
        &self,
        zone: &TimeZone,
        now: chrono::DateTime<Utc>,
    ) -> Option<chrono::DateTime<Utc>> {
        let today = wall_now(zone, now)?.date_naive();
        for offset in 0..SEARCH_DAYS {
            let date = today.checked_add_signed(chrono::Duration::days(offset))?;
            if !self.day_matches(date) {
                continue;
            }
            for hour in &self.hours {
                for minute in &self.minutes {
                    let candidate = wall_time_instant(zone, date, *hour, *minute)?;
                    if candidate > now {
                        return Some(candidate);
                    }
                }
            }
        }
        None
    }
}
