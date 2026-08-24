// C1 wave-3 re-cluster: small typed JSON field-reading helpers used by the shell-state persistence functions, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

pub(crate) fn json_string_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a str> {
    object.get(key)?.as_str()
}

pub(crate) fn json_bool_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<bool> {
    object.get(key)?.as_bool()
}

pub(crate) fn json_array_field<'a>(
    object: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<&'a Vec<serde_json::Value>> {
    object.get(key)?.as_array()
}

pub(crate) fn json_u64_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<u64> {
    object.get(key).and_then(json_u64_value)
}

pub(crate) fn json_u64_value(value: &serde_json::Value) -> Option<u64> {
    match value {
        serde_json::Value::Number(number) => number.as_u64(),
        serde_json::Value::String(text) => text.parse::<u64>().ok(),
        _ => None,
    }
}

pub(crate) fn json_f32_field(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<f32> {
    object.get(key).and_then(json_value_to_f32)
}

pub(crate) fn json_number_f32(value: f32) -> serde_json::Value {
    serde_json::Value::Number(
        serde_json::Number::from_f64(value as f64).unwrap_or_else(|| serde_json::Number::from(0)),
    )
}

pub(crate) fn has_duplicate_u64(values: &[u64]) -> bool {
    values
        .iter()
        .enumerate()
        .any(|(index, value)| values.iter().skip(index + 1).any(|other| other == value))
}
