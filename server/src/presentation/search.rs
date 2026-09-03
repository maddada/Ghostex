use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

use super::*;

/*
CDXC:Sessions 2026-06-22-06:27:
Search parity with TypeScript depends on JavaScript-like parameter truthiness, Unicode lowercasing, and title trust filters because Previous Sessions uses these metadata rows as its restore surface. Keep malformed or non-restorable titles out of search history instead of letting generic paths, commands, or G-session IDs become previous-session results.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SearchProjectIdFilter {
    Any,
    Matches(String),
    MatchesNothing,
}

impl SearchProjectIdFilter {
    fn matches(&self, session: &Value) -> bool {
        match self {
            Self::Any => true,
            Self::Matches(project_id) => {
                string_field(session, "projectId").as_deref() == Some(project_id.as_str())
            }
            Self::MatchesNothing => false,
        }
    }
}

pub(crate) fn normalize_search_query(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
        .to_lowercase()
}

pub(crate) fn normalize_project_id_filter(value: Option<&Value>) -> SearchProjectIdFilter {
    match value {
        None | Some(Value::Null) => SearchProjectIdFilter::Any,
        Some(Value::String(project_id)) if project_id.is_empty() => SearchProjectIdFilter::Any,
        Some(Value::String(project_id)) => SearchProjectIdFilter::Matches(project_id.clone()),
        Some(Value::Bool(false)) => SearchProjectIdFilter::Any,
        Some(Value::Bool(true)) => SearchProjectIdFilter::MatchesNothing,
        Some(Value::Number(number)) => match number.as_f64() {
            Some(0.0) => SearchProjectIdFilter::Any,
            Some(_) => SearchProjectIdFilter::MatchesNothing,
            None => SearchProjectIdFilter::MatchesNothing,
        },
        Some(Value::Array(_) | Value::Object(_)) => SearchProjectIdFilter::MatchesNothing,
    }
}

pub(crate) fn normalize_session_tags(value: Option<&Value>) -> Result<Vec<&str>, DomainStateError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => {
            let mut tags = Vec::new();
            for item in items {
                let Some(tag) = item.as_str() else {
                    continue;
                };
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            Ok(tags)
        }
        Some(_) => Err(DomainStateError {
            code: "internalError",
            message: "values?.filter is not a function".to_string(),
        }),
    }
}

pub(crate) fn search_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = normalize_search_query(params.get("query"));
    let project_id_filter = normalize_project_id_filter(params.get("projectId"));
    let tags = normalize_session_tags(params.get("sessionTags"))?;
    let families = SessionForkFamilies::build(&sessions);
    let mut candidates = sessions
        .into_iter()
        .filter(|session| project_id_filter.matches(session))
        .filter(|session| session_matches_tag_filters(session, &tags))
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
            let mut result = search_result(project, &session, matched);
            if let (Some(output), Some(session_id)) =
                (result.as_object_mut(), string_field(&session, "sessionId"))
            {
                families.insert_fork_fields(&session_id, output);
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
    Ok(Value::Object(output))
}

pub(crate) fn search_previous_sessions(
    projects: Vec<Value>,
    sessions: Vec<Value>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    /*
    CDXC:Sessions 2026-06-19-14:30:
    Rust listPreviousSessions must be the same previous-only restore surface as TypeScript: exclude active rows and command-pane sessions, keep pinned/favorite/tagged history, return closedAt, and rank by provider close time instead of last activity or metadata edits.
    */
    let limit = normalize_limit(params.get("limit"));
    let offset = normalize_cursor(params.get("cursor"));
    let include_active = params.get("includeActive").and_then(Value::as_bool) != Some(false);
    let include_previous = params.get("includePrevious").and_then(Value::as_bool) != Some(false);
    let query = normalize_search_query(params.get("query"));
    let project_id_filter = normalize_project_id_filter(params.get("projectId"));
    let tags = normalize_session_tags(params.get("sessionTags"))?;
    /*
    CDXC:SessionFork 2026-08-28:
    Built over EVERY registry row, before any candidate filtering: the row that
    supersedes a closed ancestor is very often an ACTIVE session, which this
    surface would otherwise never look at, and the ancestor would stay listed
    beside its own continuation forever.
    */
    let families = SessionForkFamilies::build(&sessions);
    let mut candidates = sessions
        .into_iter()
        .filter(is_previous_session_history_candidate)
        /*
        A closed row that something else continues from is history the family's
        living branch already owns. Both leaves of a deliberate fork survive
        this, because neither of them has a descendant.
        */
        .filter(|session| {
            string_field(session, "sessionId")
                .is_none_or(|session_id| !families.is_superseded(&session_id))
        })
        .filter(|session| project_id_filter.matches(session))
        .filter(|session| session_matches_tag_filters(session, &tags))
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
                if let Some(session_id) = string_field(&session, "sessionId") {
                    families.insert_fork_fields(&session_id, output);
                }
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
    Ok(Value::Object(output))
}

pub(crate) fn is_previous_session_history_candidate(session: &Value) -> bool {
    if string_field(session, "surface").as_deref() != Some("workspace") {
        return false;
    }
    if is_active(session) {
        return false;
    }
    if session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || session.get("isParked").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session_tag_is_truthy(session)
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

pub(crate) fn previous_session_closed_at(session: &Value) -> String {
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

pub(crate) fn search_result(project: Option<&Value>, session: &Value, matched: Value) -> Value {
    let mut output = Map::new();
    if let Some(agent_id) = string_field(session, "agentId").filter(|value| !value.is_empty()) {
        output.insert(
            "agentIcon".to_string(),
            Value::String(session_agent_icon(project, session).unwrap_or_else(|| agent_id.clone())),
        );
        output.insert("agentId".to_string(), Value::String(agent_id));
    }
    insert_optional_string(
        &mut output,
        "agentName",
        read_runtime_text(session, "agentName")
            .or_else(|| string_field(session, "agentId"))
            .filter(|value| !value.is_empty()),
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
    insert_optional_js_truthy_value(&mut output, "cwd", session.get("cwd").cloned());
    merge_object(&mut output, project_session_title(session));
    output.insert("isFavorite".to_string(), Value::Bool(is_favorite(session)));
    output.insert("isParked".to_string(), value_field(session, "isParked"));
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
    insert_optional_js_truthy_value(
        &mut output,
        "sessionTag",
        session.get("sessionTag").cloned(),
    );
    insert_present_value(
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

pub(crate) fn match_session(
    project: Option<&Value>,
    session: &Value,
    query: &str,
) -> Option<Value> {
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
        if value.to_lowercase().contains(query) {
            return Some(json!({ "field": field, "snippet": value }));
        }
    }
    None
}

pub(crate) fn push_field(
    fields: &mut Vec<(&'static str, String)>,
    field: &'static str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        fields.push((field, value.to_string()));
    }
}

pub(crate) fn push_owned_field(
    fields: &mut Vec<(&'static str, String)>,
    field: &'static str,
    value: Option<String>,
) {
    if let Some(value) = value {
        fields.push((field, value));
    }
}
