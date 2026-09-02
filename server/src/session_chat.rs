use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File},
    io,
    path::Path,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{FileExt, MetadataExt};
#[cfg(windows)]
use std::os::windows::fs::{FileExt, MetadataExt};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/*
CDXC:SessionChatCore 2026-07-31:
Session Chat renders an agent terminal session as a normalized chat by tailing
the agent CLI's own JSONL transcript. This module is the Rust mirror of
`packages/shared/session-chat.ts` plus the upstream chat spec's decoders/readers/watch
engine: serde shapes must serialize to IDENTICAL JSON (kebab-case block tags,
camelCase fields, skip-none optionals), decoders never throw on unknown
records, and the reverse tail reader keeps the spec's exact limit/hasMore/
over-read-by-one semantics. The follower engine emits sessionChatSnapshot/
Appended/Replaced/State frames through a caller-provided broadcast closure;
epoch/seq live in `SessionChatStream` so `/api/readSessionChat` can report the
live stream position without touching the presentation revision sequencer.
*/

pub const SESSION_CHAT_INITIAL_LIMIT: usize = 300;
pub const SESSION_CHAT_MAX_LIMIT: usize = 10_000;

#[cfg(unix)]
pub(crate) fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
    file.read_exact_at(buffer, offset)
}

#[cfg(windows)]
pub(crate) fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
    let mut read = 0;
    while read < buffer.len() {
        let count = file.seek_read(&mut buffer[read..], offset + read as u64)?;
        if count == 0 {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
        }
        read += count;
    }
    Ok(())
}

/*
CDXC:SessionChatCore 2026-08-24:
The largest transcript line the reader will parse. This was 2 MiB, which real
rollouts exceed routinely — one 106 MB Codex rollout on this machine holds 15
lines above 2 MiB (up to 8.4 MB), all giant `custom_tool_call_output` /
`function_call_output` records — and every one of those lines was dropped, so
the matching tool-call/tool-result rows silently vanished from the middle of the
chat. 16 MiB clears the observed ceiling with room to spare; the cost is
transient (one line is held while it is parsed, and tail reads stay chunked at
TAIL_CHUNK_BYTES). The cap now exists only to bound a pathological line, NOT to
bound what reaches a client — DISPLAY size is bounded per block at decode time,
see MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS.
*/
pub(crate) const MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const TAIL_CHUNK_BYTES: usize = 64 * 1024;
const APPEND_BATCH_MESSAGE_LIMIT: usize = 40;
const BOUNDARY_FINGERPRINT_BYTES: u64 = 64;
pub(crate) const RECONCILIATION_INTERVAL: Duration = Duration::from_millis(1_000);
pub(crate) const INITIAL_RESOLVE_POLL: Duration = Duration::from_millis(500);
pub(crate) const MAX_RESOLVE_POLL: Duration = Duration::from_millis(5_000);
/// How long a subscribe waits for its one model/effort probe before emitting
/// the snapshot anyway, and how long a read waits for the same probe before
/// answering without the screen-derived fields. See
/// `CDXC:SessionChatSeedDetection` and `CDXC:SessionChatReadDetectionDeadline`.
pub(crate) const SEED_OPTION_DETECTION_DEADLINE: Duration = Duration::from_millis(500);
/// How long the steady-state model/effort/notice probe may take before the
/// reconcile pass abandons it. The probe runs on the blocking pool but is
/// awaited inline, so without a deadline a daemon that accepts the capture
/// connection and never answers wedges the whole follower.
pub(crate) const STEADY_OPTION_DETECTION_DEADLINE: Duration = Duration::from_millis(10_000);
/*
CDXC:SessionChatFollowerLiveness 2026-08-24:
How long a follower task may run without reaching a new reconcile iteration
before the sync path calls it wedged and respawns it. Every legitimate long
wait PARKS the heartbeat and is exempt, so this only has to clear the longest
inline await a healthy pass can contain (the 10s steady-state probe deadline,
plus the blocking drain / re-resolution / successor scan behind it).
*/
pub(crate) const SESSION_CHAT_FOLLOWER_WEDGE_DEADLINE: Duration = Duration::from_millis(45_000);
/// A working session whose transcript has been silent this long is tailing a
/// file the agent has moved on from; re-resolve the path.
pub(crate) const STALE_TRANSCRIPT_IDLE: Duration = Duration::from_millis(10_000);
pub(crate) const INTERRUPTED_STATUS_TEXT: &str = "Conversation interrupted";
/*
CDXC:SessionChatCore 2026-08-23:
Codex's compaction seam. Unlike Claude, Codex leaves every summarised turn in
the rollout, so a compaction changes nothing a transcript projection can see —
the conversation just silently stops carrying the older turns' context. Its TUI
draws the seam as an info row (`• Context compacted`), and chat draws the same
one from the `ContextCompaction` thread item behind it. The wording is Codex's
own so the two surfaces read alike; the client renders it as the completed-
action status row Claude's compaction already gets.
*/
pub(crate) const CONTEXT_COMPACTED_STATUS_TEXT: &str = "Context compacted";
/*
The upstream chat spec persists pasted clipboard images as `<host>-paste-*.png`
temp files whose absolute path Grok concatenates with the typed prompt. Ghostex
uses its own prefix; the surrounding match logic stays identical to the spec's
regex shape.
*/
pub(crate) const GROK_PASTED_IMAGE_TOKEN: &str = "ghostex-paste-";

