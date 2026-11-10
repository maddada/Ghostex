//! The `/api/sendSessionChatMessage` route's send: the plain send, plus what a message from a
//! caller means for coordinators. It reopens a done thread it is sent to (coordinators/reopen.rs)
//! and asks a coordinator Ghostex gave up on to try its reports again
//! (`request_report_retry`, server/src/server/coordinator_report_retry.rs). gxserver's own sends
//! (reports, nudges, automations) call `handle_send_session_chat_message_http` and do neither, and
//! a raw key (`key`, such as Shift+Tab's mode change) is no message, so it does neither either.

use super::*;

use axum::body::{to_bytes, Body};
use axum::http::{header::CONTENT_LENGTH, Response};

use crate::coordinators::{
    apply_reopen, caller_may_reopen, prepare_reopen, ReopenPlan, SessionKey,
};
use crate::security::Principal;
use crate::session_chat_send_requests::{send_once, SendRequestKey, SEND_REQUEST_ID_PARAM};

/// Result field: the send ran the reopen check, so a replay of it answers from the ledger as is.
const REOPEN_CHECKED: &str = "threadReopenChecked";
const RESPONSE_MAX_BYTES: usize = 16 * 1024 * 1024;

pub(crate) async fn handle_send_session_chat_message_from(
    state: &AppState,
    caller: &Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let started_ms = chrono::Utc::now().timestamp_millis();
    let attempt_caller = caller.clone();
    let response = send_once(
        state,
        endpoint_path,
        request_id,
        body,
        move |state, endpoint_path, request_id, body| async move {
            let target = target_key(&body).filter(|_| is_message(&body));
            let plan = match &target {
                Some(key) => plan_reopen(&state, &attempt_caller, key, started_ms).await,
                None => None,
            };
            let response =
                send_session_chat_message_attempt(&state, endpoint_path, request_id, &body).await;
            if !response.response.status().is_success() {
                return response;
            }
            let mut fields = Map::new();
            fields.insert(REOPEN_CHECKED.to_string(), json!(true));
            if let Some(plan) = plan {
                if run_reopen(&state, plan.clone(), sent_message(&body, started_ms)).await {
                    fields.insert("reopenedThread".to_string(), json!(true));
                    fields.insert("heldReport".to_string(), json!(plan.held_report));
                }
            }
            if let Some(key) = &target {
                crate::server::request_report_retry(key);
            }
            edit_result(response, |result| result.extend(fields)).await
        },
    )
    .await;
    recover_replayed_reopen(state, caller, body, response).await
}

/// CDXC:Coordinators 2026-11-10 WHY:
/// An attempt a gxserver restart cut off is settled by the ledger from the agent's transcript, with a stored result that never ran the reopen check. Its replay runs the check here, judged by the time the original attempt began (the ledger row's `createdAtMs`), so a Mark Done given since still wins. The held reply cannot be known after the fact, so none is returned.
async fn recover_replayed_reopen(
    state: &AppState,
    caller: &Principal,
    body: &Value,
    response: RoutedResponse,
) -> RoutedResponse {
    let (Some(target), Some(send_request_id)) = (
        target_key(body),
        body.get("params")
            .and_then(|params| params.get(SEND_REQUEST_ID_PARAM))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
    ) else {
        return response;
    };
    if !response.response.status().is_success() || !is_message(body) {
        return response;
    }
    let (response, result) = read_result(response).await;
    let unchecked = result.as_ref().is_some_and(|result| {
        result.get("duplicate") == Some(&json!(true))
            && result.get(REOPEN_CHECKED) != Some(&json!(true))
    });
    if !unchecked {
        return response;
    }
    let ledger_key = SendRequestKey::new(&target.0, &target.1, &send_request_id);
    // Replays of one recovered send take turns, so the first one's outcome is the one stored and
    // answered: a second would find the thread already reopened and store "not reopened".
    static RECOVERING: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _turn = RECOVERING.lock().await;
    if let Some(stored) =
        crate::session_chat_send_requests::stored_send_result(&state.paths, &ledger_key)
            .filter(|stored| stored.get(REOPEN_CHECKED) == Some(&json!(true)))
    {
        return edit_result(response, |result| {
            for field in [REOPEN_CHECKED, "reopenedThread", "heldReport"] {
                if let Some(value) = stored.get(field) {
                    result.insert(field.to_string(), value.clone());
                }
            }
        })
        .await;
    }
    let Some(send_ms) =
        crate::session_chat_send_requests::send_request_created_ms(&state.paths, &ledger_key)
    else {
        return response;
    };
    let mut fields = Map::new();
    fields.insert(REOPEN_CHECKED.to_string(), json!(true));
    if let Some(plan) = plan_reopen(state, caller, &target, send_ms).await {
        if run_reopen(state, plan, sent_message(body, send_ms)).await {
            fields.insert("reopenedThread".to_string(), json!(true));
            fields.insert("heldReport".to_string(), Value::Null);
        }
    }
    // Stored with the send, so the next replay answers the same without checking again.
    let mut stored = result.unwrap_or_default();
    stored.remove("duplicate");
    stored.extend(fields.clone());
    crate::session_chat_send_requests::mark_sent(&state.paths, &ledger_key, &Value::Object(stored));
    edit_result(response, |result| result.extend(fields)).await
}

