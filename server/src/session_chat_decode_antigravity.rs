/*
Antigravity CLI's chat is read through the Ghostex-owned mirror that
`session_chat_antigravity_mirror.rs` derives from the CLI's per-conversation
step log. One mirror row per rendered message, in this shape:

    {"stepIndex": 7, "part": "reasoning", "createdAt": "2026-…", "text": "…"}
    {"stepIndex": 7, "part": "assistant", "createdAt": "…", "text": "pong",
     "toolCalls": [{"name": "run_command", "args": {"CommandLine": "echo pong"}}]}
    {"stepIndex": 27, "part": "assistant", "createdAt": "…", "text": "…will wait…",
     "awaitsBackgroundTasks": true}
    {"stepIndex": 0, "part": "user", "createdAt": "…", "text": "…"}
    {"stepIndex": 34, "part": "task", "taskIndex": 0, "createdAt": "…",
     "text": "<task-notification>…</task-notification>"}
    {"stepIndex": 2, "part": "tool", "createdAt": "…", "toolType": "GENERIC",
     "text": "The command exited with code 0.\nOutput:\npong", "isError": true}

`stepIndex` is the CLI's own trajectory index, so ids are stable across mirror
rewrites; a planner step's reasoning row (its thinking) and message row share
it and differ by their part, and a step that reports background tasks ending
gets one `task` row per task before its own rows, numbered by `taskIndex`.
*/

use serde_json::{Map, Value};

use crate::session_chat::*;

fn antigravity_row_id(record: &Map<String, Value>, part: &str, fallback_id: &str) -> String {
    match record.get("stepIndex").and_then(Value::as_u64) {
        Some(step_index) if part == "reasoning" => {
            format!("antigravity-step-{step_index}-reasoning")
        }
        Some(step_index) if part == "task" => {
            let task_index = record.get("taskIndex").and_then(Value::as_u64).unwrap_or(0);
            format!("antigravity-step-{step_index}-task-{task_index}")
        }
        Some(step_index) => format!("antigravity-step-{step_index}"),
        None => fallback_id.to_string(),
    }
}

/*
CDXC:SessionChat 2026-10-01 WHY: agy's tools name their arguments in PascalCase (`CommandLine`,
`AbsolutePath`, `TargetFile`, `TargetContent`/`ReplacementContent`), which none of the chat's
tool rules read, so a command row showed raw JSON and an edit drew no diff. The calls are
decoded here into the names agy's own terminal prints for them (`Bash(…)`, `Read(…)`,
`Edit(…)`, `WebSearch(…)`) with the argument names the chat already renders, once for every
client. A tool with no counterpart keeps its name, and agy's one-line `toolSummary` becomes its
`description` so the row previews it instead of the argument JSON. 2026-10-25: agy 1.4.7 renamed
its file tools (`read_file`, `edit_file` and `write_file`, each naming its file `Path`, with
`Target`/`Replacement` for an edit and `Content` for a write) and added `manage_task` (a
background task's status) and `schedule` (a timer that wakes the agent), seen in a reporter's
log; the older names stay for logs written by earlier builds.
*/
fn antigravity_canonical_tool_call(name: &str, args: &Value) -> (String, Value) {
    let Some(record) = args.as_object() else {
        return (name.to_string(), args.clone());
    };
    let text = |key: &str| record.get(key).filter(|value| !value.is_null()).cloned();
    let object = |entries: Vec<(&str, Option<Value>)>| {
        Value::Object(
            entries
                .into_iter()
                .filter_map(|(key, value)| value.map(|value| (key.to_string(), value)))
                .collect(),
        )
    };
    let edit = |chunk: &Map<String, Value>| {
        object(vec![
            ("old_string", chunk.get("TargetContent").cloned()),
            ("new_string", chunk.get("ReplacementContent").cloned()),
        ])
    };
    let canonical = match name {
        "run_command" => Some((
            "Bash",
            object(vec![
                ("command", text("CommandLine")),
                ("cwd", text("Cwd")),
                ("description", text("toolSummary")),
            ]),
        )),
        "view_file" | "read_file" => {
            let start = record.get("StartLine").and_then(Value::as_u64);
            let end = record.get("EndLine").and_then(Value::as_u64);
            Some((
                "Read",
                object(vec![
                    ("file_path", text("AbsolutePath").or_else(|| text("Path"))),
                    ("offset", start.map(Value::from)),
                    (
                        "limit",
                        start
                            .zip(end)
                            .filter(|(start, end)| end >= start)
                            .map(|(start, end)| Value::from(end - start + 1)),
                    ),
                ]),
            ))
        }
        "write_to_file" => Some((
            "Write",
            object(vec![
                ("file_path", text("TargetFile")),
                ("content", text("CodeContent")),
            ]),
        )),
        "write_file" => Some((
            "Write",
            object(vec![
                ("file_path", text("Path")),
                ("content", text("Content")),
            ]),
        )),
        "edit_file" => Some((
            "Edit",
            object(vec![
                ("file_path", text("Path")),
                ("old_string", text("Target")),
                ("new_string", text("Replacement")),
            ]),
        )),
        "manage_task" => {
            let task = record
                .get("TaskId")
                .and_then(Value::as_str)
                .map(|id| id.rsplit('/').next().unwrap_or(id).to_string());
            let action = record.get("Action").and_then(Value::as_str);
            let description = match (action, task) {
                (Some("status"), Some(task)) => format!("Check background {task}"),
                (Some(action), Some(task)) => format!("{action} background {task}"),
                (_, Some(task)) => format!("Background {task}"),
                _ => "Background task".to_string(),
            };
            let mut input = record.clone();
            input.insert("description".into(), Value::from(description));
            Some(("manage_task", Value::Object(input)))
        }
        "schedule" => {
            let seconds = record.get("DurationSeconds").and_then(Value::as_u64);
            let prompt = record.get("Prompt").and_then(Value::as_str);
            let description = match (seconds, prompt) {
                (Some(seconds), Some(prompt)) => format!("Wait {seconds}s, then: {prompt}"),
                (Some(seconds), None) => format!("Wait {seconds}s"),
                (None, Some(prompt)) => format!("Timer: {prompt}"),
                (None, None) => "Timer".to_string(),
            };
            let mut input = record.clone();
            input.insert("description".into(), Value::from(description));
            Some(("schedule", Value::Object(input)))
        }
        "replace_file_content" => {
            let mut input = edit(record);
            if let (Value::Object(input), Some(path)) = (&mut input, text("TargetFile")) {
                input.insert("file_path".into(), path);
            }
            Some(("Edit", input))
        }
        "multi_replace_file_content" => Some((
            "MultiEdit",
            object(vec![
                ("file_path", text("TargetFile")),
                (
                    "edits",
                    record
                        .get("ReplacementChunks")
                        .and_then(Value::as_array)
                        .map(|chunks| {
                            Value::Array(
                                chunks
                                    .iter()
                                    .filter_map(Value::as_object)
                                    .map(edit)
                                    .collect(),
                            )
                        }),
                ),
            ]),
        )),
        "list_dir" => Some(("LS", object(vec![("path", text("DirectoryPath"))]))),
        "find_by_name" => Some((
            "Glob",
            object(vec![
                ("pattern", text("Pattern")),
                ("path", text("SearchDirectory")),
            ]),
        )),
        "grep_search" => Some((
            "Grep",
            object(vec![
                ("pattern", text("Query")),
                ("path", text("SearchPath")),
            ]),
        )),
        "search_web" => Some(("WebSearch", object(vec![("query", text("query"))]))),
        "read_url_content" => Some(("WebFetch", object(vec![("url", text("Url"))]))),
        _ => None,
    };
    if let Some((canonical_name, input)) = canonical {
        return (canonical_name.to_string(), input);
    }
    let mut input = record.clone();
    input.remove("toolAction");
    if let Some(summary) = input.remove("toolSummary") {
        input.entry("description").or_insert(summary);
    }
    (name.to_string(), Value::Object(input))
}

