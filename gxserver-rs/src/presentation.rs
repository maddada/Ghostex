use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

/*
CDXC:GxserverRustPort 2026-06-14-22:12:
Phase 3 presentation is a read-only projection over the durable project/session repository. Keep it metadata-only and camelCase so sidebar inventory can compare Rust and TypeScript without moving pane layout, terminal text, prompts, or other client-local/private state into gxserver.
*/
pub fn read_presentation_snapshot(
    db: &Connection,
    server_id: &str,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    Ok(project_snapshot(
        repository.list_projects()?,
        repository.list_sessions(None)?,
        read_presentation_revision(db)?,
    ))
}

pub fn search_presentation_sessions(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    let projects = repository.list_projects()?;
    let sessions = repository.list_sessions(None)?;
    Ok(search_sessions(projects, sessions, params))
}

pub fn list_previous_sessions(
    db: &Connection,
    server_id: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let repository = DomainRepository::new(db, server_id);
    let projects = repository.list_projects()?;
    let sessions = repository.list_sessions(None)?;
    let mut previous_params = params.clone();
    previous_params.insert("includeActive".to_string(), Value::Bool(false));
    previous_params.insert("includePrevious".to_string(), Value::Bool(true));
    Ok(search_previous_sessions(
        projects,
        sessions,
        &previous_params,
    ))
}

pub fn build_presentation_project_delta(
    repository: &DomainRepository<'_>,
    project_id: &str,
    delta_type: &str,
) -> Result<Value, DomainStateError> {
    let Some(project) = repository.get_project(project_id)? else {
        return Ok(json!({
            "projectId": project_id,
            "type": "projectRemoved",
        }));
    };
    Ok(json!({
        "domainProject": project,
        "project": project_presentation_project(&project),
        "type": delta_type,
    }))
}

pub fn build_presentation_session_delta(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let project = repository.get_project(project_id)?;
    let session = repository.get_session(project_id, session_id)?;
    let (Some(project), Some(session)) = (project, session) else {
        return Ok(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": "sessionRemoved",
        }));
    };
    if !should_include_presentation_session(&session) {
        return Ok(json!({
            "projectId": project_id,
            "sessionId": session_id,
            "type": "sessionRemoved",
        }));
    }
    Ok(json!({
        "session": project_presentation_session(
            &project,
            &default_group_id(project_id),
            &session,
            &now_iso(),
        ),
        "type": "sessionPresentationChanged",
    }))
}

pub fn increment_presentation_revision(db: &Connection) -> Result<i64, DomainStateError> {
    let next_revision = read_presentation_revision(db)? + 1;
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params!["presentationRevision", next_revision.to_string(), now_iso()],
    )
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite presentation error: {error}"),
    })?;
    Ok(next_revision)
}

pub fn read_presentation_revision(db: &Connection) -> Result<i64, DomainStateError> {
    let value = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            ["presentationRevision"],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite presentation error: {error}"),
        })?;
    Ok(value
        .and_then(|text| text.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1))
}

fn project_snapshot(projects: Vec<Value>, sessions: Vec<Value>, revision: i64) -> Value {
    let generated_at = now_iso();
    let mut projects_sorted = projects;
    projects_sorted.sort_by(|left, right| project_sort_key(left).cmp(&project_sort_key(right)));
    let mut presentation_projects = Vec::new();
    let mut groups = Vec::new();
    let mut presentation_sessions = Vec::new();
    for project in projects_sorted {
        let project_id = string_field(&project, "projectId").unwrap_or_default();
        let group_id = default_group_id(&project_id);
        let mut project_sessions = sessions
            .iter()
            .filter(|session| {
                string_field(session, "projectId").as_deref() == Some(project_id.as_str())
            })
            .filter(|session| should_include_presentation_session(session))
            .cloned()
            .collect::<Vec<_>>();
        project_sessions
            .sort_by(|left, right| session_sort_key(left).cmp(&session_sort_key(right)));
        let project_presentation_sessions = project_sessions
            .into_iter()
            .map(|session| {
                project_presentation_session(&project, &group_id, &session, &generated_at)
            })
            .collect::<Vec<_>>();
        groups.push(json!({
            "groupId": group_id,
            "projectId": project_id,
            "sessionIds": project_presentation_sessions
                .iter()
                .filter_map(|session| string_field(session, "sessionId"))
                .collect::<Vec<_>>(),
            "sortKey": format!("{}:active", project_sort_key(&project)),
            "title": "Active",
        }));
        presentation_projects.push(project_presentation_project(&project));
        presentation_sessions.extend(project_presentation_sessions);
    }
    json!({
        "generatedAt": generated_at,
        "groups": groups,
        "projects": presentation_projects,
        "revision": revision,
        "sessions": presentation_sessions,
    })
}

