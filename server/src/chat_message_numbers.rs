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
    QuotedMessage, is_own_prompt, message_ref, parse_message_ref, preview, quote_prefix,
    quote_totals, referenced_messages, shown_text, with_quotes,
};
use ghostex_gx_protocol::ChatMessage;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Map, Value, json};

use crate::paths::GxserverPaths;
use crate::storage::open_gxserver_database;

/// The `metadata` key holding when this gxserver started numbering messages (epoch ms).
const SINCE_KEY: &str = "chatMessageNumbersSince";
/// How many pages of a transcript `ghostex message show` reads back to find a message, at
/// [`SHOW_PAGE_ROWS`] rows each.
const SHOW_PAGES: usize = 400;
const SHOW_PAGE_ROWS: usize = 200;
/// `/api/chatMessages`: `ghostex message show` and `list`.
pub(crate) const CHAT_MESSAGES_ENDPOINT: &str = "/api/chatMessages";

static PATHS: OnceLock<GxserverPaths> = OnceLock::new();

/// Lets the frame builders, which have no `AppState`, reach the database. Called once when
/// gxserver starts.
static STATE: OnceLock<std::sync::Arc<crate::server::AppState>> = OnceLock::new();

pub(crate) fn install(state: &std::sync::Arc<crate::server::AppState>) {
    let paths = &state.paths;
    let _ = PATHS.set(paths.clone());
    // Numbering reads a chat's older pages to keep the numbers in the order messages were sent
    // (`older_candidates`).
    let _ = STATE.set(state.clone());
    // The first start of a gxserver that numbers messages is where numbering begins: stamped
    // now, not at the first chat read, which can come after messages were already sent.
    if let Ok(db) = open_gxserver_database(paths) {
        since_ms(&db);
    }
}

/// What is known of one chat's numbers: message id to number.
#[derive(Default)]
struct Known {
    /// Whether `by_id` holds everything the database has for the chat.
    loaded: bool,
    by_id: HashMap<String, u32>,
    /// Rows that will never get a number (another agent's message, a report, a note, a row sent
    /// before numbering began), so each read stops parsing them again. Queued rows are not
    /// kept here: the agent's queue releases them as the user's message.
    skipped: std::collections::HashSet<String>,
}

/// Past this many remembered skipped rows a chat starts the list over rather than grow it.
const SKIPPED_LIMIT: usize = 4_096;

type ChatKey = (String, String);

type Chats = Mutex<HashMap<ChatKey, std::sync::Arc<Mutex<Known>>>>;

/// Every chat whose numbers were read this run, each behind a lock of its own: numbering one chat
/// (a database write that may wait on SQLite) never holds up another chat's reads. The map's own
/// lock is held only to find a chat's.
fn chats() -> &'static Chats {
    static CHATS: OnceLock<Chats> = OnceLock::new();
    CHATS.get_or_init(Default::default)
}

fn chat(key: &ChatKey) -> std::sync::Arc<Mutex<Known>> {
    let mut all = chats()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    all.entry(key.clone()).or_default().clone()
}

/// Runs `work`, which may wait on the database, without holding up the async workers: on the
/// multi-thread runtime it hands the worker's other tasks to another thread for the duration
/// (`block_in_place`); anywhere else (a blocking thread, a test) it simply runs.
fn off_worker<R>(work: impl FnOnce() -> R) -> R {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(work)
        }
        _ => work(),
    }
}

static SINCE: OnceLock<i64> = OnceLock::new();

