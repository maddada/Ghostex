//! Work items for any session: an ordinary session keeps its own work items, in the same store,
//! with the same commands (`ghostex work item …`, `ghostex coordinator item …`) and in the same
//! Work panel and dock rows as a coordinator, while the Work items extension is on. Off, an
//! ordinary session works exactly as before: no Work button, no skill, and the item commands
//! refuse. Every session (coordinators too) has a one-line Topic at the top of its Work panel.
//!
//! CDXC:Coordinators 2026-11-18 DECISION:
//! User, on the Briefs pages (issue #670): "brief and work items and the side bar can be merged
//! into 1 thing that covers both cases, coordinators and regular threads … extension still needs
//! to be enabled so the skill gets loaded automatically when it's on only and the button in the
//! titlebar is shown. otherwise if not enabled regular threads work normally", then Q4135 A
//! (`ghostex work item` for every session, `ghostex coordinator item` kept as its alias), Q4136
//! "Work" for the button, panel and sidebar list, and Q4137 keep the one line at the top of the
//! Work panel for every session, "instead of goal its more like topic", rewritten with a click.
//! The extension keeps its id (`thread-brief`) so existing installs keep working. A coordinator's
//! thread keeps no list of its own: its coordinator's Work panel tracks it.
//! SEE-ALSO: server/src/coordinators/work.rs (the store and its requests), server/src/ghostex_cli/work.rs (the verbs),
//! packages/gx-chat-core/src/extras/work_source.rs (`sessionWork` in the chat), ~/dev/_active/Ghostex-extensions/extensions/thread-brief (the extension and its skill).

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::records::{now_iso, sql_error, SessionKey};
use super::work::{work_value, WorkItem};
use crate::domain::DomainStateError;

/// The extension that turns work items on for ordinary sessions. Its id stays the one Thread
/// brief shipped with, so the installs people already have carry over.
pub const WORK_ITEMS_EXTENSION: &str = "thread-brief";

/// What an ordinary session hears when it keeps work items with the extension off.
pub const WORK_ITEMS_OFF: &str = "Work items are off for this session. Turn on Work items in Settings > Extensions to keep work items in ordinary sessions.";

/// Where every session's Topic is kept (the `metadata` table): `{"<projectId>:<sessionId>":
/// {text, byUser, at}}`.
const TOPICS_KEY: &str = "sessionWorkTopics";
/// A Topic is one line.
pub const TOPIC_MAX_CHARS: usize = 200;

/// Whether the Work items extension is installed and on.
pub fn work_items_extension_on(paths: &crate::paths::GxserverPaths) -> bool {
    crate::extensions::read_store(&paths.extensions_store_file)
        .ok()
        .and_then(|store| store.get(WORK_ITEMS_EXTENSION).map(|entry| entry.enabled))
        .unwrap_or(false)
        && paths
            .extensions_dir
            .join("installed")
            .join(WORK_ITEMS_EXTENSION)
            .is_dir()
}

fn topic_record_key(key: &SessionKey) -> String {
    format!("{}:{}", key.0, key.1)
}

/// Every session's Topic, read once per tick.
pub(super) fn read_topics(db: &Connection) -> Map<String, Value> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        params![TOPICS_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    .and_then(|value| value.as_object().cloned())
    .unwrap_or_default()
}

/// A session's Topic in `topics`: `{text, byUser, at}`, or `Value::Null`.
pub(super) fn topic_in(topics: &Map<String, Value>, key: &SessionKey) -> Value {
    topics
        .get(&topic_record_key(key))
        .filter(|topic| topic["text"].as_str().is_some_and(|text| !text.is_empty()))
        .cloned()
        .unwrap_or(Value::Null)
}

/// A session's Topic as the Work panel shows it: `{text, byUser, at}`, or `Value::Null`.
pub fn read_topic(db: &Connection, key: &SessionKey) -> Value {
    topic_in(&read_topics(db), key)
}

/// Sets (or, with empty words, clears) a session's Topic. The user's Topic stays theirs: an agent's
/// rewrite of it is refused, so the agent asks instead.
pub fn write_topic(
    db: &Connection,
    key: &SessionKey,
    words: &str,
    by_user: bool,
) -> Result<Value, DomainStateError> {
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    if words.chars().count() > TOPIC_MAX_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Keep the topic to one line under {TOPIC_MAX_CHARS} characters."
        )));
    }
    let mut topics = read_topics(db);
    let record = topic_record_key(key);
    if !by_user
        && topics
            .get(&record)
            .is_some_and(|topic| topic["byUser"] == true && topic["text"] != json!(words))
    {
        return Err(DomainStateError::bad_request(
            "The user wrote this topic, so it stays theirs. If it no longer fits, ask them in your reply.",
        ));
    }
    let at = now_iso();
    let topic = json!({ "text": words, "byUser": by_user, "at": at });
    if words.is_empty() {
        topics.remove(&record);
    } else {
        topics.insert(record, topic.clone());
    }
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        params![TOPICS_KEY, Value::Object(topics).to_string(), at],
    )
    .map_err(sql_error)?;
    Ok(topic)
}

