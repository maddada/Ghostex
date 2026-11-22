/*
CDXC:SessionChat 2026-10-17 WHY:
GitHub Copilot CLI (measured on 1.0.98) writes every session to
`~/.copilot/session-state/<sessionId>/events.jsonl` (or `$COPILOT_HOME/…`), and its
hooks name that file as `transcript_path`. One JSON event per line:

    {"type":"user.message","data":{"content":"…","delivery":"idle","interactionId":"…"},"id":"…","timestamp":"…"}
    {"type":"assistant.message","data":{"content":"…","toolRequests":[{"toolCallId":"…","name":"bash","arguments":{…}}],"reasoningText":"…"}}
    {"type":"tool.execution_complete","data":{"toolCallId":"…","success":true,"result":{"content":"…"}}}
    {"type":"abort","data":{"reason":"user_initiated"}}

Copilot's own schema ships with the CLI (`schemas/session-events.schema.json`). Events
a sub-agent produced carry `parentToolCallId` and stay out of the main chat. Copilot's
tool names (`bash`, `edit`, `create`, `view`) are shown under the names the chat's
shared tool tables know (Bash, Edit, Write, Read), with their inputs renamed to match,
so edits draw their diff and commands their command line. No persisted event marks the
end of a turn: the agent loop is over when the model answers without asking for a tool,
and an `abort` is a Stop.
*/

use serde_json::{json, Map, Value};

use crate::session_chat::*;

/// The prefix Copilot puts on a notification it feeds the model as a user turn.
const COPILOT_SYSTEM_NOTIFICATION_PREFIX: &str = "<system_notification>";

fn copilot_event(line: &str) -> Option<(String, Map<String, Value>, Map<String, Value>)> {
    let record = parse_json_object(line)?;
    let kind = record.get("type")?.as_str()?.to_string();
    let data = record.get("data")?.as_object()?.clone();
    Some((kind, record, data))
}

fn from_sub_agent(data: &Map<String, Value>) -> bool {
    extract_string(data.get("parentToolCallId")).is_some()
}

/// The chat's name for a Copilot tool, and its input in the shape that name expects.
fn copilot_tool_call(name: &str, arguments: Option<&Value>) -> (String, Value) {
    let arguments = arguments.cloned().unwrap_or(Value::Null);
    let field = |key: &str| arguments.get(key).cloned().unwrap_or(Value::Null);
    match name {
        "bash" | "powershell" => (
            "Bash".to_string(),
            json!({ "command": field("command"), "description": field("description") }),
        ),
        "edit" | "str_replace_editor" => (
            "Edit".to_string(),
            json!({
                "file_path": field("path"),
                "old_string": field("old_str"),
                "new_string": field("new_str"),
            }),
        ),
        "create" => (
            "Write".to_string(),
            json!({ "file_path": field("path"), "content": field("file_text") }),
        ),
        "view" => {
            let mut input = json!({ "file_path": field("path") });
            if let Some(range) = arguments.get("view_range").filter(|value| !value.is_null()) {
                input["view_range"] = range.clone();
            }
            ("Read".to_string(), input)
        }
        "grep" | "rg" => ("Grep".to_string(), arguments),
        "glob" => ("Glob".to_string(), arguments),
        "web_fetch" => ("WebFetch".to_string(), arguments),
        _ => (name.to_string(), bounded_tool_call_input(arguments)),
    }
}

fn copilot_tool_call_blocks(data: &Map<String, Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(requests)) = data.get("toolRequests") else {
        return Vec::new();
    };
    requests
        .iter()
        .filter_map(|request| {
            let request = request.as_object()?;
            let name = extract_string(request.get("name"))?;
            // Copilot's own bookkeeping call, which its terminal never draws as a tool row.
            if name == "report_intent" {
                return None;
            }
            let (name, input) = copilot_tool_call(&name, request.get("arguments"));
            Some(SessionChatBlock::ToolCall {
                name,
                input,
                call_id: extract_string(request.get("toolCallId")),
            })
        })
        .collect()
}

fn copilot_tool_result(data: &Map<String, Value>) -> SessionChatBlock {
    let success = data.get("success").and_then(Value::as_bool) != Some(false);
    let output = if success {
        tool_result_output(as_record(data.get("result")).and_then(|result| result.get("content")))
    } else {
        let error = as_record(data.get("error"));
        extract_string(error.and_then(|error| error.get("message")))
            .or_else(|| extract_string(data.get("error")))
            .unwrap_or_else(|| {
                tool_result_output(
                    as_record(data.get("result")).and_then(|result| result.get("content")),
                )
            })
    };
    SessionChatBlock::ToolResult {
        output: bounded_tool_payload(output),
        is_error: (!success).then_some(true),
        call_id: extract_string(data.get("toolCallId")),
        patch: None,
    }
}

