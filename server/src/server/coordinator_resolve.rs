//! `/api/resolveCoordinatorThread`: Mark Thread Done and Reopen Thread for every client.

use super::*;
use crate::presentation::effective_lifecycle_state;

/// What a failed wake or close answered, for the thread's own error message.
async fn routed_failure(routed: RoutedResponse) -> String {
    let bytes = to_bytes(routed.response.into_body(), 64 * 1024)
        .await
        .unwrap_or_default();
    serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|body| {
            body.get("message")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| String::from_utf8_lossy(&bytes).trim().to_string())
}

/// Runs `work` against the gxserver database, answering a database that will not open as an
/// internal error.
fn with_repository<T>(
    state: &AppState,
    work: impl FnOnce(&rusqlite::Connection, &DomainRepository<'_>) -> Result<T, DomainStateError>,
) -> Result<T, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    work(&db, &repository)
}

/// How long a reopen waits for the thread's session to wake before answering.
const REOPEN_WAKE_WAIT: std::time::Duration = std::time::Duration::from_secs(8);

/// Marks a thread done and closes its session (`keepOpen` leaves it open, never parked), or reopens it,
/// waking its session first when it is not running.
///
/// CDXC:Coordinators 2026-10-13 WHY:
/// The user asked for Mark Thread Done and Reopen Thread on thread rows on the desktop and the phone as well as `ghostex orchestrator resolve` and `reopen`. The wake and close steps live here instead of in each client, so every door resolves a thread the same way, through the one `resolved` flag `setCoordinatorThreadResolved` already keeps.
/// SEE-ALSO: server/src/ghostex_cli/coordinator/threads.rs (`set_resolved`), apps/desktop/src/app/gx_store/coordinator_actions.rs (the desktop's rows), apps/mobile/app/src/screens/sessions-screen/use-sessions-screen-menus.tsx (the phone's, through the CLI).
pub(crate) async fn handle_resolve_coordinator_thread_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let fail = |error: DomainStateError| {
        domain_error_response(endpoint_path.clone(), request_id.clone(), error)
    };
    let params = match crate::domain::read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return fail(error),
    };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let Some(resolved) = params.get("resolved").and_then(Value::as_bool) else {
        return fail(DomainStateError::bad_request(
            "Say resolved: true or false.",
        ));
    };
    let close_session = resolved && params.get("keepOpen").and_then(Value::as_bool) != Some(true);
    let lifecycle = match with_repository(state, |_, repository| {
        match repository.get_session(&project_id, &session_id)? {
            Some(session) => Ok(effective_lifecycle_state(&session)),
            None => Err(DomainStateError::not_found(
                "The thread session does not exist.",
            )),
        }
    }) {
        Ok(lifecycle) => lifecycle,
        Err(error) => return fail(error),
    };
    let closed = lifecycle == "stopped";
    let session_body = json!({ "params": { "projectId": project_id, "sessionId": session_id } });
    let close_body = json!({ "params": {
        "projectId": project_id,
        "sessionId": session_id,
        "closeSource": "server.threadMarkedDone",
    } });
    // Woken before it is reopened: an open thread whose session is still closed is reported to the
    // coordinator as closed and marked done again, and a sleeping one would leave a message waiting.
    // CDXC:Coordinators 2026-10-12 WHY: a sleeping thread is woken too; reopen used to wake only a closed (`stopped`) one, so it answered "Reopened" and left a sleeping thread asleep with a message waiting for it.
    let resumed = !resolved && lifecycle != "running";
    let mut resuming = false;
    if resumed {
        // CDXC:Coordinators 2026-11-10 WHY: waking a closed thread resumes its agent, which can take
        // longer than a client waits for this answer: `ghostex coordinator reopen` gave up after 15
        // seconds and the thread stayed done (GitHub #531). A wake that fails quickly still refuses
        // the reopen; one still going after `REOPEN_WAKE_WAIT` finishes on its own while the thread
        // is reopened, and the supervisor's grace for a closed session (coordinator_runtime.rs,
        // `CLOSED_GRACE_MS`) keeps it from being reported closed meanwhile.
        let wake_state = state.clone();
        let wake_body = session_body.clone();
        let mut wake = tokio::spawn(async move {
            handle_zmx_lifecycle_http(
                &wake_state,
                "/api/wakeSession".to_string(),
                uuid::Uuid::new_v4().to_string(),
                &wake_body,
            )
            .await
        });
        match tokio::time::timeout(REOPEN_WAKE_WAIT, &mut wake).await {
            Ok(Ok(woke)) if !woke.response.status().is_success() => {
                let detail = routed_failure(woke).await;
                return fail(DomainStateError::bad_request(format!(
                    "Could not resume the thread's session: {detail}. It was not reopened."
                )));
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                return fail(DomainStateError::bad_request(format!(
                    "Could not resume the thread's session: {error}. It was not reopened."
                )));
            }
            Err(_) => resuming = true,
        }
    }
    let mut result = match with_repository(state, |db, repository| {
        let mut inner = Map::new();
        inner.insert("projectId".to_string(), json!(project_id));
        inner.insert("sessionId".to_string(), json!(session_id));
        inner.insert("resolved".to_string(), json!(resolved));
        inner.insert("closeSession".to_string(), json!(close_session));
        coordinator_runtime::handle_coordinator_http(
            state,
            "/api/setCoordinatorThreadResolved",
            db,
            repository,
            &inner,
        )
    }) {
        Ok(result) => result,
        Err(error) => return fail(error),
    };
    if close_session && !closed {
        let killed = handle_zmx_lifecycle_http(
            state,
            "/api/killSession".to_string(),
            uuid::Uuid::new_v4().to_string(),
            &close_body,
        )
        .await;
        if !killed.response.status().is_success() {
            let detail = routed_failure(killed).await;
            // Its own code, so a client can say how to close it (the CLI names `ghostex agents close`).
            return fail(DomainStateError {
                code: "sessionNotClosed",
                message: format!(
                    "The thread was marked done, but its session did not close: {detail}."
                ),
            });
        }
    }
    result["sessionClosed"] = json!(close_session);
    result["sessionResumed"] = json!(resumed);
    result["sessionResuming"] = json!(resuming);
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}
