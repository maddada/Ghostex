//! The cross-source assembler: id first, then a text-derived turn key that merges ONLY across
//! different sources.
//!
//! Ported from `packages/core-ui/chat/session-chat-assembler.ts`. Two identical same-source
//! prompts ("continue" twice) must stay distinct, which is why the turn key is guarded by a source
//! comparison rather than used on its own.
//!
//! Correctness invariant the TypeScript locks with a test: the append path's output equals a full
//! rebuild over base plus every append, for every prefix. The Rust keeps it by routing both
//! through [`merge_one`].

use std::cmp::Ordering;
use std::collections::BTreeMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatSource};

use crate::session::constants::{
    LAUNCH_PENDING_ID_PREFIX, PENDING_ID_PREFIX, STREAMING_ID, TERMINAL_TOOL_ID_PREFIX,
};

/// One message plus its position in the file-ordered transport list.
///
/// The TypeScript kept that position in a `WeakMap` keyed by object identity
/// (`stampSessionChatArrivalOrder`). Rust has no object identity to key on, so the stamp rides
/// with the message instead. It is a tie-break only: rows the server stamped with a byte offset
/// never consult it.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub message: ChatMessage,
    pub arrival: Option<usize>,
}

impl Row {
    /// A row with no file position, which is every client- and hook-sourced message.
    pub fn new(message: ChatMessage) -> Self {
        Self {
            message,
            arrival: None,
        }
    }

    /// The transport list, stamped with its own order.
    pub fn stamped(messages: &[ChatMessage]) -> Vec<Self> {
        messages
            .iter()
            .enumerate()
            .map(|(index, message)| Self {
                message: message.clone(),
                arrival: Some(index),
            })
            .collect()
    }
}

/// `SESSION_CHAT_SOURCE_PRIORITY`: transcript wins over hook wins over client.
pub fn source_priority(source: &ChatSource) -> u8 {
    match source {
        ChatSource::Transcript => 3,
        ChatSource::Hook => 2,
        ChatSource::Client => 1,
        ChatSource::Other(_) => 0,
    }
}

fn stable_stringify(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

fn non_text_block_digest(message: &ChatMessage) -> String {
    let mut parts: Vec<String> = Vec::new();
    for block in &message.blocks {
        match block {
            ChatBlock::ToolCall { name, input } => {
                parts.push(format!("call:{name}:{}", stable_stringify(input)));
            }
            ChatBlock::ToolResult { output, .. } => parts.push(format!("result:{output}")),
            ChatBlock::ImageRef { path, url, alt, .. } => {
                let value = path
                    .as_deref()
                    .or(url.as_deref())
                    .or(alt.as_deref())
                    .unwrap_or_default();
                parts.push(format!("image:{value}"));
            }
            ChatBlock::Text { .. } | ChatBlock::Unknown => {}
        }
    }
    parts.join("|")
}

fn text_of(message: &ChatMessage, separator: &str) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            ChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(separator)
}

/// `sessionChatTurnKey`.
pub fn turn_key(message: &ChatMessage) -> String {
    if let Some(turn_id) = message.turn_id.as_ref().filter(|value| !value.is_empty()) {
        return format!("turn:{turn_id}");
    }
    let text = crate::session::text::collapse_whitespace(&text_of(message, " ").to_lowercase());
    format!(
        "{}:{}:{}",
        message.role.as_str(),
        text,
        non_text_block_digest(message)
    )
}

/// `turnKeyDigest`: FNV-1a over the UTF-16 code units, printed base 36.
fn turn_key_digest(key: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for unit in key.encode_utf16() {
        hash ^= u32::from(unit);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    base36(hash)
}

fn base36(mut value: u32) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `sessionChatShadowedId`.
///
/// Transcript rows that carry no record uuid fall back to the API response id, which every row of
/// one response shares. Two DISTINCT rows then collide on one id; re-keying the second keeps both.
/// Derived from the row's byte offset, or its content when the server did not stamp one, never
/// from a counter, so every read path produces the same id for the same row.
pub fn shadowed_id(message: &ChatMessage) -> String {
    match message.byte_offset {
        None => format!("{}#{}", message.id, turn_key_digest(&turn_key(message))),
        Some(offset) => format!("{}@{offset}", message.id),
    }
}

/// True when `incoming` is a different row that merely shares `existing`'s id, as opposed to the
/// same row re-emitted by another read path.
pub fn id_collides(existing: &ChatMessage, incoming: &ChatMessage) -> bool {
    if existing.id != incoming.id {
        return false;
    }
    match (existing.byte_offset, incoming.byte_offset) {
        (Some(left), Some(right)) => left != right,
        _ => turn_key(existing) != turn_key(incoming),
    }
}

/// Strict greater-than: an equal-priority cross-source duplicate never replaces.
fn supersedes(candidate: &ChatMessage, existing: &ChatMessage) -> bool {
    source_priority(&candidate.source) > source_priority(&existing.source)
}

/// `sessionChatMessageSortRank`: three tiers above the transcript.
///
/// Tiering exists because the streaming preview has `timestamp: null` and would sort to the front,
/// and optimistic echoes carry a real send time and would sort past the preview. The pending tool
/// row is stamped with the time gxserver first saw it painted, which falls between a tool call and
/// its result; the row is the transcript's tail by definition, so it ranks after every transcript
/// row instead of between those two.
pub fn sort_rank(message: &ChatMessage) -> u8 {
    if message.id == STREAMING_ID {
        return 1;
    }
    if message.id.starts_with(TERMINAL_TOOL_ID_PREFIX) {
        return 2;
    }
    if message.id.starts_with(PENDING_ID_PREFIX) || message.id.starts_with(LAUNCH_PENDING_ID_PREFIX)
    {
        return 3;
    }
    0
}

/// `compareSessionChatMessages`: tier, then timestamp, then file order, then id.
pub fn compare(left: &Row, right: &Row) -> Ordering {
    let rank = sort_rank(&left.message).cmp(&sort_rank(&right.message));
    if rank != Ordering::Equal {
        return rank;
    }
    // `null` sorts before any timestamp, which is what `Number.NEGATIVE_INFINITY` does there.
    let stamp = left
        .message
        .timestamp
        .unwrap_or(i64::MIN)
        .cmp(&right.message.timestamp.unwrap_or(i64::MIN));
    if stamp != Ordering::Equal {
        return stamp;
    }
    match (left.message.byte_offset, right.message.byte_offset) {
        (Some(a), Some(b)) if a != b => return a.cmp(&b),
        (Some(_), Some(_)) => {}
        _ => {
            if let (Some(a), Some(b)) = (left.arrival, right.arrival) {
                if a != b {
                    return a.cmp(&b);
                }
            }
        }
    }
    left.message.id.cmp(&right.message.id)
}

/// The incremental assembler: the two indexes plus the sorted list they produce.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Assembler {
    by_id: BTreeMap<String, Row>,
    by_turn: BTreeMap<String, Row>,
    /// The assembled, sorted list.
    pub rows: Vec<Row>,
}

