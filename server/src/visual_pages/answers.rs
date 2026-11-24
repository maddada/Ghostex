use std::sync::Arc;

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderValue, Response, StatusCode},
};
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

use super::{
    questions::page_questions,
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{note_delivery, store_answers, store_outside_answers, StoreAnswersError},
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
        .unwrap_or("Live page")
        .to_string();
    let message = tagged_answers_message(&state_dir, &id, &title, &answers);
    let (error, wait_for_idle) = deliver(
        &state,
        project_id.clone(),
        session_id.clone(),
        message,
        format!("visual-page-{id}"),
    )
    .await;
    let state_dir = state.paths.root_dir.clone();
    let noted = error.clone();
    let refresh_state = state.clone();
    let _ = tokio::task::spawn_blocking(move || {
        note_delivery(&state_dir, &id, noted.as_deref());
        crate::session_chat_open_questions::refresh(&refresh_state, &project_id, &session_id, None);
    })
    .await;
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
        // The user's own answers on a page: they settle the asks they name.
        crate::session_chat_send::sender_mark::note_user_send(
            &state.paths,
            &project_id,
            &session_id,
            &message,
        );
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
/// The envelope is parsed by packages/gx-chat-core/src/transcript/page_answers.rs (the answers card and the page card's Answered state) and described to agents in server/src/ghostex_cli/visual_guide.md. Change them together.
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

/// Maximum sanitized length of an outside visitor's display name.
const OUTSIDE_NAME_MAX_CHARS: usize = 60;
/// Maximum length of a single string answer value from an outside visitor.
const OUTSIDE_VALUE_MAX_CHARS: usize = 8 * 1024;
/// Maximum number of entries in an outside answers payload.
const OUTSIDE_MAX_FIELDS: usize = 80;
/// Maximum items in a multi-select answer array from an outside visitor.
const OUTSIDE_MAX_ARRAY_ITEMS: usize = 30;

/// Sanitizes an outside visitor's name: plain text, single-line, no angle brackets or quotes,
/// collapsed whitespace, capped at 60 chars. Returns `None` if empty after sanitization.
pub(crate) fn sanitize_outside_name(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|ch| !ch.is_control() && !matches!(ch, '<' | '>' | '"' | '\'' | '`'))
        .collect();
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let clipped: String = collapsed.chars().take(OUTSIDE_NAME_MAX_CHARS).collect();
    let trimmed = clipped.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Validates that `answers` is a flat JSON object whose keys correspond strictly to `questions`
/// (question keys, `<key> (note)`, `<key> · <choice> (note)`, or `Additional notes`) and whose
/// values are bounded scalar or string-array answers.
fn validate_outside_answers(
    answers: &Value,
    questions: &[super::questions::PageQuestion],
) -> Result<(), &'static str> {
    let Some(map) = answers.as_object() else {
        return Err("Answers must be a JSON object.");
    };
    if map.is_empty() {
        return Err("No answers were provided.");
    }
    if map.len() > OUTSIDE_MAX_FIELDS {
        return Err("Too many answer fields.");
    }
    let note_suffix = ghostex_gx_visual::questions::NOTE_SUFFIX;
    let notes_key = ghostex_gx_visual::questions::NOTES_KEY;
    for (key, value) in map {
        let allowed = questions.iter().any(|q| {
            key == &q.key
                || key == &format!("{}{note_suffix}", q.key)
                || (key.starts_with(&format!("{} \u{b7} ", q.key)) && key.ends_with(note_suffix))
        }) || key == notes_key
            || (key.starts_with(&format!("{notes_key} (")) && key.ends_with(')'));
        if !allowed {
            return Err("Answers include a field that does not belong to this page's questions.");
        }
        match value {
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
            Value::String(text) => {
                if text.chars().count() > OUTSIDE_VALUE_MAX_CHARS {
                    return Err("An answer is too long.");
                }
            }
            Value::Array(items) => {
                if items.len() > OUTSIDE_MAX_ARRAY_ITEMS {
                    return Err("Too many selections in an answer.");
                }
                for item in items {
                    let Some(text) = item.as_str() else {
                        return Err("Multi-select answers must be plain text.");
                    };
                    if text.chars().count() > 1024 {
                        return Err("A selected answer is too long.");
                    }
                }
            }
            Value::Object(_) => {
                return Err("Nested objects are not allowed in answers.");
            }
        }
    }
    Ok(())
}

