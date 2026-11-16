use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderValue, Response, StatusCode},
};
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

use super::{
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{note_delivery, store_answers, StoreAnswersError},
};

pub(crate) const VISUAL_PAGE_ANSWERS_SUFFIX: &str = "/answers";
/// The page's `{answers}` body; answers are form fields, not files.
pub(super) const ANSWERS_BODY_MAX_BYTES: usize = 64 * 1024;

/// Whether `POST <path>` is a page's answers route, `/visual/<id>/answers`.
pub(crate) fn is_answers_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_ANSWERS_SUFFIX)
}

/// `POST /visual/<id>/answers` `{"answers": <any JSON>}`: the page's `ghostex.submit(answers)`.
/// Stores the answers with the page and sends them to the session that published it.
///
/// CDXC:SessionChat 2026-11-01 DECISION: User: "No we shouldn't send right away, we should just send normally, it'll send the message after tool call finishes". This supersedes the 2026-10-11 queue-until-idle rule; the Advanced toggle keeps it available. The page still stores one answer and sends its publishing session a `<ghostex-page-answers>` message, which chat draws as the answers card.
/// CDXC:ServerApi 2026-10-11 WHY:
/// Answered before the auth gate like `GET /visual/<id>`: the page has no token, and its unguessable id is the grant. The page has an opaque origin, so it posts plain text with `Origin: null` (no preflight) and reads the reply through `Access-Control-Allow-Origin: *`; its content policy lets it reach this one URL and nothing else on gxserver. The first submission wins, so a page cannot type a stream of messages into the agent.
pub(crate) async fn submit(
    state: Arc<AppState>,
    path: String,
    body: Body,
) -> crate::server::RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_ANSWERS_SUFFIX))
        .map(str::to_string)
    else {
        return reply(
            StatusCode::NOT_FOUND,
            json!({"ok": false, "error": "No such page."}),
        );
    };
    let Ok(bytes) = axum::body::to_bytes(body, ANSWERS_BODY_MAX_BYTES).await else {
        return reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"ok": false, "error": "The answers are larger than 64 KiB."}),
        );
    };
    let answers = match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(mut body)) if body.contains_key("answers") => {
            body.remove("answers").unwrap_or(Value::Null)
        }
        _ => {
            return reply(
                StatusCode::BAD_REQUEST,
                json!({"ok": false, "error": "Send {\"answers\": …} as JSON."}),
            )
        }
    };
    let state_dir = state.paths.root_dir.clone();
    let stored = {
        let state_dir = state_dir.clone();
        let id = id.clone();
        let answers = answers.clone();
        tokio::task::spawn_blocking(move || store_answers(&state_dir, &id, &answers)).await
    };
    let record = match stored {
        Ok(Ok(record)) => record,
        Ok(Err(StoreAnswersError::NotFound)) => {
            return reply(
                StatusCode::NOT_FOUND,
                json!({"ok": false, "error": "No such page."}),
            )
        }
        Ok(Err(StoreAnswersError::AlreadyAnswered(answered_at))) => {
            return reply(
                StatusCode::CONFLICT,
                json!({"ok": false, "error": "These answers were already sent.", "answeredAt": answered_at}),
            )
        }
        Ok(Err(StoreAnswersError::Io(error))) => {
            return reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"ok": false, "error": format!("The answers could not be saved: {error}")}),
            )
        }
        Err(_) => {
            return reply(
                StatusCode::INTERNAL_SERVER_ERROR,
                json!({"ok": false, "error": "The answers could not be saved."}),
            )
        }
    };
    let answered_at = record["answeredAt"].clone();
    let session = record
        .get("sessionRef")
        .and_then(Value::as_str)
        .map(crate::agent_hooks::probing::parse_global_session_ref);
    let Some((Some(project_id), Some(session_id))) = session else {
        note_delivery(&state_dir, &id, Some("The page has no publishing session."));
        return reply(
            StatusCode::OK,
            json!({"ok": true, "answeredAt": answered_at, "delivered": false,
                   "error": "Saved, but the session that published this page is unknown, so no agent was told."}),
        );
    };
    let title = record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("Ghostex page")
        .to_string();
    let message = tagged_answers_message(&state_dir, &id, &title, &answers);
    let (error, wait_for_idle) = deliver(
        &state,
        project_id,
        session_id,
        message,
        format!("visual-page-{id}"),
    )
    .await;
    let state_dir = state.paths.root_dir.clone();
    let noted = error.clone();
    let _ =
        tokio::task::spawn_blocking(move || note_delivery(&state_dir, &id, noted.as_deref())).await;
    match error {
        None => reply(
            StatusCode::OK,
            json!({"ok": true, "answeredAt": answered_at, "queued": wait_for_idle, "delivered": true}),
        ),
        Some(error) => reply(
            StatusCode::OK,
            json!({"ok": true, "answeredAt": answered_at, "queued": false,
                   "error": format!("Saved, but the agent was not told: {error}")}),
        ),
    }
}