fn project_presentation_project(project: &Value) -> Value {
    let project_id = string_field(project, "projectId").unwrap_or_default();
    let mut output = Map::new();
    output.insert("createdAt".to_string(), value_field(project, "createdAt"));
    output.insert(
        "groupIds".to_string(),
        json!([default_group_id(&project_id)]),
    );
    output.insert("isFavorite".to_string(), value_field(project, "isFavorite"));
    output.insert("isPinned".to_string(), value_field(project, "isPinned"));
    insert_optional_value(&mut output, "path", project.get("path").cloned());
    output.insert("projectId".to_string(), Value::String(project_id.clone()));
    output.insert(
        "sortKey".to_string(),
        Value::String(project_sort_key(project)),
    );
    output.insert("title".to_string(), value_field(project, "name"));
    output.insert("updatedAt".to_string(), value_field(project, "updatedAt"));
    insert_optional_value(&mut output, "worktree", project.get("worktree").cloned());
    Value::Object(output)
}

fn project_presentation_session(
    project: &Value,
    group_id: &str,
    session: &Value,
    generated_at: &str,
) -> Value {
    let title = project_session_title(session);
    let activity = presentation_activity(session, generated_at);
    let lifecycle_state = effective_lifecycle_state(session);
    let subtitle = string_field(session, "cwd").or_else(|| string_field(project, "path"));
    let mut output = Map::new();
    output.insert(
        "actions".to_string(),
        presentation_actions(session, &activity),
    );
    output.insert("activity".to_string(), Value::String(activity.clone()));
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName").or_else(|| string_field(session, "agentId")),
    );
    if let Some(agent_id) = string_field(session, "agentId") {
        output.insert("agentId".to_string(), Value::String(agent_id.clone()));
        output.insert("agentIcon".to_string(), Value::String(agent_id));
    }
    insert_optional_string(
        &mut output,
        "agentSessionId",
        read_runtime_text(session, "agentSessionId"),
    );
    insert_optional_string(
        &mut output,
        "agentSessionPath",
        read_runtime_text(session, "agentSessionPath"),
    );
    if activity == "attention" {
        output.insert("attention".to_string(), attention_state(session));
    }
    output.insert("createdAt".to_string(), value_field(session, "createdAt"));
    insert_optional_value(&mut output, "cwd", session.get("cwd").cloned());
    output.insert("groupId".to_string(), Value::String(group_id.to_string()));
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    output.insert(
        "isGeneratingFirstPromptTitle".to_string(),
        Value::Bool(
            read_runtime_text(session, "gxserverFirstPromptAutoTitleStatus").as_deref()
                == Some("running"),
        ),
    );
    if session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("gxserverFirstPromptAutoTitleShouldSubmitStagedCommand"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        output.insert(
            "shouldSubmitStagedFirstPromptTitleCommand".to_string(),
            Value::Bool(true),
        );
    }
    output.insert("isPinned".to_string(), value_field(session, "isPinned"));
    merge_object(&mut output, title);
    output.insert("kind".to_string(), value_field(session, "kind"));
    output.insert(
        "lastActiveAt".to_string(),
        Value::String(last_active_at(session)),
    );
    output.insert("lifecycleState".to_string(), Value::String(lifecycle_state));
    if let Some(provider_state) = read_provider_text(session, "lifecycleState") {
        output.insert(
            "providerSessionState".to_string(),
            Value::String(provider_state),
        );
    }
    output.insert("projectId".to_string(), value_field(session, "projectId"));
    output.insert("sessionId".to_string(), value_field(session, "sessionId"));
    if let Some(provider) = search_session_persistence_provider(session) {
        output.insert(
            "sessionPersistenceProvider".to_string(),
            Value::String(provider),
        );
    }
    insert_optional_value(
        &mut output,
        "sessionTag",
        session.get("sessionTag").cloned(),
    );
    insert_optional_value(
        &mut output,
        "sidebarOrder",
        session.get("sidebarOrder").cloned(),
    );
    output.insert(
        "sortKey".to_string(),
        Value::String(session_sort_key(session)),
    );
    insert_optional_string(&mut output, "subtitle", subtitle);
    output.insert("surface".to_string(), value_field(session, "surface"));
    output.insert("updatedAt".to_string(), value_field(session, "updatedAt"));
    output.insert(
        "visibleInSidebarByDefault".to_string(),
        Value::Bool(
            string_field(session, "surface").as_deref() == Some("workspace") && is_active(session),
        ),
    );
    output.insert("zmxName".to_string(), value_field(session, "zmxName"));
    Value::Object(output)
}