fn replace_entry(
    by_id: &mut BTreeMap<String, Row>,
    by_turn: &mut BTreeMap<String, Row>,
    previous: &Row,
    next: Row,
) {
    by_id.remove(&previous.message.id);
    by_turn.remove(&turn_key(&previous.message));
    by_id.insert(next.message.id.clone(), next.clone());
    by_turn.insert(turn_key(&next.message), next);
}

/// Returns the row when this message became a NEW entry, so the caller can append it, or `None`
/// when it merged into or was superseded by an existing one.
fn merge_one(
    by_id: &mut BTreeMap<String, Row>,
    by_turn: &mut BTreeMap<String, Row>,
    incoming: Row,
) -> Option<Row> {
    let mut row = incoming;
    if let Some(existing) = by_id.get(&row.message.id).cloned() {
        if !id_collides(&existing.message, &row.message) {
            if supersedes(&row.message, &existing.message) {
                replace_entry(by_id, by_turn, &existing, row);
            }
            return None;
        }
        // A different row wearing the same id: keep both under a derived key.
        row.message.id = shadowed_id(&row.message);
        if let Some(shadow) = by_id.get(&row.message.id).cloned() {
            if supersedes(&row.message, &shadow.message) {
                replace_entry(by_id, by_turn, &shadow, row);
            }
            return None;
        }
    }
    let key = turn_key(&row.message);
    if let Some(existing) = by_turn.get(&key).cloned() {
        // CROSS-SOURCE ONLY: same-source identical turns stay distinct.
        if existing.message.source != row.message.source {
            if supersedes(&row.message, &existing.message) {
                replace_entry(by_id, by_turn, &existing, row);
            }
            return None;
        }
    }
    by_id.insert(row.message.id.clone(), row.clone());
    by_turn.insert(key, row.clone());
    Some(row)
}

impl Assembler {
    /// A canonical rebuild, byte for byte what a one-shot assembly produces.
    pub fn reset(&mut self, base: &[Row]) {
        self.by_id = BTreeMap::new();
        self.by_turn = BTreeMap::new();
        for row in base {
            merge_one(&mut self.by_id, &mut self.by_turn, row.clone());
        }
        self.rows = self.sorted_values();
    }

    /// Applies new rows, using the tail fast path when every one of them landed at the end.
    pub fn apply_appends(&mut self, incoming: &[Row]) -> &[Row] {
        if incoming.is_empty() {
            return &self.rows;
        }
        // Collect what was actually STORED: a row re-keyed off a shared id enters the maps as a
        // different value than the one that arrived.
        let mut added: Vec<Row> = Vec::new();
        for row in incoming {
            if let Some(stored) = merge_one(&mut self.by_id, &mut self.by_turn, row.clone()) {
                added.push(stored);
            }
        }
        if added.len() == incoming.len() && is_tail_append(&self.rows, &added) {
            added.sort_by(compare);
            self.rows.extend(added);
            return &self.rows;
        }
        self.rows = self.sorted_values();
        &self.rows
    }

    fn sorted_values(&self) -> Vec<Row> {
        let mut rows: Vec<Row> = self.by_id.values().cloned().collect();
        rows.sort_by(compare);
        rows
    }
}

fn is_tail_append(current: &[Row], incoming: &[Row]) -> bool {
    let Some(last) = current.last() else {
        return true;
    };
    for row in incoming {
        if row.message.timestamp.is_none() {
            // `null` sorts to the FRONT: never a tail append.
            return false;
        }
        if compare(row, last) == Ordering::Less {
            return false;
        }
    }
    true
}