/// Sends a `<ghostex-page-answers>` message to the session, the way the page's own Send does:
/// queued until the agent is idle when the Advanced setting asks for it, else sent normally.
/// Returns the error that kept it from the agent, if any, and whether it was queued.
pub(super) async fn deliver(
    state: &Arc<AppState>,
    project_id: String,
    session_id: String,
    message: String,
    send_request_id: String,
) -> (Option<String>, bool) {
    let wait_for_idle =
        crate::session_chat_catch_up::chat_setting_enabled(&state.paths, "chatAnswersWaitForIdle");
    let error = if wait_for_idle {
        let queue_state = state.clone();
        tokio::task::spawn_blocking(move || {
            let mut params = serde_json::Map::new();
            params.insert("projectId".to_string(), json!(project_id));
            params.insert("sessionId".to_string(), json!(session_id));
            params.insert("text".to_string(), json!(message));
            crate::session_chat_queue::handle_session_chat_queue_endpoint(
                &queue_state.paths,
                queue_state.metadata.server_id.as_str(),
                "/api/queueSessionChatPrompt",
                &params,
            )
            .map(|result| {
                if result.broadcast {
                    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                        &queue_state,
                        &project_id,
                        &session_id,
                    );
                }
            })
            .map_err(|error| error.message)
        })
        .await
        .unwrap_or_else(|_| Err("The answers could not be queued.".to_string()))
        .err()
    } else {
        let body = json!({"params": {
            "projectId": project_id,
            "sessionId": session_id,
            "text": message,
            "sendRequestId": send_request_id,
        }});
        let sent = crate::session_chat_send::handle_send_session_chat_message_http(
            state,
            "/api/sendSessionChatMessage".to_string(),
            send_request_id,
            &body,
        )
        .await;
        if sent.response.status().is_success() {
            None
        } else {
            let bytes = to_bytes(sent.response.into_body(), 64 * 1024)
                .await
                .unwrap_or_default();
            Some(String::from_utf8_lossy(&bytes).to_string())
        }
    };
    (error, wait_for_idle)
}

/// The message the publishing agent receives.
///
/// CDXC:SessionChat 2026-10-11 SEE-ALSO:
/// The envelope is parsed by packages/gx-chat-core/src/transcript/page_answers.rs (the answers card and the page card's Answered state) and described to agents in skills/ghostex-visuals/SKILL.md. Change them together.
pub(super) fn answers_message(id: &str, title: &str, answers: &Value) -> String {
    ghostex_gx_visual::questions::page_answers_message(id, title, answers)
}

/// [`answers_message`] with each question's tag, read from the page's `<gx-questions>` markup
/// (`questions.rs`), so the chat's answers card can show the tags (CDXC:SessionChat 2026-11-13
/// in packages/gx-visual/src/questions.rs `page_answers_message_tagged`).
pub(crate) fn tagged_answers_message(
    state_dir: &std::path::Path,
    id: &str,
    title: &str,
    answers: &Value,
) -> String {
    let keys: Vec<(String, String)> = super::questions::page_questions(state_dir, id)
        .map(|(_, questions)| {
            questions
                .into_iter()
                .map(|question| (question.key, question.tag))
                .collect()
        })
        .unwrap_or_default();
    ghostex_gx_visual::questions::page_answers_message_tagged(id, title, answers, &keys)
}

pub(super) fn reply(status: StatusCode, body: Value) -> RoutedResponse {
    let mut response = Response::new(Body::from(body.to_string()));
    *response.status_mut() = status;
    for (name, value) in [
        (header::CONTENT_TYPE, "application/json"),
        (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        (header::CACHE_CONTROL, "no-store"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    RoutedResponse {
        endpoint_path: None,
        response,
    }
}
