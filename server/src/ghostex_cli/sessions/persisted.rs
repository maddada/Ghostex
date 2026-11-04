use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_json_value, Flags};
use crate::ghostex_cli::rpc::{gxserver_root, CliError, GXSERVER_PRODUCT};

use super::*;

// ---------------------------------------------------------------------------
// persisted-state fallback (read-only SQLite)
// ---------------------------------------------------------------------------

pub(super) fn read_persisted_gxserver_session_list(
    cause: &CliError,
    flags: &Flags,
) -> Option<Value> {
    if !should_use_local_gxserver_state_fallback(flags) {
        return None;
    }
    let db_path = gxserver_root().join("state.db");
    if !db_path.exists() {
        return None;
    }
    let rows = read_persisted_gxserver_inventory_rows(&db_path, true)
        .or_else(|| read_persisted_gxserver_inventory_rows(&db_path, false))?;
    let server_id = read_persisted_gxserver_server_id();
    Some(build_persisted_gxserver_session_list(
        &rows,
        server_id.as_deref(),
        &cause.to_string(),
        flags,
    ))
}

pub(super) fn build_persisted_gxserver_session_list(
    rows: &[Value],
    server_id: Option<&str>,
    error_message: &str,
    flags: &Flags,
) -> Value {
    let projects: Vec<Value> = rows
        .iter()
        .filter(|row| row.get("rowType").and_then(Value::as_str) == Some("project"))
        .map(|row| {
            let mut map = Map::new();
            let is_recent = row
                .get("isRecentProject")
                .map(|value| value.as_i64() == Some(1) || value == &Value::Bool(true))
                .unwrap_or(false);
            map.insert("isRecentProject".to_string(), json!(is_recent));
            insert_present(&mut map, "name", row.get("name"));
            insert_present(&mut map, "path", row.get("path"));
            insert_present(&mut map, "projectId", row.get("projectId"));
            insert_non_null(&mut map, "systemKind", row.get("systemKind"));
            map.insert(
                "visibility".to_string(),
                match row.get("visibility") {
                    Some(value) if !value.is_null() => value.clone(),
                    _ => json!("visible"),
                },
            );
            Value::Object(map)
        })
        .filter(is_active_gxserver_inventory_project)
        .collect();
    let mut project_by_id: HashMap<Option<String>, &Value> = HashMap::new();
    for project in &projects {
        project_by_id.insert(value_key(project.get("projectId")), project);
    }
    let sessions: Vec<Value> = rows
        .iter()
        .filter(|row| row.get("rowType").and_then(Value::as_str) == Some("session"))
        .filter(|row| project_by_id.contains_key(&value_key(row.get("projectId"))))
        .map(|row| {
            let mut map = Map::new();
            insert_non_null(&mut map, "agentId", row.get("agentId"));
            insert_non_null(&mut map, "cwd", row.get("cwd"));
            let server_truthy = server_id.map(|id| !id.is_empty()).unwrap_or(false);
            if server_truthy && js_truthy(row.get("projectId")) && js_truthy(row.get("sessionId")) {
                map.insert(
                    "globalRef".to_string(),
                    json!(format!(
                        "{}:{}:{}",
                        server_id.unwrap_or_default(),
                        js_template(row.get("projectId")),
                        js_template(row.get("sessionId"))
                    )),
                );
            }
            insert_present(&mut map, "kind", row.get("kind"));
            insert_non_null(&mut map, "lastActiveAt", row.get("lastActiveAt"));
            insert_present(&mut map, "lifecycleState", row.get("lifecycleState"));
            insert_present(&mut map, "projectId", row.get("projectId"));
            map.insert(
                "providerState".to_string(),
                parse_persisted_provider_state(row.get("providerStateJson")),
            );
            insert_present(&mut map, "sessionId", row.get("sessionId"));
            insert_present(&mut map, "title", row.get("title"));
            insert_present(&mut map, "updatedAt", row.get("updatedAt"));
            insert_present(&mut map, "zmxName", row.get("zmxName"));
            Value::Object(map)
        })
        .collect();
    let listed_sessions: Vec<&Value> = if should_include_stopped_gxserver_sessions(flags) {
        sessions.iter().collect()
    } else {
        sessions
            .iter()
            .filter(|session| !is_stopped_gxserver_session(session))
            .collect()
    };
    /*
     * CDXC:Cli 2026-06-03-20:28:
     * Bridge-down session inventory degrades from gxserver's own durable state.
     * Keep the fallback read-only and visibly marked so humans and Android can
     * distinguish stale persisted rows from live daemon data.
     */
    let cli_sessions: Vec<Value> = listed_sessions
        .iter()
        .enumerate()
        .map(|(index, session)| {
            to_cli_session(
                session,
                project_by_id
                    .get(&value_key(session.get("projectId")))
                    .copied(),
                index,
                None,
                None,
            )
        })
        .collect();
    json!({
        "error": error_message,
        "fallback": "persisted-gxserver-state",
        "ok": true,
        "product": GXSERVER_PRODUCT,
        "projects": projects,
        "sessions": cli_sessions,
    })
}

