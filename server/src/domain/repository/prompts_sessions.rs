use rusqlite::Connection;
use serde_json::{json, Value};

use crate::domain::{
    normalize_domain_lifecycle_state, normalize_zmx_provider_state, parse_object_map, sql_error,
    DomainResult,
};

/// CDXC:SavedPrompts 2026-09-11 WHY:
/// Saved prompt jumps need session identities, titles and ownership, but full registry hydration also decodes unrelated launch settings, rules and worktrees on every list or deletion.
/// Retain the canonical lifecycle normalizers and registry ordering so resumed conversation owners and equal-timestamp ties resolve identically to list_sessions.
pub(super) fn read_stashed_prompt_sessions(db: &Connection) -> DomainResult<Vec<Value>> {
    let mut statement = db
        .prepare_cached(
            r#"
            SELECT projectId, sessionId, title, lifecycleState,
                   CASE WHEN NOT json_valid(providerStateJson) THEN providerStateJson
                        WHEN json_type(providerStateJson) <> 'object' THEN providerStateJson
                        ELSE (SELECT json_group_object(key,
                            CASE WHEN type IN ('array', 'object') THEN json(value)
                                 WHEN type = 'true' THEN json('true')
                                 WHEN type = 'false' THEN json('false') ELSE value END)
                            FROM json_each(providerStateJson) WHERE key = 'lifecycleState')
                   END,
                   CASE WHEN NOT json_valid(runtimeSettingsJson) THEN runtimeSettingsJson
                        WHEN json_type(runtimeSettingsJson) <> 'object' THEN runtimeSettingsJson
                        ELSE (SELECT json_group_object(key,
                            CASE WHEN type IN ('array', 'object') THEN json(value)
                                 WHEN type = 'true' THEN json('true')
                                 WHEN type = 'false' THEN json('false') ELSE value END)
                            FROM json_each(runtimeSettingsJson) WHERE key = 'agentSessionId')
                   END
            FROM sessions
            ORDER BY updatedAt DESC, projectId ASC, sessionId ASC
            "#,
        )
        .map_err(sql_error)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(sql_error)?;
    rows.map(|row| {
        let (project_id, session_id, title, lifecycle, provider, runtime) = row.map_err(sql_error)?;
        let row_id = format!("{project_id}/{session_id}");
        let provider = parse_object_map(&provider, "providerStateJson", "session", &row_id)?;
        let runtime = parse_object_map(&runtime, "runtimeSettingsJson", "session", &row_id)?;
        Ok(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "title": title,
            "lifecycleState": normalize_domain_lifecycle_state(Some(&Value::String(lifecycle))),
            "providerState": { "lifecycleState": normalize_zmx_provider_state(provider, "").remove("lifecycleState") },
            "runtimeSettings": runtime,
        }))
    })
    .collect()
}