fn search_session_persistence_provider(session: &Value) -> Option<String> {
    let value = read_runtime_text(session, "sessionPersistenceProvider")
        .or_else(|| read_provider_text(session, "provider"))?;
    matches!(value.as_str(), "tmux" | "zmx" | "zellij").then_some(value)
}

fn search_session_persistence_name(session: &Value, provider: &str) -> Option<String> {
    if provider == "zmx" {
        return read_provider_text(session, "zmxName").or_else(|| string_field(session, "zmxName"));
    }
    read_provider_text(session, "providerName")
        .or_else(|| read_runtime_text(session, "sessionPersistenceName"))
}

fn search_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Value {
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_ascii_lowercase();
    let project_id_filter = params.get("projectId").and_then(Value::as_str);
    let tags = params
        .get("sessionTags")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut candidates = sessions
        .into_iter()
        .filter(|session| {
            project_id_filter
                .map(|project_id| string_field(session, "projectId").as_deref() == Some(project_id))
                .unwrap_or(true)
        })
        .filter(|session| {
            tags.is_empty()
                || string_field(session, "sessionTag")
                    .map(|tag| tags.iter().any(|expected| *expected == tag))
                    .unwrap_or(false)
        })
        .filter(|session| {
            let active = is_active(session);
            (active && include_active) || (!active && include_previous)
        })
        .filter_map(|session| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            match_session(project, &session, &query).map(|matched| (session, matched))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left, _), (right, _)| {
        last_active_at(right)
            .cmp(&last_active_at(left))
            .then_with(|| string_field(left, "sessionId").cmp(&string_field(right, "sessionId")))
    });
    let total = candidates.len();
    let page = candidates
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(session, matched)| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            search_result(project, &session, matched)
        })
        .collect::<Vec<_>>();
    let mut output = Map::new();
    if offset + limit < total {
        output.insert(
            "cursor".to_string(),
            Value::String((offset + limit).to_string()),
        );
    }
    output.insert("results".to_string(), Value::Array(page));
    Value::Object(output)
}

fn search_previous_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Value {
    /*
    CDXC:PreviousSessions 2026-06-19-14:30:
    Rust listPreviousSessions must be the same previous-only restore surface as TypeScript: exclude active rows and command-pane sessions, keep pinned/favorite/tagged history, return closedAt, and rank by provider close time instead of last activity or metadata edits.
    */
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_ascii_lowercase();
    let project_id_filter = params.get("projectId").and_then(Value::as_str);
    let tags = params
        .get("sessionTags")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut candidates = sessions
        .into_iter()
        .filter(is_previous_session_history_candidate)
        .filter(|session| {
            project_id_filter
                .map(|project_id| string_field(session, "projectId").as_deref() == Some(project_id))
                .unwrap_or(true)
        })
        .filter(|session| {
            tags.is_empty()
                || string_field(session, "sessionTag")
                    .map(|tag| tags.iter().any(|expected| *expected == tag))
                    .unwrap_or(false)
        })
        .filter(|session| {
            let active = is_active(session);
            (active && include_active) || (!active && include_previous)
        })
        .filter_map(|session| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            match_session(project, &session, &query).map(|matched| (session, matched))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left, _), (right, _)| {
        previous_session_closed_at(right)
            .cmp(&previous_session_closed_at(left))
            .then_with(|| string_field(left, "sessionId").cmp(&string_field(right, "sessionId")))
    });
    let total = candidates.len();
    let page = candidates
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|(session, matched)| {
            let project = projects.iter().find(|project| {
                string_field(project, "projectId") == string_field(&session, "projectId")
            });
            let mut result = search_result(project, &session, matched);
            if let Some(output) = result.as_object_mut() {
                output.insert(
                    "closedAt".to_string(),
                    Value::String(previous_session_closed_at(&session)),
                );
            }
            result
        })
        .collect::<Vec<_>>();
    let mut output = Map::new();
    if offset + limit < total {
        output.insert(
            "cursor".to_string(),
            Value::String((offset + limit).to_string()),
        );
    }
    output.insert("results".to_string(), Value::Array(page));
    Value::Object(output)
}

fn is_previous_session_history_candidate(session: &Value) -> bool {
    if string_field(session, "surface").as_deref() != Some("workspace") {
        return false;
    }
    if is_active(session) {
        return false;
    }
    if session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session.get("sessionTag").is_some()
    {
        return true;
    }
    if string_field(session, "lifecycleState").as_deref() != Some("stopped") {
        return false;
    }
    project_session_title(session)
        .get("trustedResumeTitle")
        .is_some()
}

