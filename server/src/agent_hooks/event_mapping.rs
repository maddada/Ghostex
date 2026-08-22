use std::env;

use serde_json::{json, Map, Value};

use super::notify_runtime::read_state_string;
use super::probing::{normalize_prompt_text, now_iso};

pub(crate) fn nested_get<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in keys {
        current = current.get(*key)?;
    }
    Some(current)
}

pub(crate) fn first_string<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(normalize_prompt_text)
            .filter(|text| !text.is_empty())
        {
            return Some(text);
        }
    }
    None
}

pub(crate) fn first_path<const N: usize>(values: [Option<&Value>; N]) -> Option<String> {
    for value in values.into_iter().flatten() {
        if let Some(text) = value
            .as_str()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            return Some(text.to_string());
        }
    }
    None
}

pub(crate) fn env_string(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|value| normalize_prompt_text(&value))
        .filter(|value| !value.is_empty())
}

pub(crate) fn normalized_hook_agent_key(value: &str) -> String {
    let normalized = normalize_prompt_text(&value.to_ascii_lowercase());
    let mapped = match normalized.as_str() {
        "claude" | "claude code" => "claude",
        "codex" | "openai codex" | "codex cli" => "codex",
        "pi" | "π" => "pi",
        "omp" => "omp",
        "opencode" | "open code" => "opencode",
        "grok" | "grok build" => "grok",
        "amp" | "amp cli" => "amp",
        "cursor" | "cursor agent" | "cursor cli" | "cursor-agent" => "cursor",
        "gemini" | "gemini cli" => "gemini",
        "agy" | "antigravity" | "antigravity cli" => "antigravity",
        "copilot" | "github copilot" => "copilot",
        "codebuddy" | "code buddy" => "codebuddy",
        "droid" | "factory" | "factory droid" => "droid",
        "kiro" | "kiro-cli" | "kiro cli" => "kiro",
        "qoder" | "qodercli" => "qoder",
        "rovo" | "rovo dev" | "rovodev" => "rovodev",
        "hermes" | "hermes agent" | "hermes-agent" => "hermes-agent",
        other => other,
    };
    let cleaned = mapped
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "codex".to_string()
    } else {
        cleaned
    }
}

pub(crate) fn activity_for_hook_event(agent_key: &str, event_name: &str, payload: &Value) -> Option<String> {
    let normalized_event_name = normalize_prompt_text(event_name);
    let lower = normalized_event_name.to_ascii_lowercase();
    if agent_key == "codex" {
        if lower == "stop" {
            return Some("attention".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if agent_key == "claude" {
        if matches!(lower.as_str(), "stop" | "idle" | "sessionend") {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "notification" | "notify" | "permissionrequest"
        ) {
            return Some("attention".to_string());
        }
        if matches!(
            lower.as_str(),
            "userpromptsubmit" | "prompt-submit" | "pretooluse" | "pre-tool-use"
        ) {
            return Some("working".to_string());
        }
        if matches!(lower.as_str(), "sessionend" | "session-end") {
            return Some("idle".to_string());
        }
    }
    if matches!(agent_key, "copilot" | "codebuddy" | "droid" | "qoder") {
        if matches!(
            lower.as_str(),
            "stop" | "notification" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(lower.as_str(), "pretooluse" | "pre-tool-use") {
            return Some("working".to_string());
        }
    }
    if agent_key == "antigravity" {
        let fully_idle = payload_boolean(
            payload,
            &[
                "fullyIdle",
                "fully_idle",
                "metadata.fullyIdle",
                "properties.fullyIdle",
            ],
        );
        if fully_idle == Some(false)
            && matches!(lower.as_str(), "stop" | "turn-completion" | "notification")
        {
            return Some("working".to_string());
        }
        if matches!(
            lower.as_str(),
            "stop" | "turn-completion" | "sessionend" | "session-end"
        ) {
            return Some("idle".to_string());
        }
        if matches!(
            lower.as_str(),
            "preinvocation" | "pretooluse" | "posttooluse"
        ) {
            return Some("working".to_string());
        }
    }
    let compact = lower.replace(['_', '-', '.'], "");
    if matches!(
        compact.as_str(),
        "agentstart"
            | "beforeagentstart"
            | "beforeagent"
            | "beforeshellexecution"
            | "beforesubmitprompt"
            | "onsessionreset"
            | "onsessionstart"
            | "ontoolpermission"
            | "postapprovalresponse"
            | "posttooluse"
            | "prellmcall"
            | "pretoolcall"
            | "preinvocation"
            | "pretooluse"
            | "promptsubmit"
            | "userpromptsubmit"
    ) {
        return Some("working".to_string());
    }
    if matches!(
        compact.as_str(),
        "notification" | "notify" | "permissionrequest" | "preapprovalrequest"
    ) {
        return Some("attention".to_string());
    }
    if matches!(
        compact.as_str(),
        "afteragent"
            | "afteragentresponse"
            | "agentend"
            | "agentresponse"
            | "oncomplete"
            | "onerror"
            | "onsessionend"
            | "onsessionfinalize"
            | "postllmcall"
            | "release"
            | "sessionend"
            | "sessionshutdown"
            | "stop"
            | "turncompletion"
    ) {
        return Some("idle".to_string());
    }
    None
}

fn payload_boolean(payload: &Value, keys: &[&str]) -> Option<bool> {
    for key in keys {
        let value = if key.contains('.') {
            nested_get(payload, &key.split('.').collect::<Vec<_>>())
        } else {
            payload.get(*key)
        };
        match value {
            Some(Value::Bool(value)) => return Some(*value),
            Some(Value::String(value)) if matches!(value.as_str(), "true" | "1") => {
                return Some(true)
            }
            Some(Value::String(value)) if matches!(value.as_str(), "false" | "0") => {
                return Some(false)
            }
            _ => {}
        }
    }
    None
}

pub(crate) fn update_hook_status(state: &mut Map<String, Value>, status: &str) {
    let timestamp = now_iso();
    state.insert("status".to_string(), json!(status));
    state.insert("statusUpdatedAt".to_string(), json!(timestamp.clone()));
    state.insert("lastActivityAt".to_string(), json!(timestamp.clone()));
    if status == "attention" {
        state.insert(
            "attentionEventId".to_string(),
            json!(format!("{timestamp}:attention")),
        );
        state.insert("attentionAcknowledgedAt".to_string(), json!(""));
        state.insert("attentionAcknowledgedEventId".to_string(), json!(""));
    } else if status == "working" {
        state.insert("attentionAcknowledgedAt".to_string(), json!(timestamp));
        let event_id = read_state_string(state, "attentionEventId").unwrap_or_default();
        state.insert("attentionAcknowledgedEventId".to_string(), json!(event_id));
    }
}

pub(crate) fn is_prompt_event(event_name: &str) -> bool {
    let lower = event_name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "userpromptsubmit"
            | "beforeagent"
            | "preinvocation"
            | "pretooluse"
            | "beforesubmitprompt"
            | "beforeshellexecution"
            | "pre_llm_call"
            | "pre_tool_call"
            | "on_tool_permission"
            | "agent_start"
            | "agent.start"
            | "before_agent_start"
    )
}