// ---------------------------------------------------------------------------
// Schema (Rust mirror of packages/shared/session-chat.ts)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionChatRole {
    User,
    Assistant,
    Reasoning,
    Tool,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionChatSource {
    Transcript,
    Hook,
    Client,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SessionChatBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool-call")]
    ToolCall {
        name: String,
        #[serde(default)]
        input: Value,
    },
    #[serde(rename = "tool-result")]
    ToolResult {
        output: String,
        #[serde(rename = "isError", skip_serializing_if = "Option::is_none", default)]
        is_error: Option<bool>,
    },
    #[serde(rename = "image-ref")]
    ImageRef {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        path: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        alt: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatMessage {
    pub id: String,
    pub role: SessionChatRole,
    pub blocks: Vec<SessionChatBlock>,
    /// Epoch ms; serialized as `null` when absent (null sorts before any timestamp).
    pub timestamp: Option<i64>,
    pub source: SessionChatSource,
    #[serde(rename = "turnId", skip_serializing_if = "Option::is_none", default)]
    pub turn_id: Option<String>,
    /*
    CDXC:SessionChatCore 2026-08-01:
    Byte offset of the record's line in the transcript file. Stamped by the
    readers (the decoders are line-local and cannot know it), so it is stable
    across tail, incremental and pagination reads of the same file. Clients use
    it to break (timestamp) ties in file order instead of by random uuid, which
    reordered rows inside one turn and broke tool folding.
    */
    #[serde(
        rename = "byteOffset",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub byte_offset: Option<u64>,
    /*
    CDXC:SessionChatCore 2026-08-19:
    The row is a prompt sitting in the agent's own queue that has NOT been
    handed to the model yet (see `TranscriptQueueOp`). Clients label it; the
    server retracts it the moment the queue releases it, and the delivered row
    takes its place. Never set on client-sourced optimistic echoes — those
    render identically to real turns by design.
    */
    #[serde(default, skip_serializing_if = "is_not_queued")]
    pub queued: bool,
}

fn is_not_queued(queued: &bool) -> bool {
    !queued
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionChatTurnLifecycleState {
    Working,
    Completed,
    Interrupted,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatTurnLifecycle {
    pub state: SessionChatTurnLifecycleState,
    #[serde(rename = "turnId")]
    pub turn_id: String,
    pub timestamp: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatStatus {
    Loading,
    Ready,
    Working,
    Empty,
    Starting,
    Error,
    Unsupported,
}

impl SessionChatStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionChatStatus::Loading => "loading",
            SessionChatStatus::Ready => "ready",
            SessionChatStatus::Working => "working",
            SessionChatStatus::Empty => "empty",
            SessionChatStatus::Starting => "starting",
            SessionChatStatus::Error => "error",
            SessionChatStatus::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatTranscriptAgent {
    Antigravity,
    Claude,
    Codex,
    Cursor,
    Grok,
    Hermes,
    Pi,
}

/// `claude` and `openclaude` share the Claude transcript format.
pub fn resolve_session_chat_transcript_agent(
    agent: Option<&str>,
) -> Option<SessionChatTranscriptAgent> {
    match agent?.trim().to_ascii_lowercase().as_str() {
        "antigravity" | "agy" | "antigravity cli" | "antigravity-cli" => {
            Some(SessionChatTranscriptAgent::Antigravity)
        }
        "claude" | "openclaude" => Some(SessionChatTranscriptAgent::Claude),
        "codex" => Some(SessionChatTranscriptAgent::Codex),
        "cursor" | "cursor-agent" | "cursor cli" => Some(SessionChatTranscriptAgent::Cursor),
        "grok" | "grok-build" => Some(SessionChatTranscriptAgent::Grok),
        "hermes" | "hermes agent" | "hermes-agent" => Some(SessionChatTranscriptAgent::Hermes),
        "pi" | "omp" => Some(SessionChatTranscriptAgent::Pi),
        _ => None,
    }
}

pub fn session_chat_transcript_agent_id(agent: Option<&str>) -> Option<&'static str> {
    match resolve_session_chat_transcript_agent(agent)? {
        SessionChatTranscriptAgent::Antigravity => Some("antigravity"),
        SessionChatTranscriptAgent::Claude => Some("claude"),
        SessionChatTranscriptAgent::Codex => Some("codex"),
        SessionChatTranscriptAgent::Cursor => Some("cursor"),
        SessionChatTranscriptAgent::Grok => Some("grok"),
        SessionChatTranscriptAgent::Hermes => Some("hermes"),
        SessionChatTranscriptAgent::Pi => Some("pi"),
    }
}

pub type SessionChatLineDecoder = fn(&str, &str) -> Option<SessionChatMessage>;
pub type SessionChatLifecycleDecoder = fn(&str, &str) -> Option<SessionChatTurnLifecycle>;

pub fn session_chat_line_decoder(agent: SessionChatTranscriptAgent) -> SessionChatLineDecoder {
    match agent {
        SessionChatTranscriptAgent::Antigravity => decode_antigravity_transcript_line,
        SessionChatTranscriptAgent::Claude => decode_claude_transcript_line,
        SessionChatTranscriptAgent::Codex => decode_codex_transcript_line,
        SessionChatTranscriptAgent::Cursor => decode_cursor_transcript_line,
        SessionChatTranscriptAgent::Grok => decode_grok_transcript_line,
        SessionChatTranscriptAgent::Hermes => decode_hermes_transcript_line,
        SessionChatTranscriptAgent::Pi => decode_pi_transcript_line,
    }
}

pub fn session_chat_lifecycle_decoder(
    agent: SessionChatTranscriptAgent,
) -> Option<SessionChatLifecycleDecoder> {
    match agent {
        SessionChatTranscriptAgent::Antigravity => Some(decode_antigravity_turn_lifecycle),
        SessionChatTranscriptAgent::Claude => Some(decode_claude_turn_lifecycle),
        SessionChatTranscriptAgent::Codex => Some(decode_codex_turn_lifecycle),
        SessionChatTranscriptAgent::Cursor => Some(decode_cursor_turn_lifecycle),
        SessionChatTranscriptAgent::Grok => Some(decode_grok_turn_lifecycle),
        SessionChatTranscriptAgent::Hermes => Some(decode_hermes_turn_lifecycle),
        SessionChatTranscriptAgent::Pi => None,
    }
}

/*
CDXC:SessionChatCore 2026-08-19:
Agent-side prompt queue. Typing while the model is mid-turn does NOT write a
prompt row — the harness parks the text in its own queue and writes bookkeeping
rows instead, then delivers it later (Claude: `queue-operation`
enqueue/dequeue/remove/popAll). The queue is a FIFO, and only the removal rows
name WHICH entry left, so the readers replay these ops in file order rather
than pairing them by position.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptQueueOp {
    /// A prompt entered the queue. `key` is its normalized text, empty when
    /// the row does not carry the text (still tracked, so the FIFO stays
    /// aligned).
    Enqueued { key: String },
    /// One entry left the queue — delivered to the model, or dropped by the
    /// user. `None` means the row does not name it, which for a FIFO is the
    /// oldest entry.
    Left { key: Option<String> },
    /// The whole queue was discarded at once.
    Cleared,
}

/*
CDXC:SessionChatRewind 2026-09-02:
Claude Code's own resume loader reads `last-prompt` rows to decide which leaf a
reopened conversation continues from. Only `explicit: true` rows are leaf
markers: the harness also writes non-explicit ones as ordinary bookkeeping
after a turn, and treating those as markers would truncate live conversations.
`leafUuid: null` on an explicit row means the conversation was rewound to
before its first message. A marker is void the moment any tree row is written
after it, because that row is the new leaf.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscriptLeafMarker {
    /// `leafUuid` names the row the conversation was rewound to.
    Row(String),
    /// `leafUuid: null`, i.e. rewound to before the first message.
    Empty,
}

impl TranscriptLeafMarker {
    pub fn leaf_id(&self) -> Option<&str> {
        match self {
            Self::Row(id) => Some(id.as_str()),
            Self::Empty => None,
        }
    }
}

/// One row's position in a transcript that is a message TREE rather than a
/// flat log. Only Claude writes one (`uuid` / `parentUuid`); Pi has its own
/// tree reader, and the Codex/Grok rollouts are linear. Queue bookkeeping rows
/// and `last-prompt` leaf markers ride the same extractor because they are the
/// only other rows whose meaning spans lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptLineage {
    pub id: String,
    pub parent_id: Option<String>,
    #[allow(clippy::struct_field_names)]
    pub queue: Option<TranscriptQueueOp>,
    /// Set only on an explicit `last-prompt` row, which carries no `uuid` and
    /// is never part of the tree itself.
    pub leaf_marker: Option<TranscriptLeafMarker>,
}

/// `(line, fallback_id)` — queue rows carry no `uuid`, so they are identified
/// by the same `<path>:<byte offset>` id the decoder stamps on them.
pub type SessionChatLineageExtractor = fn(&str, &str) -> Option<TranscriptLineage>;

pub fn session_chat_lineage_extractor(
    agent: SessionChatTranscriptAgent,
) -> Option<SessionChatLineageExtractor> {
    match agent {
        SessionChatTranscriptAgent::Claude => Some(claude_transcript_lineage),
        SessionChatTranscriptAgent::Antigravity
        | SessionChatTranscriptAgent::Codex
        | SessionChatTranscriptAgent::Cursor
        | SessionChatTranscriptAgent::Grok
        | SessionChatTranscriptAgent::Hermes
        | SessionChatTranscriptAgent::Pi => None,
    }
}

/*
CDXC:SessionChatCore 2026-08-18:
Abandoned prompts. Submitting a prompt and then revising or re-sending it
before the model answered leaves BOTH rows in the transcript as siblings of the
same `parentUuid`; only the last one is ever answered. The terminal renders the
branch that was actually taken, so chat showed prompts the terminal never did
(reported for `290fff5d…`, two "ok please implement the fixes you suggested"
rows 13s apart, the first with no children at all).

The rule below is deliberately the NARROWEST one that catches it: a real prompt
row (role `User` — never a tool_result, meta or interrupted row) that no
user/assistant row descends from, whose parent already carries a NEWER prompt.
Walking the leaf chain instead would have been catastrophic — compaction and
resume legitimately break the chain, and a real turn's parallel tool calls and
hook `attachment` rows mean an ordinary parent has several children.

Both halves of "no reply" and "re-taken branch" are counted over decodable
message rows only. A prompt often collects a hook `attachment` child that is not
an answer, and letting a non-prompt sibling do the retracting would kill a
prompt typed while the previous turn was still streaming.

Measured over the 80 most recent local transcripts this drops 16 rows, every one
a re-sent or revised submission; "keep only the leaf chain" dropped 9k rows and
"keep the newest child of every parent" 9.2k.

CDXC:SessionChatRewind 2026-09-02 (generalized to whole subtrees):
The rule above only ever fired on a prompt with NO message descendants, so a
`/rewind` → "Restore conversation" left its abandoned turns in chat: the rewind
writes nothing at the time, and the next prompt lands as a SECOND prompt child
of the rewound leaf while the answered rows in between stay in the file. The
read path therefore drops the older prompt sibling AND everything descending
from it (`session_chat_branch.rs`), which subsumes the no-descendants case
because that subtree is empty. Still not the leaf chain: only a real prompt
sibling can retract a branch, so compaction and resume boundaries, which break
the chain without ever producing two prompts on one parent, are untouched.
Re-measured over the 120 most recent local transcripts: 2,065 of 55,622 rows,
in 12 files, every drop a rewind or a revised re-send.
*/
pub(crate) fn parse_json_object(line: &str) -> Option<Map<String, Value>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(trimmed).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

pub(crate) fn extract_string(value: Option<&Value>) -> Option<String> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub(crate) fn as_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value?.as_object()
}

pub(crate) fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
    match value? {
        Value::String(text) => chrono::DateTime::parse_from_rfc3339(text.trim())
            .ok()
            .map(|parsed| parsed.timestamp_millis()),
        Value::Number(number) => {
            let raw = number.as_f64()?;
            if !raw.is_finite() || raw <= 0.0 {
                return None;
            }
            Some(if raw > 1_000_000_000_000.0 {
                raw as i64
            } else {
                (raw * 1_000.0) as i64
            })
        }
        _ => None,
    }
}

pub(crate) fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
    timestamp_ms(value)
}

const TRANSCRIPT_POSITION_WIDTH: usize = 16;

pub fn transcript_fallback_id(file_path: &Path, byte_offset: u64) -> String {
    format!(
        "{}:{:0width$}",
        file_path.display(),
        byte_offset,
        width = TRANSCRIPT_POSITION_WIDTH
    )
}

/*
Kill-key bytes leaked into a recorded turn. The chat send path clears the TUI
composer with a Ctrl+U/Ctrl+K burst before pasting; when that burst coalesces
into the same stdin chunk as the paste frame, the TUI's chunk-level paste
handling inserts the burst as literal text at the head of the prompt (observed
2026-08-23: Claude Code recorded 39×0x15 + 39×0x0b inside a user message).
Those bytes are never typeable content, so decoded text drops all C0 controls
except \t/\n/\r (kept: real text) and ESC (kept: ANSI styling is the noise
filter's concern, and a bare strip would leave dangling `[1m` fragments).
*/
fn is_leaked_control_char(character: char) -> bool {
    matches!(
        character,
        '\u{00}'..='\u{08}'
            | '\u{0b}'
            | '\u{0c}'
            | '\u{0e}'..='\u{1a}'
            | '\u{1c}'..='\u{1f}'
            | '\u{7f}'
    )
}

fn strip_leaked_control_chars(text: String) -> String {
    if text.chars().any(is_leaked_control_char) {
        text.chars()
            .filter(|character| !is_leaked_control_char(*character))
            .collect()
    } else {
        text
    }
}

pub(crate) fn text_block(text: impl Into<String>) -> SessionChatBlock {
    SessionChatBlock::Text {
        text: strip_leaked_control_chars(text.into()),
    }
}

/*
CDXC:SessionChatCore 2026-08-24:
How much of a tool payload a client is shown. Tool outputs and raw tool-call
inputs are the only blocks that reach megabyte size, and every decoded block
crosses the events websocket into the page, so the DISPLAY copy is bounded here
while the transcript file keeps the record whole. This bounds a viewer; it hides
nothing — the truncation says so on its own line, and the full text is still one
`open the transcript` away.

Only tool payloads are bounded. User and assistant TEXT must never be, because
the chat's optimistic echo is de-duplicated by matching the decoded user text
against what the composer sent: a shortened user turn would never match and the
sent message would stay duplicated in the view.
*/
pub(crate) const MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS: usize = 64 * 1024;

/// Cuts `text` to `MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS` chars on a char
/// boundary (never mid-UTF-8) and marks the cut. Untouched below the bound.
pub(crate) fn bounded_tool_payload(text: String) -> String {
    // A char is at least one byte, so a short byte length can't be over-length.
    if text.len() <= MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS {
        return text;
    }
    let total = text.chars().count();
    if total <= MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS {
        return text;
    }
    let cut = text
        .char_indices()
        .nth(MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    let mut bounded = text;
    bounded.truncate(cut);
    bounded.push_str(&format!(
        "\n… [tool payload truncated for display: {MAX_SESSION_CHAT_TOOL_PAYLOAD_CHARS} of {total} characters shown]"
    ));
    bounded
}

/// Bounds a tool-call INPUT the way `tool_result_output` bounds an output.
/// Only raw strings are measured: `custom_tool_call` carries its entire input as
/// one string and is the only input shape observed in the megabyte range, while
/// structured inputs are small and serializing every one of them just to size it
/// would cost more than the bound saves.
pub(crate) fn bounded_tool_call_input(value: Value) -> Value {
    match value {
        Value::String(text) => Value::String(bounded_tool_payload(text)),
        other => other,
    }
}

pub(crate) fn tool_result_output(value: Option<&Value>) -> String {
    bounded_tool_payload(tool_result_output_raw(value))
}

fn tool_result_output_raw(value: Option<&Value>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    match value {
        Value::String(text) => text.clone(),
        Value::Array(items) => {
            let mut parts: Vec<String> = Vec::new();
            for item in items {
                if let Value::String(text) = item {
                    parts.push(text.clone());
                    continue;
                }
                let record = item.as_object();
                if let Some(text) = extract_string(record.and_then(|inner| inner.get("text")))
                    .or_else(|| extract_string(record.and_then(|inner| inner.get("content"))))
                {
                    parts.push(text);
                }
            }
            parts.join("\n")
        }
        Value::Null => String::new(),
        other => {
            if let Some(record) = other.as_object() {
                if let Some(text) = extract_string(record.get("text"))
                    .or_else(|| extract_string(record.get("content")))
                {
                    return text;
                }
            }
            serde_json::to_string(other).unwrap_or_default()
        }
    }
}

pub(crate) const PASTED_IMAGE_ALT: &str = "Pasted image";

/*
CDXC:SessionChatCore 2026-08-01:
Claude records a pasted/screenshotted image as
`{"type":"image","source":{"type":"base64",…}}` — no url, no path. Returning
None there dropped the block, and an image-only user turn then decoded to zero
blocks and vanished from chat entirely. Base64 sources now emit an image-ref
carrying only `alt`, which the chat clients render as an attachment chip. The
bytes are deliberately NOT forwarded: transcripts hold multi-megabyte data URLs
and every frame crosses the websocket.
*/
pub(crate) fn image_ref_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
    let source = as_record(record.get("source"));
    let url = extract_string(source.and_then(|inner| inner.get("url")))
        .or_else(|| extract_string(record.get("url")))
        .or_else(|| extract_string(record.get("image_url")));
    let path = extract_string(record.get("path"))
        .or_else(|| extract_string(source.and_then(|inner| inner.get("path"))));
    let alt = extract_string(record.get("alt"))
        .or_else(|| extract_string(record.get("file_name")))
        .or_else(|| extract_string(source.and_then(|inner| inner.get("file_name"))));
    if url.is_none() && path.is_none() {
        let has_inline_bytes = source.is_some_and(|inner| {
            inner.get("data").is_some_and(|data| !data.is_null())
                || extract_string(inner.get("type")).as_deref() == Some("base64")
        });
        if !has_inline_bytes {
            return None;
        }
        return Some(SessionChatBlock::ImageRef {
            path: None,
            url: None,
            alt: Some(alt.unwrap_or_else(|| PASTED_IMAGE_ALT.to_string())),
        });
    }
    Some(SessionChatBlock::ImageRef { path, url, alt })
}

// ---------------------------------------------------------------------------
// Claude decoder (upstream chat spec §2.2)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct SessionChatIncrementalState {
    pub offset: u64,
    pending_chunks: Vec<Vec<u8>>,
    pending_start: u64,
    pending_bytes: usize,
    dropping_oversized_record: bool,
    /*
    CDXC:SessionChatCore 2026-08-18:
    Forward half of the abandoned-prompt rule (see `superseded_prompt_id`). The
    tail read can decide in one pass because it walks newest-first; the append
    stream sees the prompt BEFORE the row that abandons it, so it must publish
    the prompt immediately and retract it when the sibling lands — which can be
    a minute later, hence the state lives across drains. Only prompts that have
    no reply yet are held, so this is a handful of entries at most.
    */
    unanswered_prompt_by_parent: HashMap<String, String>,
    unanswered_prompt_parents: HashMap<String, String>,
    superseded_prompt_ids: Vec<String>,
    /*
    CDXC:SessionChatCore 2026-08-19:
    Prompts published with the "Queued" label that the agent's queue has not
    released yet, oldest first, as `(normalized text, message id)`. The release
    row retracts them through the same channel abandoned prompts use. Seeded
    from the tail read after every snapshot, because `rebase` wipes this state
    while the queue itself keeps waiting.
    */
    queued_prompts: VecDeque<(String, String)>,
    /*
    CDXC:SessionChatRewind 2026-09-02:
    The newest tree row the stream has seen, seeded from the tail read after
    every snapshot. An ordinary prompt names it as its parent; a prompt that
    names anything else re-attaches further up the tree, which is a rewind and
    is answered by a fresh generation instead of an append, because retracting
    a whole dead subtree row by row would race the client's own ordering.
    */
    leaf_row_id: Option<String>,
    active_branch_changed: bool,
}