/// When this gxserver started numbering messages, stamped the first time it is asked.
fn since_ms(db: &Connection) -> i64 {
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

/// Forgets every number of a removed session (`session_id`) or of every session of a removed
/// project (`None`): the words kept for quotes go with their chat. Called beside
/// `forget_session_chat_queue` wherever sessions are deleted.
///
/// CDXC:SessionChat 2026-11-20 WHY:
/// The review of the first build found the kept words outlived deleted sessions and their
/// transcripts. Only the start of each message is kept now (gx-chat-core `quote_prefix`), and it
/// goes when the session does.
pub(crate) fn forget(db: &Connection, project_id: &str, session_id: Option<&str>) {
    let _ = match session_id {
        Some(session_id) => db.execute(
            "DELETE FROM chat_message_numbers WHERE projectId = ?1 AND sessionId = ?2",
            params![project_id, session_id],
        ),
        None => db.execute(
            "DELETE FROM chat_message_numbers WHERE projectId = ?1",
            params![project_id],
        ),
    };
    if let Ok(mut all) = chats().lock() {
        all.retain(|key, _| {
            key.0 != project_id || session_id.is_some_and(|session| key.1 != session)
        });
    }
}

/// The number this chat already gave the message sent at `sent_at` with these words: a resumed
/// conversation that copied it under a new id is the same message.
fn number_in_chat(db: &Connection, key: &ChatKey, sent_at: i64, hash: &str) -> Option<u32> {
    db.query_row(
        "SELECT number FROM chat_message_numbers \
         WHERE projectId = ?1 AND sessionId = ?2 AND sentAt = ?3 AND textHash = ?4",
        params![key.0, key.1, sent_at, hash],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

/// Reads the chat's numbers into `known`, keeping the rows it already knows never count.
fn load_known(db: &Connection, key: &ChatKey, known: &mut Known) {
    let Ok(mut statement) = db.prepare_cached(
        "SELECT messageId, number FROM chat_message_numbers WHERE projectId = ?1 AND sessionId = ?2",
    ) else {
        return;
    };
    let rows = statement.query_map(params![key.0, key.1], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
    });
    for (id, number) in rows.into_iter().flatten().flatten() {
        known.by_id.insert(id, number);
    }
    known.loaded = true;
}

/// Writes the message `candidate` into the chat's numbers in one statement, which takes the next
/// number itself (or `inherited`, a fork's copy's, when this chat has not used it), so two
/// readers, or two gxserver processes, can never give two messages one number. Answers the
/// message's number, also when another reader stored it first.
fn assign(db: &Connection, key: &ChatKey, candidate: &Candidate) -> Option<u32> {
    let hash = text_hash(&candidate.text);
    // The same message again under another id (a resumed conversation's copy).
    if let Some(number) = number_in_chat(db, key, candidate.sent_at, &hash) {
        return Some(number);
    }
    let inherited = copied_number(db, key, candidate.sent_at, &hash);
    let (lines, chars) = quote_totals(&candidate.text);
    let prefix = quote_prefix(&candidate.text);
    for inherited in [inherited, None] {
        let inserted = db
            .query_row(
                "INSERT INTO chat_message_numbers \
                 (projectId, sessionId, number, messageId, sentAt, textHash, quote, lineCount, \
                  charCount, createdAt) \
                 VALUES (?1, ?2, COALESCE( \
                   (SELECT ?3 WHERE ?3 IS NOT NULL AND NOT EXISTS ( \
                      SELECT 1 FROM chat_message_numbers \
                      WHERE projectId = ?1 AND sessionId = ?2 AND number = ?3)), \
                   (SELECT COALESCE(MAX(number), 0) + 1 FROM chat_message_numbers \
                    WHERE projectId = ?1 AND sessionId = ?2)), \
                   ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT DO NOTHING RETURNING number",
                params![
                    key.0,
                    key.1,
                    inherited,
                    candidate.id,
                    candidate.sent_at,
                    hash,
                    prefix,
                    lines,
                    chars,
                    chrono::Utc::now().to_rfc3339()
                ],
                |row| row.get::<_, u32>(0),
            )
            .optional()
            .ok()
            .flatten();
        if inserted.is_some() {
            return inserted;
        }
        // Nothing written: another reader stored this message, or took the number first.
        if let Some(number) = number_in_chat(db, key, candidate.sent_at, &hash) {
            return Some(number);
        }
    }
    None
}

/// A message the user wrote that has no number yet.
#[derive(Clone)]
struct Candidate {
    index: usize,
    id: String,
    sent_at: i64,
    order: (i64, u64),
    text: String,
}

/// The number a fork's copy of a message takes: that of the message its parent chat (the session
/// it was forked or restored from, `restoredFromSessionId`) sent at the same moment with the same
/// words. Only the parent's: two unrelated chats can send the same words in the same millisecond
/// (an automation writing to several), and neither is the other's copy.
fn copied_number(db: &Connection, key: &ChatKey, sent_at: i64, hash: &str) -> Option<u32> {
    let parent: String = db
        .query_row(
            "SELECT restoredFromSessionId FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
            params![key.0, key.1],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .ok()
        .flatten()
        .flatten()
        .filter(|parent| !parent.is_empty() && *parent != key.1)?;
    number_in_chat(db, &(key.0.clone(), parent), sent_at, hash)
}

/// Numbers the user's messages among `messages` that have none yet and writes `messageNumber`
/// and `messageRef` on every one that has a number.
///
/// CDXC:SessionChat 2026-11-20 WHY:
/// The review of the first build found the numbering did its database work on the async workers
/// under one lock for every chat, so one slow write (SQLite's busy wait) held up every other
/// chat's reads and snapshots. A chat whose rows are all known now needs no database at all;
/// otherwise the work runs off the workers (`off_worker`, and `spawn_blocking` in the read route,
/// `annotate_result_async`) under that chat's own lock, and each number is taken in one SQL
/// statement (`assign`).
pub(crate) fn annotate_messages(messages: &mut [Value], project_id: &str, session_id: &str) {
    if project_id.is_empty() || session_id.is_empty() {
        return;
    }
    let any_user = messages
        .iter()
        .any(|message| message.get("role").and_then(Value::as_str) == Some("user"));
    if !any_user {
        return;
    }
    let key: ChatKey = (project_id.to_string(), session_id.to_string());
    let chat = chat(&key);
    // Every row already known: no database. The lock is only tried here, on the caller's thread
    // (an async worker for the follower); waiting for it happens off the workers, below.
    let free = match chat.try_lock() {
        Ok(known) => Some(known),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    };
    if let Some(mut known) = free {
        if let (true, Some(since)) = (known.loaded, SINCE.get().copied()) {
            let (stamped, candidates) = classify(&mut known, messages, since);
            if candidates.is_empty() {
                drop(known);
                stamp(messages, &stamped, session_id);
                return;
            }
        }
    }
    let Some(paths) = PATHS.get() else {
        return;
    };
    let stamped = off_worker(|| {
        let mut known = chat.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let db = open_gxserver_database(paths).ok()?;
        // The transcript is looked up only when older pages are needed.
        let mut pages: Option<Option<Box<PageReader<'static>>>> = None;
        let mut older = |before: Option<u64>| {
            let reader = pages.get_or_insert_with(|| transcript_pages(&key));
            reader.as_mut().and_then(|read| read(before))
        };
        Some(annotate_with(&db, &key, &mut known, messages, &mut older))
    });
    if let Some(stamped) = stamped {
        stamp(messages, &stamped, session_id);
    }
}

/// [`annotate_messages`] on a read answer's or frame's `messages`.
pub(crate) fn annotate_result(result: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    if let Some(messages) = result.get_mut("messages").and_then(Value::as_array_mut) {
        annotate_messages(messages, project_id, session_id);
    }
}

/// [`annotate_result`] from an async handler: the work runs on a blocking thread. A panic in it
/// leaves the rows as they were, unnumbered, rather than losing them.
pub(crate) async fn annotate_result_async(
    result: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let Some(Value::Array(rows)) = result.remove("messages") else {
        return;
    };
    let (project, session) = (project_id.to_string(), session_id.to_string());
    let rows = std::sync::Arc::new(Mutex::new(rows));
    let worked = rows.clone();
    let _ = tokio::task::spawn_blocking(move || {
        let mut rows = worked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        annotate_messages(&mut rows, &project, &session);
    })
    .await;
    let rows = std::mem::take(&mut *rows.lock().unwrap_or_else(|poisoned| poisoned.into_inner()));
    result.insert("messages".to_string(), Value::Array(rows));
}

/// Writes `messageNumber` and `messageRef` on the rows `stamped` names.
fn stamp(messages: &mut [Value], stamped: &[(usize, u32)], session_id: &str) {
    for (index, number) in stamped {
        if let Some(object) = messages[*index].as_object_mut() {
            object.insert("messageNumber".to_string(), json!(number));
            object.insert(
                "messageRef".to_string(),
                json!(message_ref(session_id, *number)),
            );
        }
    }
}

/// Sorts the user rows of `messages` without the database: those with a known number
/// (`stamped`), those that never get one (remembered in `known.skipped`), and the user's own
/// messages still to number (`candidates`, oldest first).
fn classify(
    known: &mut Known,
    messages: &[Value],
    since: i64,
) -> (Vec<(usize, u32)>, Vec<Candidate>) {
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
        if known.skipped.contains(id) {
            continue;
        }
        let queued = message.get("queued").and_then(Value::as_bool) == Some(true);
        let skip = |known: &mut Known| {
            if !queued {
                if known.skipped.len() >= SKIPPED_LIMIT {
                    known.skipped.clear();
                }
                known.skipped.insert(id.to_string());
            }
        };
        let Some(sent_at) = message.get("timestamp").and_then(Value::as_i64) else {
            continue;
        };
        if sent_at < since {
            skip(known);
            continue;
        }
        let Ok(parsed) = serde_json::from_value::<ChatMessage>(message.clone()) else {
            continue;
        };
        if !is_own_prompt(&parsed) {
            skip(known);
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
    (stamped, candidates)
}

/// The database part of [`annotate_messages`]: reads the chat's numbers when they are not loaded
/// yet, then numbers its new messages. Answers the rows to stamp. Blocking.
fn annotate_with(
    db: &Connection,
    key: &ChatKey,
    known: &mut Known,
    messages: &[Value],
    older: &mut PageReader<'_>,
) -> Vec<(usize, u32)> {
    let since = since_ms(db);
    if !known.loaded {
        load_known(db, key, known);
    }
    let (mut stamped, candidates) = classify(known, messages, since);
    if candidates.is_empty() {
        return stamped;
    }
    // The user's messages the rows in hand don't reach, numbered first, so the numbers follow
    // the order the messages were sent whichever page is read first.
    let mut all = older_candidates(known, since, older);
    let mut seen: std::collections::HashSet<String> =
        all.iter().map(|candidate| candidate.id.clone()).collect();
    for candidate in &candidates {
        if seen.insert(candidate.id.clone()) {
            all.push(candidate.clone());
        }
    }
    all.sort_by_key(|candidate| candidate.order);
    for candidate in &all {
        if let Some(number) = assign(db, key, candidate) {
            known.by_id.insert(candidate.id.clone(), number);
        }
    }
    for candidate in candidates {
        if let Some(number) = known.by_id.get(&candidate.id) {
            stamped.push((candidate.index, *number));
        }
    }
    stamped
}

/// How many pages back numbering reads a chat's transcript, at [`OLDER_PAGE_ROWS`] rows each,
/// for messages still without a number.
const OLDER_PAGES: usize = 20;
const OLDER_PAGE_ROWS: usize = 200;

/// Reads one page of a chat's transcript ending before `before` (the newest page for `None`):
/// its rows oldest first, whether older rows remain, and where the page starts.
type PageReader<'a> = dyn FnMut(Option<u64>) -> Option<(Vec<Value>, bool, u64)> + 'a;

/// The user's messages without a number that are older than the rows in hand, read back from
/// the newest page until a numbered message, a message from before numbering began, the start of
/// the chat or [`OLDER_PAGES`].
///
/// CDXC:SessionChat 2026-11-20 WHY:
/// The review of the first build found a chat read from its newest page numbered those messages
/// first, so older messages on a page read later got higher numbers, and messages on pages nobody
/// read got none (`message list` missed them). One long turn of tool rows is enough to push the
/// earlier messages off the newest page.
fn older_candidates(known: &mut Known, since: i64, read: &mut PageReader<'_>) -> Vec<Candidate> {
    let mut found = Vec::new();
    let mut before: Option<u64> = None;
    for _ in 0..OLDER_PAGES {
        let Some((rows, has_more, start)) = read(before) else {
            break;
        };
        let reached = rows.iter().any(|row| {
            row.get("role").and_then(Value::as_str) == Some("user")
                && (row
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| known.by_id.contains_key(id))
                    || row
                        .get("timestamp")
                        .and_then(Value::as_i64)
                        .is_some_and(|at| at < since))
        });
        let (_, candidates) = classify(known, &rows, since);
        found.extend(candidates);
        if reached || !has_more {
            break;
        }
        before = Some(start);
    }
    found
}

/// The production [`PageReader`]: the chat's own transcript, as the chat reads it (fork lineages
/// included). `None` when the chat has no transcript to read. Blocking.
fn transcript_pages(key: &ChatKey) -> Option<Box<PageReader<'static>>> {
    let state = STATE.get()?;
    let (agent, path) = crate::session_chat_read::session_transcript(state, &key.0, &key.1)?;
    Some(Box::new(move |before: Option<u64>| {
        let page = crate::session_chat_fork_stitch::read_session_chat_tail_page_stitched(
            agent,
            &path,
            OLDER_PAGE_ROWS,
            before,
        )
        .ok()?;
        match page.page {
            crate::session_chat::SessionChatTailPage::Page {
                messages,
                has_more,
                before_offset,
                ..
            } => match serde_json::to_value(&messages) {
                Ok(Value::Array(rows)) => Some((rows, has_more, before_offset)),
                _ => None,
            },
            crate::session_chat::SessionChatTailPage::NotFound => None,
        }
    }))
}

/// One stored message: its number, where it is in the transcript, and the start of its words.
struct Stored {
    project_id: String,
    session_id: String,
    number: u32,
    message_id: String,
    sent_at: i64,
    text_hash: String,
    /// The start of its words (gx-chat-core `quote_prefix`).
    quote: String,
    /// The whole message's lines and characters (`quote_totals`).
    totals: (usize, usize),
}

impl Stored {
    fn sent_label(&self) -> Option<String> {
        let at = chrono::DateTime::from_timestamp_millis(self.sent_at)?;
        Some(
            at.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
        )
    }

    fn json(&self, text: Value) -> Value {
        json!({
            "ref": message_ref(&self.session_id, self.number),
            "id": format!("M{}", self.number),
            "number": self.number,
            "projectId": self.project_id,
            "sessionId": self.session_id,
            "sentAt": self.sent_at,
            "sent": self.sent_label(),
            "text": text,
        })
    }
}

const STORED_COLUMNS: &str =
    "projectId, sessionId, number, messageId, sentAt, textHash, quote, lineCount, charCount";

fn read_stored(row: &rusqlite::Row<'_>) -> rusqlite::Result<Stored> {
    Ok(Stored {
        project_id: row.get(0)?,
        session_id: row.get(1)?,
        number: row.get(2)?,
        message_id: row.get(3)?,
        sent_at: row.get(4)?,
        text_hash: row.get(5)?,
        quote: row.get(6)?,
        totals: (row.get(7)?, row.get(8)?),
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
        &format!(
            "SELECT {STORED_COLUMNS} FROM chat_message_numbers \
             WHERE sessionId = ?1 AND number = ?2 AND (?3 IS NULL OR projectId = ?3) LIMIT 1"
        ),
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
    off_worker(|| {
        let Ok(db) = open_gxserver_database(paths) else {
            return text.to_string();
        };
        quote_with(&db, project_id, session_id, text, &references)
    })
}

fn quote_with(
    db: &Connection,
    project_id: &str,
    session_id: &str,
    text: &str,
    references: &[ghostex_gx_chat_core::transcript::message_numbers::MessageRef],
) -> String {
    use ghostex_gx_chat_core::transcript::message_numbers::MAX_QUOTED_MESSAGES;
    // Only the first few are looked up and quoted; the rest are named.
    let (quoted, named) = references.split_at(references.len().min(MAX_QUOTED_MESSAGES));
    let unquoted: Vec<String> = named.iter().map(|reference| reference.written()).collect();
    let quotes: Vec<QuotedMessage> = quoted
        .iter()
        .map(|reference| {
            let stored = match &reference.session_id {
                Some(other) if other != session_id => find(db, None, other, reference.number),
                _ => find(db, Some(project_id), session_id, reference.number),
            };
            QuotedMessage {
                reference: reference.written(),
                sent_at: stored.as_ref().and_then(Stored::sent_label),
                totals: stored.as_ref().map(|stored| stored.totals),
                text: stored.map(|stored| stored.quote),
            }
        })
        .collect();
    with_quotes(text, &quotes, &unquoted)
}

/// The whole of a stored message, read back from its session's transcript: the row with its id,
/// or with its send time and words (a resumed conversation's copy). `None` when the transcript no
/// longer has it. Blocking.
fn whole_text(state: &crate::server::AppState, stored: &Stored) -> Option<String> {
    let (agent, path) = crate::session_chat_read::session_transcript(
        state,
        &stored.project_id,
        &stored.session_id,
    )?;
    let mut before: Option<u64> = None;
    for _ in 0..SHOW_PAGES {
        let page = crate::session_chat_fork_stitch::read_session_chat_tail_page_stitched(
            agent,
            &path,
            SHOW_PAGE_ROWS,
            before,
        )
        .ok()?;
        let crate::session_chat::SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset,
            ..
        } = page.page
        else {
            return None;
        };
        for message in messages.iter().rev() {
            if message.role != crate::session_chat::SessionChatRole::User {
                continue;
            }
            let same_id = message.id == stored.message_id;
            let same_time = message.timestamp == Some(stored.sent_at);
            if !same_id && !same_time {
                continue;
            }
            let Ok(parsed) =
                serde_json::to_value(message).and_then(serde_json::from_value::<ChatMessage>)
            else {
                continue;
            };
            let text = shown_text(&parsed);
            if same_id || text_hash(&text) == stored.text_hash {
                return Some(text);
            }
        }
        // Older than the message: it is not further back.
        if !has_more
            || messages
                .first()
                .and_then(|message| message.timestamp)
                .is_some_and(|at| at < stored.sent_at)
        {
            return None;
        }
        before = Some(before_offset);
    }
    None
}

/// `/api/chatMessages`, off the async workers: it reads the database and, for `show`, the
/// session's transcript.
pub(crate) async fn handle_chat_messages_http(
    state: std::sync::Arc<crate::server::AppState>,
    endpoint_path: String,
    request_id: String,
    body: Value,
) -> crate::server::RoutedResponse {
    let params = match crate::domain::read_domain_rpc_params(&body) {
        Ok(params) => params,
        Err(error) => {
            return crate::server::domain_error_response(endpoint_path, request_id, error);
        }
    };
    let answer = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state.paths).map_err(|error| {
            crate::domain::DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            }
        })?;
        handle_chat_messages_request(Some(&state), &db, &params)
    })
    .await;
    match answer {
        Ok(Ok(value)) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, value),
        ),
        Ok(Err(error)) => crate::server::domain_error_response(endpoint_path, request_id, error),
        Err(error) => crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "internalError",
                message: error.to_string(),
            },
        ),
    }
}

