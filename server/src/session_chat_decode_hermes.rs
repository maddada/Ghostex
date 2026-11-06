/*
Hermes Agent persists conversations in its session store (`~/.hermes/state.db`
or a profile's own; SQLite `messages` rows), not a jsonl transcript. `session_chat_hermes.rs` mirrors each session's
display history into a per-session jsonl file — one JSON object per logical
message, in the shape decoded here — so the rest of the chat pipeline can treat
Hermes like every other agent. The record shape is Ghostex-owned:

    {"rowId": 15, "role": "tool", "content": "…", "toolCalls": [...],
     "toolName": "terminal", "toolCallId": "call_…", "timestamp": 1787…,
     "finishReason": "stop", "reasoning": "…", "reasoningContent": "…",
     "displayKind": "steer", "displayText": "…"}

plus `{"rowId": 42, "role": "compaction", "timestamp": …}` where Hermes compacted.

`toolCalls` is the OpenAI-style array Hermes stores on assistant rows
(`function.name` + `function.arguments` as a JSON string). Tool rows store the
tool's return value serialized as JSON, usually `{"output": …, "exit_code": …,
"error": …}` for terminal-family tools.
*/

use serde_json::{Map, Value};

use crate::session_chat::*;

use crate::external_sessions::HERMES_INJECTED_PREFIXES;

/// Hermes' wrapper around a message typed mid-turn (`/steer`).
const HERMES_STEER_OPEN: &str = "[OUT-OF-BAND USER MESSAGE";
const HERMES_STEER_CLOSE: &str = "[/OUT-OF-BAND USER MESSAGE]";
/// Compaction restates an unfinished request after its summary under this header.
const HERMES_INFLIGHT_REPLAY_PREFIX: &str = "[STILL IN PROGRESS";
/// Gateway notices (model switch, personality) that Hermes stores as user rows and never shows.
const HERMES_HIDDEN_USER_PREFIX: &str = "[System:";

/*
CDXC:SessionChat 2026-09-28 WHY:
Hermes stores things it did not receive from the user as user rows, and its terminal shows each one its own way. Current Hermes types them with `display_kind`: a finished background subagent batch or process prints one line ("◈ Subagent Tasks Completed: …", its `display_text`) while the row holds the whole report meant for the model, and a `/steer` message is wrapped in an out-of-band marker. Older rows carry only Hermes' synthetic prefixes. Chat drew all of them as the user's own bubble. The restated unfinished request after a compaction is a model handoff the terminal never prints, and the request itself is already in the history.
*/
enum HermesUserRow<'a> {
    Prompt(&'a str),
    Notice(&'a str),
    Hidden,
}

fn trimmed_str<'a>(record: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    record
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn hermes_failed_turn(record: &Map<String, Value>) -> bool {
    record.get("displayKind").and_then(Value::as_str) == Some("failed_turn")
}

fn hermes_user_row(record: &Map<String, Value>) -> HermesUserRow<'_> {
    let Some(content) = trimmed_str(record, "content") else {
        return HermesUserRow::Hidden;
    };
    let steer = || HermesUserRow::Prompt(hermes_steer_text(content).unwrap_or(content));
    let notice = || {
        HermesUserRow::Notice(
            trimmed_str(record, "displayText")
                .unwrap_or_else(|| content.lines().next().unwrap_or(content)),
        )
    };
    match record.get("displayKind").and_then(Value::as_str) {
        Some("hidden") => HermesUserRow::Hidden,
        Some("steer") => steer(),
        Some(_) => notice(),
        None if content.starts_with(HERMES_STEER_OPEN) => steer(),
        None if content.starts_with(HERMES_HIDDEN_USER_PREFIX)
            || content.starts_with(HERMES_INFLIGHT_REPLAY_PREFIX) =>
        {
            HermesUserRow::Hidden
        }
        None if HERMES_INJECTED_PREFIXES
            .iter()
            .any(|prefix| content.starts_with(prefix)) =>
        {
            notice()
        }
        None => HermesUserRow::Prompt(content),
    }
}

fn hermes_steer_text(content: &str) -> Option<&str> {
    let open = content.find(HERMES_STEER_OPEN)?;
    let start = open + content[open..].find('\n')? + 1;
    let end = start + content[start..].find(HERMES_STEER_CLOSE)?;
    Some(content[start..end].trim()).filter(|text| !text.is_empty())
}

fn hermes_row_id(record: &Map<String, Value>, fallback_id: &str) -> String {
    record
        .get("rowId")
        .and_then(Value::as_i64)
        .map(|row_id| format!("hermes-row-{row_id}"))
        .unwrap_or_else(|| fallback_id.to_string())
}

