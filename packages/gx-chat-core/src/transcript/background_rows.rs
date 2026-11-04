//! A background command's end, drawn as the tool call it reports on.
//!
//! CDXC:SessionChat 2026-10-25 DECISION: User (Q550, on the "Background command finished" row): "let's please make these not cards, make them regular message instead for agents (look similar to and get treated just like tool call rows EXACTLY)". A harness `<task-notification>` (Claude's own, and the one gxserver writes for Antigravity's background tasks) is projected as one tool call per task: the terminal tile, the summary as its label, a check or a cross for how it ended, the fact the old pill held ("exit code 1") as a failed call's reason, and its details on click. It folds into the turn's work and counts as one of its "N tool calls" (`turns.rs`, `work_summary.rs`), and simple mode groups it like any other call. Turn boundaries are unchanged: a notification that wakes the agent after its reply still ends that reply's turn (the 2026-10-20 DECISION on `wakes_settled_agent`). Supersedes the 2026-10-18 one-line status card for background tasks (desktop `status_rows.rs`, phone `SystemRows.tsx`), which compactions and setting changes keep.

use std::borrow::Cow;

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};
use serde_json::json;

use crate::transcript::jsstr::js_trim;
use crate::transcript::noise::{
    classify_suppressed_turn, parse_task_notifications, StatusRow, SuppressedTurn,
};
use crate::transcript::prose::joined_text;

/// The tool name the rows carry; `tool_glyph` reads "command" as the terminal tile.
pub const BACKGROUND_COMMAND_TOOL: &str = "Background command";

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
    !background_task_statuses(message).is_empty()
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
/// writes Antigravity's into `<result>`) or the file Claude Code keeps it in.
fn result_text(status: &StatusRow, notification: &str) -> String {
    let mut lines = Vec::new();
    if status.tone == "error" {
        lines.push(failure_line(status));
    }
    lines.push(status.label.clone());
    if let Some(result) = tag_body(notification, "result") {
        lines.push(String::new());
        lines.push(result.to_string());
    }
    if let Some(path) = tag_body(notification, "output-file") {
        lines.push(String::new());
        lines.push(format!("Output file: {path}"));
    }
    lines.join("\n")
}

/// The row as one tool call and its result per task, under the same id, for every reader that
/// draws or counts tool calls. Any other row comes back as it is.
pub fn background_task_source(message: &ChatMessage) -> Cow<'_, ChatMessage> {
    let statuses = background_task_statuses(message);
    if statuses.is_empty() {
        return Cow::Borrowed(message);
    }
    let raw = js_trim(&joined_text(&message.blocks)).to_string();
    let notifications: Vec<&str> = raw.split("<task-notification>").skip(1).collect();
    let mut blocks = Vec::with_capacity(statuses.len() * 2);
    for (index, status) in statuses.iter().enumerate() {
        let call_id = format!("{}#background-{index}", message.id);
        let failed = status.tone == "error";
        blocks.push(ChatBlock::ToolCall {
            name: BACKGROUND_COMMAND_TOOL.to_string(),
            input: json!({ "description": status.label }),
            call_id: Some(call_id.clone()),
        });
        // `parse_task_notifications` skips an empty notification, so the two only line up when
        // none was; otherwise the details read the whole row.
        let notification = notifications
            .get(index)
            .filter(|_| notifications.len() == statuses.len())
            .copied()
            .unwrap_or(&raw);
        blocks.push(ChatBlock::ToolResult {
            output: result_text(status, notification),
            is_error: failed.then_some(true),
            call_id: Some(call_id),
            patch: None,
        });
    }
    Cow::Owned(ChatMessage {
        role: ChatRole::Assistant,
        blocks,
        ..message.clone()
    })
}
