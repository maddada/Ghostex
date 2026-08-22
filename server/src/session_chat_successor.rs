use std::collections::HashSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

use serde_json::{Map, Value};

use crate::session_chat::*;
use crate::session_chat_decode_claude::claude_content_blocks;
use crate::session_chat_paths::{
    is_claude_sidechain_transcript_name, read_transcript_head_complete_lines,
    CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES,
};
use crate::session_chat_tail::{find_last_complete_line_end, TailLineAccumulator};

/// Reverse-scan budget for "when did this transcript last carry a real
/// conversation row". The dead file in the 2026-08-02 repro had its last
/// substantive record 1941 bytes before EOF behind ~30 bare mode records, so
/// this window is generous while staying O(1).
const SUBSTANTIVE_TAIL_SCAN_BYTES: u64 = 1024 * 1024;
/// A resolved transcript with no `user`/`assistant` record for this long, while
/// a client is watching a RUNNING session, is not the conversation the pane is
/// driving any more. 90s clears the longest realistic gap inside a live turn
/// (a single long tool call still writes its `tool_result` user row when it
/// returns) without making recovery feel manual.
pub const SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS: i64 = 90_000;
/// Newest-first cap on head-scanned candidates per hop.
const SUCCESSOR_CANDIDATE_LIMIT: usize = 24;
/// A successor can itself be compacted; follow the chain but never loop.
const SUCCESSOR_CHAIN_LIMIT: usize = 8;
const CLAUDE_CONTINUATION_MARKER: &str =
    "This session is being continued from a previous conversation";

fn substantive_record_timestamp_ms(line: &str) -> Option<i64> {
    let record = parse_json_object(line)?;
    let record_type = record.get("type").and_then(Value::as_str)?;
    if record_type != "user" && record_type != "assistant" {
        return None;
    }
    if record.get("isSidechain") == Some(&Value::Bool(true)) {
        return None;
    }
    timestamp_ms(record.get("timestamp"))
}

