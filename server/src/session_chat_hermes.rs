/*
Hermes Agent has no per-session transcript file: every conversation lives in
the `messages` table of its session store (`~/.hermes/state.db`, or
`profiles/<name>/state.db` for a named profile; rollback-journal SQLite, written
row-by-row as the turn progresses, with monotonic AUTOINCREMENT ids). The chat
pipeline is built around tailing an append-only jsonl file, so this module
materializes one: each Hermes session's display history is mirrored into
`<gxserver state dir>/hermes-chat-mirror/<agent-session-id>.jsonl`, one JSON
object per logical message in the shape `decode_hermes_transcript_line` reads.

Sync runs at the two places every consumer already passes through:
`resolve_session_chat_transcript_path` (follower resolve/staleness polls, HTTP
long-poll reads, queue runtime, export, watchdog) and the follower's
`follower_drain_once` steady-state tick. New messages append; a rewind (Hermes
`/undo` flips `active` off on old rows) rewrites the mirror atomically via
rename, which the follower's inode-identity check reads as `content_replaced`
and answers with a fresh snapshot.

The mirror's own record shape adds a `{"role": "compaction"}` marker where each
compaction happened, and its timestamps never go backwards in file order.

The mirror file's stem IS the Hermes session id, which is what lets the drain
hook re-derive the session from the path alone.
*/

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
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

pub(crate) fn hermes_home() -> PathBuf {
    configured_agent_directory("HERMES_HOME", ".hermes")
}

/// Every Hermes session store: the default profile's, then each named profile's.
pub(crate) fn hermes_state_db_paths(hermes_home: &Path) -> Vec<PathBuf> {
    let mut profile_paths = fs::read_dir(hermes_home.join("profiles"))
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path().join("state.db"))
                .filter(|path| path.is_file())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    profile_paths.sort();
    let mut paths = vec![hermes_home.join("state.db")];
    paths.extend(profile_paths);
    paths
}

/// Where each session id was found, or `None` with when it was last looked for in vain.
static SESSION_STORES: Mutex<Option<HashMap<String, (Option<PathBuf>, Instant)>>> =
    Mutex::new(None);

/// How long a session id no store holds yet is answered with the root store without a rescan.
const SESSION_STORE_MISS_TTL: Duration = Duration::from_secs(1);

/// CDXC:Bots 2026-09-26 WHY:
/// Each Hermes profile keeps its sessions in its own `profiles/<name>/state.db`, so a `hermes -p harry` session never reaches the root store. The store is found by session id, not parsed from the launch command, because chat reads arrive with only the id; ids are unique across profiles, and a hit is cached because a session never moves. A miss is cached for a second: Hermes writes the row only on the first turn, and until then the title, detect and follower passes would each rescan every store several times a second.
pub(crate) fn hermes_state_db_path(hermes_home: &Path, session_id: &str) -> PathBuf {
    hermes_session_store(hermes_home, session_id).unwrap_or_else(|| hermes_home.join("state.db"))
}

/// The store that holds this session, or `None` while no store does (Hermes writes the row on the
/// first turn). [`hermes_state_db_path`] answers a miss with the root store.
pub(crate) fn hermes_session_store(hermes_home: &Path, session_id: &str) -> Option<PathBuf> {
    let cached = SESSION_STORES
        .lock()
        .ok()
        .and_then(|stores| stores.as_ref()?.get(session_id).cloned());
    match cached {
        Some((Some(path), _)) if path.is_file() => return Some(path),
        Some((None, looked_at)) if looked_at.elapsed() < SESSION_STORE_MISS_TTL => return None,
        _ => {}
    }
    let found = hermes_state_db_paths(hermes_home).into_iter().find(|path| {
        open_read_only_state_db(path).is_some_and(|connection| {
            connection
                .query_row(
                    "SELECT 1 FROM sessions WHERE id = ?1",
                    rusqlite::params![session_id],
                    |_| Ok(()),
                )
                .is_ok()
        })
    });
    if let Ok(mut stores) = SESSION_STORES.lock() {
        stores
            .get_or_insert_with(HashMap::new)
            .insert(session_id.to_string(), (found.clone(), Instant::now()));
    }
    found
}