impl SessionChatIncrementalState {
    pub fn new() -> Self {
        Self {
            offset: 0,
            pending_chunks: Vec::new(),
            pending_start: 0,
            pending_bytes: 0,
            dropping_oversized_record: false,
            unanswered_prompt_by_parent: HashMap::new(),
            unanswered_prompt_parents: HashMap::new(),
            superseded_prompt_ids: Vec::new(),
            queued_prompts: VecDeque::new(),
            leaf_row_id: None,
            active_branch_changed: false,
        }
    }

    pub fn reset(&mut self) {
        self.offset = 0;
        self.pending_chunks.clear();
        self.pending_start = 0;
        self.pending_bytes = 0;
        self.dropping_oversized_record = false;
        self.unanswered_prompt_by_parent.clear();
        self.unanswered_prompt_parents.clear();
        self.superseded_prompt_ids.clear();
        self.queued_prompts.clear();
        self.leaf_row_id = None;
        self.active_branch_changed = false;
    }

    /// Hands the tail read's still-waiting queue entries to the append stream
    /// so their release rows can retract them. Call AFTER `rebase`.
    pub fn seed_queued_prompts(&mut self, entries: Vec<(String, String)>) {
        self.queued_prompts = entries.into_iter().collect();
    }

    /// Ids of already-published prompts that a later row proved abandoned.
    /// Drained by the caller, which removes them from the batch it is about to
    /// emit and reports the rest to clients.
    pub fn take_superseded_prompt_ids(&mut self) -> Vec<String> {
        std::mem::take(&mut self.superseded_prompt_ids)
    }