fn previous_session_closed_at(session: &Value) -> String {
    let provider_closed_at = if string_field(session, "lifecycleState").as_deref()
        == Some("stopped")
        && read_provider_trimmed_text(session, "lifecycleState").as_deref() == Some("missing")
    {
        read_provider_trimmed_text(session, "probedAt")
    } else {
        None
    };
    provider_closed_at
        .or_else(|| string_field(session, "updatedAt"))
        .or_else(|| string_field(session, "createdAt"))
        .unwrap_or_default()
}

fn search_result(project: Option<&Value>, session: &Value, matched: Value) -> Value {
    let mut output = Map::new();
    if let Some(agent_id) = string_field(session, "agentId") {
        output.insert("agentIcon".to_string(), Value::String(agent_id.clone()));
        output.insert("agentId".to_string(), Value::String(agent_id));
    }
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName").or_else(|| string_field(session, "agentId")),
    );
    insert_optional_string(
        &mut output,
        "agentSessionId",
        read_runtime_text(session, "agentSessionId"),
    );
    insert_optional_string(
        &mut output,
        "agentSessionPath",
        read_runtime_text(session, "agentSessionPath"),
    );
    output.insert("createdAt".to_string(), value_field(session, "createdAt"));
    insert_optional_value(&mut output, "cwd", session.get("cwd").cloned());
    merge_object(&mut output, project_session_title(session));
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    output.insert("isPinned".to_string(), value_field(session, "isPinned"));
    output.insert(
        "lastActiveAt".to_string(),
        Value::String(last_active_at(session)),
    );
    output.insert(
        "lifecycleState".to_string(),
        value_field(session, "lifecycleState"),
    );
    output.insert("match".to_string(), matched);
    output.insert("projectId".to_string(), value_field(session, "projectId"));
    output.insert(
        "projectTitle".to_string(),
        project
            .and_then(|project| string_field(project, "name"))
            .or_else(|| string_field(session, "projectId"))
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    output.insert("sessionId".to_string(), value_field(session, "sessionId"));
    if let Some(provider) = search_session_persistence_provider(session) {
        output.insert(
            "sessionPersistenceProvider".to_string(),
            Value::String(provider.clone()),
        );
        if let Some(name) = search_session_persistence_name(session, &provider) {
            output.insert("sessionPersistenceName".to_string(), Value::String(name));
        }
    }
    insert_optional_value(
        &mut output,
        "sessionTag",
        session.get("sessionTag").cloned(),
    );
    insert_optional_value(
        &mut output,
        "sidebarOrder",
        session.get("sidebarOrder").cloned(),
    );
    insert_optional_string(
        &mut output,
        "subtitle",
        string_field(session, "cwd")
            .or_else(|| project.and_then(|project| string_field(project, "path"))),
    );
    output.insert("surface".to_string(), value_field(session, "surface"));
    output.insert("updatedAt".to_string(), value_field(session, "updatedAt"));
    insert_optional_value(&mut output, "zmxName", session.get("zmxName").cloned());
    Value::Object(output)
}

fn match_session(project: Option<&Value>, session: &Value, query: &str) -> Option<Value> {
    if query.is_empty() {
        return Some(json!({ "field": "title" }));
    }
    let title = project_session_title(session);
    let mut fields: Vec<(&str, String)> = Vec::new();
    push_field(
        &mut fields,
        "title",
        title.get("title").and_then(Value::as_str),
    );
    push_field(
        &mut fields,
        "title",
        title.get("primaryTitle").and_then(Value::as_str),
    );
    push_field(
        &mut fields,
        "title",
        title.get("terminalTitle").and_then(Value::as_str),
    );
    push_owned_field(&mut fields, "agent", string_field(session, "agentId"));
    push_owned_field(
        &mut fields,
        "agent",
        read_runtime_text(session, "agentName"),
    );
    push_owned_field(
        &mut fields,
        "project",
        project.and_then(|project| string_field(project, "name")),
    );
    push_owned_field(
        &mut fields,
        "project",
        project.and_then(|project| string_field(project, "path")),
    );
    push_owned_field(&mut fields, "cwd", string_field(session, "cwd"));
    push_owned_field(&mut fields, "command", string_field(session, "commandId"));
    push_owned_field(&mut fields, "id", string_field(session, "sessionId"));
    push_owned_field(&mut fields, "id", string_field(session, "globalRef"));
    push_owned_field(&mut fields, "timestamp", string_field(session, "createdAt"));
    push_owned_field(&mut fields, "timestamp", string_field(session, "updatedAt"));
    push_owned_field(&mut fields, "timestamp", Some(last_active_at(session)));
    for (field, value) in fields {
        if value.to_ascii_lowercase().contains(query) {
            return Some(json!({ "field": field, "snippet": value }));
        }
    }
    None
}