/// Timestamp of the newest `user`/`assistant` record, found with the same
/// reverse chunked tail read the chat reader uses, bounded to
/// `SUBSTANTIVE_TAIL_SCAN_BYTES`. `None` means "no substantive record inside the
/// window" — callers must treat that as unknown, never as "stale".
pub fn last_substantive_transcript_timestamp_ms(file_path: &Path) -> Option<i64> {
    let file = File::open(file_path).ok()?;
    let size = file.metadata().ok()?.len();
    if size == 0 {
        return None;
    }
    let consumed_to = find_last_complete_line_end(&file, size).ok()?;
    if consumed_to == 0 {
        return None;
    }
    let mut trailing = [0u8; 1];
    read_exact_at(&file, &mut trailing, consumed_to - 1).ok()?;
    let mut cursor = consumed_to - u64::from(trailing[0] == b'\n');
    let floor = consumed_to.saturating_sub(SUBSTANTIVE_TAIL_SCAN_BYTES);
    let mut accumulator = TailLineAccumulator::new();
    let mut oversized_record_count = 0usize;
    let mut buffer = vec![0u8; TAIL_CHUNK_BYTES];
    while cursor > floor {
        let start = cursor.saturating_sub(TAIL_CHUNK_BYTES as u64).max(floor);
        let length = (cursor - start) as usize;
        read_exact_at(&file, &mut buffer[..length], start).ok()?;
        let mut segment_end = length;
        let mut index = length;
        while index > 0 {
            index -= 1;
            if buffer[index] != b'\n' {
                continue;
            }
            accumulator.retain_part(&buffer[index + 1..segment_end], &mut oversized_record_count);
            if accumulator.oversized {
                accumulator.reset();
            } else if let Some(line) = accumulator.take_line() {
                if let Some(timestamp) = substantive_record_timestamp_ms(&line) {
                    return Some(timestamp);
                }
            }
            segment_end = index;
        }
        if segment_end > 0 {
            accumulator.retain_part(&buffer[..segment_end], &mut oversized_record_count);
        }
        cursor = start;
    }
    if cursor == 0 {
        if let Some(line) = accumulator.take_line() {
            return substantive_record_timestamp_ms(&line);
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatSuccessorLineage {
    /// Head records carry the stale id in their own `sessionId`/`session_id`
    /// field — Claude copies the predecessor id into the resumed/compacted file.
    PredecessorIdField,
    /// Compact-continuation user record plus a `file-history-snapshot`
    /// inherited from the stale session.
    ContinuationSnapshot,
}

impl SessionChatSuccessorLineage {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionChatSuccessorLineage::PredecessorIdField => "predecessor-id-field",
            SessionChatSuccessorLineage::ContinuationSnapshot => "continuation-snapshot",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatSuccessorTranscript {
    pub agent_session_id: String,
    pub path: PathBuf,
    pub lineage: SessionChatSuccessorLineage,
    pub last_substantive_ms: i64,
    /// 1 = direct successor of the stale id, 2 = successor of that successor, …
    pub hops: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SessionChatSuccessorOutcome {
    /// No qualifying successor — keep tailing what we have.
    NotFound,
    /// A successor IS proven, but another live session is already bound to it.
    /// Reported separately so the log can say so: silently reporting `NotFound`
    /// is what made the first runtime failure invisible.
    OwnedByAnotherSession {
        candidate_session_ids: Vec<String>,
    },
    /// Several successors of the same predecessor are equally recent. Adopting
    /// either could bind the session to the wrong conversation, so adopt none.
    Ambiguous {
        predecessor_session_id: String,
        candidate_session_ids: Vec<String>,
    },
    Found(SessionChatSuccessorTranscript),
}

struct SuccessorHeadScan {
    declares_own_id: bool,
    predecessor_id_field: bool,
    continuation_marker: bool,
    predecessor_snapshot: bool,
}

impl SuccessorHeadScan {
    fn lineage(&self) -> Option<SessionChatSuccessorLineage> {
        if !self.declares_own_id {
            return None;
        }
        if self.predecessor_id_field {
            return Some(SessionChatSuccessorLineage::PredecessorIdField);
        }
        if self.continuation_marker && self.predecessor_snapshot {
            return Some(SessionChatSuccessorLineage::ContinuationSnapshot);
        }
        None
    }
}

fn claude_record_leading_text(record: &Map<String, Value>) -> Option<String> {
    let message = record.get("message").and_then(Value::as_object)?;
    claude_content_blocks(message.get("content"))
        .into_iter()
        .find_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text),
            _ => None,
        })
}

fn scan_successor_head(
    path: &Path,
    candidate_session_id: &str,
    predecessor_session_id: &str,
) -> SuccessorHeadScan {
    let mut scan = SuccessorHeadScan {
        declares_own_id: false,
        predecessor_id_field: false,
        continuation_marker: false,
        predecessor_snapshot: false,
    };
    let Some(head) = read_transcript_head_complete_lines(path, CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES)
    else {
        return scan;
    };
    for line in head.lines() {
        let Some(record) = parse_json_object(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        // Both spellings are read INDEPENDENTLY: a continuation file declares
        // its own id in `sessionId` and the predecessor's in `session_id`.
        for field in ["sessionId", "session_id"] {
            let Some(declared) = extract_string(record.get(field)) else {
                continue;
            };
            if declared == candidate_session_id {
                scan.declares_own_id = true;
            } else if declared == predecessor_session_id {
                scan.predecessor_id_field = true;
            }
        }
        match record.get("type").and_then(Value::as_str) {
            Some("user") => {
                if claude_record_leading_text(&record)
                    .is_some_and(|text| text.trim_start().starts_with(CLAUDE_CONTINUATION_MARKER))
                {
                    scan.continuation_marker = true;
                }
            }
            Some("file-history-snapshot") => {
                // The snapshot's tracked-file paths live under the predecessor's
                // own per-session scratch directory, so the id appears as data
                // rather than prose. Only this record type counts.
                if line.contains(predecessor_session_id) {
                    scan.predecessor_snapshot = true;
                }
            }
            _ => {}
        }
    }
    scan
}

pub(crate) fn is_uuid_transcript_stem(stem: &str) -> bool {
    let groups = [8usize, 4, 4, 4, 12];
    let mut parts = stem.split('-');
    for group in groups {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != group || !part.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return false;
        }
    }
    parts.next().is_none()
}

fn file_modified_ms(metadata: &fs::Metadata) -> i64 {
    #[cfg(unix)]
    {
        metadata.mtime() * 1_000 + metadata.mtime_nsec() / 1_000_000
    }
    #[cfg(windows)]
    {
        windows_filetime_to_unix_ms(metadata.last_write_time()) as i64
    }
}

fn collect_successor_candidates(
    directory: &Path,
    predecessor_session_id: &str,
    predecessor_last_substantive_ms: i64,
    visited_session_ids: &HashSet<String>,
) -> Vec<SessionChatSuccessorTranscript> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut prefiltered: Vec<(PathBuf, String, i64)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
            continue;
        }
        if is_claude_sidechain_transcript_name(&path) {
            continue;
        }
        let Some(stem) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if stem == predecessor_session_id
            || !is_uuid_transcript_stem(&stem)
            || visited_session_ids.contains(&stem)
        {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        // A successor's own records are written AFTER the predecessor's last
        // conversation row, so its mtime must be newer. mtime is used only as a
        // cheap inclusion prefilter here — never as a liveness signal.
        let modified_ms = file_modified_ms(&metadata);
        if modified_ms <= predecessor_last_substantive_ms {
            continue;
        }
        prefiltered.push((path, stem, modified_ms));
    }
    prefiltered.sort_by(|left, right| right.2.cmp(&left.2));
    prefiltered.truncate(SUCCESSOR_CANDIDATE_LIMIT);

    let mut qualified: Vec<SessionChatSuccessorTranscript> = Vec::new();
    for (path, stem, _) in prefiltered {
        let Some(lineage) = scan_successor_head(&path, &stem, predecessor_session_id).lineage()
        else {
            continue;
        };
        let Some(last_substantive_ms) = last_substantive_transcript_timestamp_ms(&path) else {
            continue;
        };
        // The successor continues the conversation, so it must carry rows newer
        // than the predecessor's last one.
        if last_substantive_ms <= predecessor_last_substantive_ms {
            continue;
        }
        qualified.push(SessionChatSuccessorTranscript {
            agent_session_id: stem,
            path,
            lineage,
            last_substantive_ms,
            hops: 0,
        });
    }
    qualified
}

