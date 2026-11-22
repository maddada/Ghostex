//! CDXC:SessionStatus 2026-10-16 WHY:
//! ZCode runs its `Stop` hook only for a turn its model finished; a turn that fails (a provider error such as "Insufficient balance", code 1113) or that the user cancels fires no hook at all, and ZCode has no failure or interrupt hook event, so the working that `UserPromptSubmit` set stayed on forever (observed 2026-10-16, throwaway G9kqs on ZCode 3.15.3). Its titles carry no status either. ZCode records every turn's outcome in its own database (`turn_usage.status`: running, completed, error, cancelled), so this observer watches that row for ZCode sessions that still show working and sends the `StopFailure` event other agents send for a turn that ended without finishing, which settles the session to idle.
//! SEE-ALSO: server/src/freebuff_activity.rs (the same observer shape), server/src/session_chat_zcode.rs (`zcode_database_path`).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, read_session_text, session_observer_key, AppState};
use crate::storage::open_gxserver_database;

pub(crate) fn spawn_zcode_turn_activity_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        // Turn ids already reported, per session, so one failed turn sends one event.
        let mut reported = HashMap::new();
        loop {
            let state = state.clone();
            reported = tokio::task::spawn_blocking(move || {
                refresh(&state, &mut reported);
                reported
            })
            .await
            .unwrap_or_default();
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }
        }
    })
}

/// The newest turn of a ZCode session when it ended without finishing: its id and end time (ms).
fn unfinished_turn_end(connection: &Connection, zcode_session_id: &str) -> Option<(String, i64)> {
    connection
        .query_row(
            "select turn_id, status, completed_at from turn_usage where session_id = ?1 order by started_at desc limit 1",
            [zcode_session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten()
        .and_then(|(turn_id, status, completed_at)| {
            matches!(status.as_str(), "error" | "cancelled")
                .then_some((turn_id, completed_at?))
        })
}

/// When the session's current working stint began (ms), so a turn that ended before it is old news.
pub(crate) fn working_since_ms(session: &Value) -> Option<i64> {
    let activity = session.pointer("/runtimeSettings/agentActivity")?;
    if activity.get("activity").and_then(Value::as_str) != Some("working") {
        return None;
    }
    ["workingStartedAt", "lastChangedAt"]
        .iter()
        .find_map(|key| activity.get(*key).and_then(Value::as_str))
        .and_then(crate::session_status::parse_iso_ms)
}

fn refresh(state: &AppState, reported: &mut HashMap<String, String>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    // Every two seconds: with no ZCode session running there is nothing to watch, and one
    // indexed column read says so without hydrating every running session.
    match repository.has_running_session_with_agent("zcode") {
        Ok(true) => {}
        Ok(false) => {
            reported.clear();
            return;
        }
        Err(_) => return,
    }
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return;
    };
    let db_path = crate::session_chat_zcode::zcode_database_path(None);
    let mut connection = None;
    let mut active = HashSet::new();
    for session in &sessions {
        if read_session_text(session, "agentId").as_deref() != Some("zcode") {
            continue;
        }
        let (Some(project_id), Some(session_id), Some(zcode_session_id)) = (
            read_session_text(session, "projectId"),
            read_session_text(session, "sessionId"),
            read_runtime_text(session, "agentSessionId")
                .filter(|id| crate::session_chat_zcode::is_safe_zcode_session_id(id)),
        ) else {
            continue;
        };
        let key = session_observer_key(&project_id, &session_id);
        active.insert(key.clone());
        let Some(working_since) = working_since_ms(session) else {
            continue;
        };
        if connection.is_none() {
            connection = ghostex_find::agent_store::open_agent_store(&db_path).ok();
            if let Some(connection) = &connection {
                let _ = connection.busy_timeout(Duration::from_secs(1));
            }
        }
        let Some((turn_id, ended_at)) = connection
            .as_ref()
            .and_then(|connection| unfinished_turn_end(connection, &zcode_session_id))
        else {
            continue;
        };
        if ended_at < working_since || reported.get(&key) == Some(&turn_id) {
            continue;
        }
        reported.insert(key, turn_id);
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("agentName".to_string(), json!("zcode"));
        params.insert("eventName".to_string(), json!("StopFailure"));
        let Ok(output) = dispatch_agent_endpoint(
            &repository,
            &db,
            &state.paths.home_dir,
            "/api/ingestAgentHookEvent",
            &params,
            None,
        ) else {
            continue;
        };
        if let Some((project_id, session_id)) = output.presentation_session {
            let _ = crate::server::schedule_presentation_session_delta(
                state,
                &db,
                &repository,
                &project_id,
                &session_id,
            );
        }
    }
    reported.retain(|key, _| active.contains(key));
}
