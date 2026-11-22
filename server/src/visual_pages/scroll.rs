use std::sync::Arc;

use axum::{
    body::Body,
    http::{Method, StatusCode},
};
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

use super::{
    answers::reply,
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{is_page_id, read_record, store_scroll, StoreAnswersError},
};

pub(crate) const VISUAL_PAGE_SCROLL_SUFFIX: &str = "/scroll";
/// `{"y": <number>}` and nothing more.
const SCROLL_BODY_MAX_BYTES: usize = 1024;
/// Further down than any page goes; a larger number is refused.
const SCROLL_Y_MAX: f64 = 10_000_000.0;

/// Whether `GET` or `POST <path>` is a page's scroll route, `/visual/<id>/scroll`.
pub(crate) fn is_scroll_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_SCROLL_SUFFIX)
}

/// `GET /visual/<id>/scroll` answers `{"y": <pixels>}`, where the reader left the page (0 when
/// never scrolled); `POST` with `{"y": <pixels>}` keeps a new position. The page's own script
/// (apps/desktop/src/app/window/visual_page_modal/page_script.js) saves it as the reader scrolls
/// and goes back to it once the page has loaded.
///
/// CDXC:SessionChat 2026-10-23 DECISION:
/// User: scroll memory for the floating page window, "use js for this too better", so the page remembers its own position instead of the app reading it. A published page has an opaque origin and no storage of its own, so the position lives on the page's record, like its draft (drafts.rs): answered before the auth gate, reached only by that page through its content policy (serve.rs), and limited to one number. On the record it outlasts closing the page, Refresh and restarts.
pub(crate) async fn handle(
    state: Arc<AppState>,
    method: Method,
    path: String,
    body: Body,
) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_SCROLL_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
    else {
        return not_found();
    };
    let state_dir = state.paths.root_dir.clone();
    if method == Method::GET {
        let record = tokio::task::spawn_blocking(move || read_record(&state_dir, &id))
            .await
            .ok()
            .flatten();
        return match record {
            Some(record) => {
                let y = record.get("scrollY").and_then(Value::as_u64).unwrap_or(0);
                reply(StatusCode::OK, json!({"ok": true, "y": y}))
            }
            None => not_found(),
        };
    }
    let Ok(bytes) = axum::body::to_bytes(body, SCROLL_BODY_MAX_BYTES).await else {
        return reply(
            StatusCode::PAYLOAD_TOO_LARGE,
            json!({"ok": false, "error": "Send only {\"y\": <number>}."}),
        );
    };
    let y = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|body| body.get("y").and_then(Value::as_f64))
        .filter(|y| y.is_finite() && (0.0..=SCROLL_Y_MAX).contains(y));
    let Some(y) = y else {
        return reply(
            StatusCode::BAD_REQUEST,
            json!({"ok": false, "error": "Send {\"y\": <pixels from the top>} as JSON."}),
        );
    };
    let y = y.round() as u64;
    let stored = tokio::task::spawn_blocking(move || store_scroll(&state_dir, &id, y)).await;
    match stored {
        Ok(Ok(())) => reply(StatusCode::OK, json!({"ok": true})),
        Ok(Err(StoreAnswersError::NotFound)) => not_found(),
        Ok(Err(StoreAnswersError::Io(error))) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": format!("The position could not be saved: {error}")}),
        ),
        Ok(Err(StoreAnswersError::AlreadyAnswered(_))) | Err(_) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": "The position could not be saved."}),
        ),
    }
}

fn not_found() -> RoutedResponse {
    reply(
        StatusCode::NOT_FOUND,
        json!({"ok": false, "error": "No such page."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_page_scroll_path_is_the_route() {
        assert!(is_scroll_route(
            "/visual/0123456789abcdef0123456789abcdef/scroll"
        ));
        assert!(!is_scroll_route(
            "/visual/0123456789abcdef0123456789abcdef/draft"
        ));
        assert!(!is_scroll_route("/api/scroll"));
    }

    #[test]
    fn the_position_is_kept_on_the_page_record() {
        let dir = tempfile::tempdir().unwrap();
        let id =
            super::super::store::write_page(dir.path(), "Page", "<p>page</p>", None, None).unwrap();
        assert!(read_record(dir.path(), &id)
            .unwrap()
            .get("scrollY")
            .is_none());
        assert!(store_scroll(dir.path(), &id, 1600).is_ok());
        assert_eq!(read_record(dir.path(), &id).unwrap()["scrollY"], 1600);
        assert!(matches!(
            store_scroll(dir.path(), "0123456789abcdef0123456789abcdef", 1),
            Err(StoreAnswersError::NotFound)
        ));
    }
}
