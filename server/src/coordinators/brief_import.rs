//! Thread brief's briefs become work items, once: when the Work items version of the extension
//! (the one without its own server) is installed and on, every session's brief in the extension's
//! old store (`<extensions>/data/thread-brief/briefs.json`) turns into that session's items and
//! Topic, and the file is renamed `briefs.converted.json`, so it never runs twice and nothing is
//! lost.
//!
//! What each part becomes: the objective the Topic (the user's stays the user's); Next up items
//! Later, up next, in order; Done items Finished; Blocked items Waiting on what they waited on;
//! Parked items Later, parked.
//!
//! CDXC:Coordinators 2026-11-18 SEE-ALSO: session_work.rs (the user's decision to merge Briefs into
//! work items), ~/dev/_active/Ghostex-extensions/extensions/thread-brief/src/server.mjs in its 0.2.0
//! version (the store's shape).

use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::Connection;
use serde_json::Value;

use super::records::{now_iso, read_coordinator, read_thread, SessionKey};
use super::session_work::{read_topic, write_topic, TOPIC_MAX_CHARS, WORK_ITEMS_EXTENSION};
use super::work::{next_item_number, write_work_item, WorkEvent, WorkItem, WorkWait};

/// Set once this gxserver has looked: the conversion runs at most once per start.
static LOOKED: AtomicBool = AtomicBool::new(false);

/// The extension version that keeps work items instead of briefs: 0.3.0 and later.
fn keeps_work_items(version: &str) -> bool {
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0));
    let (major, minor) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
    (major, minor) >= (0, 3)
}

fn item_text(entry: &Value) -> String {
    entry
        .as_str()
        .or_else(|| entry["text"].as_str())
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn session_exists(db: &Connection, key: &SessionKey) -> bool {
    db.query_row(
        "SELECT 1 FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        [&key.0, &key.1],
        |_| Ok(()),
    )
    .is_ok()
}

/// A coordinator or a coordinator's thread keeps no items of its own: a session promoted to
/// coordinator since its brief, or a thread, is left out.
fn is_coordinator_or_thread(db: &Connection, key: &SessionKey) -> bool {
    read_coordinator(db, &key.0, &key.1)
        .ok()
        .flatten()
        .is_some()
        || read_thread(db, &key.0, &key.1).ok().flatten().is_some()
}

/// One brief's items, written for `key`. Returns how many.
fn import_brief(db: &Connection, key: &SessionKey, brief: &Value) -> usize {
    // Finished items are stamped now, so the ones the brief had done stay under Finished today
    // instead of falling out of the panel's few days and the 14-day cleanup at once.
    let converted_at = now_iso();
    let at = brief["updatedAt"]
        .as_str()
        .filter(|at| chrono::DateTime::parse_from_rfc3339(at).is_ok())
        .map(str::to_string)
        .unwrap_or_else(now_iso);
    // A Topic is one line of at most 200 characters; a longer objective is cut to fit.
    let objective: String = item_text(&brief["objective"])
        .chars()
        .take(TOPIC_MAX_CHARS)
        .collect();
    if !objective.is_empty() && read_topic(db, key).is_null() {
        let by_user = brief["objectiveByUser"] == true;
        let _ = write_topic(db, key, &objective, by_user);
    }
    let list = |field: &str| -> Vec<Value> {
        brief[field]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|entry| !item_text(entry).is_empty())
            .collect()
    };
    let mut written = 0;
    let mut add = |title: String, status: &str, reason: &str, wait: Option<String>| {
        let Ok(number) = next_item_number(db, key) else {
            return;
        };
        let finished = status == "finished";
        let item = WorkItem {
            number,
            title: title.chars().take(200).collect(),
            status: status.to_string(),
            reason: reason.to_string(),
            waits: wait
                .map(|on| {
                    vec![WorkWait {
                        on,
                        until: String::new(),
                        since: at.clone(),
                    }]
                })
                .unwrap_or_default(),
            history: vec![WorkEvent {
                at: at.clone(),
                kind: "add".to_string(),
                text: "From the session's brief".to_string(),
                status: status.to_string(),
            }],
            created_at: at.clone(),
            updated_at: if finished {
                converted_at.clone()
            } else {
                at.clone()
            },
            finished_at: finished.then(|| converted_at.clone()),
            ..WorkItem::default()
        };
        if write_work_item(db, key, &item).is_ok() {
            written += 1;
        }
    };
    for entry in list("blocked") {
        let on = item_text(&entry["waitingOn"]);
        let on = if on.is_empty() {
            "something outside".to_string()
        } else {
            on
        };
        add(item_text(&entry), "waiting", "", Some(on));
    }
    for entry in list("next") {
        add(item_text(&entry), "later", "next", None);
    }
    for entry in list("parked") {
        add(item_text(&entry), "later", "parked", None);
    }
    // Done is newest first in a brief; oldest first here, as the items were finished.
    for entry in list("done").into_iter().rev() {
        add(item_text(&entry), "finished", "", None);
    }
    written
}

