use std::sync::Arc;

use axum::{body::Body, http::StatusCode};
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

use super::{
    answers::{reply, ANSWERS_BODY_MAX_BYTES},
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{is_page_id, read_record, store_draft, StoreAnswersError},
};

pub(crate) const VISUAL_PAGE_DRAFT_SUFFIX: &str = "/draft";

/// Whether `GET` or `POST <path>` is a page's draft route, `/visual/<id>/draft`.
pub(crate) fn is_draft_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_DRAFT_SUFFIX)
}

/// `POST /visual/<id>/draft` `{"draft": <any JSON>}`: what the reader has picked and written so far,
/// saved by the page as it changes and handed back when the page opens again (serve.rs).
///
/// CDXC:SessionChat 2026-10-11 DECISION:
/// User: never lose what the reader picked or wrote in a question page before sending; closing the floating window and opening it again, a reload or a restart brings it all back, without the agent writing anything for it.
/// CDXC:SessionChat 2026-10-11 WHY:
/// A published page has an opaque origin, so it has no storage of its own; the draft lives on the page's record in gxserver, answered before the auth gate like the answers route and limited to that page by its id and content policy.
pub(crate) async fn save(state: Arc<AppState>, path: String, body: Body) -> RoutedResponse {
    let Some(id) = page_of(&path) else {
        return no_such_page();
    };
    let Ok(bytes) = axum::body::to_bytes(body, ANSWERS_BODY_MAX_BYTES).await else {
        return reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"ok": false, "error": "The draft is larger than 64 KiB."}),
        );
    };
    let (draft, edited_at) = match serde_json::from_slice::<Value>(&bytes) {
        Ok(Value::Object(mut body)) if body.contains_key("draft") => (
            body.remove("draft").unwrap_or(Value::Null),
            body.get("editedAt").and_then(Value::as_u64),
        ),
        _ => {
            return reply(
                StatusCode::BAD_REQUEST,
                json!({"ok": false, "error": "Send {\"draft\": …} as JSON."}),
            )
        }
    };
    let state_dir = state.paths.root_dir.clone();
    let stored =
        tokio::task::spawn_blocking(move || store_draft(&state_dir, &id, &draft, edited_at)).await;
    match stored {
        // `kept: false`: gxserver already has a draft from a later edit, which stays.
        Ok(Ok(kept)) => reply(StatusCode::OK, json!({"ok": true, "kept": kept})),
        Ok(Err(StoreAnswersError::NotFound)) => no_such_page(),
        Ok(Err(StoreAnswersError::AlreadyAnswered(answered_at))) => reply(
            StatusCode::CONFLICT,
            json!({"ok": false, "error": "These answers were already sent.", "answeredAt": answered_at}),
        ),
        Ok(Err(StoreAnswersError::Io(error))) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": format!("The draft could not be saved: {error}")}),
        ),
        Err(_) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": "The draft could not be saved."}),
        ),
    }
}

/// `GET /visual/<id>/draft`: the draft gxserver has now and when it was edited
/// (`{"ok": true, "draft": …, "editedAt": <ms> | null}`; no draft once the page was answered).
/// A page reads it a moment after it loads and takes it when it is newer than the one it was
/// served: on a reload the new document is fetched before the old one's last save (its `pagehide`
/// beacon) arrives, so the page it was served can be a moment old.
pub(crate) async fn read(state: Arc<AppState>, path: String) -> RoutedResponse {
    let Some(id) = page_of(&path) else {
        return no_such_page();
    };
    let state_dir = state.paths.root_dir.clone();
    let record = tokio::task::spawn_blocking(move || read_record(&state_dir, &id))
        .await
        .ok()
        .flatten();
    let Some(record) = record else {
        return no_such_page();
    };
    if record.get("answeredAt").is_some() {
        return reply(
            StatusCode::OK,
            json!({"ok": true, "answered": true, "draft": null, "editedAt": null}),
        );
    }
    reply(
        StatusCode::OK,
        json!({
            "ok": true,
            "draft": record.get("draft").cloned().unwrap_or(Value::Null),
            "editedAt": record.get("draftEditedAt").cloned().unwrap_or(Value::Null),
        }),
    )
}

fn page_of(path: &str) -> Option<String> {
    path.strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_DRAFT_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
}

fn no_such_page() -> RoutedResponse {
    reply(
        StatusCode::NOT_FOUND,
        json!({"ok": false, "error": "No such page."}),
    )
}