/// Submits answers from an outside visitor who opened a shared Live page via its isolated
/// share-proxy (`POST /__gx_share/answers`). Validates name and question keys strictly, saves the
/// outside submission without locking out the user, delivers a clearly labeled
/// `<ghostex-page-answers>` message to the publishing session, and refreshes open questions so
/// the user's Inbox pings with `"Answered by <name>"`.
///
/// CDXC:Sharing 2026-11-24 DECISION:
/// Outside answers go to the posting session as `<ghostex-page-answers>` tagged with `by` and
/// `source="shared-link"` (explicitly marked as outside input, not user instructions), without
/// marking the user's own send or blocking the user from answering/overriding.
pub(crate) async fn submit_outside_answers(
    state: Arc<AppState>,
    id: &str,
    raw_name: &str,
    answers: Value,
) -> Result<Value, (StatusCode, String)> {
    let Some(name) = sanitize_outside_name(raw_name) else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Please enter your name before sending your answers.".to_string(),
        ));
    };
    let state_dir = state.paths.root_dir.clone();
    let id_owned = id.to_string();
    let questions_lookup =
        tokio::task::spawn_blocking(move || page_questions(&state_dir, &id_owned))
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Could not read the page's questions.".to_string(),
                )
            })?;
    let Some((title, questions)) = questions_lookup else {
        return Err((StatusCode::NOT_FOUND, "No such page.".to_string()));
    };
    if questions.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "This shared page has no open questions.".to_string(),
        ));
    }
    if let Err(reason) = validate_outside_answers(&answers, &questions) {
        return Err((StatusCode::BAD_REQUEST, reason.to_string()));
    }
    let state_dir = state.paths.root_dir.clone();
    let id_owned = id.to_string();
    let name_owned = name.clone();
    let answers_owned = answers.clone();
    let stored = tokio::task::spawn_blocking(move || {
        store_outside_answers(&state_dir, &id_owned, &name_owned, &answers_owned)
    })
    .await
    .map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "The answers could not be saved.".to_string(),
        )
    })?;
    let record = match stored {
        Ok(record) => record,
        Err(StoreAnswersError::NotFound) => {
            return Err((StatusCode::NOT_FOUND, "No such page.".to_string()));
        }
        Err(StoreAnswersError::AlreadyAnswered(_)) => {
            return Err((
                StatusCode::CONFLICT,
                "These answers were already sent.".to_string(),
            ));
        }
        Err(StoreAnswersError::Io(error)) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("The answers could not be saved: {error}"),
            ));
        }
    };
    let session = record
        .get("sessionRef")
        .and_then(Value::as_str)
        .map(crate::agent_hooks::probing::parse_global_session_ref);
    let Some((Some(project_id), Some(session_id))) = session else {
        return Err((
            StatusCode::BAD_REQUEST,
            "This page is not linked to an active session.".to_string(),
        ));
    };
    let keys: Vec<(String, String)> = questions
        .into_iter()
        .map(|question| (question.key, question.tag))
        .collect();
    let message = ghostex_gx_visual::questions::page_answers_message_outside(
        id, &title, &answers, &keys, &name,
    );
    let send_id = format!("visual-page-outside-{id}-{}", uuid::Uuid::new_v4());
    let (error, wait_for_idle) = deliver_outside(
        &state,
        project_id.clone(),
        session_id.clone(),
        message,
        send_id,
    )
    .await;
    let refresh_state = state.clone();
    let refresh_project = project_id.clone();
    let refresh_session = session_id.clone();
    let _ = tokio::task::spawn_blocking(move || {
        crate::session_chat_open_questions::refresh(
            &refresh_state,
            &refresh_project,
            &refresh_session,
            None,
        );
    })
    .await;
    let answered_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    match error {
        None => Ok(json!({
            "ok": true,
            "by": name,
            "answeredAt": answered_at,
            "queued": wait_for_idle,
            "delivered": true,
        })),
        Some(error) => Ok(json!({
            "ok": true,
            "by": name,
            "answeredAt": answered_at,
            "queued": false,
            "delivered": false,
            "error": format!("Saved, but the agent was not told: {error}"),
        })),
    }
}

/// Delivers an outside visitor's `<ghostex-page-answers>` message to the publishing session
/// without calling `note_user_send`, so gxserver never treats it as the user's own send.
async fn deliver_outside(
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
