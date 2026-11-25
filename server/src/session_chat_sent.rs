//! `/api/readSessionChatSent`: the messages the user sent in a chat, for the History pill above
//! the chat box (gx-chat-core `composer/sent_history.rs`, where the user's decision lives).
//!
//! The read walks the session's transcript from the newest page back, as the chat reads it (fork
//! lineages included, `read_session_chat_tail_page_stitched`), numbers the rows the way every
//! chat read does (`chat_message_numbers::annotate_messages`), keeps the user's own messages that
//! pass the search and the filter (`sent_history::card`, `sent_history::matches`) and stops once it
//! holds a page of them. `before` resumes from where the last answer stopped, so the list pages
//! (20 cards first, then 40, `FIRST_PAGE_CARDS` and `PAGE_CARDS`) however long the chat is, and a
//! message sent before message numbers existed is listed too, without a number.
//!
//! CDXC:SessionChat 2026-11-26 WHY:
//! Remote-allowed like readSessionChat (protocol.rs): the transcript lives on the machine that runs
//! the session, and the phone and other computers ask that machine. It reads only the session's own
//! transcript and answers its user rows' words, pictures' paths and mentioned files, which a chat
//! read already sends the same callers.

use ghostex_gx_chat_core::composer::sent_history::{
    card, matches, SentCard, SentFilter, PAGE_CARDS,
};
use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

/// Transcript rows one page read takes.
const PAGE_ROWS: usize = 200;
/// Transcript pages one answer reads at most, so a search through a very long chat answers in
/// steps (the list asks for the next as it scrolls) rather than reading the whole file at once.
const MAX_PAGES: usize = 40;

fn param<'a>(body: &'a Value, key: &str) -> Option<&'a Value> {
    body.get("params")
        .and_then(|params| params.get(key))
        .or_else(|| body.get(key))
}

fn text(body: &Value, key: &str) -> String {
    param(body, key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// The cards the read found, oldest first, where the next older read starts, and whether the
/// transcript holds more before it.
struct Sent {
    cards: Vec<SentCard>,
    before: Option<u64>,
    has_more: bool,
}

fn read_sent(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    mut before: Option<u64>,
    query: &str,
    filter: SentFilter,
    limit: usize,
) -> Sent {
    let mut found: Vec<SentCard> = Vec::new();
    let Some((agent, path)) =
        crate::session_chat_read::session_transcript(state, project_id, session_id)
    else {
        return Sent {
            cards: found,
            before: None,
            has_more: false,
        };
    };
    let mut has_more = false;
    for _ in 0..MAX_PAGES {
        let Ok(page) = crate::session_chat_fork_stitch::read_session_chat_tail_page_stitched(
            agent, &path, PAGE_ROWS, before,
        ) else {
            break;
        };
        let crate::session_chat::SessionChatTailPage::Page {
            messages,
            has_more: more,
            before_offset,
            ..
        } = page.page
        else {
            break;
        };
        let Ok(Value::Array(mut rows)) = serde_json::to_value(&messages) else {
            break;
        };
        crate::chat_message_numbers::annotate_messages(&mut rows, project_id, session_id);
        let mut page_cards: Vec<SentCard> = rows
            .into_iter()
            .filter_map(|row| serde_json::from_value::<ChatMessage>(row).ok())
            .filter_map(|message| card(&message))
            .filter(|card| matches(card, query, filter))
            .collect();
        page_cards.append(&mut found);
        found = page_cards;
        has_more = more;
        before = Some(before_offset);
        if !more || found.len() >= limit {
            break;
        }
    }
    Sent {
        cards: found,
        before: before.filter(|_| has_more),
        has_more,
    }
}

pub(crate) async fn handle_read_session_chat_sent_http(
    state: std::sync::Arc<AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> RoutedResponse {
    let (project_id, session_id) = (text(&body, "projectId"), text(&body, "sessionId"));
    if project_id.is_empty() || session_id.is_empty() {
        return crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "invalidParams",
                message: "readSessionChatSent requires projectId and sessionId.".to_string(),
            },
        );
    }
    let before = param(&body, "before").and_then(Value::as_u64);
    let query = text(&body, "query");
    let filter = SentFilter::parse(&text(&body, "filter"));
    let limit = param(&body, "limit")
        .and_then(Value::as_u64)
        .map_or(PAGE_CARDS, |limit| (limit as usize).clamp(1, 200));
    let read = tokio::task::spawn_blocking(move || {
        read_sent(
            &state,
            &project_id,
            &session_id,
            before,
            &query,
            filter,
            limit,
        )
    })
    .await;
    match read {
        Ok(sent) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(
                request_id,
                json!({
                    "messages": sent.cards,
                    "before": sent.before,
                    "hasMore": sent.has_more,
                }),
            ),
        ),
        Err(_) => crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "internalError",
                message: "Could not read the messages sent in this chat.".to_string(),
            },
        ),
    }
}
