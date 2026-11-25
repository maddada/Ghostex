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
CDXC:ServerApi 2026-06-22-05:39:
TypeScript CRUD update/remove paths for projects and sessions call repository lookup methods before ID validators. Preserve that not-found behavior for stale or client-local IDs while keeping explicit readers strict for list filters, create-session project resolution, removeProject, and lifecycle APIs.
*/
pub(crate) fn read_unvalidated_project_lookup_id(params: &Map<String, Value>) -> String {
    params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| lookup_text(params.get("projectId")))
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
        .unwrap_or_else(|| lookup_text(params.get("sessionId")))
}

/// The text a lookup id that is not a string is looked up as: JavaScript's `String(value)`, except
/// that a list keeps its brackets, so no list (`["G1"]`) names the row its one item names.
///
/// CDXC:Security 2026-11-26 DECISION:
/// Coordinator, on the user's behalf (re-checks 2 and 3 of #762): guards before a domain call read ids only when they are strings, so an id the lookup turned into a row's id another way (an array of one string) reached a row the guards never saw: on `/api/updateSession` that skipped the agent mark, GBot mark, thread shelf and approval guards. Every route that looks a session or project up through these two helpers (updates, removes, relocations, session notes, the notification feed) now finds no row for a list, the same as its guards; the route-level refusal of non-text ids stays on the two update routes (server/src/session_own_limits.rs).
fn lookup_text(value: Option<&Value>) -> String {
    match value {
        Some(list @ Value::Array(_)) => list.to_string(),
        other => js_string(other),
    }
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
