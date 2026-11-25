use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;

pub(crate) const AGENT_SETTINGS_METADATA_KEY: &str = "agents.settings.v1";
pub(crate) const DEFAULT_PROMPT_AGENT_ID: &str = "claude";
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

fn read_persisted_agent_settings(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
    let value = read_agent_settings_with_metadata(db)?;
    Ok(object_field(&value, "settings"))
}

fn read_persisted_accept_all_enabled(db: &Connection) -> Result<Option<bool>, DomainStateError> {
    let Some(row) = read_agent_settings_metadata_value(db)? else {
        return Ok(None);
    };
    let parsed = parse_json_object(&row);
    Ok(parsed.get("agentAcceptAllEnabled").and_then(Value::as_bool))
}

pub(crate) fn read_shared_settings_accept_all_enabled(app_config_dir: &Path) -> Option<bool> {
    crate::shared_settings_file::read_shared_settings_in(app_config_dir)?
        .get("agentAcceptAllEnabled")
        .and_then(Value::as_bool)
}

fn app_config_dir_for_db(db: &Connection) -> Option<PathBuf> {
    let db_path = db
        .path()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)?;
    let default_paths = crate::paths::get_gxserver_paths(None);
    let matches_default = db_path == default_paths.state_db_file
        || std::fs::canonicalize(&db_path)
            .ok()
            .zip(std::fs::canonicalize(&default_paths.state_db_file).ok())
            .is_some_and(|(a, b)| a == b);
    if matches_default {
        return Some(default_paths.app_config_dir);
    }
    // Isolated profile (`GhostexPaths::unified`, used by tests and `GHOSTEX_HOME`):
    // `state_db_file` is `<root>/state/gxserver/state.db` and `app_config_dir` is `<root>`.
    let gxserver_dir = db_path.parent()?;
    let state_dir = gxserver_dir.parent()?;
    if gxserver_dir.file_name() == Some(OsStr::new("gxserver"))
        && state_dir.file_name() == Some(OsStr::new("state"))
    {
        return state_dir.parent().map(PathBuf::from);
    }
    None
}

/// CDXC:Settings 2026-11-24 WHY:
/// Issue #744: on a computer without the desktop app, `ghostex settings set` (`headless_settings.rs`) writes `native-sidebar-settings.json` while SQLite `agents.settings.v1` has no row yet (only the desktop app calls `/api/updateAgentSettings`), so launch and resume read `agentAcceptAllEnabled` as off while the security checks already fell back to the settings file. One shared reader resolves both stores so launch, resume, coordinator thread start, prompt search, and security gates always agree: when only one store sets `agentAcceptAllEnabled` (the headless case has `file = Some(...)`, `db = None`), that store wins; when both explicitly set it and disagree, `false` wins so turning Accept all off through either the settings file or `/api/updateAgentSettings` takes effect immediately instead of staying stuck on while the other copy lags (`headless_settings::save_settings_patch` also updates any already-persisted `agents.settings.v1` row so a stale DB `false` never blocks turning it on headlessly).
/// SEE-ALSO: server/src/acp/approval.rs (`global_accept_all_enabled_in_shared_settings`), server/src/headless_settings.rs, server/src/agents/launch_plan.rs, server/src/agents/resume_plan.rs.
pub(crate) fn resolve_global_accept_all_enabled(
    file_value: Option<bool>,
    db_persisted_value: Option<bool>,
) -> bool {
    match (file_value, db_persisted_value) {
        (Some(file), Some(db)) => file && db,
        (Some(file), None) => file,
        (None, Some(db)) => db,
        (None, None) => false,
    }
}

pub(crate) fn read_global_accept_all_enabled_for_db(
    db: &Connection,
) -> Result<bool, DomainStateError> {
    let db_value = read_persisted_accept_all_enabled(db)?;
    let file_value = app_config_dir_for_db(db)
        .as_deref()
        .and_then(read_shared_settings_accept_all_enabled);
    Ok(resolve_global_accept_all_enabled(file_value, db_value))
}

pub(crate) fn read_global_accept_all_enabled_for_paths(
    paths: &crate::paths::GxserverPaths,
) -> bool {
    let file_value = read_shared_settings_accept_all_enabled(&paths.app_config_dir);
    let db_value = crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| read_persisted_accept_all_enabled(&db).ok().flatten());
    resolve_global_accept_all_enabled(file_value, db_value)
}

pub(crate) fn read_agent_settings(db: &Connection) -> Result<Map<String, Value>, DomainStateError> {
    let mut settings = read_persisted_agent_settings(db)?;
    let accept_all = read_global_accept_all_enabled_for_db(db)?;
    settings.insert("agentAcceptAllEnabled".to_string(), Value::Bool(accept_all));
    Ok(settings)
}

pub(crate) fn update_agent_settings(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let mut settings = read_persisted_agent_settings(db)?;
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
        .unwrap_or(false);
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
