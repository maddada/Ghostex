use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/*
CDXC:Sessions 2026-07-12-00:00:
Named session sub-groups and the sidebar project order used to live only in
GPUI's localStorage overlay, so React Native Android could not render the same
grouped, ordered session list. gxserver now owns a durable copy of that
overlay state in the metadata table. GPUI stays the only editor and keeps its
local overlay for instant interaction; it write-through-syncs the whole
normalized state here so every client (mobile summaries, CLI, future
sidebars) reads one contract. Keep this metadata-only: group ids, titles,
session ids, and project order — never paths, prompts, command text, tokens,
or terminal output.
*/

const WORKSPACE_SESSION_GROUPS_METADATA_KEY: &str = "workspaceSessionGroups";
const MAX_PROJECT_ENTRIES: usize = 512;
const MAX_GROUPS_PER_PROJECT: usize = 20;
const MAX_SESSION_IDS_PER_GROUP: usize = 1024;
const MAX_ID_CHARS: usize = 256;
const MAX_TITLE_CHARS: usize = 256;

pub fn empty_workspace_session_groups_state() -> Value {
    json!({
        "projectOrder": [],
        "projects": {},
    })
}

pub fn read_workspace_session_groups(db: &Connection) -> Result<Value, DomainStateError> {
    let stored = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [WORKSPACE_SESSION_GROUPS_METADATA_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite workspace groups error: {error}"),
        })?;
    let parsed = stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .unwrap_or_else(empty_workspace_session_groups_state);
    Ok(normalize_workspace_session_groups_state(&parsed))
}

pub fn update_workspace_session_groups(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let state = params
        .get("state")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Workspace session groups update requires a state object.",
            )
        })?;
    let normalized = normalize_workspace_session_groups_state(state);
    let serialized = serde_json::to_string(&normalized).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Workspace groups serialization error: {error}"),
    })?;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![WORKSPACE_SESSION_GROUPS_METADATA_KEY, serialized, now_iso()],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite workspace groups error: {error}"),
    })?;
    Ok(normalized)
}

fn normalize_workspace_session_groups_state(state: &Value) -> Value {
    let project_order = state
        .get("projectOrder")
        .and_then(Value::as_array)
        .map(|entries| normalized_id_list(entries, MAX_PROJECT_ENTRIES))
        .unwrap_or_default();
    let mut projects = Map::new();
    if let Some(entries) = state.get("projects").and_then(Value::as_object) {
        for (project_id, project_state) in entries.iter().take(MAX_PROJECT_ENTRIES) {
            let project_id = project_id.trim();
            if project_id.is_empty() || project_id.chars().count() > MAX_ID_CHARS {
                continue;
            }
            let Some(normalized) = normalize_project_workspace_groups(project_state) else {
                continue;
            };
            projects.insert(project_id.to_string(), normalized);
        }
    }
    json!({
        "projectOrder": project_order,
        "projects": projects,
    })
}

fn normalize_project_workspace_groups(project_state: &Value) -> Option<Value> {
    let groups = project_state.get("groups").and_then(Value::as_array)?;
    let mut normalized_groups = Vec::new();
    let mut seen_group_ids = std::collections::HashSet::new();
    for group in groups.iter().take(MAX_GROUPS_PER_PROJECT) {
        let Some(group_id) = trimmed_bounded_text(group.get("groupId"), MAX_ID_CHARS) else {
            continue;
        };
        if !seen_group_ids.insert(group_id.clone()) {
            continue;
        }
        let title = trimmed_bounded_text(group.get("title"), MAX_TITLE_CHARS)
            .unwrap_or_else(|| group_id.clone());
        let session_ids = group
            .get("sessionIds")
            .and_then(Value::as_array)
            .map(|entries| normalized_id_list(entries, MAX_SESSION_IDS_PER_GROUP))
            .unwrap_or_default();
        normalized_groups.push(json!({
            "groupId": group_id,
            "sessionIds": session_ids,
            "title": title,
        }));
    }
    let next_group_number = project_state
        .get("nextGroupNumber")
        .and_then(Value::as_i64)
        .filter(|value| *value >= 2 && *value <= 1_000_000)
        .unwrap_or(2);
    if normalized_groups.is_empty() && next_group_number == 2 {
        return None;
    }
    Some(json!({
        "groups": normalized_groups,
        "nextGroupNumber": next_group_number,
    }))
}

fn normalized_id_list(entries: &[Value], max_entries: usize) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for entry in entries {
        if ids.len() >= max_entries {
            break;
        }
        let Some(id) = trimmed_bounded_text(Some(entry), MAX_ID_CHARS) else {
            continue;
        };
        if seen.insert(id.clone()) {
            ids.push(id);
        }
    }
    ids
}

fn trimmed_bounded_text(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let text = value?.as_str()?.trim();
    if text.is_empty() || text.chars().count() > max_chars {
        return None;
    }
    Some(text.to_string())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
