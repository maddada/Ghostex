use std::{
    collections::{HashMap, HashSet},
    fs::{self, File},
    io::{self, BufRead, BufReader, Read},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::{FileExt, MetadataExt};
#[cfg(windows)]
use std::os::windows::fs::{FileExt, MetadataExt};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::resume_lookup::{expand_home, home_dir};

/*
CDXC:SessionChatCore 2026-07-31:
Session Chat renders an agent terminal session as a normalized chat by tailing
the agent CLI's own JSONL transcript. This module is the Rust mirror of
`shared/session-chat.ts` plus the upstream chat spec's decoders/readers/watch
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
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
    file.read_exact_at(buffer, offset)
}

#[cfg(windows)]
fn read_exact_at(file: &File, buffer: &mut [u8], offset: u64) -> io::Result<()> {
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

const MAX_SESSION_CHAT_TRANSCRIPT_RECORD_BYTES: usize = 2 * 1024 * 1024;
const TAIL_CHUNK_BYTES: usize = 64 * 1024;
const APPEND_BATCH_MESSAGE_LIMIT: usize = 40;
const BOUNDARY_FINGERPRINT_BYTES: u64 = 64;
const RECONCILIATION_INTERVAL: Duration = Duration::from_millis(1_000);
const INITIAL_RESOLVE_POLL: Duration = Duration::from_millis(500);
const MAX_RESOLVE_POLL: Duration = Duration::from_millis(5_000);
/// A working session whose transcript has been silent this long is tailing a
/// file the agent has moved on from; re-resolve the path.
const STALE_TRANSCRIPT_IDLE: Duration = Duration::from_millis(10_000);
const INTERRUPTED_STATUS_TEXT: &str = "Conversation interrupted";
/*
The upstream chat spec persists pasted clipboard images as `<host>-paste-*.png`
temp files whose absolute path Grok concatenates with the typed prompt. Ghostex
uses its own prefix; the surrounding match logic stays identical to the spec's
regex shape.
*/
const GROK_PASTED_IMAGE_TOKEN: &str = "ghostex-paste-";

// ---------------------------------------------------------------------------
// Schema (Rust mirror of shared/session-chat.ts)
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
    Claude,
    Codex,
    Grok,
    Pi,
}

/// `claude` and `openclaude` share the Claude transcript format.
pub fn resolve_session_chat_transcript_agent(
    agent: Option<&str>,
) -> Option<SessionChatTranscriptAgent> {
    match agent?.trim().to_ascii_lowercase().as_str() {
        "claude" | "openclaude" => Some(SessionChatTranscriptAgent::Claude),
        "codex" => Some(SessionChatTranscriptAgent::Codex),
        "grok" => Some(SessionChatTranscriptAgent::Grok),
        "pi" | "omp" => Some(SessionChatTranscriptAgent::Pi),
        _ => None,
    }
}

pub type SessionChatLineDecoder = fn(&str, &str) -> Option<SessionChatMessage>;
pub type SessionChatLifecycleDecoder = fn(&str, &str) -> Option<SessionChatTurnLifecycle>;

pub fn session_chat_line_decoder(agent: SessionChatTranscriptAgent) -> SessionChatLineDecoder {
    match agent {
        SessionChatTranscriptAgent::Claude => decode_claude_transcript_line,
        SessionChatTranscriptAgent::Codex => decode_codex_transcript_line,
        SessionChatTranscriptAgent::Grok => decode_grok_transcript_line,
        SessionChatTranscriptAgent::Pi => decode_pi_transcript_line,
    }
}

pub fn session_chat_lifecycle_decoder(
    agent: SessionChatTranscriptAgent,
) -> Option<SessionChatLifecycleDecoder> {
    match agent {
        SessionChatTranscriptAgent::Claude => Some(decode_claude_turn_lifecycle),
        SessionChatTranscriptAgent::Codex => Some(decode_codex_turn_lifecycle),
        SessionChatTranscriptAgent::Grok | SessionChatTranscriptAgent::Pi => None,
    }
}

/// One row's position in a transcript that is a message TREE rather than a
/// flat log. Only Claude writes one (`uuid` / `parentUuid`); Pi has its own
/// tree reader, and the Codex/Grok rollouts are linear.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptLineage {
    pub id: String,
    pub parent_id: Option<String>,
}

pub type SessionChatLineageExtractor = fn(&str) -> Option<TranscriptLineage>;

pub fn session_chat_lineage_extractor(
    agent: SessionChatTranscriptAgent,
) -> Option<SessionChatLineageExtractor> {
    match agent {
        SessionChatTranscriptAgent::Claude => Some(claude_transcript_lineage),
        SessionChatTranscriptAgent::Codex
        | SessionChatTranscriptAgent::Grok
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

Because the superseded row has no message descendants by construction, nothing
downstream of it needs pruning too.
*/
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

fn parse_json_object(line: &str) -> Option<Map<String, Value>> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(trimmed).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

fn extract_string(value: Option<&Value>) -> Option<String> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn as_record(value: Option<&Value>) -> Option<&Map<String, Value>> {
    value?.as_object()
}

fn timestamp_ms(value: Option<&Value>) -> Option<i64> {
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

fn parse_timestamp(value: Option<&Value>) -> Option<i64> {
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

fn text_block(text: impl Into<String>) -> SessionChatBlock {
    SessionChatBlock::Text { text: text.into() }
}

fn is_tool_result_block(block: &SessionChatBlock) -> bool {
    matches!(block, SessionChatBlock::ToolResult { .. })
}

// ---------------------------------------------------------------------------
// Shared block mapping (upstream chat spec §2.1)
// ---------------------------------------------------------------------------

fn tool_result_output(value: Option<&Value>) -> String {
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

fn claude_content_blocks(content: Option<&Value>) -> Vec<SessionChatBlock> {
    match content {
        Some(Value::String(text)) => {
            if text.trim().is_empty() {
                Vec::new()
            } else {
                // NOTE: emits UNTRIMMED text, matching the upstream chat spec.
                vec![text_block(text.clone())]
            }
        }
        Some(Value::Array(items)) => {
            let mut blocks: Vec<SessionChatBlock> = Vec::new();
            for item in items {
                if let Value::String(text) = item {
                    if !text.trim().is_empty() {
                        blocks.push(text_block(text.clone()));
                    }
                    continue;
                }
                let Some(record) = item.as_object() else {
                    continue;
                };
                if let Some(block) = claude_content_block(record) {
                    blocks.push(block);
                }
            }
            blocks
        }
        _ => Vec::new(),
    }
}

fn claude_content_is_reasoning_only(content: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = content else {
        return false;
    };
    !items.is_empty()
        && items.iter().all(|item| {
            item.as_object()
                .and_then(|record| record.get("type"))
                .and_then(Value::as_str)
                == Some("thinking")
        })
}

/*
CDXC:SessionChatCore 2026-08-01:
`input_text`/`output_text`/`summary_text` are the Responses-API spellings Codex
writes inside `response_item` content arrays and inside `custom_tool_call_output`
payloads. They carry the same `text` field as Anthropic's `text` block, so the
shared mapper accepts all of them; without this a whole Codex lane decoded to
nothing. `encrypted_content` is deliberately unmapped — it has no readable text.
*/
fn claude_content_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
    match record.get("type").and_then(Value::as_str) {
        Some("text" | "input_text" | "output_text" | "summary_text") => {
            extract_string(record.get("text")).map(text_block)
        }
        Some("thinking") => {
            // Reasoning surfaces as a text block; the message role marks it as reasoning.
            extract_string(record.get("thinking"))
                .or_else(|| extract_string(record.get("text")))
                .map(text_block)
        }
        Some("tool_use") => Some(SessionChatBlock::ToolCall {
            name: extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string()),
            input: record.get("input").cloned().unwrap_or(Value::Null),
        }),
        Some("tool_result") => Some(SessionChatBlock::ToolResult {
            output: tool_result_output(record.get("content")),
            is_error: if record.get("is_error") == Some(&Value::Bool(true)) {
                Some(true)
            } else {
                None
            },
        }),
        Some("image" | "input_image") => image_ref_block(record),
        _ => None,
    }
}

const PASTED_IMAGE_ALT: &str = "Pasted image";

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
fn image_ref_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
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

/// Claude stamps `uuid`/`parentUuid` on every non-sidechain row, including the
/// `system` and `attachment` rows a prompt can hang off, so the whole tree is
/// readable from the same scan the decoder already runs.
fn claude_transcript_lineage(line: &str) -> Option<TranscriptLineage> {
    let record = parse_json_object(line)?;
    if record.get("isSidechain") == Some(&Value::Bool(true)) {
        return None;
    }
    Some(TranscriptLineage {
        id: extract_string(record.get("uuid"))?,
        parent_id: extract_string(record.get("parentUuid")),
    })
}

fn claude_interrupted_message_id(record: &Map<String, Value>) -> Option<String> {
    if record.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    extract_string(record.get("interruptedMessageId"))
}

pub fn decode_claude_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let role = record.get("type").and_then(Value::as_str)?;
    if role != "user" && role != "assistant" {
        return None;
    }
    let timestamp = parse_timestamp(record.get("timestamp"));
    let record_message_id =
        extract_string(record.get("uuid")).unwrap_or_else(|| fallback_id.to_string());

    // (A) Interruption marker — highest precedence.
    if claude_interrupted_message_id(&record).is_some() {
        return Some(SessionChatMessage {
            id: record_message_id,
            role: SessionChatRole::System,
            blocks: vec![text_block(INTERRUPTED_STATUS_TEXT)],
            timestamp,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        });
    }

    let message = record.get("message").and_then(Value::as_object);
    let content = message.and_then(|inner| inner.get("content"));
    let decoded_blocks = claude_content_blocks(content);
    if decoded_blocks.is_empty() {
        return None;
    }

    // (B) Injected/meta user turns keep only genuine tool-result output.
    let is_injected_user_turn = role == "user"
        && (record.get("isMeta") == Some(&Value::Bool(true))
            || record.get("isSynthetic") == Some(&Value::Bool(true))
            || record.get("isCompactSummary") == Some(&Value::Bool(true)));
    let blocks: Vec<SessionChatBlock> = if is_injected_user_turn {
        decoded_blocks
            .into_iter()
            .filter(is_tool_result_block)
            .collect()
    } else {
        decoded_blocks
    };
    if blocks.is_empty() {
        return None;
    }

    /*
    CDXC:SessionChatCore 2026-08-01:
    `uuid` is per-ROW; `message.id` is per-API-RESPONSE and is shared by every
    row Claude writes for one assistant turn. Falling back to `message.id` gave
    several rows the same chat id, and the client assembler's id-dedup then
    dropped all but the first — messages silently missing from chat while the
    terminal showed them. The fallback is the reader-supplied
    `<path>:<zero-padded byte offset>` id instead: unique per line and identical
    from every read path, so re-emitted tails still dedup correctly.
    */
    let message_id = extract_string(record.get("uuid"));
    let final_role = if role == "user" {
        let only_tool_results = blocks.iter().all(is_tool_result_block);
        if only_tool_results && !blocks.is_empty() {
            SessionChatRole::Tool
        } else {
            SessionChatRole::User
        }
    } else if claude_content_is_reasoning_only(content) {
        SessionChatRole::Reasoning
    } else {
        SessionChatRole::Assistant
    };
    Some(SessionChatMessage {
        id: message_id.unwrap_or_else(|| fallback_id.to_string()),
        role: final_role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
    })
}

// ---------------------------------------------------------------------------
// Codex decoder (upstream chat spec §2.3)
// ---------------------------------------------------------------------------

const CODEX_EVENT_TURN_STARTED: &str = "task_started";
const CODEX_EVENT_TURN_COMPLETE: &str = "task_complete";
const CODEX_EVENT_TURN_ABORTED: &str = "turn_aborted";

pub fn decode_codex_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let payload = as_record(record.get("payload"))?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let base_id = extract_string(payload.get("id")).unwrap_or_else(|| fallback_id.to_string());
    match record.get("type").and_then(Value::as_str) {
        Some("response_item") => codex_response_item(payload, base_id, timestamp),
        Some("event_msg") => codex_event_message(payload, base_id, timestamp),
        _ => None,
    }
}

/*
CDXC:SessionChatCore 2026-08-01:
The `message` response item is DELIBERATELY not decoded. A Codex rollout carries
every visible turn twice: once as `event_msg`/`user_message`+`agent_message` and
once as `response_item`/`message`. Measured over 4,881 local rollouts spanning
2026-03..2026-08, `response_item` assistant messages equal `event_msg`
agent_messages one-for-one in EVERY file, and `response_item` user messages are
always a superset of `event_msg` user_messages whose extra rows are exclusively
harness-injected envelopes (AGENTS.md instructions, `<environment_context>`,
`<recommended_plugins>`, `<skill>`) plus `developer` system prompts. So the
event lane loses no human/assistant text and additionally carries the prompt
WITHOUT the injected preamble, while decoding both lanes would double every
turn. The two are redundant, never complementary.

2026-08-07 update: newer Codex builds indeed stopped writing that event lane,
but replaced it with `event_msg`/`item_completed` (UserMessage/AgentMessage
items) rather than promoting this `message` lane — see codex_event_message.
This arm therefore STAYS undecoded: the `message` lane is still the duplicate,
envelope-carrying twin in both old- and new-style rollouts.
*/
fn codex_response_item(
    payload: &Map<String, Value>,
    id: String,
    timestamp: Option<i64>,
) -> Option<SessionChatMessage> {
    let transcript_message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
    };
    match payload.get("type").and_then(Value::as_str) {
        Some("reasoning") => {
            let text = extract_string(payload.get("text"))
                .or_else(|| codex_summary_text(payload.get("summary")))?;
            Some(transcript_message(
                SessionChatRole::Reasoning,
                vec![text_block(text)],
            ))
        }
        /*
        `custom_tool_call` is how current Codex records its freeform tool lane
        (`exec`, `apply_patch`, …): `name` plus a raw `input` STRING instead of
        `arguments` JSON. It is as frequent as `function_call` in real rollouts
        and used to decode to nothing at all, taking its paired output with it.
        */
        Some("function_call" | "local_shell_call" | "custom_tool_call") => {
            let name = extract_string(payload.get("name")).unwrap_or_else(|| "tool".to_string());
            Some(transcript_message(
                SessionChatRole::Assistant,
                vec![SessionChatBlock::ToolCall {
                    name,
                    input: codex_call_input(payload),
                }],
            ))
        }
        Some("function_call_output" | "custom_tool_call_output") => Some(transcript_message(
            SessionChatRole::Tool,
            vec![codex_tool_result(payload.get("output"))],
        )),
        // Hosted-tool lanes: the call carries no name field of its own.
        Some("web_search_call") => Some(transcript_message(
            SessionChatRole::Assistant,
            vec![SessionChatBlock::ToolCall {
                name: "web_search".to_string(),
                input: codex_call_input(payload),
            }],
        )),
        Some("tool_search_call") => Some(transcript_message(
            SessionChatRole::Assistant,
            vec![SessionChatBlock::ToolCall {
                name: "tool_search".to_string(),
                input: codex_call_input(payload),
            }],
        )),
        Some("tool_search_output") => Some(transcript_message(
            SessionChatRole::Tool,
            vec![SessionChatBlock::ToolResult {
                output: codex_tool_search_output(payload.get("tools")),
                is_error: None,
            }],
        )),
        /*
        Multi-agent traffic: one agent's message to another. The payload is
        mostly `encrypted_content`, but the readable `input_text` header names
        the sender/recipient, which is the only in-chat signal that a teammate
        replied. Rendered as a system row.
        */
        Some("agent_message") => {
            let blocks = claude_content_blocks(payload.get("content"));
            if blocks.is_empty() {
                return None;
            }
            Some(transcript_message(SessionChatRole::System, blocks))
        }
        _ => None,
    }
}

/// `tool_search_output` lists discovered tool namespaces rather than text.
fn codex_tool_search_output(tools: Option<&Value>) -> String {
    let Some(Value::Array(items)) = tools else {
        return String::new();
    };
    if items.is_empty() {
        return "No tools found".to_string();
    }
    let names: Vec<String> = items
        .iter()
        .filter_map(|item| extract_string(item.as_object().and_then(|inner| inner.get("name"))))
        .collect();
    if names.is_empty() {
        format!("{} tools", items.len())
    } else {
        names.join("\n")
    }
}

fn codex_event_message(
    payload: &Map<String, Value>,
    id: String,
    timestamp: Option<i64>,
) -> Option<SessionChatMessage> {
    let transcript_message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
    };
    match payload.get("type").and_then(Value::as_str) {
        Some(CODEX_EVENT_TURN_ABORTED) => Some(transcript_message(
            SessionChatRole::System,
            vec![text_block(INTERRUPTED_STATUS_TEXT)],
        )),
        Some("user_message") => extract_string(payload.get("message"))
            .map(|text| transcript_message(SessionChatRole::User, vec![text_block(text)])),
        Some("agent_message") => extract_string(payload.get("message"))
            .map(|text| transcript_message(SessionChatRole::Assistant, vec![text_block(text)])),
        /*
        CDXC:SessionChatCore 2026-08-07:
        Newer Codex builds (observed with gpt-5.6 rollouts, 2026-08) STOPPED
        writing the `user_message`/`agent_message` event lane entirely; visible
        turns instead arrive as `item_completed` events whose `item` is a
        `UserMessage`/`AgentMessage`. Without this arm every human/assistant
        message in a new-style rollout decoded to nothing — chat showed only
        reasoning summaries and tool calls, while the terminal showed the full
        final answer. The two lanes are mutually exclusive per file (measured
        over the 60 most recent local rollouts: 46 old-lane-only, 9
        new-lane-only, 0 both), so decoding both cannot double a turn. Like the
        old event lane, `item_completed` UserMessages carry only the visible
        typed prompt — never the harness-injected AGENTS.md/environment
        envelopes. Reasoning/CommandExecution/McpToolCall/FileChange items are
        deliberately NOT decoded here: new-style rollouts still write their
        `response_item` twins, which remain the sole source for those lanes.
        */
        Some("item_completed") => {
            let item = as_record(payload.get("item"))?;
            let role = match item.get("type").and_then(Value::as_str) {
                Some("UserMessage") => SessionChatRole::User,
                Some("AgentMessage") => SessionChatRole::Assistant,
                _ => return None,
            };
            let blocks = codex_item_content_blocks(item.get("content"));
            if blocks.is_empty() {
                return None;
            }
            Some(SessionChatMessage {
                id: extract_string(item.get("id")).unwrap_or(id),
                role,
                blocks,
                timestamp,
                source: SessionChatSource::Transcript,
                turn_id: None,
                byte_offset: None,
            })
        }
        _ => None,
    }
}

/*
`item_completed` content blocks use their own spellings: `Text` (capitalized)
in AgentMessages, `text` in UserMessages, and `skill` for a slash-skill
invocation chip (`{type:"skill",name,path}`). Anything else falls through to
the shared mapper so future image/attachment blocks render like Claude's.
*/
fn codex_item_content_blocks(content: Option<&Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(items)) = content else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|entry| {
            let record = entry.as_object()?;
            match record.get("type").and_then(Value::as_str) {
                Some("Text") => extract_string(record.get("text")).map(text_block),
                Some("skill") => extract_string(record.get("name"))
                    .map(|name| text_block(format!("Skill: {name}"))),
                _ => claude_content_block(record),
            }
        })
        .collect()
}

fn codex_call_input(payload: &Map<String, Value>) -> Value {
    if let Some(arguments) = payload.get("arguments") {
        return arguments.clone();
    }
    payload
        .get("input")
        .filter(|value| !value.is_null())
        .or_else(|| payload.get("action").filter(|value| !value.is_null()))
        .cloned()
        .unwrap_or(Value::Null)
}

fn codex_tool_result(output: Option<&Value>) -> SessionChatBlock {
    let record = output.and_then(Value::as_object);
    let is_error = record.is_some_and(|inner| {
        inner.get("success") == Some(&Value::Bool(false))
            || inner.get("is_error") == Some(&Value::Bool(true))
    });
    let content = record
        .and_then(|inner| inner.get("content").or_else(|| inner.get("output")))
        .or(output);
    SessionChatBlock::ToolResult {
        output: tool_result_output(content),
        is_error: if is_error { Some(true) } else { None },
    }
}

