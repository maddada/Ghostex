use serde_json::{json, Map, Value};

use crate::session_status::effective_agent_activity_value;

use super::*;

pub(crate) const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";
pub(crate) const TERMINAL_TITLE_MARKER: &str = "\u{2217}";
pub(crate) const UNSYNCED_TITLE_LABEL: &str = "(Unsynced title)";

pub(crate) fn project_session_title(session: &Value) -> Map<String, Value> {
    let title = string_field(session, "title")
        .unwrap_or_else(|| DEFAULT_TERMINAL_SESSION_TITLE.to_string());
    let title_source = session_title_source(session, &title);
    let agent_id = string_field(session, "agentId");
    let primary_candidate = session_card_primary_title(&title, agent_id.as_deref());
    let trusted_resume_title = trusted_resume_title(&title, &title_source);
    let primary_title = primary_candidate;
    let terminal_title: Option<String> = None;
    let is_primary_terminal = trusted_resume_title.is_some();
    let display_title = format_display_session_title(
        is_primary_terminal,
        primary_title.as_deref(),
        terminal_title.as_deref(),
        &title,
        false,
    );
    let display_title_tooltip = format_display_session_title(
        is_primary_terminal,
        primary_title.as_deref(),
        terminal_title.as_deref(),
        &title,
        true,
    );
    let mut output = Map::new();
    output.insert("displayTitle".to_string(), Value::String(display_title));
    output.insert(
        "displayTitleTooltip".to_string(),
        Value::String(display_title_tooltip),
    );
    output.insert(
        "isPrimaryTitleTerminalTitle".to_string(),
        Value::Bool(is_primary_terminal),
    );
    output.insert(
        "isTemporaryTitle".to_string(),
        Value::Bool(title_source == "placeholder" || is_temporary_session_title(&title)),
    );
    insert_optional_string(&mut output, "primaryTitle", primary_title);
    insert_optional_string(&mut output, "terminalTitle", terminal_title);
    output.insert("title".to_string(), Value::String(title));
    output.insert("titleSource".to_string(), Value::String(title_source));
    insert_optional_string(&mut output, "trustedResumeTitle", trusted_resume_title);
    output
}

pub fn project_session_title_projection(session: &Value) -> Value {
    Value::Object(project_session_title(session))
}

pub(crate) fn presentation_actions(session: &Value, activity: &str) -> Value {
    /*
    CDXC:RepoStructure 2026-06-15-18:06:
    Phase 5 adds real zmx session I/O endpoints, so sidebar read/send/focus/sleep actions must require a confirmed provider route. A running domain row with providerState=unknown stays attachable but must not advertise live I/O until probe/start proves zmx exists.
    */
    let lifecycle = effective_lifecycle_state(session);
    let provider_session_state = provider_session_state(session);
    let provider_exists = provider_session_state == "exists";
    let is_running = lifecycle == "running";
    let is_sleeping = lifecycle == "sleeping";
    let is_stopped = lifecycle == "stopped";
    let can_attach =
        provider_exists || (is_running && provider_session_state == "unknown") || is_sleeping;
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

pub(crate) fn presentation_activity(session: &Value, generated_at: &str) -> String {
    if effective_lifecycle_state(session) == "running"
        && crate::session_chat_compacting::session_chat_compacting_detected_at(session).is_some()
    {
        // A whole live-screen capture is stronger evidence than a stale hook
        // or title state: the CLI is actively compacting, not waiting for an
        // answer left over from the preceding turn.
        return "working".to_string();
    }
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    let raw_activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"));
    let effective = effective_agent_activity_value(raw_activity, "idle", generated_at_ms);
    let activity = effective
        .as_object()
        .and_then(|activity| activity.get("activity"))
        .and_then(Value::as_str);
    match activity {
        Some("attention" | "working") => activity.unwrap().to_string(),
        _ => "idle".to_string(),
    }
}

pub(crate) fn attention_state(session: &Value, generated_at: &str) -> Value {
    let generated_at_ms = parse_iso_ms(generated_at).unwrap_or_else(now_ms);
    let raw_activity = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentActivity"));
    let activity = effective_agent_activity_value(raw_activity, "idle", generated_at_ms)
        .as_object()
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

pub(crate) fn should_include_presentation_session(session: &Value) -> bool {
    is_active(session)
        || session.get("isPinned").and_then(Value::as_bool) == Some(true)
        || session.get("isParked").and_then(Value::as_bool) == Some(true)
        || is_favorite(session)
        || session_tag_is_truthy(session)
}

pub(crate) fn is_active(session: &Value) -> bool {
    matches!(
        effective_lifecycle_state(session).as_str(),
        "running" | "sleeping"
    )
}

pub(crate) fn effective_lifecycle_state(session: &Value) -> String {
    if provider_exists(session)
        && string_field(session, "lifecycleState").as_deref() != Some("stopped")
    {
        return "running".to_string();
    }
    string_field(session, "lifecycleState").unwrap_or_else(|| "unknown".to_string())
}

pub(crate) fn provider_exists(session: &Value) -> bool {
    provider_session_state(session) == "exists"
}

pub(crate) fn provider_session_state(session: &Value) -> String {
    if read_session_persistence_provider(session).as_deref() == Some("off") {
        return "persistence-disabled".to_string();
    }
    match session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|provider| provider.get("lifecycleState"))
        .and_then(Value::as_str)
    {
        Some("exists") => "exists".to_string(),
        Some("missing") => "missing".to_string(),
        Some("unknown") => "unknown".to_string(),
        _ => "unknown".to_string(),
    }
}

pub(crate) fn title_observation_state(session: &Value) -> Option<Value> {
    let observation = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("zmxTitleObservation"))
        .and_then(Value::as_object)?;
    let status = match observation.get("status").and_then(Value::as_str) {
        Some("active" | "failed" | "retrying" | "starting") => {
            observation.get("status").cloned().unwrap_or(Value::Null)
        }
        _ => return None,
    };
    let mut output = Map::new();
    if let Some(failure_count) = observation
        .get("failureCount")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.floor() as i64)
    {
        output.insert(
            "failureCount".to_string(),
            Value::Number(serde_json::Number::from(failure_count)),
        );
    }
    insert_optional_observation_text(&mut output, observation, "lastFailedAt");
    insert_optional_observation_text(&mut output, observation, "lastObservedAt");
    insert_optional_observation_text(&mut output, observation, "lastStartedAt");
    insert_optional_observation_text(&mut output, observation, "nextRetryAt");
    output.insert("status".to_string(), status);
    Some(Value::Object(output))
}

