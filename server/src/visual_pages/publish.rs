use std::sync::Arc;

use axum::http::StatusCode;
use serde_json::{json, Value};

use crate::{
    domain::{read_domain_rpc_params, DomainStateError},
    protocol::rpc_success,
    server::{domain_error_response, routed_json, AppState, RoutedResponse},
};

use super::{
    serve::VISUAL_PAGE_ROUTE_PREFIX,
    store::{
        clip_chars, note_awaits_seen, write_page, VISUAL_PAGE_MAX_BYTES,
        VISUAL_PAGE_TITLE_MAX_CHARS,
    },
};

pub(crate) const PUBLISH_VISUAL_PAGE_ENDPOINT: &str = "/api/publishVisualPage";
/// The JSON body carries the page as an escaped string, which can grow it to twice its size.
pub(crate) const PUBLISH_VISUAL_PAGE_BODY_LIMIT_BYTES: usize =
    2 * VISUAL_PAGE_MAX_BYTES + 64 * 1024;

/// `/api/publishVisualPage` `{title, html, sessionRef?, sourcePath?, awaitsSeen?}`: stores a page
/// `ghostex show` prepared and answers `{id, title, path}`, where `path` is the `/visual/<id>`
/// route that serves it. `sourcePath` is the HTML file it was prepared from, kept for Refresh
/// (`source.rs`). `awaitsSeen` marks a page that asks nothing as waiting until the reader marks it
/// seen.
///
/// CDXC:SessionChat 2026-11-11 DECISION:
/// User, of the dock above the chat box that lists what needs him: "and if it's just a page that i can look at (doesn't have questions) then still show it but i need to scroll to bottom and reply with Mark as read and should have area to reply on the document of course". `ghostex show` sends `awaitsSeen: true` for a page with no `<gx-questions>` and no `ghostex.submit` of its own, and prints `"awaitsSeen": true` in its block. The kit then ends the page with a reply box and Mark as read (kit.rs `SEEN_FOOTER`), which posts `{"seen": true, "reply": …}` through the page's answers route, so gxserver stores it and tells the publishing agent as it does a page's answers. Until then the chat counts the page as waiting (packages/gx-chat-core/src/transcript/open_questions.rs) and the page window's hidden tab shows its dot (`page_questions_left`). Only pages published from now on carry the mark: a page published before it has no `awaitsSeen` on its record or its block, so it neither shows the footer nor waits in the dock. Older apps and phones read neither field and treat the page as before.
pub(crate) fn handle_publish_visual_page_http(
    state: &Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };
    let Some(html) = params
        .get("html")
        .and_then(Value::as_str)
        .filter(|html| !html.trim().is_empty())
    else {
        return bad_request(
            endpoint_path,
            request_id,
            "publishVisualPage requires html.",
        );
    };
    if html.len() > VISUAL_PAGE_MAX_BYTES {
        return bad_request(
            endpoint_path,
            request_id,
            "The page is larger than the 8 MiB limit.",
        );
    }
    let Some(title) = text("title").map(|title| clip_chars(title, VISUAL_PAGE_TITLE_MAX_CHARS))
    else {
        return bad_request(
            endpoint_path,
            request_id,
            "publishVisualPage requires title.",
        );
    };
    let source = text("sourcePath")
        .map(std::path::Path::new)
        .filter(|path| path.is_absolute() && path.is_file());
    let publisher = text("sessionRef")
        .map(crate::agent_hooks::probing::parse_global_session_ref)
        .and_then(|parts| match parts {
            (Some(project_id), Some(session_id)) => Some((project_id, session_id)),
            _ => None,
        });
    let tags = super::questions::page_tags(html);
    if let (Some(key), false) = (&publisher, tags.is_empty()) {
        if let Some(refusal) = taken_tag_refusal(state, key, &tags) {
            return bad_request(endpoint_path, request_id, &refusal);
        }
    }
    let id = match write_page(
        &state.paths.root_dir,
        &title,
        html,
        text("sessionRef"),
        source,
    ) {
        Ok(id) => {
            if params.get("awaitsSeen").and_then(Value::as_bool) == Some(true) {
                let _ = note_awaits_seen(&state.paths.root_dir, &id);
            }
            if let Some(source) = source {
                super::retire::retire_earlier_from_same_file(state, &id, &source.to_string_lossy());
            }
            // Its tags are taken from now on, before the chat even shows the page: an ask or
            // `next-tag` right after gives the tag after them.
            if let (Some(key), Some(highest)) = (&publisher, highest_tag(&tags)) {
                if let Ok(db) = crate::storage::open_gxserver_database(&state.paths) {
                    let _ = crate::session_needs_you::hold_tags(&db, key, highest);
                }
            }
            id
        }
        Err(error) => {
            return domain_error_response(
                endpoint_path,
                request_id,
                DomainStateError {
                    code: "internalError",
                    message: format!("Could not save the page: {error}"),
                },
            )
        }
    };
    routed_json(
        Some(endpoint_path),
        StatusCode::OK,
        rpc_success(
            request_id,
            json!({
                "id": id,
                "title": title,
                "path": format!("{VISUAL_PAGE_ROUTE_PREFIX}{id}"),
            }),
        ),
    )
}

