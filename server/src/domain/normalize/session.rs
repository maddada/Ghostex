use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::domain::normalize::fields::{
    has_string_field, insert_optional_object, insert_optional_string,
    normalize_domain_lifecycle_state, normalize_object, read_optional_text, set_optional_string,
    update_object_field, update_optional_object_field, update_optional_text_field,
};
use crate::domain::{js_string, DomainResult, DomainStateError};
use crate::ids::{create_global_session_ref, create_zmx_session_name, is_gxserver_session_id};

pub(crate) fn normalize_session_input(
    server_id: &str,
    project_id: &str,
    session_id: &str,
    timestamp: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let zmx_name = create_zmx_session_name(server_id, project_id, session_id);
    let title = read_optional_text(input.get("title")).unwrap_or_else(|| session_id.to_string());
    let mut runtime_settings = normalize_object(input.get("runtimeSettings"));
    if is_temporary_session_title(&title) && !has_string_field(&runtime_settings, "titleSource") {
        runtime_settings.insert(
            "titleSource".to_string(),
            Value::String("placeholder".to_string()),
        );
    }
    if !runtime_settings.contains_key("agentActivity") {
        runtime_settings.insert(
            "agentActivity".to_string(),
            default_agent_activity(input.get("agentId").and_then(Value::as_str), timestamp),
        );
    }
    let mut launch_settings = normalize_object(input.get("launchSettings"));
    normalize_launch_settings_with_surface(&mut launch_settings, input.get("surface"));
    let surface = resolve_surface(input.get("surface"), &launch_settings, &runtime_settings);
    let session_tag = normalize_optional_session_tag(input.get("sessionTag"))?;
    let provider_state =
        normalize_zmx_provider_state(normalize_object(input.get("providerState")), &zmx_name);

    let mut session = Map::new();
    insert_optional_string(
        &mut session,
        "agentId",
        read_optional_text(input.get("agentId")),
    );
    session.insert(
        "attentionRules".to_string(),
        Value::Object(normalize_object(input.get("attentionRules"))),
    );
    insert_optional_string(
        &mut session,
        "commandId",
        read_optional_text(input.get("commandId")),
    );
    session.insert(
        "completionRules".to_string(),
        Value::Object(normalize_object(input.get("completionRules"))),
    );
    session.insert(
        "createdAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    insert_optional_string(&mut session, "cwd", read_optional_text(input.get("cwd")));
    session.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(server_id, project_id, session_id)),
    );
    let mut hidden = Map::new();
    insert_optional_string(
        &mut hidden,
        "restoredFromHistoryId",
        read_optional_text(input.get("restoredFromHistoryId")),
    );
    if let Some(restored) = normalize_session_restore_id(input.get("restoredFromSessionId"))? {
        hidden.insert("restoredFromSessionId".to_string(), Value::String(restored));
    }
    session.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    session.insert(
        "isFavorite".to_string(),
        Value::Bool(
            session_tag.as_deref() == Some("favorite")
                || (session_tag.is_none()
                    && input.get("isFavorite").and_then(Value::as_bool) == Some(true)),
        ),
    );
    session.insert(
        "isPinned".to_string(),
        Value::Bool(
            input.get("isPinned").and_then(Value::as_bool) == Some(true)
                && input.get("isParked").and_then(Value::as_bool) != Some(true),
        ),
    );
    session.insert(
        "isParked".to_string(),
        Value::Bool(input.get("isParked").and_then(Value::as_bool) == Some(true)),
    );
    session.insert(
        "kind".to_string(),
        Value::String(normalize_session_kind(input.get("kind"))),
    );
    insert_optional_string(
        &mut session,
        "lastActiveAt",
        read_optional_text(input.get("lastActiveAt")),
    );
    session.insert("launchSettings".to_string(), Value::Object(launch_settings));
    session.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_domain_lifecycle_state(
            input.get("lifecycleState"),
        )),
    );
    session.insert(
        "notificationRules".to_string(),
        Value::Object(normalize_object(input.get("notificationRules"))),
    );
    session.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    session.insert("providerState".to_string(), Value::Object(provider_state));
    session.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    session.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    if let Some(tag) = session_tag {
        session.insert("sessionTag".to_string(), Value::String(tag));
    }
    if let Some(order) = normalize_optional_sidebar_order(input.get("sidebarOrder")) {
        session.insert("sidebarOrder".to_string(), json!(order));
    } else {
        session.insert("sidebarOrder".to_string(), json!(0));
    }
    session.insert("surface".to_string(), Value::String(surface));
    session.insert("title".to_string(), Value::String(title));
    session.insert(
        "updatedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    insert_optional_object(
        &mut session,
        "worktree",
        normalize_object(input.get("worktree")),
    );
    session.insert("zmxName".to_string(), Value::String(zmx_name));
    Ok(Value::Object(session))
}