fn push_field(fields: &mut Vec<(&'static str, String)>, field: &'static str, value: Option<&str>) {
    if let Some(value) = value {
        fields.push((field, value.to_string()));
    }
}

fn push_owned_field(
    fields: &mut Vec<(&'static str, String)>,
    field: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        fields.push((field, value));
    }
}

fn project_session_title(session: &Value) -> Map<String, Value> {
    let title = string_field(session, "title").unwrap_or_else(|| "Terminal Session".to_string());
    let title_source = read_runtime_text(session, "titleSource")
        .or_else(|| read_runtime_text(session, "restoreTitleSource"))
        .filter(|value| {
            matches!(
                value.as_str(),
                "browser-auto" | "generated" | "placeholder" | "terminal-auto" | "user"
            )
        })
        .unwrap_or_else(|| {
            if is_temporary_title(&title) {
                "placeholder".to_string()
            } else {
                "user".to_string()
            }
        });
    let primary_title = visible_primary_title(&title, string_field(session, "agentId").as_deref());
    let trusted_resume_title = if title_source == "placeholder" || is_temporary_title(&title) {
        None
    } else {
        Some(title.clone())
    };
    let is_primary_terminal = trusted_resume_title.is_some();
    let display_title = primary_title.clone().unwrap_or_else(|| title.clone());
    let mut output = Map::new();
    output.insert(
        "displayTitle".to_string(),
        Value::String(display_title.clone()),
    );
    output.insert(
        "displayTitleTooltip".to_string(),
        Value::String(display_title),
    );
    output.insert(
        "isPrimaryTitleTerminalTitle".to_string(),
        Value::Bool(is_primary_terminal),
    );
    output.insert(
        "isTemporaryTitle".to_string(),
        Value::Bool(title_source == "placeholder" || is_temporary_title(&title)),
    );
    insert_optional_string(&mut output, "primaryTitle", primary_title);
    output.insert("title".to_string(), Value::String(title));
    output.insert("titleSource".to_string(), Value::String(title_source));
    insert_optional_string(&mut output, "trustedResumeTitle", trusted_resume_title);
    output
}

pub fn project_session_title_projection(session: &Value) -> Value {
    Value::Object(project_session_title(session))
}

fn presentation_actions(session: &Value, activity: &str) -> Value {
    /*
    CDXC:GxserverRustPort 2026-06-15-18:06:
    Phase 5 adds real zmx session I/O endpoints, so sidebar read/send/focus/sleep actions must require a confirmed provider route. A running domain row with providerState=unknown stays attachable but must not advertise live I/O until probe/start proves zmx exists.
    */
    let lifecycle = effective_lifecycle_state(session);
    let provider_exists = provider_exists(session);
    let is_running = lifecycle == "running";
    let is_sleeping = lifecycle == "sleeping";
    let is_stopped = lifecycle == "stopped";
    let can_attach = is_running || is_sleeping || provider_exists;
    let can_interact = provider_exists && !is_sleeping && !is_stopped;
    json!({
        "acknowledgeAttention": activity == "attention",
        "attach": can_attach,
        "focus": can_interact,
        "kill": !is_stopped,
        "readText": can_interact,
        "sendMessage": can_interact,
        "sendText": can_interact,
        "sleep": can_interact,
        "wake": is_sleeping,
    })
}

fn presentation_activity(session: &Value, _generated_at: &str) -> String {
    let activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(Value::as_object)
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str);
    match activity {
        Some("attention" | "working") => activity.unwrap().to_string(),
        _ => "idle".to_string(),
    }
}

fn attention_state(session: &Value) -> Value {
    let activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"))
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut output = Map::new();
    output.insert(
        "acknowledged".to_string(),
        Value::Bool(activity.get("isAcknowledged").and_then(Value::as_bool) == Some(true)),
    );
    insert_optional_value(
        &mut output,
        "enteredAt",
        activity.get("lastChangedAt").cloned(),
    );
    insert_optional_value(
        &mut output,
        "eventId",
        activity
            .get("attentionEventId")
            .cloned()
            .or_else(|| activity.get("lastChangedAt").cloned()),
    );
    Value::Object(output)
}

fn should_include_presentation_session(session: &Value) -> bool {
    is_active(session)
        || session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session.get("sessionTag").is_some()
}

fn is_active(session: &Value) -> bool {
    matches!(
        effective_lifecycle_state(session).as_str(),
        "running" | "sleeping"
    )
}

fn effective_lifecycle_state(session: &Value) -> String {
    if provider_exists(session)
        && string_field(session, "lifecycleState").as_deref() != Some("stopped")
    {
        return "running".to_string();
    }
    string_field(session, "lifecycleState").unwrap_or_else(|| "unknown".to_string())
}

