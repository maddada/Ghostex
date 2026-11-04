/*
CDXC:AgentProviders 2026-09-03:
Antigravity CLI (`agy`) keeps its conversation store encrypted
(`~/.gemini/antigravity-cli/conversations/<id>.pb|.db`, 8.0 bits/byte), but it
also appends a plaintext step log per conversation that its own checkpoint
summaries point the model at:

    ~/.gemini/antigravity-cli/brain/<conversationId>/.system_generated/logs/
        transcript_full.jsonl   # 1.1.24+: real tool-arg values
        transcript.jsonl        # every build: arg values JSON-stringified

Its hooks name that file (`transcriptPath`) and the conversation
(`conversationId`) on every event, which the hook receiver already stores as
`agentSessionPath` / `agentSessionId`. Each line is ONE trajectory step:

    {"step_index": 7, "source": "MODEL", "type": "PLANNER_RESPONSE",
     "status": "DONE", "created_at": "2026-09-02T19:36:02Z",
     "content": "pong", "thinking": "**…**", "tool_calls": [{"name": …, "args": {…}}]}

    USER_EXPLICIT / USER_INPUT       the prompt, wrapped in <USER_REQUEST> plus
                                     harness metadata tags
    MODEL / PLANNER_RESPONSE         any of thinking, content, tool_calls in ONE step
    MODEL / <tool type> or GENERIC   the tool's result text (RUN_COMMAND,
                                     VIEW_FILE, … before 1.1.24; GENERIC after)
    SYSTEM / CONVERSATION_HISTORY,
    SYSTEM / CHECKPOINT              harness bookkeeping, never shown

A planner step carries the model's thinking AND its answer or tool calls in one
record, while the chat contract decodes one message per line, so the thinking
would vanish. This module mirrors the raw log into a Ghostex-owned jsonl in
the gxserver state dir where every step becomes one or two rows, tool results
lose their `Created At:` headers, and the older build's stringified args are
parsed back.

The thinking becomes a reasoning row of its own (the 2026-10-25 DECISION on
`mirror_rows_for_step`). The mirror is what the chat pipeline tails; the same
freshen-before-read and rename-on-rewrite contract as the Cursor and Hermes
mirrors applies, so `session_chat_decode_antigravity.rs` stays line-local.
*/

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::hash::Hasher;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Map, Value};

use crate::resume_lookup::home_dir;
use crate::session_chat::{extract_string, parse_json_object};

const ANTIGRAVITY_SESSION_ID_MAX_LENGTH: usize = 128;
pub(crate) const ANTIGRAVITY_TRANSCRIPT_FILE_NAMES: [&str; 2] =
    ["transcript_full.jsonl", "transcript.jsonl"];

