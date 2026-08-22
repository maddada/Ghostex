use std::collections::{HashSet, VecDeque};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde_json::Value;

use crate::session_chat::*;
use crate::session_chat_decode_pi::decode_pi_transcript_line;

fn superseded_prompt_id(
    lineage: &TranscriptLineage,
    message: Option<&SessionChatMessage>,
    parents_of_newer_messages: &HashSet<String>,
    parents_of_newer_prompts: &HashSet<String>,
) -> Option<String> {
    if message?.role != SessionChatRole::User {
        return None;
    }
    let parent_id = lineage.parent_id.as_ref()?;
    // Children are always written after their parent, so in a newest-first scan
    // they have already been seen.
    if parents_of_newer_messages.contains(&lineage.id) {
        return None;
    }
    parents_of_newer_prompts
        .contains(parent_id)
        .then(|| lineage.id.clone())
}

// ---------------------------------------------------------------------------
// Shared primitives (upstream chat spec §1)
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct SessionChatTailFileResult {
    pub messages: Vec<SessionChatMessage>,
    pub lifecycle: Option<SessionChatTurnLifecycle>,
    pub consumed_to: u64,
    pub has_more: bool,
    pub before_offset: u64,
    pub malformed_record_count: usize,
    pub oversized_record_count: usize,
    /*
    CDXC:SessionChatCore 2026-08-19:
    Prompts still sitting in the agent's queue at `consumed_to`, oldest first,
    as `(normalized text, message id)`. The follower seeds the append stream
    with these after every snapshot: `rebase` clears the forward state, so
    without the hand-off the release row that lands a second later has nothing
    to retract and the "Queued" label sticks forever.
    */
    pub outstanding_queued_prompts: Vec<(String, String)>,
}

pub(crate) struct TailLineAccumulator {
    // Reverse-ordered parts of the line currently being assembled.
    parts: Vec<Vec<u8>>,
    pub(crate) bytes: usize,
    pub(crate) oversized: bool,
}

impl TailLineAccumulator {
    pub(crate) fn new() -> Self {
        Self {
            parts: Vec::new(),
            bytes: 0,
            oversized: false,
        }
    }

    pub(crate) fn retain_part(&mut self, part: &[u8], oversized_record_count: &mut usize) {
        if self.oversized {
            return;
        }
        self.bytes += part.len();
        if self.bytes > MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES {
            self.parts.clear();
            self.oversized = true;
            *oversized_record_count += 1;
        } else {
            self.parts.push(part.to_vec());
        }
    }