fn hermes_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("hermes-chat-mirror")
}

/// Where the sync left off for one session: the newest row id read and how
/// many visible rows the store held then, the messages already written and the
/// newest timestamp written. `visible_count` is what detects a rewind or an
/// in-place compaction: appends grow it in lockstep with new ids, while `/undo`
/// and a compaction hide rows.
#[derive(Default)]
struct HermesMirrorCursor {
    last_row_id: i64,
    visible_count: i64,
    last_local_bytes: u64,
    written: HashSet<HermesMirrorKey>,
    last_timestamp: f64,
}

impl HermesMirrorCursor {
    /// CDXC:SessionChat 2026-09-28 WHY:
    /// Hermes compaction keeps the conversation in its session store but moves it around. In place (the default) it archives older rows (`active = 0, compacted = 1`), hides the recent rows it carries forward (`active = 0, compacted = 0`, the same flags `/undo` uses) and re-inserts them after the summary, sometimes with the summary folded into the first one and tool output pruned ("[read_file] read … (2,781 chars)"). With `compression.in_place: false` it instead ends the session (`end_reason = 'compression'`) and continues in a child session whose first rows are copies. A chat that read only the live rows of the session it follows lost every earlier prompt and reply the moment a busy session compacted, while the terminal still showed them (Dobby session G26an lost four of Sven's five prompts). So every logical message that still has a live or archived copy is written once, as its first row, even when that row is hidden: it is what the terminal printed, where it printed it. A copy keeps its role and timestamp and a tool result its call id, while pruning and a folded summary change everything else, so those three identify it. Timestamps never go backwards because chat orders by timestamp and Hermes by row id: a /steer message is stamped 0.6 ms after the reply it precedes, so chat drew it below the answer while the terminal printed "⏩ Steered" above it.
    fn render_rows(&mut self, rows: &[HermesMessageRow]) -> String {
        let keys: Vec<u64> = rows.iter().map(hermes_message_key).collect();
        let shown: HashSet<u64> = if rows.iter().all(|row| row.visible) {
            HashSet::new()
        } else {
            rows.iter()
                .zip(&keys)
                .filter(|(row, _)| row.visible)
                .map(|(_, key)| *key)
                .collect()
        };
        let mut rendered = String::new();
        for (row, key) in rows.iter().zip(keys) {
            let mirror_key = match (row.compaction_summary, row.visible) {
                (true, true) => HermesMirrorKey::Compaction(row.row_id),
                (true, false) => continue,
                (false, visible) if visible || shown.contains(&key) => {
                    HermesMirrorKey::Message(key)
                }
                (false, _) => continue,
            };
            if !self.written.insert(mirror_key) {
                continue;
            }
            self.last_timestamp = self.last_timestamp.max(row.timestamp);
            if row.compaction_summary {
                let marker = json!({
                    "rowId": row.row_id,
                    "role": "compaction",
                    "timestamp": self.last_timestamp,
                });
                rendered.push_str(&format!("{marker}\n"));
            } else {
                rendered.push_str(&hermes_row_json_line(row, self.last_timestamp));
            }
        }
        rendered
    }
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
    message_items: Option<String>,
    display_kind: Option<String>,
    display_text: Option<String>,
    compaction_summary: bool,
    visible: bool,
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum HermesMirrorKey {
    Message(u64),
    Compaction(i64),
}

fn hermes_message_key(row: &HermesMessageRow) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (&row.role, row.timestamp.to_bits(), &row.tool_call_id).hash(&mut hasher);
    hasher.finish()
}