fn provider_exists(session: &Value) -> bool {
    session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|provider| provider.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists")
}

fn is_favorite(session: &Value) -> bool {
    string_field(session, "sessionTag").as_deref() == Some("favorite")
        || session.get("isFavorite").and_then(Value::as_bool) == Some(true)
}

fn project_sort_key(project: &Value) -> String {
    let pin_rank = if project.get("isPinned").and_then(Value::as_bool) == Some(true) {
        "0"
    } else if project.get("isFavorite").and_then(Value::as_bool) == Some(true) {
        "1"
    } else {
        "2"
    };
    format!(
        "{}:{}:{}",
        pin_rank,
        string_field(project, "name")
            .unwrap_or_default()
            .to_ascii_lowercase(),
        string_field(project, "projectId").unwrap_or_default()
    )
}

fn session_sort_key(session: &Value) -> String {
    let active_rank = if is_active(session) { "0" } else { "1" };
    let pin_rank = if session.get("isPinned").and_then(Value::as_bool) == Some(true) {
        "0"
    } else if is_favorite(session) {
        "1"
    } else {
        "2"
    };
    let sidebar_order = session
        .get("sidebarOrder")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| format!("{:012}", value.floor() as i64))
        .unwrap_or_else(|| "z".to_string());
    format!(
        "{}:{}:{}:{}:{}",
        sidebar_order,
        active_rank,
        pin_rank,
        last_active_at(session),
        string_field(session, "sessionId").unwrap_or_default()
    )
}

fn last_active_at(session: &Value) -> String {
    string_field(session, "lastActiveAt")
        .or_else(|| string_field(session, "createdAt"))
        .unwrap_or_default()
}

fn visible_primary_title(title: &str, agent_id: Option<&str>) -> Option<String> {
    let normalized = title
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if normalized.is_empty() || is_temporary_title(&normalized) {
        Some(agent_default_title(agent_id))
    } else {
        Some(normalized)
    }
}

fn agent_default_title(agent_id: Option<&str>) -> String {
    let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return "Terminal Session".to_string();
    };
    let title = agent_id
        .replace(['-', '_'], " ")
        .split_whitespace()
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    format!("{title} Session")
}

fn is_temporary_title(title: &str) -> bool {
    matches!(
        title.trim().to_ascii_lowercase().as_str(),
        "terminal session"
            | "search by text"
            | "codex session"
            | "codex cli session"
            | "claude session"
            | "claude code session"
            | "cursor session"
            | "cursor agent session"
    ) || title.trim().starts_with("Session ")
}

fn normalize_limit(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.trunc().clamp(1.0, 100.0) as usize)
        .unwrap_or(40)
}

