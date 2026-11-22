use std::collections::HashSet;

use chrono::{Datelike, Timelike, Utc};
use jiff::tz::TimeZone;

#[cfg(test)]
use chrono::TimeZone as _;
#[cfg(test)]
use serde_json::json;
use serde_json::Value;

use crate::domain::DomainStateError;

use super::schedule_zone::{schedule_zone, wall_now, wall_time_instant};

#[cfg(test)]
use super::*;

pub(super) fn compute_next_run_at(
    schedule: &Value,
    after: Option<chrono::DateTime<Utc>>,
) -> Option<String> {
    let now = after.unwrap_or_else(Utc::now);
    let next = match schedule.get("kind").and_then(Value::as_str)? {
        "once" => {
            let run_at = chrono::DateTime::parse_from_rfc3339(
                schedule.get("runAt").and_then(Value::as_str)?,
            )
            .ok()?
            .with_timezone(&Utc);
            (run_at > now).then_some(run_at)?
        }
        "interval" => {
            let every_ms = schedule.get("everyMs").and_then(Value::as_i64)?;
            now + chrono::Duration::milliseconds(every_ms)
        }
        "daily" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            next_weekly(&schedule_zone(schedule), now, hour, minute, &ALL_DAYS)?
        }
        "weekly" => {
            let (hour, minute) = parse_time(schedule.get("time").and_then(Value::as_str)?)?;
            let days = schedule.get("days").and_then(Value::as_array)?;
            let days = days.iter().filter_map(Value::as_u64).collect::<Vec<_>>();
            next_weekly(&schedule_zone(schedule), now, hour, minute, &days)?
        }
        "cron" => next_basic_cron(
            &schedule_zone(schedule),
            now,
            schedule.get("expression").and_then(Value::as_str)?,
        )?,
        _ => return None,
    };
    Some(next.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

pub(super) fn is_one_shot_schedule(schedule: &Value) -> bool {
    schedule.get("kind").and_then(Value::as_str) == Some("once")
}

const ALL_DAYS: [u64; 7] = [0, 1, 2, 3, 4, 5, 6];

/// The first `hour:minute` in `zone` after `now` on one of `days` (0 = Sunday), stored as UTC.
fn next_weekly(
    zone: &TimeZone,
    now: chrono::DateTime<Utc>,
    hour: u32,
    minute: u32,
    days: &[u64],
) -> Option<chrono::DateTime<Utc>> {
    let day_set = days.iter().copied().collect::<HashSet<_>>();
    let today = wall_now(zone, now)?.date_naive();
    // Eight days: today's time may already have passed, and a DST jump can push a candidate later.
    for offset in 0..=7 {
        let date = today.checked_add_signed(chrono::Duration::days(offset))?;
        let weekday = date.weekday().num_days_from_sunday() as u64;
        if !day_set.contains(&weekday) {
            continue;
        }
        let candidate = wall_time_instant(zone, date, hour, minute)?;
        if candidate > now {
            return Some(candidate);
        }
    }
    None
}

fn next_basic_cron(
    zone: &TimeZone,
    now: chrono::DateTime<Utc>,
    expression: &str,
) -> Option<chrono::DateTime<Utc>> {
    let fields = super::cron::parse_cron(expression)?;
    match fields.minute_step {
        Some(step) => next_minute_step(zone, now, step),
        None => fields.next_after(zone, now),
    }
}

/// CDXC:Automations 2026-11-08 WHY:
/// `*/N * * * *` matches the minutes of the hour on the zone's clock that divide by N, then minute 0 of the next hour, as cron and the desktop's own preview read it. Counting from the epoch in UTC put a step such as `*/45` on the wrong minutes in a zone like UTC+5:45 and let `*/7` drift across hours.
fn next_minute_step(
    zone: &TimeZone,
    now: chrono::DateTime<Utc>,
    step: u32,
) -> Option<chrono::DateTime<Utc>> {
    let wall = wall_now(zone, now)?;
    let next_minute = ((wall.minute() / step) + 1) * step;
    let minutes_ahead = next_minute.min(60) - wall.minute();
    let this_minute = now.timestamp() - i64::from(wall.second());
    chrono::DateTime::from_timestamp(this_minute + i64::from(minutes_ahead) * 60, 0)
}

fn parse_time(value: &str) -> Option<(u32, u32)> {
    let (hour, minute) = value.split_once(':')?;
    let hour = hour.parse::<u32>().ok()?;
    let minute = minute.parse::<u32>().ok()?;
    (hour <= 23 && minute <= 59).then_some((hour, minute))
}

pub(super) fn normalize_time(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value.ok_or_else(|| DomainStateError::bad_request("Schedule time is required."))?;
    parse_time(&value)
        .map(|_| value)
        .ok_or_else(|| DomainStateError::bad_request("Schedule time must be HH:mm."))
}

pub(super) fn normalize_run_at(value: Option<String>) -> Result<String, DomainStateError> {
    let value = value
        .ok_or_else(|| DomainStateError::bad_request("One-time schedule runAt is required."))?;
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|run_at| {
            run_at
                .with_timezone(&Utc)
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        })
        .map_err(|_| {
            DomainStateError::bad_request("One-time schedule runAt must be an ISO 8601 date.")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_input_is_anchored_as_a_one_time_schedule() {
        let before = Utc::now();
        let normalized = normalize_schedule(Some(&json!({
            "kind": "timer",
            "delayMs": 60_000,
        })))
        .expect("timer should normalize");
        let run_at = chrono::DateTime::parse_from_rfc3339(
            normalized
                .get("runAt")
                .and_then(Value::as_str)
                .expect("timer has runAt"),
        )
        .expect("runAt is ISO 8601")
        .with_timezone(&Utc);
        assert_eq!(normalized.get("kind").and_then(Value::as_str), Some("once"));
        assert!(run_at >= before + chrono::Duration::seconds(59));
        assert!(run_at <= Utc::now() + chrono::Duration::seconds(61));
    }

    #[test]
    fn one_time_schedule_has_no_next_run_after_its_deadline() {
        let schedule = json!({
            "kind": "once",
            "runAt": "2026-08-14T09:30:00.000Z",
        });
        let before = Utc.with_ymd_and_hms(2026, 8, 14, 9, 0, 0).single().unwrap();
        let after = Utc
            .with_ymd_and_hms(2026, 8, 14, 10, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            compute_next_run_at(&schedule, Some(before)).as_deref(),
            Some("2026-08-14T09:30:00.000Z")
        );
        assert_eq!(compute_next_run_at(&schedule, Some(after)), None);
    }

    #[test]
    fn prompt_instructions_never_parse_as_a_result() {
        /*
        CDXC:Automations 2026-07-30:
        Regression lock for the pair of bugs that made every delivered run report a
        bogus result: the prompt is echoed into the scanned scrollback, so the
        instruction block must not contain a parseable marker line, and the scan
        must not stop at the first occurrence.
        */
        let prompt = build_automation_prompt("Me fale quem é o NAP");
        assert!(prompt.contains(AUTOMATION_RESULT_PREFIX));
        assert_eq!(parse_automation_result(&prompt), None);
    }

    #[test]
    fn agent_marker_after_echoed_instructions_wins() {
        let session_text = format!(
            "{}\n\nI looked into it.\n\nAUTOMATION_RESULT: needs_attention\nNo NAP context found in the repo.\n",
            build_automation_prompt("Me fale quem é o NAP")
        );
        assert_eq!(
            parse_automation_result(&session_text),
            Some((
                "needs_attention".to_string(),
                Some("No NAP context found in the repo.".to_string())
            ))
        );
    }

    #[test]
    fn last_valid_marker_wins_over_earlier_ones() {
        let session_text = "AUTOMATION_RESULT: findings\nfirst pass\n\nAUTOMATION_RESULT: no_findings\nsecond pass\n";
        assert_eq!(
            parse_automation_result(session_text),
            Some(("no_findings".to_string(), Some("second pass".to_string())))
        );
    }

    #[test]
    fn wrapped_instruction_echo_still_never_parses() {
        /*
        The pane hard-wraps long lines, so the instruction text can break right
        after the marker. An empty or partial trailing line must not parse.
        */
        for wrapped in [
            "AUTOMATION_RESULT:\ncompleted by exactly one of these three words - findings,\n",
            "  AUTOMATION_RESULT: completed by exactly one of these three\n  words - findings, no_findings, or needs_attention.\n",
        ] {
            assert_eq!(parse_automation_result(wrapped), None, "{wrapped}");
        }
    }

    #[test]
    fn prompt_shows_the_marker_alone_on_its_line() {
        /*
        Regression lock for an observed live failure: prose placed right after the
        marker got copied into the agent's answer as
        `AUTOMATION_RESULT: completed no_findings`, which parses as nothing and
        sent the run back to the watcher timeout. The marker must appear only on a
        line of its own, followed by a placeholder and nothing else.
        */
        let prompt = build_automation_prompt("Check the deploy");
        let marker_line = prompt
            .lines()
            .find(|line| line.contains(AUTOMATION_RESULT_PREFIX))
            .expect("prompt names the marker");
        assert_eq!(marker_line.trim(), "AUTOMATION_RESULT: <status>");
        assert_eq!(
            prompt.matches(AUTOMATION_RESULT_PREFIX).count(),
            1,
            "one marker occurrence keeps the echo unambiguous"
        );
    }

    #[test]
    fn status_word_must_stand_alone_after_the_marker() {
        for drifted in [
            "AUTOMATION_RESULT: completed no_findings\nsummary\n",
            "AUTOMATION_RESULT: status findings\nsummary\n",
            "AUTOMATION_RESULT: <status>\nsummary\n",
        ] {
            assert_eq!(parse_automation_result(drifted), None, "{drifted}");
        }
    }

    #[test]
    fn missing_marker_leaves_the_run_pending() {
        assert_eq!(
            parse_automation_result("claude booted and is waiting at an empty composer"),
            None
        );
    }
}