/// CDXC:SessionChat 2026-09-27 WHY:
/// The OpenAI Responses models Hermes runs (Dobby's gpt-6-sol) store a tool-calling step's message ("Using beads-board to claim the card…") only in `codex_message_items`, as `commentary`, and leave `content` empty, so chat showed tool rows and no messages while Discord showed every one. A final answer repeats its text in both columns, so the items are read only when `content` is empty.
fn hermes_row_text(row: &HermesMessageRow) -> Option<String> {
    if let Some(content) = row.content.as_ref().filter(|text| !text.trim().is_empty()) {
        return Some(content.clone());
    }
    let items = serde_json::from_str::<Value>(row.message_items.as_deref()?).ok()?;
    let text = items
        .as_array()?
        .iter()
        .filter(|item| item["type"] == "message")
        .filter_map(|item| item["content"].as_array())
        .flatten()
        .filter(|part| part["type"] == "output_text")
        .filter_map(|part| part["text"].as_str())
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    (!text.is_empty()).then_some(text)
}

fn hermes_row_json_line(row: &HermesMessageRow, timestamp: f64) -> String {
    let mut record = json!({
        "rowId": row.row_id,
        "role": row.role,
        "timestamp": timestamp,
    });
    let object = record.as_object_mut().expect("literal object");
    if let Some(content) = hermes_row_text(row).or_else(|| row.content.clone()) {
        object.insert("content".into(), Value::String(content));
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
    if let Some(display_kind) = &row.display_kind {
        object.insert("displayKind".into(), Value::String(display_kind.clone()));
    }
    if let Some(display_text) = &row.display_text {
        object.insert("displayText".into(), Value::String(display_text.clone()));
    }
    let mut line = record.to_string();
    line.push('\n');
    line
}

pub(crate) fn open_hermes_state_db(session_id: &str) -> Option<Connection> {
    open_read_only_state_db(&hermes_state_db_path(&hermes_home(), session_id))
}

pub(crate) fn open_read_only_state_db(db_path: &Path) -> Option<Connection> {
    if !db_path.is_file() {
        return None;
    }
    let connection = ghostex_find::agent_store::open_agent_store(db_path).ok()?;
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
pub(crate) fn read_hermes_session_title(
    state_db_path: &Path,
    session_id: &str,
) -> Option<HermesSessionTitle> {
    if !is_safe_hermes_session_id(session_id) {
        return None;
    }
    let connection = open_read_only_state_db(state_db_path)?;
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

/// The session (`?1`) and the sessions it continues after a compaction that rotated it. A parent
/// that ended any other way (a subagent's parent, a reset) is a different conversation.
const HERMES_LINEAGE_CTE: &str = "WITH RECURSIVE lineage(id) AS (SELECT ?1 UNION \
     SELECT parent.id FROM lineage \
     JOIN sessions AS child ON child.id = lineage.id \
     JOIN sessions AS parent ON parent.id = child.parent_session_id \
     AND parent.end_reason = 'compression') ";

/// The mirror's two reads for one store: the visible rows' count and newest id, and the rows
/// after `?2` (only visible ones while `?3` is 1; hidden ones too for a rebuild).
struct HermesMirrorQueries {
    count: String,
    rows: String,
}

/// Built from the store's own columns: older Hermes stores lack the newer ones.
fn hermes_mirror_queries(connection: &Connection) -> Option<HermesMirrorQueries> {
    let mut statement = connection
        .prepare("SELECT name FROM pragma_table_info('messages')")
        .ok()?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?
        .collect::<rusqlite::Result<HashSet<String>>>()
        .ok()?;
    let column = |name: &'static str, fallback: &'static str| {
        if columns.contains(name) {
            name
        } else {
            fallback
        }
    };
    // Hermes' own guard: a malformed metadata value reads as no metadata instead of failing the query.
    let metadata = |path: &str| {
        format!(
            "json_extract(CASE WHEN json_valid(display_metadata) \
             THEN display_metadata ELSE '{{}}' END, '{path}')"
        )
    };
    let visible = if columns.contains("compacted") {
        "(active = 1 OR compacted = 1)"
    } else {
        "active != 0"
    };
    let in_session = "session_id IN (SELECT id FROM lineage)";
    let (display_text, shown) = if columns.contains("display_metadata") {
        let model_only = metadata("$.model_only");
        (
            metadata("$.display_text"),
            format!("COALESCE({model_only}, 0) = 0"),
        )
    } else {
        ("NULL".to_string(), "1".to_string())
    };
    Some(HermesMirrorQueries {
        // Change detection only, so it skips the per-row `model_only` check: a model-only row costs one rebuild.
        count: format!(
            "{HERMES_LINEAGE_CTE}SELECT COUNT(*), COALESCE(MAX(id), 0) FROM messages \
             WHERE {in_session} AND {visible}"
        ),
        rows: format!(
            "{HERMES_LINEAGE_CTE}SELECT id, role, content, tool_calls, tool_name, tool_call_id, \
                    timestamp, finish_reason, reasoning, reasoning_content, {}, {}, \
                    {display_text}, {}, {visible} \
             FROM messages WHERE {in_session} AND {shown} AND id > ?2 AND ({visible} OR ?3 = 0) \
             ORDER BY id",
            column("codex_message_items", "NULL"),
            column("display_kind", "NULL"),
            column("_compressed_summary", "0"),
        ),
    })
}

fn read_hermes_rows(
    connection: &Connection,
    queries: &HermesMirrorQueries,
    session_id: &str,
    after_row_id: i64,
    visible_only: bool,
) -> rusqlite::Result<Vec<HermesMessageRow>> {
    let mut statement = connection.prepare(&queries.rows)?;
    let params = rusqlite::params![session_id, after_row_id, visible_only];
    let rows = statement.query_map(params, |row| {
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
            message_items: row.get(10)?,
            display_kind: row.get(11)?,
            display_text: row.get(12)?,
            compaction_summary: row.get::<_, Option<i64>>(13)?.is_some_and(|flag| flag != 0),
            visible: row.get(14)?,
        })
    })?;
    rows.collect()
}

fn hermes_local_mirror_path(session_id: &str) -> PathBuf {
    hermes_mirror_dir().join(format!("{session_id}.local.jsonl"))
}

fn read_hermes_local_rows(local_path: &Path) -> Vec<HermesMessageRow> {
    let Ok(raw) = fs::read_to_string(local_path) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(row_id) = val.get("rowId").and_then(Value::as_i64) else {
            continue;
        };
        let Some(role) = val.get("role").and_then(Value::as_str) else {
            continue;
        };
        let Some(timestamp) = val.get("timestamp").and_then(Value::as_f64) else {
            continue;
        };
        let content = val
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_string);
        rows.push(HermesMessageRow {
            row_id,
            role: role.to_string(),
            content,
            tool_calls: None,
            tool_name: None,
            tool_call_id: None,
            timestamp,
            finish_reason: None,
            reasoning: None,
            reasoning_content: None,
            message_items: None,
            display_kind: None,
            display_text: None,
            compaction_summary: false,
            visible: true,
        });
    }
    rows
}

