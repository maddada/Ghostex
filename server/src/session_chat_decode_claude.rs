use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde_json::{Map, Value};

use crate::session_chat::*;

fn is_tool_result_block(block: &SessionChatBlock) -> bool {
    matches!(block, SessionChatBlock::ToolResult { .. })
}

// ---------------------------------------------------------------------------
// Shared block mapping (upstream chat spec §2.1)
// ---------------------------------------------------------------------------

pub(crate) fn claude_content_blocks(content: Option<&Value>) -> Vec<SessionChatBlock> {
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
            item.as_object().is_some_and(|record| {
                record.get("type").and_then(Value::as_str) == Some("thinking")
                    && !claude_thinking_is_narration(record)
            })
        })
}

/// CDXC:SessionChat 2026-09-26 WHY:
/// Claude Code (2.1.281 with Opus 5.5) saves the prose it writes between tool calls as a `thinking` block whose signature the API tags `narration`, and its terminal paints that block as an ordinary reply. Decoded as reasoning, the row never retired the reply chat had already streamed off the terminal (`terminal_stream_retired` in gx-chat-core matches assistant and system rows only), so the same paragraph showed twice: a collapsed thinking row and a reply. The tag is read the way Claude Code's own classifier reads it (signature envelope field 2, then 1, then 8), and a narration block without text stays ordinary thinking, as Claude Code documents.
fn claude_thinking_is_narration(record: &Map<String, Value>) -> bool {
    if extract_string(record.get("thinking")).is_none() {
        return false;
    }
    let Some(signature) = record.get("signature").and_then(Value::as_str) else {
        return false;
    };
    let Ok(envelope) = BASE64_STANDARD.decode(signature) else {
        return false;
    };
    protobuf_bytes_field(&envelope, 2)
        .and_then(|inner| protobuf_bytes_field(inner, 1))
        .and_then(|inner| protobuf_bytes_field(inner, 8))
        == Some(b"narration".as_slice())
}

/// The last length-delimited value of `field` in a protobuf message; `None` when the field is
/// absent or the message does not parse.
fn protobuf_bytes_field(message: &[u8], field: u64) -> Option<&[u8]> {
    let mut found = None;
    let mut rest = message;
    while !rest.is_empty() {
        let key;
        (key, rest) = protobuf_varint(rest)?;
        match key & 7 {
            0 => (_, rest) = protobuf_varint(rest)?,
            1 => rest = rest.get(8..)?,
            2 => {
                let length;
                (length, rest) = protobuf_varint(rest)?;
                let length = usize::try_from(length)
                    .ok()
                    .filter(|length| *length <= rest.len())?;
                let (value, tail) = rest.split_at(length);
                if key >> 3 == field {
                    found = Some(value);
                }
                rest = tail;
            }
            5 => rest = rest.get(4..)?,
            _ => return None,
        }
    }
    found
}

fn protobuf_varint(bytes: &[u8]) -> Option<(u64, &[u8])> {
    let mut value = 0u64;
    for (index, byte) in bytes.iter().enumerate().take(10) {
        value |= u64::from(byte & 0x7f) << (7 * index);
        if byte & 0x80 == 0 {
            return Some((value, &bytes[index + 1..]));
        }
    }
    None
}

/*
CDXC:SessionChat 2026-08-01:
`input_text`/`output_text`/`summary_text` are the Responses-API spellings Codex
writes inside `response_item` content arrays and inside `custom_tool_call_output`
payloads. They carry the same `text` field as Anthropic's `text` block, so the
shared mapper accepts all of them; without this a whole Codex lane decoded to
nothing. `encrypted_content` is deliberately unmapped — it has no readable text.
*/
pub(crate) fn claude_content_block(record: &Map<String, Value>) -> Option<SessionChatBlock> {
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
            call_id: extract_string(record.get("id")),
        }),
        Some("tool_result") => Some(SessionChatBlock::ToolResult {
            output: tool_result_output(record.get("content")),
            is_error: if record.get("is_error") == Some(&Value::Bool(true)) {
                Some(true)
            } else {
                None
            },
            call_id: extract_string(record.get("tool_use_id")),
        }),
        Some("image" | "input_image") => image_ref_block(record),
        _ => None,
    }
}

