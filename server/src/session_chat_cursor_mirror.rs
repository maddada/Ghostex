/*
Cursor CLI writes two records of one conversation. The jsonl under
`~/.cursor/projects/<project>/agent-transcripts/<id>/` is the model-message
log the chat pipeline tails: user prompts, assistant text, `tool_use` blocks
and `turn_ended`. It never carries the thinking the terminal shows — Grok
sessions redact reasoning entirely, and Claude sessions glue the summary onto
a plain `text` block. The readable thinking lives only in the per-session
SQLite store at `~/.cursor/chats/<md5(projectPath)>/<id>/store.db`: a
content-addressed blob tree (protobuf) whose root (`meta.latestRootBlobId`)
lists one turn blob per prompt in repeated field 8, each turn listing its
ordered UI steps in field 2 — text (field 1), tool (field 2) and thinking
(field 3, with text, duration and timestamps).

This module keeps the jsonl as the spine (it alone knows turn lifecycle) and
materializes `<gxserver state dir>/cursor-chat-mirror/<id>.jsonl`: the raw
lines, byte-for-byte, with a `thinking` line spliced in wherever the store
places a thinking step. Placement is by walking both in lockstep per turn —
each assistant record consumes its text step (matched by content) and one tool
step per `tool_use`; thinking steps met on the way are emitted before the
record. A jsonl text block that IS a thinking summary (Claude's quirk) is
re-typed as thinking instead of being shown twice.

Sync runs where every consumer already passes through: transcript-path
resolution and the follower's drain tick. Appends grow the mirror in place;
anything else (cold start, rewind, late-arriving thinking) rewrites it via
rename so the follower sees `content_replaced` and re-snapshots.
*/

use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Map, Value};

use crate::resume_lookup::{home_dir, parse_cursor_meta_value};
use crate::session_chat_paths::find_cursor_chat_transcript;

const CURSOR_SESSION_ID_MAX_LENGTH: usize = 128;
const CURSOR_STORE_BUSY_TIMEOUT_MS: u64 = 250;
const CURSOR_REDACTED_REASONING: &str = "[REDACTED]";
const CURSOR_BLOB_ID_LEN: usize = 32;

pub(crate) fn is_safe_cursor_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= CURSOR_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn cursor_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("cursor-chat-mirror")
}

/* ------------------------------------------------------------ protobuf */

enum ProtoValue<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

fn read_varint(bytes: &[u8], index: &mut usize) -> Option<u64> {
    let mut value: u64 = 0;
    let mut shift = 0u32;
    loop {
        let byte = *bytes.get(*index)?;
        *index += 1;
        if shift >= 64 {
            return None;
        }
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
    }
}

/// Wire-level field list of one message; `None` when the bytes are not a
/// well-formed message (blob contents such as attached files are not).
fn proto_fields(bytes: &[u8]) -> Option<Vec<(u32, ProtoValue<'_>)>> {
    let mut index = 0usize;
    let mut fields = Vec::new();
    while index < bytes.len() {
        let key = read_varint(bytes, &mut index)?;
        let number = u32::try_from(key >> 3).ok()?;
        if number == 0 {
            return None;
        }
        match key & 0x7 {
            0 => fields.push((number, ProtoValue::Varint(read_varint(bytes, &mut index)?))),
            1 => {
                index = index.checked_add(8).filter(|end| *end <= bytes.len())?;
                fields.push((number, ProtoValue::Fixed));
            }
            2 => {
                let length = usize::try_from(read_varint(bytes, &mut index)?).ok()?;
                let end = index
                    .checked_add(length)
                    .filter(|end| *end <= bytes.len())?;
                fields.push((number, ProtoValue::Bytes(&bytes[index..end])));
                index = end;
            }
            5 => {
                index = index.checked_add(4).filter(|end| *end <= bytes.len())?;
                fields.push((number, ProtoValue::Fixed));
            }
            _ => return None,
        }
    }
    Some(fields)
}