pub(crate) fn normalize_zmx_provider_state(
    mut provider_state: Map<String, Value>,
    zmx_name: &str,
) -> Map<String, Value> {
    /*
    CDXC:RemoteMachines 2026-06-30-00:11:
    Remote sidebar clients depend on presentation publishing both the canonical zmx session name and its provider label so titles, status dots, and native idle indicators agree. Store `provider: "zmx"` with every gxserver session provider state instead of forcing clients to infer it from zmxName.
    */
    provider_state.insert(
        "lifecycleState".to_string(),
        Value::String(normalize_provider_lifecycle_state(
            provider_state.get("lifecycleState"),
        )),
    );
    provider_state.insert("provider".to_string(), Value::String("zmx".to_string()));
    provider_state.insert("zmxName".to_string(), Value::String(zmx_name.to_string()));
    provider_state
}

pub(crate) fn merge_session_update(
    server_id: &str,
    current: Value,
    updated_at: &str,
    input: &Map<String, Value>,
) -> DomainResult<Value> {
    let current = current.as_object().ok_or_else(|| {
        DomainStateError::corrupt_state("Session row did not decode as an object.")
    })?;
    let project_id = current
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state("projectId missing from session domain state.")
        })?;
    let session_id = current
        .get("sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            DomainStateError::corrupt_state("sessionId missing from session domain state.")
        })?;
    let zmx_name = create_zmx_session_name(server_id, &project_id, &session_id);
    let mut next = current.clone();
    update_optional_text_field(&mut next, input, "agentId");
    update_object_field(&mut next, input, "attentionRules");
    update_optional_text_field(&mut next, input, "commandId");
    update_object_field(&mut next, input, "completionRules");
    update_optional_text_field(&mut next, input, "cwd");
    let mut hidden = next
        .get("hiddenMetadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    if input.contains_key("restoredFromHistoryId") {
        set_optional_string(
            &mut hidden,
            "restoredFromHistoryId",
            read_optional_text(input.get("restoredFromHistoryId")),
        );
    }
    if input.contains_key("restoredFromSessionId") {
        if let Some(restored) = normalize_session_restore_id(input.get("restoredFromSessionId"))? {
            hidden.insert("restoredFromSessionId".to_string(), Value::String(restored));
        } else {
            hidden.remove("restoredFromSessionId");
        }
    }
    next.insert("hiddenMetadata".to_string(), Value::Object(hidden));
    if input.contains_key("isPinned") {
        let is_pinned = input.get("isPinned").and_then(Value::as_bool) == Some(true);
        next.insert("isPinned".to_string(), Value::Bool(is_pinned));
        if is_pinned {
            next.insert("isParked".to_string(), Value::Bool(false));
        }
    }
    if input.contains_key("isParked") {
        let is_parked = input.get("isParked").and_then(Value::as_bool) == Some(true);
        next.insert("isParked".to_string(), Value::Bool(is_parked));
        if is_parked {
            next.insert("isPinned".to_string(), Value::Bool(false));
        }
    }
    if input.contains_key("kind") {
        next.insert(
            "kind".to_string(),
            Value::String(normalize_session_kind(input.get("kind"))),
        );
    }
    update_optional_text_field(&mut next, input, "lastActiveAt");
    let mut launch_settings = if input.contains_key("launchSettings") {
        normalize_object(input.get("launchSettings"))
    } else {
        next.get("launchSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    let runtime_settings = if input.contains_key("runtimeSettings") {
        let title = input
            .get("title")
            .and_then(Value::as_str)
            .or_else(|| next.get("title").and_then(Value::as_str));
        let mut settings = normalize_object(input.get("runtimeSettings"));
        if title.map(is_temporary_session_title).unwrap_or(false)
            && !has_string_field(&settings, "titleSource")
        {
            settings.insert(
                "titleSource".to_string(),
                Value::String("placeholder".to_string()),
            );
        }
        settings
    } else {
        next.get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default()
    };
    if input.contains_key("launchSettings") || input.contains_key("surface") {
        let explicit_surface = input
            .get("surface")
            .filter(|_| input.contains_key("surface"));
        normalize_launch_settings_with_surface(&mut launch_settings, explicit_surface);
        let surface = resolve_surface(explicit_surface, &launch_settings, &runtime_settings);
        next.insert("surface".to_string(), Value::String(surface));
    } else {
        next.insert(
            "surface".to_string(),
            Value::String(resolve_surface(None, &launch_settings, &runtime_settings)),
        );
    }
    next.insert("launchSettings".to_string(), Value::Object(launch_settings));
    if input.contains_key("lifecycleState") {
        next.insert(
            "lifecycleState".to_string(),
            Value::String(normalize_domain_lifecycle_state(
                input.get("lifecycleState"),
            )),
        );
    }
    update_object_field(&mut next, input, "notificationRules");
    if input.contains_key("providerState") {
        let provider_state =
            normalize_zmx_provider_state(normalize_object(input.get("providerState")), &zmx_name);
        next.insert("providerState".to_string(), Value::Object(provider_state));
    } else if let Some(provider_state) = next
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
    {
        let provider_state = normalize_zmx_provider_state(provider_state, &zmx_name);
        next.insert("providerState".to_string(), Value::Object(provider_state));
    }
    next.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    if input.contains_key("sessionTag") || input.contains_key("isFavorite") {
        let session_tag = if input.contains_key("sessionTag") {
            normalize_optional_session_tag(input.get("sessionTag"))?
        } else if input.get("isFavorite").and_then(Value::as_bool) == Some(true) {
            Some("favorite".to_string())
        } else {
            None
        };
        if let Some(tag) = session_tag {
            next.insert("sessionTag".to_string(), Value::String(tag.clone()));
            next.insert("isFavorite".to_string(), Value::Bool(tag == "favorite"));
        } else {
            next.remove("sessionTag");
            next.insert("isFavorite".to_string(), Value::Bool(false));
        }
    }
    if input.contains_key("sidebarOrder") {
        match normalize_optional_sidebar_order(input.get("sidebarOrder")) {
            Some(order) => {
                next.insert("sidebarOrder".to_string(), json!(order));
            }
            None => {
                next.remove("sidebarOrder");
            }
        }
    }
    if input.contains_key("title") {
        next.insert(
            "title".to_string(),
            Value::String(read_optional_text(input.get("title")).unwrap_or(session_id.clone())),
        );
    }
    update_optional_object_field(&mut next, input, "worktree");
    next.insert(
        "globalRef".to_string(),
        Value::String(create_global_session_ref(
            server_id,
            &project_id,
            &session_id,
        )),
    );
    next.insert(
        "updatedAt".to_string(),
        Value::String(updated_at.to_string()),
    );
    next.insert("zmxName".to_string(), Value::String(zmx_name));
    Ok(Value::Object(next))
}

