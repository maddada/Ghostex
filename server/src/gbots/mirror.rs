//! A GBots conversation as a transcript the chat pipeline can tail: each conversation's messages,
//! read from the sidecar's REST API, are mirrored into
//! `<gxserver state>/gbots/chat-mirror/<conversation id>.jsonl`, one Ghostex-owned record per line
//! in the shape `session_chat_decode_gbots.rs` reads:
//!
//! ```text
//! {"kind":"message","id":"…","role":"user"|"assistant"|"system","text":"…","timestamp":…,"turnId":"…"}
//! {"kind":"tool","id":"…","name":"…","input":…,"output":…,"isError":false,"timestamp":…,"turnId":"…"}
//! {"kind":"lifecycle","state":"working"|"completed"|"interrupted","turnId":"…","timestamp":…}
//! ```
//!
//! A turn's id is the id of the message that asked for it, so its reply and its lifecycle group.
//! New messages append; a changed message (a streamed reply growing, an edit) rewrites the file
//! atomically by rename, which the chat follower reads as replaced content and answers with a
//! fresh snapshot. The event stream (events.rs) syncs a conversation the moment it changes; the
//! follower's drain tick syncs it too, at most every few seconds, in case an event was missed.

use std::collections::HashMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// The newest messages a conversation mirrors.
const MESSAGE_LIMIT: usize = 500;
/// How often the drain tick may re-read a conversation the event stream keeps current.
const DRAIN_SYNC_EVERY: Duration = Duration::from_secs(5);

static LAST_SYNC: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn mirror_dir() -> PathBuf {
    super::sidecar::state_dir().join("chat-mirror")
}

pub(crate) fn mirror_path(thread_id: &str) -> PathBuf {
    mirror_dir().join(format!("{thread_id}.jsonl"))
}

/// The mirror for a conversation (created empty when missing, so the chat opens at once).
pub(crate) fn resolve_gbots_chat_transcript_path(thread_id: &str) -> Option<PathBuf> {
    super::client::id_segment(thread_id).ok()?;
    let path = mirror_path(thread_id);
    if !path.is_file() {
        let _ = std::fs::create_dir_all(mirror_dir());
        let _ = std::fs::File::create(&path);
        sync_thread(thread_id);
    }
    Some(path)
}

/// The drain tick's hook: the file's stem is the conversation id.
pub(crate) fn sync_gbots_transcript_mirror_for_path(path: &Path) {
    let Some(thread_id) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return;
    };
    let due = LAST_SYNC
        .lock()
        .ok()
        .and_then(|last| last.as_ref()?.get(thread_id).copied())
        .is_none_or(|at| at.elapsed() >= DRAIN_SYNC_EVERY);
    if due {
        sync_thread(thread_id);
    }
}

/// Reads the conversation and brings its mirror up to date. Nothing while the sidecar is down:
/// the mirror keeps what it last showed.
pub(crate) fn sync_thread(thread_id: &str) {
    let Ok(thread_id) = super::client::id_segment(thread_id) else {
        return;
    };
    if let Ok(mut last) = LAST_SYNC.lock() {
        last.get_or_insert_with(HashMap::new)
            .insert(thread_id.to_string(), Instant::now());
    }
    let Ok(answer) = super::client::get(&format!(
        "/api/threads/{thread_id}/messages?limit={MESSAGE_LIMIT}"
    )) else {
        return;
    };
    let messages = answer
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let busy = super::projects::thread_busy(thread_id);
    let text = mirror_lines(&messages, busy).join("");
    let _guard = WRITE_LOCK.lock();
    let path = mirror_path(thread_id);
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    if current == text {
        return;
    }
    let _ = std::fs::create_dir_all(mirror_dir());
    if let Some(added) = text.strip_prefix(current.as_str()) {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
        {
            let _ = file.write_all(added.as_bytes());
        }
        return;
    }
    let temporary = path.with_extension("jsonl.tmp");
    if std::fs::write(&temporary, &text).is_ok() {
        let _ = std::fs::rename(&temporary, &path);
    }
}

