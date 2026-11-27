//! Chat search: every message of every agent chat (the user's prompts and the agent's replies,
//! every agent, Native and Terminal chats alike, every project, closed sessions too), searchable
//! as you type.
//!
//! gxserver holds no messages of its own: a chat is the agent's transcript file, which the chat
//! reads through the agent's decoder. The indexer (`indexer.rs`) reads each session's transcript
//! with that same decoder, only the bytes it grew by, and keeps the words in a trigram FTS5 index
//! in its own database in the state folder (`store.rs`). A query (`query.rs`) reads the newest
//! matching messages and returns one result per chat, each with a snippet.
//!
//! CDXC:Sessions 2026-11-27 DECISION:
//! User: "i don't care about searching session files anymore. i wanna make the search work like
//! [the other agent GUI]'s where it searches all the chats suuuper fast. we need to show chat
//! search here in this ui [the Sessions search] so it's not something separate". So the index is
//! built from the chats Ghostex runs, Native ones included, not from the agents' history folders
//! (packages/find), and each result names the session it opens.
//! SEE-ALSO: server/src/session_chat_open_questions.rs (the same transcript resolution and
//! incremental reads), server/src/ghostex_cli/sessions/search.rs (`ghostex sessions search`).

mod indexer;
mod query;
mod store;

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::server::AppState;

/// `/api/searchChats`: `ghostex sessions search`. The Sessions search asks through
/// `/api/listPreviousSessions` with `chatSearch` (`matches_for_sessions_list`).
pub(crate) const SEARCH_ENDPOINT: &str = "/api/searchChats";

/// CDXC:Sessions 2026-11-27 WHY:
/// The Sessions search (desktop and phone) matches what was said in each chat, so the index starts
/// with gxserver and backfills in the background (throttled, low priority): the first search
/// then finds every chat instead of waiting for a backfill to begin.
const START_WITH_GXSERVER: bool = true;

#[derive(Default)]
struct Status {
    started_at: Option<Instant>,
    /// Sessions the running full pass has checked, out of how many.
    checked: usize,
    total: usize,
    /// Whether one full pass (the backfill) has finished.
    complete: bool,
    last_full_pass: Option<Duration>,
}

fn status() -> &'static Mutex<Status> {
    static STATUS: OnceLock<Mutex<Status>> = OnceLock::new();
    STATUS.get_or_init(Default::default)
}

fn note_progress(checked: usize, total: usize) {
    if let Ok(mut status) = status().lock() {
        status.checked = checked;
        status.total = total;
    }
}

fn note_full_sweep(took: Duration) {
    if let Ok(mut status) = status().lock() {
        status.complete = true;
        status.last_full_pass = Some(took);
    }
}

/// Called when gxserver starts.
pub(crate) fn start_with_gxserver(state: &Arc<AppState>) {
    if START_WITH_GXSERVER {
        ensure_started(state);
    }
}

/// Starts the indexer thread once.
pub(crate) fn ensure_started(state: &Arc<AppState>) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    if let Ok(mut status) = status().lock() {
        status.started_at = Some(Instant::now());
    }
    let state = state.clone();
    let _ = std::thread::Builder::new()
        .name("chat-search-index".to_string())
        .spawn(move || {
            for attempt in 0..5 {
                match indexer::Indexer::new(state.clone()) {
                    Ok(indexer) => return indexer.run(),
                    Err(error) => eprintln!("[chat-search] index unavailable: {error}"),
                }
                std::thread::sleep(Duration::from_secs(2 << attempt));
            }
        });
}

/// The connection queries read through, opened once.
fn reader(state: &AppState) -> &'static Mutex<Option<Connection>> {
    static READER: OnceLock<Mutex<Option<Connection>>> = OnceLock::new();
    let reader = READER.get_or_init(Default::default);
    if let Ok(mut slot) = reader.lock() {
        if slot.is_none() {
            *slot = store::open_reader(&store::database_file(&state.paths)).ok();
        }
    }
    reader
}

/// Answers `/api/searchChats`: `{ query, projectId?, limit? }` to `{ results, terms, index,
/// tookMs }`. Each result names the session it opens (`projectId`, `sessionId`, `title`,
/// `lifecycleState`, `agentId`), the message (`messageId`, `messageAtMs`, `source`: `user` or
/// `assistant`), a one-line `snippet` that starts near the match, how many of the newest
/// messages matched (`hits`) and the match `quality` the results are ranked by. Clients highlight
/// `terms` in the snippet themselves. Blocking.
pub(crate) fn search(
    state: &Arc<AppState>,
    domain: &Connection,
    params: &Map<String, Value>,
) -> Value {
    ensure_started(state);
    let started = Instant::now();
    let text = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .map_or(query::DEFAULT_LIMIT, |limit| {
            (limit as usize).clamp(1, query::MAX_LIMIT)
        });
    let request = query::Query {
        text,
        project_id,
        limit,
    };
    let results = match reader(state).lock() {
        Ok(slot) => slot
            .as_ref()
            .map(|index| query::search(index, domain, &request))
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    json!({
        "results": results,
        "terms": query::terms(&request.text),
        "index": index_status(),
        "tookMs": started.elapsed().as_secs_f64() * 1000.0,
    })
}

/// The chats whose words match a Sessions list query, for `/api/listPreviousSessions` with
/// `chatSearch: true` (clients that leave it out, such as older phones, get the list as before).
/// Each entry is a `search` result: `projectId`, `sessionId`, `snippet`, `source`, `quality`...
/// Blocking.
pub(crate) fn matches_for_sessions_list(
    state: &Arc<AppState>,
    domain: &Connection,
    params: &Map<String, Value>,
) -> Vec<Value> {
    if params.get("chatSearch").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    let text = params
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if text.is_empty() {
        return Vec::new();
    }
    ensure_started(state);
    let request = query::Query {
        text,
        project_id: params
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        limit: query::MAX_LIMIT,
    };
    let Ok(slot) = reader(state).lock() else {
        return Vec::new();
    };
    slot.as_ref()
        .map(|index| query::search(index, domain, &request))
        .unwrap_or_default()
}

/// How far the index is: `ready` once every chat has been read at least once.
fn index_status() -> Value {
    let Ok(status) = status().lock() else {
        return json!({});
    };
    json!({
        "ready": status.complete,
        "checkedSessions": status.checked,
        "totalSessions": status.total,
        "lastFullPassMs": status.last_full_pass.map(|took| took.as_millis() as u64),
        "runningForMs": status.started_at.map(|at| at.elapsed().as_millis() as u64),
    })
}