fn parse_persisted_provider_state(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => parse_json_value(text)
            .filter(|parsed| !parsed.is_null())
            .unwrap_or_else(|| json!({})),
        Some(Value::Number(number)) => Value::Number(number.clone()),
        Some(Value::Bool(flag)) => Value::Bool(*flag),
        _ => json!({}),
    }
}

fn read_persisted_gxserver_inventory_rows(
    db_path: &Path,
    include_project_visibility_columns: bool,
) -> Option<Vec<Value>> {
    let connection = ghostex_find::agent_store::open_agent_store(db_path).ok()?;
    read_persisted_gxserver_inventory_rows_from(&connection, include_project_visibility_columns)
}

pub(super) fn read_persisted_gxserver_inventory_rows_from(
    connection: &rusqlite::Connection,
    include_project_visibility_columns: bool,
) -> Option<Vec<Value>> {
    let sql = create_persisted_gxserver_inventory_sql(include_project_visibility_columns);
    let mut statement = connection.prepare(&sql).ok()?;
    let column_names: Vec<String> = statement
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();
    let mut rows = statement.query([]).ok()?;
    let mut result: Vec<Value> = Vec::new();
    loop {
        match rows.next() {
            Ok(Some(row)) => {
                let mut map = Map::new();
                for (index, name) in column_names.iter().enumerate() {
                    let value = match row.get_ref(index) {
                        Ok(value) => value,
                        Err(_) => return None,
                    };
                    map.insert(name.clone(), sqlite_value_to_json(value));
                }
                result.push(Value::Object(map));
            }
            Ok(None) => break,
            Err(_) => return None,
        }
    }
    Some(result)
}

fn sqlite_value_to_json(value: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef;
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => serde_json::Number::from_f64(number)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(text) => Value::String(String::from_utf8_lossy(text).into_owned()),
        ValueRef::Blob(blob) => Value::String(String::from_utf8_lossy(blob).into_owned()),
    }
}

fn create_persisted_gxserver_inventory_sql(include_project_visibility_columns: bool) -> String {
    let project_visibility_columns = if include_project_visibility_columns {
        "isRecentProject, visibility, systemKind"
    } else {
        "isRecentProject, 'visible' AS visibility, NULL AS systemKind"
    };
    [
        format!("SELECT 'project' AS rowType, projectId, name, path, {project_visibility_columns}, NULL AS sessionId, NULL AS kind, NULL AS title, NULL AS lifecycleState, NULL AS providerStateJson, NULL AS zmxName, NULL AS cwd, NULL AS agentId, NULL AS updatedAt, NULL AS lastActiveAt FROM projects"),
        "UNION ALL".to_string(),
        "SELECT 'session' AS rowType, projectId, NULL AS name, NULL AS path, NULL AS isRecentProject, NULL AS visibility, NULL AS systemKind, sessionId, kind, title, lifecycleState, providerStateJson, zmxName, cwd, agentId, updatedAt, lastActiveAt FROM sessions".to_string(),
        "ORDER BY rowType ASC, updatedAt DESC, projectId ASC, sessionId ASC".to_string(),
    ]
    .join(" ")
}