fn antigravity_tool_call_blocks(record: &Map<String, Value>) -> Vec<SessionChatBlock> {
    let Some(Value::Array(tool_calls)) = record.get("toolCalls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            let name = extract_string(tool_call.get("name")).unwrap_or_else(|| "tool".to_string());
            let args = tool_call.get("args").cloned().unwrap_or(Value::Null);
            let (name, input) = antigravity_canonical_tool_call(&name, &args);
            Some(SessionChatBlock::ToolCall {
                name,
                input,
                call_id: None,
            })
        })
        .collect()
}

pub fn decode_antigravity_transcript_line(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatMessage> {
    let record = parse_json_object(line)?;
    let part = record.get("part").and_then(Value::as_str)?;
    let id = antigravity_row_id(&record, part, fallback_id);
    let timestamp = parse_timestamp(record.get("createdAt"));
    let text = extract_string(record.get("text"));
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
    };

    match part {
        // A background task's end, which the chat draws as the call it reports on.
        "user" | "task" => Some(message(SessionChatRole::User, vec![text_block(text?)])),
        "reasoning" => Some(message(SessionChatRole::Reasoning, vec![text_block(text?)])),
        "assistant" => {
            let mut blocks = Vec::new();
            if let Some(text) = text {
                blocks.push(text_block(text));
            }
            blocks.extend(antigravity_tool_call_blocks(&record));
            if blocks.is_empty() {
                return None;
            }
            Some(message(SessionChatRole::Assistant, blocks))
        }
        "tool" => {
            let is_error = (record.get("isError") == Some(&Value::Bool(true))).then_some(true);
            Some(message(
                SessionChatRole::Tool,
                vec![SessionChatBlock::ToolResult {
                    output: text.unwrap_or_default(),
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
The step log writes no explicit turn markers: a user row opens a turn, and a
planner step that answers in prose WITHOUT asking for tools is the turn's last
step (a step with tool calls is always followed by the tool's result and
another planner step), unless the mirror marked it `awaitsBackgroundTasks`.
Interrupts write no closing row, so ready-state recovery for them rides the
agent-hook activity signal like Pi's.
*/
pub fn decode_antigravity_turn_lifecycle(
    line: &str,
    fallback_id: &str,
) -> Option<SessionChatTurnLifecycle> {
    let record = parse_json_object(line)?;
    let part = record.get("part").and_then(Value::as_str)?;
    let turn_id = antigravity_row_id(&record, part, fallback_id);
    let timestamp = parse_timestamp(record.get("createdAt"));
    match part {
        "user" => Some(SessionChatTurnLifecycle {
            state: SessionChatTurnLifecycleState::Working,
            turn_id,
            timestamp,
        }),
        "assistant" => {
            let asks_for_tools = record
                .get("toolCalls")
                .and_then(Value::as_array)
                .is_some_and(|tool_calls| !tool_calls.is_empty());
            // A reply written while a background task runs is not the answer: agy runs the
            // model again when the task ends (CDXC:SessionChat 2026-10-23 in the mirror module).
            let awaits_background_tasks =
                record.get("awaitsBackgroundTasks") == Some(&Value::Bool(true));
            if asks_for_tools || awaits_background_tasks {
                return None;
            }
            Some(SessionChatTurnLifecycle {
                state: SessionChatTurnLifecycleState::Completed,
                turn_id,
                timestamp,
            })
        }
        _ => None,
    }
}
