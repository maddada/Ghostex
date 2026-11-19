//! Message IDs on gxserver: the number each message the user writes in a chat gets (M1, M2, …),
//! the quote a later message that points at one sends the agent, and `ghostex message`.
//!
//! gxserver decides, because every client (desktop, web, the phone, other computers) must see
//! the same number for the same message: every user row it sends a chat (the readSessionChat
//! answer, history pages, and the follower's snapshot and appended frames) is stamped with
//! `messageNumber` and `messageRef` here, next to `mentionPaths`. A number is handed out the first
//! time gxserver sees a message of the user's (gx-chat-core `is_own_prompt`), in the order the
//! messages were sent, and kept in `chat_message_numbers` with the message's words.
//!
//! CDXC:SessionChat 2026-11-20 DECISION: the user's choices (Q4237 to Q4240) are written at
//! packages/gx-chat-core/src/transcript/message_numbers.rs. Here they mean:
//!   * Only new messages: a message sent before this gxserver first numbered anything (the
//!     `chatMessageNumbersSince` stamp in `metadata`) never gets a number, so every chat starts at
//!     M1 with its first message after the update.
//!   * Never given out twice: the next number is one past the highest the chat ever had, so the
//!     messages a rewind hid keep theirs (and `ghostex message show` still prints them) and the
//!     next message gets a new one.
//!   * A fork keeps the numbers up to where it was cut: a message the fork copied (the same send
//!     time and the same words as a numbered message of another chat) takes that message's number,
//!     and the fork goes on from there.
//!   * A queued message gets its number once the agent has it: a row still in the agent's own
//!     queue is not the user's message yet (`queued`), and Ghostex's own queue is not a row at all.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/message_numbers.rs (the rules and the quote),
//! server/src/session_chat_mention_paths.rs (the same stamping for `@` mentions),
//! server/src/session_chat_queue_runtime/send.rs (where the quote is added),
//! server/src/ghostex_cli/message.rs (`ghostex message`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ghostex_gx_chat_core::transcript::message_numbers::{
    is_own_prompt, message_ref, parse_message_ref, preview, referenced_messages, shown_text,
    with_quotes, QuotedMessage,
};
use ghostex_gx_protocol::ChatMessage;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

/// The `metadata` key holding when this gxserver started numbering messages (epoch ms).
const SINCE_KEY: &str = "chatMessageNumbersSince";
/// The most of a message's words kept for `ghostex message show`.
const STORED_TEXT_MAX_BYTES: usize = 64 * 1024;
/// `/api/chatMessages`: `ghostex message show` and `list`.
pub(crate) const CHAT_MESSAGES_ENDPOINT: &str = "/api/chatMessages";

static PATHS: OnceLock<GxserverPaths> = OnceLock::new();

/// Lets the frame builders, which have no `AppState`, reach the database. Called once when
/// gxserver starts.
pub(crate) fn install(paths: &GxserverPaths) {
    let _ = PATHS.set(paths.clone());
    // The first start of a gxserver that numbers messages is where numbering begins: stamped
    // now, not at the first chat read, which can come after messages were already sent.
    if let Ok(db) = open_gxserver_database(paths) {
        since_ms(&db);
    }
}

/// What is known of one chat's numbers: message id to number, and the highest number given.
#[derive(Default)]
struct Known {
    by_id: HashMap<String, u32>,
    highest: u32,
}

type ChatKey = (String, String);

/// Every chat whose numbers were read this run. One lock for all of them: numbering is rare (a
/// new message), and two readers of the same chat must never hand out the same number.
fn known() -> &'static Mutex<HashMap<ChatKey, Known>> {
    static KNOWN: OnceLock<Mutex<HashMap<ChatKey, Known>>> = OnceLock::new();
    KNOWN.get_or_init(Default::default)
}

/// When this gxserver started numbering messages, stamped the first time it is asked.
fn since_ms(db: &Connection) -> i64 {
    static SINCE: OnceLock<i64> = OnceLock::new();
    if let Some(since) = SINCE.get() {
        return *since;
    }
    let stored: Option<i64> = db
        .query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [SINCE_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .and_then(|value| value.parse().ok());
    let since = stored.unwrap_or_else(|| {
        let now = chrono::Utc::now();
        let _ = db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
             ON CONFLICT(key) DO NOTHING",
            params![
                SINCE_KEY,
                now.timestamp_millis().to_string(),
                now.to_rfc3339()
            ],
        );
        db.query_row(
            "SELECT value FROM metadata WHERE key = ?1",
            [SINCE_KEY],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(now.timestamp_millis())
    });
    *SINCE.get_or_init(|| since)
}

fn text_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(text.as_bytes())[..16])
}