/*
Walks the continuation chain from `stale_session_id` to its newest proven
successor.

`owned_session_ids` are ids bound to OTHER sessions that could actually be
tailing them — adopting one of those would steal a live conversation.

CDXC:SessionChatIdentity 2026-08-02 (bug fix, same day):
Ownership is checked AFTER the lineage proof, not as a pre-filter, and the
caller must pass ACTIVE owners only. The first cut excluded every id in the
registry and screened candidates out before the head scan: this machine's
registry holds 3487 stopped rows, two of which still carry the ids of the two
proven continuations, so the real repro silently produced `NotFound` with
nothing logged. A stopped session cannot be tailing anything; only running /
sleeping / provider-alive rows own an identity (`is_active_identity_owner`).
Rejecting after the proof also means the caller can SAY that a successor exists
but is owned, instead of the outcome being indistinguishable from "nothing on
disk".
*/
pub fn find_claude_successor_transcript(
    stale_session_id: &str,
    stale_path: &Path,
    stale_last_substantive_ms: i64,
    owned_session_ids: &[String],
) -> SessionChatSuccessorOutcome {
    let Some(directory) = stale_path.parent() else {
        return SessionChatSuccessorOutcome::NotFound;
    };
    let owned: HashSet<String> = owned_session_ids
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(stale_session_id.to_string());

    let mut predecessor_session_id = stale_session_id.to_string();
    let mut predecessor_last_substantive_ms = stale_last_substantive_ms;
    let mut found: Option<SessionChatSuccessorTranscript> = None;
    let mut owned_candidate_session_ids: Vec<String> = Vec::new();

    for hop in 1..=SUCCESSOR_CHAIN_LIMIT {
        let mut candidates = collect_successor_candidates(
            directory,
            &predecessor_session_id,
            predecessor_last_substantive_ms,
            &visited,
        );
        candidates.retain(|candidate| {
            if owned.contains(&candidate.agent_session_id) {
                owned_candidate_session_ids.push(candidate.agent_session_id.clone());
                return false;
            }
            true
        });
        if candidates.is_empty() {
            break;
        }
        candidates.sort_by(|left, right| right.last_substantive_ms.cmp(&left.last_substantive_ms));
        if candidates.len() > 1
            && candidates[0].last_substantive_ms == candidates[1].last_substantive_ms
        {
            /*
            Two continuations of one predecessor that stopped at the same
            instant: nothing on disk says which one the pane is running. Adopt
            none (the caller logs it once) unless an earlier hop already proved
            a successor, in which case that one still stands.

            This is the one place where terminal scrollback (a candidate uuid
            printed in the pane's last lines) could break the tie. It is
            deliberately NOT wired: statuslines are user-customised, so it can
            only ever confirm, never trigger, and a tie is rare enough that
            adopting nothing is the safe answer.
            */
            if found.is_none() {
                return SessionChatSuccessorOutcome::Ambiguous {
                    predecessor_session_id,
                    candidate_session_ids: candidates
                        .into_iter()
                        .map(|candidate| candidate.agent_session_id)
                        .collect(),
                };
            }
            break;
        }
        let mut chosen = candidates.remove(0);
        chosen.hops = hop;
        visited.insert(chosen.agent_session_id.clone());
        predecessor_session_id = chosen.agent_session_id.clone();
        predecessor_last_substantive_ms = chosen.last_substantive_ms;
        found = Some(chosen);
    }

    match found {
        Some(successor) => SessionChatSuccessorOutcome::Found(successor),
        None if !owned_candidate_session_ids.is_empty() => {
            SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: owned_candidate_session_ids,
            }
        }
        None => SessionChatSuccessorOutcome::NotFound,
    }
}

// ---------------------------------------------------------------------------
// Stream position (epoch/seq shared by the follower and /api/readSessionChat)
// ---------------------------------------------------------------------------

