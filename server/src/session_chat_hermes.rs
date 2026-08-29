/*
Hermes Agent has no per-session transcript file: every conversation lives in
the `messages` table of `~/.hermes/state.db` (rollback-journal SQLite, written
row-by-row as the turn progresses, with monotonic AUTOINCREMENT ids). The chat
pipeline is built around tailing an append-only jsonl file, so this module
materializes one: each Hermes session's active rows are mirrored into
`<gxserver state dir>/hermes-chat-mirror/<agent-session-id>.jsonl`, one JSON
object per row in the shape `decode_hermes_transcript_line` reads.

Sync runs at the two places every consumer already passes through:
`resolve_session_chat_transcript_path` (follower resolve/staleness polls, HTTP
long-poll reads, queue runtime, export, watchdog) and the follower's
`follower_drain_once` steady-state tick. New rows append; a rewind or
deactivation (Hermes `/undo` flips `active` off on old rows) rewrites the
mirror atomically via rename, which the follower's inode-identity check reads
as `content_replaced` and answers with a fresh snapshot.

The mirror file's stem IS the Hermes session id, which is what lets the drain
hook re-derive the session from the path alone.
*/

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};

use crate::session_chat_paths::configured_agent_directory;

const HERMES_SESSION_ID_MAX_LENGTH: usize = 128;
const HERMES_STATE_DB_BUSY_TIMEOUT_MS: u64 = 250;

pub(crate) fn is_safe_hermes_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= HERMES_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

pub(crate) fn hermes_state_db_path() -> PathBuf {
    configured_agent_directory("HERMES_HOME", ".hermes").join("state.db")
}

fn hermes_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("hermes-chat-mirror")
}

/// Where the sync left off for one session: the newest mirrored row id and how
/// many active rows the mirror holds. `active_count` is what detects a rewind:
/// appends grow it in lockstep with new ids, deactivation shrinks it.
#[derive(Clone, Copy, Default)]
struct HermesMirrorCursor {
    last_row_id: i64,
    active_count: i64,
}

static MIRROR_CURSORS: Mutex<Option<HashMap<String, HermesMirrorCursor>>> = Mutex::new(None);

struct HermesMessageRow {
    row_id: i64,
    role: String,
    content: Option<String>,
    tool_calls: Option<String>,
    tool_name: Option<String>,
    tool_call_id: Option<String>,
    timestamp: f64,
    finish_reason: Option<String>,
    reasoning: Option<String>,
    reasoning_content: Option<String>,
}

fn hermes_row_json_line(row: &HermesMessageRow) -> String {
    let mut record = json!({
        "rowId": row.row_id,
        "role": row.role,
        "timestamp": row.timestamp,
    });
    let object = record.as_object_mut().expect("literal object");
    if let Some(content) = &row.content {
        object.insert("content".into(), Value::String(content.clone()));
    }
    if let Some(tool_calls) = &row.tool_calls {
        let parsed = serde_json::from_str::<Value>(tool_calls)
            .unwrap_or_else(|_| Value::String(tool_calls.clone()));
        object.insert("toolCalls".into(), parsed);
    }
    if let Some(tool_name) = &row.tool_name {
        object.insert("toolName".into(), Value::String(tool_name.clone()));
    }
    if let Some(tool_call_id) = &row.tool_call_id {
        object.insert("toolCallId".into(), Value::String(tool_call_id.clone()));
    }
    if let Some(finish_reason) = &row.finish_reason {
        object.insert("finishReason".into(), Value::String(finish_reason.clone()));
    }
    if let Some(reasoning) = &row.reasoning {
        object.insert("reasoning".into(), Value::String(reasoning.clone()));
    }
    if let Some(reasoning_content) = &row.reasoning_content {
        object.insert(
            "reasoningContent".into(),
            Value::String(reasoning_content.clone()),
        );
    }
    let mut line = record.to_string();
    line.push('\n');
    line
}

fn open_hermes_state_db() -> Option<Connection> {
    let db_path = hermes_state_db_path();
    if !db_path.is_file() {
        return None;
    }
    let connection = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .busy_timeout(std::time::Duration::from_millis(
            HERMES_STATE_DB_BUSY_TIMEOUT_MS,
        ))
        .ok()?;
    Some(connection)
}

/*
The name the agent gave one session, plus the provenance it recorded for it
(`derived` for the instant name taken from the opening message, `llm` for the
model's upgrade of it, `user` for a name typed with `/title`). The provenance
travels with the title because it is the only thing that distinguishes the
throwaway first name from the one meant to stick.
*/
pub(crate) struct HermesSessionTitle {
    pub(crate) title: String,
    pub(crate) title_source: Option<String>,
}

