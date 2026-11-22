use std::path::PathBuf;

use chrono::Utc;
use serde_json::{json, Map, Value};

use crate::{domain::DomainStateError, zmx::ZmxEndpointError};

pub(super) fn is_active_status(status: &str) -> bool {
    status == "queued" || status == "running"
}

pub(super) fn read_param_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn read_value_text(object: &Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn required_value_text(value: &Value, key: &str) -> Result<String, DomainStateError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| DomainStateError::corrupt_state(format!("Missing {key}.")))
}

pub(super) fn bool_to_int(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

pub(super) fn parse_json_value(text: &str) -> Value {
    serde_json::from_str(text).unwrap_or_else(|_| json!({}))
}

pub(super) fn stringify_json(value: &Value) -> Result<String, DomainStateError> {
    serde_json::to_string(value).map_err(internal_error)
}

pub(super) fn normalize_path(path: &str) -> String {
    PathBuf::from(path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .take(6)
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "run".to_string()
    } else {
        slug
    }
}

pub(super) fn now_iso() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) fn internal_error(error: impl std::fmt::Display) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: error.to_string(),
    }
}

pub(super) fn zmx_error(error: ZmxEndpointError) -> DomainStateError {
    match error {
        ZmxEndpointError::Domain(error) => error,
        ZmxEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "internalError",
            message,
        },
    }
}

pub(super) fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite automation state error: {error}"),
    }
}