fn normalize_cursor(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

fn default_group_id(project_id: &str) -> String {
    format!("{project_id}:active")
}

fn read_runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read_provider_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn read_provider_trimmed_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn value_field(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or(Value::Null)
}

fn insert_optional_string(map: &mut Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value.filter(|value| !value.is_null()) {
        map.insert(key.to_string(), value);
    }
}

fn merge_object(target: &mut Map<String, Value>, values: Map<String, Value>) {
    target.extend(values);
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::DomainRepository,
        paths::get_gxserver_paths,
        storage::{initialize_gxserver_storage, open_gxserver_database},
    };

    #[test]
    fn snapshot_sorts_projects_and_sessions_for_sidebar_projection() {
        let projects = vec![
            project("P100", "Zulu", false, false),
            project("P101", "Alpha", true, false),
        ];
        let sessions = vec![
            session("P101", "G100", "Later", "running", 2000.0),
            session("P101", "G101", "Earlier", "running", 1000.0),
            session("P101", "G102", "Hidden stopped", "stopped", 0.0),
        ];
        let snapshot = project_snapshot(projects, sessions, 7);
        let projects = snapshot
            .get("projects")
            .and_then(Value::as_array)
            .expect("projects");
        assert_eq!(
            projects[0].get("projectId").and_then(Value::as_str),
            Some("P101")
        );
        let groups = snapshot
            .get("groups")
            .and_then(Value::as_array)
            .expect("groups");
        assert_eq!(groups[0].get("sessionIds"), Some(&json!(["G101", "G100"])));
    }

    #[test]
    fn search_matches_case_insensitive_project_text_and_paginates() {
        let projects = vec![project("P100", "Search Project", false, false)];
        let sessions = vec![
            session("P100", "G100", "First", "running", 1000.0),
            session("P100", "G101", "Second", "running", 2000.0),
        ];
        let params = json!({
            "limit": 1,
            "query": "search project",
        });
        let result = search_sessions(
            projects,
            sessions,
            params.as_object().expect("params object"),
        );
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(result.get("cursor").and_then(Value::as_str), Some("1"));
        assert_eq!(
            results[0]
                .get("match")
                .and_then(Value::as_object)
                .and_then(|matched| matched.get("field"))
                .and_then(Value::as_str),
            Some("project")
        );
    }

    #[test]
    fn list_previous_sessions_reads_domain_rows_with_closed_at() {
        let (_temp, db) = open_test_database();
        let repository = DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({
                    "name": "History",
                })
                .as_object()
                .expect("project params"),
            )
            .expect("project created");
        let project_id = string_field(&project, "projectId").expect("project id");
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "lifecycleState": "stopped",
                    "projectId": project_id,
                    "providerState": {
                        "lifecycleState": "missing",
                        "probedAt": "2026-06-06T12:00:00.000Z",
                        "provider": "zmx",
                    },
                    "runtimeSettings": {
                        "titleSource": "terminal-auto",
                    },
                    "title": "Restorable session",
                })
                .as_object()
                .expect("session params"),
                false,
            )
            .expect("session created");

        let result = list_previous_sessions(&db, "S7k", &Map::new()).expect("previous sessions");
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].get("sessionId"), session.get("sessionId"));
        assert_eq!(
            results[0].get("closedAt").and_then(Value::as_str),
            Some("2026-06-06T12:00:00.000Z")
        );
    }

    #[test]
    fn previous_sessions_filter_candidates_and_return_closed_at() {
        let projects = vec![project("P100", "History", false, false)];
        let trusted = previous_session(
            "G100",
            "Trusted title",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:30:00.000Z",
            "2026-06-01T09:00:00.000Z",
        );
        let placeholder = previous_session(
            "G101",
            "Search by Text",
            "stopped",
            "workspace",
            Some("2026-06-07T12:00:00.000Z"),
            "2026-06-07T12:30:00.000Z",
            "2026-06-07T09:00:00.000Z",
        );
        let mut favorite_placeholder = previous_session(
            "G102",
            "Search by Text",
            "stopped",
            "workspace",
            None,
            "2026-06-05T12:30:00.000Z",
            "2026-06-05T09:00:00.000Z",
        );
        favorite_placeholder
            .as_object_mut()
            .expect("favorite object")
            .insert(
                "sessionTag".to_string(),
                Value::String("favorite".to_string()),
            );
        let mut command_pinned = previous_session(
            "G103",
            "Pinned command",
            "stopped",
            "commands",
            Some("2026-06-08T12:00:00.000Z"),
            "2026-06-08T12:30:00.000Z",
            "2026-06-08T09:00:00.000Z",
        );
        command_pinned
            .as_object_mut()
            .expect("command object")
            .insert("isPinned".to_string(), Value::Bool(true));
        let running = previous_session(
            "G104",
            "Running",
            "running",
            "workspace",
            Some("2026-06-09T12:00:00.000Z"),
            "2026-06-09T12:30:00.000Z",
            "2026-06-09T09:00:00.000Z",
        );

        let result = search_previous_sessions(
            projects,
            vec![
                trusted,
                placeholder,
                favorite_placeholder,
                command_pinned,
                running,
            ],
            &Map::new(),
        );
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");

        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["G100", "G102"]
        );
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["2026-06-06T12:00:00.000Z", "2026-06-05T12:30:00.000Z"]
        );

        let query_params = json!({ "query": "09:00:00.000Z" });
        let query_result = search_previous_sessions(
            vec![project("P100", "History", false, false)],
            vec![
                previous_session(
                    "G100",
                    "Trusted title",
                    "stopped",
                    "workspace",
                    Some("2026-06-06T12:00:00.000Z"),
                    "2026-06-06T12:30:00.000Z",
                    "2026-06-01T09:00:00.000Z",
                ),
                previous_session(
                    "G102",
                    "Favorite title",
                    "stopped",
                    "workspace",
                    None,
                    "2026-06-05T12:30:00.000Z",
                    "2026-06-05T10:00:00.000Z",
                ),
            ],
            query_params.as_object().expect("query params"),
        );
        let query_results = query_result
            .get("results")
            .and_then(Value::as_array)
            .expect("query results");
        assert_eq!(query_results.len(), 1);
        assert_eq!(
            query_results[0].get("sessionId").and_then(Value::as_str),
            Some("G100")
        );
        assert_eq!(
            query_results[0]
                .get("match")
                .and_then(Value::as_object)
                .and_then(|matched| matched.get("field"))
                .and_then(Value::as_str),
            Some("timestamp")
        );
    }

    #[test]
    fn previous_sessions_rank_by_close_time_then_session_id() {
        let projects = vec![project("P100", "History", false, false)];
        let closed_recent = previous_session(
            "G1close",
            "Closed recently",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-01T09:00:00.000Z",
        );
        let active_before_close = previous_session(
            "G2active",
            "Active before close",
            "stopped",
            "workspace",
            Some("2026-06-05T12:00:00.000Z"),
            "2026-06-05T12:00:00.000Z",
            "2026-06-07T09:00:00.000Z",
        );
        let metadata_edited = previous_session(
            "G3meta",
            "Metadata edited after close",
            "stopped",
            "workspace",
            Some("2026-06-04T12:00:00.000Z"),
            "2026-06-08T12:00:00.000Z",
            "2026-06-04T09:00:00.000Z",
        );
        let same_time_later_id = previous_session(
            "G9same",
            "Same close later id",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T09:00:00.000Z",
        );
        let same_time_earlier_id = previous_session(
            "G0same",
            "Same close earlier id",
            "stopped",
            "workspace",
            Some("2026-06-06T12:00:00.000Z"),
            "2026-06-06T12:00:00.000Z",
            "2026-06-06T09:00:00.000Z",
        );

        let result = search_previous_sessions(
            projects,
            vec![
                active_before_close,
                metadata_edited,
                same_time_later_id,
                closed_recent,
                same_time_earlier_id,
            ],
            &Map::new(),
        );
        let results = result
            .get("results")
            .and_then(Value::as_array)
            .expect("results");

        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("sessionId").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec!["G0same", "G1close", "G9same", "G2active", "G3meta"]
        );
        assert_eq!(
            results
                .iter()
                .filter_map(|result| result.get("closedAt").and_then(Value::as_str))
                .collect::<Vec<_>>(),
            vec![
                "2026-06-06T12:00:00.000Z",
                "2026-06-06T12:00:00.000Z",
                "2026-06-06T12:00:00.000Z",
                "2026-06-05T12:00:00.000Z",
                "2026-06-04T12:00:00.000Z"
            ]
        );
    }

    fn project(project_id: &str, name: &str, is_pinned: bool, is_favorite: bool) -> Value {
        json!({
            "createdAt": "2026-06-15T09:55:00.000Z",
            "isFavorite": is_favorite,
            "isPinned": is_pinned,
            "name": name,
            "projectId": project_id,
            "updatedAt": "2026-06-15T09:55:00.000Z",
        })
    }

    fn session(
        project_id: &str,
        session_id: &str,
        title: &str,
        lifecycle_state: &str,
        sidebar_order: f64,
    ) -> Value {
        json!({
            "createdAt": "2026-06-15T09:55:00.000Z",
            "isFavorite": false,
            "isPinned": false,
            "kind": "terminal",
            "lifecycleState": lifecycle_state,
            "projectId": project_id,
            "providerState": { "lifecycleState": "missing" },
            "runtimeSettings": {},
            "sessionId": session_id,
            "sidebarOrder": sidebar_order,
            "surface": "workspace",
            "title": title,
            "updatedAt": "2026-06-15T09:55:00.000Z",
            "zmxName": format!("S7k-{project_id}-{session_id}"),
        })
    }

    fn previous_session(
        session_id: &str,
        title: &str,
        lifecycle_state: &str,
        surface: &str,
        probed_at: Option<&str>,
        updated_at: &str,
        last_active_at: &str,
    ) -> Value {
        let provider_state = match probed_at {
            Some(probed_at) => json!({
                "lifecycleState": "missing",
                "probedAt": probed_at,
                "provider": "zmx",
                "zmxName": format!("S7k-P100-{session_id}"),
            }),
            None => json!({
                "lifecycleState": "missing",
                "provider": "zmx",
                "zmxName": format!("S7k-P100-{session_id}"),
            }),
        };
        json!({
            "agentId": "codex",
            "createdAt": "2026-06-01T08:00:00.000Z",
            "isFavorite": false,
            "isPinned": false,
            "kind": "agent",
            "lastActiveAt": last_active_at,
            "lifecycleState": lifecycle_state,
            "projectId": "P100",
            "providerState": provider_state,
            "runtimeSettings": {
                "titleSource": if title == "Search by Text" { "placeholder" } else { "terminal-auto" },
            },
            "sessionId": session_id,
            "surface": surface,
            "title": title,
            "updatedAt": updated_at,
            "zmxName": format!("S7k-P100-{session_id}"),
        })
    }

    fn open_test_database() -> (tempfile::TempDir, Connection) {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));
        initialize_gxserver_storage(&paths).expect("storage init");
        let db = open_gxserver_database(&paths).expect("open db");
        (temp, db)
    }
}
