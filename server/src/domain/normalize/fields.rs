use serde_json::{Map, Value};

use crate::domain::{parse_object_map, DomainResult, DomainStateError};

pub(crate) fn normalize_required_text(value: Option<&Value>, field: &str) -> DomainResult<String> {
    read_optional_text(value).ok_or_else(|| {
        DomainStateError::bad_request(format!("{field} must be a non-empty string."))
    })
}

pub(crate) fn read_optional_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn normalize_object(value: Option<&Value>) -> Map<String, Value> {
    value
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn normalize_object_array(value: Option<&Value>) -> Vec<Value> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_object().cloned().map(Value::Object))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn normalize_string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn normalize_domain_lifecycle_state(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("running" | "sleeping" | "stopped" | "missing" | "unknown") => {
            value.unwrap().as_str().unwrap().to_string()
        }
        _ => "unknown".to_string(),
    }
}

pub(crate) fn has_string_field(map: &Map<String, Value>, key: &str) -> bool {
    matches!(map.get(key), Some(Value::String(_)))
}

pub(crate) fn insert_optional_string(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

pub(crate) fn insert_optional_trimmed_string(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    let trimmed = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    insert_optional_string(map, key, trimmed);
}

/*
CDXC:SidebarV2Lifecycle 2026-07-29-00:00:
`settledOverride` is the explicit user pin: "settled" forces the settled shelf,
"active" pins a session into the inbox and suppresses auto-settle. Any other
stored value is corrupt or retired state and hydrates as "no override", the same
way an old state.db row without the column does.
*/
pub fn normalize_settled_override(value: Option<&str>) -> Option<String> {
    match value.map(str::trim) {
        Some("settled") => Some("settled".to_string()),
        Some("active") => Some("active".to_string()),
        _ => None,
    }
}

pub(crate) fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        if !value.is_null() {
            map.insert(key.to_string(), value);
        }
    }
}

pub(crate) fn set_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    } else {
        map.remove(key);
    }
}

pub(crate) fn insert_optional_object(
    map: &mut Map<String, Value>,
    key: &str,
    value: Map<String, Value>,
) {
    if !value.is_empty() {
        map.insert(key.to_string(), Value::Object(value));
    }
}

pub(crate) fn insert_parsed_optional_object(
    map: &mut Map<String, Value>,
    key: &str,
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<()> {
    let parsed = parse_object_map(value, column, row_kind, row_id)?;
    if !parsed.is_empty() {
        map.insert(key.to_string(), Value::Object(parsed));
    }
    Ok(())
}

pub(crate) fn update_object_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Object(normalize_object(input.get(key))),
        );
    }
}

pub(crate) fn update_optional_object_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        let value = normalize_object(input.get(key));
        if value.is_empty() {
            next.remove(key);
        } else {
            next.insert(key.to_string(), Value::Object(value));
        }
    }
}

pub(crate) fn update_object_array_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Array(normalize_object_array(input.get(key))),
        );
    }
}

pub(crate) fn update_string_array_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        next.insert(
            key.to_string(),
            Value::Array(
                normalize_string_array(input.get(key))
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
}

pub(crate) fn update_optional_text_field(
    next: &mut Map<String, Value>,
    input: &Map<String, Value>,
    key: &str,
) {
    if input.contains_key(key) {
        set_optional_string(next, key, read_optional_text(input.get(key)));
    }
}
