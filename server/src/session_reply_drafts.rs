//! Reply drafts: the words typed in a question's or work item's Reply box and not sent yet, by
//! session and row, so they are there again after the app quits, restarts, is rebuilt or crashes.
//! One `metadata` entry per session (`sessionReplyDrafts:<projectId>:<sessionId>`), the store the
//! session settings beside it use, holding `{ <row>: { text, updatedAt } }`.
//!
//! CDXC:Coordinators 2026-11-11 DECISION:
//! User, of Reply drafts kept only in the app's memory ("a rebuild or a quit drops them"): "not
//! good enough fam. pls fix." gxserver keeps them, as it keeps the chat box's draft
//! (`session_chat_drafts`); they stay out of that table and its send path, which carry one draft
//! per session with versions. A draft goes when it is sent or its row is gone (the client drops
//! it, apps/desktop/src/app/native_chat/reply_popover.rs); one left untouched for `STALE_DAYS` is
//! dropped here when its session's drafts are read, so none piles up.
//!
//! SEE-ALSO: apps/desktop/src/app/native_chat/reply_popover.rs (`KEPT_REPLIES`, the client).

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

const KEY_PREFIX: &str = "sessionReplyDrafts:";
/// A draft's longest text, and the most rows one session keeps (the oldest go first).
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_ROWS: usize = 200;
/// A draft nobody touched for this long is dropped when its session's drafts are read.
const STALE_DAYS: i64 = 30;

fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite Reply drafts error: {error}"),
    }
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn read(db: &Connection, key: &str) -> Result<Map<String, Value>, DomainStateError> {
    let stored = db
        .query_row("SELECT value FROM metadata WHERE key = ?1", [key], |row| {
            row.get::<_, String>(0)
        })
        .optional()
        .map_err(sql_error)?;
    Ok(stored
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default())
}

fn write(db: &Connection, key: &str, drafts: &Map<String, Value>) -> Result<(), DomainStateError> {
    if drafts.is_empty() {
        db.execute("DELETE FROM metadata WHERE key = ?1", [key])
            .map_err(sql_error)?;
        return Ok(());
    }
    db.execute(
        r#"
        INSERT INTO metadata (key, value, updatedAt)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt
        "#,
        rusqlite::params![key, Value::Object(drafts.clone()).to_string(), now()],
    )
    .map_err(sql_error)?;
    Ok(())
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn updated_at(draft: &Value) -> Option<DateTime<Utc>> {
    draft
        .get("updatedAt")
        .and_then(Value::as_str)
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc))
}

/// `/api/sessionReplyDrafts`: `{ projectId, sessionId, op }`.
/// - `list` (the default): answers `{ drafts: { <row>: <text> } }`.
/// - `set`: `{ row, text }` keeps `text` as the row's draft, an empty `text` drops it.
/// - `drop`: `{ rows: [<row>] }` drops those rows' drafts (sent, or their question gone).
///
/// Every op answers the session's drafts after it.
pub fn handle(db: &Connection, params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let (Some(project_id), Some(session_id)) =
        (text(params, "projectId"), text(params, "sessionId"))
    else {
        return Err(DomainStateError::bad_request(
            "sessionReplyDrafts requires projectId and sessionId.",
        ));
    };
    let key = format!("{KEY_PREFIX}{project_id}:{session_id}");
    let mut drafts = read(db, &key)?;
    let before = drafts.len();
    let mut changed = false;
    // Stale ones go whenever the session's drafts are read.
    let cutoff = Utc::now() - chrono::Duration::days(STALE_DAYS);
    drafts.retain(|_, draft| updated_at(draft).is_some_and(|at| at > cutoff));
    changed |= drafts.len() != before;
    match params.get("op").and_then(Value::as_str).unwrap_or("list") {
        "list" => {}
        "set" => {
            let row = text(params, "row")
                .ok_or_else(|| DomainStateError::bad_request("Name the draft's row."))?;
            let words = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if words.trim().is_empty() {
                changed |= drafts.remove(&row).is_some();
            } else {
                let words: String = words.chars().take(MAX_TEXT_CHARS).collect();
                drafts.insert(row, json!({ "text": words, "updatedAt": now() }));
                // The oldest go past the session's limit.
                while drafts.len() > MAX_ROWS {
                    let oldest = drafts
                        .iter()
                        .min_by_key(|(_, draft)| updated_at(draft))
                        .map(|(row, _)| row.clone());
                    match oldest {
                        Some(row) => drafts.remove(&row),
                        None => break,
                    };
                }
                changed = true;
            }
        }
        "drop" => {
            for row in params
                .get("rows")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                changed |= drafts.remove(row).is_some();
            }
        }
        other => {
            return Err(DomainStateError::bad_request(format!(
                "Unknown sessionReplyDrafts op {other}. Use list, set or drop."
            )))
        }
    }
    if changed {
        write(db, &key, &drafts)?;
    }
    let texts: Map<String, Value> = drafts
        .iter()
        .map(|(row, draft)| (row.clone(), draft.get("text").cloned().unwrap_or_default()))
        .collect();
    Ok(json!({ "drafts": texts }))
}