    /// Hands the tail read's active leaf to the append stream. Call AFTER
    /// `rebase`, like `seed_queued_prompts`.
    pub fn seed_leaf_row_id(&mut self, leaf_row_id: Option<String>) {
        self.leaf_row_id = leaf_row_id;
    }

    /// `true` once this stream has seen a row that moved the conversation onto
    /// another branch. Drained by the follower, which answers with a snapshot.
    pub fn take_active_branch_change(&mut self) -> bool {
        std::mem::take(&mut self.active_branch_changed)
    }

    fn observe_lineage(&mut self, row: &TranscriptLineage, message: Option<&SessionChatMessage>) {
        if let Some(queue_op) = row.queue.as_ref() {
            self.observe_queue_operation(queue_op, &row.id);
            return;
        }
        if row.leaf_marker.is_some() {
            // An explicit leaf marker appended live IS the rewind.
            self.active_branch_changed = true;
            return;
        }
        // Every tree row is the leaf until the next one lands, including the
        // hook `attachment` and `system` rows that decode to nothing.
        let previous_leaf = self.leaf_row_id.replace(row.id.clone());
        let Some(message) = message else {
            // Hook `attachment` and bookkeeping rows are neither an answer nor
            // a re-taken branch (see `session_chat_branch`).
            return;
        };
        let Some(parent_id) = row.parent_id.clone() else {
            return;
        };
        // A message descending from a prompt settles it: never abandoned now.
        if let Some(settled_prompt_parent) = self.unanswered_prompt_parents.remove(&parent_id) {
            self.unanswered_prompt_by_parent
                .remove(&settled_prompt_parent);
        }
        if !crate::session_chat_branch::transcript_message_is_branch_prompt(Some(message)) {
            return;
        }
        // A second PROMPT on the same parent means the branch was re-taken, so
        // the prompt that was waiting there was abandoned.
        if let Some(abandoned) = self.unanswered_prompt_by_parent.remove(&parent_id) {
            if abandoned != row.id {
                self.unanswered_prompt_parents.remove(&abandoned);
                self.superseded_prompt_ids.push(abandoned);
            }
        } else if previous_leaf.is_some_and(|leaf| leaf != parent_id) {
            /*
            The prompt skipped the leaf and re-attached further up: everything
            between its parent and the old leaf is a dead branch now. The
            retraction channel only carries ids the client already has, and the
            dead rows can be older than its window, so this is answered with a
            fresh generation instead.
            */
            self.active_branch_changed = true;
        }
        self.unanswered_prompt_by_parent
            .insert(parent_id.clone(), row.id.clone());
        self.unanswered_prompt_parents
            .insert(row.id.clone(), parent_id);
    }

