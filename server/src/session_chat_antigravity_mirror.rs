/*
CDXC:SessionChatAntigravity 2026-09-03:
Antigravity CLI (`agy`) keeps its conversation store encrypted
(`~/.gemini/antigravity-cli/conversations/<id>.pb|.db`, 8.0 bits/byte), but it
also appends a plaintext step log per conversation that its own checkpoint
summaries point the model at:

    ~/.gemini/antigravity-cli/brain/<conversationId>/.system_generated/logs/
        transcript_full.jsonl   # 1.1.24+: real tool-arg values
        transcript.jsonl        # every build: arg values JSON-stringified

Its hooks name that file (`transcriptPath`) and the conversation
(`conversationId`) on every event, which the hook receiver already stores as
`agentSessionPath` / `agentSessionId`. Each line is ONE trajectory step:

    {"step_index": 7, "source": "MODEL", "type": "PLANNER_RESPONSE",
     "status": "DONE", "created_at": "2026-09-02T19:36:02Z",
     "content": "pong", "thinking": "**…**", "tool_calls": [{"name": …, "args": {…}}]}

    USER_EXPLICIT / USER_INPUT       the prompt, wrapped in <USER_REQUEST> plus
                                     harness metadata tags
    MODEL / PLANNER_RESPONSE         any of thinking, content, tool_calls in ONE step
    MODEL / <tool type> or GENERIC   the tool's result text (RUN_COMMAND,
                                     VIEW_FILE, … before 1.1.24; GENERIC after)
    SYSTEM / CONVERSATION_HISTORY,
    SYSTEM / CHECKPOINT              harness bookkeeping, never shown

A planner step carries the model's thinking AND its answer or tool calls in one
record, while the chat contract decodes one message per line, so the thinking
would vanish. This module mirrors the raw log into a Ghostex-owned jsonl in
the gxserver state dir where every step becomes one or two rows, tool results
lose their `Created At:` headers, and the older build's stringified args are
parsed back.

The thinking is agy's only mid-turn narration (its TUI shows nothing else
between a prompt and the tool rows), so it is mirrored as the assistant's own
text (`narration: true`) rather than as a reasoning turn: a reasoning row
that tool rows attach to collapses to its first line behind a caret, which
hid the one paragraph that explains what the agent is doing. The narration
flag keeps the lifecycle decoder from reading that row as the turn's answer. The mirror is what the chat pipeline tails; the same
freshen-before-read and rename-on-rewrite contract as the Cursor and Hermes
mirrors applies, so `session_chat_decode_antigravity.rs` stays line-local.
*/

use std::collections::HashMap;
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::resume_lookup::home_dir;
use crate::session_chat::{extract_string, parse_json_object};

const ANTIGRAVITY_SESSION_ID_MAX_LENGTH: usize = 128;
pub(crate) const ANTIGRAVITY_TRANSCRIPT_FILE_NAMES: [&str; 2] =
    ["transcript_full.jsonl", "transcript.jsonl"];

pub(crate) fn is_safe_antigravity_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= ANTIGRAVITY_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// `~/.gemini/antigravity-cli`, the CLI's app data directory (its own log
/// calls it `appDataDir`; the skills resolver already assumes the same root).
pub(crate) fn antigravity_app_data_dir() -> PathBuf {
    home_dir().join(".gemini").join("antigravity-cli")
}

fn antigravity_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("antigravity-chat-mirror")
}

