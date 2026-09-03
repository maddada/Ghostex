use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;

pub(crate) const AGENT_SETTINGS_METADATA_KEY: &str = "agents.settings.v1";
pub(crate) const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
pub(crate) const MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH: usize = 120;
/*
CDXC:Drafts 2026-08-20:
`firstUserMessage` is a prompt gxserver submits for the user. A draft is the
opposite contract: text staged into the new agent's composer and never sent, so
the user writes their own prompt around it (the "Export transcript" result
dialog stages `@<exported markdown> ` this way). The status key is the
consume-once marker, mirroring `gxserverForkInitialRenameStatus`: created
`pending`, claimed by the first provider start, then `applied`/`failed`, so a
wake, re-attach, or daemon restart never retypes it.
*/
pub(crate) const FIRST_USER_INPUT_DRAFT_KEY: &str = "firstUserInputDraft";
pub(crate) const FIRST_USER_INPUT_DRAFT_STATUS_KEY: &str = "gxserverFirstUserInputDraftStatus";
pub(crate) const FIRST_USER_INPUT_DRAFT_UPDATED_AT_KEY: &str =
    "gxserverFirstUserInputDraftUpdatedAt";

pub(crate) fn read_agent_settings_with_metadata(
    db: &Connection,
) -> Result<Value, DomainStateError> {
    let row = read_agent_settings_metadata_value(db)?;
    let parsed = row.as_deref().map(parse_json_object);
    Ok(json!({
        "isPersisted": row.is_some(),
        "settings": normalize_agent_settings(parsed.as_ref()),
    }))
}

pub(crate) fn read_agent_settings_metadata_value(
    db: &Connection,
) -> Result<Option<String>, DomainStateError> {
    read_metadata_value(db, AGENT_SETTINGS_METADATA_KEY)
}

pub(crate) fn read_metadata_value(
    db: &Connection,
    key: &str,
) -> Result<Option<String>, DomainStateError> {
    db.query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
        row.get::<_, String>(0)
    })
    .optional()
    .map_err(sql_error)
}

pub(crate) fn read_agent_settings(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
    let value = read_agent_settings_with_metadata(db)?;
    Ok(object_field(&value, "settings"))
}

pub(crate) fn update_agent_settings(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut settings = read_agent_settings(db)?;
    if let Some(value) = params.get("agentAcceptAllEnabled").and_then(Value::as_bool) {
        settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(value));
    }
    if let Some(value) = params.get("defaultPromptAgentId").and_then(Value::as_str) {
        settings.insert(
            "defaultPromptAgentId".to_string(),
            Value::String(value.to_string()),
        );
    }
    let value = Value::Object(normalize_agent_settings(Some(&Value::Object(settings))));
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![
            AGENT_SETTINGS_METADATA_KEY,
            serde_json::to_string(&value).map_err(|error| {
                DomainStateError::bad_request(format!(
                    "Agent settings must be JSON serializable: {error}"
                ))
            })?,
            now_iso()
        ],
    )
    .map_err(sql_error)?;
    Ok(value)
}

pub(crate) fn normalize_agent_settings(value: Option<&Value>) -> Map<String, Value> {
    let object = value.and_then(Value::as_object);
    let accept_all = object
        .and_then(|settings| settings.get("agentAcceptAllEnabled"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let mut settings = Map::new();
    settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(accept_all));
    settings.insert(
        "defaultPromptAgentId".to_string(),
        Value::String(normalize_default_prompt_agent_id(
            object
                .and_then(|settings| settings.get("defaultPromptAgentId"))
                .and_then(Value::as_str),
        )),
    );
    settings
}

pub(crate) fn normalize_default_prompt_agent_id(value: Option<&str>) -> String {
    let trimmed = value.unwrap_or_default().trim();
    let normalized = if trimmed.is_empty() {
        DEFAULT_PROMPT_AGENT_ID
    } else {
        trimmed
    };
    normalized
        .chars()
        .take(MAX_DEFAULT_PROMPT_AGENT_ID_LENGTH)
        .collect()
}