fn hermes_tool_call_blocks(record: &Map<String, Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(tool_calls)) = record.get("toolCalls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            let function = as_record(tool_call.get("function"));
            let name = function
                .and_then(|function| extract_string(function.get("name")))
                .or_else(|| extract_string(tool_call.get("name")))
                .unwrap_or_else(|| "tool".to_string());
            // Arguments arrive as a JSON string; parse so the UI renders
            // structured input instead of an escaped blob.
            let input = function
                .and_then(|function| function.get("arguments"))
                .map(|arguments| match arguments {
                    Value::String(text) => {
                        serde_json::from_str::<Value>(text).unwrap_or(arguments.clone())
                    }
                    other => other.clone(),
                })
                .unwrap_or(Value::Null);
            Some(SessionChatBlock::ToolCall {
                name,
                input,
                call_id: None,
            })
        })
        .collect()
}

/// Terminal-family tools return `{"output": …, "exit_code": …, "error": …}`;
/// other tools return arbitrary JSON. Prefer the human-readable `output`
/// field, and fall back to the raw payload.
fn hermes_tool_result(content: &str) -> (String, Option<bool>) {
    let Ok(Value::Object(record)) = serde_json::from_str::<Value>(content) else {
        return (content.to_string(), None);
    };
    let is_error = record
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""))
        || record.get("success") == Some(&Value::Bool(false))
        || record
            .get("exit_code")
            .and_then(Value::as_i64)
            .is_some_and(|code| code != 0);
    let output = match record.get("output").or_else(|| record.get("result")) {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(value) if !value.is_null() => {
            serde_json::to_string_pretty(value).unwrap_or_else(|_| content.to_string())
        }
        _ => match record.get("error").and_then(Value::as_str) {
            Some(error) if !error.trim().is_empty() => error.to_string(),
            _ => serde_json::to_string_pretty(&Value::Object(record.clone()))
                .unwrap_or_else(|_| content.to_string()),
        },
    };
    (output, if is_error { Some(true) } else { None })
}

pub fn decode_hermes_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let role = record.get("role").and_then(Value::as_str)?;
    let id = hermes_row_id(&record, fallback_id);
    let timestamp = parse_timestamp(record.get("timestamp"));
    let content = extract_string(record.get("content"));
    let message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    };

    match role {
        "user" => match hermes_user_row(&record) {
            HermesUserRow::Prompt(text) => {
                Some(message(SessionChatRole::User, vec![text_block(text)]))
            }
            HermesUserRow::Notice(text) => {
                Some(message(SessionChatRole::System, vec![text_block(text)]))
            }
            HermesUserRow::Hidden => None,
        },
        // The mirror's marker where Hermes compacted; the terminal printed "Compressed: …" there.
        "compaction" => Some(SessionChatMessage {
            id: format!("hermes-compaction-{}", record.get("rowId")?),
            ..message(
                SessionChatRole::System,
                vec![text_block(CONTEXT_COMPACTED_STATUS_TEXT)],
            )
        }),
        "assistant" if hermes_failed_turn(&record) => {
            Some(message(SessionChatRole::System, vec![text_block(content?)]))
        }
        "assistant" => {
            let mut blocks = Vec::new();
            if let Some(text) = content {
                blocks.push(text_block(text));
            }
            blocks.extend(hermes_tool_call_blocks(&record));
            if blocks.is_empty() {
                let reasoning = extract_string(record.get("reasoning"))
                    .or_else(|| extract_string(record.get("reasoningContent")))?;
                return Some(message(
                    SessionChatRole::Reasoning,
                    vec![text_block(reasoning)],
                ));
            }
            Some(message(SessionChatRole::Assistant, blocks))
        }
        "tool" => {
            let (output, is_error) = hermes_tool_result(&content?);
            Some(message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output,
                    is_error,
                    call_id: None,
                    patch: None,
                }],
            ))
        }
        _ => None,
    }
}

/*
Hermes writes no explicit turn markers: a user row opens a turn, and the
closing signal is the final assistant row's `finish_reason`. `tool_calls`
means the turn is still running (the model asked for tools and will be called
again); every other recorded finish reason (`stop`, `length`,
`content_filter`, `incomplete`) ends the turn, and so does a `failed_turn`
notice, which has none. Interrupts, and a failure after a tool ran, write no
closing row at all, so ready-state recovery for them rides the agent-hook
activity signal like Pi's.
*/
pub fn decode_hermes_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let turn_id = hermes_row_id(&record, fallback_id);
    let timestamp = parse_timestamp(record.get("timestamp"));
    let state = match record.get("role").and_then(Value::as_str)? {
        // A restated request or hidden notice is written mid-turn or after a manual /compress; it starts nothing.
        "user" if matches!(hermes_user_row(&record), HermesUserRow::Hidden) => return None,
        "user" => SessionChatTurnLifecycleState::Working,
        "assistant" => {
            // Hermes' failed-turn notice carries no finish reason, yet it is the turn's last row.
            let ends_turn = match record.get("finishReason").and_then(Value::as_str) {
                Some(finish_reason) => !finish_reason.eq_ignore_ascii_case("tool_calls"),
                None => hermes_failed_turn(&record),
            };
            if !ends_turn {
                return None;
            }
            SessionChatTurnLifecycleState::Completed
        }
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id,
        timestamp,
    })
}