/*
CDXC:AgentScreenDetection 2026-08-28:
Claude Code records a safeguards refusal as a SYNTHETIC assistant row it writes
itself (`message.model` is `<synthetic>`): `isApiErrorMessage: true` plus
`message.stop_reason == "refusal"` (mirrored in `stop_details.type`), with the
full user-facing explanation — including the category tag and request id — as
the row's only text block. Detection reads exactly those structured fields,
never the prose, so an agent that merely QUOTES an "API Error:" line cannot
trigger it. Transient API errors (retries, overloads) carry
`isApiErrorMessage` without a `refusal` stop reason and are deliberately not
matched: they resolve on their own and a notice for them would be noise.
*/
pub(crate) fn claude_api_refusal_text(line: &str) -> Option<String> {
    let record = parse_json_object(line)?;
    if record.get("type").and_then(Value::as_str) != Some("assistant")
        || record.get("isApiErrorMessage") != Some(&Value::Bool(true))
    {
        return None;
    }
    let message = record.get("message")?.as_object()?;
    let refusal = message.get("stop_reason").and_then(Value::as_str) == Some("refusal")
        || message
            .get("stop_details")
            .and_then(Value::as_object)
            .and_then(|details| details.get("type"))
            .and_then(Value::as_str)
            == Some("refusal");
    if !refusal {
        return None;
    }
    let text = message
        .get("content")?
        .as_array()?
        .iter()
        .filter_map(|block| {
            let block = block.as_object()?;
            if block.get("type").and_then(Value::as_str) != Some("text") {
                return None;
            }
            block.get("text").and_then(Value::as_str)
        })
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Claude stamps `uuid`/`parentUuid` on every non-sidechain row, including the
/// `system` and `attachment` rows a prompt can hang off, so the whole tree is
/// readable from the same scan the decoder already runs.
pub(crate) fn claude_transcript_lineage(
    line: &str,
    fallback_id: &str,
) -> Option<TranscriptLineage> {
    claude_transcript_lineage_record(&parse_json_object(line)?, fallback_id)
}

/// Same rule as `claude_transcript_lineage` for a record the caller already
/// parsed. The branch scanner reads several fields per row and must not pay for
/// a second parse of every line.
pub(crate) fn claude_transcript_lineage_record(
    record: &Map<String, Value>,
    fallback_id: &str,
) -> Option<TranscriptLineage> {
    if record.get("isSidechain") == Some(&Value::Bool(true)) {
        return None;
    }
    let record_type = record.get("type").and_then(Value::as_str);
    if record_type == Some(CLAUDE_QUEUE_RECORD_TYPE) {
        return Some(TranscriptLineage {
            id: fallback_id.to_string(),
            parent_id: None,
            queue: Some(claude_queue_operation(record)?),
            delivered_queue_keys: Vec::new(),
            leaf_marker: None,
        });
    }
    if record_type == Some(CLAUDE_LEAF_MARKER_RECORD_TYPE) {
        if record.get("explicit") != Some(&Value::Bool(true)) {
            return None;
        }
        let leaf_marker = match extract_string(record.get("leafUuid")) {
            Some(leaf) => TranscriptLeafMarker::Row(leaf),
            None => TranscriptLeafMarker::Empty,
        };
        return Some(TranscriptLineage {
            id: fallback_id.to_string(),
            parent_id: None,
            queue: None,
            delivered_queue_keys: Vec::new(),
            leaf_marker: Some(leaf_marker),
        });
    }
    Some(TranscriptLineage {
        id: extract_string(record.get("uuid"))?,
        parent_id: extract_string(record.get("parentUuid")),
        queue: None,
        delivered_queue_keys: claude_delivered_queue_keys(record),
        leaf_marker: None,
    })
}

/// A Claude row can only decode to a `User` message when it is one of these
/// types, so the branch scanner skips the decode for every other row instead of
/// paying for one on assistant rows that carry whole tool payloads.
pub(crate) fn claude_record_type_can_be_prompt(record: &Map<String, Value>) -> bool {
    matches!(
        record.get("type").and_then(Value::as_str),
        Some("user" | CLAUDE_ATTACHMENT_RECORD_TYPE)
    )
}

/*
CDXC:SessionChat 2026-08-19:
Queue text is matched between the enqueue row and the removal row that releases
it, and the two are not byte-identical (the removal echoes what the composer
finally submitted). Whitespace folding is the narrowest normalization that
makes them agree.
*/
fn queued_prompt_key(text: &str) -> String {
    let text = strip_claude_pasted_content_envelopes(text);
    cross_session_queue_key(&text)
        .unwrap_or(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// CDXC:SessionChat 2026-09-25 WHY:
/// Claude writes a peer message's `hop-chain` attribute on its enqueue row only; the removal row and the delivered row drop it. Keyed by its opening tag, the queued copy never matched its release and stayed on screen next to the delivered message, so a `<cross-session-message>` envelope is keyed by its body.
fn cross_session_queue_key(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix(CLAUDE_CROSS_SESSION_OPEN)?;
    if !rest.starts_with([' ', '>']) {
        return None;
    }
    let body = rest
        .split_once('>')?
        .1
        .strip_suffix(CLAUDE_CROSS_SESSION_CLOSE)?;
    Some(format!("{CLAUDE_CROSS_SESSION_OPEN}>{body}"))
}

/// CDXC:SessionChat 2026-09-08 DECISION:
/// User: agent finished/stopped notifications should not appear twice in a row.
/// CDXC:SessionChat 2026-09-08 WHY:
/// Claude can dequeue a newer agent notification ahead of older background-command notifications, so treating its content-free `dequeue` as FIFO removes the wrong entry and leaves a duplicate queued pill.
/// The delivered `user` row names the actual content; `queued_command` attachments already have a keyed `remove` and must not release a second identical entry.
/// Match each delivery, not task ids globally, because a resumed agent can finish more than once.
fn claude_delivered_queue_keys(record: &Map<String, Value>) -> Vec<String> {
    if record.get("type").and_then(Value::as_str) != Some("user")
        || (record.get("promptSource").and_then(Value::as_str) != Some("queued")
            && record.get("queueSkipAttachments") != Some(&Value::Bool(true)))
    {
        return Vec::new();
    }
    let content = as_record(record.get("message")).and_then(|message| message.get("content"));
    let blocks = claude_content_blocks(content);
    // A peer message was queued as its bare envelope; the delivered row wraps it in Claude's preamble.
    if record.get("isMeta") == Some(&Value::Bool(true)) {
        if let Some(envelope) = claude_cross_session_envelope(&blocks) {
            return vec![queued_prompt_key(&envelope)];
        }
    }
    blocks
        .into_iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => {
                let key = queued_prompt_key(&text);
                (!key.is_empty()).then_some(key)
            }
            _ => None,
        })
        .collect()
}

fn claude_queue_operation(record: &Map<String, Value>) -> Option<TranscriptQueueOp> {
    let key = extract_string(record.get("content"))
        .map(|content| queued_prompt_key(&content))
        .filter(|key| !key.is_empty());
    match record.get("operation").and_then(Value::as_str)? {
        "enqueue" => Some(TranscriptQueueOp::Enqueued {
            key: key.unwrap_or_default(),
        }),
        "remove" => Some(TranscriptQueueOp::Left { key }),
        "popAll" => Some(TranscriptQueueOp::Cleared),
        _ => None,
    }
}

fn claude_interrupted_message_id(record: &Map<String, Value>) -> Option<String> {
    if record.get("type").and_then(Value::as_str) != Some("user") {
        return None;
    }
    extract_string(record.get("interruptedMessageId"))
}

const CLAUDE_QUEUE_RECORD_TYPE: &str = "queue-operation";
const CLAUDE_LEAF_MARKER_RECORD_TYPE: &str = "last-prompt";
const CLAUDE_ATTACHMENT_RECORD_TYPE: &str = "attachment";
const CLAUDE_QUEUED_COMMAND_ATTACHMENT: &str = "queued_command";

/*
CDXC:SessionChat 2026-08-19:
A prompt the user typed mid-turn is NOT written as a `user` row when the
harness injects it into the running turn: the queue entry is released as an
`attachment`/`queued_command` row and the model answers from that. The prompt
therefore exists nowhere else in the file (verified on `13b6c3ae…`, where "please
babysit this pr …" appears only as the enqueue row, its removal row, and this
attachment), so skipping it dropped a real, answered user turn from chat while
the terminal showed it — and the optimistic echo that stood in for it vanished
on the next remount.

Every `queued_command` is decoded, not just the human-authored ones: the
harness-injected envelopes it also carries (`<task-notification>`,
`<cross-session-message>`) are exactly what the shared noise classifier already
collapses for ordinary user rows, so routing them through the same rule keeps
one source of truth instead of an `origin.kind` whitelist that silently drops
turns whenever the harness adds a kind.
*/
fn decode_claude_queued_command(
    record: &Map<String, Value>,
    fallback_id: &str,
) -> Option<SessionChatMessage> {
    let attachment = as_record(record.get("attachment"))?;
    if attachment.get("type").and_then(Value::as_str) != Some(CLAUDE_QUEUED_COMMAND_ATTACHMENT) {
        return None;
    }
    let prompt = strip_claude_pasted_content_envelopes(&extract_string(attachment.get("prompt"))?);
    if prompt.trim().is_empty() {
        return None;
    }
    Some(SessionChatMessage {
        id: extract_string(record.get("uuid")).unwrap_or_else(|| fallback_id.to_string()),
        role: SessionChatRole::User,
        blocks: vec![text_block(prompt)],
        timestamp: parse_timestamp(record.get("timestamp"))
            .or_else(|| parse_timestamp(attachment.get("timestamp"))),
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
    })
}

/// The enqueue row is the only record of a prompt while it waits in the queue.
/// It is published immediately and retracted by the reader the moment the queue
/// releases it, so the label can never outlive the wait.
fn decode_claude_queued_prompt(
    record: &Map<String, Value>,
    fallback_id: &str,
) -> Option<SessionChatMessage> {
    if record.get("operation").and_then(Value::as_str) != Some("enqueue") {
        return None;
    }
    let content = strip_claude_pasted_content_envelopes(&extract_string(record.get("content"))?);
    if content.trim().is_empty() {
        return None;
    }
    Some(SessionChatMessage {
        id: fallback_id.to_string(),
        role: SessionChatRole::User,
        blocks: vec![text_block(content)],
        timestamp: parse_timestamp(record.get("timestamp")),
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: true,
    })
}

const CLAUDE_CROSS_SESSION_OPEN: &str = "<cross-session-message";
const CLAUDE_CROSS_SESSION_CLOSE: &str = "</cross-session-message>";

/// CDXC:SessionChat 2026-09-25 WHY:
/// A message another Claude session sent renders as chat's message-from-another-agent card. When it reaches an idle session Claude writes a meta user row: a one-line preamble ("Another Claude session sent a message:"), the `<cross-session-message>` envelope, then trust instructions meant for the model. The meta filter dropped the whole row, so chat never showed the message. Only the envelope is kept, the same text a mid-turn delivery's `queued_command` carries, so both deliveries render alike. Anything longer than one line before the envelope (a compaction summary quoting one) is not a delivery.
/// SEE-ALSO: packages/gx-chat-core/src/transcript/agent_message.rs parses the envelope into the card.
fn claude_cross_session_envelope(blocks: &[SessionChatBlock]) -> Option<String> {
    let text = blocks
        .iter()
        .filter_map(|block| match block {
            SessionChatBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let start = text.find(CLAUDE_CROSS_SESSION_OPEN)?;
    if text[..start].trim().contains('\n')
        || !text[start + CLAUDE_CROSS_SESSION_OPEN.len()..].starts_with([' ', '>'])
    {
        return None;
    }
    let end = text.rfind(CLAUDE_CROSS_SESSION_CLOSE)? + CLAUDE_CROSS_SESSION_CLOSE.len();
    (end > start).then(|| text[start..end].to_string())
}

pub fn decode_claude_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let role = record.get("type").and_then(Value::as_str)?;
    if role == CLAUDE_ATTACHMENT_RECORD_TYPE {
        return decode_claude_queued_command(&record, fallback_id);
    }
    if role == CLAUDE_QUEUE_RECORD_TYPE {
        return decode_claude_queued_prompt(&record, fallback_id);
    }
    /*
    Claude Code records composer rejections as top-level informational system
    rows, not as assistant content. These are the only durable explanation for
    a prompt that never reached the model (for example `Unknown command` and
    `Args from unknown skill`), and the terminal prints them as ordinary
    visible warning lines. Keep only warning/error informational rows: the
    other system record families are lifecycle/config plumbing the terminal
    does not present as conversation content.
    */
    if role == "system"
        && record.get("subtype").and_then(Value::as_str) == Some("informational")
        && matches!(
            record.get("level").and_then(Value::as_str),
            Some("warning" | "error")
        )
    {
        let content = extract_string(record.get("content"))?;
        return Some(SessionChatMessage {
            id: extract_string(record.get("uuid")).unwrap_or_else(|| fallback_id.to_string()),
            role: SessionChatRole::System,
            blocks: vec![text_block(content)],
            timestamp: parse_timestamp(record.get("timestamp")),
            source: SessionChatSource::Transcript,
            turn_id: None,
            byte_offset: None,
            async_questions: None,
            queued: false,
        });
    }
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
            async_questions: None,
            queued: false,
        });
    }

    let message = record.get("message").and_then(Value::as_object);
    let content = message.and_then(|inner| inner.get("content"));
    let mut decoded_blocks = claude_content_blocks(content);
    if role == "user" {
        decoded_blocks = decoded_blocks
            .into_iter()
            .map(unwrap_claude_pasted_content_block)
            .collect();
    }
    if decoded_blocks.is_empty() {
        return None;
    }

    // (B) Injected/meta user turns keep only genuine tool-result output.
    let is_injected_user_turn = role == "user"
        && (record.get("isMeta") == Some(&Value::Bool(true))
            || record.get("isSynthetic") == Some(&Value::Bool(true))
            || record.get("isCompactSummary") == Some(&Value::Bool(true)));
    let cross_session_envelope = (role == "user"
        && record.get("isMeta") == Some(&Value::Bool(true)))
    .then(|| claude_cross_session_envelope(&decoded_blocks))
    .flatten();
    let blocks: Vec<SessionChatBlock> = if let Some(envelope) = cross_session_envelope {
        vec![text_block(envelope)]
    } else if is_injected_user_turn {
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
    CDXC:SessionChat 2026-08-01:
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
        async_questions: None,
        queued: false,
    })
}