/// The mirror's lines for a conversation's messages, oldest first. `busy` says the bot is still
/// working on the last request, which keeps its turn open.
pub(crate) fn mirror_lines(messages: &[Value], busy: bool) -> Vec<String> {
    let text = |message: &Value, key: &str| -> Option<String> {
        message.get(key).and_then(Value::as_str).map(str::to_string)
    };
    let mut lines = Vec::new();
    let mut open_turn: Option<String> = None;
    for message in messages {
        let Some(id) = text(message, "id") else {
            continue;
        };
        let at = message.get("at").and_then(Value::as_i64);
        let kind = text(message, "kind").unwrap_or_default();
        // A turn's tool and memory summary, which the bot's reply already covers.
        if kind == "digest" {
            continue;
        }
        // A step the bot took: a failure or a notice the user must see (signed out, a missing
        // engine) is a system line; its tool steps are drawn as tool rows.
        if kind == "activity" {
            let Some(tool) = message.get("tool") else {
                continue;
            };
            let name = tool.get("name").and_then(Value::as_str).unwrap_or_default();
            let turn_id = text(message, "requestMessageId").or_else(|| open_turn.clone());
            if let Some(notice) = name
                .strip_prefix("error: ")
                .or_else(|| name.strip_prefix("notice: "))
            {
                lines.push(line(json!({
                    "kind": "message",
                    "id": id,
                    "role": "system",
                    "text": notice,
                    "timestamp": at,
                    "turnId": turn_id,
                })));
            } else if !name.is_empty() {
                lines.push(line(json!({
                    "kind": "tool",
                    "id": id,
                    "name": name,
                    "input": tool.get("input").or_else(|| tool.get("detail")).cloned().unwrap_or(Value::Null),
                    "output": tool.get("output").or_else(|| tool.get("result")).cloned(),
                    "isError": tool.get("ok").and_then(Value::as_bool) == Some(false),
                    "timestamp": at,
                    "turnId": turn_id,
                })));
            }
            continue;
        }
        let role = match text(message, "role").as_deref() {
            Some("user") => "user",
            Some("bot") => "assistant",
            _ => "system",
        };
        let mut body = text(message, "text").unwrap_or_default();
        if body.trim().is_empty() {
            if let Some(card) = message.get("card") {
                body = [card.get("title"), card.get("subtitle")]
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join("\n\n");
            }
        }
        if body.trim().is_empty() {
            continue;
        }
        let turn_id = if role == "user" {
            Some(id.clone())
        } else {
            text(message, "requestMessageId")
        };
        if role == "user" {
            if let Some(previous) = open_turn.take() {
                lines.push(line(json!({ "kind": "lifecycle", "state": "completed", "turnId": previous, "timestamp": at })));
            }
            open_turn = Some(id.clone());
            lines.push(line(
                json!({ "kind": "lifecycle", "state": "working", "turnId": id, "timestamp": at }),
            ));
        }
        lines.push(line(json!({
            "kind": "message",
            "id": id,
            "role": role,
            "text": body,
            "timestamp": at,
            "turnId": turn_id,
        })));
        if role == "assistant" && message.get("turnTerminal").and_then(Value::as_bool) == Some(true)
        {
            let succeeded = message.get("turnSucceeded").and_then(Value::as_bool) != Some(false);
            let finished = turn_id.clone().or_else(|| open_turn.clone());
            if let Some(finished) = finished {
                if open_turn.as_deref() == Some(finished.as_str()) {
                    open_turn = None;
                }
                let at = message
                    .get("turnCompletedAt")
                    .and_then(Value::as_i64)
                    .or(at);
                lines.push(line(json!({
                    "kind": "lifecycle",
                    "state": if succeeded { "completed" } else { "interrupted" },
                    "turnId": finished,
                    "timestamp": at,
                })));
            }
        }
    }
    if let Some(open) = open_turn.filter(|_| !busy) {
        lines.push(line(
            json!({ "kind": "lifecycle", "state": "completed", "turnId": open, "timestamp": Value::Null }),
        ));
    }
    lines
}

fn line(record: Value) -> String {
    format!("{record}\n")
}
