//! A user's own action in a chat (Stop, answering a prompt, typing into a session) that gxserver
//! answered with a request for the owner's approval: gxserver tells the chat in plain words what
//! waits and where, and sends the call itself once the request is approved, so nobody has to do
//! it again.
//!
//! CDXC:Security 2026-11-08 WHY:
//! User, on a red line under "Interrupted the agent" after pressing Escape in the chat of a session on their connected Windows PC: "wtf is this line". The line was gxserver's answer meant for agents, "Waiting for your approval on your computer or phone: Stop what session G506f is doing. Approve it there, then repeat this call with approvalId \"apr_…\"": a person can't repeat a call. A chat asks with `finishWhenApproved` (desktop and web, `native_chat/rpc.rs`); its answer then names what waits and where, with the code `approvalPending` (which no app prompts for), and gxserver repeats the call itself with the approval, through its own HTTP route and the caller's own token (the token file, or the session's token), the way the voice tools finish theirs (voice_agent/approvals.rs). Agents and the CLI, which don't ask for it, keep the answer they act on.

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::http::StatusCode;
use ghostex_gx_protocol::approval::{ApprovalRequest, ApprovalStatus, APPROVAL_ID_PARAM};
use serde_json::{json, Value};

use crate::{
    constants::GXSERVER_PROTOCOL_VERSION,
    protocol::rpc_error,
    security::{self, Principal},
};

use super::{routed_json, AppState, RoutedResponse};

/// The param a chat sends to have gxserver finish its call once it is approved.
pub(crate) const FINISH_WHEN_APPROVED_PARAM: &str = "finishWhenApproved";

/// The routes a chat's own actions use that can wait for an approval.
pub(crate) const FINISHABLE_ENDPOINTS: &[&str] = &[
    "/api/interruptSessionChat",
    "/api/answerSessionChatPrompt",
    "/api/sendSessionText",
    "/api/sendSessionEnter",
    "/api/sendSessionMessage",
];

const POLL: Duration = Duration::from_secs(1);

/// Whether this call asked to be finished by gxserver once approved.
pub(crate) fn wants_finish(endpoint_path: &str, body: &Value) -> bool {
    FINISHABLE_ENDPOINTS.contains(&endpoint_path)
        && body
            .get("params")
            .and_then(|params| params.get(FINISH_WHEN_APPROVED_PARAM))
            .and_then(Value::as_bool)
            == Some(true)
}

/// The token gxserver repeats the call with: the caller's own. `None` for a caller gxserver can't
/// act as (it then answers as it does for agents).
fn token_for(state: &AppState, caller: &Principal) -> Option<String> {
    match caller {
        Principal::LocalAgent => Some(state.auth_token.clone()),
        Principal::Session {
            project_id,
            session_id,
            ..
        } => state
            .security
            .session_tokens
            .token_of(project_id, session_id),
        _ => None,
    }
}

/// The plain answer for a chat whose action waits on `request`, after arranging for gxserver to
/// finish it; `None` when gxserver can't act as this caller.
pub(crate) fn waiting_answer(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    request_id: &str,
    body: &Value,
    request: &ApprovalRequest,
) -> Option<RoutedResponse> {
    let token = token_for(state, caller)?;
    let mut params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let object = params.as_object_mut()?;
    object.remove(APPROVAL_ID_PARAM);
    object.remove(FINISH_WHEN_APPROVED_PARAM);
    finish_once_approved(state, caller.clone(), endpoint_path, params, token, request);
    let computer = crate::connected_computers::this_computer_name();
    let message = format!(
        "Waiting for your approval: {}. Approve it on {computer} (Ghostex shows it there) or on your phone, and it goes ahead by itself.",
        request.summary
    );
    let mut answer = serde_json::to_value(rpc_error(
        super::security_gates::APPROVAL_PENDING_ERROR,
        message,
        Some(request_id.to_string()),
    ))
    .unwrap_or_else(|_| json!({}));
    if let Some(object) = answer.as_object_mut() {
        object.insert(
            "approval".to_string(),
            serde_json::to_value(request).unwrap_or(Value::Null),
        );
        object.insert("finishesWhenApproved".to_string(), Value::Bool(true));
    }
    Some(routed_json(
        Some(endpoint_path.to_string()),
        StatusCode::FORBIDDEN,
        answer,
    ))
}

/// Watches `request` until it is decided or lapses, and sends the call with its approval when it
/// is approved. One watcher per request, however often the user pressed the button.
fn finish_once_approved(
    state: &AppState,
    caller: Principal,
    endpoint_path: &str,
    params: Value,
    token: String,
    request: &ApprovalRequest,
) {
    static WATCHED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let watched = WATCHED.get_or_init(Default::default);
    if !watched
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(request.id.clone())
    {
        return;
    }
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let approvals = state.security.approvals.clone();
    let host = match state.config.listeners.local.host.as_str() {
        "" | "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        host => host.to_string(),
    };
    let url = format!(
        "http://{host}:{}{endpoint_path}",
        state.config.listeners.local.port
    );
    let id = request.id.clone();
    runtime.spawn(async move {
        let decided = loop {
            tokio::time::sleep(POLL).await;
            match approvals.read(&id, &caller, security::now_ms()) {
                Some(found) if found.status == ApprovalStatus::Pending => continue,
                other => break other.map(|found| found.status),
            }
        };
        if decided == Some(ApprovalStatus::Approved) {
            let mut params = params;
            if let Some(object) = params.as_object_mut() {
                object.insert(APPROVAL_ID_PARAM.to_string(), Value::String(id.clone()));
            }
            let body = json!({"params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION});
            // The route records the outcome in the security log, like any approved call.
            let _ = tokio::task::spawn_blocking(move || {
                crate::http_agent::answering_agent()
                    .post(&url)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .header(
                        "x-gxserver-protocol-version",
                        GXSERVER_PROTOCOL_VERSION.to_string(),
                    )
                    .config()
                    .timeout_global(Some(Duration::from_secs(60)))
                    .build()
                    .send(body.to_string())
                    .map(|response| response.status().as_u16())
            })
            .await;
        }
        WATCHED
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    });
}
