use serde_json::{Map, Value};

pub(crate) fn normalize_spaces(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn normalize_limit(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.trunc().clamp(1.0, 100.0) as usize)
        .unwrap_or(40)
}

pub(crate) fn normalize_cursor(value: Option<&Value>) -> usize {
    value
        .and_then(Value::as_str)
        .filter(|value| value.chars().all(|ch| ch.is_ascii_digit()))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0)
}

pub(crate) fn default_group_id(project_id: &str) -> String {
    format!("{project_id}:active")
}

pub(crate) fn read_runtime_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_provider_trimmed_text(session: &Value, key: &str) -> Option<String> {
    session
        .get("providerState")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn session_agent_icon(project: Option<&Value>, session: &Value) -> Option<String> {
    let stored_icon = session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("icon"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if stored_icon.is_some() {
        return stored_icon;
    }

    let agent_id = string_field(session, "agentId")?;
    project_custom_agent_icon(project, &agent_id).or(Some(agent_id))
}

/// CDXC:AgentProviders 2026-11-08 FIX: User, on the phone's bell showing
/// "custom-claude-70-dl0rpxbt39e0-…" for a session: "Name of coordinator here looks wrong please fix".
/// A session of a custom agent kept its `custom-…` id after the agent left Settings > Agents, and
/// every name fallback (the session's `agentName`, the push title) printed that id. A custom agent is
/// named by the project's custom agent entry, else by its base agent (the CLI family its launch icon
/// declares: "Claude"); None for any other agent id, whose callers keep their own fallback.
pub(crate) fn custom_agent_display_name(
    project: Option<&Value>,
    session: &Value,
) -> Option<String> {
    let agent_id = string_field(session, "agentId")?;
    let agent_id = agent_id.trim();
    if !agent_id.starts_with("custom-") {
        return None;
    }
    project
        .and_then(|project| project.get("customAgents"))
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents.iter().find(|agent| {
                agent
                    .get("agentId")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(agent_id))
            })
        })
        .and_then(|agent| string_field(agent, "name"))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| {
            let icon = session_agent_icon(project, session)?;
            let base = crate::agents::default_agent_icon_to_id(&icon)?;
            crate::sidebar_hud::default_agent_name(base).map(str::to_string)
        })
}

/// CDXC:AgentProviders 2026-11-11 FIX: User, on a push titled "custom-claude-70-dl0rpxbt39e0-…" for his coordinator
/// (a session of a custom agent that is no longer in Settings > Agents): a push must never show an internal id. The
/// session's stored `agentName` was that id itself, and the session list, the push, the Live Activity and the widgets
/// all read it before the custom agent fallback above. The agent's name as the app shows it: the session's stored name
/// unless that is its own `custom-…` id, then the custom agent's name (`custom_agent_display_name`); None when neither
/// is known, and callers keep their own fallback, never a `custom-…` id.
pub(crate) fn session_agent_name(project: Option<&Value>, session: &Value) -> Option<String> {
    let agent_id = string_field(session, "agentId").unwrap_or_default();
    let agent_id = agent_id.trim();
    read_runtime_text(session, "agentName")
        .filter(|name| !(is_custom_agent_id(agent_id) && name.eq_ignore_ascii_case(agent_id)))
        .filter(|name| !is_custom_agent_id(name))
        .or_else(|| custom_agent_display_name(project, session))
}

/// A custom agent's internal id (`custom-<base>-<n>-<random>`), never a name to show.
pub(crate) fn is_custom_agent_id(value: &str) -> bool {
    let value = value.trim();
    value.len() > "custom-".len()
        && value
            .get(.."custom-".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("custom-"))
        && !value.contains(char::is_whitespace)
}

pub(crate) fn project_custom_agent_icon(project: Option<&Value>, agent_id: &str) -> Option<String> {
    project
        .and_then(|project| project.get("customAgents"))
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents.iter().find(|agent| {
                agent
                    .get("agentId")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(agent_id))
            })
        })
        .and_then(|agent| string_field(agent, "icon"))
        .map(|icon| icon.trim().to_string())
        .filter(|icon| !icon.is_empty())
}

pub(crate) fn value_field(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or(Value::Null)
}

pub(crate) fn insert_optional_string(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

pub(crate) fn insert_optional_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value.filter(|value| !value.is_null()) {
        map.insert(key.to_string(), value);
    }
}

pub(crate) fn insert_present_value(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        map.insert(key.to_string(), value);
    }
}

pub(crate) fn merge_object(target: &mut Map<String, Value>, values: Map<String, Value>) {
    target.extend(values);
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn parse_iso_ms(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_millis())
}
