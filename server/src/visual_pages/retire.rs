//! `ghostex page retire`: an agent takes back pages it published and replaced, so they stop
//! waiting on the reader. Each page is removed from the open questions of every chat that shows
//! it the way the reader's own Dismiss or Mark all read removes it (`questionRemove`,
//! server/src/session_chat_open_questions.rs `set_removed`): it leaves the dock, the ? pill, the
//! sidebar's dot, the board's Needs you and notifications on every client, and its questions stop
//! waiting. The page still opens; its record notes `retiredAt`.
//!
//! CDXC:Coordinators 2026-11-20 WHY:
//! A page's card shows in whichever chat prints its block, not only in the session that ran
//! `ghostex show` (the release smoke test shows a page from its own session and has another chat
//! echo it; a coordinator shows a page a thread wrote). Retiring only the publishing session left
//! the page waiting in the chat that showed it, in its Needs you and the waiting state
//! `clear-command` checks, for three releases running. So a retire reaches every chat whose
//! resource index or open questions hold the page, and a chat that shows a retired page later (or
//! before its index listed it) lets it go on its next refresh (`take_back_retired_pages`).
//!
//! CDXC:Coordinators 2026-11-16 DECISION:
//! User picked "reply and build" on issue #626, whose reporter asked for "a CLI command for
//! agents ... so the agent can retire a page it replaced": agents get `ghostex page retire <page>`
//! for a page and its open questions, with its own --help, and the coordinator playbook and Ghostex
//! Help say where it is documented.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::{
    domain::read_domain_rpc_params,
    protocol::rpc_success,
    server::{AppState, RoutedResponse, routed_json},
};

use super::store::{earlier_pages_from, is_page_id, note_retired, read_record};

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
/// `{retired: [{id, title, chats}], notFound: [...], noSession: [...]}`. A page no chat shows and
/// no session published waits on nobody, so there is nothing to retire (`noSession`).
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

pub(super) fn retire(state: &AppState, given: &[String]) -> Value {
    let dir = &state.paths.root_dir;
    let (mut retired, mut not_found, mut no_session) = (Vec::new(), Vec::new(), Vec::new());
    let db = match crate::storage::open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            log_failure(state, None, &error);
            return json!({"retired": [], "notFound": [], "noSession": given});
        }
    };
    // The pages a retired cover's links made, retired with it after the loop.
    let mut sections: Vec<String> = Vec::new();
    for text in given {
        let Some((id, record)) =
            page_id_of(text).and_then(|id| read_record(dir, &id).map(|record| (id, record)))
        else {
            not_found.push(json!(text));
            continue;
        };
        let publisher = record
            .get("sessionRef")
            .and_then(Value::as_str)
            .map(crate::agent_hooks::probing::parse_global_session_ref)
            .and_then(|parsed| match parsed {
                (Some(project_id), Some(session_id)) => Some((project_id, session_id)),
                _ => None,
            });
        let chats = chats_showing(&db, &id, publisher);
        if chats.is_empty() {
            no_session.push(json!(id));
            continue;
        }
        let mut taken_back = 0;
        for (project_id, session_id) in &chats {
            // The retired list first: `set_removed` then tells every client, its frame carrying
            // both.
            let result = note_retired_in_session(&db, project_id, session_id, &id).and_then(|()| {
                crate::session_chat_open_questions::set_removed(state, project_id, session_id, &id)
            });
            if let Err(error) = result {
                log_failure(state, Some(&id), &error);
                continue;
            }
            taken_back += 1;
            // A page the reader had already dismissed changed no removed list, so no frame went
            // out.
            emit_frame(state, project_id, session_id);
        }
        if taken_back == 0 {
            no_session.push(json!(id));
            continue;
        }
        note_retired(dir, &id);
        remember_retired(&id);
        sections.extend(super::file_page::sections_of(dir, &id));
        retired.push(json!({
            "id": id,
            "title": record.get("title").and_then(Value::as_str).unwrap_or("Live page"),
            "chats": taken_back,
        }));
    }
    // A cover's sections answer to its agent only through the cover (file_page.rs
    // `page_for_cover_link`): retiring the cover takes them back too, so none keeps waiting on the
    // reader. Sections record their cover, never another section, so this goes one level deep.
    //
    // CDXC:SessionChat 2026-11-21 DECISION:
    // Security review of linked sections (Fable), smallest fix: "retiring a cover (ghostex page
    // retire, or a re-publish that replaces it) also retires the pages its links made, and a
    // retired cover's links open read-only".
    sections.retain(|id| {
        !given
            .iter()
            .any(|text| page_id_of(text).as_deref() == Some(id))
    });
    if !sections.is_empty() {
        let more = retire(state, &sections);
        retired.extend(more["retired"].as_array().cloned().unwrap_or_default());
    }
    json!({"retired": retired, "notFound": not_found, "noSession": no_session})
}

fn log_failure(state: &AppState, id: Option<&str>, error: &anyhow::Error) {
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "visualPages.retireFailed".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: id.map(|id| json!({ "pageId": id })),
    });
}

