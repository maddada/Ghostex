//! The hook side of the comparison: every hook event Claude, Codex and OpenCode post.

use serde_json::{Map, Value};

use super::comparison::{record, ActivityEvent, Side};
use crate::server::AppState;

/// Hook params the activity mapping reads, copied so both sides map through the same rules.
const MAPPED_PARAMS: &[&str] = &[
    "background_tasks",
    "notificationKind",
    "notificationType",
    "source",
    "status",
    "stopStatus",
    "toolName",
    "trigger",
];

/// Records a hook event `/api/ingestAgentHookEvent` received, including one the identity guard
/// kept from moving the session to another conversation (Claude's SessionStart after /clear, held
/// until the new conversation is written): the hook did report it.
pub(crate) fn observe_hook_event(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if endpoint_path != "/api/ingestAgentHookEvent"
        || params.contains_key(super::drive::OBSERVER_MARK)
        || !super::task::comparison_enabled(state)
        || result.get("reason").and_then(Value::as_str) == Some("agent-hook-agent-mismatch")
    {
        return;
    }
    let agent =
        match crate::agents::normalize_agent_id(params.get("agentName").and_then(Value::as_str))
            .as_deref()
        {
            Some("claude") => "claude",
            Some("codex") => "codex",
            Some("opencode") => "opencode",
            _ => return,
        };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id), Some(event_name)) = (
        text("projectId"),
        text("sessionId"),
        text("eventName").or_else(|| text("rawEventName")),
    ) else {
        return;
    };
    let mut event = ActivityEvent::new(
        &project_id,
        &session_id,
        agent,
        &event_name,
        text("agentSessionId"),
    );
    for key in MAPPED_PARAMS {
        if let Some(value) = params.get(*key) {
            event.payload.insert((*key).to_string(), value.clone());
        }
    }
    record(&state.logger, Side::Hook, event);
}