fn codex_summary_text(summary: Option<&Value>) -> Option<String> {
    let Some(Value::Array(items)) = summary else {
        return None;
    };
    let parts: Vec<String> = items
        .iter()
        .filter_map(|item| {
            extract_string(item.as_object().and_then(|inner| inner.get("text")))
                .or_else(|| extract_string(Some(item)))
        })
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

// ---------------------------------------------------------------------------
// Grok decoder (upstream chat spec §2.4)
// ---------------------------------------------------------------------------

pub fn decode_grok_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let record_type = extract_string(record.get("type"))?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let record_id = extract_string(record.get("id"));
    // Grok rows often omit timestamps and only some carry ids — prefix with the
    // JSONL position so ids stay unique and ordered.
    let id = match record_id {
        Some(record_id) => format!("{fallback_id}:{record_id}"),
        None => fallback_id.to_string(),
    };
    let transcript_message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
    };

    match record_type.as_str() {
        "user" | "assistant" => {
            if record_type == "user"
                && (has_non_empty_synthetic_reason(&record)
                    || is_grok_bootstrap_context(record.get("content")))
            {
                return None;
            }
            let raw_blocks = claude_content_blocks(record.get("content"));
            let blocks: Vec<SessionChatBlock> = if record_type == "user" {
                raw_blocks
                    .into_iter()
                    .flat_map(normalize_grok_user_query_block)
                    .collect()
            } else {
                raw_blocks
            };
            if blocks.is_empty() {
                let tool_blocks = grok_tool_call_blocks(record.get("tool_calls"));
                if tool_blocks.is_empty() {
                    return None;
                }
                return Some(transcript_message(SessionChatRole::Assistant, tool_blocks));
            }
            if record_type == "assistant" {
                let mut combined = blocks;
                combined.extend(grok_tool_call_blocks(record.get("tool_calls")));
                return Some(transcript_message(SessionChatRole::Assistant, combined));
            }
            Some(transcript_message(SessionChatRole::User, blocks))
        }
        "reasoning" => {
            let text = extract_string(record.get("text"))
                .or_else(|| grok_summary_text(record.get("summary")))
                .or_else(|| {
                    extract_string(as_record(record.get("content")).and_then(|c| c.get("text")))
                })?;
            if text.trim().is_empty() {
                return None;
            }
            Some(transcript_message(
                SessionChatRole::Reasoning,
                vec![text_block(text)],
            ))
        }
        "backend_tool_call" | "tool_call" => {
            let name = extract_string(
                as_record(record.get("kind")).and_then(|kind| kind.get("tool_type")),
            )
            .or_else(|| extract_string(record.get("name")))
            .or_else(|| extract_string(record.get("tool")))
            .unwrap_or_else(|| "tool".to_string());
            let input = record
                .get("kind")
                .filter(|value| !value.is_null())
                .or_else(|| record.get("arguments").filter(|value| !value.is_null()))
                .or_else(|| record.get("input").filter(|value| !value.is_null()))
                .cloned()
                .unwrap_or(Value::Null);
            Some(transcript_message(
                SessionChatRole::Assistant,
                vec![SessionChatBlock::ToolCall { name, input }],
            ))
        }
        "tool_result" => {
            let content = record
                .get("content")
                .filter(|value| !value.is_null())
                .or_else(|| record.get("output").filter(|value| !value.is_null()))
                .or_else(|| record.get("result").filter(|value| !value.is_null()));
            let is_error = record.get("is_error") == Some(&Value::Bool(true))
                || record.get("isError") == Some(&Value::Bool(true));
            Some(transcript_message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: tool_result_output(content),
                    is_error: if is_error { Some(true) } else { None },
                }],
            ))
        }
        _ => None,
    }
}

fn grok_tool_call_blocks(value: Option<&Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    let mut blocks: Vec<SessionChatBlock> = Vec::new();
    for item in items {
        let Some(record) = item.as_object() else {
            continue;
        };
        let name = extract_string(record.get("name"))
            .or_else(|| extract_string(record.get("tool")))
            .unwrap_or_else(|| "tool".to_string());
        let mut input = record
            .get("arguments")
            .filter(|value| !value.is_null())
            .or_else(|| record.get("input").filter(|value| !value.is_null()))
            .or_else(|| record.get("args").filter(|value| !value.is_null()))
            .cloned()
            .unwrap_or(Value::Null);
        if let Value::String(text) = &input {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                input = parsed;
            }
        }
        blocks.push(SessionChatBlock::ToolCall { name, input });
    }
    blocks
}

fn grok_summary_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts: Vec<String> = items
                .iter()
                .filter_map(|item| {
                    let record = item.as_object();
                    extract_string(record.and_then(|inner| inner.get("text"))).or_else(|| {
                        extract_string(record.and_then(|inner| inner.get("summary_text")))
                    })
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

fn has_non_empty_synthetic_reason(record: &Map<String, Value>) -> bool {
    record
        .get("synthetic_reason")
        .and_then(Value::as_str)
        .is_some_and(|reason| !reason.trim().is_empty())
}

fn standalone_text_content(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) if items.len() == 1 => {
            let record = items[0].as_object()?;
            if record.get("type").and_then(Value::as_str) == Some("text") {
                record
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn is_grok_bootstrap_context(content: Option<&Value>) -> bool {
    let Some(text) = standalone_text_content(content) else {
        return false;
    };
    let normalized = text.trim().to_lowercase();
    if !normalized.starts_with("<user_info>") {
        return false;
    }
    let Some(end) = normalized.find("</user_info>") else {
        return false;
    };
    let remainder = normalized[end + "</user_info>".len()..].trim().to_string();
    // Grok 0.2.93 appends a git snapshot; reject ONLY that known envelope.
    remainder.is_empty()
        || (remainder.starts_with("<git_status>") && remainder.ends_with("</git_status>"))
}

fn normalize_grok_user_query_block(block: SessionChatBlock) -> Vec<SessionChatBlock> {
    let SessionChatBlock::Text { text } = &block else {
        return vec![block];
    };
    let stripped = strip_grok_user_query_envelope(text);
    if stripped.trim().is_empty() {
        return Vec::new();
    }
    match split_grok_pasted_image_query(&stripped) {
        None => {
            if stripped == *text {
                vec![block]
            } else {
                vec![text_block(stripped)]
            }
        }
        Some((path, query)) => {
            let mut blocks = vec![SessionChatBlock::ImageRef {
                path: Some(path),
                url: None,
                alt: None,
            }];
            if !query.is_empty() {
                blocks.push(text_block(query));
            }
            blocks
        }
    }
}

fn strip_grok_user_query_envelope(text: &str) -> String {
    let opener = "<user_query>";
    let closer = "</user_query>";
    let lower = text.to_lowercase();
    let Some(start) = lower.find(opener) else {
        return text.to_string();
    };
    let body_start = start + opener.len();
    match lower[body_start..].find(closer) {
        None => text[body_start..].trim().to_string(),
        Some(relative_end) => text[body_start..body_start + relative_end]
            .trim()
            .to_string(),
    }
}

/*
Manual port of the upstream chat spec's pasted-image regex (no regex crate here):
^((win-drive|/|UNC)(.*?[\\/])?ghostex-paste-[^\\/\r\n]+?\.png)([\s\S]*)$ with
case-insensitive matching. The token must sit directly after a path separator
(every prefix alternative ends in one), the file name may not cross a
separator or newline, and the path portion before the token may not contain a
newline.
*/
fn split_grok_pasted_image_query(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    let valid_start = text.starts_with('/')
        || text.starts_with('\\')
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'/' | b'\\'));
    if !valid_start {
        return None;
    }
    let lower = text.to_lowercase();
    let mut search_from = 0usize;
    while let Some(found) = lower[search_from..].find(GROK_PASTED_IMAGE_TOKEN) {
        let token_start = search_from + found;
        let preceded_by_separator =
            token_start > 0 && matches!(bytes[token_start - 1], b'/' | b'\\');
        if preceded_by_separator && !text[..token_start].contains(['\r', '\n']) {
            let name_start = token_start + GROK_PASTED_IMAGE_TOKEN.len();
            if let Some(png_relative) = lower[name_start..].find(".png") {
                let name_segment = &text[name_start..name_start + png_relative];
                if !name_segment.is_empty() && !name_segment.contains(['/', '\\', '\r', '\n']) {
                    let end = name_start + png_relative + ".png".len();
                    return Some((text[..end].to_string(), text[end..].trim().to_string()));
                }
            }
        }
        search_from = token_start + 1;
    }
    None
}

// ---------------------------------------------------------------------------
// Pi-family decoder
// ---------------------------------------------------------------------------

fn pi_inline_image_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
    let has_inline_bytes = record
        .get("data")
        .is_some_and(|data| data.as_str().is_some_and(|text| !text.trim().is_empty()));
    if !has_inline_bytes {
        return image_ref_block(record);
    }
    Some(SessionChatBlock::ImageRef {
        path: None,
        url: None,
        alt: Some(PASTED_IMAGE_ALT.to_string()),
    })
}

fn pi_message_content(content: Option<&Value>) -> (Vec<SessionChatBlock>, Vec<SessionChatBlock>) {
    let mut visible = Vec::new();
    let mut reasoning = Vec::new();
    let items: Vec<&Value> = match content {
        Some(Value::Array(items)) => items.iter().collect(),
        Some(value) => vec![value],
        None => Vec::new(),
    };
    for item in items {
        if let Value::String(text) = item {
            if !text.trim().is_empty() {
                visible.push(text_block(text.clone()));
            }
            continue;
        }
        let Some(record) = item.as_object() else {
            continue;
        };
        match record.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = extract_string(record.get("text")) {
                    visible.push(text_block(text));
                }
            }
            Some("thinking") => {
                if let Some(text) = extract_string(record.get("thinking")) {
                    reasoning.push(text_block(text));
                }
            }
            Some("toolCall") => {
                visible.push(SessionChatBlock::ToolCall {
                    name: extract_string(record.get("name")).unwrap_or_else(|| "tool".to_string()),
                    input: record.get("arguments").cloned().unwrap_or(Value::Null),
                });
            }
            Some("image") => {
                if let Some(block) = pi_inline_image_block(record) {
                    visible.push(block);
                }
            }
            _ => {}
        }
    }
    (visible, reasoning)
}

pub fn decode_pi_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let record_type = record.get("type").and_then(Value::as_str)?;
    let id = extract_string(record.get("id")).unwrap_or_else(|| fallback_id.to_string());
    let record_timestamp = parse_timestamp(record.get("timestamp"));
    let transcript_message = |role, blocks, timestamp| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
    };

    if record_type == "compaction" || record_type == "branch_summary" {
        let summary = extract_string(record.get("summary"))?;
        return Some(transcript_message(
            SessionChatRole::System,
            vec![text_block(summary)],
            record_timestamp,
        ));
    }
    if record_type == "custom_message" {
        if record.get("display") == Some(&Value::Bool(false)) {
            return None;
        }
        let (blocks, _) = pi_message_content(record.get("content"));
        if blocks.is_empty() {
            return None;
        }
        return Some(transcript_message(
            SessionChatRole::System,
            blocks,
            record_timestamp,
        ));
    }
    if record_type != "message" {
        return None;
    }

    let message = as_record(record.get("message"))?;
    let role = message.get("role").and_then(Value::as_str)?;
    let timestamp = parse_timestamp(message.get("timestamp")).or(record_timestamp);
    let (mut blocks, reasoning) = pi_message_content(message.get("content"));
    match role {
        "user" => {
            if blocks.is_empty() {
                return None;
            }
            Some(transcript_message(SessionChatRole::User, blocks, timestamp))
        }
        "assistant" => {
            if blocks.is_empty() {
                if let Some(error) = extract_string(message.get("errorMessage")) {
                    blocks.push(text_block(error));
                } else if !reasoning.is_empty() {
                    return Some(transcript_message(
                        SessionChatRole::Reasoning,
                        reasoning,
                        timestamp,
                    ));
                } else {
                    return None;
                }
            }
            Some(transcript_message(
                SessionChatRole::Assistant,
                blocks,
                timestamp,
            ))
        }
        "toolResult" => {
            let output = tool_result_output(message.get("content"));
            let is_error = message.get("isError") == Some(&Value::Bool(true));
            let mut tool_blocks = vec![SessionChatBlock::ToolResult {
                output,
                is_error: if is_error { Some(true) } else { None },
            }];
            tool_blocks.extend(
                blocks
                    .into_iter()
                    .filter(|block| matches!(block, SessionChatBlock::ImageRef { .. })),
            );
            Some(transcript_message(
                SessionChatRole::Tool,
                tool_blocks,
                timestamp,
            ))
        }
        "bashExecution" => {
            let command = extract_string(message.get("command")).unwrap_or_default();
            let output = extract_string(message.get("output")).unwrap_or_default();
            let text = match (command.is_empty(), output.is_empty()) {
                (true, true) => return None,
                (false, true) => format!("$ {command}"),
                (true, false) => output,
                (false, false) => format!("$ {command}\n{output}"),
            };
            Some(transcript_message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: text,
                    is_error: None,
                }],
                timestamp,
            ))
        }
        "custom" => {
            if message.get("display") == Some(&Value::Bool(false)) || blocks.is_empty() {
                return None;
            }
            Some(transcript_message(
                SessionChatRole::System,
                blocks,
                timestamp,
            ))
        }
        "branchSummary" | "compactionSummary" => {
            let summary = extract_string(message.get("summary"))?;
            Some(transcript_message(
                SessionChatRole::System,
                vec![text_block(summary)],
                timestamp,
            ))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Noise filter (upstream chat spec §9.1) — needed by the Claude lifecycle decoder.
// ---------------------------------------------------------------------------

const KNOWN_HARNESS_TAG_NAMES: &[&str] = &[
    "agent-message",
    "bash-input",
    "bash-stderr",
    "bash-stdout",
    "command-args",
    "command-message",
    "command-name",
    "cross-session-message",
    "fork-boilerplate",
    "local-command-caveat",
    "local-command-stderr",
    "local-command-stdout",
    "mcp-polling-update",
    "mcp-resource-update",
    "system-reminder",
    "task-notification",
    "teammate-message",
    "user-memory-input",
    "user-prompt-submit-hook",
];

const HARNESS_INJECTED_TURN_PREFIXES: &[&str] = &[
    "<channel source=",
    "[request interrupted",
    "a message arrived from ",
    "another claude session sent a message",
    "no response requested.",
    "caveat: the messages below were generated by the user while running local commands",
    "this session is being continued from a previous conversation",
];

fn message_text(message: &SessionChatMessage) -> String {
    message
        .blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
        .trim()
        .to_string()
}

fn leading_tag_name(normalized: &str) -> Option<&str> {
    let rest = normalized.strip_prefix('<')?;
    let first = rest.chars().next()?;
    if !first.is_ascii_lowercase() {
        return None;
    }
    let end = rest
        .find(|ch: char| !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'))
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let terminator = rest[end..].chars().next();
    match terminator {
        None => Some(&rest[..end]),
        Some(ch) if ch.is_whitespace() || ch == '>' => Some(&rest[..end]),
        Some(_) => None,
    }
}

fn is_known_harness_injected_user_turn_text(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    if let Some(tag) = leading_tag_name(&normalized) {
        if KNOWN_HARNESS_TAG_NAMES.contains(&tag) {
            return true;
        }
    }
    HARNESS_INJECTED_TURN_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}

pub fn is_noise_message(message: &SessionChatMessage) -> bool {
    if message.role != SessionChatRole::User && message.role != SessionChatRole::System {
        return false;
    }
    if message.blocks.iter().any(|block| {
        matches!(
            block,
            SessionChatBlock::ToolCall { .. } | SessionChatBlock::ToolResult { .. }
        )
    }) {
        return false;
    }
    is_known_harness_injected_user_turn_text(&message_text(message))
}

// ---------------------------------------------------------------------------
// Turn lifecycle decoders (upstream chat spec §3)
// ---------------------------------------------------------------------------

pub fn decode_codex_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) != Some("event_msg") {
        return None;
    }
    let payload = as_record(record.get("payload"))?;
    let state = match payload.get("type").and_then(Value::as_str) {
        Some(CODEX_EVENT_TURN_STARTED) => SessionChatTurnLifecycleState::Working,
        Some(CODEX_EVENT_TURN_ABORTED) => SessionChatTurnLifecycleState::Interrupted,
        Some(CODEX_EVENT_TURN_COMPLETE) => SessionChatTurnLifecycleState::Completed,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: extract_string(payload.get("turn_id")).unwrap_or_else(|| fallback_id.to_string()),
        timestamp: parse_timestamp(record.get("timestamp")),
    })
}

const CLAUDE_TERMINAL_STOP_REASONS: &[&str] =
    &["end_turn", "max_tokens", "stop_sequence", "refusal"];
// NOTE: 'tool_use' is deliberately ABSENT — it is mid-turn.

fn assistant_has_renderable_content(message: Option<&Map<String, Value>>) -> bool {
    let content = message.and_then(|inner| inner.get("content"));
    match content {
        Some(Value::String(text)) => !text.trim().is_empty(),
        Some(Value::Array(items)) => items.iter().any(|item| {
            let Some(record) = item.as_object() else {
                return false;
            };
            match record.get("type").and_then(Value::as_str) {
                Some("text") => record
                    .get("text")
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.trim().is_empty()),
                Some("thinking" | "redacted_thinking") => true,
                _ => false,
            }
        }),
        _ => false,
    }
}

fn assistant_has_tool_use(message: Option<&Map<String, Value>>) -> bool {
    let Some(Value::Array(items)) = message.and_then(|inner| inner.get("content")) else {
        return false;
    };
    items.iter().any(|item| {
        item.as_object()
            .and_then(|record| record.get("type"))
            .and_then(Value::as_str)
            == Some("tool_use")
    })
}

pub fn decode_claude_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let message = record.get("message").and_then(Value::as_object);
    let timestamp = parse_timestamp(record.get("timestamp"));

    // 1. Interrupt beats everything.
    if let Some(interrupted_message_id) = claude_interrupted_message_id(&record) {
        return Some(SessionChatTurnLifecycle {
            state: SessionChatTurnLifecycleState::Interrupted,
            turn_id: interrupted_message_id,
            timestamp,
        });
    }

    // 2. Assistant rows.
    if record.get("type").and_then(Value::as_str) == Some("assistant") {
        let stop_reason = message
            .and_then(|inner| inner.get("stop_reason"))
            .and_then(Value::as_str);
        let stop_reason_absent = message
            .and_then(|inner| inner.get("stop_reason"))
            .map(Value::is_null)
            .unwrap_or(true);
        let is_terminal = stop_reason
            .is_some_and(|reason| CLAUDE_TERMINAL_STOP_REASONS.contains(&reason))
            || (stop_reason_absent
                && assistant_has_renderable_content(message)
                && !assistant_has_tool_use(message)); // ← tool_use-is-not-terminal rule
        if is_terminal {
            return Some(SessionChatTurnLifecycle {
                state: SessionChatTurnLifecycleState::Completed,
                turn_id: extract_string(record.get("uuid"))
                    .or_else(|| extract_string(message.and_then(|inner| inner.get("id"))))
                    .unwrap_or_else(|| fallback_id.to_string()),
                timestamp,
            });
        }
        return None; // NOT a boundary; do NOT settle.
    }

    // 3. User rows — possible new generation.
    if record.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    let decoded = decode_claude_transcript_line(line, fallback_id)?;
    if decoded.role != SessionChatRole::User || decoded.blocks.iter().any(is_tool_result_block) {
        return None; // tool-result user rows continue the ACTIVE turn
    }
    if is_noise_message(&decoded) {
        return None; // harness noise is not a new generation
    }
    Some(SessionChatTurnLifecycle {
        state: SessionChatTurnLifecycleState::Working,
        turn_id: decoded.id,
        timestamp,
    })
}

// ---------------------------------------------------------------------------
// Reverse tail reader (upstream chat spec §4)
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
}

