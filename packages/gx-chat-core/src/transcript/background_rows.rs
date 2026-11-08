//! A background command's end, drawn as the tool call it reports on.
//!
//! CDXC:SessionChat 2026-10-25 DECISION: User (Q550, on the "Background command finished" row): "let's please make these not cards, make them regular message instead for agents (look similar to and get treated just like tool call rows EXACTLY)". A harness `<task-notification>` (Claude's own, and the one gxserver writes for Antigravity's background tasks) is projected as one tool call per task: the terminal tile, the summary as its label, a check or a cross for how it ended, the fact the old pill held ("exit code 1") as a failed call's reason, and its details on click. It folds into the turn's work and counts as one of its "N tool calls" (`turns.rs`, `work_summary.rs`), and simple mode groups it like any other call. Turn boundaries are unchanged: a notification that wakes the agent after its reply still ends that reply's turn (the 2026-10-20 DECISION on `wakes_settled_agent`). Supersedes the 2026-10-18 one-line status card for background tasks (desktop `status_rows.rs`, phone `SystemRows.tsx`), which compactions and setting changes keep.

use std::borrow::Cow;
use std::collections::HashMap;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::{json, Value};

use crate::transcript::jsstr::js_trim;
use crate::transcript::noise::{
    classify_suppressed_turn, parse_task_notifications, task_notification_tone, StatusRow,
    SuppressedTurn,
};
use crate::transcript::prose::joined_text;
use crate::transcript::tool_fold::ToolPair;

/// The tool name the rows carry; `tool_glyph` reads "command" as the terminal tile.
pub const BACKGROUND_COMMAND_TOOL: &str = "Background command";

/// The `customType` of the message OMP delivers finished background jobs in
/// (`session/async-job-delivery.ts` in oh-my-pi).
pub const OMP_ASYNC_RESULT: &str = "async-result";

/// One finished background task: how its row reads, what its details show, and whether it was
/// stopped before it finished.
struct BackgroundTask {
    status: StatusRow,
    result: String,
    stopped: bool,
}

/// Whether a call is a background task that was stopped before it finished (an OMP job that ended
/// `cancelled`), which its row and the tool card draw as stopped, neither done nor failed.
///
/// CDXC:SessionChat 2026-10-30 WHY: a cancelled OMP job neither finished nor failed, so a green check would claim it did its work. It takes the tool card's stopped look (`Status::Stopped` in `tool_card.rs`) through a `stopped` flag on its own call, which only `background_task_source` writes; other agents' calls, answered or not, keep their rows.
pub fn is_stopped_task(pair: &ToolPair<'_>) -> bool {
    pair.call_name() == Some(BACKGROUND_COMMAND_TOOL)
        && pair
            .call_input()
            .is_some_and(|input| input["stopped"] == true)
}

/// The tasks a harness `<task-notification>` row reports, or an empty list for any other row.
fn background_task_statuses(message: &ChatMessage) -> Vec<StatusRow> {
    if message.role != ChatRole::User {
        return Vec::new();
    }
    if !matches!(
        classify_suppressed_turn(message),
        Some(SuppressedTurn::Status { .. })
    ) {
        return Vec::new();
    }
    parse_task_notifications(js_trim(&joined_text(&message.blocks)))
}

/// Whether the row reports background tasks ending, so it is drawn and counted as tool calls.
pub fn is_background_task_row(message: &ChatMessage) -> bool {
    let message = notification_part(message);
    omp_job_list(&message).is_some() || !background_task_statuses(&message).is_empty()
}

/// The row as the harness wrote it: without the agent's calls the chat's tool fold put after it
/// (`takes_tool_burst_row`), which are not part of what it reports.
fn notification_part(message: &ChatMessage) -> Cow<'_, ChatMessage> {
    let is_tool = |block: &ChatBlock| {
        matches!(
            block,
            ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
        )
    };
    if !message.blocks.iter().any(is_tool) {
        return Cow::Borrowed(message);
    }
    Cow::Owned(ChatMessage {
        blocks: message
            .blocks
            .iter()
            .filter(|block| !is_tool(block))
            .cloned()
            .collect(),
        ..message.clone()
    })
}

/// The jobs an OMP `async-result` row reports (`details.jobs`), or `None` for any other row.
fn omp_job_list(message: &ChatMessage) -> Option<&Vec<Value>> {
    let custom = message.custom_message.as_ref()?;
    if message.role != ChatRole::System || custom.custom_type != OMP_ASYNC_RESULT {
        return None;
    }
    custom
        .details
        .as_ref()?
        .get("jobs")?
        .as_array()
        .filter(|jobs| !jobs.is_empty())
}

