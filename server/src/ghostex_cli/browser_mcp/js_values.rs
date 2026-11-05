use std::time::Duration;

use serde_json::Value;

/// stringFlag: non-strings become String(value) (nullish -> null); strings are
/// trimmed and empty trims become null.
pub(super) fn string_flag(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Some(other) => Some(js_string_of(Some(other))),
    }
}

/// normalizePositiveInteger: JS Number coercion, then Number.isInteger && > 0.
pub(super) fn normalize_positive_integer_opt(value: Option<Value>) -> Option<u64> {
    let number = js_number_of_value(value.as_ref())?;
    if number.fract() == 0.0 && number > 0.0 && number <= u64::MAX as f64 {
        Some(number as u64)
    } else {
        None
    }
}

/// JS Number(value) for JSON values; None represents NaN.
fn js_number_of_value(value: Option<&Value>) -> Option<f64> {
    match value {
        None => None,
        Some(Value::Null) => Some(0.0),
        Some(Value::Bool(flag)) => Some(if *flag { 1.0 } else { 0.0 }),
        Some(Value::Number(number)) => number.as_f64(),
        Some(Value::String(text)) => crate::ghostex_cli::args::js_number(text),
        Some(_) => None,
    }
}

pub(super) fn unique_numbers(values: &[Option<u64>]) -> Vec<u64> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for value in values.iter().flatten() {
        if *value > 0 && seen.insert(*value) {
            unique.push(*value);
        }
    }
    unique
}

pub(super) fn defined(value: Option<&Value>) -> Option<&Value> {
    value.filter(|value| !value.is_null())
}

pub(super) fn defaulted(object: &Value, key: &str) -> Value {
    defined(object.get(key))
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()))
}

pub(super) fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|n| n != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// String(value) for the JSON values the MCP handler sees.
pub(super) fn js_string_of(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Bool(flag)) => flag.to_string(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| match value {
                Value::Null => String::new(),
                other => js_string_of(Some(other)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

/// `http_json` with the gxserver token. A failure carries the HTTP status when the server answered.
pub(super) fn http_json_with_bearer(
    url: &str,
    token: &str,
    timeout_ms: u64,
) -> Result<Value, (Option<u16>, String)> {
    ghostex_paths::loopback_owner::check_url(url).map_err(|error| (None, error.to_string()))?;
    let response = crate::http_agent::agent()
        .get(url)
        .header("Authorization", format!("Bearer {token}"))
        .config()
        .timeout_global(Some(Duration::from_millis(timeout_ms)))
        .http_status_as_error(false)
        .build()
        .call()
        .map_err(|error| (None, error.to_string()))?;
    let code = response.status().as_u16();
    if response.status().is_success() {
        return crate::http_agent::read_json::<Value>(response)
            .map_err(|error| (None, error.to_string()));
    }
    Err((
        Some(code),
        crate::http_agent::read_json::<Value>(response)
            .ok()
            .and_then(|body| body.get("message")?.as_str().map(str::to_string))
            .unwrap_or_else(|| format!("HTTP {code}")),
    ))
}

pub(super) fn http_json(url: &str, timeout_ms: u64) -> Result<Value, String> {
    match crate::http_agent::agent()
        .get(url)
        .config()
        .timeout_global(Some(Duration::from_millis(timeout_ms)))
        .build()
        .call()
    {
        Ok(response) => {
            crate::http_agent::read_json::<Value>(response).map_err(|error| error.to_string())
        }
        Err(ureq::Error::StatusCode(code)) => Err(format!("HTTP {code}")),
        Err(error) => Err(error.to_string()),
    }
}
