//! The `sideChats` chat field: a session's side chats, as its chat's Side chats pill lists them.
//!
//! CDXC:SideChats 2026-11-02 WHY:
//! The coordinator supervisor's tick rebuilds this cache and republishes the main session's chat when its side chats changed, the way it does the Workers panel (coordinators/panel.rs); the frame builders only read the cache because they run under the stream's emit lock. Absent on a frame means unchanged, so a builder that leaves it out never blanks the pill; a main session whose last side chat went away gets an empty list once.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (the tick), server/src/session_chat_follower/frames.rs and session_chat_read.rs (carry it), packages/gx-chat-core/src/extras/side_chats.rs (the pill).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::launch::PARENT_KEY;
use crate::domain::DomainRepository;
use crate::presentation::{effective_lifecycle_state, now_iso, presentation_activity};

type SessionKey = (String, String);

/// What the frames carry: each main session's list, and each side chat's main session.
#[derive(Default)]
struct Cache {
    lists: HashMap<SessionKey, Value>,
    parents: HashMap<SessionKey, Value>,
}

fn cache() -> &'static Mutex<Cache> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}

/// One side chat's row: what the pill's list shows and what its dot reads.
fn build_row(session: &Value, generated_at: &str) -> Value {
    let lifecycle = effective_lifecycle_state(session);
    let running = lifecycle == "running";
    let activity = presentation_activity(session, generated_at);
    let questions = if running {
        crate::session_chat_async_questions::pending_question_count(session)
            + crate::session_chat_open_questions::open_question_count(session)
    } else {
        0
    };
    let mut row = json!({
        "projectId": session.get("projectId").cloned().unwrap_or(Value::Null),
        "sessionId": session.get("sessionId").cloned().unwrap_or(Value::Null),
        "title": super::side_chat_title(session)
            .unwrap_or_else(|| crate::coordinators::session_title_of(session)),
        "working": running && activity == "working",
        "lifecycleState": lifecycle,
        "createdAt": session.get("createdAt").cloned().unwrap_or(Value::Null),
    });
    if crate::session_approval::session_awaits_approval(session, generated_at) {
        row["needsApproval"] = json!(true);
    } else if questions > 0 {
        row["needsAnswer"] = json!(true);
    }
    // A reply the user has not seen yet, like the sidebar's finished-turn attention.
    if running && activity == "attention" {
        row["unread"] = json!(true);
    }
    row
}

/// Every side chat on this computer, by its main session, oldest first.
fn side_chats_by_parent(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> (HashMap<SessionKey, Vec<Value>>, HashMap<SessionKey, Value>) {
    let generated_at = now_iso();
    let mut by_parent: HashMap<SessionKey, Vec<Value>> = HashMap::new();
    let mut parents: HashMap<SessionKey, Value> = HashMap::new();
    let mut parent_titles: HashMap<SessionKey, String> = HashMap::new();
    let Ok(mut statement) = db.prepare_cached(&format!(
        "SELECT * FROM sessions WHERE instr(runtimeSettingsJson, '\"{PARENT_KEY}\"') > 0 ORDER BY createdAt"
    )) else {
        return (by_parent, parents);
    };
    let Ok(rows) = statement.query_map([], crate::domain::session_row_from_sql) else {
        return (by_parent, parents);
    };
    for row in rows.flatten() {
        let Ok(session) = crate::domain::session_from_row(&repository.server_id, row) else {
            continue;
        };
        let (Some(project_id), Some(parent)) = (
            session.get("projectId").and_then(Value::as_str),
            super::parent_of(&session),
        ) else {
            continue;
        };
        let parent_key = (project_id.to_string(), parent.clone());
        let parent_title = parent_titles
            .entry(parent_key.clone())
            .or_insert_with(|| {
                repository
                    .get_session(project_id, &parent)
                    .ok()
                    .flatten()
                    .map(|session| crate::coordinators::session_title_of(&session))
                    .unwrap_or_default()
            })
            .clone();
        if let Some(session_id) = session.get("sessionId").and_then(Value::as_str) {
            parents.insert(
                (project_id.to_string(), session_id.to_string()),
                json!({ "sessionId": parent, "title": parent_title }),
            );
        }
        by_parent
            .entry(parent_key)
            .or_default()
            .push(build_row(&session, &generated_at));
    }
    (by_parent, parents)
}

/// Rebuilds every main session's side chat list and every side chat's main session, and returns
/// the sessions whose chat must be sent again.
pub(crate) fn refresh_side_chat_panels(
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Vec<SessionKey> {
    let (built, parents) = side_chats_by_parent(db, repository);
    let Ok(mut cache) = cache().lock() else {
        return Vec::new();
    };
    let mut changed = Vec::new();
    for (key, rows) in &built {
        let value = Value::Array(rows.clone());
        if cache.lists.get(key) != Some(&value) {
            cache.lists.insert(key.clone(), value);
            changed.push(key.clone());
        }
    }
    // A main session whose last side chat went away publishes an empty list once.
    let gone = cache
        .lists
        .iter()
        .filter(|(key, value)| {
            !built.contains_key(*key) && value.as_array().is_some_and(|rows| !rows.is_empty())
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in gone {
        cache.lists.insert(key.clone(), json!([]));
        changed.push(key);
    }
    for (key, parent) in &parents {
        if cache.parents.get(key) != Some(parent) {
            changed.push(key.clone());
        }
    }
    cache.parents = parents;
    changed.sort();
    changed.dedup();
    changed
}

/// The cached list for one main session, as `/api/listSideChats` returns it.
pub(crate) fn cached_side_chats(project_id: &str, session_id: &str) -> Vec<Value> {
    cache()
        .lock()
        .ok()
        .and_then(|cache| {
            cache
                .lists
                .get(&(project_id.to_string(), session_id.to_string()))
                .and_then(Value::as_array)
                .cloned()
        })
        .unwrap_or_default()
}

/// Adds `sideChats` to a chat frame or read result of a session that has had side chats, and
/// `sideChatOf` (its main session's id and title) to one of a side chat.
pub fn insert_side_chats(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let Ok(cache) = cache().lock() else {
        return;
    };
    let key = (project_id.to_string(), session_id.to_string());
    if let Some(value) = cache.lists.get(&key) {
        frame.insert("sideChats".to_string(), value.clone());
    }
    if let Some(parent) = cache.parents.get(&key) {
        frame.insert("sideChatOf".to_string(), parent.clone());
    }
}