/// The chat's copy of every ordinary session's work, rebuilt by the supervisor's tick
/// (`refresh_session_work`) and read by the frame builders, which must not touch the database.
#[derive(Default)]
struct SessionWorkCache {
    /// The extension is on.
    enabled: bool,
    /// Ordinary sessions with items or a Topic: their `sessionWork`.
    values: HashMap<SessionKey, Value>,
    /// Coordinators and their threads: never an ordinary session's Work panel.
    excluded: HashSet<SessionKey>,
}

fn cache() -> &'static Mutex<SessionWorkCache> {
    static CACHE: std::sync::OnceLock<Mutex<SessionWorkCache>> = std::sync::OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// Adds `sessionWork` to a chat frame or read of any session: `{enabled}` for a session that keeps
/// no work items of its own (a coordinator, a thread, or any session while the extension is off),
/// else `{enabled: true, work, topic}`. A core from before reads nothing of it.
pub fn insert_session_work(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let Ok(cache) = cache().lock() else {
        return;
    };
    let key = (project_id.to_string(), session_id.to_string());
    let value = if cache.excluded.contains(&key) || !cache.enabled {
        json!({ "enabled": false })
    } else {
        cache
            .values
            .get(&key)
            .cloned()
            .unwrap_or_else(|| json!({ "enabled": true, "work": empty_work(), "topic": null }))
    };
    frame.insert("sessionWork".to_string(), value);
}

/// The work of a session with no items yet: an empty Work panel, with its Topic line.
fn empty_work() -> Value {
    json!({ "items": [], "asks": [], "counts": {}, "topic": null })
}

/// One ordinary session's `sessionWork`.
fn session_value(items: &[WorkItem], topic: Value, key: &SessionKey) -> Value {
    let mut work = work_value(&[], items, false, Utc::now(), None, Some(key));
    // The agent of the session is on its In progress items, not a coordinator.
    for item in work["items"].as_array_mut().into_iter().flatten() {
        if item["owner"]["kind"] == "coordinator" {
            item["owner"] = json!({ "kind": "agent", "label": "Agent" });
        }
    }
    // The Topic rides in `work` too, where a coordinator's does (panel.rs).
    work["topic"] = topic.clone();
    json!({
        "enabled": true,
        "work": work,
        "topic": topic,
    })
}

/// Rebuilds every ordinary session's `sessionWork` after the coordinators' panels. `items` are the
/// work items no coordinator took (every ordinary session's), `excluded` the coordinators and
/// their threads. Returns the sessions whose copy changed (their chats are republished), and
/// every one that had a copy when the extension was turned on or off.
pub(super) fn refresh_session_work(
    items: HashMap<SessionKey, Vec<WorkItem>>,
    topics: &Map<String, Value>,
    excluded: HashSet<SessionKey>,
) -> Vec<SessionKey> {
    let enabled = work_items_extension_on(&crate::paths::get_gxserver_paths(None));
    let mut keys: HashSet<SessionKey> = items.keys().cloned().collect();
    for record in topics.keys() {
        if let Some((project_id, session_id)) = record.split_once(':') {
            keys.insert((project_id.to_string(), session_id.to_string()));
        }
    }
    let mut values = HashMap::new();
    if enabled {
        for key in keys.into_iter().filter(|key| !excluded.contains(key)) {
            let own = items.get(&key).map(Vec::as_slice).unwrap_or_default();
            values.insert(
                key.clone(),
                session_value(own, topic_in(topics, &key), &key),
            );
        }
    }
    let Ok(mut cache) = cache().lock() else {
        return Vec::new();
    };
    let mut changed: Vec<SessionKey> = if cache.enabled != enabled {
        // The Work button comes or goes: the chats with work republish now, the rest pick it up
        // with their next frame.
        cache.values.keys().chain(values.keys()).cloned().collect()
    } else {
        values
            .iter()
            .filter(|(key, value)| cache.values.get(*key) != Some(*value))
            .map(|(key, _)| key.clone())
            .chain(
                cache
                    .values
                    .keys()
                    .filter(|key| !values.contains_key(*key))
                    .cloned(),
            )
            .collect()
    };
    changed.sort();
    changed.dedup();
    *cache = SessionWorkCache {
        enabled,
        values,
        excluded,
    };
    changed
}

/// Whether `key` may keep work items of its own: an existing ordinary session while the extension
/// is on. `Err` says why not.
pub(super) fn require_session_work(
    repository: &crate::domain::DomainRepository<'_>,
    key: &SessionKey,
) -> Result<(), DomainStateError> {
    if repository.get_session(&key.0, &key.1)?.is_none() {
        return Err(DomainStateError::not_found("No such session."));
    }
    if !work_items_extension_on(&crate::paths::get_gxserver_paths(None)) {
        return Err(DomainStateError::bad_request(WORK_ITEMS_OFF));
    }
    Ok(())
}
