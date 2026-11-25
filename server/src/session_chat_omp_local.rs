//! Sidecar persistence for OMP Native local slash commands (`/context`, `/help`, etc.).
//!
//! `omp acp` handles local slash commands by streaming `agent_message_chunk` without writing
//! rows to OMP's own session JSONL file (which OMP manages as a `parentId`-linked tree).
//! Storing those exchanges in a small sidecar under Ghostex's state directory lets
//! `session_chat_tail` merge them chronologically into chat reads without mutating OMP's tree.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatSource,
};

fn is_safe_omp_session_id(session_id: &str) -> bool {
    let trimmed = session_id.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 160
        && trimmed
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn omp_local_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("omp-local-transcripts")
}

fn omp_local_path(session_id: &str) -> PathBuf {
    omp_local_dir().join(format!("{session_id}.local.jsonl"))
}

/// Extracts the session id from a Pi/OMP transcript filename (`<timestamp>_<id>.jsonl` or `<id>.jsonl`).
pub(crate) fn extract_omp_session_id_from_path(file_path: &Path) -> Option<String> {
    let stem = file_path.file_stem()?.to_str()?.trim();
    if stem.is_empty() {
        return None;
    }
    let id = match stem.rsplit_once('_') {
        Some((_, tail)) if !tail.is_empty() => tail,
        _ => stem,
    };
    is_safe_omp_session_id(id).then(|| id.to_string())
}

/// Byte size of the local slash-command sidecar for `file_path`, or `0` if none exists.
pub(crate) fn sidecar_len_for_transcript(file_path: &Path) -> u64 {
    let Some(session_id) = extract_omp_session_id_from_path(file_path) else {
        return 0;
    };
    fs::metadata(omp_local_path(&session_id))
        .map(|meta| meta.len())
        .unwrap_or(0)
}

fn strip_ansi_escapes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.peek().copied() {
                Some('[') => {
                    chars.next();
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' && chars.peek() == Some(&'\\') {
                            chars.next();
                            break;
                        }
                    }
                }
                Some(_) => {
                    chars.next();
                }
                None => {}
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Record an OMP Native local slash-command exchange (`/context`, `/help`, etc.) that `omp acp`
/// streamed via `agent_message_chunk` without writing rows to its session JSONL.
pub(crate) fn append_native_slash_exchange(session_id: &str, prompt: &str, response: &str) {
    let prompt = prompt.trim();
    let cleaned = strip_ansi_escapes(response);
    let response = cleaned.trim();
    if !is_safe_omp_session_id(session_id) || prompt.is_empty() || response.is_empty() {
        return;
    }
    let path = omp_local_path(session_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let user_row = json!({
        "id": format!("omp-local-{now_ms}-u"),
        "role": "user",
        "timestamp": now_ms,
        "text": prompt,
    });
    let assistant_row = json!({
        "id": format!("omp-local-{}-a", now_ms + 1),
        "role": "assistant",
        "timestamp": now_ms + 1,
        "text": response,
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{user_row}");
        let _ = writeln!(file, "{assistant_row}");
        let _ = file.flush();
    }
}

/// Reads any persisted OMP local slash-command rows associated with `file_path`.
pub(crate) fn read_local_slash_messages(file_path: &Path) -> Vec<SessionChatMessage> {
    let Some(session_id) = extract_omp_session_id_from_path(file_path) else {
        return Vec::new();
    };
    let Ok(raw) = fs::read_to_string(omp_local_path(&session_id)) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(val) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(id) = val
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let role = match val.get("role").and_then(Value::as_str) {
            Some("user") => SessionChatRole::User,
            Some("assistant") => SessionChatRole::Assistant,
            _ => continue,
        };
        let Some(text) = val.get("text").and_then(Value::as_str) else {
            continue;
        };
        if text.trim().is_empty() {
            continue;
        }
        let timestamp = val.get("timestamp").and_then(Value::as_i64);
        out.push(SessionChatMessage {
            id: id.to_string(),
            role,
            blocks: vec![SessionChatBlock::Text {
                text: text.to_string(),
            }],
            async_questions: None,
            timestamp,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            queued: false,
            api_error: None,
            custom_message: None,
        });
    }
    out
}