// ---------------------------------------------------------------------------
// Codex decoder (upstream chat spec §2.3)
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

pub(crate) fn message_text(message: &SessionChatMessage) -> String {
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

pub(crate) fn is_known_harness_injected_user_turn_text(text: &str) -> bool {
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

    // CDXC:SessionChat 2026-09-16 WHY:
    // Claude records a plain /compact user turn but no assistant end_turn afterward, leaving queued prompts held forever despite an idle terminal.
    // The screen marker holds the queue during compaction, and only a manual boundary settles the prior turn; automatic compaction continues the active response.
    if record.get("type").and_then(Value::as_str) == Some("system")
        && record.get("subtype").and_then(Value::as_str) == Some("compact_boundary")
        && record
            .get("compactMetadata")
            .and_then(|metadata| metadata.get("trigger"))
            .and_then(Value::as_str)
            == Some("manual")
    {
        return Some(SessionChatTurnLifecycle {
            state: SessionChatTurnLifecycleState::Completed,
            turn_id: extract_string(record.get("uuid")).unwrap_or_else(|| fallback_id.to_string()),
            timestamp,
        });
    }

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
        // CDXC:SessionChat 2026-09-09 WHY:
        // Claude streams text and thinking with an explicit null stop_reason before later tool calls or end_turn; treating null as a historical missing field folds live subagent work immediately.
        let stop_reason_absent = message.is_none_or(|inner| !inner.contains_key("stop_reason"));
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
    if crate::session_chat_terminal_activity::transcript_message_starts_session_chat_activity(
        Some("claude"),
        &decoded,
    ) {
        // /compact starts screen-owned work, not an assistant generation, including when Claude declines it because there are not enough messages.
        return None;
    }
    Some(SessionChatTurnLifecycle {
        state: SessionChatTurnLifecycleState::Working,
        turn_id: decoded.id,
        timestamp,
    })
}

// ---------------------------------------------------------------------------
// Claude Code's pasted-content envelope
// ---------------------------------------------------------------------------

const CLAUDE_PASTED_CONTENT_OPEN: &str = "<pasted_content id=\"";
const CLAUDE_PASTED_CONTENT_CLOSE: &str = "</pasted_content";

/*
CDXC:SessionChat 2026-09-20 WHY:
Chat delivers every multi-line send as a bracketed paste (`build_session_chat_paste_bytes`), and Claude Code records the submitted prompt with each pasted span wrapped in `<pasted_content id="ab12">` … `</pasted_content id="ab12">`, escaping a literal tag spelling inside the body by slipping a backslash in behind the `<`.
Those tags exist only to tell the model where the clipboard text began: Claude Code's own TUI shows the collapsed `[Pasted text #1 …]` placeholder and never prints them, so a transcript that renders the row verbatim put markup the user never typed into their own chat bubble and left the optimistic echo unmatched against the row it belongs to.
Unwrap the envelope for user rows only; assistant text is never wrapped.
*/
pub(crate) fn strip_claude_pasted_content_envelopes(text: &str) -> String {
    if !text.contains(CLAUDE_PASTED_CONTENT_OPEN) {
        return text.to_string();
    }
    let mut unwrapped = String::with_capacity(text.len());
    let mut rest = text;
    let mut envelopes = 0usize;
    while let Some(open_at) = rest.find(CLAUDE_PASTED_CONTENT_OPEN) {
        let after_open = &rest[open_at + CLAUDE_PASTED_CONTENT_OPEN.len()..];
        let Some(id_end) = after_open.find("\">") else {
            break;
        };
        let id = &after_open[..id_end];
        let body = &after_open[id_end + "\">".len()..];
        let Some((pasted, after_close)) = split_claude_pasted_content_body(body, id) else {
            break;
        };
        unwrapped.push_str(&rest[..open_at]);
        unwrapped.push_str(&unescape_claude_pasted_content(pasted));
        rest = after_close;
        envelopes += 1;
    }
    if envelopes == 0 {
        return text.to_string();
    }
    unwrapped.push_str(rest);
    // The envelope carries its own padding newlines around the prompt.
    unwrapped.trim().to_string()
}

/// Splits `body` at the close tag that matches `id`, returning the pasted text
/// and everything after the tag. A body carrying the tag spelling has it
/// escaped, so the first well-formed close is the real one.
fn split_claude_pasted_content_body<'a>(body: &'a str, id: &str) -> Option<(&'a str, &'a str)> {
    let close_with_id = format!(" id=\"{id}\">");
    let mut searched = 0usize;
    while let Some(relative) = body[searched..].find(CLAUDE_PASTED_CONTENT_CLOSE) {
        let close_at = searched + relative;
        let tail = &body[close_at + CLAUDE_PASTED_CONTENT_CLOSE.len()..];
        if let Some(after_close) = tail
            .strip_prefix('>')
            .or_else(|| tail.strip_prefix(close_with_id.as_str()))
        {
            let pasted = &body[..close_at];
            let pasted = pasted.strip_prefix('\n').unwrap_or(pasted);
            let pasted = pasted.strip_suffix('\n').unwrap_or(pasted);
            return Some((pasted, after_close));
        }
        searched = close_at + CLAUDE_PASTED_CONTENT_CLOSE.len();
    }
    None
}

/// Claude Code escapes a literal tag spelling inside a pasted body by slipping
/// a backslash in behind the `<`, so `<\pasted_content` is text the user typed.
fn unescape_claude_pasted_content(pasted: &str) -> String {
    pasted
        .replace("<\\/pasted_content", "</pasted_content")
        .replace("<\\pasted_content", "<pasted_content")
}

fn unwrap_claude_pasted_content_block(block: SessionChatBlock) -> SessionChatBlock {
    match block {
        SessionChatBlock::Text { text } => text_block(strip_claude_pasted_content_envelopes(&text)),
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Reverse tail reader (upstream chat spec §4)
// ---------------------------------------------------------------------------
