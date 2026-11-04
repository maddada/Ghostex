//! `/api/openCoordinatorThread`: a row of a coordinator's Threads panel was clicked.
//!
//! CDXC:Coordinators 2026-10-08 WHY:
//! The user: "when I click on this session in the threads list ... it's not opening it". Most of a coordinator's threads are resolved and their sessions closed (`stopped`), and a closed session has no live terminal or chat to open, so the click must resume it first. Each client used to decide that on its own: the desktop and the phone woke a `stopped` row and the web build never did, so a click there showed a dead transcript. gxserver now decides once for every client: it checks the thread belongs to the coordinator whose chat asks, resumes its session when it is closed (the same `/api/wakeSession` `ghostex coordinator reopen` uses), and only then does the chat core ask its host to focus it. Opening a thread does not reopen its task: the thread stays resolved, so the coordinator is not sent its reports again (`ghostex coordinator reopen` does that on purpose).
//! SEE-ALSO: packages/gx-chat-core/src/extras/coordinator_threads.rs (`open_thread`, the call and the focus that follows), apps/desktop/src/app/session_chat_fork_branches.rs, apps/gpui-web/src/app/chat_host.rs and apps/mobile/app/src/chat/native/NativeChatScreen.tsx (the focus).

use super::*;

use crate::coordinators::{list_remote_threads_for, read_thread, RemoteThreadRecord};

pub(crate) async fn handle_open_coordinator_thread_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let worker_state = state.clone();
    let worker_endpoint = endpoint_path.clone();
    let worker_request_id = request_id.clone();
    match tokio::task::spawn_blocking(move || {
        let wake_request_id = worker_request_id.clone();
        handle_domain_http(
            &worker_state,
            worker_endpoint,
            worker_request_id,
            &body,
            |repository, db, params, _| {
                open_thread(&worker_state, repository, db, params, wake_request_id)
            },
        )
    })
    .await
    {
        Ok(response) => response,
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::corrupt_state(format!("openCoordinatorThread failed: {error}")),
        ),
    }
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}

/// `projectId`/`sessionId` name the coordinator (the chat that asks), `threadProjectId`/
/// `threadSessionId` the thread. Answers `{projectId, sessionId, resumed}` for the thread.
fn open_thread(
    state: &AppState,
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    params: &Map<String, Value>,
    request_id: String,
) -> std::result::Result<Value, DomainStateError> {
    let coordinator = (
        param(params, "projectId")?.to_string(),
        param(params, "sessionId")?.to_string(),
    );
    let project_id = param(params, "threadProjectId")?;
    let session_id = param(params, "threadSessionId")?;
    let server_id = params
        .get("threadServerId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|server_id| !server_id.is_empty() && *server_id != repository.server_id);
    let local = match server_id {
        Some(_) => None,
        None => read_thread(db, project_id, session_id)?
            .filter(|thread| thread.coordinator_key() == coordinator),
    };
    let Some(thread) = local else {
        // A client that sends no server id (the phone, an older build) still finds a thread on
        // another computer by its ids.
        let remote = list_remote_threads_for(db, &coordinator)?
            .into_iter()
            .find(|thread| {
                thread.project_id == project_id
                    && thread.session_id == session_id
                    && server_id.is_none_or(|server_id| thread.server_id == server_id)
            })
            .ok_or_else(|| {
                DomainStateError::not_found("That session is not a thread of this coordinator.")
            })?;
        return open_remote_thread(remote);
    };
    let session = repository
        .get_session(&thread.project_id, &thread.session_id)?
        .ok_or_else(|| DomainStateError::not_found("That thread's session no longer exists."))?;
    let closed = session.get("lifecycleState").and_then(Value::as_str) == Some("stopped");
    if closed {
        let mut wake = Map::new();
        wake.insert("projectId".into(), json!(thread.project_id));
        wake.insert("sessionId".into(), json!(thread.session_id));
        let woken = dispatch_zmx_lifecycle_http_blocking(
            state,
            "/api/wakeSession".to_string(),
            request_id,
            wake,
        );
        if !woken.response.status().is_success() {
            return Err(DomainStateError::corrupt_state(
                "The thread's session could not be resumed. Try `ghostex coordinator reopen` for it.",
            ));
        }
    }
    Ok(json!({
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "resumed": closed,
    }))
}

/// A thread on another computer: resumed there through this computer's connection to it when its
/// session is closed, then answered with its computer's `serverId`, which the host opens it on.
///
/// CDXC:RemoteMachines 2026-11-04 WHY:
/// The Workers list now lists a coordinator's threads on other computers (coordinators/panel.rs), and a click must open the session on the computer that runs it, as the sidebar's nested row does. The same rule as a local thread: a closed one is resumed first, so it never opens as a dead transcript.
fn open_remote_thread(thread: RemoteThreadRecord) -> std::result::Result<Value, DomainStateError> {
    let closed = thread.state() == "closed";
    if closed {
        let computer = crate::connected_computers::find(&thread.server_id)
            .filter(|computer| computer.agent_access)
            .ok_or_else(|| {
                DomainStateError::not_found(
                    "That worker's computer isn't connected. Connect it in Settings › Remote and try again.",
                )
            })?;
        let woken = super::coordinator_remote_runtime::remote_rpc(
            &computer,
            "/api/wakeSession",
            json!({ "projectId": thread.project_id, "sessionId": thread.session_id }),
        );
        if woken.is_none() {
            return Err(DomainStateError::corrupt_state(format!(
                "The worker's session on {} could not be resumed.",
                computer.name
            )));
        }
    }
    Ok(json!({
        "serverId": thread.server_id,
        "projectId": thread.project_id,
        "sessionId": thread.session_id,
        "resumed": closed,
    }))
}