fn proto_bytes<'a>(fields: &[(u32, ProtoValue<'a>)], number: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|(field, value)| match value {
        ProtoValue::Bytes(bytes) if *field == number => Some(*bytes),
        _ => None,
    })
}

fn proto_varint(fields: &[(u32, ProtoValue<'_>)], number: u32) -> Option<u64> {
    fields.iter().find_map(|(field, value)| match value {
        ProtoValue::Varint(varint) if *field == number => Some(*varint),
        _ => None,
    })
}

fn proto_blob_ids<'a>(fields: &[(u32, ProtoValue<'a>)], number: u32) -> Vec<String> {
    fields
        .iter()
        .filter_map(|(field, value)| match value {
            ProtoValue::Bytes(bytes) if *field == number && bytes.len() == CURSOR_BLOB_ID_LEN => {
                Some(hex_string(bytes))
            }
            _ => None,
        })
        .collect()
}

fn hex_string(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

/* --------------------------------------------------------------- store */

#[derive(Clone, Debug)]
enum CursorStep {
    Text(String),
    Tool,
    Thinking {
        text: String,
        duration_ms: Option<u64>,
        ended_at_ms: Option<u64>,
    },
    Other,
}

/// One timeline step blob. The step is a oneof keyed by its first field:
/// 1 = assistant text `{1: text, 2: started, 3: ended}`, 2 = tool call (shape
/// varies per tool), 3 = thinking `{1: text, 2: duration ms, 3: started, 4:
/// ended}`. User prompts hang off the turn blob's field 1 instead and carry
/// fields 17/25/26; they never appear in the step list.
fn decode_cursor_step(bytes: &[u8]) -> CursorStep {
    let Some(fields) = proto_fields(bytes) else {
        return CursorStep::Other;
    };
    if fields
        .iter()
        .any(|(number, _)| matches!(number, 17 | 25 | 26))
    {
        return CursorStep::Other;
    }
    match fields.first() {
        Some((1, ProtoValue::Bytes(inner))) => proto_fields(inner)
            .and_then(|inner| proto_bytes(&inner, 1).map(|text| text.to_vec()))
            .map(|text| CursorStep::Text(String::from_utf8_lossy(&text).into_owned()))
            .unwrap_or(CursorStep::Other),
        Some((2, _)) => CursorStep::Tool,
        Some((3, ProtoValue::Bytes(inner))) => {
            let Some(inner) = proto_fields(inner) else {
                return CursorStep::Other;
            };
            let Some(text) = proto_bytes(&inner, 1) else {
                return CursorStep::Other;
            };
            CursorStep::Thinking {
                text: String::from_utf8_lossy(text).into_owned(),
                duration_ms: proto_varint(&inner, 2),
                ended_at_ms: proto_varint(&inner, 4),
            }
        }
        _ => CursorStep::Other,
    }
}

fn find_cursor_store(session_id: &str) -> Option<PathBuf> {
    let chats_dir = home_dir().join(".cursor").join("chats");
    for project_dir in fs::read_dir(chats_dir).ok()?.flatten() {
        let candidate = project_dir.path().join(session_id).join("store.db");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

fn open_cursor_store(path: &Path) -> Option<Connection> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    connection
        .busy_timeout(std::time::Duration::from_millis(
            CURSOR_STORE_BUSY_TIMEOUT_MS,
        ))
        .ok()?;
    Some(connection)
}

fn read_cursor_store_root_id(connection: &Connection) -> Option<String> {
    let mut statement = connection.prepare("select value from meta").ok()?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .ok()?;
    let root_id = rows.flatten().find_map(|raw| {
        let meta = parse_cursor_meta_value(&raw)?;
        let root = meta.get("latestRootBlobId")?.as_str()?.trim();
        (!root.is_empty()).then(|| root.to_string())
    });
    root_id
}

fn read_cursor_blob(connection: &Connection, id: &str) -> Option<Vec<u8>> {
    connection
        .query_row(
            "select data from blobs where id = ?1",
            rusqlite::params![id],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .ok()
}

/// Every turn's ordered steps, from the root down. `None` when any blob on
/// the way is missing or malformed, which happens while Cursor is mid-write
/// (blobs land before the root that names them, but not atomically with it);
/// the caller keeps the previous mirror and retries next tick.
fn load_cursor_turns(
    connection: &Connection,
    root_id: &str,
    cache: &mut HashMap<String, CursorStep>,
) -> Option<Vec<Vec<CursorStep>>> {
    let root = read_cursor_blob(connection, root_id)?;
    let root_fields = proto_fields(&root)?;
    let mut turns = Vec::new();
    for turn_id in proto_blob_ids(&root_fields, 8) {
        let turn = read_cursor_blob(connection, &turn_id)?;
        let turn_fields = proto_fields(&turn)?;
        let mut steps = Vec::new();
        for (number, value) in &turn_fields {
            let ProtoValue::Bytes(body) = value else {
                continue;
            };
            if *number != 1 {
                continue;
            }
            let body_fields = proto_fields(body)?;
            for step_id in proto_blob_ids(&body_fields, 2) {
                let step = match cache.get(&step_id) {
                    Some(step) => step.clone(),
                    None => {
                        let step = decode_cursor_step(&read_cursor_blob(connection, &step_id)?);
                        cache.insert(step_id, step.clone());
                        step
                    }
                };
                steps.push(step);
            }
        }
        turns.push(steps);
    }
    Some(turns)
}

/* --------------------------------------------------------------- merge */

enum RawLine<'a> {
    User(&'a [u8]),
    Event(&'a [u8]),
    Assistant {
        line: &'a [u8],
        record: Map<String, Value>,
        text: Option<String>,
        text_block_count: usize,
        tool_count: usize,
    },
    Other(&'a [u8]),
}

fn cursor_visible_text(text: &str) -> Option<String> {
    let visible = text
        .lines()
        .filter(|line| line.trim() != CURSOR_REDACTED_REASONING)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    (!visible.is_empty()).then_some(visible)
}

fn classify_raw_line(line: &[u8]) -> RawLine<'_> {
    let Ok(Value::Object(record)) = serde_json::from_slice::<Value>(line) else {
        return RawLine::Other(line);
    };
    if record.get("type").and_then(Value::as_str).is_some() {
        return RawLine::Event(line);
    }
    match record.get("role").and_then(Value::as_str) {
        Some("user") => RawLine::User(line),
        Some("assistant") => {
            let mut texts = Vec::new();
            let mut text_block_count = 0usize;
            let mut tool_count = 0usize;
            if let Some(items) = record
                .get("message")
                .and_then(Value::as_object)
                .and_then(|message| message.get("content"))
                .and_then(Value::as_array)
            {
                for item in items {
                    match item.get("type").and_then(Value::as_str) {
                        Some("text") => {
                            text_block_count += 1;
                            if let Some(text) = item
                                .get("text")
                                .and_then(Value::as_str)
                                .and_then(cursor_visible_text)
                            {
                                texts.push(text);
                            }
                        }
                        Some("tool_use") => tool_count += 1,
                        _ => {}
                    }
                }
            }
            RawLine::Assistant {
                line,
                record,
                text: (!texts.is_empty()).then(|| texts.join("\n\n")),
                text_block_count,
                tool_count,
            }
        }
        _ => RawLine::Other(line),
    }
}

/// Whitespace-free form for comparisons: the store keeps streamed summaries
/// without the paragraph breaks the jsonl copy has, so only the characters
/// between them can be trusted to agree.
fn normalize_text(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn same_text(left: &str, right: &str) -> bool {
    normalize_text(left) == normalize_text(right)
}

/// `Some(rest)` when `full` begins with `prefix` (whitespace-insensitively);
/// `rest` is the whitespace-free remainder, empty on an exact match.
fn strip_text_prefix(full: &str, prefix: &str) -> Option<String> {
    let full = normalize_text(full);
    let prefix = normalize_text(prefix);
    full.strip_prefix(prefix.as_str())
        .map(|rest| rest.trim().to_string())
}

fn push_raw_line(out: &mut Vec<u8>, line: &[u8]) {
    out.extend_from_slice(line);
    out.push(b'\n');
}

fn push_thinking_line(out: &mut Vec<u8>, step: &CursorStep) {
    let CursorStep::Thinking {
        text,
        duration_ms,
        ended_at_ms,
    } = step
    else {
        return;
    };
    let mut block = json!({ "type": "thinking", "text": text });
    if let Some(duration_ms) = duration_ms {
        block["durationMs"] = json!(duration_ms);
    }
    if let Some(ended_at_ms) = ended_at_ms {
        block["endedAt"] = json!(ended_at_ms);
    }
    let line = json!({
        "role": "assistant",
        "message": { "content": [block] },
        "origin": "cursor-store",
    });
    out.extend_from_slice(line.to_string().as_bytes());
    out.push(b'\n');
}

enum RecordText {
    Keep,
    Remove,
    Replace(String),
}

/// Re-serialize an assistant record with its (single) text block removed or
/// replaced. Skips the line entirely when nothing visible would remain.
fn push_record_line(out: &mut Vec<u8>, line: &[u8], record: Map<String, Value>, text: RecordText) {
    let mut record = record;
    let mut removed_everything = false;
    if !matches!(text, RecordText::Keep) {
        if let Some(items) = record
            .get_mut("message")
            .and_then(Value::as_object_mut)
            .and_then(|message| message.get_mut("content"))
            .and_then(Value::as_array_mut)
        {
            match &text {
                RecordText::Remove => {
                    items.retain(|item| item.get("type").and_then(Value::as_str) != Some("text"));
                    removed_everything = items.is_empty();
                }
                RecordText::Replace(replacement) => {
                    for item in items.iter_mut() {
                        if item.get("type").and_then(Value::as_str) == Some("text") {
                            item["text"] = Value::String(replacement.clone());
                        }
                    }
                }
                RecordText::Keep => {}
            }
        }
    }
    if removed_everything {
        return;
    }
    if matches!(text, RecordText::Keep) {
        push_raw_line(out, line);
    } else {
        out.extend_from_slice(Value::Object(record).to_string().as_bytes());
        out.push(b'\n');
    }
}

struct TurnWalk<'a> {
    steps: &'a [CursorStep],
    index: usize,
}

impl<'a> TurnWalk<'a> {
    fn peek(&self) -> Option<&'a CursorStep> {
        self.steps.get(self.index)
    }

    fn flush_thinking(&mut self, out: &mut Vec<u8>) {
        while let Some(step @ CursorStep::Thinking { .. }) = self.peek() {
            push_thinking_line(out, step);
            self.index += 1;
        }
    }

    fn merge_assistant(
        &mut self,
        out: &mut Vec<u8>,
        line: &[u8],
        record: Map<String, Value>,
        text: Option<String>,
        text_block_count: usize,
        tool_count: usize,
    ) {
        let rewritable = text_block_count == 1;
        let mut remaining_text = text;
        let mut rewrite = RecordText::Keep;
        let mut deferred: Vec<&'a CursorStep> = Vec::new();

        // Thinking that precedes the record, then the record's own text step.
        while let Some(step) = self.peek() {
            match step {
                CursorStep::Thinking { text, .. } => {
                    if rewritable
                        && remaining_text
                            .as_deref()
                            .is_some_and(|candidate| same_text(candidate, text))
                    {
                        // Cursor wrote this summary as the record's text block.
                        remaining_text = None;
                        rewrite = RecordText::Remove;
                    }
                    push_thinking_line(out, step);
                    self.index += 1;
                }
                CursorStep::Text(step_text) => {
                    let Some(candidate) = remaining_text.take() else {
                        break;
                    };
                    self.index += 1;
                    if let Some(rest) = strip_text_prefix(&candidate, step_text) {
                        if !rest.is_empty() && rewritable {
                            if let Some(next @ CursorStep::Thinking { text, .. }) = self.peek() {
                                if same_text(&rest, text) {
                                    // Summary glued onto the text: split it back out.
                                    rewrite = RecordText::Replace(step_text.clone());
                                    deferred.push(next);
                                    self.index += 1;
                                }
                            }
                        }
                    }
                    break;
                }
                CursorStep::Tool => break,
                CursorStep::Other => self.index += 1,
            }
        }

        // One tool step per tool_use; thinking met in between trails the record.
        let mut needed = tool_count;
        while needed > 0 {
            match self.peek() {
                Some(CursorStep::Tool) => {
                    self.index += 1;
                    needed -= 1;
                }
                Some(step @ CursorStep::Thinking { .. }) => {
                    deferred.push(step);
                    self.index += 1;
                }
                Some(CursorStep::Other) => self.index += 1,
                Some(CursorStep::Text(_)) | None => break,
            }
        }

        push_record_line(out, line, record, rewrite);
        for step in deferred {
            push_thinking_line(out, step);
        }
    }
}

/// The mirror's full content for a raw jsonl and the store's turns. Only
/// newline-terminated raw lines are consumed; a trailing partial line waits
/// for the next sync so it is never emitted as a truncated record.
fn merge_cursor_transcript(raw: &[u8], turns: &[Vec<CursorStep>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len() + raw.len() / 4);
    let complete_end = raw
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |at| at + 1);
    let mut walk = TurnWalk {
        steps: &[],
        index: 0,
    };
    let mut turn_index = 0usize;
    for line in raw[..complete_end].split(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match classify_raw_line(line) {
            RawLine::User(line) => {
                walk.flush_thinking(&mut out);
                walk = TurnWalk {
                    steps: turns.get(turn_index).map(Vec::as_slice).unwrap_or(&[]),
                    index: 0,
                };
                turn_index += 1;
                push_raw_line(&mut out, line);
            }
            RawLine::Event(line) => {
                walk.flush_thinking(&mut out);
                push_raw_line(&mut out, line);
            }
            RawLine::Other(line) => push_raw_line(&mut out, line),
            RawLine::Assistant {
                line,
                record,
                text,
                text_block_count,
                tool_count,
            } => walk.merge_assistant(&mut out, line, record, text, text_block_count, tool_count),
        }
    }
    walk.flush_thinking(&mut out);
    out
}

/* ---------------------------------------------------------------- sync */

#[derive(Default)]
struct CursorMirrorState {
    raw_path: Option<PathBuf>,
    store_path: Option<PathBuf>,
    raw_len: u64,
    raw_modified: Option<SystemTime>,
    root_id: Option<String>,
    output_len: usize,
    output_hash: u64,
    steps: HashMap<String, CursorStep>,
}

static MIRROR_STATES: Mutex<Option<HashMap<String, CursorMirrorState>>> = Mutex::new(None);

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

fn write_mirror(
    mirror_path: &Path,
    output: &[u8],
    state: &CursorMirrorState,
    mirror_exists: bool,
) -> Option<()> {
    fs::create_dir_all(mirror_path.parent()?).ok()?;
    let pure_append = mirror_exists
        && state.output_len > 0
        && output.len() >= state.output_len
        && content_hash(&output[..state.output_len]) == state.output_hash;
    if pure_append {
        if output.len() > state.output_len {
            let mut file = fs::OpenOptions::new().append(true).open(mirror_path).ok()?;
            file.write_all(&output[state.output_len..]).ok()?;
        }
        return Some(());
    }
    let temp_path = mirror_path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&temp_path).ok()?;
    file.write_all(output).ok()?;
    file.flush().ok()?;
    drop(file);
    fs::rename(&temp_path, mirror_path).ok()
}

/// One sync pass for one session. `raw_hint` is a hook-supplied transcript
/// path, used when it exists so the mirror follows the file Cursor named.
fn sync_cursor_transcript_mirror(session_id: &str, raw_hint: Option<&Path>) -> Option<PathBuf> {
    if !is_safe_cursor_session_id(session_id) {
        return None;
    }
    let mirror_path = cursor_mirror_dir().join(format!("{session_id}.jsonl"));

    let mut states_guard = MIRROR_STATES.lock().ok()?;
    let states = states_guard.get_or_insert_with(HashMap::new);
    let state = states.entry(session_id.to_string()).or_default();
    // The drain tick knows only the mirror path, so the raw file it shadows
    // is remembered from the resolve that first found it.
    let raw_path = raw_hint
        .filter(|path| path.is_file())
        .map(Path::to_path_buf)
        .or_else(|| state.raw_path.clone().filter(|path| path.is_file()))
        .or_else(|| find_cursor_chat_transcript(session_id))?;
    state.raw_path = Some(raw_path.clone());
    let raw_meta = fs::metadata(&raw_path).ok()?;
    let raw_len = raw_meta.len();
    let raw_modified = raw_meta.modified().ok();
    // A state only counts while the file it describes is still there; a wiped
    // state dir or fresh daemon always rebuilds.
    let mirror_exists = mirror_path.is_file();
    if !mirror_exists {
        state.output_len = 0;
        state.output_hash = 0;
    }
    if state.store_path.is_none() {
        state.store_path = find_cursor_store(session_id);
    }
    let connection = state.store_path.as_deref().and_then(open_cursor_store);
    let root_id = connection.as_ref().and_then(read_cursor_store_root_id);

    let up_to_date = mirror_exists
        && state.output_len > 0
        && state.raw_len == raw_len
        && state.raw_modified == raw_modified
        && state.root_id == root_id;
    if up_to_date {
        return Some(mirror_path);
    }

    let turns = match (&connection, root_id.as_deref()) {
        (Some(connection), Some(root_id)) => {
            load_cursor_turns(connection, root_id, &mut state.steps)
        }
        _ => None,
    };
    if turns.is_none() && state.store_path.is_some() && mirror_exists && state.output_len > 0 {
        // The store exists but is unreadable this instant; serving a mirror
        // without its thinking would force a rewrite now and another one on
        // the next tick. Keep the current file and retry.
        return Some(mirror_path);
    }
    let turns = turns.unwrap_or_default();

    let raw = fs::read(&raw_path).ok()?;
    let output = merge_cursor_transcript(&raw, &turns);
    write_mirror(&mirror_path, &output, state, mirror_exists)?;
    state.raw_len = raw_len;
    state.raw_modified = raw_modified;
    state.root_id = root_id;
    state.output_len = output.len();
    state.output_hash = content_hash(&output);
    Some(mirror_path)
}

/// Path-resolution entry: sync, then hand back the mirror as "the transcript".
pub(crate) fn resolve_cursor_chat_transcript_path(
    session_id: Option<&str>,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            // Hooks may name only the raw jsonl; its stem is the session id.
            raw_hint
                .and_then(|path| path.file_stem())
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
        })?;
    sync_cursor_transcript_mirror(&session_id, raw_hint)
}

/// Steady-state entry for the follower's drain tick, which holds only the
/// already-resolved path. The stem is the Cursor session id by construction.
pub(crate) fn sync_cursor_transcript_mirror_for_path(mirror_path: &Path) {
    if let Some(session_id) = mirror_path.file_stem().and_then(|stem| stem.to_str()) {
        sync_cursor_transcript_mirror(session_id, None);
    }
}
