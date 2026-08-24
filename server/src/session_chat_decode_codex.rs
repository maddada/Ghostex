use serde_json::{Map, Value};

use crate::session_chat::*;
use crate::session_chat_decode_claude::{claude_content_block, claude_content_blocks};

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
        queued: false,
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
        queued: false,
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
            let item_type = item.get("type").and_then(Value::as_str);
            /*
            Codex's image generator is an Extension item rather than a tool
            call or an AgentMessage. The TUI renders it as `Generated Image`
            with the revised prompt and saved file; decoding the native event
            preserves that same information and gives the shared image viewer
            the local file path for its thumbnail. The multi-megabyte inline
            `result` stays in the rollout and never crosses the websocket.
            */
            if item_type == Some("Extension")
                && item.get("kind").and_then(Value::as_str) == Some("image_gen.generation")
                && item.get("status").and_then(Value::as_str) == Some("completed")
            {
                let generation_id = extract_string(item.get("id"))?;
                let path = extract_string(item.get("savedPath"))
                    .map(std::path::PathBuf::from)
                    .filter(|candidate| candidate.is_file());
                let revised_prompt = extract_string(item.get("revisedPrompt"));
                let mut detail = revised_prompt.unwrap_or_default();
                if let Some(path) = path.as_ref() {
                    if !detail.is_empty() {
                        detail.push_str("\n\n");
                    }
                    detail.push_str("Saved to: file://");
                    detail.push_str(&path.to_string_lossy());
                }
                let mut blocks = Vec::new();
                if let Some(path) = path {
                    blocks.push(SessionChatBlock::ImageRef {
                        path: Some(path.to_string_lossy().into_owned()),
                        url: None,
                        alt: Some("Generated image".to_string()),
                    });
                }
                blocks.push(SessionChatBlock::ToolCall {
                    name: "Generated Image".to_string(),
                    input: Value::String(detail),
                });
                return Some(SessionChatMessage {
                    id: generation_id,
                    role: SessionChatRole::Assistant,
                    blocks,
                    timestamp,
                    source: SessionChatSource::Transcript,
                    turn_id: None,
                    byte_offset: None,
                    queued: false,
                });
            }
            /*
            CDXC:SessionChatCore 2026-08-23:
            Compaction is a thread item, not a message, and it carries no
            content of its own — the whole record is its type. Codex keeps the
            summarised turns in the rollout, so without this row the chat gives
            the reader nothing at all to explain why the agent stopped knowing
            what it just did; the TUI prints `• Context compacted` at exactly
            this point. Read from the transcript rather than the screen so past
            compactions are still there when the user scrolls back, and so an
            AUTOMATIC one (`compact_token_budget`, no command typed) is marked
            too. It is the only `item_completed` variant decoded here that is
            not a visible message.
            */
            if item_type == Some("ContextCompaction") {
                return Some(SessionChatMessage {
                    id: extract_string(item.get("id")).unwrap_or(id),
                    role: SessionChatRole::System,
                    blocks: vec![text_block(CONTEXT_COMPACTED_STATUS_TEXT)],
                    timestamp,
                    source: SessionChatSource::Transcript,
                    turn_id: None,
                    byte_offset: None,
                    queued: false,
                });
            }
            let role = match item_type {
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
                queued: false,
            })
        }
        _ => None,
    }
}

/*
`item_completed` content blocks use their own spellings: `Text` (capitalized)
in AgentMessages, `text` in UserMessages, and `skill` for a slash-skill
invocation chip (`{type:"skill",name,path}`). The skill chip decodes to
NOTHING: the typed text already carries the `$name` mention (codex-cli 0.149
keeps the composer's `[$name](path)` link verbatim), and folding the chip into
the turn as extra text made the decoded user turn differ from the chat
composer's optimistic echo — the echo was then never consumed and the sent
message stayed duplicated at the bottom of the chat. Anything else falls
through to the shared mapper so future image/attachment blocks render like
Claude's.
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
                Some("skill") => None,
                _ => claude_content_block(record),
            }
        })
        .collect()
}

/// `custom_tool_call` carries its whole input as a raw string, which reaches the
/// same megabyte sizes tool OUTPUTS do, so the input takes the same display
/// bound (`bounded_tool_call_input`). Structured inputs pass through untouched.
fn codex_call_input(payload: &Map<String, Value>) -> Value {
    let raw = if let Some(arguments) = payload.get("arguments") {
        arguments.clone()
    } else {
        payload
            .get("input")
            .filter(|value| !value.is_null())
            .or_else(|| payload.get("action").filter(|value| !value.is_null()))
            .cloned()
            .unwrap_or(Value::Null)
    };
    bounded_tool_call_input(raw)
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
// Grok decoder (grok's ACP session-update log)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatGrokUpdates 2026-08-22:
Grok keeps two logs per session and chat follows the SECOND one.

`chat_history.jsonl` is the persisted conversation, rewritten only when a model
call finishes. A turn that spends a minute reasoning, or answers in one long
message, therefore lands in that file all at once: measured 43s of total silence
for a no-tool answer, and a 78s gap inside a real session's turn, during which
the chat pane sat on the user's prompt while the terminal streamed the reply.
That is the "chat is stuck on my message until I switch away and back" report —
switching remounts and re-reads the file, which by then has the whole turn.

`updates.jsonl` is the ACP session-update stream the TUI itself renders from,
appended block by block: the prompt, every thought, every assistant message,
each tool call and its completion, and a `turn_completed` boundary. It holds the
whole conversation for resumed AND forked sessions (a fork replays the parent's
turns into it), so it is a complete transcript, not just a live tail. It is also
the path grok's own hooks report, so `agentSessionPath` now needs no rewriting.

Two things come free with it: grok gets real turn boundaries (it had no
lifecycle decoder at all, which left the prompt queue's readiness rule guessing),
and the bootstrap rows chat_history keeps — the `<user_info>`/`<rules>` context
block and the `<system-reminder>` envelopes, which chat rendered as the user's
own first messages — do not exist in the update stream at all.

Transcript EXPORT deliberately still reads `chat_history.jsonl`: it is a
one-shot render of a finished conversation, where the persisted form is the
better source. See `resolve_export_transcript_path`.
*/

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