pub(crate) fn insert_optional_observation_text(
    output: &mut Map<String, Value>,
    observation: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = observation
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        output.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub(crate) fn build_session_tooltip(project: &Value, session: &Value, title: &str) -> String {
    let mut parts = Vec::new();
    if !title.is_empty() {
        parts.push(title.to_string());
    }
    parts.extend(
        [
            string_field(project, "name"),
            string_field(session, "cwd"),
            string_field(session, "agentId"),
            string_field(session, "commandId"),
        ]
        .into_iter()
        .flatten()
        .filter(|value| !value.is_empty()),
    );
    parts.join(" - ")
}

pub(crate) fn snapshot_subtitle(project: &Value, session: &Value) -> Option<String> {
    let value = match session.get("cwd") {
        Some(value) if !value.is_null() => Some(value),
        _ => project.get("path"),
    }?;
    value
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn session_tag_is_truthy(session: &Value) -> bool {
    session.get("sessionTag").map(js_truthy).unwrap_or(false)
}

pub(crate) fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(number) => number.as_f64().map(|value| value != 0.0).unwrap_or(true),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

pub(crate) fn insert_optional_js_truthy_value(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<Value>,
) {
    if let Some(value) = value.filter(js_truthy) {
        map.insert(key.to_string(), value);
    }
}

pub(crate) fn is_favorite(session: &Value) -> bool {
    string_field(session, "sessionTag").as_deref() == Some("favorite")
        || session.get("isFavorite").and_then(Value::as_bool) == Some(true)
}

pub(crate) fn effective_session_tag_for_filter(session: &Value) -> Option<String> {
    if let Some(tag) = string_field(session, "sessionTag").filter(|tag| !tag.is_empty()) {
        return Some(tag);
    }
    if session.get("isFavorite").and_then(Value::as_bool) == Some(true) {
        return Some("favorite".to_string());
    }
    None
}

pub(crate) fn session_matches_tag_filters(session: &Value, tags: &[&str]) -> bool {
    if tags.is_empty() {
        return true;
    }
    match effective_session_tag_for_filter(session) {
        Some(tag) => tags.iter().any(|expected| *expected == tag),
        None => tags.iter().any(|expected| *expected == "untagged"),
    }
}

pub(crate) fn project_sort_key(project: &Value) -> String {
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
            .to_lowercase(),
        string_field(project, "projectId").unwrap_or_default()
    )
}

pub(crate) fn session_sort_key(session: &Value) -> String {
    let active_rank = if is_active(session) { "0" } else { "1" };
    let pin_rank = if session.get("isPinned").and_then(Value::as_bool) == Some(true) {
        "0"
    } else if session.get("isParked").and_then(Value::as_bool) == Some(true) {
        "3"
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

pub(crate) fn last_active_at(session: &Value) -> String {
    string_field(session, "lastActiveAt")
        .or_else(|| string_field(session, "createdAt"))
        .unwrap_or_default()
}