pub(crate) fn normalize_create_agent_session_params(
    input: &Map<String, Value>,
) -> Map<String, Value> {
    let mut params = input.clone();
    let agent_id = read_optional_text(input.get("agentId")).unwrap_or_else(|| "codex".to_string());
    let mut launch_settings = normalize_object(input.get("launchSettings"));
    let mut runtime_settings = normalize_object(input.get("runtimeSettings"));
    let base_command = read_optional_text(launch_settings.get("agentCommand"))
        .or_else(|| default_agent_command(&agent_id).map(str::to_string))
        .unwrap_or_default();
    let command = crate::agents::resolve_agent_launch_command(
        &agent_id,
        &base_command,
        read_optional_text(launch_settings.get("acceptAllMode")).as_deref(),
        false,
        read_optional_text(launch_settings.get("icon")).as_deref(),
    );
    let startup_text = if command.is_empty() {
        String::new()
    } else {
        format!(" {command}\r")
    };
    let mut plan = Map::new();
    if !base_command.is_empty() {
        plan.insert(
            "agentCommand".to_string(),
            Value::String(base_command.clone()),
        );
    }
    plan.insert("command".to_string(), Value::String(command.clone()));
    plan.insert("startupText".to_string(), Value::String(startup_text));
    plan.insert(
        "startupTextDisposition".to_string(),
        Value::String(
            if command.is_empty() {
                "none"
            } else {
                "queueAfterTerminalReady"
            }
            .to_string(),
        ),
    );
    if let Some(first_user_message) = read_optional_text(runtime_settings.get("firstUserMessage")) {
        plan.insert(
            "firstUserMessage".to_string(),
            Value::String(first_user_message),
        );
    }
    launch_settings.insert("agentLaunchPlan".to_string(), Value::Object(plan));
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        json!({ "queueProviderStartupText": !command.is_empty() }),
    );
    if !command.is_empty() {
        runtime_settings.insert("agentCommand".to_string(), Value::String(base_command));
    }
    runtime_settings.insert("agentName".to_string(), Value::String(agent_id.clone()));
    runtime_settings.insert("launchAgentId".to_string(), Value::String(agent_id.clone()));
    params.insert("agentId".to_string(), Value::String(agent_id));
    params.insert("kind".to_string(), Value::String("agent".to_string()));
    params.insert("launchSettings".to_string(), Value::Object(launch_settings));
    params.insert(
        "lifecycleState".to_string(),
        input
            .get("lifecycleState")
            .cloned()
            .unwrap_or_else(|| Value::String("running".to_string())),
    );
    params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );
    params
}