fn load_known(db: &Connection, key: &ChatKey) -> Known {
    let mut known = Known::default();
    let Ok(mut statement) = db.prepare_cached(
        "SELECT messageId, number FROM chat_message_numbers WHERE projectId = ?1 AND sessionId = ?2",
    ) else {
        return known;
    };
    let rows = statement.query_map(params![key.0, key.1], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    });
    for (id, number) in rows.into_iter().flatten().flatten() {
        known.highest = known.highest.max(number);
        known.by_id.insert(id, number);
    }
    known
}

/// A message the user wrote that has no number yet.
struct Candidate {
    index: usize,
    id: String,
    sent_at: i64,
    order: (i64, u64),
    text: String,
}

/// The number a fork's copy of a message takes: that of a numbered message of another chat sent
/// at the same moment with the same words.
fn copied_number(db: &Connection, key: &ChatKey, sent_at: i64, hash: &str) -> Option<u32> {
    db.query_row(
        "SELECT number FROM chat_message_numbers \
         WHERE sentAt = ?1 AND textHash = ?2 AND NOT (projectId = ?3 AND sessionId = ?4) \
         ORDER BY createdAt LIMIT 1",
        params![sent_at, hash, key.0, key.1],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Numbers the user's messages among `messages` that have none yet and writes `messageNumber`
/// and `messageRef` on every one that has a number. Blocking.
pub(crate) fn annotate_messages(messages: &mut [Value], project_id: &str, session_id: &str) {
    let Some(paths) = PATHS.get() else {
        return;
    };
    if project_id.is_empty() || session_id.is_empty() {
        return;
    }
    let any_user = messages
        .iter()
        .any(|message| message.get("role").and_then(Value::as_str) == Some("user"));
    if !any_user {
        return;
    }
    let Ok(db) = open_gxserver_database(paths) else {
        return;
    };
    annotate_with(&db, messages, project_id, session_id);
}

/// [`annotate_messages`] on a read answer's or frame's `messages`.
pub(crate) fn annotate_result(result: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    if let Some(messages) = result.get_mut("messages").and_then(Value::as_array_mut) {
        annotate_messages(messages, project_id, session_id);
    }
}

fn annotate_with(db: &Connection, messages: &mut [Value], project_id: &str, session_id: &str) {
    let key: ChatKey = (project_id.to_string(), session_id.to_string());
    let since = since_ms(db);
    let Ok(mut all) = known().lock() else {
        return;
    };
    let known = all
        .entry(key.clone())
        .or_insert_with(|| load_known(db, &key));
    let mut stamped: Vec<(usize, u32)> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(id) = message.get("id").and_then(Value::as_str) else {
            continue;
        };
        if let Some(number) = known.by_id.get(id) {
            stamped.push((index, *number));
            continue;
        }
        let Some(sent_at) = message.get("timestamp").and_then(Value::as_i64) else {
            continue;
        };
        if sent_at < since {
            continue;
        }
        let Ok(parsed) = serde_json::from_value::<ChatMessage>(message.clone()) else {
            continue;
        };
        if !is_own_prompt(&parsed) {
            continue;
        }
        let offset = message
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or_default();
        candidates.push(Candidate {
            index,
            id: id.to_string(),
            sent_at,
            order: (sent_at, offset),
            text: shown_text(&parsed),
        });
    }
    // In the order they were sent, so a page read at once numbers its messages in that order.
    candidates.sort_by_key(|candidate| candidate.order);
    for candidate in candidates {
        let hash = text_hash(&candidate.text);
        let taken = |number: u32| known.by_id.values().any(|given| *given == number);
        let number = copied_number(db, &key, candidate.sent_at, &hash)
            .filter(|number| !taken(*number))
            .unwrap_or(known.highest + 1);
        let mut text = candidate.text;
        if text.len() > STORED_TEXT_MAX_BYTES {
            let mut cut = STORED_TEXT_MAX_BYTES;
            while !text.is_char_boundary(cut) {
                cut -= 1;
            }
            text.truncate(cut);
        }
        let written = db.execute(
            "INSERT INTO chat_message_numbers \
             (projectId, sessionId, number, messageId, sentAt, textHash, text, createdAt) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                key.0,
                key.1,
                number,
                candidate.id,
                candidate.sent_at,
                hash,
                text,
                chrono::Utc::now().to_rfc3339()
            ],
        );
        if written.is_err() {
            // Another gxserver process, or a row this run's cache missed: read what is stored.
            *known = load_known(db, &key);
            if let Some(number) = known.by_id.get(&candidate.id) {
                stamped.push((candidate.index, *number));
            }
            continue;
        }
        known.highest = known.highest.max(number);
        known.by_id.insert(candidate.id, number);
        stamped.push((candidate.index, number));
    }
    drop(all);
    for (index, number) in stamped {
        if let Some(object) = messages[index].as_object_mut() {
            object.insert("messageNumber".to_string(), json!(number));
            object.insert(
                "messageRef".to_string(),
                json!(message_ref(session_id, number)),
            );
        }
    }
}