    /// Forward half of the queue rule (see `replay_transcript_queue`, which is
    /// the newest-first half). A removal that matches nothing is ignored, not
    /// treated as a FIFO pop, for the same reason.
    fn observe_queue_operation(&mut self, op: &TranscriptQueueOp, row_id: &str) {
        match op {
            TranscriptQueueOp::Enqueued { key } => {
                self.queued_prompts
                    .push_back((key.clone(), row_id.to_string()));
            }
            TranscriptQueueOp::Left { key: Some(key) } => {
                if let Some(index) = self
                    .queued_prompts
                    .iter()
                    .position(|(queued, _)| queued == key)
                {
                    if let Some((_, id)) = self.queued_prompts.remove(index) {
                        self.superseded_prompt_ids.push(id);
                    }
                }
            }
            TranscriptQueueOp::Left { key: None } => {
                if let Some((_, id)) = self.queued_prompts.pop_front() {
                    self.superseded_prompt_ids.push(id);
                }
            }
            TranscriptQueueOp::Cleared => {
                for (_, id) in self.queued_prompts.drain(..) {
                    self.superseded_prompt_ids.push(id);
                }
            }
        }
    }

    pub fn rebase(&mut self, offset: u64) {
        self.reset();
        self.offset = offset;
        self.pending_start = offset;
    }

