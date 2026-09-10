//! CDXC:SessionStatus 2026-09-10 WHY:
//! Subagent liveness belongs to the latest persisted child turn, including turns resumed after an earlier completion. Read complete records from the tail and cache by file identity/version; a partial final write is not a new lifecycle event.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use crate::session_chat::{
    read_exact_at, read_transcript_file_version, SessionChatTranscriptAgent,
    SessionChatTurnLifecycleState, TranscriptFileVersion, TAIL_CHUNK_BYTES,
};
use crate::session_chat_tail::{find_last_complete_line_end, TailLineAccumulator};

#[derive(Clone, Debug)]
pub(crate) struct ChildTurn {
    pub working: bool,
    pub timestamp: i64,
    pub started_at: i64,
}

pub(crate) fn timestamp(record: &Value) -> Option<i64> {
    record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_time)
}

pub(crate) fn parse_time(value: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|at| at.timestamp_millis())
}

/// Returning false stops the scan. The first record is visited too, even in a one-line file.
pub(crate) fn scan_tail(
    path: &Path,
    mut visit: impl FnMut(&str) -> io::Result<bool>,
) -> io::Result<()> {
    let file = File::open(path)?;
    let end = find_last_complete_line_end(&file, file.metadata()?.len())?;
    let mut cursor = end.saturating_sub(1);
    let mut line = TailLineAccumulator::new();
    let mut oversized = 0;
    let mut buffer = vec![0; TAIL_CHUNK_BYTES];
    while cursor > 0 {
        let start = cursor.saturating_sub(TAIL_CHUNK_BYTES as u64);
        let length = (cursor - start) as usize;
        read_exact_at(&file, &mut buffer[..length], start)?;
        let mut segment_end = length;
        for index in (0..length).rev() {
            if buffer[index] != b'\n' {
                continue;
            }
            line.retain_part(&buffer[index + 1..segment_end], &mut oversized);
            if line.oversized {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Subagent lifecycle record is too large",
                ));
            }
            if let Some(record) = line.take_line() {
                if !visit(&record)? {
                    return Ok(());
                }
            }
            segment_end = index;
        }
        line.retain_part(&buffer[..segment_end], &mut oversized);
        cursor = start;
    }
    if line.oversized {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Subagent lifecycle record is too large",
        ));
    }
    if let Some(record) = line.take_line() {
        visit(&record)?;
    }
    Ok(())
}

type Cache = HashMap<PathBuf, (TranscriptFileVersion, Option<ChildTurn>)>;

pub(crate) fn latest_child_turn(
    path: &Path,
    agent: SessionChatTranscriptAgent,
) -> io::Result<Option<ChildTurn>> {
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let cache = CACHE.get_or_init(Mutex::default);
    let version = read_transcript_file_version(path)?;
    if let Some((_, turn)) = cache
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).filter(|(old, _)| old == &version).cloned())
    {
        return Ok(turn);
    }
    let mut turn: Option<ChildTurn> = None;
    scan_tail(path, |line| {
        let record: Value = serde_json::from_str(line)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let Some(at) = timestamp(&record) else {
            return Ok(true);
        };
        if agent == SessionChatTranscriptAgent::Codex {
            if let Some(event) =
                crate::session_chat_decode_codex::decode_codex_turn_lifecycle(line, "")
            {
                turn = Some(ChildTurn {
                    working: event.state == SessionChatTurnLifecycleState::Working,
                    timestamp: at,
                    started_at: at,
                });
                return Ok(false);
            }
            return Ok(true);
        }
        let event = crate::session_chat_decode_claude::decode_claude_turn_lifecycle(line, "");
        if event
            .as_ref()
            .is_some_and(|event| event.state != SessionChatTurnLifecycleState::Working)
        {
            if turn.is_none() {
                turn = Some(ChildTurn {
                    working: false,
                    timestamp: at,
                    started_at: at,
                });
            }
            return Ok(false);
        }
        let kind = record.get("type").and_then(Value::as_str);
        let blocks = record.pointer("/message/content");
        let child_message = |text: &str| {
            text.trim_start().starts_with("<agent-message")
                || text.trim_start().starts_with("<teammate-message")
        };
        let resumes_child = blocks.and_then(Value::as_str).is_some_and(child_message)
            || blocks.and_then(Value::as_array).is_some_and(|blocks| {
                blocks.iter().any(|block| {
                    block.get("type").and_then(Value::as_str) == Some("text")
                        && block
                            .get("text")
                            .and_then(Value::as_str)
                            .is_some_and(child_message)
                })
            });
        // Compaction summaries and completion notifications are not new child prompts.
        let user_prompt = kind == Some("user")
            && (resumes_child
                || event
                    .is_some_and(|event| event.state == SessionChatTurnLifecycleState::Working));
        if kind == Some("assistant") || user_prompt {
            let latest = turn.get_or_insert(ChildTurn {
                working: true,
                timestamp: at,
                started_at: at,
            });
            latest.started_at = at;
            if user_prompt {
                return Ok(false);
            }
        }
        Ok(true)
    })?;
    if let Ok(mut cache) = cache.lock() {
        if cache.len() >= 4096 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (version, turn.clone()));
    }
    Ok(turn)
}