fn copilot_user_message_hidden(data: &Map<String, Value>, content: &str) -> bool {
    content.starts_with(COPILOT_SYSTEM_NOTIFICATION_PREFIX)
        || data.get("isAutopilotContinuation").and_then(Value::as_bool) == Some(true)
        // A skill's hidden context injection (`skill-pdf`, …), which the terminal never shows.
        || extract_string(data.get("source")).is_some_and(|source| source.starts_with("skill"))
}

pub fn decode_copilot_transcript_line(line: &str, fallback_id: &str) -> Option<SessionChatMessage> {
    let (kind, record, data) = copilot_event(line)?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let id = extract_string(record.get("id"))
        .map(|id| format!("copilot-{id}"))
        .unwrap_or_else(|| fallback_id.to_string());
    let message = |role, blocks| SessionChatMessage {
        id: id.clone(),
        role,
        blocks,
        timestamp,
        source: SessionChatSource::Transcript,
        turn_id: extract_string(data.get("interactionId")),
        byte_offset: None,
        async_questions: None,
        queued: false,
        api_error: None,
        custom_message: None,
    };
    match kind.as_str() {
        "user.message" => {
            let content = extract_string(data.get("content"))?;
            if copilot_user_message_hidden(&data, &content) {
                return None;
            }
            Some(message(SessionChatRole::User, vec![text_block(content)]))
        }
        "assistant.message" => {
            if from_sub_agent(&data) {
                return None;
            }
            let mut blocks = Vec::new();
            if let Some(text) = extract_string(data.get("content")) {
                blocks.push(text_block(text));
            }
            blocks.extend(copilot_tool_call_blocks(&data));
            if !blocks.is_empty() {
                return Some(message(SessionChatRole::Assistant, blocks));
            }
            // A message that only carries the model's thinking.
            extract_string(data.get("reasoningText"))
                .map(|text| message(SessionChatRole::Reasoning, vec![text_block(text)]))
        }
        "tool.execution_complete" => {
            if from_sub_agent(&data) {
                return None;
            }
            Some(message(
                SessionChatRole::Tool,
                vec![copilot_tool_result(&data)],
            ))
        }
        "abort" => Some(message(
            SessionChatRole::System,
            vec![text_block(INTERRUPTED_STATUS_TEXT)],
        )),
        "session.compaction_complete" => {
            (data.get("success").and_then(Value::as_bool) != Some(false)).then(|| {
                message(
                    SessionChatRole::System,
                    vec![text_block(CONTEXT_COMPACTED_STATUS_TEXT)],
                )
            })
        }
        "session.error" => extract_string(data.get("message"))
            .map(|text| message(SessionChatRole::System, vec![text_block(text)])),
        _ => None,
    }
}

/// A prompt opens the turn; the model's answer without a tool request closes it, and `abort` is a
/// Stop. A message typed while the turn runs (`delivery: "steering"`) joins the running turn.
pub fn decode_copilot_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let (kind, record, data) = copilot_event(line)?;
    let timestamp = parse_timestamp(record.get("timestamp"));
    let turn_id = extract_string(data.get("interactionId"));
    let state = match kind.as_str() {
        "user.message" => {
            let content = extract_string(data.get("content"))?;
            if copilot_user_message_hidden(&data, &content)
                || data.get("delivery").and_then(Value::as_str) == Some("steering")
            {
                return None;
            }
            SessionChatTurnLifecycleState::Working
        }
        "assistant.message" => {
            let asks_for_tools = data
                .get("toolRequests")
                .and_then(Value::as_array)
                .is_some_and(|requests| !requests.is_empty());
            if from_sub_agent(&data) || asks_for_tools || turn_id.is_none() {
                return None;
            }
            SessionChatTurnLifecycleState::Completed
        }
        "abort" => SessionChatTurnLifecycleState::Interrupted,
        _ => return None,
    };
    Some(SessionChatTurnLifecycle {
        state,
        turn_id: turn_id.unwrap_or_else(|| fallback_id.to_string()),
        timestamp,
    })
}
