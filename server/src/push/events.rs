use serde_json::Value;

use crate::domain::DomainRepository;
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;

use super::message::{AgentStartEvent, AlertEvent, AlertKind};

/// CDXC:Notifications 2026-11-01 WHY:
/// A phone hears of exactly the rows the feed committed: a finished, needs-input or failed row is queued for push with its own id once the row is stored, so the push and the row are one notice on the phone, and a row that was not stored sends nothing. Bell, custom and account rows stay on the computer. The title uses the same launcher display name as the mobile summary and the row's project name; the row's title (the session's title) and its text never go out.
/// Finished rows keep the Finished switch and wording even while the session waits on the user; needs-input rows use the Waiting switch and its question wording.
/// SEE-ALSO: server/src/notification_feed/producer.rs and failure.rs (where rows are committed).
pub(crate) fn announce_feed_row(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    row: &Value,
) {
    let kind = match row.get("kind").and_then(Value::as_str) {
        Some("finished") => AlertKind::Finished,
        Some("needsInput") => AlertKind::NeedsInput,
        Some("failed") => AlertKind::Failed,
        _ => return,
    };
    let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
    let occurred_at_ms = chrono::DateTime::parse_from_rfc3339(text("createdAt"))
        .map(|at| at.timestamp_millis())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis());
    let agents = repository
        .list_projects_without_session_history()
        .map(|projects| crate::sidebar_hud::sidebar_agent_buttons_from_projects(&projects))
        .unwrap_or_else(|error| {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "push.agentNameReadFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message),
                details: None,
            });
            Value::Null
        });
    let agent_id = session
        .get("agentId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let agent_name = agents
        .as_array()
        .and_then(|agents| {
            agents
                .iter()
                .find(|agent| agent.get("agentId").and_then(Value::as_str) == Some(agent_id))
        })
        .and_then(|agent| agent.get("name").and_then(Value::as_str))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| crate::presentation::read_runtime_text(session, "agentName"))
        .unwrap_or_else(|| agent_id.trim().to_string());
    state.push.enqueue_alert(AlertEvent {
        kind,
        project_id: text("projectId").to_string(),
        session_id: text("sessionId").to_string(),
        notification_id: text("id").to_string(),
        agent_name,
        project_name: text("subtitle").to_string(),
        occurred_at_ms,
    });
}

/// CDXC:Notifications 2026-11-01 WHY:
/// The background-only start message goes out when an agent's own signal (a hook or its terminal title) moves its session from not working to working; an activity call from a client, a program's status report, a subagent's hook and a stale or foreign event never start one, and a session that keeps working sends nothing more. The phone's three switches do not apply to it; a phone without a registration gets nothing.
pub(crate) fn observe_agent_start(state: &AppState, endpoint_path: &str, result: &Value) {
    if !matches!(
        endpoint_path,
        "/api/ingestAgentHookEvent" | "/api/ingestTerminalTitleEvent"
    ) {
        return;
    }
    let previous = result.get("previousActivity").and_then(Value::as_str);
    let Some(activity) = result.get("activity").and_then(Value::as_object) else {
        return;
    };
    let started = previous.is_some_and(|previous| previous != "working")
        && activity.get("activity").and_then(Value::as_str) == Some("working")
        && activity
            .get("agentName")
            .and_then(Value::as_str)
            .is_some_and(|agent| !agent.trim().is_empty())
        && !matches!(
            result.get("reason").and_then(Value::as_str),
            Some(
                "subagent-hook"
                    | "agent-hook-agent-mismatch"
                    | "passive-session-identity-conflict"
                    | "stale-activity-event"
            )
        );
    let Some(session_id) = result.pointer("/session/sessionId").and_then(Value::as_str) else {
        return;
    };
    if started {
        state.push.enqueue_agent_start(AgentStartEvent {
            session_id: session_id.to_string(),
            occurred_at_ms: chrono::Utc::now().timestamp_millis(),
        });
    }
}