/// `/api/chatMessages {projectId, sessionId, op}`: `show` (`ref`: `M12` or `G9jt7.M12`) prints a
/// message whole (from the transcript; the kept start when the transcript no longer has it, or
/// with no `state`); `list` (`last`, `grep`) the chat's messages with their first line, newest
/// first, `grep` matching the start that is kept.
fn handle_chat_messages_request(
    state: Option<&crate::server::AppState>,
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
                Some(stored) => {
                    let text = match state.and_then(|state| whole_text(state, &stored)) {
                        Some(text) => json!(text),
                        None => json!(format!(
                            "{}\n\n(Only its start: the transcript no longer has this message.)",
                            stored.quote
                        )),
                    };
                    Ok(json!({ "message": stored.json(text) }))
                }
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
                .prepare(&format!(
                    "SELECT {STORED_COLUMNS} FROM chat_message_numbers \
                     WHERE projectId = ?1 AND sessionId = ?2 ORDER BY number DESC"
                ))
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
                        let lower = stored.quote.to_lowercase();
                        words.iter().any(|word| lower.contains(word))
                    }
                })
                .take(last as usize)
                .map(|stored| stored.json(json!(preview(&stored.quote, 120))))
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
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);
             CREATE TABLE sessions (projectId TEXT, sessionId TEXT, restoredFromSessionId TEXT);",
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

    /// [`annotate_messages`] against the test's own database.
    fn number_rows(db: &Connection, rows: &mut [Value], project_id: &str, session_id: &str) {
        let key: ChatKey = (project_id.to_string(), session_id.to_string());
        let held = chat(&key);
        let mut known = held.lock().unwrap();
        let stamped = annotate_with(db, &key, &mut known, rows, &mut |_| None);
        drop(known);
        stamp(rows, &stamped, session_id);
    }

    /// Reading the newest page first still numbers the messages in the order they were sent:
    /// the older page is read back before any number is given.
    #[test]
    fn the_newest_page_first_still_numbers_in_send_order() {
        let db = db();
        let key: ChatKey = ("P1".to_string(), "G1g".to_string());
        let older_page = vec![user("a", 1_000, "one"), user("b", 2_000, "two")];
        let newest_page = vec![user("c", 3_000, "three"), user("d", 4_000, "four")];
        let mut read = |before: Option<u64>| match before {
            None => Some((newest_page.clone(), true, 100)),
            Some(100) => Some((older_page.clone(), false, 0)),
            Some(_) => None,
        };
        let held = chat(&key);
        let mut newest = newest_page.clone();
        let stamped = annotate_with(&db, &key, &mut held.lock().unwrap(), &newest, &mut read);
        stamp(&mut newest, &stamped, &key.1);
        assert_eq!(numbers(&newest), vec![Some(3), Some(4)]);
        let mut older = older_page.clone();
        number_rows(&db, &mut older, "P1", "G1g");
        assert_eq!(numbers(&older), vec![Some(1), Some(2)]);
    }

    /// Two readers numbering the same chat at once never give two messages one number.
    #[test]
    fn a_number_is_taken_in_one_statement() {
        let db = db();
        let key: ChatKey = ("P1".to_string(), "G1f".to_string());
        let candidate = |id: &str, at: i64, text: &str| Candidate {
            index: 0,
            id: id.to_string(),
            sent_at: at,
            order: (at, 0),
            text: text.to_string(),
        };
        // Neither reader has seen the other's message: each asks for the next number.
        assert_eq!(assign(&db, &key, &candidate("a", 1_000, "one")), Some(1));
        assert_eq!(assign(&db, &key, &candidate("b", 2_000, "two")), Some(2));
        // The same message from a second reader gets the number already given.
        assert_eq!(assign(&db, &key, &candidate("a", 1_000, "one")), Some(1));
    }

    fn numbers(rows: &[Value]) -> Vec<Option<u64>> {
        rows.iter()
            .map(|row| row.get("messageNumber").and_then(Value::as_u64))
            .collect()
    }

    // `since_ms` is cached for the run, so every test shares one stamp of 0. Each test uses chats
    // of its own (`G1a`, `G1b`…), so the shared cache never mixes them and nothing clears it
    // while another test runs.

    #[test]
    fn numbers_the_users_messages_in_order_and_never_twice() {
        let db = db();
        let mut rows = vec![
            user("b", 2_000, "second"),
            json!({ "id": "r", "role": "assistant", "source": "transcript", "timestamp": 1_500,
                    "blocks": [{ "type": "text", "text": "reply" }] }),
            user("a", 1_000, "first"),
        ];
        number_rows(&db, &mut rows, "P1", "G1a");
        assert_eq!(numbers(&rows), vec![Some(2), None, Some(1)]);
        assert_eq!(rows[0]["messageRef"], "G1a.M2");
        // A rewind hides "b"; the next message still gets a new number.
        let mut next = vec![
            user("a", 1_000, "first"),
            user("c", 3_000, "after the rewind"),
        ];
        number_rows(&db, &mut next, "P1", "G1a");
        assert_eq!(numbers(&next), vec![Some(1), Some(3)]);
    }

    #[test]
    fn leaves_out_queued_rows_and_other_agents_messages() {
        let db = db();
        let mut queued = user("q", 1_000, "still in the agent's queue");
        queued["queued"] = json!(true);
        let report = user(
            "x",
            1_100,
            "Done.\n\nMessage from another agent\nAgent: Claude\nSession: Thread\nSession ID: G2b\nReply to: S1:P1:G2b",
        );
        let mut rows = vec![queued, report, user("u", 1_200, "mine")];
        number_rows(&db, &mut rows, "P1", "G1b");
        assert_eq!(numbers(&rows), vec![None, None, Some(1)]);
        // The report is remembered as never numbered; the queued row is looked at again.
        let held = chat(&("P1".to_string(), "G1b".to_string()));
        let known = held.lock().unwrap();
        assert!(known.skipped.contains("x"));
        assert!(!known.skipped.contains("q"));
    }

    #[test]
    fn a_fork_keeps_the_numbers_it_copied_and_goes_on() {
        let db = db();
        db.execute(
            "INSERT INTO sessions (projectId, sessionId, restoredFromSessionId) VALUES ('P1', 'G1d', 'G1c')",
            [],
        )
        .unwrap();
        let mut source = vec![
            user("z", 500, "earlier"),
            user("a", 1_000, "one"),
            user("b", 2_000, "two"),
        ];
        number_rows(&db, &mut source, "P1", "G1c");
        let mut fork = vec![
            user("a2", 1_000, "one"),
            user("b2", 2_000, "two"),
            user("c2", 3_000, "new in the fork"),
        ];
        number_rows(&db, &mut fork, "P1", "G1d");
        assert_eq!(numbers(&fork), vec![Some(2), Some(3), Some(4)]);
        // An unrelated chat that sent the same words at the same moment is nobody's copy.
        let mut other = vec![user("a3", 2_000, "two")];
        number_rows(&db, &mut other, "P1", "G1y");
        assert_eq!(numbers(&other), vec![Some(1)]);
    }

    #[test]
    fn a_reference_quotes_the_start_of_the_message() {
        let db = db();
        let mut rows = vec![user(
            "a",
            1_000,
            "make them 30px\nand use the accent colour",
        )];
        number_rows(&db, &mut rows, "P1", "G1e");
        let text = "same as [M1](message:M1) and #M9";
        let sent = quote_with(&db, "P1", "G1e", text, &referenced_messages(text));
        assert!(sent.starts_with("same as [M1](message:M1) and #M9\n\n<ghostex-system>"));
        assert!(sent.contains("> make them 30px\n> and use the accent colour"));
        assert!(sent.contains("M9: Ghostex has no message with this ID."));
        // With no transcript to read, `show` gives the start it keeps and says so.
        let shown = handle_chat_messages_request(
            None,
            &db,
            json!({ "projectId": "P1", "sessionId": "G1e", "op": "show", "ref": "G1e.M1" })
                .as_object()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(shown["message"]["ref"], "G1e.M1");
        assert_eq!(
            shown["message"]["text"],
            "make them 30px\nand use the accent colour\n\n(Only its start: the transcript no longer has this message.)"
        );
        // Only the start of a long message is kept, with its size for the quote's note.
        let long = format!("{}end", "a line\n".repeat(400));
        let mut rows = vec![user("b", 2_000, &long)];
        number_rows(&db, &mut rows, "P1", "G1e");
        let (kept, lines): (String, i64) = db
            .query_row(
                "SELECT quote, lineCount FROM chat_message_numbers WHERE sessionId = 'G1e' AND number = 2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(kept.chars().count(), 1_000);
        assert_eq!(lines, 401);
        let text = "#M2";
        let sent = quote_with(&db, "P1", "G1e", text, &referenced_messages(text));
        assert!(sent.contains("(the first 6 of 401 lines;"));
        // A resumed conversation's copy of a message (new id, same time and words) keeps its number.
        let mut resumed = vec![user("b-copy", 2_000, &long)];
        number_rows(&db, &mut resumed, "P1", "G1e");
        assert_eq!(numbers(&resumed), vec![Some(2)]);
        // The numbers go with their session.
        forget(&db, "P1", Some("G1e"));
        assert!(find(&db, Some("P1"), "G1e", 1).is_none());
    }
}
