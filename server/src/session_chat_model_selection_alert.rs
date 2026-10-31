//! A model change that cannot be applied: the red sidebar dot it raises and the log line that
//! explains it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_chat_compacting::SessionChatCompactingPublisher;
use crate::session_chat_model_selection::PendingModelSelection;
use crate::storage::open_gxserver_database;

/// Runtime-settings key holding the reason, published as `modelSelectionFailure`.
pub(crate) const MODEL_SELECTION_FAILURE_KEY: &str = "sessionChatModelSelectionFailure";

/// How long a retrying change may keep failing before it counts as failed. A change can wait a
/// few seconds for the agent's input box on any ordinary turn; that must not flash the dot.
const STUCK_AFTER_MS: i64 = 30_000;

type SelectionKey = (String, String, String);

fn first_failures() -> &'static Mutex<HashMap<SelectionKey, i64>> {
    static FIRST: OnceLock<Mutex<HashMap<SelectionKey, i64>>> = OnceLock::new();
    FIRST.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The failure reason a session carries, while it carries one.
pub(crate) fn session_model_selection_failure(session: &Value) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(|settings| settings.get(MODEL_SELECTION_FAILURE_KEY))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// CDXC:SessionChat 2026-09-27 DECISION:
/// User: "we shouldn't send on wrong model we must show a red dot on the session in the sidebar if we fail and log reason for failing and the terminal screen copy so we can fix this issue". A model change the agent refused for good, or one still failing after 30 seconds of retries, marks the session (the sidebar draws a red dot with the reason) and writes one `sessionChatModelSelectionFailed` line with the terminal screen to the send-failure log. Messages behind it stay held; picking a model again, or the change finally applying, clears the mark.
/// CDXC:SessionChat 2026-10-31 DECISION:
/// User, answering Q1 on GitHub #374: a refused mode gets no red dot and doesn't hold later messages, since the menu now explains it. This narrows the 2026-09-27 decision above to everything else: a mode the agent's own cycle leaves out ends quietly in session_chat_model_selection.rs (`mode_was_refused`) and is greyed with how to turn it on (session_chat_mode_availability.rs). Any other failed model change keeps the dot and the hold.
/// SEE-ALSO: session_chat_queue_runtime/scheduler.rs (holds prompts behind a failed change), session_chat_send/send_http.rs (holds a new send behind a pending change), packages/gx-core/src/sidebar_view/rows.rs and apps/desktop/src/app/native_sidebar/status.rs (the red dot).
pub(crate) async fn note_model_selection_failure(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    pending: &PendingModelSelection,
    error: &DomainStateError,
    final_failure: bool,
) {
    let now = chrono::Utc::now().timestamp_millis();
    let since = first_failures().lock().map_or(now, |mut failures| {
        *failures
            .entry((
                project_id.to_string(),
                session_id.to_string(),
                pending.id.clone(),
            ))
            .or_insert(now)
    });
    if !final_failure && now - since < STUCK_AFTER_MS {
        return;
    }
    // The connection must not live across the log write's await.
    let session = {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
            return;
        };
        session
    };
    if session_model_selection_failure(&session).is_some() {
        return;
    }
    let target = [pending.model.as_str(), pending.effort.as_str()]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let reason = if target.is_empty() {
        format!("The chat option change failed: {}", error.message)
    } else {
        format!("The change to {target} failed: {}", error.message)
    };
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        MODEL_SELECTION_FAILURE_KEY,
        Some(&reason),
    );
    let zmx_name = session
        .get("zmxName")
        .and_then(Value::as_str)
        .map(str::to_string);
    crate::session_chat_send_diagnostics::record_model_selection_failure(
        state,
        project_id,
        session_id,
        zmx_name,
        json!({
            "selectionId": pending.id,
            "model": pending.model,
            "effort": pending.effort,
            "scope": pending.scope,
            "options": pending.options,
            "code": error.code,
            "message": error.message,
            "final": final_failure,
            "failingSinceMs": since,
        }),
    )
    .await;
}

/// Clears the mark once the change applied or the user picked again.
pub(crate) fn clear_model_selection_failure(state: &AppState, project_id: &str, session_id: &str) {
    if let Ok(mut failures) = first_failures().lock() {
        failures.retain(|(project, session, _), _| project != project_id || session != session_id);
    }
    SessionChatCompactingPublisher::new(state).publish_marker(
        project_id,
        session_id,
        MODEL_SELECTION_FAILURE_KEY,
        None,
    );
}
