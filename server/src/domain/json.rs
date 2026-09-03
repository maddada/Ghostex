use serde_json::{Map, Value};

use crate::domain::{DomainResult, DomainStateError};

const JSON_LIMIT_CHARS: usize = 1_000_000;

const JSON_MAX_DEPTH: usize = 10;

pub(crate) fn parse_object(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    Ok(Value::Object(parse_object_map(
        value, column, row_kind, row_id,
    )?))
}

pub(crate) fn parse_object_map(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Map<String, Value>> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    parsed
        .as_object()
        .cloned()
        .ok_or_else(|| corrupt_json_column(column, row_kind, row_id, "expected a JSON object"))
}

pub(crate) fn parse_object_array(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    let Some(items) = parsed.as_array() else {
        return Err(corrupt_json_column(
            column,
            row_kind,
            row_id,
            "expected a JSON array of objects",
        ));
    };
    let mut output = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(object) = item.as_object() else {
            return Err(corrupt_json_column(
                column,
                row_kind,
                row_id,
                &format!("expected object at array index {index}"),
            ));
        };
        output.push(Value::Object(object.clone()));
    }
    Ok(Value::Array(output))
}

pub(crate) fn parse_string_array(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    let parsed = parse_json_column(value, column, row_kind, row_id)?;
    let Some(items) = parsed.as_array() else {
        return Err(corrupt_json_column(
            column,
            row_kind,
            row_id,
            "expected a JSON array of strings",
        ));
    };
    let mut output = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let Some(text) = item
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Err(corrupt_json_column(
                column,
                row_kind,
                row_id,
                &format!("expected non-empty string at array index {index}"),
            ));
        };
        output.push(Value::String(text.to_string()));
    }
    Ok(Value::Array(output))
}

pub(crate) fn parse_json_column(
    value: &str,
    column: &str,
    row_kind: &str,
    row_id: &str,
) -> DomainResult<Value> {
    serde_json::from_str(value).map_err(|error| {
        corrupt_json_column(column, row_kind, row_id, &format!("invalid JSON ({error})"))
    })
}

fn corrupt_json_column(
    column: &str,
    row_kind: &str,
    row_id: &str,
    detail: &str,
) -> DomainStateError {
    DomainStateError::corrupt_state(format!(
        "Corrupt gxserver domain-state JSON in {row_kind} {row_id} column {column}: {detail}. Refusing to read or update the row so persisted state is not overwritten."
    ))
}

pub(crate) fn stringify_domain_json_field(field: &str, value: &Value) -> DomainResult<String> {
    assert_domain_json_depth(field, value, 0)?;
    let text = serde_json::to_string(value).map_err(|_| {
        DomainStateError::bad_request(format!("{field} must be JSON-serializable."))
    })?;
    if domain_json_text_length(&text) > JSON_LIMIT_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the gxserver domain-state JSON size limit of {JSON_LIMIT_CHARS} characters."
        )));
    }
    Ok(text)
}

fn domain_json_text_length(text: &str) -> usize {
    /*
    CDXC:StateSync 2026-06-22-05:22:
    TypeScript enforces the domain JSON limit with JavaScript string length, which counts UTF-16 code units rather than UTF-8 bytes. Match that boundary so non-ASCII project/session metadata is not rejected earlier in Rust.
    */
    text.encode_utf16().count()
}

fn assert_domain_json_depth(field: &str, value: &Value, depth: usize) -> DomainResult<()> {
    if depth > JSON_MAX_DEPTH {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the gxserver domain-state JSON depth limit of {JSON_MAX_DEPTH}."
        )));
    }
    match value {
        Value::Array(items) => {
            for item in items {
                assert_domain_json_depth(field, item, depth + 1)?;
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                assert_domain_json_depth(field, item, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