fn default_agent_command(agent_id: &str) -> Option<&'static str> {
    /*
    CDXC:AgentProviders 2026-07-02-14:20:
    The launcher default-command registry is owned by the agents module so every
    create path resolves the same command set; keeping a second literal map here
    silently dropped newer built-in agents from this normalization path.
    */
    crate::agents::default_agent_command(agent_id)
}

fn default_agent_activity(agent_id: Option<&str>, timestamp: &str) -> Value {
    let mut activity = Map::new();
    activity.insert("activity".to_string(), Value::String("idle".to_string()));
    if let Some(agent_id) = agent_id.filter(|value| !value.trim().is_empty()) {
        activity.insert("agentName".to_string(), Value::String(agent_id.to_string()));
    }
    activity.insert("hasSeenWorking".to_string(), Value::Bool(false));
    activity.insert("isAcknowledged".to_string(), Value::Bool(true));
    activity.insert(
        "lastChangedAt".to_string(),
        Value::String(timestamp.to_string()),
    );
    activity.insert(
        "suppressedUntil".to_string(),
        Value::String(timestamp.to_string()),
    );
    Value::Object(activity)
}

pub(crate) fn reject_stopped_session_revive(
    current: &Value,
    input: &Map<String, Value>,
    reason: &str,
) -> DomainResult<()> {
    if current.get("lifecycleState").and_then(Value::as_str) != Some("stopped") {
        return Ok(());
    }
    if let Some(requested) = input.get("lifecycleState").and_then(Value::as_str) {
        if requested != "stopped" {
            return Err(DomainStateError::bad_request(format!(
                "{reason} cannot change a stopped session to {requested}; use a lifecycle endpoint to wake or start it."
            )));
        }
    }
    if input
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|provider| provider.get("lifecycleState"))
        .and_then(Value::as_str)
        == Some("exists")
    {
        return Err(DomainStateError::bad_request(format!(
            "{reason} cannot mark a stopped session provider as exists; use a lifecycle endpoint to wake or start it."
        )));
    }
    Ok(())
}

