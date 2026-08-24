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
    project
        .and_then(|project| project.get("customAgents"))
        .and_then(Value::as_array)
        .and_then(|agents| {
            agents.iter().find(|agent| {
                agent
                    .get("agentId")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.trim().eq_ignore_ascii_case(&agent_id))
            })
        })
        .and_then(|agent| string_field(agent, "icon"))
        .map(|icon| icon.trim().to_string())
        .filter(|icon| !icon.is_empty())
        .or(Some(agent_id))
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