/// Converts the old briefs once, when the Work items version of the extension is installed and
/// on. Called from the supervisor's tick (panel.rs).
pub(super) fn import_briefs_once(db: &Connection) {
    if LOOKED.load(Ordering::Relaxed) {
        return;
    }
    let paths = crate::paths::get_gxserver_paths(None);
    if !super::session_work::work_items_extension_on(&paths) {
        return;
    }
    let extensions_dir = paths.extensions_dir.as_path();
    let installed = extensions_dir.join("installed").join(WORK_ITEMS_EXTENSION);
    let Ok(manifest) = crate::extensions::read_manifest(&installed, Some(WORK_ITEMS_EXTENSION))
    else {
        return;
    };
    if !keeps_work_items(&manifest.version) {
        return;
    }
    // One caller converts: the supervisor's tick and a request may both get here.
    if LOOKED.swap(true, Ordering::Relaxed) {
        return;
    }
    let data = extensions_dir.join("data").join(WORK_ITEMS_EXTENSION);
    let store = data.join("briefs.json");
    // Moved aside first, so a folder where the file cannot be renamed converts nothing (and a
    // later start converts nothing twice); put back if the conversion does not commit.
    let converting = data.join("briefs.converting.json");
    // A start cut off mid-way left it moved aside with nothing committed: it converts from there.
    if !converting.is_file() && std::fs::rename(&store, &converting).is_err() {
        return;
    }
    let put_back = || {
        let _ = std::fs::rename(&converting, &store);
    };
    let Some(briefs) = std::fs::read_to_string(&converting)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        put_back();
        return;
    };
    // Every item and Topic in one transaction: all of them or none.
    let Ok(transaction) = db.unchecked_transaction() else {
        put_back();
        return;
    };
    let mut sessions = 0;
    let mut items = 0;
    for (reference, brief) in briefs["briefs"].as_object().into_iter().flatten() {
        let Some((project_id, session_id)) = reference.split_once(':') else {
            continue;
        };
        let key = (project_id.to_string(), session_id.to_string());
        if !session_exists(db, &key) || is_coordinator_or_thread(db, &key) {
            continue;
        }
        sessions += 1;
        items += import_brief(db, &key, brief);
    }
    if transaction.commit().is_err() {
        put_back();
        return;
    }
    // An earlier conversion's file is kept: this one takes a name of its own.
    let mut kept = data.join("briefs.converted.json");
    if kept.exists() {
        kept = data.join(format!(
            "briefs.converted-{}.json",
            chrono::Utc::now().format("%Y%m%d-%H%M%S")
        ));
    }
    let _ = std::fs::rename(&converting, kept);
    eprintln!(
        "[gxserver] Work items: converted {sessions} session briefs into {items} work items once."
    );
}
