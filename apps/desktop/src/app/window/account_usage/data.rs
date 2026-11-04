use serde_json::Value;

pub(super) fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

pub(super) fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}

pub(super) fn timestamp(value: &str) -> Option<i64> {
    crate::notification_feed::parse_iso_epoch_secs(value).map(|secs| secs * 1000)
}

pub(super) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub(super) fn duration(ms: i64) -> String {
    if ms <= 0 {
        return "now".into();
    }
    let hours = ms / 3_600_000;
    let days = hours / 24;
    if days > 0 {
        format!("{days}d {}h", hours % 24)
    } else if hours > 0 {
        format!("{hours}h {}m", ms % 3_600_000 / 60_000)
    } else {
        format!("{}m", (ms / 60_000).max(1))
    }
}

pub(super) fn reset_text(value: &str, now: i64) -> String {
    match timestamp(value) {
        Some(reset) if reset <= now => "resets now".into(),
        Some(reset) => format!("resets in {}", duration(reset - now)),
        None => "reset unavailable".into(),
    }
}

pub(super) fn pace_warning(bar: &Value, now: i64) -> String {
    let Some(reset) = timestamp(text(bar, "resetsAt")) else {
        return String::new();
    };
    let period = bar["limitWindowSeconds"].as_f64().unwrap_or(0.0) * 1000.0;
    let used = bar["usedPercent"].as_f64().unwrap_or(0.0);
    let elapsed = now as f64 - (reset as f64 - period);
    if period <= 0.0 || used < 5.0 || elapsed < 60_000.0_f64.max(period * 0.01) || now >= reset {
        return String::new();
    }
    let projected = used / elapsed * period;
    let eta = (100.0 - used) / (projected / period);
    if projected > 100.0 && eta > 0.0 && eta < (reset - now) as f64 {
        format!("At this pace the limit is hit in {}", duration(eta as i64))
    } else {
        String::new()
    }
}

pub(super) fn compact_tokens(tokens: f64) -> String {
    let mut divisor = 1.0;
    for suffix in ["", "K", "M", "B", "T"] {
        let rounded = if suffix.is_empty() {
            tokens.round()
        } else {
            (tokens / divisor * 10.0).round() / 10.0
        };
        if rounded < 1000.0 || suffix == "T" {
            if suffix.is_empty() {
                return format!("{rounded:.0}");
            }
            return format!(
                "{}{suffix}",
                format!("{rounded:.1}")
                    .trim_end_matches('0')
                    .trim_end_matches('.')
            );
        }
        divisor *= 1000.0;
    }
    unreachable!()
}

pub(super) fn exact_tokens(tokens: f64) -> String {
    let digits = format!("{tokens:.0}");
    let mut result = String::new();
    for (index, c) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result
}

/// How old a reading is, short: "Updated just now", "Updated 15s ago", "Updated 2m ago",
/// "Updated 1h ago", "Updated 3d ago". "Not updated yet" when its time can't be known.
pub(super) fn updated_age(stamp: &str, now: i64) -> String {
    let Some(at) = timestamp(stamp) else {
        return "Not updated yet".into();
    };
    let seconds = (now - at).max(0) / 1000;
    match seconds {
        0..5 => "Updated just now".into(),
        5..60 => format!("Updated {seconds}s ago"),
        60..3_600 => format!("Updated {}m ago", seconds / 60),
        3_600..86_400 => format!("Updated {}h ago", seconds / 3_600),
        _ => format!("Updated {}d ago", seconds / 86_400),
    }
}

/// How long until `updated_age` may read differently, in milliseconds: the next second under a
/// minute, the next minute under an hour, and never over 30 seconds, which the reset countdowns
/// and the pace warning also tick on.
pub(super) fn next_age_tick(stamp: &str, now: i64) -> u64 {
    const CAP: i64 = 30_000;
    let Some(at) = timestamp(stamp) else {
        return CAP as u64;
    };
    let age = (now - at).max(0);
    let delay = match age {
        0..60_000 => 1_000 - age % 1_000,
        60_000..3_600_000 => CAP.min(60_000 - age % 60_000),
        _ => CAP,
    };
    delay as u64
}

pub(super) fn local_date(value: &str, with_date: bool) -> String {
    let Some(ms) = timestamp(value) else {
        return String::new();
    };
    let Some(date) = chrono::DateTime::from_timestamp_millis(ms) else {
        return String::new();
    };
    let local = date.with_timezone(&chrono::Local);
    if with_date {
        local.format("%b %-d, %Y, %-I:%M %p").to_string()
    } else {
        local.format("%I:%M %p").to_string()
    }
}

/// Available resets, soonest deadline first, each keyed by its id and how many earlier entries share that id: a Claude grant with several resets left lists one entry per reset under the same grant id.
pub(super) fn credit_keys(account: &Value) -> Vec<(String, &Value)> {
    let mut credits: Vec<_> = array(account, "resetCreditDetails").iter().collect();
    credits.sort_by_key(|credit| timestamp(text(credit, "expiresAt")).unwrap_or(i64::MAX));
    let mut seen = std::collections::HashMap::<&str, usize>::new();
    credits
        .into_iter()
        .map(|credit| {
            let count = seen.entry(text(credit, "id")).or_default();
            *count += 1;
            (format!("{}#{count}", text(credit, "id")), credit)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAMP: &str = "2026-10-31T10:00:00Z";

    fn at() -> i64 {
        timestamp(STAMP).unwrap()
    }

    #[test]
    fn the_age_is_short_at_every_boundary() {
        let cases = [
            (-5_000, "Updated just now"),
            (0, "Updated just now"),
            (4_999, "Updated just now"),
            (5_000, "Updated 5s ago"),
            (15_000, "Updated 15s ago"),
            (59_999, "Updated 59s ago"),
            (60_000, "Updated 1m ago"),
            (119_999, "Updated 1m ago"),
            (120_000, "Updated 2m ago"),
            (3_599_999, "Updated 59m ago"),
            (3_600_000, "Updated 1h ago"),
            (86_399_999, "Updated 23h ago"),
            (86_400_000, "Updated 1d ago"),
            (3 * 86_400_000, "Updated 3d ago"),
        ];
        for (age, words) in cases {
            assert_eq!(updated_age(STAMP, at() + age), words, "{age}");
        }
    }

    #[test]
    fn a_reading_without_a_known_time_says_not_updated_yet() {
        assert_eq!(updated_age("", at()), "Not updated yet");
        assert_eq!(updated_age("not a date", at()), "Not updated yet");
    }

    /// Both stamps gxserver sends: Codex's `to_rfc3339` carries a fraction and an offset.
    #[test]
    fn a_fractional_stamp_reads_the_same() {
        assert_eq!(
            updated_age("2026-10-31T10:00:00.123456789+00:00", at() + 15_000),
            "Updated 15s ago"
        );
    }

    #[test]
    fn it_ticks_each_second_under_a_minute_and_rarely_after() {
        assert_eq!(next_age_tick(STAMP, at()), 1_000);
        assert_eq!(next_age_tick(STAMP, at() + 15_400), 600);
        assert_eq!(next_age_tick(STAMP, at() + 60_000), 30_000);
        assert_eq!(next_age_tick(STAMP, at() + 110_000), 10_000);
        assert_eq!(next_age_tick(STAMP, at() + 7_200_000), 30_000);
        assert_eq!(next_age_tick("", at()), 30_000);
    }
}
