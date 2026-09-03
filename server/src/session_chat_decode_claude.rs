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
            item.as_object()
                .and_then(|record| record.get("type"))
                .and_then(Value::as_str)
                == Some("thinking")
        })
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
            leaf_marker: Some(leaf_marker),
        });
    }
    Some(TranscriptLineage {
        id: extract_string(record.get("uuid"))?,
        parent_id: extract_string(record.get("parentUuid")),
        queue: None,
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
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn claude_queue_operation(record: &Map<String, Value>) -> Option<TranscriptQueueOp> {
    let key = extract_string(record.get("content"))
        .map(|content| queued_prompt_key(&content))
        .filter(|key| !key.is_empty());
    match record.get("operation").and_then(Value::as_str)? {
        "enqueue" => Some(TranscriptQueueOp::Enqueued {
            key: key.unwrap_or_default(),
        }),
        "dequeue" | "remove" => Some(TranscriptQueueOp::Left { key }),
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
    let prompt = extract_string(attachment.get("prompt"))?;
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
    let content = extract_string(record.get("content"))?;
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
        queued: true,
    })
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
            queued: false,
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
