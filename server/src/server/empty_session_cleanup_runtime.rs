//! Runs the new-session cleanup after a user's new-session create: reads each candidate's agent
//! input box and closes the ones that are still fully empty through `/api/transitionSession`.
//!
//! SEE-ALSO: server/src/empty_session_cleanup.rs (the marker, the rule and CDXC:Sessions 2026-10-09).

use super::*;

use crate::empty_session_cleanup::{empty_session_candidates, EmptySessionCandidate};

/// The `/api/transitionSession` reason of every close this cleanup makes.
const CLOSE_REASON: &str = "replacedByNewSession";

/// CDXC:Sessions 2026-10-04 WHY:
/// A chat composer pushes its draft when it loses focus, which is the moment the new session takes it, so the create can land before that push. Waiting this long lets the push arrive before the old session is judged empty.
const SETTLE: Duration = Duration::from_secs(2);

pub(crate) fn schedule_empty_session_cleanup(
    state: Arc<AppState>,
    project_id: String,
    new_session_id: String,
) {
    tokio::spawn(async move {
        tokio::time::sleep(SETTLE).await;
        for candidate in read_candidates(&state, &project_id, &new_session_id).await {
            if !input_box_is_empty(&candidate).await {
                continue;
            }
            // Read again right before closing: the user may have typed into its chat meanwhile.
            let still_empty = read_candidates(&state, &project_id, &new_session_id)
                .await
                .iter()
                .any(|again| again.session_id == candidate.session_id);
            if still_empty {
                let (state, project_id) = (state.clone(), project_id.clone());
                let new_session_id = new_session_id.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    close_empty_session(&state, &project_id, &candidate.session_id, &new_session_id)
                })
                .await;
            }
        }
    });
}

async fn read_candidates(
    state: &Arc<AppState>,
    project_id: &str,
    new_session_id: &str,
) -> Vec<EmptySessionCandidate> {
    let state = state.clone();
    let (project_id, new_session_id) = (project_id.to_string(), new_session_id.to_string());
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        empty_session_candidates(&db, &repository, &project_id, &new_session_id).ok()
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// Only an input box the agent's screen grammar recognises as empty counts; a screen that cannot
/// be read, or an agent mid-dialog, holds text as far as this rule knows.
async fn input_box_is_empty(candidate: &EmptySessionCandidate) -> bool {
    crate::session_chat_send::capture_session_terminal_text_vt(&candidate.zmx_name)
        .await
        .and_then(|screen| {
            crate::session_chat_composer::session_chat_composer_input(&candidate.agent_id, &screen)
        })
        .is_some_and(|input| input.is_empty())
}

/// Closes one empty session through `/api/transitionSession` and logs it.
///
/// CDXC:Sessions 2026-10-15 WHY:
/// Issue #236 found empty-session closes impossible to diagnose: the session just vanished and no persisted log said why. Closing a session the user did not close is rare (the setting is off by default) and removes what they see, so each close is an important entry, persisted without a diagnostic area, naming the session, the reason and what triggered it.
fn close_empty_session(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    new_session_id: &str,
) -> bool {
    let mut params = Map::new();
    params.insert("action".into(), json!("close"));
    params.insert("projectId".into(), json!(project_id));
    params.insert("reason".into(), json!(CLOSE_REASON));
    params.insert("closeSource".into(), json!("server.emptySessionCleanup"));
    params.insert("sessionId".into(), json!(session_id));
    let response = dispatch_zmx_lifecycle_http_blocking(
        state,
        "/api/transitionSession".to_string(),
        "empty-session-cleanup".to_string(),
        params,
    );
    if let Ok(db) = open_gxserver_database(&state.paths) {
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let _ =
            schedule_presentation_session_delta(state, &db, &repository, project_id, session_id);
    }
    let closed = response.response.status().is_success();
    if closed {
        let _ = state.logger.log(GxserverLogInput {
            level: LogLevel::Warn,
            event: "emptySessionClosed".to_string(),
            server_id: Some(state.metadata.server_id.clone()),
            request_id: None,
            client: None,
            duration_ms: None,
            error: None,
            details: Some(json!({
                "newSessionId": new_session_id,
                "trigger": "newSession",
                "projectId": project_id,
                "reason": CLOSE_REASON,
                "sessionId": session_id,
            })),
        });
    }
    closed
}