/// A message, not a raw key: `key` carries a keystroke with no text (send_http.rs).
fn is_message(body: &Value) -> bool {
    body.pointer("/params/key").is_none_or(Value::is_null)
}

/// What the delivery watch keeps: the message's text, its `sendRequestId` (what the sender's
/// confirmation names), and when it was sent.
fn sent_message(body: &Value, sent_ms: i64) -> Option<(String, Option<String>, i64)> {
    let send_id = body
        .get("params")
        .and_then(|params| params.get(SEND_REQUEST_ID_PARAM))
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((message_text(body), send_id, sent_ms))
}

fn target_key(body: &Value) -> Option<SessionKey> {
    let read = |field: &str| {
        body.pointer(&format!("/params/{field}"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    Some((read("projectId")?, read("sessionId")?))
}

fn message_text(body: &Value) -> String {
    body.pointer("/params/text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

async fn plan_reopen(
    state: &AppState,
    caller: &Principal,
    key: &SessionKey,
    send_ms: i64,
) -> Option<ReopenPlan> {
    let (state, caller, key) = (state.clone(), caller.clone(), key.clone());
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).ok()?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        prepare_reopen(&db, &repository, &key, send_ms, |thread| {
            caller_may_reopen(&caller, thread)
        })
        .ok()
        .flatten()
    })
    .await
    .ok()
    .flatten()
}

/// Reopens the planned thread and republishes it; true when it reopened.
async fn run_reopen(
    state: &AppState,
    plan: ReopenPlan,
    watch: Option<(String, Option<String>, i64)>,
) -> bool {
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return false;
        };
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let watch = watch
            .as_ref()
            .map(|(text, send_id, sent)| (text.as_str(), send_id.as_deref(), *sent));
        let reopened = apply_reopen(&db, &repository, &plan, watch).unwrap_or(false);
        if reopened {
            let _ = crate::server::schedule_presentation_session_delta(
                &state,
                &db,
                &repository,
                &plan.thread_key.0,
                &plan.thread_key.1,
            );
        }
        reopened
    })
    .await
    .unwrap_or(false)
}

/// The response with its body read, and the `result` object of a success.
async fn read_result(routed: RoutedResponse) -> (RoutedResponse, Option<Map<String, Value>>) {
    let RoutedResponse {
        endpoint_path,
        response,
    } = routed;
    let (parts, body) = response.into_parts();
    let bytes = to_bytes(body, RESPONSE_MAX_BYTES).await.unwrap_or_default();
    let result = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|envelope| envelope.get("result").and_then(Value::as_object).cloned());
    (
        RoutedResponse {
            endpoint_path,
            response: Response::from_parts(parts, Body::from(bytes)),
        },
        result,
    )
}

/// Changes the `result` object of a success response; anything else passes through.
async fn edit_result(
    routed: RoutedResponse,
    edit: impl FnOnce(&mut Map<String, Value>),
) -> RoutedResponse {
    let RoutedResponse {
        endpoint_path,
        response,
    } = routed;
    let (mut parts, body) = response.into_parts();
    let bytes = to_bytes(body, RESPONSE_MAX_BYTES).await.unwrap_or_default();
    let mut envelope: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let bytes = match envelope.get_mut("result").and_then(Value::as_object_mut) {
        Some(result) => {
            edit(result);
            parts.headers.remove(CONTENT_LENGTH);
            envelope.to_string().into_bytes()
        }
        None => bytes.to_vec(),
    };
    RoutedResponse {
        endpoint_path,
        response: Response::from_parts(parts, Body::from(bytes)),
    }
}