/// What OMP wrote for each job of `ids` (its `prompts/tools/async-result.md`): the
/// `<system-notice>` wrapper and the sentence telling the model to resume are left out, and a
/// delivery of several jobs is split at the `── Job <id>` line OMP writes above each one. Only
/// when those lines name exactly these jobs in this order does each job get its part; otherwise
/// every job's details show the whole text.
fn omp_job_results(text: &str, ids: &[&str]) -> Vec<String> {
    let body = text
        .strip_prefix("<system-notice>")
        .and_then(|inner| inner.strip_suffix("</system-notice>"))
        .map_or(text, js_trim);
    let body = match body.split_once("\n\n") {
        Some((lead, rest)) if lead.contains("Resume your work using the result") => js_trim(rest),
        _ => body,
    };
    if ids.len() == 1 {
        return vec![body.to_string()];
    }
    let mut headers: Vec<&str> = Vec::new();
    let mut parts: Vec<Vec<&str>> = Vec::new();
    for line in body.lines() {
        if let Some(header) = line.strip_prefix("── Job ") {
            headers.push(header);
            parts.push(Vec::new());
        } else if let Some(part) = parts.last_mut() {
            part.push(line);
        }
    }
    // `── Job <id>`, then ` (<label>) ──` or ` ──`: the id must end where the header goes on.
    let names_job = |header: &str, id: &str| {
        header
            .strip_prefix(id)
            .is_some_and(|rest| rest.starts_with(" (") || rest.starts_with(" ──"))
    };
    if headers.len() != ids.len()
        || !headers
            .iter()
            .zip(ids)
            .all(|(header, id)| !id.is_empty() && names_job(header, id))
    {
        return vec![body.to_string(); ids.len()];
    }
    parts
        .iter()
        .map(|lines| js_trim(&lines.join("\n")).to_string())
        .collect()
}

/// The jobs an OMP `async-result` row reports, each as a background task.
///
/// CDXC:SessionChat 2026-10-30 WHY: OMP delivers a finished background job (a command or a subagent task) as an `async-result` custom message instead of a harness notification on a user row, so the chat drew it as one raw muted line holding the whole result. It is the same event the decision above draws as a tool call, so each job in `details.jobs` becomes one: "Background job "<label>" completed" (failed, cancelled), its result on click, the time OMP recorded it ran, and for a failed job the first line OMP delivered for it, which is the job's error, as its reason. A job with no status is "completed", as OMP's own terminal says.
fn omp_jobs(message: &ChatMessage) -> Vec<BackgroundTask> {
    let Some(jobs) = omp_job_list(message) else {
        return Vec::new();
    };
    let ids: Vec<&str> = jobs
        .iter()
        .map(|job| job.get("jobId").and_then(Value::as_str).unwrap_or_default())
        .collect();
    let results = omp_job_results(js_trim(&joined_text(&message.blocks)), &ids);
    jobs.iter()
        .zip(results)
        .map(|(job, result)| {
            let field = |key: &str| {
                job.get(key)
                    .and_then(Value::as_str)
                    .map(js_trim)
                    .filter(|value| !value.is_empty())
            };
            let ended = field("status").unwrap_or("completed");
            let label = match field("label").or_else(|| field("jobId")) {
                Some(name) => format!("Background job \"{name}\" {ended}"),
                None => format!("Background job {ended}"),
            };
            let tone = task_notification_tone(ended);
            let detail = (tone == "error")
                .then(|| result.lines().map(js_trim).find(|line| !line.is_empty()))
                .flatten()
                .map(str::to_string);
            let status = StatusRow {
                label,
                tone,
                detail,
            };
            BackgroundTask {
                result: result_text(&status, Some(&result), None),
                status,
                stopped: ended == "cancelled",
            }
        })
        .collect()
}

