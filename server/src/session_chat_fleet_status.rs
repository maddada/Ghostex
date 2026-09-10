//! CDXC:SessionStatus 2026-09-06 WHY:
//! Subagents and Claude monitors can outlive the lead turn while nobody follows its chat. Probe running Claude/Codex sessions on the daemon's clock so the shared status projection discovers and retires that work without an open chat.

use std::sync::Arc;
use std::time::Duration;

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::session_chat_follower::session_chat_agent_for_session;
use crate::session_chat_options::{
    session_chat_option_agent, SessionChatOptionAgent, SESSION_CHAT_OPTION_CACHE_TTL,
};
use crate::storage::open_gxserver_database;

/// CDXC:SessionStatus 2026-09-06 WHY:
/// A crashed child may leave an unfinished turn on disk. A root startup/resume starts a new process run; old child turns must not make that resumed thread work forever. Compaction keeps the current children alive.
pub(crate) fn record_agent_start(
    repository: &DomainRepository<'_>,
    session: &serde_json::Value,
    params: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<serde_json::Value>, crate::domain::DomainStateError> {
    use serde_json::{json, Value};
    let agent = params.get("agentName").and_then(Value::as_str);
    if !matches!(agent, Some("codex" | "claude"))
        || !params
            .get("eventName")
            .and_then(Value::as_str)
            .is_some_and(|event| {
                event
                    .replace(['_', '-'], "")
                    .eq_ignore_ascii_case("sessionstart")
            })
        || !matches!(
            params.get("hookSource").and_then(Value::as_str),
            Some("startup" | "resume" | "clear")
        )
    {
        return Ok(None);
    }
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime.insert(
        if agent == Some("claude") {
            "sessionChatClaudeStartedAt"
        } else {
            "sessionChatCodexStartedAt"
        }
        .to_string(),
        json!(crate::domain::now_iso()),
    );
    let update = json!({
        "projectId": session.get("projectId"),
        "sessionId": session.get("sessionId"),
        "runtimeSettings": runtime,
    });
    repository
        .update_session(update.as_object().unwrap())
        .map(Some)
}

pub(crate) fn read_fleet(
    session: &serde_json::Value,
    screen: Option<&str>,
) -> anyhow::Result<Option<crate::session_chat_agent_fleet::SessionChatAgentFleet>> {
    match session_chat_option_agent(session_chat_agent_for_session(session).as_deref()) {
        Some(SessionChatOptionAgent::Claude) => {
            crate::session_chat_claude_fleet::read_claude_fleet(session, screen)
        }
        Some(SessionChatOptionAgent::Codex) => {
            crate::session_chat_codex_fleet::read_codex_fleet(session)
        }
        _ => Ok(None),
    }
}

pub(crate) fn spawn_fleet_status_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            let state = state.clone();
            let _ = tokio::task::spawn_blocking(move || refresh_fleet_status(&state)).await;
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    })
}

fn refresh_fleet_status(state: &AppState) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return;
    };
    let publisher = crate::session_chat_compacting::SessionChatCompactingPublisher::new(state);
    for session in sessions {
        let agent = session_chat_agent_for_session(&session);
        if !matches!(
            session_chat_option_agent(agent.as_deref()),
            Some(SessionChatOptionAgent::Claude | SessionChatOptionAgent::Codex)
        ) {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            session.get("projectId").and_then(serde_json::Value::as_str),
            session.get("sessionId").and_then(serde_json::Value::as_str),
        ) else {
            continue;
        };
        let key = crate::server::session_observer_key(project_id, session_id);
        if state
            .session_chat_option_cache
            .lock()
            .ok()
            .is_some_and(|cache| {
                cache.get(&key).is_some_and(|entry| {
                    entry.value.fleet_observed
                        && entry.fetched_at.elapsed() < SESSION_CHAT_OPTION_CACHE_TTL
                })
            })
        {
            continue;
        }
        // This daemon-wide observer reads evidence only. The full chat detector
        // can dismiss a Claude diff panel, which belongs to the user's open chat.
        let capture = if session_chat_option_agent(agent.as_deref())
            == Some(SessionChatOptionAgent::Claude)
        {
            crate::zmx::read_zmx_session_history_capture(&repository, project_id, session_id)
                .ok()
                .filter(|capture| !capture.truncated)
        } else {
            None
        };
        let fleet = read_fleet(
            &session,
            capture.as_ref().map(|capture| capture.text.as_str()),
        )
        .unwrap_or_else(|_| {
            state
                .session_chat_option_cache
                .lock()
                .ok()
                .and_then(|cache| cache.get(&key).and_then(|entry| entry.value.fleet.clone()))
                .map(|fleet| fleet.unavailable())
        });
        let observation = match session_chat_option_agent(agent.as_deref()) {
            Some(SessionChatOptionAgent::Codex) => Some(None),
            Some(SessionChatOptionAgent::Claude) => capture.map(|capture| {
                crate::session_chat_terminal_activity::detect_session_chat_terminal_activity(
                    agent.as_deref(),
                    &capture.text,
                )
            }),
            _ => None,
        };
        let activity = observation.as_ref().and_then(|activity| activity.as_ref());
        let monitor =
            crate::session_chat_terminal_activity::is_session_chat_monitor_activity(activity);
        if let Ok(mut cache) = state.session_chat_option_cache.lock() {
            if let Some(entry) = cache.get_mut(&key) {
                entry.projected_fleet =
                    Some(fleet.as_ref().is_some_and(|fleet| fleet.is_working()));
                if observation.is_some() {
                    entry.projected_monitor = Some(monitor);
                }
                entry.value.fleet = fleet.clone();
                entry.value.fleet_observed = true;
            }
        }
        if fleet.as_ref().is_some_and(|fleet| fleet.is_working())
            != crate::session_chat_compacting::session_chat_fleet_detected_at(&session).is_some()
        {
            publisher.publish_fleet(
                project_id,
                session_id,
                fleet
                    .as_ref()
                    .filter(|fleet| fleet.is_working())
                    .map(|fleet| fleet.detected_at.as_str()),
            );
        }
        if observation.is_some()
            && monitor
                != crate::session_chat_compacting::session_chat_monitor_detected_at(&session)
                    .is_some()
        {
            publisher.publish_monitor(
                project_id,
                session_id,
                activity
                    .filter(|_| monitor)
                    .map(|activity| activity.detected_at.as_str()),
            );
        }
    }
}
