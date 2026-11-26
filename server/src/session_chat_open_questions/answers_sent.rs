//! The forms and pages whose answers gxserver took from the user and is typing into the agent, or
//! already typed: they count as answered from that moment, on every client, even though the agent's
//! transcript does not hold the answers yet.
//!
//! CDXC:SessionStatus 2026-11-26 DECISION:
//! User: "when I respond to the agent with a question form, an inline HTML, or a live page ... it keeps showing on my screen, on the phone and on the PC ... I wanted it to be gone as soon as I answer." A form or page leaves the count, the pink dot, `answer requested (n)`, the Inbox, Needs you and the coordinator board the moment its answers reach gxserver, without waiting for the agent to take them in (a busy agent records a message only when its tool call or turn ends), and it stays gone after a restart or a reload of any client. A send that fails takes its mark back, so the form opens again with its failure. Extends the 2026-10-20 rule in session_chat_open_questions.rs: answers on their way to the agent now count, not only answers waiting in the queue.
//! SEE-ALSO: server/src/session_chat_queue_runtime/send.rs (`send_session_chat_message_with_draft` marks and takes back), server/src/visual_pages/answers.rs (a page's answers are stored and counted before they are typed).

use std::sync::Mutex;

use ghostex_gx_chat_core::transcript::page_answers::answers_page_ids;

use super::{read_id_list, refresh, write_id_list};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Where the session keeps them (form or page ids, oldest first).
const SENT_KEY: &str = "sessionChatAnswersSent";
/// How many a session keeps at most; the oldest are dropped first, by then long in the transcript.
const SENT_LIMIT: usize = 200;
/// Marks and take-backs of one session never drop each other's ids.
static SENT_WRITE_LOCK: Mutex<()> = Mutex::new(());

/// The forms and pages marked answered by a send.
pub(super) fn sent_ids(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
) -> Vec<String> {
    read_id_list(db, project_id, session_id, SENT_KEY)
}

/// Marks the forms and pages `text` answers (`<ghostex-page-answers page="…">`) as answered and
/// brings the session's counts up to date on a thread of its own. Returns the ids it marked, for
/// [`take_back`] when the send fails.
pub(crate) fn mark(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> Vec<String> {
    let ids = answers_page_ids(text);
    if ids.is_empty() {
        return Vec::new();
    }
    let marked = {
        let _guard = SENT_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return Vec::new();
        };
        let mut sent = sent_ids(&db, project_id, session_id);
        let marked: Vec<String> = ids.into_iter().filter(|id| !sent.contains(id)).collect();
        if marked.is_empty() {
            return Vec::new();
        }
        sent.extend(marked.iter().cloned());
        if sent.len() > SENT_LIMIT {
            sent.drain(..sent.len() - SENT_LIMIT);
        }
        if write_id_list(state, &db, project_id, session_id, SENT_KEY, &sent).is_err() {
            return Vec::new();
        }
        marked
    };
    refresh_soon(state, project_id, session_id);
    marked
}

/// Takes back what [`mark`] marked for a send that did not go through. The counts are brought up to
/// date by the next refresh: a send that failed because the agent was busy is queued right after,
/// and the queue counts its answers (`broadcast_session_chat_queue_state` refreshes them).
pub(crate) fn take_back(state: &AppState, project_id: &str, session_id: &str, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    {
        let _guard = SENT_WRITE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(db) = open_gxserver_database(&state.paths) else {
            return;
        };
        let mut sent = sent_ids(&db, project_id, session_id);
        let before = sent.len();
        sent.retain(|id| !ids.contains(id));
        if sent.len() == before
            || write_id_list(state, &db, project_id, session_id, SENT_KEY, &sent).is_err()
        {
            return;
        }
    }
    refresh_soon(state, project_id, session_id);
}

/// [`refresh`] on a thread of its own, so a caller that holds a send or a queue never waits on the
/// transcript read, and never holds its own locks while the ledger's are taken.
pub(crate) fn refresh_soon(state: &AppState, project_id: &str, session_id: &str) {
    let (state, project_id, session_id) = (
        state.clone(),
        project_id.to_string(),
        session_id.to_string(),
    );
    std::thread::spawn(move || refresh(&state, &project_id, &session_id, None));
}