struct TailLineAccumulator {
    // Reverse-ordered parts of the line currently being assembled.
    parts: Vec<Vec<u8>>,
    bytes: usize,
    oversized: bool,
}

impl TailLineAccumulator {
    fn new() -> Self {
        Self {
            parts: Vec::new(),
            bytes: 0,
            oversized: false,
        }
    }

    fn retain_part(&mut self, part: &[u8], oversized_record_count: &mut usize) {
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

    fn take_line(&mut self) -> Option<String> {
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

    fn reset(&mut self) {
        self.parts.clear();
        self.bytes = 0;
        self.oversized = false;
    }
}

fn find_last_complete_line_end(file: &File, end: u64) -> std::io::Result<u64> {
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

    let decode_line = |accumulator: &mut TailLineAccumulator,
                       line_offset: u64,
                       newest_first: &mut Vec<(SessionChatMessage, u64)>,
                       lifecycle: &mut Option<SessionChatTurnLifecycle>,
                       ignore_next_malformed_record: &mut bool,
                       malformed_record_count: &mut usize,
                       parents_of_newer_messages: &mut HashSet<String>,
                       parents_of_newer_prompts: &mut HashSet<String>| {
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
        let row_lineage = lineage.and_then(|extract| extract(&line));
        if let Some(row_lineage) = row_lineage {
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
        );
    }

    newest_first.reverse();
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
    })
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
fn read_pi_session_chat_transcript_tail_file(
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
    }

    /// Ids of already-published prompts that a later row proved abandoned.
    /// Drained by the caller, which removes them from the batch it is about to
    /// emit and reports the rest to clients.
    pub fn take_superseded_prompt_ids(&mut self) -> Vec<String> {
        std::mem::take(&mut self.superseded_prompt_ids)
    }