pub(crate) fn is_safe_antigravity_session_id(session_id: &str) -> bool {
    !session_id.is_empty()
        && session_id.len() <= ANTIGRAVITY_SESSION_ID_MAX_LENGTH
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// `~/.gemini/antigravity-cli`, the CLI's app data directory (its own log
/// calls it `appDataDir`; the skills resolver already assumes the same root).
pub(crate) fn antigravity_app_data_dir() -> PathBuf {
    home_dir().join(".gemini").join("antigravity-cli")
}

fn antigravity_mirror_dir() -> PathBuf {
    ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("antigravity-chat-mirror")
}

/// The raw step log for a conversation, preferring the `_full` variant whose
/// tool args are real JSON values.
pub(crate) fn find_antigravity_transcript(session_id: &str) -> Option<PathBuf> {
    if !is_safe_antigravity_session_id(session_id) {
        return None;
    }
    let logs_dir = antigravity_app_data_dir()
        .join("brain")
        .join(session_id)
        .join(".system_generated")
        .join("logs");
    ANTIGRAVITY_TRANSCRIPT_FILE_NAMES
        .iter()
        .map(|name| logs_dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The conversation id a hook-supplied raw path belongs to:
/// `…/brain/<id>/.system_generated/logs/<file>`.
fn session_id_from_raw_path(path: &Path) -> Option<String> {
    let mut ancestors = path.ancestors();
    ancestors.next()?; // the file itself
    let logs = ancestors.next()?;
    let system_generated = ancestors.next()?;
    if logs.file_name().and_then(|name| name.to_str()) != Some("logs")
        || system_generated.file_name().and_then(|name| name.to_str()) != Some(".system_generated")
    {
        return None;
    }
    let id = ancestors
        .next()?
        .file_name()
        .and_then(|name| name.to_str())?
        .to_string();
    is_safe_antigravity_session_id(&id).then_some(id)
}

/* ------------------------------------------------------------ step rows */

const USER_REQUEST_OPEN: &str = "<USER_REQUEST>";
const USER_REQUEST_CLOSE: &str = "</USER_REQUEST>";

/// The prompt the user typed, without the harness's metadata envelope. A
/// record with no envelope is taken whole.
pub(crate) fn antigravity_user_text(content: &str) -> String {
    let Some(start) = content.find(USER_REQUEST_OPEN) else {
        return content.trim().to_string();
    };
    let body = &content[start + USER_REQUEST_OPEN.len()..];
    let body = match body.find(USER_REQUEST_CLOSE) {
        Some(end) => &body[..end],
        None => body,
    };
    body.trim().to_string()
}

/// Tool results open with two timestamp lines the TUI never shows.
fn antigravity_tool_output(content: &str) -> String {
    let mut lines = content.lines().peekable();
    while lines
        .peek()
        .is_some_and(|line| line.starts_with("Created At:") || line.starts_with("Completed At:"))
    {
        lines.next();
    }
    let rest: Vec<&str> = lines.collect();
    // Write and edit results end their first line with an instruction to the model that the
    // TUI never shows.
    let joined = rest
        .join("\n")
        .replace(
            " If relevant, proactively run terminal commands to execute this code for the USER. Don't ask for permission.",
            "",
        )
        .replace(
            "\nIf relevant, proactively run terminal commands to execute this code for the USER. Don't ask for permission.",
            "",
        );
    // The result body is indented with a run of tabs on some tools; fold
    // that indentation away without touching the text itself.
    joined
        .lines()
        .map(|line| line.strip_prefix("\t\t\t\t").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// `The command exited with code N.` with a non-zero N is the only structured
/// failure signal the result text carries.
///
/// Only a line that starts with the sentence counts: a file read that happens to contain it
/// (an older agy transcript, a build log) is not a failed call.
fn antigravity_tool_output_is_error(output: &str) -> bool {
    output.lines().any(|line| {
        ["The command exited with code ", "Command exited with code "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
            .is_some_and(|rest| {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                digits.parse::<u32>().is_ok_and(|code| code != 0)
            })
    })
}

/// `transcript.jsonl` (and every pre-1.1.24 log) stores each arg value as its
/// JSON text (`"\"echo pong\""`, `"2000"`); give the UI the value back.
fn normalize_tool_args(args: &Value) -> Value {
    let Value::Object(record) = args else {
        return args.clone();
    };
    let mut normalized = Map::with_capacity(record.len());
    for (key, value) in record {
        let restored = match value {
            Value::String(text) => serde_json::from_str::<Value>(text)
                .ok()
                .filter(|parsed| !matches!(parsed, Value::Object(_) | Value::Array(_)))
                .unwrap_or_else(|| value.clone()),
            other => other.clone(),
        };
        normalized.insert(key.clone(), restored);
    }
    Value::Object(normalized)
}

fn antigravity_tool_calls(record: &Map<String, Value>) -> Vec<Value> {
    let Some(Value::Array(tool_calls)) = record.get("tool_calls") else {
        return Vec::new();
    };
    tool_calls
        .iter()
        .filter_map(|tool_call| {
            let tool_call = tool_call.as_object()?;
            let name = extract_string(tool_call.get("name")).unwrap_or_else(|| "tool".to_string());
            let args = tool_call
                .get("args")
                .map(normalize_tool_args)
                .unwrap_or(Value::Null);
            Some(json!({ "name": name, "args": args }))
        })
        .collect()
}

/* ------------------------------------------------------------ harness envelopes */

/// CDXC:SessionChat 2026-10-25 WHY:
/// agy hands the model harness notes wrapped in `<SYSTEM_MESSAGE>` (a background task's result, a restart notice) or `<EPHEMERAL_MESSAGE>` (reminders), each led by a sentence saying the user did not send it. Alone, such a note is a `SYSTEM` step the mirror never shows, but agy can merge it into another step's text (its `disable-system-message-merging` flag turns that off), and a user then read the raw tags and a task's output in the chat as if the agent had written them. Every text the mirror writes loses these envelopes, and the background-task ends they carry become rows of their own (`BackgroundTasks`). Only the two tags agy is known to write are matched.
const HARNESS_ENVELOPES: [(&str, &str); 2] = [
    (
        "SYSTEM_MESSAGE",
        "The following is a <SYSTEM_MESSAGE> not actually sent by the user. It is provided by the system as important information to pay attention to.",
    ),
    (
        "EPHEMERAL_MESSAGE",
        "The following is an <EPHEMERAL_MESSAGE> not actually sent by the user. It is provided by the system as a set of reminders and general important information to pay attention to. Do NOT respond to this message, just act accordingly.",
    ),
];

/// `text` without agy's harness envelopes, and the `<SYSTEM_MESSAGE>` bodies it carried. An
/// envelope the step cut off runs to the end of the text.
fn split_harness_envelopes(text: &str) -> (String, Vec<String>) {
    if !text.contains("_MESSAGE>") {
        return (text.to_string(), Vec::new());
    }
    let mut rest = text.to_string();
    let mut system_messages = Vec::new();
    for (tag, preamble) in HARNESS_ENVELOPES {
        rest = rest.replace(preamble, "");
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let Some(start) = rest.find(&open) {
            let body_start = start + open.len();
            let (body_end, end) = match rest[body_start..].find(&close) {
                Some(at) => (body_start + at, body_start + at + close.len()),
                None => (rest.len(), rest.len()),
            };
            if tag == "SYSTEM_MESSAGE" {
                system_messages.push(rest[body_start..body_end].to_string());
            }
            rest.replace_range(start..end, "");
        }
    }
    (rest.trim().to_string(), system_messages)
}

/// A text field of a step without agy's harness envelopes. What an envelope leaves behind with no
/// word in it is dropped: Gemini sometimes echoes the system message it was given as its reply,
/// and once the echo is gone a stray `}` was all that remained.
fn step_text(record: &Map<String, Value>, key: &str) -> Option<String> {
    let text = extract_string(record.get(key))?;
    let (rest, bodies) = split_harness_envelopes(&text);
    let emptied = (!bodies.is_empty() || rest != text) && !rest.chars().any(char::is_alphanumeric);
    (!emptied && !rest.is_empty()).then_some(rest)
}

/* ------------------------------------------------------------ background tasks */

const BACKGROUND_TASK_LAUNCH: &str = "Tool is running as a background task with task id: ";
const BACKGROUND_TASK_DESCRIPTION: &str = "Task Description: ";
const BACKGROUND_TASK_END: &str = "Task id \"";
const BACKGROUND_TASK_RESULT: &str = "finished with result:";
const BACKGROUND_TASKS_STOPPED: &str = "background tasks have been stopped";
const BACKGROUND_TASK_SENDER: &str = "sender=";

/// One background task's end, as agy's system message reports it:
/// `… content=Task id "<id>" finished with result: <output>`.
struct TaskEnd {
    id: String,
    result: String,
}

fn task_ends(system_message: &str) -> Vec<TaskEnd> {
    let starts: Vec<usize> = system_message
        .match_indices(BACKGROUND_TASK_END)
        .map(|(start, _)| start)
        .collect();
    starts
        .iter()
        .enumerate()
        .filter_map(|(index, start)| {
            let rest = &system_message[start + BACKGROUND_TASK_END.len()..];
            let id_end = rest.find('"')?;
            let until = starts
                .get(index + 1)
                .map_or(system_message.len(), |next| *next);
            let section = &system_message[*start..until];
            let result = section
                .find(BACKGROUND_TASK_RESULT)
                .map(|at| {
                    section[at + BACKGROUND_TASK_RESULT.len()..]
                        .trim()
                        .to_string()
                })
                .unwrap_or_default();
            Some(TaskEnd {
                id: rest[..id_end].to_string(),
                result,
            })
        })
        .collect()
}

/// The exit code a command's result names (`The command exited with code 1.`, `Command exited with code 0.`).
fn result_exit_code(result: &str) -> Option<i64> {
    result.lines().find_map(|line| {
        let rest = ["The command exited with code ", "Command exited with code "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))?;
        let digits: String = rest
            .chars()
            .enumerate()
            .take_while(|(at, ch)| ch.is_ascii_digit() || (*at == 0 && *ch == '-'))
            .map(|(_, ch)| ch)
            .collect();
        digits.parse().ok()
    })
}

/// A background task's end in the `<task-notification>` shape Claude Code writes, which the chat
/// draws as the call it reports on (gx-chat-core `transcript/background_rows.rs`).
/// SEE-ALSO: packages/gx-chat-core/src/transcript/noise.rs (`parse_task_notifications` reads `status` and `summary`).
fn task_notification_text(id: &str, description: Option<&str>, result: &str) -> String {
    let exit_code = result_exit_code(result);
    let failed = exit_code.is_some_and(|code| code != 0);
    let command = match description {
        Some(description) => format!("Background command \"{description}\""),
        None => "Background command".to_string(),
    };
    let summary = match exit_code {
        Some(code) if failed => format!("{command} failed with exit code {code}"),
        Some(code) => format!("{command} completed (exit code {code})"),
        None => format!("{command} completed"),
    };
    let status = if failed { "failed" } else { "completed" };
    format!(
        "<task-notification>\n<task-id>{id}</task-id>\n<status>{status}</status>\n<summary>{summary}</summary>\n<result>\n{result}\n</result>\n</task-notification>"
    )
}

/// CDXC:SessionChat 2026-10-23 WHY:
/// A command that outlives its `WaitMsBeforeAsync` becomes a background task, and agy tells the model to "update the user with a short message (that you have launched the command and will wait for it to finish) and end the turn"; when the task ends, agy injects a `SYSTEM_MESSAGE` naming it (`Task id "<id>" finished with result: …`) and runs the model again. That "will wait" reply is prose without tool calls, the shape the lifecycle decoder reads as the turn's end, so the chat folded the turn into "Worked" seconds in and the real answer arrived minutes later outside it (GitHub #271). The mirror follows the turn's launched tasks and marks prose written while one is still running `awaitsBackgroundTasks`, which the decoder does not read as an end. A new prompt starts the count again, so a task agy never reports back on holds at most one turn open. 2026-10-25: a task's end is read from any step that carries agy's system message, not only from a `SYSTEM` step, because agy can merge the message into another step; and each end becomes a `task` row the chat draws as a "Background command" call (Q550).
/// SEE-ALSO: server/src/agent_hooks/event_mapping.rs (`antigravity_stop_awaits_background_work`, the same wait seen from agy's Stop hook).
#[derive(Default)]
struct BackgroundTasks {
    running: Vec<String>,
    /// What each launched task runs, by id, for the row that reports its end.
    descriptions: HashMap<String, String>,
    /// The newest planner step's calls still waiting for their result, oldest first: a
    /// `run_command`'s own one-line summary, which names the task it may launch.
    pending_calls: VecDeque<Option<String>>,
    reported: HashSet<String>,
}

impl BackgroundTasks {
    /// The rows for the background tasks whose end `record` carries, drawn before the step's own
    /// rows. A step can report several, and a `SYSTEM` step is nothing else. The model's own text
    /// is never read for ends: what it holds is an echo of a message agy logs as its own step.
    fn end_rows(&mut self, record: &Map<String, Value>) -> Vec<Value> {
        let source = record.get("source").and_then(Value::as_str);
        if source == Some("MODEL")
            && record.get("type").and_then(Value::as_str) == Some("PLANNER_RESPONSE")
        {
            return Vec::new();
        }
        let mut system_messages = Vec::new();
        for key in ["content", "thinking"] {
            let Some(text) = extract_string(record.get(key)) else {
                continue;
            };
            let (rest, bodies) = split_harness_envelopes(&text);
            system_messages.extend(bodies);
            // A `SYSTEM` step without the envelope is the message itself.
            if source == Some("SYSTEM") && !rest.is_empty() {
                system_messages.push(rest);
            }
        }
        let mut rows = Vec::new();
        for message in system_messages {
            if message.contains(BACKGROUND_TASKS_STOPPED) {
                self.running.clear();
            }
            // A `schedule` timer is a background task too; it ends by sending its prompt as a
            // message of its own (`sender=<task id> … content=<prompt>`), never as a finished task.
            if let Some(sender) = message
                .split_whitespace()
                .find_map(|word| word.strip_prefix(BACKGROUND_TASK_SENDER))
            {
                self.running.retain(|running| running != sender);
            }
            for end in task_ends(&message) {
                self.running.retain(|running| *running != end.id);
                if !self.reported.insert(end.id.clone()) {
                    continue;
                }
                let text = task_notification_text(
                    &end.id,
                    self.descriptions.get(&end.id).map(String::as_str),
                    &end.result,
                );
                rows.push((text, rows.len()));
            }
        }
        rows.into_iter()
            .filter_map(|(text, index)| {
                step_row(
                    record,
                    "task",
                    Map::from_iter([
                        ("text".into(), Value::from(text)),
                        ("taskIndex".into(), Value::from(index)),
                    ]),
                )
            })
            .collect()
    }

    /// Follows the turn's prompts, its calls and the tasks they launch.
    fn observe(&mut self, record: &Map<String, Value>) {
        let source = record.get("source").and_then(Value::as_str);
        let step_type = record.get("type").and_then(Value::as_str);
        match (source, step_type) {
            (Some("USER_EXPLICIT"), Some("USER_INPUT")) => {
                self.running.clear();
                self.pending_calls.clear();
            }
            (Some("MODEL"), Some("PLANNER_RESPONSE")) => {
                self.pending_calls = antigravity_tool_calls(record)
                    .iter()
                    .map(|call| {
                        (call["name"] == "run_command").then(|| {
                            let args = &call["args"];
                            extract_string(args.get("toolSummary"))
                                .or_else(|| extract_string(args.get("CommandLine")))
                        })?
                    })
                    .collect();
            }
            (Some("MODEL"), _) => {
                let call = self.pending_calls.pop_front().flatten();
                // A launch is a call still running; a finished read that shows the same sentence
                // (an older agy transcript) launched nothing.
                if record.get("status").and_then(Value::as_str) != Some("RUNNING") {
                    return;
                }
                let Some(content) = record.get("content").and_then(Value::as_str) else {
                    return;
                };
                let Some(start) = content.find(BACKGROUND_TASK_LAUNCH) else {
                    return;
                };
                let id: String = content[start + BACKGROUND_TASK_LAUNCH.len()..]
                    .chars()
                    .take_while(|ch| !ch.is_whitespace())
                    .collect();
                if id.is_empty() {
                    return;
                }
                let description = call.or_else(|| {
                    content.lines().find_map(|line| {
                        line.strip_prefix(BACKGROUND_TASK_DESCRIPTION)
                            .map(|description| description.trim().to_string())
                            .filter(|description| !description.is_empty())
                    })
                });
                if let Some(description) = description {
                    self.descriptions.insert(id.clone(), description);
                }
                if !self.running.contains(&id) {
                    self.running.push(id);
                }
            }
            _ => {}
        }
    }

    fn any_running(&self) -> bool {
        !self.running.is_empty()
    }
}

/// One mirror row for a step: its index, part and time, plus `extra`.
fn step_row(record: &Map<String, Value>, part: &str, extra: Map<String, Value>) -> Option<Value> {
    let step_index = record.get("step_index").and_then(Value::as_u64)?;
    let mut row = Map::new();
    row.insert("stepIndex".into(), Value::from(step_index));
    row.insert("part".into(), Value::from(part));
    row.insert(
        "createdAt".into(),
        record.get("created_at").cloned().unwrap_or(Value::Null),
    );
    row.extend(extra);
    Some(Value::Object(row))
}

/// Mirror rows for one raw step, in display order.
///
/// CDXC:SessionChat 2026-10-25 DECISION: User, reporting a regression in agy's chat ("I no longer have a little chat bubble that says how many tool calls, and I can click on it to see those calls. Those are now hidden behind all the thinking"), picked Q549 A: agy's thinking is a thinking row like every other agent's, drawn as the slim "Thought" line with the calls it made listed below it, and opening on click. Supersedes the 2026-09-03 choice to mirror it as the assistant's own text (`narration: true`), which the chat drew as a heading that hid the calls after it.
fn mirror_rows_for_step(record: &Map<String, Value>, awaits_background_tasks: bool) -> Vec<Value> {
    let source = record
        .get("source")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let step_type = record
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let row = |part: &str, extra: Map<String, Value>| step_row(record, part, extra);
    match (source, step_type) {
        ("USER_EXPLICIT", "USER_INPUT") => {
            let text = step_text(record, "content").map(|content| antigravity_user_text(&content));
            text.filter(|text| !text.is_empty())
                .and_then(|text| row("user", Map::from_iter([("text".into(), Value::from(text))])))
                .into_iter()
                .collect()
        }
        ("MODEL", "PLANNER_RESPONSE") => {
            let mut rows = Vec::new();
            if let Some(thinking) =
                step_text(record, "thinking").filter(|text| !text.trim().is_empty())
            {
                rows.extend(row(
                    "reasoning",
                    Map::from_iter([("text".into(), Value::from(thinking))]),
                ));
            }
            let tool_calls = antigravity_tool_calls(record);
            let text = step_text(record, "content").filter(|text| !text.trim().is_empty());
            if text.is_some() || !tool_calls.is_empty() {
                let mut extra = Map::new();
                if let Some(text) = text {
                    extra.insert("text".into(), Value::from(text));
                }
                if !tool_calls.is_empty() {
                    extra.insert("toolCalls".into(), Value::Array(tool_calls));
                } else if awaits_background_tasks {
                    extra.insert("awaitsBackgroundTasks".into(), Value::Bool(true));
                }
                rows.extend(row("assistant", extra));
            }
            rows
        }
        ("MODEL", tool_type) => {
            let Some(content) = step_text(record, "content") else {
                return Vec::new();
            };
            let output = antigravity_tool_output(&content);
            let mut extra = Map::from_iter([
                ("toolType".into(), Value::from(tool_type)),
                ("text".into(), Value::from(output.clone())),
            ]);
            // agy 1.4.7 marks a call that could not run (`file does not exist`) with an ERROR
            // status and an `error` field.
            let step_failed = record.get("status").and_then(Value::as_str) == Some("ERROR")
                || record.get("error").is_some_and(|error| !error.is_null());
            if step_failed || antigravity_tool_output_is_error(&output) {
                extra.insert("isError".into(), Value::Bool(true));
            }
            row("tool", extra).into_iter().collect()
        }
        _ => Vec::new(),
    }
}

/// The whole mirror for a raw log. Only complete lines are read: the CLI
/// appends a step in one write, but a torn tail would otherwise be mirrored
/// as a parse failure and never revisited.
fn build_mirror(raw: &[u8]) -> Vec<u8> {
    let complete = match raw.iter().rposition(|byte| *byte == b'\n') {
        Some(end) => &raw[..=end],
        None => &[][..],
    };
    let text = String::from_utf8_lossy(complete);
    let mut out = Vec::with_capacity(complete.len());
    let mut background_tasks = BackgroundTasks::default();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(record) = parse_json_object(line) else {
            continue;
        };
        // A task that ended in this step no longer holds the step's own prose open.
        let mut rows = background_tasks.end_rows(&record);
        rows.extend(mirror_rows_for_step(
            &record,
            background_tasks.any_running(),
        ));
        background_tasks.observe(&record);
        for row in rows {
            if let Ok(serialized) = serde_json::to_vec(&row) {
                out.extend_from_slice(&serialized);
                out.push(b'\n');
            }
        }
    }
    out
}

/* ------------------------------------------------------------ sync */

#[derive(Default)]
struct AntigravityMirrorState {
    raw_path: Option<PathBuf>,
    raw_len: u64,
    raw_modified: Option<std::time::SystemTime>,
    output_len: usize,
    output_hash: u64,
}

static MIRROR_STATES: Mutex<Option<HashMap<String, AntigravityMirrorState>>> = Mutex::new(None);

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// Append when the new output extends the old byte-for-byte; otherwise write
/// a temp file and rename it over the mirror so the follower's inode check
/// reports `content_replaced` instead of reading a half-written file.
fn write_mirror(
    mirror_path: &Path,
    output: &[u8],
    state: &AntigravityMirrorState,
    mirror_exists: bool,
) -> Option<()> {
    fs::create_dir_all(mirror_path.parent()?).ok()?;
    let pure_append = mirror_exists
        && state.output_len > 0
        && output.len() >= state.output_len
        && content_hash(&output[..state.output_len]) == state.output_hash;
    if pure_append {
        if output.len() > state.output_len {
            let mut file = fs::OpenOptions::new().append(true).open(mirror_path).ok()?;
            file.write_all(&output[state.output_len..]).ok()?;
        }
        return Some(());
    }
    let temp_path = mirror_path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&temp_path).ok()?;
    file.write_all(output).ok()?;
    file.flush().ok()?;
    drop(file);
    fs::rename(&temp_path, mirror_path).ok()
}

/// One sync pass for one conversation. `raw_hint` is the hook-supplied log
/// path, used when it exists so the mirror follows the file the CLI named.
fn sync_antigravity_transcript_mirror(
    session_id: &str,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    if !is_safe_antigravity_session_id(session_id) {
        return None;
    }
    let mirror_path = antigravity_mirror_dir().join(format!("{session_id}.jsonl"));

    let mut states_guard = MIRROR_STATES.lock().ok()?;
    let states = states_guard.get_or_insert_with(HashMap::new);
    let state = states.entry(session_id.to_string()).or_default();
    let raw_path = raw_hint
        .filter(|path| path.is_file())
        .map(Path::to_path_buf)
        .or_else(|| state.raw_path.clone().filter(|path| path.is_file()))
        .or_else(|| find_antigravity_transcript(session_id))?;
    state.raw_path = Some(raw_path.clone());
    let raw_meta = fs::metadata(&raw_path).ok()?;
    let raw_len = raw_meta.len();
    let raw_modified = raw_meta.modified().ok();
    // A state only counts while the file it describes is still there; a wiped
    // state dir or fresh daemon always rebuilds.
    let mirror_exists = mirror_path.is_file();
    if !mirror_exists {
        state.output_len = 0;
        state.output_hash = 0;
    }
    let up_to_date = mirror_exists
        && state.output_len > 0
        && state.raw_len == raw_len
        && state.raw_modified == raw_modified;
    if up_to_date {
        return Some(mirror_path);
    }
    let raw = fs::read(&raw_path).ok()?;
    let output = build_mirror(&raw);
    write_mirror(&mirror_path, &output, state, mirror_exists)?;
    state.raw_len = raw_len;
    state.raw_modified = raw_modified;
    state.output_len = output.len();
    state.output_hash = content_hash(&output);
    Some(mirror_path)
}

/// Path-resolution entry: sync, then hand back the mirror as "the transcript".
pub fn resolve_antigravity_chat_transcript_path(
    session_id: Option<&str>,
    raw_hint: Option<&Path>,
) -> Option<PathBuf> {
    let session_id = session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| raw_hint.and_then(session_id_from_raw_path))?;
    sync_antigravity_transcript_mirror(&session_id, raw_hint)
}

/// Steady-state entry for the follower's drain tick, which holds only the
/// already-resolved path. The stem is the conversation id by construction.
pub(crate) fn sync_antigravity_transcript_mirror_for_path(mirror_path: &Path) {
    if let Some(session_id) = mirror_path.file_stem().and_then(|stem| stem.to_str()) {
        sync_antigravity_transcript_mirror(session_id, None);
    }
}