    pub(crate) fn take_line(&mut self) -> Option<String> {
        let mut bytes: Vec<u8> = Vec::with_capacity(self.bytes);
        for part in self.parts.iter().rev() {
            bytes.extend_from_slice(part);
        }
        self.reset();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    pub(crate) fn reset(&mut self) {
        self.parts.clear();
        self.bytes = 0;
        self.oversized = false;
    }
}

pub(crate) fn find_last_complete_line_end(file: &File, end: u64) -> std::io::Result<u64> {
    if end == 0 {
        return Ok(0);
    }
    let mut last = [0u8; 1];
    read_exact_at(file, &mut last, end - 1)?;
    if last[0] == b'\n' {
        return Ok(end);
    }
    let mut cursor = end - 1;
    let mut buffer = vec![0u8; TAIL_CHUNK_BYTES];
    while cursor > 0 {
        let start = cursor.saturating_sub(TAIL_CHUNK_BYTES as u64);
        let length = (cursor - start) as usize;
        read_exact_at(file, &mut buffer[..length], start)?;
        for index in (0..length).rev() {
            if buffer[index] == b'\n' {
                return Ok(start + index as u64 + 1);
            }
        }
        cursor = start;
    }
    Ok(0)
}

pub fn read_session_chat_transcript_tail_file(
    file_path: &Path,
    limit: usize,
    decode: SessionChatLineDecoder,
    include_trailing_line: bool,
    end_offset: Option<u64>,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
    lineage: Option<SessionChatLineageExtractor>,
) -> std::io::Result<SessionChatTailFileResult> {
    let file = File::open(file_path)?;
    let file_size = file.metadata()?.len();
    let end = file_size.min(end_offset.unwrap_or(u64::MAX));
    if end == 0 {
        return Ok(SessionChatTailFileResult::default());
    }
    let consumed_to = if include_trailing_line {
        end
    } else {
        find_last_complete_line_end(&file, end)?
    };
    if consumed_to == 0 {
        return Ok(SessionChatTailFileResult::default());
    }

    let mut trailing = [0u8; 1];
    read_exact_at(&file, &mut trailing, consumed_to - 1)?;
    // A window that does not end on a newline means the first-decoded (newest)
    // record is a partial write — tolerate one malformed record silently.
    let mut ignore_next_malformed_record = trailing[0] != b'\n';
    let mut cursor = consumed_to - u64::from(trailing[0] == b'\n');

    let mut accumulator = TailLineAccumulator::new();
    let mut newest_first: Vec<(SessionChatMessage, u64)> = Vec::new();
    let mut lifecycle: Option<SessionChatTurnLifecycle> = None;
    let mut malformed_record_count = 0usize;
    let mut oversized_record_count = 0usize;

    let mut parents_of_newer_messages: HashSet<String> = HashSet::new();
    let mut parents_of_newer_prompts: HashSet<String> = HashSet::new();
    // Newest-first; replayed in file order once the window is complete.
    let mut queue_ops: Vec<(u64, TranscriptQueueOp)> = Vec::new();

    let decode_line = |accumulator: &mut TailLineAccumulator,
                       line_offset: u64,
                       newest_first: &mut Vec<(SessionChatMessage, u64)>,
                       lifecycle: &mut Option<SessionChatTurnLifecycle>,
                       ignore_next_malformed_record: &mut bool,
                       malformed_record_count: &mut usize,
                       parents_of_newer_messages: &mut HashSet<String>,
                       parents_of_newer_prompts: &mut HashSet<String>,
                       queue_ops: &mut Vec<(u64, TranscriptQueueOp)>| {
        let Some(line) = accumulator.take_line() else {
            return;
        };
        if serde_json::from_str::<Value>(&line).is_err() {
            if *ignore_next_malformed_record {
                *ignore_next_malformed_record = false;
                return;
            }
            *malformed_record_count += 1;
            return;
        }
        *ignore_next_malformed_record = false;
        let fallback_id = transcript_fallback_id(file_path, line_offset);
        if lifecycle.is_none() {
            if let Some(decode_lifecycle) = decode_lifecycle {
                *lifecycle = decode_lifecycle(&line, &fallback_id);
            }
        }
        let decoded = decode(&line, &fallback_id);
        let row_lineage = lineage.and_then(|extract| extract(&line, &fallback_id));
        if let Some(mut row_lineage) = row_lineage {
            if let Some(queue_op) = row_lineage.queue.take() {
                queue_ops.push((line_offset, queue_op));
                if let Some(mut message) = decoded {
                    message.byte_offset = Some(line_offset);
                    newest_first.push((message, line_offset));
                }
                return;
            }
            let superseded = superseded_prompt_id(
                &row_lineage,
                decoded.as_ref(),
                parents_of_newer_messages,
                parents_of_newer_prompts,
            )
            .is_some();
            if let Some(parent_id) = row_lineage.parent_id {
                if let Some(message) = decoded.as_ref() {
                    if message.role == SessionChatRole::User {
                        parents_of_newer_prompts.insert(parent_id.clone());
                    }
                    parents_of_newer_messages.insert(parent_id);
                }
            }
            if superseded {
                return;
            }
        }
        if let Some(mut message) = decoded {
            message.byte_offset = Some(line_offset);
            newest_first.push((message, line_offset));
        }
    };

    let mut buffer = vec![0u8; TAIL_CHUNK_BYTES];
    while cursor > 0 && newest_first.len() <= limit {
        let start = cursor.saturating_sub(TAIL_CHUNK_BYTES as u64);
        let length = (cursor - start) as usize;
        read_exact_at(&file, &mut buffer[..length], start)?;
        let mut segment_end = length;
        let mut index = length;
        while index > 0 && newest_first.len() <= limit {
            index -= 1;
            if buffer[index] != b'\n' {
                continue;
            }
            accumulator.retain_part(&buffer[index + 1..segment_end], &mut oversized_record_count);
            if accumulator.oversized {
                accumulator.reset();
            } else {
                decode_line(
                    &mut accumulator,
                    start + index as u64 + 1,
                    &mut newest_first,
                    &mut lifecycle,
                    &mut ignore_next_malformed_record,
                    &mut malformed_record_count,
                    &mut parents_of_newer_messages,
                    &mut parents_of_newer_prompts,
                    &mut queue_ops,
                );
            }
            segment_end = index;
        }
        if segment_end > 0 {
            accumulator.retain_part(&buffer[..segment_end], &mut oversized_record_count);
        }
        cursor = start;
    }
    if cursor == 0 && !accumulator.parts.is_empty() && newest_first.len() <= limit {
        decode_line(
            &mut accumulator,
            0,
            &mut newest_first,
            &mut lifecycle,
            &mut ignore_next_malformed_record,
            &mut malformed_record_count,
            &mut parents_of_newer_messages,
            &mut parents_of_newer_prompts,
            &mut queue_ops,
        );
    }

    newest_first.reverse();
    queue_ops.reverse();
    let outstanding = replay_transcript_queue(&queue_ops);
    let still_queued: HashSet<u64> = outstanding.iter().map(|(_, offset)| *offset).collect();
    let outstanding_queued_prompts: Vec<(String, String)> = outstanding
        .iter()
        .map(|(key, offset)| (key.clone(), transcript_fallback_id(file_path, *offset)))
        .collect();
    // A released entry's bubble is replaced by the row the harness wrote for
    // the delivery, so it must not survive in the window that carries both.
    newest_first.retain(|(message, offset)| !message.queued || still_queued.contains(offset));
    let chronological = newest_first;
    let selected: Vec<(SessionChatMessage, u64)> = if limit > 0 {
        // limit <= 0 must yield [] — a slice(-0) style bug would return EVERYTHING.
        let skip = chronological.len().saturating_sub(limit);
        chronological.iter().skip(skip).cloned().collect()
    } else {
        Vec::new()
    };
    let has_more = limit > 0 && chronological.len() > limit;
    let before_offset = selected.first().map(|(_, offset)| *offset).unwrap_or(end);
    Ok(SessionChatTailFileResult {
        messages: selected.into_iter().map(|(message, _)| message).collect(),
        lifecycle,
        consumed_to,
        has_more,
        before_offset,
        malformed_record_count,
        oversized_record_count,
        outstanding_queued_prompts,
    })
}

/// Replays queue bookkeeping rows in file order and returns what is still
/// waiting, oldest first, as `(normalized text, enqueue byte offset)`.
///
/// A removal whose text matches nothing is IGNORED rather than treated as a
/// FIFO pop: in a bounded window its enqueue is usually just off the top, and
/// popping would retract a prompt that is genuinely still queued.
fn replay_transcript_queue(ops: &[(u64, TranscriptQueueOp)]) -> Vec<(String, u64)> {
    let mut queue: VecDeque<(String, u64)> = VecDeque::new();
    for (offset, op) in ops {
        match op {
            TranscriptQueueOp::Enqueued { key } => queue.push_back((key.clone(), *offset)),
            TranscriptQueueOp::Left { key: Some(key) } => {
                if let Some(index) = queue.iter().position(|(queued, _)| queued == key) {
                    queue.remove(index);
                }
            }
            TranscriptQueueOp::Left { key: None } => {
                queue.pop_front();
            }
            TranscriptQueueOp::Cleared => queue.clear(),
        }
    }
    queue.into_iter().collect()
}

struct PiTranscriptTreeEntry {
    id: String,
    parent_id: Option<String>,
    line: String,
    offset: u64,
}

/*
Pi-family session files are append-only trees. The final entry is the current
leaf; chat history is the root-to-leaf ancestry, not every line ever appended
to the file. Read the tree once, then apply the same tail/pagination contract
as the linear transcript reader to the decoded active branch.
*/
pub(crate) fn read_pi_session_chat_transcript_tail_file(
    file_path: &Path,
    limit: usize,
    include_trailing_line: bool,
    end_offset: Option<u64>,
) -> std::io::Result<SessionChatTailFileResult> {
    let file = File::open(file_path)?;
    let file_size = file.metadata()?.len();
    if file_size == 0 {
        return Ok(SessionChatTailFileResult::default());
    }
    let selection_end = file_size.min(end_offset.unwrap_or(u64::MAX));
    let mut reader = BufReader::new(file.take(file_size));
    let mut entries = Vec::new();
    let mut offset = 0u64;
    let mut consumed_to = 0u64;
    let mut malformed_record_count = 0usize;
    let mut oversized_record_count = 0usize;
    loop {
        let line_offset = offset;
        let mut bytes = Vec::new();
        let read = reader.read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        offset += read as u64;
        let newline_terminated = bytes.last() == Some(&b'\n');
        if !newline_terminated && !include_trailing_line {
            break;
        }
        consumed_to = offset;
        if bytes.len() > MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES {
            oversized_record_count += 1;
            continue;
        }
        if newline_terminated {
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
        }
        if bytes.is_empty() {
            continue;
        }
        let line = String::from_utf8_lossy(&bytes).into_owned();
        let Some(record) = serde_json::from_str::<Value>(&line)
            .ok()
            .and_then(|value| value.as_object().cloned())
        else {
            malformed_record_count += 1;
            continue;
        };
        let Some(id) = extract_string(record.get("id")) else {
            continue;
        };
        entries.push(PiTranscriptTreeEntry {
            id,
            parent_id: extract_string(record.get("parentId")),
            line,
            offset: line_offset,
        });
    }

    let by_id = entries
        .iter()
        .map(|entry| (entry.id.as_str(), entry))
        .collect::<std::collections::HashMap<_, _>>();
    let mut active_ids = HashSet::new();
    let mut cursor = entries.last().map(|entry| entry.id.as_str());
    for _ in 0..=entries.len() {
        let Some(id) = cursor else {
            break;
        };
        if !active_ids.insert(id.to_string()) {
            break;
        }
        cursor = by_id.get(id).and_then(|entry| entry.parent_id.as_deref());
    }

    let mut chronological = Vec::new();
    for entry in entries {
        if entry.offset >= selection_end || !active_ids.contains(&entry.id) {
            continue;
        }
        let fallback_id = transcript_fallback_id(file_path, entry.offset);
        if let Some(mut message) = decode_pi_transcript_line(&entry.line, &fallback_id) {
            message.byte_offset = Some(entry.offset);
            chronological.push((message, entry.offset));
        }
    }
    let selected: Vec<(SessionChatMessage, u64)> = if limit > 0 {
        let skip = chronological.len().saturating_sub(limit);
        chronological.iter().skip(skip).cloned().collect()
    } else {
        Vec::new()
    };
    let has_more = limit > 0 && chronological.len() > limit;
    let before_offset = selected
        .first()
        .map(|(_, offset)| *offset)
        .unwrap_or(selection_end);
    Ok(SessionChatTailFileResult {
        messages: selected.into_iter().map(|(message, _)| message).collect(),
        lifecycle: None,
        consumed_to,
        has_more,
        before_offset,
        malformed_record_count,
        oversized_record_count,
        // Pi has no prompt queue.
        outstanding_queued_prompts: Vec::new(),
    })
}

fn read_session_chat_transcript_tail_file_for_agent(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
    limit: usize,
    decode: SessionChatLineDecoder,
    include_trailing_line: bool,
    end_offset: Option<u64>,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
) -> std::io::Result<SessionChatTailFileResult> {
    if agent == SessionChatTranscriptAgent::Pi {
        read_pi_session_chat_transcript_tail_file(
            file_path,
            limit,
            include_trailing_line,
            end_offset,
        )
    } else {
        read_session_chat_transcript_tail_file(
            file_path,
            limit,
            decode,
            include_trailing_line,
            end_offset,
            decode_lifecycle,
            session_chat_lineage_extractor(agent),
        )
    }
}

/// Pagination wrapper (upstream chat spec §4). `include_trailing_line = true` so a live
/// read can decode a torn final line's completed predecessors.
#[derive(Debug)]
pub enum SessionChatTailPage {
    NotFound,
    Page {
        messages: Vec<SessionChatMessage>,
        /// Omitted on older pagination pages — they must never rewind the live lifecycle.
        lifecycle: Option<SessionChatTurnLifecycle>,
        has_more: bool,
        before_offset: u64,
    },
}

pub fn read_session_chat_tail_page(
    agent: SessionChatTranscriptAgent,
    file_path: &Path,
    limit: usize,
    before_offset: Option<u64>,
) -> std::io::Result<SessionChatTailPage> {
    let decode = session_chat_line_decoder(agent);
    let decode_lifecycle = session_chat_lifecycle_decoder(agent);
    match read_session_chat_transcript_tail_file_for_agent(
        agent,
        file_path,
        limit,
        decode,
        true,
        before_offset,
        decode_lifecycle,
    ) {
        Ok(result) => Ok(SessionChatTailPage::Page {
            messages: result.messages,
            lifecycle: if before_offset.is_none() {
                result.lifecycle
            } else {
                None
            },
            has_more: result.has_more,
            before_offset: result.before_offset,
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(SessionChatTailPage::NotFound)
        }
        Err(error) => Err(error),
    }
}

// ---------------------------------------------------------------------------
// Forward incremental reader (upstream chat spec §5.11)
// ---------------------------------------------------------------------------

