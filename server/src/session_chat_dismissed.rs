//! The questions the user dismissed (or marked read) that the session's agent has not been told of
//! yet. Dismiss sends the agent nothing at the time: each dismissal is kept here by session, and
//! the next message the agent gets anyway (the user's own, a queued one, a thread report) carries
//! them in its hidden `<ghostex-system>` note (session_chat_queue_runtime/send.rs
//! `send_ghostex_notes`, the words in gx-chat-core `session::appended_notes::dismissed_note`).
//! They are forgotten once that note is delivered.
//!
//! CDXC:SessionChat 2026-11-17 DECISION:
//! User, of "I dismissed your questions there: no need to ask them again." showing up as their
//! own bubble, twice for two dismissals, and waking the agent for nothing: "a way to just mark it
//! read or handled from outside, without telling the AI". Picked on the dismiss-quietly page:
//! A (Q3800, silent, told with the next message the agent gets anyway, never a bubble), the
//! default Dismiss silent and told later (Q3801), the phone the same (Q3802), the word stays
//! Dismiss (Q3803), and coordinator work item asks and Needs you to-dos go quiet too (Q3804).
//! With the note: "Put mark as read as the option for when there's no questions, not dismissed":
//! a row with no questions (a Needs you to-do, a page that only waits to be seen) reads Mark as
//! read, one with questions Dismiss; both are silent. Supersedes the 2026-11-10 rule that Cancel
//! (Dismiss) also tells the agent (gx-chat-core questions/later.rs `cancel`).
//! SEE-ALSO: packages/gx-chat-core/src/questions/later.rs (`cancel`, which names the dismissal),
//! server/src/coordinators/work.rs (a work item ask's cancel), server/src/session_needs_you/api.rs
//! (a to-do's cancel).

use std::sync::Mutex;

/// The dismissals not yet told, by session, in gxserver's `metadata`: a JSON list of names
/// ("\"Release plan\" (Q12 to Q14)", "W7 Q12 \"Which branch?\"").
const UNTOLD_KEY: &str = "sessionChatDismissedUntold";
/// The most kept for one session; the oldest go first.
const UNTOLD_LIMIT: usize = 50;
/// A name's longest form, in characters.
const NAME_LIMIT: usize = 200;

/// One reader-writer at a time, so a dismissal recorded while a note is delivered is not lost.
static LOCK: Mutex<()> = Mutex::new(());

fn untold_key(project_id: &str, session_id: &str) -> String {
    format!("{UNTOLD_KEY}:{project_id}:{session_id}")
}

fn read(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<String> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [untold_key(project_id, session_id)],
        |row| row.get::<_, String>(0),
    )
    .ok()
    .and_then(|text| serde_json::from_str(&text).ok())
    .unwrap_or_default()
}

fn write(db: &rusqlite::Connection, project_id: &str, session_id: &str, names: &[String]) {
    let key = untold_key(project_id, session_id);
    let _ = if names.is_empty() {
        db.execute("DELETE FROM metadata WHERE key = ?1", [key])
    } else {
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
            rusqlite::params![
                key,
                serde_json::to_string(names).unwrap_or_else(|_| "[]".to_string()),
                chrono::Utc::now().to_rfc3339()
            ],
        )
    };
}

/// Keeps `name` (the dismissed question as the agent knows it) for the session's next note.
/// Blocking.
pub(crate) fn record(db: &rusqlite::Connection, project_id: &str, session_id: &str, name: &str) {
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NAME_LIMIT)
        .collect();
    if name.is_empty() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut names = read(db, project_id, session_id);
    if names.contains(&name) {
        return;
    }
    names.push(name);
    if names.len() > UNTOLD_LIMIT {
        names.drain(..names.len() - UNTOLD_LIMIT);
    }
    write(db, project_id, session_id, &names);
}

/// The dismissals the agent has not been told of, oldest first. Blocking.
pub(crate) fn untold(db: &rusqlite::Connection, project_id: &str, session_id: &str) -> Vec<String> {
    let _lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    read(db, project_id, session_id)
}

/// Forgets `told`, once the note naming them was delivered; any recorded since stay. Blocking.
pub(crate) fn forget(
    db: &rusqlite::Connection,
    project_id: &str,
    session_id: &str,
    told: &[String],
) {
    if told.is_empty() {
        return;
    }
    let _lock = LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut names = read(db, project_id, session_id);
    names.retain(|name| !told.contains(name));
    write(db, project_id, session_id, &names);
}
