//! CDXC:SessionStatus 2026-10-28 WHY:
//! DeepSeek Harness's terminal app has no Ghostex hooks, so neither usual activity source sees its turns. Its session log does: a `turn/start` with no `turn/end` after it is a running turn. This observer turns those changes into the same hook events other agents send (`UserPromptSubmit`, `Stop`), so the sidebar, phone, CLI and notifications show it working and done, the way Freebuff's chat file does for Freebuff. DSH writes a step only when it settles, so a long first model call can show working a few seconds late; dsh-tui's own OSC 7501 report covers that under the Combined status source.
//! SEE-ALSO: server/src/freebuff_activity.rs (the same observer shape), server/src/session_chat_dsh.rs (the log location and the session binding).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{json, Map, Value};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, read_session_text, session_observer_key, AppState};
use crate::storage::open_gxserver_database;

struct Observed {
    path: PathBuf,
    modified: SystemTime,
    len: u64,
    running: Option<bool>,
}

pub(crate) fn spawn_dsh_activity_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
    let state = state.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut observed = HashMap::new();
        loop {
            let state = state.clone();
            observed = tokio::task::spawn_blocking(move || {
                refresh(&state, &mut observed);
                observed
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

fn session_log(
    repository: &DomainRepository<'_>,
    session: &Value,
    sessions: &[Value],
) -> Option<PathBuf> {
    let supplied = read_runtime_text(session, "agentSessionPath").map(PathBuf::from);
    if let Some(id) = read_runtime_text(session, "agentSessionId") {
        return crate::session_chat_dsh::dsh_session_log_for(&id, supplied.as_deref());
    }
    crate::session_chat_dsh::discover_dsh_session_for_session(repository, session, sessions)
        .map(|(_, path)| path)
}

fn refresh(state: &AppState, observed: &mut HashMap<String, Observed>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    // Every two seconds: with no DeepSeek Harness session running there is nothing to watch, and
    // one indexed column read says so without hydrating every running session.
    match repository.has_running_session_with_agent("dsh") {
        Ok(true) => {}
        Ok(false) => {
            observed.clear();
            return;
        }
        Err(_) => return,
    }
    let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") else {
        return;
    };
    let mut active = HashSet::new();
    for session in &sessions {
        if read_session_text(session, "agentId").as_deref() != Some("dsh") {
            continue;
        }
        let (Some(project_id), Some(session_id)) = (
            read_session_text(session, "projectId"),
            read_session_text(session, "sessionId"),
        ) else {
            continue;
        };
        let key = session_observer_key(&project_id, &session_id);
        active.insert(key.clone());
        let Some(path) = session_log(&repository, session, &sessions) else {
            continue;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if observed.get(&key).is_some_and(|seen| {
            seen.path == path && seen.modified == modified && seen.len == metadata.len()
        }) {
            continue;
        }
        let Some(log) = crate::session_chat_dsh::read_dsh_log(&path) else {
            continue;
        };
        let next = crate::session_chat_dsh::dsh_turn_running(&log);
        let previous = observed.get(&key).map(|seen| seen.running);
        observed.insert(
            key,
            Observed {
                path,
                modified,
                len: metadata.len(),
                running: next,
            },
        );
        let event = match (previous, next) {
            (Some(previous), Some(next)) if previous == Some(next) => continue,
            (_, Some(true)) => "UserPromptSubmit",
            // A turn that was already finished when first seen (a woken session) is not news.
            (Some(Some(true)), Some(false)) => "Stop",
            _ => continue,
        };
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("agentName".to_string(), json!("dsh"));
        params.insert("eventName".to_string(), json!(event));
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
    observed.retain(|key, _| active.contains(key));
}
