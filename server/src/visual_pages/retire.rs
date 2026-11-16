//! `ghostex page retire`: an agent takes back pages it published and replaced, so they stop
//! waiting on the reader. Each page is removed from its publishing session's open questions the
//! way the reader's own Dismiss or Mark all seen removes it (`questionRemove`,
//! server/src/session_chat_open_questions.rs `set_removed`): it leaves the dock, the ? pill, the
//! sidebar's dot and notifications on every client, and its questions stop waiting. The page
//! still opens; its record notes `retiredAt`.
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User picked "reply and build" on issue #626, whose reporter asked for "a CLI command for
//! agents ... so the agent can retire a page it replaced": agents get `ghostex page retire <page>`
//! for a page and its open questions, with its own --help, and the coordinator playbook and Ghostex
//! Help say where it is documented.

use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::read_domain_rpc_params,
    protocol::rpc_success,
    server::{routed_json, AppState, RoutedResponse},
};

use super::store::{is_page_id, note_retired, read_record};

pub(crate) const RETIRE_VISUAL_PAGES_ENDPOINT: &str = "/api/retireVisualPages";
/// How many pages one call retires at most.
const MAX_PAGES: usize = 200;

/// The page id in `text`: a bare id, or a page address (`…/visual/<id>`), any case.
pub(crate) fn page_id_of(text: &str) -> Option<String> {
    let text = text.trim();
    let path = text.split(['?', '#']).next().unwrap_or_default();
    let id = path
        .trim_end_matches('/')
        .rsplit_once("/visual/")
        .map_or(path, |(_, id)| id)
        .to_ascii_lowercase();
    is_page_id(&id).then_some(id)
}

/// `/api/retireVisualPages` `{pageIds: [...]}`: retires each page (ids or addresses) and answers
/// `{retired: [{id, title}], notFound: [...], noSession: [...]}`. A page published from outside
/// a session (no `sessionRef`) waits on nobody, so there is nothing to retire.
pub(crate) async fn handle_retire_visual_pages_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let given: Vec<String> = read_domain_rpc_params(body)
        .ok()
        .and_then(|params| params.get("pageIds").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .take(MAX_PAGES)
        .collect();
    let worker = state.clone();
    let result = tokio::task::spawn_blocking(move || retire(&worker, &given))
        .await
        .unwrap_or_else(|_| json!({"retired": [], "notFound": [], "noSession": []}));
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

fn retire(state: &AppState, given: &[String]) -> Value {
    let dir = &state.paths.root_dir;
    let (mut retired, mut not_found, mut no_session) = (Vec::new(), Vec::new(), Vec::new());
    for text in given {
        let Some((id, record)) =
            page_id_of(text).and_then(|id| read_record(dir, &id).map(|record| (id, record)))
        else {
            not_found.push(json!(text));
            continue;
        };
        let session = record
            .get("sessionRef")
            .and_then(Value::as_str)
            .map(crate::agent_hooks::probing::parse_global_session_ref);
        let Some((Some(project_id), Some(session_id))) = session else {
            no_session.push(json!(id));
            continue;
        };
        if let Err(error) =
            crate::session_chat_open_questions::set_removed(state, &project_id, &session_id, &id)
        {
            no_session.push(json!(id));
            let _ = state.logger.log(crate::logging::GxserverLogInput {
                level: crate::logging::LogLevel::Warn,
                event: "visualPages.retireFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.to_string()),
                details: Some(json!({ "pageId": id })),
            });
            continue;
        }
        note_retired(dir, &id);
        retired.push(json!({
            "id": id,
            "title": record.get("title").and_then(Value::as_str).unwrap_or("Ghostex page"),
        }));
    }
    json!({"retired": retired, "notFound": not_found, "noSession": no_session})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_is_named_by_its_id_or_its_address() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(page_id_of(id).as_deref(), Some(id));
        assert_eq!(
            page_id_of(&format!(
                "http://127.0.0.1:1/visual/{}/?x=1",
                id.to_uppercase()
            ))
            .as_deref(),
            Some(id)
        );
        assert!(page_id_of("Q12").is_none());
    }
}