/// The tasks the row reports ending, OMP's jobs or a harness notification's, or an empty list.
fn background_tasks(message: &ChatMessage) -> Vec<BackgroundTask> {
    let jobs = omp_jobs(message);
    if !jobs.is_empty() {
        return jobs;
    }
    let statuses = background_task_statuses(message);
    if statuses.is_empty() {
        return Vec::new();
    }
    let raw = js_trim(&joined_text(&message.blocks)).to_string();
    let notifications: Vec<&str> = raw.split("<task-notification>").skip(1).collect();
    let count = statuses.len();
    statuses
        .into_iter()
        .enumerate()
        .map(|(index, status)| {
            // `parse_task_notifications` skips an empty notification, so the two only line up when
            // none was; otherwise the details read the whole row.
            let notification = notifications
                .get(index)
                .filter(|_| notifications.len() == count)
                .copied()
                .unwrap_or(&raw);
            BackgroundTask {
                result: result_text(
                    &status,
                    tag_body(notification, "result"),
                    tag_body(notification, "output-file"),
                ),
                status,
                stopped: false,
            }
        })
        .collect()
}

/// The id of the tool call a background row's `index`th task becomes.
fn task_call_id(message: &ChatMessage, index: usize) -> String {
    format!("{}#background-{index}", message.id)
}

/// When each job an OMP row reports ran, by its call's id, for the ones OMP recorded a duration
/// for (`durationMs`): the row lands when the job ended, so it started that long before. A
/// harness notification records none.
///
/// CDXC:SessionChat 2026-10-30 WHY: read from the delivery's details alone. Building the jobs' rows for it split and copied every job's result a second time each time the row was projected.
pub fn background_task_spans(message: &ChatMessage) -> HashMap<String, (i64, i64)> {
    let (Some(ended), Some(jobs)) = (message.timestamp, omp_job_list(message)) else {
        return HashMap::new();
    };
    jobs.iter()
        .enumerate()
        .filter_map(|(index, job)| {
            let ms = job
                .get("durationMs")
                .and_then(Value::as_i64)
                .filter(|ms| *ms > 0)?;
            Some((task_call_id(message, index), (ended - ms, ended)))
        })
        .collect()
}

/// What a failed task's row says after its label: the exit code the summary names, or "failed".
fn failure_line(status: &StatusRow) -> String {
    if let Some(detail) = &status.detail {
        return detail.clone();
    }
    let lower = status.label.to_ascii_lowercase();
    match lower.find("exit code ") {
        Some(at) => status.label[at..].to_string(),
        None => "failed".to_string(),
    }
}

/// The text between `<tag>` and `</tag>` in one notification, trimmed, or `None`.
fn tag_body<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&format!("</{tag}>"))? + start;
    Some(text[start..end].trim()).filter(|body| !body.is_empty())
}

/// What the row's details show as its result: how it ended, then what the task printed (gxserver
/// writes Antigravity's into `<result>`, OMP its job's output) or the file Claude Code keeps it in.
fn result_text(status: &StatusRow, result: Option<&str>, output_file: Option<&str>) -> String {
    let mut lines = Vec::new();
    if status.tone == "error" {
        lines.push(failure_line(status));
    }
    lines.push(status.label.clone());
    if let Some(result) = result.filter(|result| !result.is_empty()) {
        lines.push(String::new());
        lines.push(result.to_string());
    }
    if let Some(path) = output_file {
        lines.push(String::new());
        lines.push(format!("Output file: {path}"));
    }
    lines.join("\n")
}

/// The row as one tool call and its result per task, under the same id, for every reader that
/// draws or counts tool calls. Any other row comes back as it is.
pub fn background_task_source(message: &ChatMessage) -> Cow<'_, ChatMessage> {
    let tasks = background_tasks(&notification_part(message));
    if tasks.is_empty() {
        return Cow::Borrowed(message);
    }
    let mut blocks = Vec::with_capacity(tasks.len() * 2);
    for (index, task) in tasks.into_iter().enumerate() {
        let call_id = task_call_id(message, index);
        let failed = task.status.tone == "error";
        blocks.push(ChatBlock::ToolCall {
            name: BACKGROUND_COMMAND_TOOL.to_string(),
            input: if task.stopped {
                json!({ "description": task.status.label, "stopped": true })
            } else {
                json!({ "description": task.status.label })
            },
            call_id: Some(call_id.clone()),
        });
        blocks.push(ChatBlock::ToolResult {
            output: task.result,
            is_error: failed.then_some(true),
            call_id: Some(call_id),
            patch: None,
        });
    }
    // The agent's calls right after the task's end folded into this row (`takes_tool_burst_row`):
    // they follow the task's own call on the same card.
    blocks.extend(
        message
            .blocks
            .iter()
            .filter(|block| {
                matches!(
                    block,
                    ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. }
                )
            })
            .cloned(),
    );
    Cow::Owned(ChatMessage {
        role: ChatRole::Assistant,
        blocks,
        ..message.clone()
    })
}
