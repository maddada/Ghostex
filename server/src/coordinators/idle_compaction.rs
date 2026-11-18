//! A Claude session whose turn ended and that then compacted on its own while it sat idle.
//!
//! CDXC:Coordinators 2026-11-18 WHY:
//! Claude Code 2.1.326 compacts an idle session by itself shortly before its prompt cache expires (about 55 minutes after the turn ended, for a conversation over about 200k tokens) and writes `compact_boundary`, a compact summary, attachments, a `SessionStart:compact` hook row and the notice "Compacted while idle, before the prompt cache expired". On 2026-11-17 three finished coordinator threads (G9c10, G487s; quiet since the second their idle compaction ended) were read as working again and got "no new output from you for 8 minutes" nudges and heal-it reports, each nudge costing a turn. The compaction's hooks map to no activity (SessionStart source compact and an auto PostCompact stay unmapped in agent_hooks/event_mapping.rs) and its rows open no turn in the transcript, so the session's working came from outside the transcript: most likely the screen-owned compaction marker (`sessionChatCompactingDetectedAt`, session_chat_options/detector.rs), which a capture during the compaction sets and only a later whole capture clears, and nothing captures an idle session nobody watches. The stall watches therefore check the transcript itself: a turn that ended, then only an idle compaction after it, is a finished session, whatever its presented activity says. A compaction in the middle of a turn is not one: the turn's prompt is the newest turn row before its boundary.
//! SEE-ALSO: server/src/server/coordinator_runtime.rs (thread state), server/src/server/session_stall_runtime.rs (`watched`).

use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde_json::Value;

/// How much of the transcript's end is read: an idle compaction writes about 140 KB of rows
/// (the summary and the re-announced skills, tools and files).
const TAIL_BYTES: u64 = 512 * 1024;

type Stamp = (u64, Option<SystemTime>);

fn answers() -> &'static Mutex<HashMap<PathBuf, (Stamp, bool)>> {
    static ANSWERS: OnceLock<Mutex<HashMap<PathBuf, (Stamp, bool)>>> = OnceLock::new();
    ANSWERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// BLOCKING (a `stat`, and a tail read only when the transcript changed since the last look).
/// Whether a Claude session's transcript ends with a finished turn followed only by a compaction
/// Claude ran while it sat idle.
pub fn idle_compacted_after_turn_end(session: &Value) -> bool {
    let mut path = None;
    let agent = super::delivery::resolve_thread_transcript(session, &mut path);
    if agent != Some(crate::session_chat::SessionChatTranscriptAgent::Claude) {
        return false;
    }
    let Some(path) = path else {
        return false;
    };
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    let stamp = (metadata.len(), metadata.modified().ok());
    if let Some((known, answer)) = answers()
        .lock()
        .ok()
        .and_then(|answers| answers.get(&path).copied())
    {
        if known == stamp {
            return answer;
        }
    }
    let answer = read_tail(&path, stamp.0).is_some_and(|tail| ends_with_idle_compaction(&tail));
    if let Ok(mut answers) = answers().lock() {
        if answers.len() > 512 {
            answers.clear();
        }
        answers.insert(path, (stamp, answer));
    }
    answer
}

fn read_tail(path: &Path, length: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(length.saturating_sub(TAIL_BYTES)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Walks the rows from the newest: after the compaction only its own rows may follow (attachments,
/// the compact summary, meta rows, Claude's notices), and the newest turn row before its
/// `compact_boundary` must be the end of a turn.
fn ends_with_idle_compaction(tail: &str) -> bool {
    let mut boundary_seen = false;
    for line in tail.lines().rev() {
        if ![
            "\"type\":\"system\"",
            "\"type\":\"assistant\"",
            "\"type\":\"user\"",
        ]
        .iter()
        .any(|marker| line.contains(marker))
        {
            continue;
        }
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kind = row.get("type").and_then(Value::as_str).unwrap_or_default();
        let subtype = row
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match kind {
            "system" if subtype == "compact_boundary" => {
                if boundary_seen {
                    // Two compactions in a row with no turn between them: still idle.
                    continue;
                }
                boundary_seen = true;
            }
            "system"
                if boundary_seen && matches!(subtype, "turn_duration" | "stop_hook_summary") =>
            {
                return true;
            }
            "system" => {}
            "assistant" if boundary_seen => {
                let message = row.get("message");
                let stop_reason = message
                    .and_then(|message| message.get("stop_reason"))
                    .and_then(Value::as_str);
                return matches!(
                    stop_reason,
                    Some("end_turn" | "stop_sequence" | "max_tokens")
                );
            }
            "assistant" => return false,
            "user" => {
                let injected = row.get("isMeta") == Some(&Value::Bool(true))
                    || row.get("isCompactSummary") == Some(&Value::Bool(true))
                    || row.get("isVisibleInTranscriptOnly") == Some(&Value::Bool(true));
                if !injected {
                    // A prompt or a tool's result: a turn after the compaction, or the one it
                    // compacted in the middle of.
                    return false;
                }
            }
            _ => {}
        }
    }
    false
}