    fn observe_lineage(&mut self, row: &TranscriptLineage, message: Option<&SessionChatMessage>) {
        let Some(message) = message else {
            // Hook `attachment` and bookkeeping rows are neither an answer nor
            // a re-taken branch (see `superseded_prompt_id`).
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
        if message.role != SessionChatRole::User {
            return;
        }
        // A second PROMPT on the same parent means the branch was re-taken, so
        // the prompt that was waiting there was abandoned.
        if let Some(abandoned) = self.unanswered_prompt_by_parent.remove(&parent_id) {
            if abandoned != row.id {
                self.unanswered_prompt_parents.remove(&abandoned);
                self.superseded_prompt_ids.push(abandoned);
            }
        }
        self.unanswered_prompt_by_parent
            .insert(parent_id.clone(), row.id.clone());
        self.unanswered_prompt_parents
            .insert(row.id.clone(), parent_id);
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
                        if let Some(row) = extract(&line) {
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
fn windows_filetime_to_unix_ms(filetime: u64) -> i128 {
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

/// Hook-supplied `agentSessionPath` wins when it points at an existing .jsonl
/// file; otherwise fall back to the per-agent session-id search. Grok's hook
/// session file is `updates.jsonl`, while its conversation is stored in
/// `chat_history.jsonl`, so only the latter is a valid supplied Grok path.
pub fn resolve_session_chat_transcript_path(
    agent: SessionChatTranscriptAgent,
    agent_session_id: Option<&str>,
    agent_session_path: Option<&str>,
) -> Option<PathBuf> {
    if let Some(path) = agent_session_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let expanded = expand_home(path);
        let is_agent_transcript = agent != SessionChatTranscriptAgent::Grok
            || expanded.file_name().and_then(|name| name.to_str()) == Some("chat_history.jsonl");
        if is_agent_transcript
            && expanded
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("jsonl")
            && expanded.is_file()
        {
            return Some(expanded);
        }
    }
    let session_id = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    match agent {
        SessionChatTranscriptAgent::Claude => find_claude_chat_transcript(session_id),
        SessionChatTranscriptAgent::Codex => {
            crate::agent_transcripts::find_codex_transcript(session_id)
        }
        SessionChatTranscriptAgent::Grok => find_grok_chat_transcript(session_id),
        SessionChatTranscriptAgent::Pi => find_pi_family_chat_transcript(session_id),
    }
}

fn configured_agent_directory(env_key: &str, fallback: &str) -> PathBuf {
    std::env::var(env_key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value))
        .unwrap_or_else(|| home_dir().join(fallback))
}

fn find_pi_family_chat_transcript(session_id: &str) -> Option<PathBuf> {
    let pi_agent_dir = configured_agent_directory("PI_CODING_AGENT_DIR", ".pi/agent");
    let omp_agent_dir = std::env::var("PI_CONFIG_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| expand_home(&value).join("agent"))
        .unwrap_or_else(|| home_dir().join(".omp/agent"));
    let mut candidates = Vec::new();
    for agent_dir in [pi_agent_dir, omp_agent_dir] {
        collect_pi_family_transcript_candidates(&agent_dir, session_id, &mut candidates);
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    candidates.into_iter().next().map(|(path, _)| path)
}

fn collect_pi_family_transcript_candidates(
    agent_dir: &Path,
    session_id: &str,
    candidates: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    let suffix = format!("_{session_id}.jsonl");
    let mut collect_file = |path: PathBuf| {
        let file_name_matches = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name == format!("{session_id}.jsonl") || name.ends_with(&suffix));
        if !file_name_matches {
            return;
        }
        let Ok(metadata) = fs::metadata(&path) else {
            return;
        };
        if metadata.is_file() {
            candidates.push((
                path,
                metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            ));
        }
    };
    if let Ok(legacy_entries) = fs::read_dir(agent_dir) {
        for entry in legacy_entries.flatten() {
            collect_file(entry.path());
        }
    }
    let sessions_dir = agent_dir.join("sessions");
    let Ok(project_dirs) = fs::read_dir(sessions_dir) else {
        return;
    };
    for project_dir in project_dirs.flatten() {
        let path = project_dir.path();
        if path.is_file() {
            collect_file(path);
            continue;
        }
        let Ok(files) = fs::read_dir(path) else {
            continue;
        };
        for file in files.flatten() {
            collect_file(file.path());
        }
    }
}

/*
Claude filename stems are the camelCase `sessionId`, but hooks report the
snake_case `session_id`, and the two diverge on resumed/forked files
(transcript-format spec §1.1). Try the filename first; when it misses, scan
recent project transcripts for the hook id embedded in their records.
*/
fn find_claude_chat_transcript(session_id: &str) -> Option<PathBuf> {
    if let Some(path) = crate::agent_transcripts::find_claude_transcript(session_id) {
        return Some(path);
    }
    find_claude_transcript_by_embedded_session_id(session_id)
}

const CLAUDE_EMBEDDED_ID_SCAN_FILE_LIMIT: usize = 50;
const CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES: u64 = 256 * 1024;

fn find_claude_transcript_by_embedded_session_id(session_id: &str) -> Option<PathBuf> {
    let mut candidates: Vec<(PathBuf, std::time::SystemTime)> = Vec::new();
    for root in crate::agent_transcripts::claude_project_roots() {
        let Ok(project_dirs) = fs::read_dir(&root) else {
            continue;
        };
        for project_dir in project_dirs.flatten() {
            let Ok(files) = fs::read_dir(project_dir.path()) else {
                continue;
            };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("jsonl") {
                    continue;
                }
                let Ok(metadata) = file.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let modified = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                candidates.push((path, modified));
            }
        }
    }
    candidates.sort_by(|left, right| right.1.cmp(&left.1));
    for (path, _) in candidates
        .into_iter()
        .take(CLAUDE_EMBEDDED_ID_SCAN_FILE_LIMIT)
    {
        if head_declares_session_id(&path, session_id) {
            return Some(path);
        }
    }
    None
}

/// Sub-agent transcripts (`agent-<hash>.jsonl`) record the PARENT session's id
/// in every row, so they match an embedded-id scan for the main session and are
/// usually the newest file in the directory. They are never the session's own
/// transcript.
fn is_claude_sidechain_transcript_name(path: &Path) -> bool {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .is_some_and(|stem| stem.starts_with("agent-"))
}

/*
CDXC:SessionChatCore 2026-08-01:
The scan used to accept any file whose first 256KiB merely CONTAINED the literal
`"session_id":"<id>"`. Transcripts quote each other constantly (orchestrator
prompts, hook payloads, tool results), so an unrelated transcript could be
adopted and the chat would then tail a completely different conversation. The id
must now be the value of a record's OWN top-level `sessionId`/`session_id`
field, sidechain files are rejected outright, and rows flagged `isSidechain` do
not count.
*/
/// Bounded head read that never hands back a torn final line.
fn read_transcript_head_complete_lines(path: &Path, head_limit_bytes: u64) -> Option<String> {
    let file = File::open(path).ok()?;
    let head_length = file
        .metadata()
        .map(|metadata| metadata.len().min(head_limit_bytes))
        .unwrap_or(0) as usize;
    if head_length == 0 {
        return None;
    }
    let mut buffer = vec![0u8; head_length];
    read_exact_at(&file, &mut buffer, 0).ok()?;
    let head = String::from_utf8_lossy(&buffer);
    // The last line of a truncated head is very likely partial: never parse it.
    match head.rfind('\n') {
        Some(end) => Some(head[..end].to_string()),
        None if (head_length as u64) < head_limit_bytes => Some(head.into_owned()),
        None => None,
    }
}

fn head_declares_session_id(path: &Path, session_id: &str) -> bool {
    if is_claude_sidechain_transcript_name(path) {
        return false;
    }
    let Some(complete_head) =
        read_transcript_head_complete_lines(path, CLAUDE_EMBEDDED_ID_SCAN_HEAD_BYTES)
    else {
        return false;
    };
    for line in complete_head.lines() {
        let Some(record) = parse_json_object(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        let declared = extract_string(record.get("sessionId"))
            .or_else(|| extract_string(record.get("session_id")));
        if declared.as_deref() == Some(session_id) {
            return true;
        }
    }
    false
}

const GROK_SESSION_ID_MAX_LENGTH: usize = 128;

fn is_safe_grok_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= GROK_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// Grok layout: `$GROK_HOME/sessions/<url-encoded-cwd>/<session-id>/chat_history.jsonl`
/// (with a `summary.json` sidecar in the same directory).
fn find_grok_chat_transcript(session_id: &str) -> Option<PathBuf> {
    if !is_safe_grok_session_id(session_id) {
        return None;
    }
    let root = configured_agent_directory("GROK_HOME", ".grok").join("sessions");
    let entries = fs::read_dir(&root).ok()?;
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let candidate = entry.path().join(session_id).join("chat_history.jsonl");
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Successor transcript detection (Claude continuation / compaction)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatIdentity 2026-08-02:
Claude Code starts a NEW `<uuid>.jsonl` when a conversation is compacted or
resumed, and the registry only learns the new id from an agent hook. Hooks never
fire for background-job continuations, so a Ghostex session can keep its stored
`agentSessionId` pointing at a conversation that stopped receiving turns days
ago: chat then tails a dead file forever while the pane runs the successor.

Two traps make the naive checks useless:
  * the DEAD file keeps getting appends (`agent-name` / `mode` /
    `permission-mode` records with a null timestamp), so its mtime looks live —
    staleness MUST key on the last SUBSTANTIVE (`user`/`assistant`) record;
  * transcripts quote each other constantly, so "the file mentions the stale id"
    proves nothing. Lineage has to be structural.

The detector therefore proves, per candidate, BOTH:
  1. own identity — a head record whose OWN top-level `sessionId`/`session_id`
     is the candidate's own filename stem (the 6d27b5150 guardrail), and
  2. lineage to the SPECIFIC stale id — either a head record whose own
     `sessionId`/`session_id` field IS the stale id (Claude copies the
     predecessor id into the successor's records), or the compact-continuation
     user record combined with a `file-history-snapshot` inherited from the
     stale session.
Shared cwd is never a lineage signal: many concurrent sessions share one project
directory.

Codex is deliberately out of scope: a Codex rollout keeps ONE file per session
(`rollout-<ts>-<uuid>.jsonl`) and its resume/fork path re-plays `session_meta`
into a new rollout whose FIRST `session_meta` already carries the id the daemon
stores (the "first-session_meta-wins" rule in the decoders), so a Codex session
never silently outlives its transcript the way a Claude compaction does. Grok
writes one directory per session id with no continuation mechanism at all.
*/

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

fn is_uuid_transcript_stem(stem: &str) -> bool {
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

pub struct SessionChatStream {
    epoch: AtomicI64,
    seq: AtomicI64,
    /*
    CDXC:SessionChatCore 2026-08-01:
    Two threads publish into one seq counter: the follower task and hook ingest
    (prompt/activity state frames). Taking the seq and broadcasting as separate
    steps let thread B's frame reach the hub BEFORE thread A's lower seq, and
    the client treats an out-of-order seq as a gap and forces a full resync.
    Every publisher must therefore hold this lock across "take seq + broadcast",
    which `emit_sequenced` does; `begin_generation` takes it too so an epoch
    rollover can never interleave with an in-flight frame.
    */
    emit_order: std::sync::Mutex<()>,
}

impl SessionChatStream {
    pub fn new() -> Self {
        Self {
            epoch: AtomicI64::new(0),
            seq: AtomicI64::new(0),
            emit_order: std::sync::Mutex::new(()),
        }
    }

    /// Starts a new follower generation: bumps epoch, resets seq to 0.
    pub fn begin_generation(&self) -> i64 {
        let _order = self.lock_emit_order();
        self.seq.store(0, Ordering::SeqCst);
        self.epoch.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn current(&self) -> (i64, i64) {
        (
            self.epoch.load(Ordering::SeqCst),
            self.seq.load(Ordering::SeqCst),
        )
    }

    fn lock_emit_order(&self) -> std::sync::MutexGuard<'_, ()> {
        self.emit_order
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Allocates the next seq and publishes the frame it builds while holding
    /// the emission lock, so frames always reach the hub in seq order.
    /// `build` must not block: `broadcast` is a synchronous channel send.
    pub fn emit_sequenced<Build, Broadcast>(&self, build: Build, broadcast: Broadcast)
    where
        Build: FnOnce(i64) -> Value,
        Broadcast: FnOnce(Value),
    {
        let _order = self.lock_emit_order();
        let seq = self.seq.fetch_add(1, Ordering::SeqCst) + 1;
        broadcast(build(seq));
    }
}

impl Default for SessionChatStream {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Follower engine (upstream chat spec §5, poll-only: 1s reconcile owns liveness)
// ---------------------------------------------------------------------------

/// The session's CURRENT hook-derived state: the stored interactive prompt,
/// whether agent hooks report the session as working, and the provider session
/// identity the follower should be tailing right now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionChatLiveState {
    pub prompt: Option<SessionChatInteractivePrompt>,
    pub working: bool,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
}

/// Reads `SessionChatLiveState` so authoritative frames carry the live prompt
/// and working flag. Kept as a closure so the follower stays decoupled from the
/// domain repository.
pub type SessionChatStateReader = Arc<dyn Fn() -> SessionChatLiveState + Send + Sync>;

/// A proven successor transcript the follower wants to bind the session to.
#[derive(Clone, Debug, PartialEq)]
pub struct SessionChatIdentityAdoption {
    /// Identity the follower believes the registry holds right now. The write
    /// is compare-and-set against it so a real hook observation that landed in
    /// the meantime always wins.
    pub previous_agent_session_id: Option<String>,
    /// Filename stem of the stale transcript the lineage was proven against
    /// (usually the same id, but resolution can reach a file by other means).
    pub predecessor_transcript_session_id: String,
    pub agent_session_id: String,
    pub agent_session_path: String,
    pub lineage: &'static str,
    pub hops: usize,
}

/// What the follower reports about a successor search; the server maps it to a
/// gxserver log line.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionChatSuccessorNotice {
    Adopted(SessionChatIdentityAdoption),
    AdoptionRejected {
        agent_session_id: String,
        reason: &'static str,
    },
    Ambiguous {
        predecessor_session_id: String,
        candidate_session_ids: Vec<String>,
    },
    /// A proven successor exists but a live session already owns it.
    OwnedByAnotherSession {
        predecessor_session_id: String,
        candidate_session_ids: Vec<String>,
    },
}

/*
CDXC:SessionChatIdentity 2026-08-02:
Successor adoption needs three things the follower must NOT own itself (they all
touch the domain database / logger): the ids already bound to other sessions, a
write of the corrected identity through the same passive path hook observations
use, and a log sink. They travel together so the spawn site wires them once.
*/
#[derive(Clone)]
pub struct SessionChatSuccessorHooks {
    /// Agent session ids bound to other sessions that could actually be tailing
    /// them (running / sleeping / provider-alive — NEVER stopped history rows).
    /// Read only when a successor search actually runs.
    pub bound_agent_session_ids: Arc<dyn Fn() -> Vec<String> + Send + Sync>,
    /// Persists the corrected identity; returns true when the registry changed.
    pub adopt_identity: Arc<dyn Fn(SessionChatIdentityAdoption) -> bool + Send + Sync>,
    pub log: Arc<dyn Fn(SessionChatSuccessorNotice) + Send + Sync>,
}

#[derive(Clone)]
pub struct SessionChatFollowerConfig {
    pub project_id: String,
    pub session_id: String,
    /// Raw agent id (`claude`, `openclaude`, `codex`, `grok`, …).
    pub agent: Option<String>,
    pub agent_session_id: Option<String>,
    pub agent_session_path: Option<String>,
    pub limit: usize,
    pub protocol_version: u64,
    pub server_id: String,
    pub state_reader: Option<SessionChatStateReader>,
    /// Detected model/effort source (see session_chat_options.rs). Snapshot and
    /// replaced frames carry the cached value; a periodic probe re-detects and
    /// emits a state frame only when it CHANGED.
    pub options_reader: Option<crate::session_chat_options::SessionChatOptionsReader>,
    /// Registry access for successor-transcript adoption (claude only). Absent
    /// ⇒ the follower still re-resolves by the stored identity but never
    /// re-binds the session.
    pub successor_hooks: Option<SessionChatSuccessorHooks>,
    /// Timers the reconcile loop runs on. Production uses `Default`; tests
    /// shrink them so the whole identity-repair path runs in milliseconds.
    pub tuning: SessionChatFollowerTuning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionChatFollowerTuning {
    pub reconcile_interval: Duration,
    /// Drain silence before the identity is re-derived.
    pub stale_transcript_idle: Duration,
    /// Minimum spacing between successor directory scans.
    pub successor_scan_interval: Duration,
    /// Age of the tailed transcript's newest `user`/`assistant` record before a
    /// successor may be adopted.
    pub successor_stale_substantive_idle_ms: i64,
}

impl Default for SessionChatFollowerTuning {
    fn default() -> Self {
        Self {
            reconcile_interval: RECONCILIATION_INTERVAL,
            stale_transcript_idle: STALE_TRANSCRIPT_IDLE,
            successor_scan_interval: SUCCESSOR_SCAN_INTERVAL,
            successor_stale_substantive_idle_ms: SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS,
        }
    }
}

pub type SessionChatFrameEmitter = Arc<dyn Fn(Value) + Send + Sync>;

struct FollowerFileState {
    incremental: SessionChatIncrementalState,
    watched_version: Option<TranscriptFileVersion>,
    watched_boundary: String,
}

impl FollowerFileState {
    fn new() -> Self {
        Self {
            incremental: SessionChatIncrementalState::new(),
            watched_version: None,
            watched_boundary: String::new(),
        }
    }
}

enum FollowerDrainOutcome {
    /// stat/read failed — the path is gone; return to resolve-poll.
    Missing,
    Idle,
    Snapshot {
        tail: SessionChatTailFileResult,
        appended: Vec<SessionChatMessage>,
        appended_lifecycle: Option<SessionChatTurnLifecycle>,
        content_replaced: bool,
    },
    Appended {
        batches: Vec<Vec<SessionChatMessage>>,
        lifecycle: Option<SessionChatTurnLifecycle>,
        /// Prompts published by an earlier drain that this one proved
        /// abandoned (see `superseded_prompt_id`).
        superseded: Vec<String>,
    },
}

fn follower_drain_once(
    file_path: &Path,
    limit: usize,
    agent: SessionChatTranscriptAgent,
    decode: SessionChatLineDecoder,
    decode_lifecycle: Option<SessionChatLifecycleDecoder>,
    state: &mut FollowerFileState,
    want_snapshot: bool,
) -> FollowerDrainOutcome {
    let lineage = session_chat_lineage_extractor(agent);
    let Ok(current) = read_transcript_file_version(file_path) else {
        return FollowerDrainOutcome::Missing;
    };
    let current_boundary =
        boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
    let identity_changed = state
        .watched_version
        .as_ref()
        .is_some_and(|watched| watched.identity != current.identity);
    let same_size_version_changed = state.watched_version.as_ref().is_some_and(|watched| {
        watched.identity == current.identity && watched.size == current.size && *watched != current
    });
    let content_replaced = identity_changed
        || same_size_version_changed
        || current.size < state.incremental.offset
        || (state.incremental.offset > 0 && state.watched_boundary != current_boundary);
    if agent == SessionChatTranscriptAgent::Pi {
        if !want_snapshot && !content_replaced && current.size == state.incremental.offset {
            state.watched_version = Some(current);
            return FollowerDrainOutcome::Idle;
        }
        let Ok(tail) = read_pi_session_chat_transcript_tail_file(file_path, limit, false, None)
        else {
            return FollowerDrainOutcome::Missing;
        };
        state.incremental.rebase(tail.consumed_to);
        state.watched_boundary =
            boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
        state.watched_version = read_transcript_file_version(file_path)
            .ok()
            .or(Some(current));
        return FollowerDrainOutcome::Snapshot {
            tail,
            appended: Vec::new(),
            appended_lifecycle: None,
            content_replaced,
        };
    }
    if content_replaced {
        state.incremental.reset();
    }

    let outcome = if want_snapshot || content_replaced {
        match read_session_chat_transcript_tail_file(
            file_path,
            limit,
            decode,
            false,
            None,
            decode_lifecycle,
            lineage,
        ) {
            Err(_) => return FollowerDrainOutcome::Missing,
            Ok(tail) => {
                state.incremental.rebase(tail.consumed_to);
                // Pick up anything written after consumed_to before we settle.
                let mut appended_lifecycle: Option<SessionChatTurnLifecycle> = None;
                let mut capture_lifecycle =
                    |next: SessionChatTurnLifecycle| appended_lifecycle = Some(next);
                let capture_lifecycle: &mut dyn FnMut(SessionChatTurnLifecycle) =
                    &mut capture_lifecycle;
                let mut appended = read_incremental_transcript_messages(
                    file_path,
                    &mut state.incremental,
                    decode,
                    None,
                    decode_lifecycle,
                    Some(capture_lifecycle),
                    lineage,
                )
                .unwrap_or_default();
                // The window itself is already filtered, so anything the
                // trailing read retracts can only be inside `appended`.
                let superseded = state.incremental.take_superseded_prompt_ids();
                if !superseded.is_empty() {
                    appended.retain(|message| !superseded.contains(&message.id));
                }
                FollowerDrainOutcome::Snapshot {
                    tail,
                    appended,
                    appended_lifecycle,
                    content_replaced,
                }
            }
        }
    } else if current.size != state.incremental.offset {
        let mut batches: Vec<Vec<SessionChatMessage>> = Vec::new();
        let mut lifecycle: Option<SessionChatTurnLifecycle> = None;
        let mut push_batch = |batch: Vec<SessionChatMessage>| batches.push(batch);
        let push_batch: &mut dyn FnMut(Vec<SessionChatMessage>) = &mut push_batch;
        let mut capture_lifecycle = |next: SessionChatTurnLifecycle| lifecycle = Some(next);
        let capture_lifecycle: &mut dyn FnMut(SessionChatTurnLifecycle) = &mut capture_lifecycle;
        match read_incremental_transcript_messages(
            file_path,
            &mut state.incremental,
            decode,
            Some(push_batch),
            decode_lifecycle,
            Some(capture_lifecycle),
            lineage,
        ) {
            Err(_) => return FollowerDrainOutcome::Missing,
            Ok(remaining) => {
                if !remaining.is_empty() {
                    batches.push(remaining);
                }
                // A prompt abandoned inside this same drain never reaches a
                // client, so it is dropped from the batch instead of being
                // published and retracted in the same breath.
                let mut superseded = state.incremental.take_superseded_prompt_ids();
                if !superseded.is_empty() {
                    let abandoned: HashSet<String> = superseded.iter().cloned().collect();
                    let mut removed_before_publishing: HashSet<String> = HashSet::new();
                    for batch in batches.iter_mut() {
                        batch.retain(|message| {
                            if abandoned.contains(&message.id) {
                                removed_before_publishing.insert(message.id.clone());
                                return false;
                            }
                            true
                        });
                    }
                    batches.retain(|batch| !batch.is_empty());
                    // Only ids an EARLIER drain already published have to be
                    // retracted; the rest never reached a client.
                    superseded.retain(|id| !removed_before_publishing.contains(id));
                }
                if batches.is_empty() && lifecycle.is_none() && superseded.is_empty() {
                    FollowerDrainOutcome::Idle
                } else {
                    FollowerDrainOutcome::Appended {
                        batches,
                        lifecycle,
                        superseded,
                    }
                }
            }
        }
    } else {
        FollowerDrainOutcome::Idle
    };

    state.watched_boundary =
        boundary_fingerprint(file_path, state.incremental.offset).unwrap_or_default();
    match read_transcript_file_version(file_path) {
        // A write raced the drain: keep the start version so the next 1s
        // reconcile observes the difference and drains again.
        Ok(completed) if completed == current => state.watched_version = Some(completed),
        _ => state.watched_version = Some(current),
    }
    outcome
}

fn session_chat_frame(
    config: &SessionChatFollowerConfig,
    frame_type: &str,
    epoch: i64,
    seq: i64,
) -> Map<String, Value> {
    let mut frame = Map::new();
    frame.insert("type".to_string(), json!(frame_type));
    frame.insert("projectId".to_string(), json!(config.project_id));
    frame.insert("sessionId".to_string(), json!(config.session_id));
    frame.insert("epoch".to_string(), json!(epoch));
    frame.insert("seq".to_string(), json!(seq));
    frame.insert(
        "protocolVersion".to_string(),
        json!(config.protocol_version),
    );
    frame.insert("serverId".to_string(), json!(config.server_id));
    frame
}

fn insert_optional_lifecycle(
    frame: &mut Map<String, Value>,
    lifecycle: Option<&SessionChatTurnLifecycle>,
) {
    if let Some(lifecycle) = lifecycle {
        if let Ok(value) = serde_json::to_value(lifecycle) {
            frame.insert("lifecycle".to_string(), value);
        }
    }
}

fn insert_optional_agent_session_id(
    frame: &mut Map<String, Value>,
    config: &SessionChatFollowerConfig,
) {
    if let Some(agent_session_id) = config
        .agent_session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        frame.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
}

fn insert_optional_prompt(
    frame: &mut Map<String, Value>,
    prompt: Option<&SessionChatInteractivePrompt>,
) {
    if let Some(prompt) = prompt {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
}

/// Detected model/effort. Absent ⇒ the field is omitted ⇒ clients keep their
/// own truth (older daemons behave the same way).
fn insert_optional_selected_options(
    frame: &mut Map<String, Value>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) {
    if let Some(selected_options) = selected_options {
        frame.insert("selectedOptions".to_string(), selected_options.to_value());
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_state_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    status: SessionChatStatus,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: Option<bool>,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) {
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatState", epoch, seq);
            frame.insert("status".to_string(), json!(status.as_str()));
            insert_optional_prompt(&mut frame, prompt);
            if let Some(working) = working {
                frame.insert("working".to_string(), json!(working));
            }
            insert_optional_selected_options(&mut frame, selected_options);
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

#[allow(clippy::too_many_arguments)]
fn emit_snapshot_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    frame_type: &str,
    tail: &SessionChatTailFileResult,
    prompt: Option<&SessionChatInteractivePrompt>,
    working: bool,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) {
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, frame_type, epoch, seq);
            frame.insert(
                "messages".to_string(),
                serde_json::to_value(&tail.messages).unwrap_or(Value::Array(Vec::new())),
            );
            insert_optional_lifecycle(&mut frame, tail.lifecycle.as_ref());
            frame.insert("hasMore".to_string(), json!(tail.has_more));
            frame.insert("beforeOffset".to_string(), json!(tail.before_offset));
            let status = if tail.messages.is_empty() {
                SessionChatStatus::Empty
            } else {
                SessionChatStatus::Ready
            };
            frame.insert("status".to_string(), json!(status.as_str()));
            frame.insert("working".to_string(), json!(working));
            insert_optional_prompt(&mut frame, prompt);
            insert_optional_selected_options(&mut frame, selected_options);
            insert_optional_agent_session_id(&mut frame, config);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

fn emit_appended_frame(
    emit: &SessionChatFrameEmitter,
    config: &SessionChatFollowerConfig,
    stream: &SessionChatStream,
    epoch: i64,
    messages: &[SessionChatMessage],
    lifecycle: Option<&SessionChatTurnLifecycle>,
    superseded_message_ids: &[String],
) {
    stream.emit_sequenced(
        |seq| {
            let mut frame = session_chat_frame(config, "sessionChatAppended", epoch, seq);
            frame.insert(
                "messages".to_string(),
                serde_json::to_value(messages).unwrap_or(Value::Array(Vec::new())),
            );
            // Omitted when empty: daemons and clients that predate the field
            // then behave exactly as before.
            if !superseded_message_ids.is_empty() {
                frame.insert(
                    "supersededMessageIds".to_string(),
                    json!(superseded_message_ids),
                );
            }
            insert_optional_lifecycle(&mut frame, lifecycle);
            Value::Object(frame)
        },
        |frame| emit(frame),
    );
}

/// How often a follower may pay for a successor directory scan while the
/// transcript it tails stays substantively stale.
const SUCCESSOR_SCAN_INTERVAL: Duration = Duration::from_millis(30_000);

fn now_epoch_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/*
CDXC:SessionChatIdentity 2026-08-02:
Runs when the tailed transcript has had no `user`/`assistant` record for
SUCCESSOR_STALE_SUBSTANTIVE_IDLE_MS and re-resolving the stored identity landed
back on that same file. Adoption is persisted through the registry FIRST: if the
write is refused the follower keeps tailing what it has, so the chat can never
show a conversation the rest of the daemon does not agree with (and the next
staleness check cannot flap back and forth between two files).
*/
async fn detect_and_adopt_successor_transcript(
    transcript_agent: SessionChatTranscriptAgent,
    config: &SessionChatFollowerConfig,
    stale_path: &Path,
    stored_agent_session_id: Option<&str>,
    logged_notice: &mut Option<String>,
) -> Option<SessionChatIdentityAdoption> {
    if transcript_agent != SessionChatTranscriptAgent::Claude {
        return None;
    }
    let hooks = config.successor_hooks.clone()?;
    let stale_session_id = stale_path.file_stem()?.to_str()?.to_string();
    if !is_uuid_transcript_stem(&stale_session_id) {
        return None;
    }
    let now_ms = now_epoch_ms();
    let stale_substantive_idle_ms = config.tuning.successor_stale_substantive_idle_ms;
    let scan_path = stale_path.to_path_buf();
    let scan_stale_session_id = stale_session_id.clone();
    let bound_agent_session_ids = hooks.bound_agent_session_ids.clone();
    let outcome = tokio::task::spawn_blocking(move || {
        let last_substantive_ms = last_substantive_transcript_timestamp_ms(&scan_path)?;
        if now_ms.saturating_sub(last_substantive_ms) < stale_substantive_idle_ms {
            return None;
        }
        let owned = bound_agent_session_ids();
        Some(find_claude_successor_transcript(
            &scan_stale_session_id,
            &scan_path,
            last_substantive_ms,
            &owned,
        ))
    })
    .await
    .ok()
    .flatten()?;

    // Repeat scans of an unchanged directory must not spam the log.
    let mut log_once = |key: String, notice: SessionChatSuccessorNotice| {
        if logged_notice.as_deref() != Some(key.as_str()) {
            *logged_notice = Some(key);
            (hooks.log)(notice);
        }
    };

    match outcome {
        SessionChatSuccessorOutcome::NotFound => None,
        SessionChatSuccessorOutcome::Ambiguous {
            predecessor_session_id,
            candidate_session_ids,
        } => {
            let key = format!(
                "ambiguous|{predecessor_session_id}|{}",
                candidate_session_ids.join(",")
            );
            log_once(
                key,
                SessionChatSuccessorNotice::Ambiguous {
                    predecessor_session_id,
                    candidate_session_ids,
                },
            );
            None
        }
        SessionChatSuccessorOutcome::OwnedByAnotherSession {
            candidate_session_ids,
        } => {
            let key = format!("owned|{}", candidate_session_ids.join(","));
            log_once(
                key,
                SessionChatSuccessorNotice::OwnedByAnotherSession {
                    predecessor_session_id: stale_session_id,
                    candidate_session_ids,
                },
            );
            None
        }
        SessionChatSuccessorOutcome::Found(successor) => {
            let adoption = SessionChatIdentityAdoption {
                previous_agent_session_id: stored_agent_session_id.map(str::to_string),
                predecessor_transcript_session_id: stale_session_id.clone(),
                agent_session_id: successor.agent_session_id.clone(),
                agent_session_path: successor.path.to_string_lossy().into_owned(),
                lineage: successor.lineage.as_str(),
                hops: successor.hops,
            };
            let adopt_identity = hooks.adopt_identity.clone();
            let persisted_input = adoption.clone();
            let persisted = tokio::task::spawn_blocking(move || adopt_identity(persisted_input))
                .await
                .unwrap_or(false);
            if !persisted {
                let key = format!("rejected|{}", successor.agent_session_id);
                log_once(
                    key,
                    SessionChatSuccessorNotice::AdoptionRejected {
                        agent_session_id: successor.agent_session_id,
                        reason: "registry-identity-write-refused",
                    },
                );
                return None;
            }
            *logged_notice = None;
            (hooks.log)(SessionChatSuccessorNotice::Adopted(adoption.clone()));
            Some(adoption)
        }
    }
}

/*
Per-session follower task. Runs only while ≥1 client subscribes AND the
session is running (the server.rs registry enforces both). `resnapshot` is
signaled when another subscriber joins a live follower: every subscribe must
be answered by an authoritative snapshot, so the follower starts a fresh
generation (epoch bump, seq reset) and re-reads the tail instead of being
torn down and respawned mid-drain.
*/
pub async fn run_session_chat_follower(
    mut config: SessionChatFollowerConfig,
    stream: Arc<SessionChatStream>,
    resnapshot: Arc<tokio::sync::Notify>,
    emit: SessionChatFrameEmitter,
) {
    let read_live_state = || match config.state_reader.as_ref() {
        Some(reader) => reader(),
        None => SessionChatLiveState::default(),
    };
    // Cached detection only: frames must never pay for a process spawn.
    let read_cached_options = || {
        config.options_reader.as_ref().and_then(|reader| {
            reader(crate::session_chat_options::SessionChatOptionsReadMode::Cached)
        })
    };
    let Some(transcript_agent) = resolve_session_chat_transcript_agent(config.agent.as_deref())
    else {
        loop {
            let epoch = stream.begin_generation();
            emit_state_frame(
                &emit,
                &config,
                &stream,
                epoch,
                SessionChatStatus::Unsupported,
                None,
                None,
                None,
            );
            resnapshot.notified().await;
        }
    };
    let decode = session_chat_line_decoder(transcript_agent);
    let decode_lifecycle = session_chat_lifecycle_decoder(transcript_agent);

    let mut epoch = stream.begin_generation();
    let mut want_snapshot = true;
    let mut emitted_starting = false;
    let mut resolved: Option<PathBuf> = None;
    let mut resolve_delay = INITIAL_RESOLVE_POLL;
    let mut file_state = FollowerFileState::new();
    // Rolling AskUserQuestion state folded over everything decoded so far, plus
    // the last prompt/working pair actually published to clients.
    let mut transcript_prompt = SessionChatTranscriptPromptState::default();
    let mut published_prompt: Option<SessionChatInteractivePrompt> = None;
    let mut published_working = false;
    let mut published_state_valid = false;
    let mut identity = SessionChatFollowerIdentity {
        agent_session_id: config.agent_session_id.clone(),
        agent_session_path: config.agent_session_path.clone(),
    };
    let mut last_transcript_change = std::time::Instant::now();
    let mut last_staleness_check = std::time::Instant::now();
    let mut last_successor_scan = std::time::Instant::now();
    // "Adopt none and log once" for an ambiguous successor set.
    let mut logged_successor_ambiguity: Option<String> = None;
    // Model/effort the follower has published, plus counters that pace the
    // fast startup probes and periodic steady-state re-detects.
    let mut published_options: Option<crate::session_chat_options::SessionChatDetectedOptions> =
        None;
    let mut reconcile_ticks: u64 = 0;
    let mut startup_option_reconcile_ticks: u64 = 0;

    loop {
        if resolved.is_none() {
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if resolved.is_none() {
                if !emitted_starting {
                    let live = read_live_state();
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        SessionChatStatus::Starting,
                        live.prompt.as_ref(),
                        Some(live.working),
                        read_cached_options().as_ref(),
                    );
                    emitted_starting = true;
                }
                tokio::select! {
                    _ = tokio::time::sleep(resolve_delay) => {}
                    _ = resnapshot.notified() => {
                        epoch = stream.begin_generation();
                        emitted_starting = false;
                        want_snapshot = true;
                    }
                }
                resolve_delay = (resolve_delay * 2).min(MAX_RESOLVE_POLL);
                // A stale hook identity is the usual reason the path never
                // appears: re-read the session's current identity each poll.
                identity.adopt(read_live_state());
                continue;
            }
            want_snapshot = true;
            file_state = FollowerFileState::new();
            transcript_prompt = SessionChatTranscriptPromptState::default();
            last_transcript_change = std::time::Instant::now();
        }

        let path = resolved.clone().expect("resolved transcript path");
        let drain_limit = config.limit;
        let drain_want_snapshot = want_snapshot;
        let mut drain_state = std::mem::replace(&mut file_state, FollowerFileState::new());
        let Ok((returned_state, outcome)) = tokio::task::spawn_blocking(move || {
            let outcome = follower_drain_once(
                &path,
                drain_limit,
                transcript_agent,
                decode,
                decode_lifecycle,
                &mut drain_state,
                drain_want_snapshot,
            );
            (drain_state, outcome)
        })
        .await
        else {
            return;
        };
        file_state = returned_state;
        // One live-state read per reconcile: it opens the domain database.
        let live = read_live_state();

        match outcome {
            FollowerDrainOutcome::Missing => {
                // Rotation to a missing path — resolve-poll again and deliver
                // an authoritative frame once the successor file appears.
                resolved = None;
                resolve_delay = INITIAL_RESOLVE_POLL;
                epoch = stream.begin_generation();
                emitted_starting = false;
                want_snapshot = true;
                continue;
            }
            FollowerDrainOutcome::Snapshot {
                tail,
                appended,
                appended_lifecycle,
                content_replaced,
            } => {
                let frame_type = if want_snapshot {
                    "sessionChatSnapshot"
                } else {
                    if content_replaced {
                        epoch = stream.begin_generation();
                    }
                    "sessionChatReplaced"
                };
                // The tail window replaces everything the client had, so the
                // question fold restarts from it.
                transcript_prompt = SessionChatTranscriptPromptState::default();
                transcript_prompt.advance(&tail.messages);
                transcript_prompt.advance(&appended);
                let prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt);
                // A subscribing client gets the detected pills value with its
                // snapshot, so it needs no separate read.
                let snapshot_options = read_cached_options();
                emit_snapshot_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    frame_type,
                    &tail,
                    prompt.as_ref(),
                    live.working,
                    snapshot_options.as_ref(),
                );
                if snapshot_options.is_some() {
                    published_options = snapshot_options;
                }
                published_prompt = prompt;
                published_working = live.working;
                published_state_valid = true;
                want_snapshot = false;
                last_transcript_change = std::time::Instant::now();
                if !appended.is_empty() || appended_lifecycle.is_some() {
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &appended,
                        appended_lifecycle.as_ref(),
                        &[],
                    );
                }
            }
            FollowerDrainOutcome::Appended {
                batches,
                lifecycle,
                superseded,
            } => {
                last_transcript_change = std::time::Instant::now();
                if batches.is_empty() {
                    // Lifecycle-only and retraction-only frames ARE emitted.
                    emit_appended_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        &[],
                        lifecycle.as_ref(),
                        &superseded,
                    );
                } else {
                    let last_index = batches.len() - 1;
                    for (index, batch) in batches.iter().enumerate() {
                        transcript_prompt.advance(batch);
                        let batch_lifecycle = if index == last_index {
                            lifecycle.as_ref()
                        } else {
                            None
                        };
                        // The retraction rides the FIRST frame so a client can
                        // never re-order it after the rows that replace it.
                        let batch_superseded: &[String] =
                            if index == 0 { &superseded } else { &[] };
                        emit_appended_frame(
                            &emit,
                            &config,
                            &stream,
                            epoch,
                            batch,
                            batch_lifecycle,
                            batch_superseded,
                        );
                    }
                }
            }
            FollowerDrainOutcome::Idle => {}
        }

        /*
        CDXC:SessionChatCore 2026-08-01:
        Interactive cards used to depend entirely on agent hooks. When the
        installed hook script does not forward toolName/toolInput the card never
        appeared, and when it never reports PostToolUse a card answered in the
        terminal stayed on screen forever. The transcript itself answers both:
        a trailing AskUserQuestion tool call with no tool result means "pending",
        a tool result after it means "answered". The hook prompt still wins when
        both exist, so approvals and richer hook payloads are unaffected.
        */
        let effective_prompt = resolve_session_chat_prompt(live.prompt.clone(), &transcript_prompt);
        if !published_state_valid
            || effective_prompt != published_prompt
            || live.working != published_working
        {
            if published_state_valid {
                emit_state_frame(
                    &emit,
                    &config,
                    &stream,
                    epoch,
                    if live.working {
                        SessionChatStatus::Working
                    } else {
                        SessionChatStatus::Ready
                    },
                    effective_prompt.as_ref(),
                    Some(live.working),
                    published_options.as_ref(),
                );
            }
            published_prompt = effective_prompt;
            published_working = live.working;
            published_state_valid = true;
        }

        /*
        CDXC:SessionChatDetectedOptions 2026-08-01:
        Model/effort probe: a newly launched agent can paint its footer just
        after the seed read cached an empty detection. Probe each 1s reconcile
        for up to ten seconds until both values arrive, then retain the ~30s
        steady-state cadence that catches direct TUI changes. The follower only
        exists while subscribed, and a frame is emitted only when the detected
        value actually changed.
        */
        reconcile_ticks = reconcile_ticks.wrapping_add(1);
        let startup_probe_due = published_state_valid
            && config.options_reader.is_some()
            && reconcile_ticks > 1
            && startup_option_reconcile_ticks
                < crate::session_chat_options::SESSION_CHAT_OPTION_STARTUP_RECONCILE_TICKS
            && published_options.as_ref().map_or(true, |options| {
                options.selection.model.is_none() || options.selection.effort.is_none()
            });
        if startup_probe_due {
            startup_option_reconcile_ticks += 1;
        }
        let periodic_probe_due = reconcile_ticks
            % crate::session_chat_options::SESSION_CHAT_OPTION_RECONCILE_INTERVAL_TICKS
            == 0;
        if published_state_valid
            && config.options_reader.is_some()
            && (startup_probe_due || periodic_probe_due)
        {
            let reader = config.options_reader.clone();
            let detected = tokio::task::spawn_blocking(move || {
                reader.and_then(|reader| {
                    reader(crate::session_chat_options::SessionChatOptionsReadMode::Refresh)
                })
            })
            .await
            .ok()
            .flatten();
            if let Some(detected) = detected {
                if !detected.same_selection(published_options.as_ref()) {
                    emit_state_frame(
                        &emit,
                        &config,
                        &stream,
                        epoch,
                        if published_working {
                            SessionChatStatus::Working
                        } else {
                            SessionChatStatus::Ready
                        },
                        published_prompt.as_ref(),
                        Some(published_working),
                        Some(&detected),
                    );
                    published_options = Some(detected);
                }
            }
        }

        /*
        CDXC:SessionChatCore 2026-08-01:
        Stale-identity guard. `/clear` and `resume` make the agent start a NEW
        transcript file while the old one stays on disk, so the follower keeps
        tailing a file that will never grow again and the chat freezes at the
        switch point — with no Missing outcome to recover from. When the tailed
        file has been silent, re-derive the path from the session's CURRENT
        identity; a different file is treated exactly like a content
        replacement.

        CDXC:SessionChatIdentity 2026-08-02:
        The hook-driven re-resolution above only runs while hooks report the
        session as `working`. The case successor detection must recover from is
        precisely the one where hooks never fire at all (a background-job
        continuation writes a NEW transcript and nothing updates the registry),
        so `working` cannot gate it. It gets its own, slower cadence instead:
        every SUCCESSOR_SCAN_INTERVAL while the tailed file stays silent.
        */
        let successor_scan_due = transcript_agent == SessionChatTranscriptAgent::Claude
            && config.successor_hooks.is_some()
            && last_successor_scan.elapsed() >= config.tuning.successor_scan_interval;
        if last_transcript_change.elapsed() >= config.tuning.stale_transcript_idle
            && ((published_working
                && last_staleness_check.elapsed() >= config.tuning.stale_transcript_idle)
                || successor_scan_due)
        {
            last_staleness_check = std::time::Instant::now();
            identity.adopt(live);
            let agent_session_id = identity.agent_session_id.clone();
            let agent_session_path = identity.agent_session_path.clone();
            let re_resolved = tokio::task::spawn_blocking(move || {
                resolve_session_chat_transcript_path(
                    transcript_agent,
                    agent_session_id.as_deref(),
                    agent_session_path.as_deref(),
                )
            })
            .await
            .ok()
            .flatten();
            if let Some(next_path) = re_resolved {
                if Some(&next_path) != resolved.as_ref() {
                    resolved = Some(next_path);
                    file_state = FollowerFileState::new();
                    transcript_prompt = SessionChatTranscriptPromptState::default();
                    epoch = stream.begin_generation();
                    want_snapshot = true;
                    published_state_valid = false;
                    last_transcript_change = std::time::Instant::now();
                    continue;
                }
                /*
                CDXC:SessionChatIdentity 2026-08-02:
                Re-resolution landed on the SAME file, so the registry identity
                itself is stale. Look for a transcript that proves it continues
                this one and re-bind the session to it.
                */
                if successor_scan_due {
                    last_successor_scan = std::time::Instant::now();
                    if let Some(adopted) = detect_and_adopt_successor_transcript(
                        transcript_agent,
                        &config,
                        &next_path,
                        identity.agent_session_id.as_deref(),
                        &mut logged_successor_ambiguity,
                    )
                    .await
                    {
                        identity.agent_session_id = Some(adopted.agent_session_id.clone());
                        identity.agent_session_path = Some(adopted.agent_session_path.clone());
                        config.agent_session_id = Some(adopted.agent_session_id);
                        config.agent_session_path = Some(adopted.agent_session_path.clone());
                        resolved = Some(PathBuf::from(&adopted.agent_session_path));
                        file_state = FollowerFileState::new();
                        transcript_prompt = SessionChatTranscriptPromptState::default();
                        epoch = stream.begin_generation();
                        want_snapshot = true;
                        published_state_valid = false;
                        last_transcript_change = std::time::Instant::now();
                        continue;
                    }
                }
            }
        }

        tokio::select! {
            _ = tokio::time::sleep(config.tuning.reconcile_interval) => {}
            _ = resnapshot.notified() => {
                epoch = stream.begin_generation();
                want_snapshot = true;
            }
        }
    }
}

/// Identity the follower is currently tailing. Seeded from the spawn config and
/// refreshed from the live session so a hook update that did not respawn the
/// task still reaches the resolver.
struct SessionChatFollowerIdentity {
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
}

impl SessionChatFollowerIdentity {
    fn adopt(&mut self, live: SessionChatLiveState) {
        if live.agent_session_id.is_some() {
            self.agent_session_id = live.agent_session_id;
        }
        if live.agent_session_path.is_some() {
            self.agent_session_path = live.agent_session_path;
        }
    }
}

// ---------------------------------------------------------------------------
// Interactive prompts (upstream chat spec §8.1-§8.3): question/approval cards
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatQuestionOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionChatQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(rename = "multiSelect")]
    pub multi_select: bool,
    pub options: Vec<SessionChatQuestionOption>,
}

/// Rust mirror of shared/session-chat.ts `SessionChatInteractivePrompt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SessionChatInteractivePrompt {
    Question {
        questions: Vec<SessionChatQuestion>,
    },
    Approval {
        tool: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
}

/// Rust mirror of shared/session-chat.ts `SessionChatQuestionSelection`.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct SessionChatQuestionSelection {
    #[serde(default)]
    pub indices: Vec<usize>,
    #[serde(default)]
    pub other: Option<String>,
}

const APPROVAL_SUMMARY_MAX_CHARS: usize = 200;

/// Upstream `normalizeHookEventName`: camelCase → snake_case, dashes/spaces →
/// underscores, lowercased.
fn normalize_hook_event_name(value: &str) -> String {
    let mut snake = String::with_capacity(value.len() + 4);
    let mut previous_lower_or_digit = false;
    for ch in value.trim().chars() {
        if ch.is_ascii_uppercase() && previous_lower_or_digit {
            snake.push('_');
        }
        previous_lower_or_digit = ch.is_ascii_lowercase() || ch.is_ascii_digit();
        snake.push(ch);
    }
    let mut normalized = String::with_capacity(snake.len());
    let mut previous_was_separator = false;
    for ch in snake.chars() {
        if ch == '-' || ch.is_whitespace() {
            if !previous_was_separator {
                normalized.push('_');
            }
            previous_was_separator = true;
        } else {
            normalized.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        }
    }
    normalized
}

pub fn is_post_tool_hook_event(event_name: Option<&str>) -> bool {
    matches!(
        normalize_hook_event_name(event_name.unwrap_or_default()).as_str(),
        "post_tool_use" | "post_tool_use_failure"
    )
}

/// Upstream `isAskUserQuestionTool`: strip non-alphanumerics, lowercase, and match
/// AskUserQuestion (Claude) / request_user_input (Codex 0.145) spellings.
pub fn is_ask_user_question_tool(tool_name: &str) -> bool {
    let normalized: String = tool_name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    normalized == "askuserquestion" || normalized == "requestuserinput"
}

fn truncate_approval_summary(value: &str) -> String {
    if value.chars().count() > APPROVAL_SUMMARY_MAX_CHARS {
        let mut truncated: String = value.chars().take(APPROVAL_SUMMARY_MAX_CHARS).collect();
        truncated.push('\u{2026}');
        truncated
    } else {
        value.to_string()
    }
}

/// Upstream `summarizeApprovalInput`: prefer the first present command/file_path/
/// path/url/pattern field when it is a non-empty string, else the JSON body;
/// both capped at 200 chars.
pub fn summarize_approval_input(tool_input: Option<&Value>) -> String {
    if let Some(object) = tool_input.and_then(Value::as_object) {
        let direct = ["command", "file_path", "path", "url", "pattern"]
            .iter()
            .find_map(|key| object.get(*key).filter(|value| !value.is_null()));
        if let Some(direct) = direct
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            return truncate_approval_summary(direct);
        }
    }
    let json = tool_input.map(Value::to_string).unwrap_or_default();
    truncate_approval_summary(&json)
}

/// Upstream `parseQuestionsShape`: the canonical AskUserQuestion tool-input shape.
pub fn parse_session_chat_questions(input: &Value) -> Option<Vec<SessionChatQuestion>> {
    let raw_questions = input.as_object()?.get("questions")?.as_array()?;
    if raw_questions.is_empty() {
        return None;
    }
    let mut questions = Vec::new();
    for raw in raw_questions {
        let Some(record) = raw.as_object() else {
            continue;
        };
        let text = record
            .get("question")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let options = parse_session_chat_question_options(record.get("options"));
        if !text.is_empty() || !options.is_empty() {
            questions.push(SessionChatQuestion {
                question: text,
                header: record
                    .get("header")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                // The spec uses strict === true; anything else is single-select.
                multi_select: record.get("multiSelect").and_then(Value::as_bool) == Some(true),
                options,
            });
        }
    }
    (!questions.is_empty()).then_some(questions)
}

fn parse_session_chat_question_options(raw: Option<&Value>) -> Vec<SessionChatQuestionOption> {
    let Some(array) = raw.and_then(Value::as_array) else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|option| match option {
            Value::String(label) => Some(SessionChatQuestionOption {
                label: label.clone(),
                description: None,
            }),
            Value::Object(record) => Some(SessionChatQuestionOption {
                label: record.get("label").and_then(Value::as_str)?.to_string(),
                description: record
                    .get("description")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            _ => None,
        })
        .collect()
}

/*
CDXC:SessionChatSend 2026-07-31:
Hook-side prompt derivation (upstream `deriveInteractivePrompt`): an
AskUserQuestion-ish tool with input on a NON-post-tool event becomes a
question card; a `PermissionRequest` event with a tool name becomes an
approval card. Everything else derives nothing. The derived wire shape (not
the raw tool input) is what gxserver stores under
runtimeSettings.agentActivity.sessionChatPrompt, so read/stream paths never
re-parse tool payloads.
*/
pub fn derive_session_chat_prompt(
    tool_name: Option<&str>,
    tool_input: Option<&Value>,
    event_name: Option<&str>,
) -> Option<SessionChatInteractivePrompt> {
    let tool_name = tool_name.map(str::trim).filter(|value| !value.is_empty())?;
    if is_ask_user_question_tool(tool_name)
        && !is_post_tool_hook_event(event_name)
        && tool_input.is_some_and(|value| !value.is_null())
    {
        let questions = parse_session_chat_questions(tool_input?)?;
        return Some(SessionChatInteractivePrompt::Question { questions });
    }
    if event_name == Some("PermissionRequest") {
        let summary = summarize_approval_input(tool_input);
        return Some(SessionChatInteractivePrompt::Approval {
            tool: tool_name.to_string(),
            summary: (!summary.is_empty()).then_some(summary),
        });
    }
    None
}

/// Post-tool events and Stop/SessionEnd/idle transitions clear a pending
/// prompt; other events leave it alone (the contract's clear rule — narrower
/// than the upstream overwrite-on-every-event rule, so unrelated working events cannot
/// drop a still-pending card).
pub fn should_clear_session_chat_prompt(
    event_name: Option<&str>,
    next_activity: Option<&str>,
) -> bool {
    if is_post_tool_hook_event(event_name) {
        return true;
    }
    if next_activity == Some("idle") {
        return true;
    }
    matches!(
        normalize_hook_event_name(event_name.unwrap_or_default()).as_str(),
        "stop" | "session_end" | "idle"
    )
}

pub fn parse_stored_session_chat_prompt(stored: &str) -> Option<SessionChatInteractivePrompt> {
    serde_json::from_str::<SessionChatInteractivePrompt>(stored).ok()
}

/*
CDXC:SessionChatCore 2026-08-01:
Transcript-derived question detection. Hook delivery of `toolName`/`toolInput`
is optional (older installed hook scripts do not forward it) and PostToolUse is
not guaranteed, so the transcript is the second, independent source of truth for
the question card: an AskUserQuestion/request_user_input tool call with no tool
result after it is still waiting for an answer, and a tool result after it means
the user already answered — in the chat card or straight in the terminal.

The fold is incremental so the follower can advance it over appended batches
without buffering messages, and restarts from a snapshot's tail window.
*/
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionChatTranscriptPromptState {
    pending: Option<SessionChatInteractivePrompt>,
    last_question: Option<SessionChatInteractivePrompt>,
    answered: bool,
}

impl SessionChatTranscriptPromptState {
    pub fn advance(&mut self, messages: &[SessionChatMessage]) {
        for message in messages {
            for block in &message.blocks {
                match block {
                    SessionChatBlock::ToolCall { name, input }
                        if is_ask_user_question_tool(name) =>
                    {
                        self.answered = false;
                        self.pending = parse_session_chat_questions(input)
                            .map(|questions| SessionChatInteractivePrompt::Question { questions });
                        self.last_question = self.pending.clone();
                    }
                    SessionChatBlock::ToolResult { .. } => {
                        if self.last_question.is_some() {
                            self.answered = true;
                        }
                        self.pending = None;
                    }
                    _ => {}
                }
            }
        }
    }

    pub fn pending(&self) -> Option<&SessionChatInteractivePrompt> {
        self.pending.as_ref()
    }

    /// True once the most recent AskUserQuestion call has its tool result.
    pub fn answered(&self) -> bool {
        self.last_question.is_some() && self.answered
    }

    /// The most recent question the transcript proves was answered. `None`
    /// while the latest question is still pending (or was unparsable).
    pub fn answered_question(&self) -> Option<&SessionChatInteractivePrompt> {
        if self.answered {
            self.last_question.as_ref()
        } else {
            None
        }
    }
}

pub fn scan_transcript_prompt_state(
    messages: &[SessionChatMessage],
) -> SessionChatTranscriptPromptState {
    let mut state = SessionChatTranscriptPromptState::default();
    state.advance(messages);
    state
}

/// Question texts of a question prompt, in order. `None` for approvals.
fn session_chat_prompt_question_texts(prompt: &SessionChatInteractivePrompt) -> Option<Vec<&str>> {
    match prompt {
        SessionChatInteractivePrompt::Question { questions } => Some(
            questions
                .iter()
                .map(|question| question.question.as_str())
                .collect(),
        ),
        SessionChatInteractivePrompt::Approval { .. } => None,
    }
}

/// Hook-derived prompts stay authoritative — they carry approvals and richer
/// payloads the transcript cannot express. The transcript only adds a card the
/// hooks never reported, or retires a question card the transcript proves was
/// answered.
///
/// Retirement is matched by question text: an answered question retires the
/// stored card only when it asks the same questions. Claude Code does not
/// flush the assistant row for a *pending* AskUserQuestion, so while a new
/// question waits, the transcript's most recent question is the previous
/// (answered) one — an unconditional `answered()` check retired every stored
/// question after the first one in the tail window, hiding the card from all
/// report sites.
pub fn resolve_session_chat_prompt(
    stored: Option<SessionChatInteractivePrompt>,
    transcript: &SessionChatTranscriptPromptState,
) -> Option<SessionChatInteractivePrompt> {
    match stored {
        Some(prompt) => {
            let retired = transcript.answered_question().is_some_and(|answered| {
                match (
                    session_chat_prompt_question_texts(answered),
                    session_chat_prompt_question_texts(&prompt),
                ) {
                    (Some(answered_texts), Some(stored_texts)) => answered_texts == stored_texts,
                    _ => false,
                }
            });
            if retired {
                None
            } else {
                Some(prompt)
            }
        }
        None => transcript.pending().cloned(),
    }
}

/// Builds a `sessionChatState` frame carrying a prompt change (hook ingest) or
/// a fresh model/effort detection (post-dispatch probe) so a producer outside
/// the follower task can push it through a live stream.
#[allow(clippy::too_many_arguments)]
pub fn build_session_chat_prompt_state_frame(
    project_id: &str,
    session_id: &str,
    epoch: i64,
    seq: i64,
    status: SessionChatStatus,
    prompt: Option<&SessionChatInteractivePrompt>,
    agent_session_id: Option<&str>,
    protocol_version: u64,
    server_id: &str,
    working: bool,
    selected_options: Option<&crate::session_chat_options::SessionChatDetectedOptions>,
) -> Value {
    let mut frame = Map::new();
    frame.insert("type".to_string(), json!("sessionChatState"));
    frame.insert("projectId".to_string(), json!(project_id));
    frame.insert("sessionId".to_string(), json!(session_id));
    frame.insert("epoch".to_string(), json!(epoch));
    frame.insert("seq".to_string(), json!(seq));
    frame.insert("protocolVersion".to_string(), json!(protocol_version));
    frame.insert("serverId".to_string(), json!(server_id));
    frame.insert("status".to_string(), json!(status.as_str()));
    frame.insert("working".to_string(), json!(working));
    if let Some(prompt) = prompt {
        if let Ok(value) = serde_json::to_value(prompt) {
            frame.insert("prompt".to_string(), value);
        }
    }
    insert_optional_selected_options(&mut frame, selected_options);
    if let Some(agent_session_id) = agent_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        frame.insert("agentSessionId".to_string(), json!(agent_session_id));
    }
    Value::Object(frame)
}

// ---------------------------------------------------------------------------
// Inline sanity tests (real transcript files are skipped when absent)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn message_serialization_matches_shared_ts_shape() {
        let message = SessionChatMessage {
            id: "m1".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![
                text_block("hello"),
                SessionChatBlock::ToolCall {
                    name: "Bash".to_string(),
                    input: json!({"command": "ls"}),
                },
                SessionChatBlock::ToolResult {
                    output: "ok".to_string(),
                    is_error: Some(true),
                },
                SessionChatBlock::ImageRef {
                    path: Some("/tmp/a.png".to_string()),
                    url: None,
                    alt: None,
                },
            ],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        };
        let serialized = serde_json::to_value(&message).expect("serialize");
        assert_eq!(
            serialized,
            json!({
                "id": "m1",
                "role": "assistant",
                "blocks": [
                    {"type": "text", "text": "hello"},
                    {"type": "tool-call", "name": "Bash", "input": {"command": "ls"}},
                    {"type": "tool-result", "output": "ok", "isError": true},
                    {"type": "image-ref", "path": "/tmp/a.png"},
                ],
                "timestamp": Value::Null,
                "source": "transcript",
            })
        );
    }

    #[test]
    fn claude_decoder_handles_roles_interrupts_and_injected_turns() {
        let user = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u1","timestamp":"2026-07-29T01:35:31.020Z","message":{"role":"user","content":"fix the bug"}}"#,
            "fb",
        )
        .expect("user decodes");
        assert_eq!(user.role, SessionChatRole::User);
        assert_eq!(user.id, "u1");
        assert!(user.timestamp.is_some(), "RFC3339 timestamp must parse");

        let tool = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"out"}]}}"#,
            "fb",
        )
        .expect("tool result decodes");
        assert_eq!(tool.role, SessionChatRole::Tool);

        let interrupted = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u3","interruptedMessageId":"m9","message":{"role":"user","content":"x"}}"#,
            "fb",
        )
        .expect("interrupt decodes");
        assert_eq!(interrupted.role, SessionChatRole::System);

        // Injected meta turn keeps only tool results; text-only meta is dropped.
        assert!(decode_claude_transcript_line(
            r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"<local-command-caveat>x</local-command-caveat>"}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_claude_transcript_line(r#"{"type":"summary"}"#, "fb").is_none());
        assert!(decode_claude_transcript_line("not json", "fb").is_none());
    }

    #[test]
    fn claude_decoder_marks_thinking_only_rows_as_reasoning() {
        let reasoning = decode_claude_transcript_line(
            r#"{"type":"assistant","uuid":"a1","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspect the state."}]}}"#,
            "fb",
        )
        .expect("thinking row decodes");
        assert_eq!(reasoning.role, SessionChatRole::Reasoning);

        let mixed = decode_claude_transcript_line(
            r#"{"type":"assistant","uuid":"a2","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Inspect the state."},{"type":"tool_use","name":"Read","input":{}}]}}"#,
            "fb",
        )
        .expect("mixed row decodes");
        assert_eq!(mixed.role, SessionChatRole::Assistant);
    }

    #[test]
    fn grok_session_ids_reject_path_syntax() {
        assert!(is_safe_grok_session_id("session_123-abc"));
        assert!(!is_safe_grok_session_id(""));
        assert!(!is_safe_grok_session_id("../session"));
        assert!(!is_safe_grok_session_id("session\\child"));
        assert!(!is_safe_grok_session_id(&"a".repeat(129)));
    }

    #[test]
    fn claude_lifecycle_tool_use_is_not_terminal() {
        assert!(decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a1","message":{"stop_reason":"tool_use","content":[{"type":"tool_use","name":"Bash","input":{}}]}}"#,
            "fb",
        )
        .is_none());
        let completed = decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a2","message":{"stop_reason":"end_turn","content":[{"type":"text","text":"done"}]}}"#,
            "fb",
        )
        .expect("terminal");
        assert_eq!(completed.state, SessionChatTurnLifecycleState::Completed);
        // No stop_reason + prose + no tool_use → completed (historical rows).
        let historical = decode_claude_turn_lifecycle(
            r#"{"type":"assistant","uuid":"a3","message":{"content":[{"type":"text","text":"done"}]}}"#,
            "fb",
        )
        .expect("historical terminal");
        assert_eq!(historical.state, SessionChatTurnLifecycleState::Completed);
        let working = decode_claude_turn_lifecycle(
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"go"}}"#,
            "fb",
        )
        .expect("working");
        assert_eq!(working.state, SessionChatTurnLifecycleState::Working);
        // Harness noise must not restart the spinner.
        assert!(decode_claude_turn_lifecycle(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":"<system-reminder>hi</system-reminder>"}}"#,
            "fb",
        )
        .is_none());
    }

    #[test]
    fn codex_decoder_and_lifecycle_cover_event_lane() {
        let user = decode_codex_transcript_line(
            r#"{"timestamp":"2026-07-30T04:33:09.500Z","type":"event_msg","payload":{"type":"user_message","message":"do it"}}"#,
            "fb",
        )
        .expect("user");
        assert_eq!(user.role, SessionChatRole::User);
        let call = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"function_call","id":"fc1","name":"exec_command","arguments":"{\"cmd\":\"ls\"}"}}"#,
            "fb",
        )
        .expect("call");
        assert_eq!(call.role, SessionChatRole::Assistant);
        assert_eq!(call.id, "fc1");
        let output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"c1","output":"Exit code: 0"}}"#,
            "fb",
        )
        .expect("output");
        assert_eq!(output.role, SessionChatRole::Tool);
        let reasoning = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"reasoning","id":"rs1","summary":[{"type":"summary_text","text":"plan"}]}}"#,
            "fb",
        )
        .expect("reasoning");
        assert_eq!(reasoning.role, SessionChatRole::Reasoning);

        let started = decode_codex_turn_lifecycle(
            r#"{"type":"event_msg","payload":{"type":"task_started","turn_id":"t1"}}"#,
            "fb",
        )
        .expect("started");
        assert_eq!(started.state, SessionChatTurnLifecycleState::Working);
        let aborted = decode_codex_turn_lifecycle(
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","turn_id":"t1"}}"#,
            "fb",
        )
        .expect("aborted");
        assert_eq!(aborted.state, SessionChatTurnLifecycleState::Interrupted);
    }

    /*
    Record shapes copied from a real 2026-08-07 gpt-5.6 rollout. These builds
    write NO `user_message`/`agent_message` events; the visible turns arrive
    only as `item_completed` items, and dropping them made chat end at the last
    reasoning summary while the terminal showed the full final answer.
    */
    #[test]
    fn codex_decodes_item_completed_message_lane() {
        let user = decode_codex_transcript_line(
            r#"{"timestamp":"2026-08-07T13:59:00.000Z","type":"event_msg","payload":{"type":"item_completed","thread_id":"th1","turn_id":"t1","item":{"type":"UserMessage","id":"019fdc6f-1436-7b10-a55f-23ba56beb5ac","content":[{"type":"skill","name":"ghostex-browser-use","path":"/Users/madda/.agents/skills/ghostex-browser-use/SKILL.md"},{"type":"text","text":"explain questions 2 and 3"}]}}}"#,
            "fb",
        )
        .expect("user item");
        assert_eq!(user.role, SessionChatRole::User);
        assert_eq!(user.id, "019fdc6f-1436-7b10-a55f-23ba56beb5ac");
        assert_eq!(
            user.blocks,
            vec![
                text_block("Skill: ghostex-browser-use".to_string()),
                text_block("explain questions 2 and 3".to_string()),
            ]
        );

        // AgentMessage spells its text block `Text`, unlike UserMessage's `text`.
        let assistant = decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","id":"msg_0a5b","content":[{"type":"Text","text":"Created the Markdown plan."}]}}}"#,
            "fb",
        )
        .expect("assistant item");
        assert_eq!(assistant.role, SessionChatRole::Assistant);
        assert_eq!(assistant.id, "msg_0a5b");
        assert_eq!(message_text(&assistant), "Created the Markdown plan.");

        // Reasoning/CommandExecution/McpToolCall items stay on the
        // response_item lane, which new-style rollouts still write.
        assert!(decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning","id":"rs1","summary_text":["**Planning**"],"raw_content":[]}}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","id":"c1"}}}"#,
            "fb",
        )
        .is_none());
    }

    /*
    Record shapes below are copied from real rollouts under the codex sessions
    directory, with the bodies trimmed. `custom_tool_call` +
    `custom_tool_call_output` are as frequent as the function-call lane in
    real files and used to decode to nothing at all.
    */
    #[test]
    fn codex_decodes_custom_tool_and_hosted_tool_lanes() {
        let call = decode_codex_transcript_line(
            r#"{"timestamp":"2026-08-01T02:04:11.100Z","type":"response_item","payload":{"type":"custom_tool_call","id":"ctc_0e7c","call_id":"call_Nx1m","name":"exec","status":"completed","input":"const r = await tools.exec_command({cmd:\"ls\"});"}}"#,
            "fb",
        )
        .expect("custom_tool_call decodes");
        assert_eq!(call.role, SessionChatRole::Assistant);
        assert_eq!(call.id, "ctc_0e7c");
        assert_eq!(
            call.blocks,
            vec![SessionChatBlock::ToolCall {
                name: "exec".to_string(),
                input: json!("const r = await tools.exec_command({cmd:\"ls\"});"),
            }]
        );

        // Output is an array of Responses-API `input_text` blocks.
        let output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","id":"ctco_1","call_id":"call_Nx1m","output":[{"type":"input_text","text":"Script completed"},{"type":"input_text","text":"total 4"}]}}"#,
            "fb",
        )
        .expect("custom_tool_call_output decodes");
        assert_eq!(output.role, SessionChatRole::Tool);
        assert_eq!(
            output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "Script completed\ntotal 4".to_string(),
                is_error: None,
            }]
        );

        // Some builds write the same payload as a plain string.
        let string_output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call_x","output":"Exit code: 0"}}"#,
            "fb",
        )
        .expect("string output decodes");
        assert_eq!(
            string_output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "Exit code: 0".to_string(),
                is_error: None,
            }]
        );

        let web_search = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"web_search_call","status":"completed"}}"#,
            "fb",
        )
        .expect("web_search_call decodes");
        assert!(matches!(
            web_search.blocks.as_slice(),
            [SessionChatBlock::ToolCall { name, .. }] if name == "web_search"
        ));

        let tool_search = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"tool_search_call","id":"tsc_1","call_id":"call_q","status":"completed","execution":"client","arguments":{"query":"GitHub issue details"}}}"#,
            "fb",
        )
        .expect("tool_search_call decodes");
        assert_eq!(
            tool_search.blocks,
            vec![SessionChatBlock::ToolCall {
                name: "tool_search".to_string(),
                input: json!({"query": "GitHub issue details"}),
            }]
        );
        let tool_search_output = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"tool_search_output","call_id":"call_q","status":"completed","tools":[{"type":"namespace","name":"mcp__codex_apps__github","description":"…"}]}}"#,
            "fb",
        )
        .expect("tool_search_output decodes");
        assert_eq!(
            tool_search_output.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "mcp__codex_apps__github".to_string(),
                is_error: None,
            }]
        );

        // Inter-agent traffic: readable header text survives, the encrypted
        // envelope is skipped rather than dropping the whole record.
        let agent_message = decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"agent_message","author":"/root/worker","recipient":"/root","content":[{"type":"input_text","text":"Message Type: MESSAGE"},{"type":"encrypted_content","encrypted_content":"gAAA"}]}}"#,
            "fb",
        )
        .expect("agent_message decodes");
        assert_eq!(agent_message.role, SessionChatRole::System);
        assert_eq!(
            agent_message.blocks,
            vec![text_block("Message Type: MESSAGE")]
        );
    }

    /*
    Every visible Codex turn is written twice (event lane + response-item lane).
    Measured across 4,881 local rollouts the two are exactly redundant, so only
    the event lane is decoded; decoding both would double every message.
    */
    #[test]
    fn codex_message_response_items_defer_to_the_event_lane() {
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m1","role":"assistant","content":[{"type":"output_text","text":"I'll do it"}]}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m2","role":"user","content":[{"type":"input_text","text":"AGENTS.md instructions"}]}}"#,
            "fb",
        )
        .is_none());
        assert!(decode_codex_transcript_line(
            r#"{"type":"response_item","payload":{"type":"message","id":"m3","role":"developer","content":[{"type":"input_text","text":"<permissions instructions>"}]}}"#,
            "fb",
        )
        .is_none());
        // The event lane still carries both sides of the conversation.
        assert_eq!(
            decode_codex_transcript_line(
                r#"{"type":"event_msg","payload":{"type":"agent_message","message":"I'll do it"}}"#,
                "fb",
            )
            .map(|message| message.role),
            Some(SessionChatRole::Assistant)
        );
    }

    #[test]
    fn shared_block_mapper_accepts_responses_api_text_spellings() {
        let blocks = claude_content_blocks(Some(&json!([
            {"type": "input_text", "text": "in"},
            {"type": "output_text", "text": "out"},
            {"type": "summary_text", "text": "sum"},
            {"type": "encrypted_content", "encrypted_content": "gAAA"},
        ])));
        assert_eq!(
            blocks,
            vec![text_block("in"), text_block("out"), text_block("sum")]
        );
    }

    #[test]
    fn base64_images_render_as_a_pasted_image_chip() {
        // A pasted screenshot has no url and no path; returning no block at all
        // made an image-only user turn vanish from chat entirely.
        let pasted = decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}]}}"#,
            "fb",
        )
        .expect("base64 image decodes");
        assert_eq!(pasted.role, SessionChatRole::User);
        assert_eq!(
            pasted.blocks,
            vec![SessionChatBlock::ImageRef {
                path: None,
                url: None,
                alt: Some("Pasted image".to_string()),
            }]
        );
        // Bytes must never ride the wire.
        let serialized = serde_json::to_string(&pasted).expect("serialize");
        assert!(!serialized.contains("iVBORw0KGgo"));
        // A source with neither bytes nor a locator is still nothing to show.
        assert!(decode_claude_transcript_line(
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":[{"type":"image","source":{"type":"file"}}]}}"#,
            "fb",
        )
        .is_none());
    }

    #[test]
    fn claude_rows_without_uuid_get_distinct_offset_ids() {
        // Both rows of one API response share message.id; using it as the id
        // fallback made the client's id-dedup drop the second row.
        let first = decode_claude_transcript_line(
            r#"{"type":"assistant","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"one"}]}}"#,
            "/tmp/t.jsonl:0000000000000000",
        )
        .expect("first row");
        let second = decode_claude_transcript_line(
            r#"{"type":"assistant","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"two"}]}}"#,
            "/tmp/t.jsonl:0000000000000420",
        )
        .expect("second row");
        assert_ne!(first.id, second.id);
        assert_eq!(first.id, "/tmp/t.jsonl:0000000000000000");
        assert_eq!(second.id, "/tmp/t.jsonl:0000000000000420");
        // A row that HAS a uuid still keys on it.
        assert_eq!(
            decode_claude_transcript_line(
                r#"{"type":"assistant","uuid":"a1","message":{"id":"msg_shared","role":"assistant","content":[{"type":"text","text":"x"}]}}"#,
                "fb",
            )
            .map(|message| message.id),
            Some("a1".to_string())
        );
    }

    #[test]
    fn readers_stamp_byte_offsets_identically_on_every_path() {
        let path = write_temp_transcript(&[
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"one"}}"#,
            r#"{"type":"assistant","uuid":"a1","message":{"role":"assistant","content":[{"type":"text","text":"two"}]}}"#,
        ]);
        let tail = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("tail read");
        let mut state = SessionChatIncrementalState::new();
        let forward = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("forward read");
        assert_eq!(tail.messages.len(), 2);
        assert_eq!(tail.messages[0].byte_offset, Some(0));
        assert_eq!(
            tail.messages
                .iter()
                .map(|message| message.byte_offset)
                .collect::<Vec<_>>(),
            forward
                .iter()
                .map(|message| message.byte_offset)
                .collect::<Vec<_>>(),
            "the same line must report the same offset from both readers",
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn transcript_prompt_state_detects_and_retires_question_cards() {
        let ask = |input: Value| SessionChatMessage {
            id: "ask".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![SessionChatBlock::ToolCall {
                name: "AskUserQuestion".to_string(),
                input,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        };
        let result = SessionChatMessage {
            id: "res".to_string(),
            role: SessionChatRole::Tool,
            blocks: vec![SessionChatBlock::ToolResult {
                output: "Fast".to_string(),
                is_error: None,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        };
        let input = json!({
            "questions": [{"question": "Which approach?", "options": ["Fast", "Careful"]}],
        });

        // Unanswered trailing question ⇒ card, even with no hook prompt at all.
        let pending = scan_transcript_prompt_state(&[ask(input.clone())]);
        assert!(!pending.answered());
        let derived = resolve_session_chat_prompt(None, &pending).expect("card derives");
        assert!(matches!(
            derived,
            SessionChatInteractivePrompt::Question { ref questions } if questions.len() == 1
        ));

        // Its tool result landing means it was answered (possibly in the
        // terminal), so a stored question card must be retired.
        let answered = scan_transcript_prompt_state(&[ask(input.clone()), result.clone()]);
        assert!(answered.answered());
        assert!(resolve_session_chat_prompt(None, &answered).is_none());
        assert!(resolve_session_chat_prompt(
            Some(SessionChatInteractivePrompt::Question {
                questions: vec![SessionChatQuestion {
                    question: "Which approach?".to_string(),
                    header: None,
                    multi_select: false,
                    options: Vec::new(),
                }],
            }),
            &answered,
        )
        .is_none());

        // Hook-derived approvals stay authoritative regardless.
        let approval = SessionChatInteractivePrompt::Approval {
            tool: "Bash".to_string(),
            summary: None,
        };
        assert_eq!(
            resolve_session_chat_prompt(Some(approval.clone()), &answered),
            Some(approval.clone()),
        );
        // …and a hook question outranks a transcript-derived one.
        assert_eq!(
            resolve_session_chat_prompt(Some(approval.clone()), &pending),
            Some(approval),
        );
        // Unrelated tool traffic says nothing either way.
        let quiet = scan_transcript_prompt_state(&[result]);
        assert!(!quiet.answered());
        assert!(quiet.pending().is_none());
    }

    #[test]
    fn earlier_answered_question_does_not_retire_a_newer_stored_card() {
        // A *pending* AskUserQuestion has no assistant row in the transcript
        // yet, so the tail's most recent question is the previous, answered
        // one. The hook-stored card for the NEW question must survive.
        let ask = |input: Value| SessionChatMessage {
            id: "ask-old".to_string(),
            role: SessionChatRole::Assistant,
            blocks: vec![SessionChatBlock::ToolCall {
                name: "AskUserQuestion".to_string(),
                input,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        };
        let result = SessionChatMessage {
            id: "res-old".to_string(),
            role: SessionChatRole::Tool,
            blocks: vec![SessionChatBlock::ToolResult {
                output: "Red".to_string(),
                is_error: None,
            }],
            timestamp: None,
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
        };
        let old_question = json!({
            "questions": [{"question": "Which color do you prefer?", "options": ["Red", "Blue"]}],
        });
        let transcript = scan_transcript_prompt_state(&[ask(old_question), result]);
        assert!(transcript.answered());

        let new_stored = SessionChatInteractivePrompt::Question {
            questions: vec![SessionChatQuestion {
                question: "Which animal do you prefer?".to_string(),
                header: Some("Animal".to_string()),
                multi_select: false,
                options: Vec::new(),
            }],
        };
        // Different question ⇒ kept (this was the regression: it was retired).
        assert_eq!(
            resolve_session_chat_prompt(Some(new_stored.clone()), &transcript),
            Some(new_stored),
        );
        // Same question re-stored ⇒ still retired (answered in the terminal).
        let same_stored = SessionChatInteractivePrompt::Question {
            questions: vec![SessionChatQuestion {
                question: "Which color do you prefer?".to_string(),
                header: None,
                multi_select: false,
                options: Vec::new(),
            }],
        };
        assert!(resolve_session_chat_prompt(Some(same_stored), &transcript).is_none());
    }

    #[test]
    fn embedded_id_scan_requires_a_records_own_session_id() {
        let target = "aaaaaaaa-1111-2222-3333-444444444444";
        let other = "bbbbbbbb-5555-6666-7777-888888888888";
        // A transcript that merely QUOTES the id (orchestrator prompt, hook
        // payload, tool output) must not be adopted.
        let quoting = write_temp_transcript(&[&format!(
            r#"{{"type":"user","sessionId":"{other}","message":{{"role":"user","content":"resume \"session_id\":\"{target}\" please"}}}}"#
        )]);
        assert!(!head_declares_session_id(&quoting, target));
        assert!(head_declares_session_id(&quoting, other));

        // Snake-case spelling counts, sidechain rows do not.
        let sidechain = write_temp_transcript(&[
            &format!(
                r#"{{"type":"user","isSidechain":true,"session_id":"{target}","message":{{"role":"user","content":"sub"}}}}"#
            ),
            r#"{"type":"summary"}"#,
        ]);
        assert!(!head_declares_session_id(&sidechain, target));

        let owned = write_temp_transcript(&[
            &format!(
                r#"{{"type":"user","session_id":"{target}","message":{{"role":"user","content":"go"}}}}"#
            ),
            r#"{"type":"summary"}"#,
            r#"{"type":"other"}"#,
        ]);
        assert!(head_declares_session_id(&owned, target));

        // `agent-*.jsonl` sub-agent transcripts record the PARENT session id in
        // every row and are usually the newest file in the directory.
        let mut sidechain_named = std::env::temp_dir();
        sidechain_named.push(format!("agent-{}.jsonl", std::process::id()));
        fs::write(
            &sidechain_named,
            format!(
                "{{\"type\":\"user\",\"sessionId\":\"{target}\",\"message\":{{\"role\":\"user\",\"content\":\"x\"}}}}\n"
            ),
        )
        .expect("write sidechain transcript");
        assert!(!head_declares_session_id(&sidechain_named, target));

        for path in [quoting, sidechain, owned, sidechain_named] {
            let _ = fs::remove_file(path);
        }
    }

    // ---------------------------------------------------------------------
    // Successor-transcript detection fixtures
    // ---------------------------------------------------------------------

    const FIXTURE_STALE_ID: &str = "11111111-1111-4111-8111-111111111111";
    const FIXTURE_SUCCESSOR_ID: &str = "22222222-2222-4222-8222-222222222222";
    const FIXTURE_SECOND_SUCCESSOR_ID: &str = "33333333-3333-4333-8333-333333333333";
    const FIXTURE_DECOY_ID: &str = "44444444-4444-4444-8444-444444444444";

    fn successor_fixture_dir(name: &str) -> PathBuf {
        static NEXT_FIXTURE_DIR: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gxserver-session-chat-successor-{}-{}-{}",
            std::process::id(),
            name,
            NEXT_FIXTURE_DIR.fetch_add(1, Ordering::SeqCst),
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create fixture dir");
        path
    }

    fn write_fixture_transcript(directory: &Path, file_stem: &str, lines: &[String]) -> PathBuf {
        let path = directory.join(format!("{file_stem}.jsonl"));
        let mut body = String::new();
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        fs::write(&path, body).expect("write fixture transcript");
        path
    }

    fn user_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":"{text}"}}}}"#
        )
    }

    fn assistant_row(session_id: &str, uuid: &str, timestamp: &str, text: &str) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"{text}"}}]}}}}"#
        )
    }

    /// Claude copies the PREDECESSOR id into the resumed file's snake-case
    /// `session_id` while the camelCase `sessionId` stays the new file's own id.
    fn resume_fork_assistant_row(
        session_id: &str,
        predecessor_session_id: &str,
        uuid: &str,
        timestamp: &str,
    ) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"{session_id}","session_id":"{predecessor_session_id}","uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"assistant","stop_reason":"end_turn","content":[{{"type":"text","text":"resumed"}}]}}}}"#
        )
    }

    fn continuation_marker_row(session_id: &str, uuid: &str, timestamp: &str) -> String {
        format!(
            r#"{{"type":"user","sessionId":"{session_id}","isCompactSummary":true,"uuid":"{uuid}","timestamp":"{timestamp}","message":{{"role":"user","content":[{{"type":"text","text":"This session is being continued from a previous conversation that ran out of context. The summary below covers the earlier portion of the conversation."}}]}}}}"#
        )
    }

    fn inherited_snapshot_row(predecessor_session_id: &str) -> String {
        format!(
            r#"{{"type":"file-history-snapshot","messageId":"m1","snapshot":{{"trackedFileBackups":{{"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad/a.txt":{{"realParentDir":"/private/tmp/claude-501/project/{predecessor_session_id}/scratchpad"}}}}}}}}"#
        )
    }

    fn bare_mode_row(session_id: &str) -> String {
        format!(r#"{{"type":"mode","sessionId":"{session_id}","mode":"default"}}"#)
    }

    fn bare_permission_mode_row(session_id: &str) -> String {
        format!(
            r#"{{"type":"permission-mode","sessionId":"{session_id}","permissionMode":"acceptEdits","timestamp":null}}"#
        )
    }

    /// Stale predecessor: real conversation up to 2026-07-31T21:14, then only
    /// bare mode records — exactly the shape that keeps bumping mtime.
    fn write_stale_fixture(directory: &Path) -> PathBuf {
        write_fixture_transcript(
            directory,
            FIXTURE_STALE_ID,
            &[
                user_row(FIXTURE_STALE_ID, "u1", "2026-07-31T21:00:00.000Z", "start"),
                assistant_row(FIXTURE_STALE_ID, "a1", "2026-07-31T21:14:49.796Z", "done"),
                bare_mode_row(FIXTURE_STALE_ID),
                bare_permission_mode_row(FIXTURE_STALE_ID),
            ],
        )
    }

    fn stale_last_substantive_ms(stale_path: &Path) -> i64 {
        last_substantive_transcript_timestamp_ms(stale_path)
            .expect("stale fixture has a substantive record")
    }

    #[test]
    fn continuation_marker_successor_is_adopted() {
        let directory = successor_fixture_dir("continuation");
        let stale = write_stale_fixture(&directory);
        let successor = write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                // Deliberately NO predecessor id field: this exercises the
                // continuation-marker + inherited-snapshot lineage on its own.
                inherited_snapshot_row(FIXTURE_STALE_ID),
                continuation_marker_row(FIXTURE_SUCCESSOR_ID, "c1", "2026-08-01T09:00:00.000Z"),
                assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                    "continuing",
                ),
            ],
        );
        let outcome = find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        );
        match outcome {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
                assert_eq!(found.path, successor);
                assert_eq!(
                    found.lineage,
                    SessionChatSuccessorLineage::ContinuationSnapshot
                );
                assert_eq!(found.hops, 1);
            }
            other => panic!("expected the continuation successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn resume_fork_copy_successor_is_adopted() {
        let directory = successor_fixture_dir("resume-fork");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                user_row(
                    FIXTURE_SUCCESSOR_ID,
                    "u2",
                    "2026-08-01T09:06:00.000Z",
                    "keep going",
                ),
            ],
        );
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SUCCESSOR_ID);
                assert_eq!(
                    found.lineage,
                    SessionChatSuccessorLineage::PredecessorIdField
                );
            }
            other => panic!("expected the resume-fork successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn transcripts_that_merely_mention_the_stale_id_are_not_adopted() {
        let directory = successor_fixture_dir("mention-only");
        let stale = write_stale_fixture(&directory);
        // A busy neighbour session: it declares its own id, carries a
        // continuation marker of its OWN (unrelated) predecessor, and quotes the
        // stale id inside tool input, tool output and a summary record.
        write_fixture_transcript(
            &directory,
            FIXTURE_DECOY_ID,
            &[
                continuation_marker_row(FIXTURE_DECOY_ID, "c9", "2026-08-01T09:00:00.000Z"),
                format!(
                    r#"{{"type":"assistant","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d1","timestamp":"2026-08-01T09:01:00.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"rg {FIXTURE_STALE_ID} ~/.claude/projects"}}}}]}}}}"#
                ),
                format!(
                    r#"{{"type":"user","sessionId":"{FIXTURE_DECOY_ID}","uuid":"d2","timestamp":"2026-08-01T09:02:00.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"{FIXTURE_STALE_ID}.jsonl"}}]}}}}"#
                ),
                format!(
                    r#"{{"type":"summary","summary":"work on {FIXTURE_STALE_ID}","leafUuid":"d1"}}"#
                ),
                assistant_row(FIXTURE_DECOY_ID, "d3", "2026-08-01T09:03:00.000Z", "ok"),
            ],
        );
        // A sub-agent transcript with textbook lineage: still never the
        // session's own conversation.
        write_fixture_transcript(
            &directory,
            "agent-abc123",
            &[resume_fork_assistant_row(
                "agent-abc123",
                FIXTURE_STALE_ID,
                "s1",
                "2026-08-01T09:07:00.000Z",
            )],
        );
        // Records that carry ONLY the stale id (no own identity) — the
        // 6d27b5150 guardrail.
        write_fixture_transcript(
            &directory,
            FIXTURE_SECOND_SUCCESSOR_ID,
            &[format!(
                r#"{{"type":"assistant","sessionId":"{FIXTURE_STALE_ID}","uuid":"z1","timestamp":"2026-08-01T09:08:00.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"copy"}}]}}}}"#
            )],
        );
        assert_eq!(
            find_claude_successor_transcript(
                FIXTURE_STALE_ID,
                &stale,
                stale_last_substantive_ms(&stale),
                &[],
            ),
            SessionChatSuccessorOutcome::NotFound
        );
        let _ = fs::remove_dir_all(&directory);
    }

    /*
    CDXC:SessionChatIdentity 2026-08-02:
    End-to-end runtime path, not just the detector: the real follower loop, a
    fake registry behind the state reader / adopt hook, real transcript files.
    The first cut passed every detector test and still never fired in the live
    daemon, so this exercises spawn → drain → staleness → scan → registry write
    → re-snapshot with the production code path and only the timers shrunk.
    Note `working: false` throughout: hooks never report a background-job
    continuation, so adoption must not depend on the working flag.
    */
    #[tokio::test]
    async fn follower_adopts_a_successor_and_delivers_its_tail() {
        use std::sync::Mutex;

        let directory = successor_fixture_dir("follower-loop");
        let stale = write_stale_fixture(&directory);
        let successor = write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    "a3",
                    "2026-08-01T09:06:00.000Z",
                    "LIVE-SUCCESSOR-TAIL",
                ),
            ],
        );

        // Fake registry: (agentSessionId, agentSessionPath).
        let registry = Arc::new(Mutex::new((
            FIXTURE_STALE_ID.to_string(),
            stale.to_string_lossy().into_owned(),
        )));
        let notices: Arc<Mutex<Vec<SessionChatSuccessorNotice>>> = Arc::new(Mutex::new(Vec::new()));
        let frames: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));

        let state_reader: SessionChatStateReader = {
            let registry = registry.clone();
            Arc::new(move || {
                let (agent_session_id, agent_session_path) =
                    registry.lock().expect("registry").clone();
                SessionChatLiveState {
                    prompt: None,
                    working: false,
                    agent_session_id: Some(agent_session_id),
                    agent_session_path: Some(agent_session_path),
                }
            })
        };
        let hooks = SessionChatSuccessorHooks {
            bound_agent_session_ids: Arc::new(Vec::new),
            adopt_identity: {
                let registry = registry.clone();
                Arc::new(move |adoption| {
                    let mut stored = registry.lock().expect("registry");
                    // Compare-and-set, exactly like the domain write.
                    if Some(stored.0.as_str()) != adoption.previous_agent_session_id.as_deref() {
                        return false;
                    }
                    *stored = (adoption.agent_session_id, adoption.agent_session_path);
                    true
                })
            },
            log: {
                let notices = notices.clone();
                Arc::new(move |notice| notices.lock().expect("notices").push(notice))
            },
        };
        let config = SessionChatFollowerConfig {
            project_id: "P1".to_string(),
            session_id: "S1".to_string(),
            agent: Some("claude".to_string()),
            agent_session_id: Some(FIXTURE_STALE_ID.to_string()),
            agent_session_path: Some(stale.to_string_lossy().into_owned()),
            limit: 50,
            protocol_version: 1,
            server_id: "test-server".to_string(),
            state_reader: Some(state_reader),
            options_reader: None,
            successor_hooks: Some(hooks),
            tuning: SessionChatFollowerTuning {
                reconcile_interval: Duration::from_millis(20),
                stale_transcript_idle: Duration::from_millis(40),
                successor_scan_interval: Duration::from_millis(60),
                successor_stale_substantive_idle_ms: 1_000,
            },
        };
        let emit: SessionChatFrameEmitter = {
            let frames = frames.clone();
            Arc::new(move |frame| frames.lock().expect("frames").push(frame))
        };
        let task = tokio::spawn(run_session_chat_follower(
            config,
            Arc::new(SessionChatStream::new()),
            Arc::new(tokio::sync::Notify::new()),
            emit,
        ));

        let mut adopted_frame: Option<Value> = None;
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let seen = frames.lock().expect("frames").clone();
            adopted_frame = seen.into_iter().find(|frame| {
                frame.get("agentSessionId").and_then(Value::as_str) == Some(FIXTURE_SUCCESSOR_ID)
                    && frame
                        .get("messages")
                        .map(|messages| messages.to_string().contains("LIVE-SUCCESSOR-TAIL"))
                        .unwrap_or(false)
            });
            if adopted_frame.is_some() {
                break;
            }
        }
        task.abort();

        let notices = notices.lock().expect("notices").clone();
        assert!(
            adopted_frame.is_some(),
            "the follower never delivered the successor tail; notices: {notices:?}, frames: {:?}",
            frames
                .lock()
                .expect("frames")
                .iter()
                .map(|frame| frame.get("type").cloned().unwrap_or(Value::Null))
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            *registry.lock().expect("registry"),
            (
                FIXTURE_SUCCESSOR_ID.to_string(),
                successor.to_string_lossy().into_owned()
            ),
            "the corrected identity must be written back to the registry",
        );
        assert!(
            notices.iter().any(|notice| matches!(
                notice,
                SessionChatSuccessorNotice::Adopted(adoption)
                    if adoption.agent_session_id == FIXTURE_SUCCESSOR_ID
                        && adoption.previous_agent_session_id.as_deref() == Some(FIXTURE_STALE_ID)
            )),
            "adoption must be logged once: {notices:?}",
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn successor_bound_to_another_registry_session_is_not_adopted() {
        let directory = successor_fixture_dir("already-bound");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[resume_fork_assistant_row(
                FIXTURE_SUCCESSOR_ID,
                FIXTURE_STALE_ID,
                "a2",
                "2026-08-01T09:05:00.000Z",
            )],
        );
        let stale_ms = stale_last_substantive_ms(&stale);
        assert!(matches!(
            find_claude_successor_transcript(FIXTURE_STALE_ID, &stale, stale_ms, &[]),
            SessionChatSuccessorOutcome::Found(_)
        ));
        assert_eq!(
            find_claude_successor_transcript(
                FIXTURE_STALE_ID,
                &stale,
                stale_ms,
                &[FIXTURE_SUCCESSOR_ID.to_string()],
            ),
            // Reported as owned, NOT as NotFound: the difference is what makes
            // a blocked adoption visible in the log.
            SessionChatSuccessorOutcome::OwnedByAnotherSession {
                candidate_session_ids: vec![FIXTURE_SUCCESSOR_ID.to_string()],
            },
            "a transcript another live session already owns must never be stolen",
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn equally_recent_continuations_adopt_none() {
        let directory = successor_fixture_dir("ambiguous");
        let stale = write_stale_fixture(&directory);
        for stem in [FIXTURE_SUCCESSOR_ID, FIXTURE_SECOND_SUCCESSOR_ID] {
            write_fixture_transcript(
                &directory,
                stem,
                &[resume_fork_assistant_row(
                    stem,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                )],
            );
        }
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Ambiguous {
                predecessor_session_id,
                candidate_session_ids,
            } => {
                assert_eq!(predecessor_session_id, FIXTURE_STALE_ID);
                assert_eq!(candidate_session_ids.len(), 2);
            }
            other => panic!("expected an ambiguous outcome, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn chained_successors_are_followed_to_the_newest() {
        let directory = successor_fixture_dir("chain");
        let stale = write_stale_fixture(&directory);
        write_fixture_transcript(
            &directory,
            FIXTURE_SUCCESSOR_ID,
            &[
                resume_fork_assistant_row(
                    FIXTURE_SUCCESSOR_ID,
                    FIXTURE_STALE_ID,
                    "a2",
                    "2026-08-01T09:05:00.000Z",
                ),
                bare_mode_row(FIXTURE_SUCCESSOR_ID),
            ],
        );
        let newest = write_fixture_transcript(
            &directory,
            FIXTURE_SECOND_SUCCESSOR_ID,
            &[
                inherited_snapshot_row(FIXTURE_SUCCESSOR_ID),
                continuation_marker_row(
                    FIXTURE_SECOND_SUCCESSOR_ID,
                    "c2",
                    "2026-08-01T12:00:00.000Z",
                ),
                assistant_row(
                    FIXTURE_SECOND_SUCCESSOR_ID,
                    "a3",
                    "2026-08-01T12:10:00.000Z",
                    "still going",
                ),
            ],
        );
        match find_claude_successor_transcript(
            FIXTURE_STALE_ID,
            &stale,
            stale_last_substantive_ms(&stale),
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(found) => {
                assert_eq!(found.agent_session_id, FIXTURE_SECOND_SUCCESSOR_ID);
                assert_eq!(found.path, newest);
                assert_eq!(found.hops, 2, "the chain must be followed to its end");
            }
            other => panic!("expected the second successor, got {other:?}"),
        }
        let _ = fs::remove_dir_all(&directory);
    }

    /*
    The dead transcript keeps receiving bare `mode`/`permission-mode` records,
    so its mtime tracks "now" forever. Neither the substantive-staleness clock
    nor the follower's drain may treat that as activity.
    */
    #[test]
    fn bare_mode_appends_do_not_look_like_transcript_activity() {
        let directory = successor_fixture_dir("mtime-bumps");
        let stale = write_stale_fixture(&directory);
        let before = stale_last_substantive_ms(&stale);
        let version_before = read_transcript_file_version(&stale).expect("stat fixture");

        let mut drain_state = FollowerFileState::new();
        let first = follower_drain_once(
            &stale,
            300,
            SessionChatTranscriptAgent::Claude,
            decode_claude_transcript_line,
            Some(decode_claude_turn_lifecycle),
            &mut drain_state,
            true,
        );
        assert!(matches!(first, FollowerDrainOutcome::Snapshot { .. }));

        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&stale)
            .expect("append to fixture");
        writeln!(file, "{}", bare_permission_mode_row(FIXTURE_STALE_ID))
            .expect("write bare record");
        drop(file);

        let version_after = read_transcript_file_version(&stale).expect("stat fixture");
        assert!(
            version_after.size > version_before.size,
            "the fixture must actually have grown"
        );
        assert_eq!(
            last_substantive_transcript_timestamp_ms(&stale),
            Some(before),
            "bare records must not move the substantive clock"
        );
        let second = follower_drain_once(
            &stale,
            300,
            SessionChatTranscriptAgent::Claude,
            decode_claude_transcript_line,
            Some(decode_claude_turn_lifecycle),
            &mut drain_state,
            false,
        );
        assert!(
            matches!(second, FollowerDrainOutcome::Idle),
            "a bare mode append must not count as transcript activity"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn stream_frames_are_published_in_seq_order() {
        use std::sync::Mutex;
        let stream = Arc::new(SessionChatStream::new());
        stream.begin_generation();
        let published: Arc<Mutex<Vec<i64>>> = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let stream = stream.clone();
            let published = published.clone();
            handles.push(std::thread::spawn(move || {
                for _ in 0..50 {
                    stream.emit_sequenced(
                        |seq| json!(seq),
                        |frame| {
                            published
                                .lock()
                                .expect("publish lock")
                                .push(frame.as_i64().expect("seq"));
                        },
                    );
                }
            }));
        }
        for handle in handles {
            handle.join().expect("thread");
        }
        let published = published.lock().expect("publish lock").clone();
        assert_eq!(published.len(), 400);
        assert!(
            published.windows(2).all(|pair| pair[0] < pair[1]),
            "frames must reach the hub in seq order",
        );
    }

    #[test]
    fn grok_decoder_unwraps_user_query_and_skips_bootstrap() {
        assert!(
            decode_grok_transcript_line(r#"{"type":"system","content":"You are Grok"}"#, "fb",)
                .is_none()
        );
        assert!(decode_grok_transcript_line(
            r#"{"type":"user","content":[{"type":"text","text":"ctx"}],"synthetic_reason":"startup"}"#,
            "fb",
        )
        .is_none());
        let user = decode_grok_transcript_line(
            r#"{"type":"user","content":[{"type":"text","text":"<user_query>hello there</user_query>"}]}"#,
            "fb",
        )
        .expect("user");
        assert_eq!(user.blocks, vec![text_block("hello there")]);
        let pasted = decode_grok_transcript_line(
            r#"{"type":"user","content":[{"type":"text","text":"<user_query>/tmp/ghostex-paste-1.png what is this</user_query>"}]}"#,
            "fb",
        )
        .expect("pasted image");
        assert_eq!(
            pasted.blocks,
            vec![
                SessionChatBlock::ImageRef {
                    path: Some("/tmp/ghostex-paste-1.png".to_string()),
                    url: None,
                    alt: None,
                },
                text_block("what is this"),
            ]
        );
        let assistant = decode_grok_transcript_line(
            r#"{"type":"assistant","content":"checking","tool_calls":[{"name":"Read","arguments":"{\"path\":\"/a\"}"}]}"#,
            "fb",
        )
        .expect("assistant");
        assert_eq!(assistant.role, SessionChatRole::Assistant);
        assert_eq!(assistant.blocks.len(), 2);
    }

    #[test]
    fn pi_decoder_maps_shared_chat_blocks() {
        let assistant = decode_pi_transcript_line(
            r#"{"type":"message","id":"a1","parentId":"u1","timestamp":"2026-08-03T10:00:00.000Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":"private plan"},{"type":"text","text":"I will inspect it."},{"type":"toolCall","id":"call-1","name":"read","arguments":{"path":"/tmp/example"}}]}}"#,
            "fb",
        )
        .expect("assistant");
        assert_eq!(assistant.role, SessionChatRole::Assistant);
        assert_eq!(assistant.id, "a1");
        assert_eq!(
            assistant.blocks,
            vec![
                text_block("I will inspect it."),
                SessionChatBlock::ToolCall {
                    name: "read".to_string(),
                    input: json!({"path": "/tmp/example"}),
                },
            ]
        );

        let result = decode_pi_transcript_line(
            r#"{"type":"message","id":"t1","parentId":"a1","message":{"role":"toolResult","toolCallId":"call-1","toolName":"read","content":[{"type":"text","text":"contents"}],"isError":false}}"#,
            "fb",
        )
        .expect("tool result");
        assert_eq!(result.role, SessionChatRole::Tool);
        assert_eq!(
            result.blocks,
            vec![SessionChatBlock::ToolResult {
                output: "contents".to_string(),
                is_error: None,
            }]
        );
    }

    #[test]
    fn pi_tail_reader_follows_the_current_branch() {
        let path = write_temp_transcript(&[
            r#"{"type":"session","version":3,"id":"session-1","timestamp":"2026-08-03T10:00:00.000Z","cwd":"/tmp"}"#,
            r#"{"type":"message","id":"u1","parentId":null,"message":{"role":"user","content":[{"type":"text","text":"root"}]}}"#,
            r#"{"type":"message","id":"a-abandoned","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"old branch"}]}}"#,
            r#"{"type":"message","id":"a-current","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"current branch"}]}}"#,
        ]);
        let tail = read_session_chat_tail_page(SessionChatTranscriptAgent::Pi, &path, 300, None)
            .expect("Pi tail");
        let SessionChatTailPage::Page { messages, .. } = tail else {
            panic!("expected Pi transcript page");
        };
        assert_eq!(
            messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["u1", "a-current"]
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn prompt_derivation_matches_canonical_shapes() {
        // AskUserQuestion tool input on a pre-tool event → question card.
        let tool_input = json!({
            "questions": [
                {
                    "question": "Which approach?",
                    "header": "Approach",
                    "multiSelect": false,
                    "options": [
                        {"label": "Fast", "description": "quick"},
                        "Careful",
                    ],
                },
            ],
        });
        let question = derive_session_chat_prompt(
            Some("ask_user_question"),
            Some(&tool_input),
            Some("PreToolUse"),
        )
        .expect("question derives");
        assert_eq!(
            serde_json::to_value(&question).expect("serialize"),
            json!({
                "kind": "question",
                "questions": [
                    {
                        "question": "Which approach?",
                        "header": "Approach",
                        "multiSelect": false,
                        "options": [
                            {"label": "Fast", "description": "quick"},
                            {"label": "Careful"},
                        ],
                    },
                ],
            })
        );

        // The SAME tool on a post-tool event derives nothing (card would
        // linger after answering otherwise).
        assert!(derive_session_chat_prompt(
            Some("AskUserQuestion"),
            Some(&tool_input),
            Some("PostToolUse"),
        )
        .is_none());
        // Codex 0.145 spelling matches too.
        assert!(derive_session_chat_prompt(
            Some("request_user_input"),
            Some(&tool_input),
            Some("PreToolUse"),
        )
        .is_some());

        // PermissionRequest + tool name → approval with direct-field summary.
        let approval = derive_session_chat_prompt(
            Some("Bash"),
            Some(&json!({"command": "rm -rf /tmp/x", "description": "cleanup"})),
            Some("PermissionRequest"),
        )
        .expect("approval derives");
        assert_eq!(
            serde_json::to_value(&approval).expect("serialize"),
            json!({"kind": "approval", "tool": "Bash", "summary": "rm -rf /tmp/x"})
        );
        // Non-string direct field falls back to the JSON body, capped at 200.
        let long = "x".repeat(300);
        let capped = derive_session_chat_prompt(
            Some("Write"),
            Some(&json!({"file_path": 42, "content": long})),
            Some("PermissionRequest"),
        )
        .expect("capped approval");
        if let SessionChatInteractivePrompt::Approval { summary, .. } = &capped {
            let summary = summary.as_deref().expect("summary present");
            assert_eq!(summary.chars().count(), 201);
            assert!(summary.ends_with('\u{2026}'));
        } else {
            panic!("expected approval");
        }

        // Unrelated tools/events derive nothing.
        assert!(
            derive_session_chat_prompt(Some("Bash"), Some(&tool_input), Some("PreToolUse"))
                .is_none()
        );
        assert!(derive_session_chat_prompt(None, None, Some("PermissionRequest")).is_none());

        // Stored round trip: wire JSON parses back to the same prompt.
        let stored = serde_json::to_string(&question).expect("stringify");
        assert_eq!(parse_stored_session_chat_prompt(&stored), Some(question));

        // Clear rules: post-tool events and Stop/SessionEnd/idle transitions.
        assert!(should_clear_session_chat_prompt(Some("PostToolUse"), None));
        assert!(should_clear_session_chat_prompt(
            Some("post_tool_use_failure"),
            Some("working"),
        ));
        assert!(should_clear_session_chat_prompt(Some("Stop"), Some("idle")));
        assert!(should_clear_session_chat_prompt(Some("SessionEnd"), None));
        assert!(should_clear_session_chat_prompt(
            Some("Notification"),
            Some("idle"),
        ));
        assert!(!should_clear_session_chat_prompt(
            Some("PreToolUse"),
            Some("working"),
        ));
        assert!(!should_clear_session_chat_prompt(
            Some("PermissionRequest"),
            Some("attention"),
        ));
    }

    #[test]
    fn question_parsing_follows_canonical_shape_rules() {
        // Strict multiSelect === true; strings and label objects both parse;
        // malformed options drop; question objects need text OR options.
        let parsed = parse_session_chat_questions(&json!({
            "questions": [
                {"question": "Q1", "multiSelect": true, "options": ["A", {"label": "B"}, 7]},
                {"question": "", "options": []},
                {"question": "Q2", "multiSelect": "yes"},
            ],
        }))
        .expect("questions parse");
        assert_eq!(parsed.len(), 2);
        assert!(parsed[0].multi_select);
        assert_eq!(
            parsed[0]
                .options
                .iter()
                .map(|option| option.label.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B"]
        );
        assert!(!parsed[1].multi_select);
        assert!(parsed[1].options.is_empty());
        assert!(parse_session_chat_questions(&json!({"questions": []})).is_none());
        assert!(parse_session_chat_questions(&json!({"notQuestions": true})).is_none());
    }

    fn write_temp_transcript(lines: &[&str]) -> PathBuf {
        // Tests run in parallel: the name must be unique per CALL, not per
        // line count, or two tests share (and then delete) one file.
        static NEXT_TEMP_TRANSCRIPT: std::sync::atomic::AtomicU64 =
            std::sync::atomic::AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gxserver-session-chat-test-{}-{}.jsonl",
            std::process::id(),
            NEXT_TEMP_TRANSCRIPT.fetch_add(1, Ordering::SeqCst),
        ));
        let mut file = File::create(&path).expect("create temp transcript");
        for line in lines {
            writeln!(file, "{line}").expect("write line");
        }
        path
    }

    #[test]
    fn tail_reader_limit_and_has_more_semantics() {
        let lines: Vec<String> = (0..10)
            .map(|index| {
                format!(
                    r#"{{"type":"user","uuid":"u{index}","message":{{"role":"user","content":"prompt {index}"}}}}"#
                )
            })
            .collect();
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let path = write_temp_transcript(&line_refs);

        let all = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            None,
            Some(decode_claude_turn_lifecycle),
            Some(claude_transcript_lineage),
        )
        .expect("tail read");
        assert_eq!(all.messages.len(), 10);
        assert!(!all.has_more);
        assert_eq!(all.before_offset, 0);
        assert_eq!(
            all.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
            Some(SessionChatTurnLifecycleState::Working)
        );

        let limited = read_session_chat_transcript_tail_file(
            &path,
            3,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("limited read");
        assert_eq!(limited.messages.len(), 3);
        assert!(limited.has_more);
        assert_eq!(
            limited.messages.last().map(|message| message.id.as_str()),
            Some("u9")
        );

        // limit 0 ⇒ [] (never "everything").
        let zero = read_session_chat_transcript_tail_file(
            &path,
            0,
            decode_claude_transcript_line,
            true,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("zero read");
        assert!(zero.messages.is_empty());
        assert!(!zero.has_more);

        // Paging by beforeOffset excludes the newer window.
        let older = read_session_chat_transcript_tail_file(
            &path,
            300,
            decode_claude_transcript_line,
            true,
            Some(limited.before_offset),
            None,
            Some(claude_transcript_lineage),
        )
        .expect("older page");
        assert_eq!(older.messages.len(), 7);
        assert_eq!(
            older.messages.last().map(|message| message.id.as_str()),
            Some("u6")
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn incremental_reader_resumes_from_offset() {
        let path = write_temp_transcript(&[
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"one"}}"#,
        ]);
        let mut state = SessionChatIncrementalState::new();
        let first = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("first pass");
        assert_eq!(first.len(), 1);
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("append");
        writeln!(
            file,
            r#"{{"type":"assistant","uuid":"a1","message":{{"role":"assistant","content":[{{"type":"text","text":"two"}}]}}}}"#
        )
        .expect("append line");
        drop(file);
        let second = read_incremental_transcript_messages(
            &path,
            &mut state,
            decode_claude_transcript_line,
            None,
            None,
            None,
            Some(claude_transcript_lineage),
        )
        .expect("second pass");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, "a1");
        let _ = fs::remove_file(&path);
    }

    /*
    Real-data sanity checks: decode this machine's actual transcripts when
    they exist and assert non-trivial role coverage. Skipped silently on
    machines without the files.
    */
    #[test]
    fn decodes_real_claude_transcript_when_present() {
        let path = home_dir()
            .join(".claude/projects/-Users-madda-dev--active-Ghostex")
            .join("a3aadc10-7b82-417a-96de-e5ca56ae2e3e.jsonl");
        if !path.is_file() {
            return;
        }
        let result = read_session_chat_transcript_tail_file(
            &path,
            100_000,
            decode_claude_transcript_line,
            true,
            None,
            Some(decode_claude_turn_lifecycle),
            Some(claude_transcript_lineage),
        )
        .expect("read real claude transcript");
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::User)
            .count();
        let assistants = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Assistant)
            .count();
        let tools = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Tool)
            .count();
        eprintln!(
            "real claude transcript: {} messages ({} user / {} assistant / {} tool), lifecycle: {:?}, malformed {} oversized {}",
            result.messages.len(),
            users,
            assistants,
            tools,
            result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
            result.malformed_record_count,
            result.oversized_record_count,
        );
        assert!(users > 0, "expected user messages, got {users}");
        assert!(
            assistants > 0,
            "expected assistant messages, got {assistants}"
        );
        assert!(tools > 0, "expected tool messages, got {tools}");
    }

    #[test]
    fn decodes_real_codex_transcript_when_present() {
        let root = home_dir().join(".codex/sessions/2026");
        if !root.is_dir() {
            return;
        }
        let mut newest: Option<(PathBuf, std::time::SystemTime)> = None;
        let mut stack = vec![root];
        while let Some(current) = stack.pop() {
            let Ok(entries) = fs::read_dir(&current) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with("rollout-") || !name.ends_with(".jsonl") {
                    continue;
                }
                let modified = entry
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                if newest
                    .as_ref()
                    .map(|(_, newest_modified)| modified > *newest_modified)
                    .unwrap_or(true)
                {
                    newest = Some((path, modified));
                }
            }
        }
        let Some((path, _)) = newest else {
            return;
        };
        let result = read_session_chat_transcript_tail_file(
            &path,
            100_000,
            decode_codex_transcript_line,
            true,
            None,
            Some(decode_codex_turn_lifecycle),
            None,
        )
        .expect("read real codex transcript");
        let users = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::User)
            .count();
        let tools = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Tool)
            .count();
        let assistants = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Assistant)
            .count();
        let reasoning = result
            .messages
            .iter()
            .filter(|message| message.role == SessionChatRole::Reasoning)
            .count();
        eprintln!(
            "real codex transcript {}: {} messages ({} user / {} assistant / {} reasoning / {} tool), lifecycle: {:?}",
            path.display(),
            result.messages.len(),
            users,
            assistants,
            reasoning,
            tools,
            result.lifecycle.as_ref().map(|lifecycle| lifecycle.state),
        );
        assert!(
            users > 0,
            "expected user messages in {}, got {users}",
            path.display()
        );
        assert!(
            tools > 0,
            "expected tool outputs in {}, got {tools}",
            path.display()
        );
    }

    /*
    Live-files probe for the 2026-08-02 repro: Ghostex session G1ipk still stores
    `agentSessionId = f8ba5a62…` while the pane runs the background-job
    continuation `fb7572ef…`. The directory also holds a SECOND continuation of
    the same predecessor (`d34a3f3c…`, last substantive record 2026-08-01) plus
    ~180 unrelated transcripts, so this asserts the detector picks exactly the
    live one. Skipped on machines without those files.
    */
    #[test]
    fn real_stale_session_resolves_to_its_live_successor_when_present() {
        let stale_session_id = "f8ba5a62-94b4-410f-ba6e-4f462a3e55bc";
        let expected_successor_id = "fb7572ef-2965-4e5e-b21e-bb0e3c455b66";
        let stale_path = home_dir()
            .join(".claude/projects/-Users-madda-dev--active-Ghostex")
            .join(format!("{stale_session_id}.jsonl"));
        let successor_path = stale_path.with_file_name(format!("{expected_successor_id}.jsonl"));
        if !stale_path.is_file() || !successor_path.is_file() {
            return;
        }
        let stale_last_substantive_ms = last_substantive_transcript_timestamp_ms(&stale_path)
            .expect("stale transcript has a substantive record");
        let successor_last_substantive_ms =
            last_substantive_transcript_timestamp_ms(&successor_path)
                .expect("successor transcript has a substantive record");
        eprintln!(
            "stale last substantive {stale_last_substantive_ms}, successor last substantive {successor_last_substantive_ms}"
        );
        // Live-files drift guard (2026-08-07): the "stale" transcript keeps
        // receiving fresh substantive records on this machine, so the
        // staleness premise can expire. Never freeze relationships between
        // live files in tests — skip when the repro state is gone.
        if successor_last_substantive_ms <= stale_last_substantive_ms {
            eprintln!("skipping: stale transcript is no longer stale on this machine");
            return;
        }
        match find_claude_successor_transcript(
            stale_session_id,
            &stale_path,
            stale_last_substantive_ms,
            &[],
        ) {
            SessionChatSuccessorOutcome::Found(successor) => {
                eprintln!(
                    "real successor: {} via {} ({} hop(s))",
                    successor.agent_session_id,
                    successor.lineage.as_str(),
                    successor.hops
                );
                assert_eq!(successor.agent_session_id, expected_successor_id);
                assert_eq!(successor.path, successor_path);
            }
            other => panic!("expected the live successor, got {other:?}"),
        }
        /*
        Bound to another registry session ⇒ never adopted. These are LIVE
        transcripts that keep moving: the second real continuation
        (d34a3f3c…) participates as a fallback candidate only while it is
        still substantively newer than the stale transcript, so the
        drift-dependent shape is asserted conditionally (on 2026-08-02 the
        stale file gained a fresher substantive record and the fallback aged
        out of candidacy).
        */
        let second_continuation_id = "d34a3f3c-3947-4053-8811-af237c1ae288";
        let second_continuation_path =
            stale_path.with_file_name(format!("{second_continuation_id}.jsonl"));
        let second_continuation_ms =
            last_substantive_transcript_timestamp_ms(&second_continuation_path)
                .expect("second continuation has a substantive record");
        if second_continuation_ms > stale_last_substantive_ms {
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[expected_successor_id.to_string()],
                ),
                SessionChatSuccessorOutcome::Found(SessionChatSuccessorTranscript {
                    agent_session_id: second_continuation_id.to_string(),
                    path: second_continuation_path.clone(),
                    lineage: SessionChatSuccessorLineage::PredecessorIdField,
                    last_substantive_ms: second_continuation_ms,
                    hops: 1,
                }),
                "excluding the live successor must fall to the other proven continuation, never to an unrelated file",
            );
            /*
            The 2026-08-02 runtime failure in one assertion: BOTH real
            continuations are also carried by long-STOPPED registry rows
            (G7vq1 / G1z1z here). Feeding those in as owners is what produced
            a silent no-op, which is why the ownership list must contain
            active sessions only — and why this case now reports
            OwnedByAnotherSession instead of NotFound.
            */
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[
                        expected_successor_id.to_string(),
                        second_continuation_id.to_string(),
                    ],
                ),
                SessionChatSuccessorOutcome::OwnedByAnotherSession {
                    candidate_session_ids: vec![
                        expected_successor_id.to_string(),
                        second_continuation_id.to_string(),
                    ],
                },
            );
        } else {
            // Drifted world: with the fallback aged out, excluding the live
            // successor must leave no adoptable candidate — the exclusion
            // itself is the invariant under test.
            assert_eq!(
                find_claude_successor_transcript(
                    stale_session_id,
                    &stale_path,
                    stale_last_substantive_ms,
                    &[expected_successor_id.to_string()],
                ),
                SessionChatSuccessorOutcome::OwnedByAnotherSession {
                    candidate_session_ids: vec![expected_successor_id.to_string()],
                },
            );
        }
    }
}
