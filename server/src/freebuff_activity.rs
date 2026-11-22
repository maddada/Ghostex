//! CDXC:SessionStatus 2026-10-04 DECISION:
//! User: "when freebuff is working I don't see any indication in the sidebar that it's working".
//! Freebuff has no hooks and keeps its terminal title fixed while it works, so neither usual activity source sees its turns. Its saved chat does: a prompt without a finished reply is a running turn, and the reply's `isComplete` ends it. This observer turns those changes into the same hook events other agents send (`UserPromptSubmit`, `Stop`), so the sidebar, phone and CLI show Freebuff working and done.
//! SEE-ALSO: server/src/session_chat_freebuff.rs owns the chat file location and the session's chat binding.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde_json::{json, Map, Value};

use crate::agents::dispatch_agent_endpoint;
use crate::domain::DomainRepository;
use crate::server::{read_runtime_text, read_session_text, session_observer_key, AppState};
use crate::storage::open_gxserver_database;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnState {
    Working,
    Complete,
}

struct Observed {
    path: PathBuf,
    modified: SystemTime,
    state: Option<TurnState>,
}

pub(crate) fn spawn_freebuff_activity_task(state: &Arc<AppState>) -> tokio::task::JoinHandle<()> {
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

/// The newest prompt's turn: running until a reply after it is marked complete.
fn turn_state(messages: &[Value]) -> Option<TurnState> {
    let last_prompt = messages
        .iter()
        .rposition(|message| message.get("variant").and_then(Value::as_str) == Some("user"))?;
    let reply = messages[last_prompt + 1..].iter().rev().find(|message| {
        message
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("ai-"))
    });
    Some(
        if reply
            .and_then(|reply| reply.get("isComplete"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            TurnState::Complete
        } else {
            TurnState::Working
        },
    )
}

fn chat_messages_path(
    repository: &DomainRepository<'_>,
    session: &Value,
    sessions: &[Value],
) -> Option<PathBuf> {
    if let Some(path) = read_runtime_text(session, "agentSessionPath")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
    {
        return Some(path);
    }
    crate::session_chat_freebuff::discover_freebuff_chat_for_session(repository, session, sessions)
        .map(|(_, path)| path)
}

fn refresh(state: &AppState, observed: &mut HashMap<String, Observed>) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    // Every two seconds: with no Freebuff session running there is nothing to watch, and one
    // indexed column read says so without hydrating every running session.
    match repository.has_running_session_with_agent("freebuff") {
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
        if read_session_text(session, "agentId").as_deref() != Some("freebuff") {
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
        let Some(path) = chat_messages_path(&repository, session, &sessions) else {
            continue;
        };
        let Ok(modified) = std::fs::metadata(&path).and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if observed
            .get(&key)
            .is_some_and(|seen| seen.path == path && seen.modified == modified)
        {
            continue;
        }
        // A torn read mid-rewrite fails to parse; the next pass sees the finished file.
        let Some(messages) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<Value>>(&raw).ok())
        else {
            continue;
        };
        let next = turn_state(&messages);
        let previous = observed.get(&key).map(|seen| seen.state);
        observed.insert(
            key,
            Observed {
                path,
                modified,
                state: next,
            },
        );
        let event = match (previous, next) {
            (Some(previous), Some(next)) if previous == Some(next) => continue,
            (_, Some(TurnState::Working)) => "UserPromptSubmit",
            // A turn that was already finished when first seen (a woken session) is not news.
            (Some(Some(TurnState::Working)), Some(TurnState::Complete)) => "Stop",
            _ => continue,
        };
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        params.insert("agentName".to_string(), json!("freebuff"));
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