/// The chats that show the page: its publishing session, and every session whose resource index
/// lists it or whose open questions still count it. Page ids are 32 random hex characters, so a
/// plain text match finds no other page.
fn chats_showing(
    db: &rusqlite::Connection,
    id: &str,
    publisher: Option<(String, String)>,
) -> Vec<(String, String)> {
    let mut chats: Vec<(String, String)> = publisher.into_iter().collect();
    let found = db
        .prepare(&format!(
            "SELECT projectId, sessionId FROM session_chat_resources WHERE instr(pagesJson, ?1) > 0
            UNION SELECT projectId, sessionId FROM sessions
            WHERE instr(COALESCE(json_extract(runtimeSettingsJson, '$.{}'), ''), ?1) > 0",
            crate::session_chat_open_questions::RUNTIME_KEY
        ))
        .and_then(|mut statement| {
            statement
                .query_map(rusqlite::params![id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()
        })
        .unwrap_or_default();
    for chat in found {
        if !chats.contains(&chat) {
            chats.push(chat);
        }
    }
    chats
}

/// Whether each page seen by a refresh was retired, so its record is read once per run of
/// gxserver: `retiredAt` is written only by [`retire`], which updates this too.
fn retired_memo() -> &'static Mutex<HashMap<String, bool>> {
    static MEMO: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    MEMO.get_or_init(Default::default)
}

fn remember_retired(id: &str) {
    if let Ok(mut memo) = retired_memo().lock() {
        memo.insert(id.to_string(), true);
    }
}

fn page_retired(dir: &std::path::Path, id: &str) -> bool {
    if let Some(known) = retired_memo()
        .lock()
        .ok()
        .and_then(|memo| memo.get(id).copied())
    {
        return known;
    }
    let retired = read_record(dir, id).is_some_and(|record| record.get("retiredAt").is_some());
    if let Ok(mut memo) = retired_memo().lock() {
        memo.insert(id.to_string(), retired);
    }
    retired
}

/// Of `waiting` (pages this chat still counts), the ones their agent retired: each is marked
/// Replaced and removed here, as [`retire`] does, and returned so the caller stops counting it.
/// The caller (`session_chat_open_questions::refresh`) brings the count up to date itself.
pub(crate) fn take_back_retired_pages<'a>(
    state: &AppState,
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    waiting: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let dir = &state.paths.root_dir;
    let mut taken = Vec::new();
    for id in waiting {
        if !page_retired(dir, id) {
            continue;
        }
        let result = note_retired_in_session(db, project_id, session_id, id).and_then(|()| {
            crate::session_chat_open_questions::remove_without_refresh(
                state, db, project_id, session_id, id,
            )
        });
        match result {
            Ok(_) => taken.push(id.to_string()),
            Err(error) => log_failure(state, Some(id), &error),
        }
    }
    taken
}

/// After `ghostex show` published `newest` from the file at `source`: the pages published from that
/// file before are retired in every chat that shows them, on a background thread. A chat that
/// holds both cards already reads the older one Replaced (gx-chat-core matches the block's
/// `source`); this also reaches a chat that shows only the older card.
pub(crate) fn retire_earlier_from_same_file(state: &Arc<AppState>, newest: &str, source: &str) {
    let (state, newest, source) = (state.clone(), newest.to_string(), source.to_string());
    tokio::task::spawn_blocking(move || {
        let earlier = earlier_pages_from(&state.paths.root_dir, &source, &newest);
        if !earlier.is_empty() {
            retire(&state, &earlier);
        }
    });
}

/// Where a session keeps the pages its agent retired (page ids), beside the removed list.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User: a page an agent retires with `ghostex page retire` says Replaced on its card, not Seen or
/// Canceled, on the desktop and the phone, "even though it needs a separate list on gxserver";
/// older phones may keep showing Seen or Canceled. The chat frames carry the list as
/// `retiredPageIds` (gx-chat-core transcript/document.rs marks those cards Replaced).
const RETIRED_KEY: &str = "sessionChatPagesRetired";
/// How many retired pages a session keeps at most; the oldest are dropped first.
const RETIRED_LIMIT: usize = 500;

/// The pages this session's agent retired, for the chat's frames (`retiredPageIds`).
pub(crate) fn read_retired_page_ids(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    use rusqlite::OptionalExtension;
    let value: Option<String> = db
        .query_row(
            &format!(
                "SELECT json_extract(runtimeSettingsJson, '$.{RETIRED_KEY}') FROM sessions WHERE projectId = ?1 AND sessionId = ?2"
            ),
            rusqlite::params![project_id, session_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten();
    value
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn note_retired_in_session(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    id: &str,
) -> anyhow::Result<()> {
    let mut ids = read_retired_page_ids(db, project_id, session_id);
    if ids.iter().any(|known| known == id) {
        return Ok(());
    }
    ids.push(id.to_string());
    if ids.len() > RETIRED_LIMIT {
        ids.drain(..ids.len() - RETIRED_LIMIT);
    }
    db.execute(
        &format!(
            r#"UPDATE sessions SET runtimeSettingsJson = json_set(runtimeSettingsJson, '$.{RETIRED_KEY}', json(?3))
            WHERE projectId = ?1 AND sessionId = ?2"#
        ),
        rusqlite::params![project_id, session_id, serde_json::to_string(&ids)?],
    )?;
    Ok(())
}

/// Sends the session's chat its state frame again, which carries `retiredPageIds`.
fn emit_frame(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    if let Ok(Some(session)) = repository.get_session(project_id, session_id) {
        crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &session);
    }
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
