use serde_json::{Map, Value};

use crate::domain::normalize::fields::{
    insert_optional_object, insert_optional_string, normalize_object, normalize_object_array,
    normalize_required_text, normalize_string_array, read_optional_text, update_object_array_field,
    update_object_field, update_optional_object_field, update_optional_text_field,
    update_string_array_field,
};
use crate::domain::{DomainResult, DomainStateError};

pub(crate) fn normalize_project_input(
    project_id: &str,
    timestamp: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let mut project = Map::new();
    project.insert(
        "attentionRules".to_string(),
        Value::Object(normalize_object(input.get("attentionRules"))),
    );
    project.insert(
        "completionRules".to_string(),
        Value::Object(normalize_object(input.get("completionRules"))),
    );
    project.insert(
        "createdAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    project.insert(
        "customAgentOrder".to_string(),
        Value::Array(
            normalize_string_array(input.get("customAgentOrder"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "customAgents".to_string(),
        Value::Array(normalize_object_array(input.get("customAgents"))),
    );
    project.insert(
        "customCommandOrder".to_string(),
        Value::Array(
            normalize_string_array(input.get("customCommandOrder"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "customCommands".to_string(),
        Value::Array(normalize_object_array(input.get("customCommands"))),
    );
    insert_optional_string(
        &mut project,
        "defaultCommand",
        read_optional_text(input.get("defaultCommand")),
    );
    project.insert(
        "deletedDefaultCommandIds".to_string(),
        Value::Array(
            normalize_string_array(input.get("deletedDefaultCommandIds"))
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
    );
    project.insert(
        "gitConfig".to_string(),
        Value::Object(normalize_object(input.get("gitConfig"))),
    );
    insert_optional_object(
        &mut project,
        "identityIcon",
        normalize_object(input.get("identityIcon")),
    );
    project.insert(
        "isFavorite".to_string(),
        Value::Bool(input.get("isFavorite").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "isPinned".to_string(),
        Value::Bool(input.get("isPinned").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "isRecentProject".to_string(),
        Value::Bool(input.get("isRecentProject").and_then(Value::as_bool) == Some(true)),
    );
    project.insert(
        "launchSettings".to_string(),
        Value::Object(normalize_object(input.get("launchSettings"))),
    );
    project.insert(
        "name".to_string(),
        Value::String(normalize_required_text(input.get("name"), "name")?),
    );
    project.insert(
        "notificationRules".to_string(),
        Value::Object(normalize_object(input.get("notificationRules"))),
    );
    insert_optional_string(&mut project, "path", read_optional_text(input.get("path")));
    project.insert(
        "previousSessionHistory".to_string(),
        Value::Array(normalize_object_array(input.get("previousSessionHistory"))),
    );
    project.insert(
        "projectBoardConfig".to_string(),
        Value::Object(normalize_object(input.get("projectBoardConfig"))),
    );
    project.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    insert_optional_string(
        &mut project,
        "recentClosedAt",
        read_optional_text(input.get("recentClosedAt")),
    );
    project.insert(
        "runtimeSettings".to_string(),
        Value::Object(normalize_object(input.get("runtimeSettings"))),
    );
    insert_optional_string(
        &mut project,
        "systemKind",
        normalize_project_system_kind(input.get("systemKind"))?,
    );
    project.insert(
        "updatedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    project.insert(
        "visibility".to_string(),
        Value::String(normalize_project_visibility(input.get("visibility"))?),
    );
    insert_optional_object(
        &mut project,
        "worktree",
        normalize_object(input.get("worktree")),
    );
    Ok(Value::Object(project))
}

pub(crate) fn merge_project_update(
    current: Value,
    updated_at: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let current = current.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Project row did not decode as an object.")
    })?;
    let mut next = current.clone();
    update_object_field(&mut next, input, "attentionRules");
    update_object_field(&mut next, input, "completionRules");
    update_string_array_field(&mut next, input, "customAgentOrder");
    update_object_array_field(&mut next, input, "customAgents");
    update_string_array_field(&mut next, input, "customCommandOrder");
    update_object_array_field(&mut next, input, "customCommands");
    update_optional_text_field(&mut next, input, "defaultCommand");
    update_string_array_field(&mut next, input, "deletedDefaultCommandIds");
    update_object_field(&mut next, input, "gitConfig");
    update_optional_object_field(&mut next, input, "identityIcon");
    if let Some(value) = input.get("isFavorite") {
        next.insert(
            "isFavorite".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    if let Some(value) = input.get("isPinned") {
        next.insert(
            "isPinned".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    if let Some(value) = input.get("isRecentProject") {
        next.insert(
            "isRecentProject".to_string(),
            Value::Bool(value.as_bool() == Some(true)),
        );
    }
    update_object_field(&mut next, input, "launchSettings");
    if input.contains_key("name") {
        next.insert(
            "name".to_string(),
            Value::String(normalize_required_text(input.get("name"), "name")?),
        );
    }
    update_object_field(&mut next, input, "notificationRules");
    update_optional_text_field(&mut next, input, "path");
    update_object_array_field(&mut next, input, "previousSessionHistory");
    update_object_field(&mut next, input, "projectBoardConfig");
    update_optional_text_field(&mut next, input, "recentClosedAt");
    update_object_field(&mut next, input, "runtimeSettings");
    if input.contains_key("systemKind") {
        insert_optional_string(
            &mut next,
            "systemKind",
            normalize_project_system_kind(input.get("systemKind"))?,
        );
    }
    if input.contains_key("visibility") {
        next.insert(
            "visibility".to_string(),
            Value::String(normalize_project_visibility(input.get("visibility"))?),
        );
    }
    update_optional_object_field(&mut next, input, "worktree");
    next.insert(
        "updatedAt".to_string(),
        Value::String(updated_at.to_string()),
    );
    Ok(Value::Object(next))
}

pub(crate) fn normalize_project_visibility(value: Option<&Value>) -> DomainResult<String> {
    match normalize_optional_project_enum_text(value, "visibility")?.as_deref() {
        None => Ok("visible".to_string()),
        Some("visible") => Ok("visible".to_string()),
        Some("hidden") => Ok("hidden".to_string()),
        Some(_) => Err(DomainStateError::bad_request(
            "visibility must be either visible or hidden.",
        )),
    }
}

pub(crate) fn normalize_project_system_kind(value: Option<&Value>) -> DomainResult<Option<String>> {
    match normalize_optional_project_enum_text(value, "systemKind")?.as_deref() {
        None => Ok(None),
        Some("remoteAttachCarrier") => Ok(Some("remoteAttachCarrier".to_string())),
        Some(_) => Err(DomainStateError::bad_request(
            "systemKind must be remoteAttachCarrier when provided.",
        )),
    }
}

fn normalize_optional_project_enum_text(
    value: Option<&Value>,
    field: &str,
) -> DomainResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(value
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string())
        .map(|value| if value.is_empty() { None } else { Some(value) }),
        Some(_) => Err(DomainStateError::bad_request(format!(
            "{field} must be a string when provided."
        ))),
    }
}
