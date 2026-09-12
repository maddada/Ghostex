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

pub(super) fn reset_credits(account: &Value) -> Vec<&Value> {
    let mut credits: Vec<_> = array(account, "resetCreditDetails").iter().collect();
    credits.sort_by_key(|credit| timestamp(text(credit, "expiresAt")).unwrap_or(i64::MAX));
    credits
}
