use std::collections::HashSet;

use chrono::Utc;
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::*;

pub(super) fn normalize_definition_payload(
    project: &ProjectRecord,
    params: &Map<String, Value>,
) -> Result<AutomationDefinitionRecord, DomainStateError> {
    let value = params
        .get("definition")
        .or_else(|| params.get("automation"))
        .cloned()
        .ok_or_else(|| DomainStateError::bad_request("Automation definition is required."))?;
    let object = value
        .as_object()
        .ok_or_else(|| DomainStateError::bad_request("Automation definition must be an object."))?;
    let now = now_iso();
    let id = read_value_text(object, "id")
        .unwrap_or_else(|| format!("automation-{}", uuid::Uuid::new_v4()));
    let name = read_value_text(object, "name")
        .ok_or_else(|| DomainStateError::bad_request("Automation name is required."))?;
    let agent_id = read_value_text(object, "agentId")
        .ok_or_else(|| DomainStateError::bad_request("Automation agentId is required."))?;
    let prompt = read_value_text(object, "prompt")
        .ok_or_else(|| DomainStateError::bad_request("Automation prompt is required."))?;
    let schedule = normalize_schedule(object.get("schedule"))?;
    let execution_mode = normalize_execution_mode(object.get("executionMode"))?;
    let project_id = object
        .get("projectIds")
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find_map(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| project.project_id.clone());
    let enabled = object
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let created_at = read_value_text(object, "createdAt").unwrap_or_else(|| now.clone());
    let next_run_at = if enabled {
        let next_run_at = if is_one_shot_schedule(&schedule) {
            compute_next_run_at(&schedule, None)
        } else {
            read_value_text(object, "nextRunAt").or_else(|| compute_next_run_at(&schedule, None))
        };
        Some(next_run_at.ok_or_else(|| {
            DomainStateError::bad_request(
                "Automation schedule must have a future run before it can be enabled.",
            )
        })?)
    } else {
        None
    };
    Ok(AutomationDefinitionRecord {
        agent_id,
        created_at,
        enabled,
        execution_mode,
        id,
        name,
        next_run_at,
        project_id,
        prompt,
        schedule,
        updated_at: now,
        owner_bot_id: None,
        precheck: None,
        deliver: None,
    })
}

pub(super) fn normalize_schedule(value: Option<&Value>) -> Result<Value, DomainStateError> {
    let schedule = value
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("Automation schedule is required."))?;
    let kind = read_value_text(schedule, "kind")
        .ok_or_else(|| DomainStateError::bad_request("Automation schedule kind is required."))?;
    match kind.as_str() {
        "once" => {
            let run_at = normalize_run_at(read_value_text(schedule, "runAt"))?;
            Ok(json!({ "kind": "once", "runAt": run_at }))
        }
        "timer" => {
            let delay_ms = schedule
                .get("delayMs")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 1_000 && *value <= 365 * 24 * 60 * 60 * 1000)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Timer schedule delayMs is invalid.")
                })?;
            let run_at = (Utc::now() + chrono::Duration::milliseconds(delay_ms))
                .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
            Ok(json!({ "kind": "once", "runAt": run_at }))
        }
        "interval" => {
            let every_ms = schedule
                .get("everyMs")
                .and_then(Value::as_i64)
                .filter(|value| *value >= 60_000 && *value <= 365 * 24 * 60 * 60 * 1000)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Interval schedule everyMs is invalid.")
                })?;
            Ok(json!({ "kind": "interval", "everyMs": every_ms }))
        }
        "daily" => {
            let time = normalize_time(read_value_text(schedule, "time"))?;
            Ok(json!({
                "kind": "daily",
                "time": time,
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        "weekly" => {
            let time = normalize_time(read_value_text(schedule, "time"))?;
            let days = schedule
                .get("days")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|value| value.as_u64())
                .filter(|value| *value <= 6)
                .collect::<HashSet<_>>();
            if days.is_empty() {
                return Err(DomainStateError::bad_request(
                    "Weekly schedule days are required.",
                ));
            }
            let mut days = days.into_iter().collect::<Vec<_>>();
            days.sort_unstable();
            Ok(json!({
                "days": days,
                "kind": "weekly",
                "time": time,
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        "cron" => {
            let expression = read_value_text(schedule, "expression")
                .ok_or_else(|| DomainStateError::bad_request("Cron expression is required."))?;
            Ok(json!({
                "expression": expression,
                "kind": "cron",
                "timezone": read_value_text(schedule, "timezone").unwrap_or_else(|| "local".to_string()),
            }))
        }
        _ => Err(DomainStateError::bad_request(
            "Unsupported automation schedule kind.",
        )),
    }
}

fn normalize_execution_mode(value: Option<&Value>) -> Result<Value, DomainStateError> {
    let mode = value
        .and_then(Value::as_object)
        .ok_or_else(|| DomainStateError::bad_request("Automation executionMode is required."))?;
    let kind = read_value_text(mode, "kind").unwrap_or_else(|| "local".to_string());
    match kind.as_str() {
        "local" => Ok(json!({ "kind": "local" })),
        "worktree" => Ok(json!({
            "kind": "worktree",
            "setupCommand": read_value_text(mode, "setupCommand"),
        })),
        "thread" => {
            let session_id = read_value_text(mode, "sessionId");
            let agent_session_id = read_value_text(mode, "agentSessionId");
            if session_id.is_none() && agent_session_id.is_none() {
                return Err(DomainStateError::bad_request(
                    "Thread executionMode requires sessionId or agentSessionId.",
                ));
            }
            Ok(json!({
                "agentSessionId": agent_session_id,
                "expiresAt": read_value_text(mode, "expiresAt"),
                "kind": "thread",
                "sessionId": session_id,
            }))
        }
        _ => Err(DomainStateError::bad_request(
            "Unsupported automation executionMode kind.",
        )),
    }
}
