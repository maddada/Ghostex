use serde_json::Value;

use super::*;

pub(super) fn parse_line(line: &[u8]) -> Option<Value> {
    let text = std::str::from_utf8(line).ok()?;
    let trimmed = text.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() || !trimmed.starts_with('{') {
        return None;
    }
    serde_json::from_str(trimmed).ok()
}

pub fn field<'a>(v: &'a Value, name: &str) -> Option<&'a Value> {
    v.as_object()?.get(name)
}

pub fn string_field<'a>(v: &'a Value, name: &str) -> Option<&'a str> {
    field(v, name)?.as_str()
}

/// Extract plain text from a message `content` field that may be either a JSON
/// string or an array of text-bearing content blocks.
pub(super) fn content_text(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let mut buf = String::new();
            for item in items {
                let Some(t) = string_field(item, "type") else {
                    continue;
                };
                if t != "text" && t != "input_text" {
                    continue;
                }
                let Some(txt) = string_field(item, "text") else {
                    continue;
                };
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(txt);
            }
            if buf.is_empty() {
                None
            } else {
                Some(buf)
            }
        }
        _ => None,
    }
}

pub(super) fn int_val(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                if i > 0 {
                    i as u64
                } else {
                    0
                }
            } else if let Some(f) = n.as_f64() {
                if f > 0.0 {
                    f as u64
                } else {
                    0
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

pub(super) fn float_val(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub(super) fn timestamp_from_object(v: &Value) -> i64 {
    for name in [
        "updated_at",
        "updatedAt",
        "timestamp",
        "created_at",
        "createdAt",
        "ts",
    ] {
        let ts = timestamp_value(field(v, name));
        if ts > 0 {
            return ts;
        }
    }
    let ts = timestamp_value(field(v, "time"));
    if ts > 0 {
        return ts;
    }
    0
}

pub(super) fn timestamp_value(v: Option<&Value>) -> i64 {
    let Some(v) = v else { return 0 };
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                normalize_timestamp(i)
            } else if let Some(f) = n.as_f64() {
                normalize_timestamp(f as i64)
            } else {
                0
            }
        }
        Value::String(s) => parse_timestamp_string(s),
        Value::Object(_) => {
            for name in [
                "updated",
                "updated_at",
                "updatedAt",
                "created",
                "created_at",
                "createdAt",
                "timestamp",
                "ts",
            ] {
                let ts = timestamp_value(field(v, name));
                if ts > 0 {
                    return ts;
                }
            }
            0
        }
        _ => 0,
    }
}

pub(super) fn normalize_timestamp(n: i64) -> i64 {
    if n <= 0 {
        return 0;
    }
    if n > 10_000_000_000 {
        n / 1000
    } else {
        n
    }
}

fn parse_timestamp_string(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    if let Ok(n) = s.parse::<i64>() {
        return normalize_timestamp(n);
    }
    parse_iso8601_seconds(s)
}

pub(super) fn title_from_fields(v: &Value, fields: &[&str]) -> Option<String> {
    if !v.is_object() {
        return None;
    }
    for name in fields {
        let Some(title) = string_field(v, name) else {
            continue;
        };
        if let Some(safe) = clean_title(title) {
            return Some(safe);
        }
    }
    None
}

pub(super) fn title_from_object(v: &Value) -> Option<String> {
    title_from_fields(
        v,
        &[
            "thread_name",
            "threadName",
            "title",
            "session_title",
            "sessionTitle",
            "name",
        ],
    )
}

// CDXC:PromptSearch 2026-06-07-14:59:
// Sanitize display titles at ingestion so corrupt or control-bearing metadata
// cannot render mojibake in the result list.
pub(super) fn clean_title(title: &str) -> Option<String> {
    let trimmed = title.trim_matches([' ', '\t', '\r', '\n']);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed
        .chars()
        .any(|c| (c as u32) < 0x20 || c == '\u{7f}' || c == '\u{fffd}')
    {
        return None;
    }
    Some(trimmed.to_string())
}