pub(crate) fn normalize_session_order_ids(value: Option<&Value>) -> DomainResult<Vec<String>> {
    let Some(items) = value.and_then(Value::as_array) else {
        return Err(DomainStateError::bad_request(
            "sessionIds must contain at least one session ID.",
        ));
    };
    if items.is_empty() {
        return Err(DomainStateError::bad_request(
            "sessionIds must contain at least one session ID.",
        ));
    }
    let mut seen = HashSet::new();
    let mut session_ids = Vec::new();
    for item in items {
        let Some(session_id) = item.as_str() else {
            return Err(DomainStateError::bad_request(format!(
                "Invalid sessionId: {item}."
            )));
        };
        if !is_gxserver_session_id(session_id) {
            return Err(DomainStateError::bad_request(format!(
                "Invalid sessionId: {session_id}."
            )));
        }
        if !seen.insert(session_id.to_string()) {
            return Err(DomainStateError::bad_request(format!(
                "Duplicate sessionId: {session_id}."
            )));
        }
        session_ids.push(session_id.to_string());
    }
    Ok(session_ids)
}

/*
CDXC:ServerApi 2026-06-22-05:29:
Restored session references are user-provided gxserver session IDs. Match TypeScript by accepting only undefined, null, the exact empty string, or a valid G-id; whitespace and non-string values must be rejected instead of silently dropping the restore link.
*/
fn normalize_session_restore_id(value: Option<&Value>) -> DomainResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) if value.is_empty() => Ok(None),
        Some(Value::String(value)) if is_gxserver_session_id(value) => Ok(Some(value.clone())),
        _ => Err(DomainStateError::bad_request(format!(
            "Invalid restoredFromSessionId: {}.",
            js_string(value)
        ))),
    }
}

fn normalize_optional_sidebar_order(value: Option<&Value>) -> Option<i64> {
    let number = value.and_then(Value::as_f64)?;
    if number.is_finite() && number >= 0.0 {
        Some(number.floor() as i64)
    } else {
        None
    }
}

pub(crate) fn normalize_optional_session_tag(
    value: Option<&Value>,
) -> DomainResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let Some(tag) = value.as_str() else {
        return Err(DomainStateError::bad_request(
            "sessionTag must be a supported session tag.",
        ));
    };
    match tag {
        "favorite" | "high-priority" | "research" | "todo" | "in-progress" | "testing"
        | "blocked" | "low-priority" | "on-hold" | "done" | "bug" | "feature" | "design" => {
            Ok(Some(tag.to_string()))
        }
        _ => Err(DomainStateError::bad_request(
            "sessionTag must be a supported session tag.",
        )),
    }
}

pub(crate) fn normalize_session_kind(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("agent") => "agent".to_string(),
        _ => "terminal".to_string(),
    }
}

fn normalize_provider_lifecycle_state(value: Option<&Value>) -> String {
    match value.and_then(Value::as_str) {
        Some("exists" | "missing" | "unknown") => value.unwrap().as_str().unwrap().to_string(),
        _ => "unknown".to_string(),
    }
}

fn normalize_launch_settings_with_surface(
    launch_settings: &mut Map<String, Value>,
    explicit_surface: Option<&Value>,
) {
    if let Some(surface) = normalize_session_surface(explicit_surface)
        .or_else(|| normalize_session_surface(launch_settings.get("surface")))
    {
        launch_settings.insert("surface".to_string(), Value::String(surface));
    }
}

pub(crate) fn resolve_surface(
    explicit: Option<&Value>,
    launch_settings: &Map<String, Value>,
    runtime_settings: &Map<String, Value>,
) -> String {
    for value in [
        explicit.and_then(Value::as_str),
        launch_settings.get("surface").and_then(Value::as_str),
        runtime_settings.get("surface").and_then(Value::as_str),
    ]
    .into_iter()
    .flatten()
    {
        if value == "commands" || value == "workspace" {
            return value.to_string();
        }
    }
    "workspace".to_string()
}

fn normalize_session_surface(value: Option<&Value>) -> Option<String> {
    match value.and_then(Value::as_str) {
        Some(surface @ ("commands" | "workspace")) => Some(surface.to_string()),
        _ => None,
    }
}

fn is_temporary_session_title(title: &str) -> bool {
    /*
    CDXC:StateSync 2026-06-22-05:22:
    TypeScript domain normalization only auto-persists placeholder title provenance for Search by Text launches. Broader generic session labels are presentation and restore-filtering concerns, so the Rust repository must not store titleSource=placeholder for them at the durable row boundary.
    */
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .eq_ignore_ascii_case("search by text")
}