/// The current title for one session, or `None` while it has no name yet.
pub(crate) fn read_hermes_session_title(session_id: &str) -> Option<HermesSessionTitle> {
    if !is_safe_hermes_session_id(session_id) {
        return None;
    }
    let connection = open_hermes_state_db()?;
    let (title, title_source) = connection
        .query_row(
            "SELECT title, title_source FROM sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .ok()?;
    let title = title?.trim().to_string();
    (!title.is_empty()).then_some(HermesSessionTitle {
        title,
        title_source,
    })
}

fn read_hermes_active_rows(
    connection: &Connection,
    session_id: &str,
    after_row_id: i64,
) -> rusqlite::Result<Vec<HermesMessageRow>> {
    let mut statement = connection.prepare(
        "SELECT id, role, content, tool_calls, tool_name, tool_call_id, timestamp, \
                finish_reason, reasoning, reasoning_content \
         FROM messages WHERE session_id = ?1 AND active != 0 AND id > ?2 ORDER BY id",
    )?;
    let rows = statement.query_map(rusqlite::params![session_id, after_row_id], |row| {
        Ok(HermesMessageRow {
            row_id: row.get(0)?,
            role: row.get(1)?,
            content: row.get(2)?,
            tool_calls: row.get(3)?,
            tool_name: row.get(4)?,
            tool_call_id: row.get(5)?,
            timestamp: row.get(6)?,
            finish_reason: row.get(7)?,
            reasoning: row.get(8)?,
            reasoning_content: row.get(9)?,
        })
    })?;
    rows.collect()
}

/// One sync pass for one session. Returns the mirror path once the session has
/// at least one active row; `None` before the first prompt (the follower keeps
/// polling with status "starting", exactly as for an agent whose transcript
/// file has not appeared yet).
fn sync_hermes_transcript_mirror(session_id: &str) -> Option<PathBuf> {
    if !is_safe_hermes_session_id(session_id) {
        return None;
    }
    let mirror_path = hermes_mirror_dir().join(format!("{session_id}.jsonl"));
    let connection = open_hermes_state_db()?;
    let (active_count, max_row_id) = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(MAX(id), 0) FROM messages \
             WHERE session_id = ?1 AND active != 0",
            rusqlite::params![session_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok()?;
    if active_count == 0 {
        return None;
    }

    let mut cursors_guard = MIRROR_CURSORS.lock().ok()?;
    let cursors = cursors_guard.get_or_insert_with(HashMap::new);
    // A cursor only counts when the file it describes is still there; a wiped
    // state dir or fresh daemon always rebuilds.
    let cursor = cursors
        .get(session_id)
        .copied()
        .filter(|_| mirror_path.is_file());

    let up_to_date = cursor.is_some_and(|cursor| {
        cursor.last_row_id == max_row_id && cursor.active_count == active_count
    });
    if up_to_date {
        return Some(mirror_path);
    }

    let appended_rows = match cursor {
        Some(cursor) if max_row_id > cursor.last_row_id => {
            read_hermes_active_rows(&connection, session_id, cursor.last_row_id).ok()?
        }
        _ => Vec::new(),
    };
    let pure_append = cursor
        .is_some_and(|cursor| active_count == cursor.active_count + appended_rows.len() as i64);

    fs::create_dir_all(mirror_path.parent()?).ok()?;
    if pure_append {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&mirror_path)
            .ok()?;
        for row in &appended_rows {
            file.write_all(hermes_row_json_line(row).as_bytes()).ok()?;
        }
    } else {
        // Cold start, rewind, or deactivation: rebuild the whole mirror and
        // swap it in by rename so no reader ever sees a torn file. The new
        // inode is what tells the follower the content was replaced.
        let rows = read_hermes_active_rows(&connection, session_id, 0).ok()?;
        let temp_path = mirror_path.with_extension("jsonl.tmp");
        let mut file = fs::File::create(&temp_path).ok()?;
        for row in &rows {
            file.write_all(hermes_row_json_line(row).as_bytes()).ok()?;
        }
        file.flush().ok()?;
        drop(file);
        fs::rename(&temp_path, &mirror_path).ok()?;
    }
    cursors.insert(
        session_id.to_string(),
        HermesMirrorCursor {
            last_row_id: max_row_id,
            active_count,
        },
    );
    Some(mirror_path)
}

/// Path-resolution entry: sync, then hand back the mirror as "the transcript".
pub(crate) fn resolve_hermes_chat_transcript_path(session_id: &str) -> Option<PathBuf> {
    sync_hermes_transcript_mirror(session_id)
}

/// Steady-state entry for the follower's drain tick, which holds only the
/// already-resolved path. The stem is the Hermes session id by construction.
pub(crate) fn sync_hermes_transcript_mirror_for_path(mirror_path: &Path) {
    if let Some(session_id) = mirror_path.file_stem().and_then(|stem| stem.to_str()) {
        sync_hermes_transcript_mirror(session_id);
    }
}
