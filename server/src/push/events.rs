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
    enqueue_row_alert(state, repository, session, row, kind, PushRoute::default());
}

/// Where a push goes besides its kind's own choice: a worker's news follows the phone's Workers
/// choice, and a tap may open another session (CDXC:Notifications 2026-11-14 in
/// server/src/worker_alerts.rs).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PushRoute {
    pub(crate) worker: bool,
    pub(crate) open_session_id: Option<String>,
}

impl PushRoute {
    /// A worker's own news.
    pub(crate) fn worker() -> Self {
        Self {
            worker: true,
            open_session_id: None,
        }
    }

    /// Something only the user can do in a worker (an approval), opened on its coordinator.
    pub(crate) fn opens(session_id: &str) -> Self {
        Self {
            worker: false,
            open_session_id: Some(session_id.to_string()),
        }
    }
}

/// A committed feed row as `kind`, routed by `route`.
pub(crate) fn announce_feed_row_as(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    row: &Value,
    kind: AlertKind,
    route: PushRoute,
) {
    enqueue_row_alert(state, repository, session, row, kind, route);
}

/// A thread's finished turn under a coordinator, which the alert rule keeps quiet on the computer:
/// it goes only to the phones whose Workers (Thread reports) choice is not Off
/// (`AlertKind::ThreadReport`).
pub(crate) fn announce_thread_report(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    row: &Value,
) {
    enqueue_row_alert(
        state,
        repository,
        session,
        row,
        AlertKind::ThreadReport,
        PushRoute::worker(),
    );
}

/// A worker's news that has no Notifications row (Workers is Off on this computer): it still goes
/// to the phones whose own Workers choice is not Off, under a notice id of its own.
pub(crate) fn announce_worker_event(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    kind: AlertKind,
) {
    let row = rowless_notice(repository, session, &uuid::Uuid::new_v4().to_string());
    enqueue_row_alert(state, repository, session, &row, kind, PushRoute::worker());
}

/// The fields a push reads from a feed row, for a notice whose row is not the feed's own kind.
fn rowless_notice(repository: &DomainRepository<'_>, session: &Value, id: &str) -> Value {
    let project_id = session["projectId"].as_str().unwrap_or_default();
    let subtitle = repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| project["name"].as_str().map(str::to_string))
        .unwrap_or_default();
    serde_json::json!({
        "id": id,
        "projectId": project_id,
        "sessionId": session["sessionId"],
        "subtitle": subtitle,
        "createdAt": chrono::Utc::now().to_rfc3339(),
    })
}

/// Why a red-dot row goes to the phones (`announce_red_dot`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BlockedWhy {
    /// Not moving, and nobody heals it.
    Stuck,
    /// Every account is out of usage.
    Usage,
    /// The memory guard stopped something because the computer was running out of memory.
    OutOfMemory,
}

/// CDXC:Notifications 2026-11-08 DECISION:
/// The red dot alerts on the phone too ("red dot on any session needs to notify ... getting
/// blocked cuz account ran out of usage ... notify and play a sound always"): the bell row of a
/// session that became stuck with nobody healing it, or that waits with every account out of usage,
/// goes out as a push under the phone's Error switch. The row is the feed's `custom` row, which the
/// phone's own feed reader does not announce, so the push is its only alert there.
/// SEE-ALSO: packages/gx-core/src/alert_rule.rs (the rule).
pub(crate) fn announce_blocked(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    notification_id: &str,
    why: BlockedWhy,
    route: PushRoute,
) {
    let row = rowless_notice(repository, session, notification_id);
    enqueue_row_alert(state, repository, session, &row, why.kind(), route);
}

impl BlockedWhy {
    pub(crate) fn kind(self) -> AlertKind {
        match self {
            Self::Stuck => AlertKind::Stuck,
            Self::Usage => AlertKind::Blocked,
            Self::OutOfMemory => AlertKind::OutOfMemory,
        }
    }
}

fn enqueue_row_alert(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    row: &Value,
    kind: AlertKind,
    route: PushRoute,
) {
    let text = |key: &str| row.get(key).and_then(Value::as_str).unwrap_or_default();
    let occurred_at_ms = chrono::DateTime::parse_from_rfc3339(text("createdAt"))
        .map(|at| at.timestamp_millis())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp_millis());
    let projects = repository
        .list_projects_without_session_history()
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
            Vec::new()
        });
    let agents = crate::sidebar_hud::sidebar_agent_buttons_from_projects(&projects);
    let project = projects.iter().find(|project| {
        project.get("projectId").and_then(Value::as_str) == Some(text("projectId"))
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
        .or_else(|| crate::presentation::session_agent_name(project, session))
        // Never a custom agent's internal id (CDXC:AgentProviders 2026-11-11 in presentation/util.rs).
        .or_else(|| {
            Some(agent_id.trim())
                .filter(|id| !id.is_empty() && !crate::presentation::is_custom_agent_id(id))
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Agent".to_string());
    let session_title = crate::coordinators::session_title_of(session);
    state.push.enqueue_alert(AlertEvent {
        kind,
        project_id: text("projectId").to_string(),
        session_id: text("sessionId").to_string(),
        notification_id: text("id").to_string(),
        agent_name,
        project_name: text("subtitle").to_string(),
        session_title,
        occurred_at_ms,
        worker: route.worker,
        open_session_id: route.open_session_id,
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
