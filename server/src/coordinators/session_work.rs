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

/// What a coordinator's thread hears when it keeps work items: its coordinator keeps them.
pub const THREAD_KEEPS_NONE: &str = "This session is a coordinator's thread: its coordinator keeps the work items. Put what you need from the user before them with `ghostex needs-you add \"<what to do>\"`, and report the rest to your coordinator.";

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
    /// Coordinators and their threads (here or on another computer): never an ordinary
    /// session's Work panel.
    excluded: HashSet<SessionKey>,
    /// What each value was built from, and when: an unchanged one is kept, rebuilt at least every
    /// minute for its clock-dependent parts (Finished today).
    inputs: HashMap<SessionKey, BuiltFrom>,
    /// Keys with items or a Topic whose session row is gone, and when that was last seen: not
    /// looked up again for a minute.
    missing: HashMap<SessionKey, std::time::Instant>,
    /// The tick has filled it once.
    primed: bool,
}

/// What one session's `sessionWork` was built from: its items and Topic, the question tags its
/// chat asked (a wait on one clears when it is answered) and its project's delivery events (a
/// wait on a release clears when one happens), and when.
#[derive(Clone)]
struct BuiltFrom {
    items: Vec<WorkItem>,
    topic: Value,
    asked: Vec<(String, bool)>,
    deliveries: String,
    at: std::time::Instant,
}

impl BuiltFrom {
    fn same_inputs(&self, other: &BuiltFrom) -> bool {
        self.items == other.items
            && self.topic == other.topic
            && self.asked == other.asked
            && self.deliveries == other.deliveries
    }
}

/// How long a session's work built from unchanged inputs is kept.
const REBUILD_EVERY: std::time::Duration = std::time::Duration::from_secs(60);

/// The sessions whose work in short (`session_work_summary`) changed since the supervisor last
/// asked: it tells the extension API's listeners their sessions changed (`sessionsChanged`).
static SUMMARY_CHANGES: Mutex<Vec<SessionKey>> = Mutex::new(Vec::new());

/// Takes the sessions whose work in short changed (server/coordinator_runtime.rs).
pub fn take_summary_changes() -> Vec<SessionKey> {
    SUMMARY_CHANGES
        .lock()
        .map(|mut changes| std::mem::take(&mut *changes))
        .unwrap_or_default()
}

/// Whether the extension is on, read again only when its state file changed.
pub(super) fn extension_on_cached() -> bool {
    static SEEN: Mutex<Option<(Option<std::time::SystemTime>, bool, bool)>> = Mutex::new(None);
    let paths = crate::paths::get_gxserver_paths(None);
    let modified = std::fs::metadata(&paths.extensions_store_file)
        .and_then(|metadata| metadata.modified())
        .ok();
    let installed = paths
        .extensions_dir
        .join("installed")
        .join(WORK_ITEMS_EXTENSION)
        .is_dir();
    let Ok(mut seen) = SEEN.lock() else {
        return work_items_extension_on(&paths);
    };
    if let Some((at, was_installed, on)) = *seen {
        if at == modified && was_installed == installed {
            return on;
        }
    }
    let on = work_items_extension_on(&paths);
    *seen = Some((modified, installed, on));
    on
}

/// This computer's sessions that are threads of a coordinator on another computer
/// (`REMOTE_COORDINATOR_THREAD_KEY`): their coordinator keeps their work. Read again once a minute,
/// not every tick (a thread linked since is refused by `require_session_work` meanwhile).
fn remote_threads(db: &Connection) -> Vec<SessionKey> {
    static READ: Mutex<Option<(std::time::Instant, Vec<SessionKey>)>> = Mutex::new(None);
    let Ok(mut read) = READ.lock() else {
        return read_remote_threads(db);
    };
    if let Some((at, keys)) = read.as_ref() {
        if at.elapsed() < REBUILD_EVERY {
            return keys.clone();
        }
    }
    let keys = read_remote_threads(db);
    *read = Some((std::time::Instant::now(), keys.clone()));
    keys
}