/// One stored message.
struct Stored {
    project_id: String,
    session_id: String,
    number: u32,
    sent_at: Option<i64>,
    text: String,
}

impl Stored {
    fn sent_label(&self) -> Option<String> {
        let at = chrono::DateTime::from_timestamp_millis(self.sent_at?)?;
        Some(
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
        )
    }

    fn json(&self, full: bool) -> Value {
        json!({
            "ref": message_ref(&self.session_id, self.number),
            "id": format!("M{}", self.number),
            "number": self.number,
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "sentAt": self.sent_at,
            "sent": self.sent_label(),
            "text": if full { self.text.clone() } else { preview(&self.text, 120) },
        })
    }
}

fn read_stored(row: &rusqlite::Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        project_id: row.get(0)?,
        session_id: row.get(1)?,
        number: row.get(2)?,
        sent_at: row.get(3)?,
        text: row.get(4)?,
    })
}

/// The message `number` of the chat `session_id` (in `project_id` when given; a session id names
/// one chat on this computer).
fn find(
    db: &Connection,
    project_id: Option<&str>,
    session_id: &str,
    number: u32,
) -> Option<Stored> {
    db.query_row(
        "SELECT projectId, sessionId, number, sentAt, text FROM chat_message_numbers \
         WHERE sessionId = ?1 AND number = ?2 AND (?3 IS NULL OR projectId = ?3) LIMIT 1",
        params![session_id, number, project_id],
        read_stored,
    )
    .optional()
    .ok()
    .flatten()
}