/// The raw step log for a conversation, preferring the `_full` variant whose
/// tool args are real JSON values.
pub(crate) fn find_antigravity_transcript(session_id: &str) -> Option<PathBuf> {
    if !is_safe_antigravity_session_id(session_id) {
        return None;
    }
    let logs_dir = antigravity_app_data_dir()
        .join("brain")
        .join(session_id)
        .join(".system_generated")
        .join("logs");
    ANTIGRAVITY_TRANSCRIPT_FILE_NAMES
        .iter()
        .map(|name| logs_dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The conversation id a hook-supplied raw path belongs to:
/// `…/brain/<id>/.system_generated/logs/<file>`.
fn session_id_from_raw_path(path: &Path) -> Option<String> {
    let mut ancestors = path.ancestors();
    ancestors.next()?; // the file itself
    let logs = ancestors.next()?;
    let system_generated = ancestors.next()?;
    if logs.file_name().and_then(|name| name.to_str()) != Some("logs")
        || system_generated.file_name().and_then(|name| name.to_str()) != Some(".system_generated")
    {
        return None;
    }
    let id = ancestors
        .next()?
        .file_name()
        .and_then(|name| name.to_str())?
        .to_string();
    is_safe_antigravity_session_id(&id).then_some(id)
}

/* ------------------------------------------------------------ step rows */

const USER_REQUEST_OPEN: &str = "<USER_REQUEST>";
const USER_REQUEST_CLOSE: &str = "</USER_REQUEST>";

/// The prompt the user typed, without the harness's metadata envelope. A
/// record with no envelope is taken whole.
fn antigravity_user_text(content: &str) -> String {
    let Some(start) = content.find(USER_REQUEST_OPEN) else {
        return content.trim().to_string();
    };
    let body = &content[start + USER_REQUEST_OPEN.len()..];
    let body = match body.find(USER_REQUEST_CLOSE) {
        Some(end) => &body[..end],
        None => body,
    };
    body.trim().to_string()
}

/// Tool results open with two timestamp lines the TUI never shows.
fn antigravity_tool_output(content: &str) -> String {
    let mut lines = content.lines().peekable();
    while lines
        .peek()
        .is_some_and(|line| line.starts_with("Created At:") || line.starts_with("Completed At:"))
    {
        lines.next();
    }
    let rest: Vec<&str> = lines.collect();
    let joined = rest.join("\n");
    // The result body is indented with a run of tabs on some tools; fold
    // that indentation away without touching the text itself.
    joined
        .lines()
        .map(|line| line.strip_prefix("\t\t\t\t").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// `The command exited with code N.` with a non-zero N is the only structured
/// failure signal the result text carries.
fn antigravity_tool_output_is_error(output: &str) -> bool {
    const EXIT_PREFIX: &str = "exited with code ";
    output.lines().any(|line| {
        line.find(EXIT_PREFIX).is_some_and(|start| {
            let digits: String = line[start + EXIT_PREFIX.len()..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            digits.parse::<u32>().is_ok_and(|code| code != 0)
        })
    })
}

/// `transcript.jsonl` (and every pre-1.1.24 log) stores each arg value as its
/// JSON text (`"\"echo pong\""`, `"2000"`); give the UI the value back.
fn normalize_tool_args(args: &Value) -> Value {
    let Value::Object(record) = args else {
        return args.clone();
    };
    let mut normalized = Map::with_capacity(record.len());
    for (key, value) in record {
        let restored = match value {
            Value::String(text) => serde_json::from_str::<Value>(text)
                .ok()
                .filter(|parsed| !matches!(parsed, Value::Object(_) | Value::Array(_)))
                .unwrap_or_else(|| value.clone()),
            other => other.clone(),
        };
        normalized.insert(key.clone(), restored);
    }
    Value::Object(normalized)
}

fn antigravity_tool_calls(record: &Map<String, Value>) -> Vec<Value> {
    let Some(Value::Array(tool_calls)) = record.get("tool_calls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            let name = extract_string(tool_call.get("name")).unwrap_or_else(|| "tool".to_string());
            let args = tool_call
                .get("args")
                .map(normalize_tool_args)
                .unwrap_or(Value::Null);
            Some(json!({ "name": name, "args": args }))
        })
        .collect()
}

/// Mirror rows for one raw step, in display order.
fn mirror_rows_for_step(line: &str) -> Vec<Value> {
    let Some(record) = parse_json_object(line) else {
        return Vec::new();
    };
    let Some(step_index) = record.get("step_index").and_then(Value::as_u64) else {
        return Vec::new();
    };
    let source = record
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let step_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let created_at = record.get("created_at").cloned().unwrap_or(Value::Null);
    let content = extract_string(record.get("content"));
    let row = |part: &str, extra: Map<String, Value>| {
        let mut row = Map::new();
        row.insert("stepIndex".into(), Value::from(step_index));
        row.insert("part".into(), Value::from(part));
        row.insert("createdAt".into(), created_at.clone());
        row.extend(extra);
        Value::Object(row)
    };
    match (source, step_type) {
        ("USER_EXPLICIT", "USER_INPUT") => {
            let text = content.map(|content| antigravity_user_text(&content));
            match text.filter(|text| !text.is_empty()) {
                Some(text) => vec![row(
                    "user",
                    Map::from_iter([("text".into(), Value::from(text))]),
                )],
                None => Vec::new(),
            }
        }
        ("MODEL", "PLANNER_RESPONSE") => {
            let mut rows = Vec::new();
            if let Some(thinking) =
                extract_string(record.get("thinking")).filter(|text| !text.trim().is_empty())
            {
                rows.push(row(
                    "assistant",
                    Map::from_iter([
                        ("text".into(), Value::from(thinking)),
                        ("narration".into(), Value::Bool(true)),
                    ]),
                ));
            }
            let tool_calls = antigravity_tool_calls(&record);
            let text = content.filter(|text| !text.trim().is_empty());
            if text.is_some() || !tool_calls.is_empty() {
                let mut extra = Map::new();
                if let Some(text) = text {
                    extra.insert("text".into(), Value::from(text));
                }
                if !tool_calls.is_empty() {
                    extra.insert("toolCalls".into(), Value::Array(tool_calls));
                }
                rows.push(row("assistant", extra));
            }
            rows
        }
        ("MODEL", tool_type) => {
            let Some(content) = content else {
                return Vec::new();
            };
            let output = antigravity_tool_output(&content);
            let mut extra = Map::from_iter([
                ("toolType".into(), Value::from(tool_type)),
                ("text".into(), Value::from(output.clone())),
            ]);
            if antigravity_tool_output_is_error(&output) {
                extra.insert("isError".into(), Value::Bool(true));
            }
            vec![row("tool", extra)]
        }
        _ => Vec::new(),
    }
}

/// The whole mirror for a raw log. Only complete lines are read: the CLI
/// appends a step in one write, but a torn tail would otherwise be mirrored
/// as a parse failure and never revisited.
fn build_mirror(raw: &[u8]) -> Vec<u8> {
    let complete = match raw.iter().rposition(|byte| *byte == b'\n') {
        Some(end) => &raw[..=end],
        None => &[][..],
    };
    let text = String::from_utf8_lossy(complete);
    let mut out = Vec::with_capacity(complete.len());
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        for row in mirror_rows_for_step(line) {
            if let Ok(serialized) = serde_json::to_vec(&row) {
                out.extend_from_slice(&serialized);
                out.push(b'\n');
            }
        }
    }
    out
}

/* ------------------------------------------------------------ sync */

#[derive(Default)]
struct AntigravityMirrorState {
    raw_path: Option<PathBuf>,
    raw_len: u64,
    raw_modified: Option<std::time::SystemTime>,
    output_len: usize,
    output_hash: u64,
}

static MIRROR_STATES: Mutex<Option<HashMap<String, AntigravityMirrorState>>> = Mutex::new(None);

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Append when the new output extends the old byte-for-byte; otherwise write
/// a temp file and rename it over the mirror so the follower's inode check
/// reports `content_replaced` instead of reading a half-written file.
fn write_mirror(
    mirror_path: &Path,
    output: &[u8],
    state: &AntigravityMirrorState,
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

/// One sync pass for one conversation. `raw_hint` is the hook-supplied log
/// path, used when it exists so the mirror follows the file the CLI named.
fn sync_antigravity_transcript_mirror(
    session_id: &str,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    if !is_safe_antigravity_session_id(session_id) {
        return None;
    }
    let mirror_path = antigravity_mirror_dir().join(format!("{session_id}.jsonl"));

    let mut states_guard = MIRROR_STATES.lock().ok()?;
    let states = states_guard.get_or_insert_with(HashMap::new);
    let state = states.entry(session_id.to_string()).or_default();
    let raw_path = raw_hint
        .filter(|path| path.is_file())
        .map(Path::to_path_buf)
        .or_else(|| state.raw_path.clone().filter(|path| path.is_file()))
        .or_else(|| find_antigravity_transcript(session_id))?;
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
    let up_to_date = mirror_exists
        && state.output_len > 0
        && state.raw_len == raw_len
        && state.raw_modified == raw_modified;
    if up_to_date {
        return Some(mirror_path);
    }
    let raw = fs::read(&raw_path).ok()?;
    let output = build_mirror(&raw);
    write_mirror(&mirror_path, &output, state, mirror_exists)?;
    state.raw_len = raw_len;
    state.raw_modified = raw_modified;
    state.output_len = output.len();
    state.output_hash = content_hash(&output);
    Some(mirror_path)
}

/// Path-resolution entry: sync, then hand back the mirror as "the transcript".
pub fn resolve_antigravity_chat_transcript_path(
    session_id: Option<&str>,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| raw_hint.and_then(session_id_from_raw_path))?;
    sync_antigravity_transcript_mirror(&session_id, raw_hint)
}

/// Steady-state entry for the follower's drain tick, which holds only the
/// already-resolved path. The stem is the conversation id by construction.
pub(crate) fn sync_antigravity_transcript_mirror_for_path(mirror_path: &Path) {
    if let Some(session_id) = mirror_path.file_stem().and_then(|stem| stem.to_str()) {
        sync_antigravity_transcript_mirror(session_id, None);
    }
}