fn read_remote_threads(db: &Connection) -> Vec<SessionKey> {
    let pattern = format!("%{}%", super::REMOTE_COORDINATOR_THREAD_KEY);
    let Ok(mut statement) = db.prepare_cached(
        "SELECT projectId, sessionId FROM sessions WHERE runtimeSettingsJson LIKE ?1",
    ) else {
        return Vec::new();
    };
    statement
        .query_map([pattern], |row| Ok((row.get(0)?, row.get(1)?)))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

fn session_row_exists(db: &Connection, key: &SessionKey) -> bool {
    db.prepare_cached("SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2")
        .and_then(|mut statement| statement.exists([&key.0, &key.1]))
        .unwrap_or(false)
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

/// An ordinary session's work in short, for the Work items extension's sidebar list (the
/// extension API's sessions, extensions/ext_api.rs): `{topic, open, forYou, finishedToday}` (for
/// you is Needs you plus Blocked), or
/// `Value::Null` for a session with none (or while the extension is off).
pub fn session_work_summary(project_id: &str, session_id: &str) -> Value {
    let Ok(cache) = cache().lock() else {
        return Value::Null;
    };
    let Some(value) = cache
        .values
        .get(&(project_id.to_string(), session_id.to_string()))
    else {
        return Value::Null;
    };
    summary_of(value)
}

/// [`session_work_summary`] of one `sessionWork` value.
fn summary_of(value: &Value) -> Value {
    let counts = &value["work"]["counts"];
    let count = |status: &str| counts[status].as_u64().unwrap_or(0);
    let open: u64 = ["needs", "blocked", "moving", "idle", "waiting", "later"]
        .iter()
        .map(|status| count(status))
        .sum();
    json!({
        "topic": value["topic"]["text"].as_str().unwrap_or_default(),
        "open": open,
        "forYou": count("needs") + count("blocked"),
        "finishedToday": count("finished"),
    })
}

/// In an ordinary session's work (a list's `items`, or one item), its agent is on its In progress
/// items, not a coordinator.
pub(super) fn name_agent_owner(work: &mut Value) {
    let mut rename = |item: &mut Value| {
        if item["owner"]["kind"] == "coordinator" {
            item["owner"] = json!({ "kind": "agent", "label": "Agent" });
        }
    };
    match work.get_mut("items").and_then(Value::as_array_mut) {
        Some(items) => items.iter_mut().for_each(&mut rename),
        None => rename(work),
    }
}

/// One ordinary session's `sessionWork`.
fn session_value(items: &[WorkItem], topic: Value, key: &SessionKey) -> Value {
    let mut work = work_value(&[], items, false, Utc::now(), None, Some(key));
    name_agent_owner(&mut work);
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
/// their threads. Sessions with no row any more are left out; one whose items and Topic are
/// unchanged keeps its value. Returns the sessions whose copy changed (their chats are
/// republished), and every one that had a copy when the extension was turned on or off.
pub(super) fn refresh_session_work(
    db: &Connection,
    items: HashMap<SessionKey, Vec<WorkItem>>,
    topics: &Map<String, Value>,
    mut excluded: HashSet<SessionKey>,
) -> Vec<SessionKey> {
    let enabled = extension_on_cached();
    excluded.extend(remote_threads(db));
    let mut keys: HashSet<SessionKey> = items.keys().cloned().collect();
    for record in topics.keys() {
        if let Some((project_id, session_id)) = record.split_once(':') {
            keys.insert((project_id.to_string(), session_id.to_string()));
        }
    }
    // What the last build left, taken under the lock; the build itself runs without it, so the
    // frame builders never wait on it.
    let (mut previous_inputs, previous_values, mut missing) = match cache().lock() {
        Ok(mut cache) => (
            std::mem::take(&mut cache.inputs),
            cache.values.clone(),
            std::mem::take(&mut cache.missing),
        ),
        Err(_) => return Vec::new(),
    };
    missing.retain(|_, at| at.elapsed() < REBUILD_EVERY);
    let mut inputs = HashMap::new();
    let mut values = HashMap::new();
    if enabled {
        for key in keys.into_iter().filter(|key| !excluded.contains(key)) {
            if missing.contains_key(&key) {
                continue;
            }
            let mut asked: Vec<(String, bool)> =
                crate::session_chat_open_questions::asked_question_tags(&key.0, &key.1)
                    .into_iter()
                    .collect();
            asked.sort();
            let now = BuiltFrom {
                items: items.get(&key).cloned().unwrap_or_default(),
                topic: topic_in(topics, &key),
                asked,
                deliveries: super::work::deliveries_stamp(&key.0),
                at: std::time::Instant::now(),
            };
            let kept = previous_inputs
                .remove(&key)
                .filter(|was| was.same_inputs(&now) && was.at.elapsed() < REBUILD_EVERY)
                .and_then(|was| Some((previous_values.get(&key)?.clone(), was)));
            let (value, built) = match kept {
                Some(kept) => kept,
                None => {
                    if !session_row_exists(db, &key) {
                        missing.insert(key, std::time::Instant::now());
                        continue;
                    }
                    (session_value(&now.items, now.topic.clone(), &key), now)
                }
            };
            values.insert(key.clone(), value);
            inputs.insert(key, built);
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
    // The extension's list hears of every session whose work in short changed, and of every one
    // with work on the first fill (it may have drawn before this gxserver started).
    let moved: Vec<SessionKey> = if cache.primed {
        let summary =
            |values: &HashMap<SessionKey, Value>, key: &SessionKey| values.get(key).map(summary_of);
        changed
            .iter()
            .filter(|key| summary(&cache.values, key) != summary(&values, key))
            .cloned()
            .collect()
    } else {
        values.keys().cloned().collect()
    };
    if let Ok(mut pending) = SUMMARY_CHANGES.lock() {
        pending.extend(moved);
    }
    *cache = SessionWorkCache {
        enabled,
        values,
        excluded,
        inputs,
        missing,
        primed: true,
    };
    changed
}

/// A restored session is a new row: its work items and Topic move to it, unless it already has
/// some (previous_session_restore.rs).
pub fn carry_session_work(db: &Connection, from: &SessionKey, to: &SessionKey) {
    if from == to {
        return;
    }
    let _ = db.execute(
        r#"
        UPDATE coordinator_work_items SET coordinatorProjectId = ?3, coordinatorSessionId = ?4
        WHERE coordinatorProjectId = ?1 AND coordinatorSessionId = ?2
          AND NOT EXISTS (
            SELECT 1 FROM coordinator_work_items
            WHERE coordinatorProjectId = ?3 AND coordinatorSessionId = ?4
          )
        "#,
        params![from.0, from.1, to.0, to.1],
    );
    let mut topics = read_topics(db);
    if !topics.contains_key(&topic_record_key(to)) {
        if let Some(topic) = topics.remove(&topic_record_key(from)) {
            topics.insert(topic_record_key(to), topic);
            let _ = db.execute(
                "UPDATE metadata SET value = ?2, updatedAt = ?3 WHERE key = ?1",
                params![TOPICS_KEY, Value::Object(topics).to_string(), now_iso()],
            );
        }
    }
}

/// Whether `key` may keep work items of its own: an existing ordinary session while the extension
/// is on. `Err` says why not.
pub(super) fn require_session_work(
    repository: &crate::domain::DomainRepository<'_>,
    key: &SessionKey,
) -> Result<(), DomainStateError> {
    let Some(session) = repository.get_session(&key.0, &key.1)? else {
        return Err(DomainStateError::not_found("No such session."));
    };
    if super::remote_coordinator_of_session(&session).is_some() {
        return Err(DomainStateError::bad_request(THREAD_KEEPS_NONE));
    }
    if !work_items_extension_on(&crate::paths::get_gxserver_paths(None)) {
        return Err(DomainStateError::bad_request(WORK_ITEMS_OFF));
    }
    Ok(())
}
