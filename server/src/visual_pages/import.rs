//! Answers to an exported page (exported.rs) brought back into Ghostex: the reader dropped or
//! pasted the JSON the page's Export answers gave them into a chat, and the chat hands it here.
//!
//! The JSON is `{"ghostexPageAnswers": 1, "page": <id>, "title", "exportedAt", "answers": {…},
//! "questions": [{"tag", "question", "key", "value", "label", "note"}…]}`; `answers` holds the same
//! keys the page's own Send posts (kit.rs), so the agent gets the same `<ghostex-page-answers>`
//! message. A page this computer published goes to the session that published it and is marked
//! answered (the first answers win, as with Send; later imports still reach the agent, since several
//! people may answer one exported page). A page this computer does not know goes to the chat it was
//! imported into, under the title the JSON carries.
//!
//! CDXC:SessionChat 2026-11-13 SEE-ALSO:
//! The user's decision is on exported.rs. The chat finds the JSON in a send or a dropped file (packages/gx-chat-core/src/questions/page_import.rs).

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    answers::{answers_message, deliver, ANSWERS_BODY_MAX_BYTES},
    store::{
        clip_chars, is_page_id, note_delivery, read_record, store_answers, StoreAnswersError,
        VISUAL_PAGE_TITLE_MAX_CHARS,
    },
};

pub(crate) const IMPORT_VISUAL_PAGE_ANSWERS_ENDPOINT: &str = "/api/importVisualPageAnswers";

/// What an exported page's JSON says.
struct Imported {
    page: Option<String>,
    title: String,
    answers: Value,
}

/// The exported answers in `value` (the JSON text or the object), or why it is not one.
fn read_exported(value: &Value) -> Result<Imported, &'static str> {
    let parsed;
    let object = match value {
        Value::String(text) => {
            if text.len() > ANSWERS_BODY_MAX_BYTES * 2 {
                return Err("The answers are too large.");
            }
            parsed = serde_json::from_str::<Value>(text.trim())
                .map_err(|_| "This is not the answers of an exported Ghostex page.")?;
            &parsed
        }
        other => other,
    };
    let object = object
        .as_object()
        .filter(|object| object.contains_key("ghostexPageAnswers"))
        .ok_or("This is not the answers of an exported Ghostex page.")?;
    let answers = object
        .get("answers")
        .filter(|answers| answers.is_object())
        .cloned()
        .ok_or("The file has no answers in it.")?;
    if answers.to_string().len() > ANSWERS_BODY_MAX_BYTES {
        return Err("The answers are larger than 64 KiB.");
    }
    let page = object
        .get("page")
        .and_then(Value::as_str)
        .map(|page| page.trim().to_ascii_lowercase())
        .filter(|page| is_page_id(page));
    let title = object
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(|title| clip_chars(title, VISUAL_PAGE_TITLE_MAX_CHARS))
        .unwrap_or_else(|| "Exported Ghostex page".to_string());
    Ok(Imported {
        page,
        title,
        answers,
    })
}

/// `/api/importVisualPageAnswers` `{projectId, sessionId, exported}`: `exported` is the JSON an
/// exported page gave (its text or the object). Answers `{pageId, title, known, projectId,
/// sessionId, delivered, queued, alreadyAnswered, error?}`: `known` says this computer published the
/// page, and `projectId`/`sessionId` name the chat the answers went to.
pub(crate) async fn handle_import_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let imported = match params.get("exported").map(read_exported) {
        Some(Ok(imported)) => imported,
        Some(Err(message)) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError::bad_request(message),
            )
        }
        None => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError::bad_request("This call requires exported."),
            )
        }
    };
    let text = |name: &str| {
        params
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let state_dir = state.paths.root_dir.clone();
    let record = imported
        .page
        .as_deref()
        .and_then(|id| read_record(&state_dir, id));
    let known = record.is_some();
    // A known page answers to the session that published it, like its own Send.
    let published_by = record
        .as_ref()
        .and_then(|record| record.get("sessionRef"))
        .and_then(Value::as_str)
        .map(crate::agent_hooks::probing::parse_global_session_ref);
    let (project_id, session_id) = match published_by {
        Some((Some(project), Some(session))) => (Some(project), Some(session)),
        _ => (text("projectId"), text("sessionId")),
    };
    let (Some(project_id), Some(session_id)) = (project_id, session_id) else {
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError::bad_request("This call requires projectId and sessionId."),
        );
    };
    let title = record
        .as_ref()
        .and_then(|record| record.get("title"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(imported.title);
    let page_id = imported.page.unwrap_or_else(|| "exported".to_string());
    let mut already_answered = false;
    if known {
        let stored = {
            let state_dir = state_dir.clone();
            let id = page_id.clone();
            let answers = imported.answers.clone();
            tokio::task::spawn_blocking(move || store_answers(&state_dir, &id, &answers)).await
        };
        already_answered = matches!(stored, Ok(Err(StoreAnswersError::AlreadyAnswered(_))));
    }
    let message = answers_message(&page_id, &title, &imported.answers);
    let send_request_id = format!(
        "visual-page-import-{page_id}-{}",
        chrono::Utc::now().timestamp_millis()
    );
    let (error, queued) = deliver(
        state,
        project_id.clone(),
        session_id.clone(),
        message,
        send_request_id,
    )
    .await;
    if known && !already_answered {
        let noted = error.clone();
        let id = page_id.clone();
        let _ =
            tokio::task::spawn_blocking(move || note_delivery(&state_dir, &id, noted.as_deref()))
                .await;
    }
    let mut result = json!({
        "pageId": page_id,
        "title": title,
        "known": known,
        "projectId": project_id,
        "sessionId": session_id,
        "delivered": error.is_none(),
        "queued": queued && error.is_none(),
        "alreadyAnswered": already_answered,
    });
    if let Some(error) = error {
        result["error"] = json!(format!("The agent was not told: {error}"));
    }
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}