fn merge_hermes_rows(
    db_rows: Vec<HermesMessageRow>,
    local_rows: Vec<HermesMessageRow>,
) -> Vec<HermesMessageRow> {
    if local_rows.is_empty() {
        return db_rows;
    }
    if db_rows.is_empty() {
        return local_rows;
    }
    let mut tagged: Vec<(f64, u8, usize, HermesMessageRow)> =
        Vec::with_capacity(db_rows.len() + local_rows.len());
    for (idx, row) in db_rows.into_iter().enumerate() {
        tagged.push((row.timestamp, 0, idx, row));
    }
    for (idx, row) in local_rows.into_iter().enumerate() {
        tagged.push((row.timestamp, 1, idx, row));
    }
    tagged.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.cmp(&b.2))
    });
    tagged.into_iter().map(|(_, _, _, row)| row).collect()
}

/// Record a Hermes Native local slash-command exchange (`/status`, `/help`, `/version`,
/// `/tools`, `/usage`, `/context`, etc.) that `hermes acp` handled locally by streaming
/// `agent_message_chunk` without writing rows to `state.db`.
pub(crate) fn append_native_slash_exchange(session_id: &str, prompt: &str, response: &str) {
    let prompt = prompt.trim();
    let response = response.trim();
    if !is_safe_hermes_session_id(session_id) || prompt.is_empty() || response.is_empty() {
        return;
    }
    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    // Skip if Hermes already persisted this exact prompt to `state.db` recently.
    if let Some(connection) = open_hermes_state_db(session_id) {
        let already_in_db = connection
            .query_row(
                "SELECT content, timestamp FROM messages \
                 WHERE session_id = ?1 AND role = 'user' \
                 ORDER BY id DESC LIMIT 1",
                rusqlite::params![session_id],
                |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, f64>(1)?)),
            )
            .ok()
            .is_some_and(|(content, ts)| {
                content.as_deref().map(str::trim) == Some(prompt) && (now_secs - ts).abs() <= 15.0
            });
        if already_in_db {
            return;
        }
    }
    let local_path = hermes_local_mirror_path(session_id);
    let Some(parent) = local_path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let now_ms = (now_secs * 1000.0) as i64;
    let user_row = json!({
        "rowId": -(now_ms.saturating_mul(2)),
        "role": "user",
        "timestamp": now_secs,
        "content": prompt,
    });
    let assistant_row = json!({
        "rowId": -(now_ms.saturating_mul(2).saturating_add(1)),
        "role": "assistant",
        "timestamp": now_secs + 0.001,
        "content": response,
    });
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&local_path)
    {
        let _ = writeln!(file, "{user_row}");
        let _ = writeln!(file, "{assistant_row}");
        let _ = file.flush();
    }
    let _ = sync_hermes_transcript_mirror(session_id);
}