/// `text` with the start of every message it points at quoted under it, for the agent
/// (gx-chat-core `with_quotes`); `text` itself when it points at none. Blocking.
pub(crate) fn with_message_quotes(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> String {
    let references = referenced_messages(text);
    if references.is_empty() {
        return text.to_string();
    }
    let Ok(db) = open_gxserver_database(paths) else {
        return text.to_string();
    };
    quote_with(&db, project_id, session_id, text, &references)
}

fn quote_with(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
    references: &[ghostex_gx_chat_core::transcript::message_numbers::MessageRef],
) -> String {
    let quotes: Vec<QuotedMessage> = references
        .iter()
        .map(|reference| {
            let stored = match &reference.session_id {
                Some(other) if other != session_id => find(db, None, other, reference.number),
                _ => find(db, Some(project_id), session_id, reference.number),
            };
            QuotedMessage {
                reference: reference.written(),
                sent_at: stored.as_ref().and_then(Stored::sent_label),
                text: stored.map(|stored| stored.text),
            }
        })
        .collect();
    with_quotes(text, &quotes)
}

/// `/api/chatMessages {projectId, sessionId, op}`: `show` (`ref`: `M12` or `G9jt7.M12`) prints a
/// message whole; `list` (`last`, `grep`) the chat's messages with their first line, newest first.
pub(crate) fn handle_chat_messages_request(
    db: &Connection,
    params: &Map<String, Value>,
) -> Result<Value, crate::domain::DomainStateError> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let (project_id, session_id) = (text("projectId"), text("sessionId"));
    let bad = |message: String| crate::domain::DomainStateError {
        code: "invalidParams",
        message,
    };
    match text("op").as_str() {
        "show" => {
            let written = text("ref");
            let reference = parse_message_ref(&written).ok_or_else(|| {
                bad(format!(
                    "\"{written}\" is not a message ID. Write it as M12, or G9jt7.M12 for another chat's."
                ))
            })?;
            let stored = match &reference.session_id {
                Some(other) if *other != session_id => find(db, None, other, reference.number),
                _ => find(db, Some(&project_id), &session_id, reference.number),
            };
            match stored {
                Some(stored) => Ok(json!({ "message": stored.json(true) })),
                None => Err(crate::domain::DomainStateError {
                    code: "notFound",
                    message: format!("No message {} here.", reference.written()),
                }),
            }
        }
        "list" => {
            let last = params
                .get("last")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 500);
            let words: Vec<String> = text("grep")
                .split('|')
                .map(|word| word.trim().to_lowercase())
                .filter(|word| !word.is_empty())
                .collect();
            let mut statement = db
                .prepare(
                    "SELECT projectId, sessionId, number, sentAt, text FROM chat_message_numbers \
                     WHERE projectId = ?1 AND sessionId = ?2 ORDER BY number DESC",
                )
                .map_err(|error| crate::domain::DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                })?;
            let rows = statement
                .query_map(params![project_id, session_id], read_stored)
                .map_err(|error| crate::domain::DomainStateError {
                    code: "internalError",
                    message: error.to_string(),
                })?;
            let messages: Vec<Value> = rows
                .flatten()
                .filter(|stored| {
                    words.is_empty() || {
                        let lower = stored.text.to_lowercase();
                        words.iter().any(|word| lower.contains(word))
                    }
                })
                .take(last as usize)
                .map(|stored| stored.json(false))
                .collect();
            Ok(json!({ "messages": messages }))
        }
        other => Err(bad(format!(
            "chatMessages does not know \"{other}\": use show or list."
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);",
        )
        .unwrap();
        db.execute_batch(include_str!(
            "storage/migrations/0083_chat_message_numbers.sql"
        ))
        .unwrap();
        // Everything in these tests was sent after numbering started.
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, '0', 'x')",
            [SINCE_KEY],
        )
        .unwrap();
        db
    }

    fn user(id: &str, at: i64, text: &str) -> Value {
        json!({
            "id": id, "role": "user", "source": "transcript", "timestamp": at,
            "blocks": [{ "type": "text", "text": text }],
        })
    }

    fn numbers(rows: &[Value]) -> Vec<Option<u64>> {
        rows.iter()
            .map(|row| row.get("messageNumber").and_then(Value::as_u64))
            .collect()
    }

    // `since_ms` is cached for the run, so every test shares one stamp of 0.
    fn reset() {
        known().lock().unwrap().clear();
    }

    #[test]
    fn numbers_the_users_messages_in_order_and_never_twice() {
        reset();
        let db = db();
        let mut rows = vec![
            user("b", 2_000, "second"),
            json!({ "id": "r", "role": "assistant", "source": "transcript", "timestamp": 1_500,
                    "blocks": [{ "type": "text", "text": "reply" }] }),
            user("a", 1_000, "first"),
        ];
        annotate_with(&db, &mut rows, "P1", "G1a");
        assert_eq!(numbers(&rows), vec![Some(2), None, Some(1)]);
        assert_eq!(rows[0]["messageRef"], "G1a.M2");
        // A rewind hides "b"; the next message still gets a new number.
        let mut next = vec![
            user("a", 1_000, "first"),
            user("c", 3_000, "after the rewind"),
        ];
        annotate_with(&db, &mut next, "P1", "G1a");
        assert_eq!(numbers(&next), vec![Some(1), Some(3)]);
    }

    #[test]
    fn leaves_out_queued_rows_and_other_agents_messages() {
        reset();
        let db = db();
        let mut queued = user("q", 1_000, "still in the agent's queue");
        queued["queued"] = json!(true);
        let report = user(
            "x",
            1_100,
            "Done.\n\nMessage from another agent\nAgent: Claude\nSession: Thread\nSession ID: G2b\nReply to: S1:P1:G2b",
        );
        let mut rows = vec![queued, report, user("u", 1_200, "mine")];
        annotate_with(&db, &mut rows, "P1", "G1b");
        assert_eq!(numbers(&rows), vec![None, None, Some(1)]);
    }

    #[test]
    fn a_fork_keeps_the_numbers_it_copied_and_goes_on() {
        reset();
        let db = db();
        let mut source = vec![user("a", 1_000, "one"), user("b", 2_000, "two")];
        annotate_with(&db, &mut source, "P1", "G1c");
        let mut fork = vec![
            user("a2", 1_000, "one"),
            user("b2", 2_000, "two"),
            user("c2", 3_000, "new in the fork"),
        ];
        annotate_with(&db, &mut fork, "P1", "G1d");
        assert_eq!(numbers(&fork), vec![Some(1), Some(2), Some(3)]);
    }

    #[test]
    fn a_reference_quotes_the_start_of_the_message() {
        reset();
        let db = db();
        let mut rows = vec![user(
            "a",
            1_000,
            "make them 30px\nand use the accent colour",
        )];
        annotate_with(&db, &mut rows, "P1", "G1e");
        let text = "same as [M1](message:M1) and #M9";
        let sent = quote_with(&db, "P1", "G1e", text, &referenced_messages(text));
        assert!(sent.starts_with("same as [M1](message:M1) and #M9\n\n<ghostex-system>"));
        assert!(sent.contains("> make them 30px\n> and use the accent colour"));
        assert!(sent.contains("M9: Ghostex has no message with this ID."));
        let shown = handle_chat_messages_request(
            &db,
            json!({ "projectId": "P1", "sessionId": "G1e", "op": "show", "ref": "G1e.M1" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(shown["message"]["ref"], "G1e.M1");
        assert_eq!(
            shown["message"]["text"],
            "make them 30px\nand use the accent colour"
        );
    }
}