    fn retain_part(&mut self, part: &[u8]) {
        if self.dropping_oversized_record {
            return;
        }
        self.pending_bytes += part.len();
        if self.pending_bytes > MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES {
            self.pending_chunks.clear();
            self.dropping_oversized_record = true;
        } else {
            self.pending_chunks.push(part.to_vec());
        }
    }

    fn reset_pending_line(&mut self, next_start: u64) {
        self.pending_chunks.clear();
        self.pending_bytes = 0;
        self.pending_start = next_start;
        self.dropping_oversized_record = false;
    }

    fn take_pending_line(&mut self) -> Option<String> {
        let mut bytes: Vec<u8> = Vec::with_capacity(self.pending_bytes);
        for part in &self.pending_chunks {
            bytes.extend_from_slice(part);
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        if bytes.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }
}

impl Default for SessionChatIncrementalState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn read_incremental_transcript_messages(
    file_path: &Path,
    state: &mut SessionChatIncrementalState,
    decode: SessionChatLineDecoder,
    mut on_batch: Option<&mut dyn FnMut(Vec<SessionChatMessage>)>,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
    mut on_lifecycle: Option<&mut dyn FnMut(SessionChatTurnLifecycle)>,
    lineage: Option<SessionChatLineageExtractor>,
) -> std::io::Result<Vec<SessionChatMessage>> {
    let file = File::open(file_path)?;
    let end = file.metadata()?.len();
    if end <= state.offset {
        return Ok(Vec::new());
    }
    let mut messages: Vec<SessionChatMessage> = Vec::new();
    let mut absolute_offset = state.offset;
    let mut buffer = vec![0u8; TAIL_CHUNK_BYTES];
    while absolute_offset < end {
        let take = ((end - absolute_offset).min(TAIL_CHUNK_BYTES as u64)) as usize;
        read_exact_at(&file, &mut buffer[..take], absolute_offset)?;
        let mut segment_start = 0usize;
        for index in 0..take {
            if buffer[index] != b'\n' {
                continue;
            }
            state.retain_part(&buffer[segment_start..index]);
            if !state.dropping_oversized_record {
                if let Some(line) = state.take_pending_line() {
                    let fallback_id = transcript_fallback_id(file_path, state.pending_start);
                    if let Some(decode_lifecycle) = decode_lifecycle {
                        if let Some(next) = decode_lifecycle(&line, &fallback_id) {
                            if let Some(on_lifecycle) = on_lifecycle.as_mut() {
                                on_lifecycle(next);
                            }
                        }
                    }
                    let decoded = decode(&line, &fallback_id);
                    if let Some(extract) = lineage {
                        if let Some(row) = extract(&line, &fallback_id) {
                            state.observe_lineage(&row, decoded.as_ref());
                        }
                    }
                    if let Some(mut message) = decoded {
                        message.byte_offset = Some(state.pending_start);
                        messages.push(message);
                        if let Some(on_batch) = on_batch.as_mut() {
                            if messages.len() >= APPEND_BATCH_MESSAGE_LIMIT {
                                on_batch(std::mem::take(&mut messages));
                            }
                        }
                    }
                }
            }
            state.reset_pending_line(absolute_offset + index as u64 + 1);
            segment_start = index + 1;
        }
        if segment_start < take {
            state.retain_part(&buffer[segment_start..take]);
        }
        absolute_offset += take as u64;
        state.offset = absolute_offset;
    }
    Ok(messages)
}

// ---------------------------------------------------------------------------
// File version + boundary fingerprint (upstream chat spec §5.3–5.4)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptFileVersion {
    pub identity: String,
    pub size: u64,
    pub mtime_ms: i128,
    pub ctime_ms: i128,
}