/// One sync pass for one session. Returns the mirror path once the session has
/// at least one visible row; `None` before the first prompt (the follower keeps
/// polling with status "starting", exactly as for an agent whose transcript
/// file has not appeared yet).
fn sync_hermes_transcript_mirror(session_id: &str) -> Option<PathBuf> {
    if !is_safe_hermes_session_id(session_id) {
        return None;
    }
    let mirror_path = hermes_mirror_dir().join(format!("{session_id}.jsonl"));
    let local_path = hermes_local_mirror_path(session_id);
    let local_bytes = fs::metadata(&local_path).map(|m| m.len()).unwrap_or(0);
    let db_state = open_hermes_state_db(session_id).and_then(|connection| {
        let queries = hermes_mirror_queries(&connection)?;
        let (visible_count, max_row_id) = connection
            .query_row(&queries.count, rusqlite::params![session_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })
            .ok()?;
        Some((connection, queries, visible_count, max_row_id))
    });
    let (visible_count, max_row_id) = db_state
        .as_ref()
        .map(|(_, _, v, m)| (*v, *m))
        .unwrap_or((0, 0));
    if visible_count == 0 && local_bytes == 0 {
        return None;
    }

    let mut cursors_guard = MIRROR_CURSORS.lock().ok()?;
    let cursors = cursors_guard.get_or_insert_with(HashMap::new);
    // A cursor only counts when the file it describes is still there; a wiped
    // state dir or fresh daemon always rebuilds.
    let mirror_exists = mirror_path.is_file();
    let up_to_date = mirror_exists
        && cursors.get(session_id).is_some_and(|cursor| {
            cursor.last_row_id == max_row_id
                && cursor.visible_count == visible_count
                && cursor.last_local_bytes == local_bytes
        });
    if up_to_date {
        return Some(mirror_path);
    }
    // Taken out until this pass succeeds, so a failed write leaves the next pass a rebuild.
    let cursor = cursors.remove(session_id).filter(|_| mirror_exists);

    let appended_rows = match (&cursor, &db_state) {
        (Some(cursor), Some((connection, queries, _, _)))
            if max_row_id > cursor.last_row_id && cursor.last_local_bytes == local_bytes =>
        {
            read_hermes_rows(connection, queries, session_id, cursor.last_row_id, true).ok()?
        }
        _ => Vec::new(),
    };
    let pure_append = cursor.as_ref().is_some_and(|cursor| {
        cursor.last_local_bytes == local_bytes
            && visible_count == cursor.visible_count + appended_rows.len() as i64
    });
    let mut cursor = cursor.filter(|_| pure_append).unwrap_or_default();

    fs::create_dir_all(mirror_path.parent()?).ok()?;
    let (written_max_row_id, written_visible_count) = if pure_append {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&mirror_path)
            .ok()?;
        file.write_all(cursor.render_rows(&appended_rows).as_bytes())
            .ok()?;
        let new_max = appended_rows
            .iter()
            .map(|row| row.row_id)
            .max()
            .unwrap_or(cursor.last_row_id);
        let new_visible = cursor.visible_count + appended_rows.len() as i64;
        (new_max, new_visible)
    } else {
        // Cold start, rewind, local slash command or in-place compaction: rebuild the whole mirror
        // from every row, hidden ones included (see `render_rows`), and swap it
        // in by rename so no reader ever sees a torn file. The new inode is what
        // tells the follower the content was replaced.
        let db_rows = match &db_state {
            Some((connection, queries, v, _)) if *v > 0 => {
                read_hermes_rows(connection, queries, session_id, 0, false).ok()?
            }
            _ => Vec::new(),
        };
        let new_max = db_rows
            .iter()
            .filter(|row| row.visible)
            .map(|row| row.row_id)
            .max()
            .unwrap_or(0);
        let new_visible = db_rows.iter().filter(|row| row.visible).count() as i64;
        let local_rows = if local_bytes > 0 {
            read_hermes_local_rows(&local_path)
        } else {
            Vec::new()
        };
        let rows = merge_hermes_rows(db_rows, local_rows);
        let temp_path = mirror_path.with_extension(format!("jsonl.{}.tmp", std::process::id()));
        let mut file = fs::File::create(&temp_path).ok()?;
        file.write_all(cursor.render_rows(&rows).as_bytes()).ok()?;
        drop(file);
        fs::rename(&temp_path, &mirror_path).ok()?;
        (new_max, new_visible)
    };
    cursor.last_row_id = written_max_row_id;
    cursor.visible_count = written_visible_count;
    cursor.last_local_bytes = local_bytes;
    cursors.insert(session_id.to_string(), cursor);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_hermes_store(db_path: &Path, session_ids: &[&str]) {
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let connection = Connection::open(db_path).unwrap();
        connection
            .execute("CREATE TABLE sessions (id TEXT PRIMARY KEY)", [])
            .unwrap();
        for session_id in session_ids {
            connection
                .execute(
                    "INSERT INTO sessions (id) VALUES (?1)",
                    rusqlite::params![session_id],
                )
                .unwrap();
        }
    }

    #[test]
    fn profile_sessions_resolve_to_their_profile_store() {
        let hermes_home = tempfile::tempdir().unwrap();
        let home = hermes_home.path();
        let plain_session = "20260926_100000_a1a1a1";
        let harry_session = "20260926_100001_b2b2b2";
        write_hermes_store(&home.join("state.db"), &[plain_session]);
        write_hermes_store(&home.join("profiles/dobby/state.db"), &[]);
        write_hermes_store(&home.join("profiles/harry/state.db"), &[harry_session]);

        assert_eq!(
            hermes_state_db_path(home, harry_session),
            home.join("profiles/harry/state.db")
        );
        assert_eq!(
            hermes_state_db_path(home, plain_session),
            home.join("state.db")
        );
        assert_eq!(
            hermes_state_db_path(home, "20260926_100002_c3c3c3"),
            home.join("state.db")
        );
    }
}
