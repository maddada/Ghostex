//! Ghostex Collab's audit log: every guest message (who, when, the words, how many pictures, and
//! what happened to it: sent straight to the agent, held, sent by the owner, edited, declined or
//! refused) and every join, leave and removal, one JSON line each in `collab-audit.jsonl`. Settings
//! > Collab and the phone read it newest first (`/api/collab` `op: "audit"`).
//!
//! CDXC:Collab 2026-11-07 WHY: the design page's "who told the agent to delete that?" risk: a shared chat's agent can act on a guest's words, so what each guest said and what the owner did with it stays on record. Kept to the last 5,000 lines.

use std::io::Write as _;
use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::{json, Value};

use super::shares::StoredShare;

const AUDIT_FILE: &str = "collab-audit.jsonl";
const KEEP_LINES: usize = 5_000;

fn lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn file() -> std::path::PathBuf {
    crate::paths::get_gxserver_paths(None)
        .app_state_dir
        .join(AUDIT_FILE)
}

/// Appends one entry; `event` is `joined`, `left`, `removed` or `message`, `outcome` (messages
/// only) `sent`, `held`, `approved`, `edited`, `declined` or `refused`.
pub fn record(
    share: &StoredShare,
    title: &str,
    event: &str,
    guest: &str,
    text: Option<&str>,
    images: usize,
    outcome: Option<&str>,
) {
    let mut entry = json!({
        "atMs": chrono::Utc::now().timestamp_millis(),
        "shareId": share.id,
        "projectId": share.project_id,
        "sessionId": share.session_id,
        "title": title,
        "event": event,
        "guest": guest,
    });
    let mut images = images;
    if let Some(text) = text {
        // Pictures pasted into the chat box are references in the words; the log counts them.
        let (words, referenced) = super::split_image_references(text);
        images += referenced.len();
        entry["text"] = json!(words.chars().take(4_000).collect::<String>());
    }
    if images > 0 {
        entry["images"] = json!(images);
    }
    if let Some(outcome) = outcome {
        entry["outcome"] = json!(outcome);
    }
    let _guard = lock().lock().unwrap_or_else(PoisonError::into_inner);
    let path = file();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut out) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(out, "{entry}");
    }
    // Trim now and then, not on every line.
    if rand::random::<u8>() < 8 {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = text.lines().collect();
            if lines.len() > KEEP_LINES {
                let kept = lines[lines.len() - KEEP_LINES..].join("\n");
                let _ = std::fs::write(&path, format!("{kept}\n"));
            }
        }
    }
}

/// The newest `limit` entries, newest first.
pub fn read_recent(limit: usize) -> Vec<Value> {
    let _guard = lock().lock().unwrap_or_else(PoisonError::into_inner);
    let Ok(text) = std::fs::read_to_string(file()) else {
        return Vec::new();
    };
    text.lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .take(limit)
        .collect()
}