pub fn read_transcript_file_version(file_path: &Path) -> std::io::Result<TranscriptFileVersion> {
    let metadata = fs::metadata(file_path)?;
    Ok(TranscriptFileVersion {
        identity: transcript_file_identity(file_path, &metadata)?,
        size: metadata.len(),
        mtime_ms: transcript_mtime_ms(&metadata),
        ctime_ms: transcript_ctime_ms(&metadata),
    })
}

#[cfg(unix)]
fn transcript_file_identity(_file_path: &Path, metadata: &fs::Metadata) -> io::Result<String> {
    Ok(format!("{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn transcript_file_identity(file_path: &Path, _metadata: &fs::Metadata) -> io::Result<String> {
    use std::{mem::zeroed, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let file = File::open(file_path)?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok(format!("{}:{index}", info.dwVolumeSerialNumber))
}

#[cfg(unix)]
fn transcript_mtime_ms(metadata: &fs::Metadata) -> i128 {
    i128::from(metadata.mtime()) * 1_000 + i128::from(metadata.mtime_nsec()) / 1_000_000
}

#[cfg(windows)]
fn transcript_mtime_ms(metadata: &fs::Metadata) -> i128 {
    windows_filetime_to_unix_ms(metadata.last_write_time())
}

#[cfg(unix)]
fn transcript_ctime_ms(metadata: &fs::Metadata) -> i128 {
    i128::from(metadata.ctime()) * 1_000 + i128::from(metadata.ctime_nsec()) / 1_000_000
}

#[cfg(windows)]
fn transcript_ctime_ms(metadata: &fs::Metadata) -> i128 {
    windows_filetime_to_unix_ms(metadata.creation_time())
}

#[cfg(windows)]
pub(crate) fn windows_filetime_to_unix_ms(filetime: u64) -> i128 {
    i128::from(filetime) / 10_000 - 11_644_473_600_000
}

/// Last ≤64 bytes before the read cursor, base64 — detects in-place rewrites
/// that preserve size and same-inode truncate+rewrite.
pub fn boundary_fingerprint(file_path: &Path, offset: u64) -> std::io::Result<String> {
    if offset == 0 {
        return Ok(String::new());
    }
    let file = File::open(file_path)?;
    let start = offset.saturating_sub(BOUNDARY_FINGERPRINT_BYTES);
    let length = (offset - start) as usize;
    let mut buffer = vec![0u8; length];
    read_exact_at(&file, &mut buffer, start)?;
    Ok(BASE64_STANDARD.encode(&buffer))
}

// ---------------------------------------------------------------------------
// Transcript path resolution
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Re-exports (Phase C2b split): external callers reached these items via
// `crate::session_chat::X` before the split into the flat session_chat_*
// family below. Keep these so every prior call site keeps compiling.
// ---------------------------------------------------------------------------
pub use crate::session_chat_decode_antigravity::*;
pub use crate::session_chat_decode_claude::*;
pub use crate::session_chat_decode_codex::*;
pub use crate::session_chat_decode_cursor::*;
pub use crate::session_chat_decode_grok::*;
pub use crate::session_chat_decode_hermes::*;
pub use crate::session_chat_decode_pi::*;
pub use crate::session_chat_follower::*;
pub use crate::session_chat_interactive::*;
pub use crate::session_chat_paths::*;
pub use crate::session_chat_stream::*;
pub use crate::session_chat_successor::*;
pub use crate::session_chat_tail::*;