/// The highest `Q` number among `tags`.
fn highest_tag(tags: &[String]) -> Option<u64> {
    tags.iter()
        .filter_map(|tag| tag.strip_prefix('Q')?.parse::<u64>().ok())
        .max()
}

/// Why the page may not be published into the chat of `key`, or `None`: one of its question tags
/// already belongs to another question there, a work item's ask with its own card or a Needs you
/// to-do. A tag means one question, and the page's answer would settle that other one.
///
/// CDXC:Coordinators 2026-11-25 WHY:
/// A thread's Live page tagged Q4846 was shown in the coordinator's chat while W1026's own ask
/// "Post this reply on issue #746?" was Q4846; the user's answer on the page ("Keep the dot")
/// settled the ask, which left Needs you unanswered. The page is refused with the next free tag.
/// SEE-ALSO: server/src/coordinators/work.rs (`own_ask_holding`, `WorkAsk::chat_question`),
/// server/src/session_chat_open_questions.rs (`read_question_tags`, `next-tag`).
fn taken_tag_refusal(state: &AppState, key: &(String, String), tags: &[String]) -> Option<String> {
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let todos = crate::session_needs_you::todo_tags(&db, key);
    let taken: Vec<String> = tags
        .iter()
        .filter_map(|tag| {
            crate::coordinators::own_ask_holding(&db, key, tag)
                .map(|holder| format!("{tag} is {holder}"))
                .or_else(|| {
                    todos
                        .iter()
                        .any(|todo| todo == tag)
                        .then(|| format!("{tag} is a Needs you to-do"))
                })
        })
        .collect();
    if taken.is_empty() {
        return None;
    }
    let asked: Vec<String> = crate::coordinators::list_work_items(&db, key)
        .unwrap_or_default()
        .iter()
        .flat_map(|item| item.asks.iter().map(|ask| ask.tag.to_ascii_uppercase()))
        .collect();
    let floor = crate::session_chat_open_questions::highest_question_tag(&key.0, &key.1)
        .max(crate::session_needs_you::highest_tag(&db, key))
        .max(highest_tag(&asked).unwrap_or(0))
        .max(highest_tag(tags).unwrap_or(0));
    Some(format!(
        "This page's question tag is already taken in that chat: {}. A tag means one question, and an answer on the page would settle that one. Give the page's question a new tag (Q{} is free; `ghostex coordinator next-tag` gives one) and publish it again.",
        taken.join("; "),
        floor + 1
    ))
}

fn bad_request(endpoint_path: String, request_id: String, message: &str) -> RoutedResponse {
    domain_error_response(
        endpoint_path,
        request_id,
        DomainStateError::bad_request(message),
    )
}
