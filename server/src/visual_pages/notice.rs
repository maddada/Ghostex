use std::sync::Arc;

use axum::{body::Body, http::StatusCode};
use serde_json::json;

use crate::server::{AppState, RoutedResponse};

use super::{
    answers::reply,
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{dismiss_layout_notice, is_page_id, StoreAnswersError},
};

pub(crate) const VISUAL_PAGE_NOTICE_SUFFIX: &str = "/notice";

/// Whether `POST <path>` is a page's notice route, `/visual/<id>/notice`.
pub(crate) fn is_notice_route(path: &str) -> bool {
    path.starts_with(VISUAL_PAGE_ROUTE_PREFIX) && path.ends_with(VISUAL_PAGE_NOTICE_SUFFIX)
}

/// `POST /visual/<id>/notice`: the reader closed the notice a v1 page shows above itself (the page
/// API in theme.rs), so serve.rs stops asking for it on this page.
///
/// CDXC:SessionChat 2026-10-27 WHY:
/// A published page has an opaque origin and no storage of its own, so "not again for the same page once dismissed" lives on the page's record, like its scroll position (scroll.rs). It is kept for answered pages too, which refuse drafts. The body is ignored.
pub(crate) async fn dismiss(state: Arc<AppState>, path: String, _body: Body) -> RoutedResponse {
    let Some(id) = path
        .strip_prefix(VISUAL_PAGE_ROUTE_PREFIX)
        .and_then(|rest| rest.strip_suffix(VISUAL_PAGE_NOTICE_SUFFIX))
        .filter(|id| is_page_id(id))
        .map(str::to_string)
    else {
        return not_found();
    };
    let state_dir = state.paths.root_dir.clone();
    let stored = tokio::task::spawn_blocking(move || dismiss_layout_notice(&state_dir, &id)).await;
    match stored {
        Ok(Ok(())) => reply(StatusCode::OK, json!({"ok": true})),
        Ok(Err(StoreAnswersError::NotFound)) => not_found(),
        Ok(Err(StoreAnswersError::Io(error))) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": format!("The notice could not be closed: {error}")}),
        ),
        Ok(Err(StoreAnswersError::AlreadyAnswered(_))) | Err(_) => reply(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"ok": false, "error": "The notice could not be closed."}),
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
    use super::super::store::{read_record, write_page};
    use super::*;

    #[test]
    fn only_a_page_notice_path_is_the_route() {
        assert!(is_notice_route(
            "/visual/0123456789abcdef0123456789abcdef/notice"
        ));
        assert!(!is_notice_route(
            "/visual/0123456789abcdef0123456789abcdef/scroll"
        ));
        assert!(!is_notice_route("/api/notice"));
    }

    #[test]
    fn closing_the_notice_is_kept_on_the_page_record_answered_or_not() {
        let dir = tempfile::tempdir().unwrap();
        let id = write_page(dir.path(), "Page", "<p>page</p>", None, None).unwrap();
        assert!(read_record(dir.path(), &id)
            .unwrap()
            .get("layoutNoticeDismissedAt")
            .is_none());
        assert!(super::super::store::store_answers(dir.path(), &id, &json!({"Q1": "a"})).is_ok());
        assert!(dismiss_layout_notice(dir.path(), &id).is_ok());
        assert!(read_record(dir.path(), &id).unwrap()["layoutNoticeDismissedAt"].is_string());
        assert!(matches!(
            dismiss_layout_notice(dir.path(), "0123456789abcdef0123456789abcdef"),
            Err(StoreAnswersError::NotFound)
        ));
    }
}
