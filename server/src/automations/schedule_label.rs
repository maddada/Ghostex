//! A schedule in plain words ("Every month on the 1st at 09:00") and a next run as a short date
//! ("Mon 1 Dec, 09:00"), on the schedule's own clock.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! A GBot turns the user's words into schedule JSON, so it and the user both need to read back what was set in words, not JSON: the "Routine set" card, `ghostex gbots routine list` and the GBot Settings Routines tab all show this sentence, written here once so every client says the same thing.

use chrono::Datelike;
use serde_json::Value;

use super::cron::parse_cron;
use super::schedule_zone::{schedule_zone, wall_now};

const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
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

/// "a", "a and b", "a, b and c".
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn ordinal(day: u32) -> String {
    let suffix = match (day % 10, day % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{day}{suffix}")
}

fn clock(hour: u32, minute: u32) -> String {
    format!("{hour:02}:{minute:02}")
}

fn plural(count: i64, one: &str, many: &str) -> String {
    if count == 1 {
        one.to_string()
    } else {
        format!("{count} {many}")
    }
}

/// "Every 15 minutes", "Every 2 hours", "Every day".
fn every_ms(ms: i64) -> String {
    const MINUTE: i64 = 60_000;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    if ms % DAY == 0 {
        format!("Every {}", plural(ms / DAY, "day", "days"))
    } else if ms % HOUR == 0 {
        format!("Every {}", plural(ms / HOUR, "hour", "hours"))
    } else {
        format!(
            "Every {}",
            plural((ms / MINUTE).max(1), "minute", "minutes")
        )
    }
}

/// "Every day", "Every weekday", "Every Monday and Friday".
fn on_days(days: &[u32]) -> String {
    let mut days: Vec<u32> = days.iter().map(|day| day % 7).collect();
    days.sort_unstable();
    days.dedup();
    match days.as_slice() {
        [0, 1, 2, 3, 4, 5, 6] => "Every day".to_string(),
        [1, 2, 3, 4, 5] => "Every weekday".to_string(),
        [0, 6] => "Every Saturday and Sunday".to_string(),
        _ => format!(
            "Every {}",
            and_list(
                &days
                    .iter()
                    .map(|day| WEEKDAYS[*day as usize].to_string())
                    .collect::<Vec<_>>()
            )
        ),
    }
}

fn cron_sentence(expression: &str) -> Option<String> {
    let fields = parse_cron(expression)?;
    if let Some(step) = fields.minute_step {
        return Some(every_ms(i64::from(step) * 60_000));
    }
    // "*/15 7-21 * * *": every 15 minutes through a stretch of hours (the day watch's).
    let step = fields
        .minutes
        .get(1)
        .map(|second| second - fields.minutes[0])
        .filter(|step| {
            fields.minutes[0] == 0
                && 60 % step == 0
                && fields.minutes.len() as u32 == 60 / step
                && fields.hours.len() > 1
                && fields.hours.len() < 24
                && fields.hours.windows(2).all(|pair| pair[1] == pair[0] + 1)
        });
    let times = if let Some(step) = step {
        let last_hour = fields.hours[fields.hours.len() - 1];
        format!(
            "every {} from {} to {}",
            plural(i64::from(step), "minute", "minutes"),
            clock(fields.hours[0], 0),
            clock(last_hour, 60 - step)
        )
    } else if fields.hours.len() == 24 && fields.minutes.len() == 1 {
        format!(
            "every hour at {} past",
            plural(i64::from(fields.minutes[0]), "1 minute", "minutes")
        )
    } else if fields.hours.len() * fields.minutes.len() <= 4 {
        let times: Vec<String> = fields
            .hours
            .iter()
            .flat_map(|hour| {
                fields
                    .minutes
                    .iter()
                    .map(move |minute| clock(*hour, *minute))
            })
            .collect();
        format!("at {}", and_list(&times))
    } else {
        return None;
    };
    let month_days = fields
        .days_of_month
        .as_ref()
        .map(|days| and_list(&days.iter().map(|day| ordinal(*day)).collect::<Vec<_>>()));
    let months = fields.months.as_ref().map(|months| {
        and_list(
            &months
                .iter()
                .map(|month| MONTHS[(*month - 1) as usize].to_string())
                .collect::<Vec<_>>(),
        )
    });
    let days = match (&month_days, &months, &fields.weekdays) {
        (None, None, None) => "Every day".to_string(),
        (None, None, Some(weekdays)) => on_days(weekdays),
        (Some(month_days), None, None) => format!("Every month on the {month_days}"),
        (Some(month_days), Some(months), None) => {
            format!("Every year on the {month_days} of {months}")
        }
        (None, Some(months), None) => format!("Every day in {months}"),
        (Some(month_days), None, Some(weekdays)) => {
            format!(
                "On the {month_days} of every month and {}",
                on_days(weekdays).to_lowercase()
            )
        }
        _ => return None,
    };
    if step.is_some() {
        return Some(match days.as_str() {
            "Every day" => capitalized(&times),
            _ => format!("{days}, {times}"),
        });
    }
    Some(format!("{days} {times}"))
}

fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// The zone note a zoned schedule ends with, when it is not this computer's own zone.
fn zone_note(schedule: &Value) -> String {
    match schedule
        .get("timezone")
        .and_then(Value::as_str)
        .map(str::trim)
    {
        None | Some("") | Some("local") => String::new(),
        Some(zone) => format!(" ({zone} time)"),
    }
}

/// `schedule` in plain words.
pub(crate) fn schedule_sentence(schedule: &Value) -> String {
    let text = |key: &str| {
        schedule
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let time = || {
        text("time")
            .split_once(':')
            .and_then(|(hour, minute)| Some(clock(hour.parse().ok()?, minute.parse().ok()?)))
            .unwrap_or_else(|| text("time").to_string())
    };
    match text("kind") {
        "once" => match next_run_label(schedule, text("runAt")) {
            Some(when) => format!("Once, on {when}"),
            None => "Once".to_string(),
        },
        "interval" => every_ms(schedule.get("everyMs").and_then(Value::as_i64).unwrap_or(0)),
        "daily" => format!("Every day at {}{}", time(), zone_note(schedule)),
        "weekly" => {
            let days: Vec<u32> = schedule
                .get("days")
                .and_then(Value::as_array)
                .map(|days| {
                    days.iter()
                        .filter_map(Value::as_u64)
                        .map(|day| day as u32)
                        .collect()
                })
                .unwrap_or_default();
            format!("{} at {}{}", on_days(&days), time(), zone_note(schedule))
        }
        "cron" => match cron_sentence(text("expression")) {
            Some(sentence) => format!("{sentence}{}", zone_note(schedule)),
            None => format!(
                "On the cron schedule {}{}",
                text("expression"),
                zone_note(schedule)
            ),
        },
        _ => "On a schedule Ghostex can't read".to_string(),
    }
}

/// `at` (an ISO time) on the schedule's clock as "Mon 1 Dec, 09:00"; `None` when it doesn't parse.
pub(crate) fn next_run_label(schedule: &Value, at: &str) -> Option<String> {
    let at = chrono::DateTime::parse_from_rfc3339(at)
        .ok()?
        .with_timezone(&chrono::Utc);
    let wall = wall_now(&schedule_zone(schedule), at)?;
    let today = wall_now(&schedule_zone(schedule), chrono::Utc::now())?;
    let year = if wall.year() != today.year() {
        format!(" {}", wall.year())
    } else {
        String::new()
    };
    Some(format!(
        "{} {} {}{}, {}",
        &WEEKDAYS[wall.weekday().num_days_from_sunday() as usize][..3],
        wall.day(),
        &MONTHS[wall.month0() as usize][..3],
        year,
        wall.format("%H:%M")
    ))
}
