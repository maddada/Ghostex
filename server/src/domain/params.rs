use serde_json::{Map, Value};

use crate::domain::{DomainResult, DomainStateError};
use crate::ids::{is_gxserver_project_id, is_gxserver_session_id};

pub(crate) fn required_string_param<'a>(
    params: &'a Map<String, Value>,
    key: &str,
) -> DomainResult<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} must be a string.")))
}

pub(crate) fn optional_trimmed_string_param(
    params: &Map<String, Value>,
    key: &str,
) -> DomainResult<Option<String>> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            Ok((!trimmed.is_empty()).then(|| trimmed.to_string()))
        }
        Some(_) => Err(DomainStateError::bad_request(format!(
            "{key} must be a string when provided."
        ))),
    }
}

pub fn read_domain_rpc_params(body: &Value) -> DomainResult<Map<String, Value>> {
    let Some(object) = body.as_object() else {
        return Err(DomainStateError::bad_request(
            "RPC request body must be an object.",
        ));
    };
    match object.get("params") {
        None => Ok(Map::new()),
        Some(Value::Object(params)) => Ok(params.clone()),
        Some(_) => Err(DomainStateError::bad_request(
            "RPC params must be an object.",
        )),
    }
}

pub fn read_optional_project_id(params: &Map<String, Value>) -> DomainResult<Option<String>> {
    match params.get("projectId") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        _ => read_project_id(params).map(Some),
    }
}

pub fn read_project_id(params: &Map<String, Value>) -> DomainResult<String> {
    let value = params
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_gxserver_project_id(value) {
        return Err(DomainStateError::bad_request(format!(
            "Invalid gxserver project ID: {}.",
            js_string(params.get("projectId"))
        )));
    }
    Ok(value.to_string())
}

/*
CDXC:GxserverCrudParity 2026-06-22-05:39:
TypeScript CRUD update/remove paths for projects and sessions call repository lookup methods before ID validators. Preserve that not-found behavior for stale or client-local IDs while keeping explicit readers strict for list filters, create-session project resolution, removeProject, and lifecycle APIs.
*/
pub(crate) fn read_unvalidated_project_lookup_id(params: &Map<String, Value>) -> String {
    params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| js_string(params.get("projectId")))
}

pub fn read_session_id(params: &Map<String, Value>) -> DomainResult<String> {
    let value = params
        .get("sessionId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !is_gxserver_session_id(value) {
        return Err(DomainStateError::bad_request(format!(
            "Invalid gxserver session ID: {}.",
            js_string(params.get("sessionId"))
        )));
    }
    Ok(value.to_string())
}

pub(crate) fn read_unvalidated_session_lookup_id(params: &Map<String, Value>) -> String {
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| js_string(params.get("sessionId")))
}

pub(crate) fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::Null) => "null".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Number(value)) => value.to_string(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| js_string(Some(item)))
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
    }
}

pub(crate) fn read_string_field(value: &Value, key: &str) -> DomainResult<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state(format!("{key} missing from domain state.")))
}

pub(crate) fn required_string(object: &Map<String, Value>, key: &str) -> DomainResult<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} must be a string.")))
}

pub(crate) fn optional_string(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn bool_field(object: &Map<String, Value>, key: &str) -> bool {
    object.get(key).and_then(Value::as_bool) == Some(true)
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
