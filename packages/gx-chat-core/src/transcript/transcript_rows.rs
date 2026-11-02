//! The per-message rows the GPUI transcript paints: tool runs, their fold, file change cards, and
//! the pending terminal tool card.
//!
//! Ported from `packages/shared/session-chat-controller/native-transcript-rows.ts`. Every
//! classification here comes from the same presentation rules React read, so GPUI only lays the
//! rows out (`apps/desktop/src/app/native_chat/tool_run.rs`, `file_change_card.rs`,
//! `terminal_tool_row.rs`).

use std::collections::HashMap;

use ghostex_gx_protocol::ChatBlock;
use serde_json::{json, Map, Value};

use crate::transcript::file_change_rows::{
    file_change_bars, file_change_counts, file_change_expandable, file_change_path_parts,
    file_change_show_all_label,
};
use crate::transcript::file_changes::FileChange;
use crate::transcript::question_exchange::answered_question_exchange;
use crate::transcript::sent_message::sent_agent_messages;
use crate::transcript::subagent::{is_subagent_self, tool_subagent};
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_label::tool_label;
use crate::transcript::tool_rows::{
    clip_tool_body, is_command_tool, tool_glyph, tool_preview, tool_run_fold,
};
use crate::transcript::tool_summary::{command_detail, format_tool_input, tool_description};
use crate::transcript::work_summary::{failure_note, tool_duration_label};

/// The arguments an open tool row shows: a command tool's command line, or the input as text.
fn tool_input_text(pair: &ToolPair<'_>) -> String {
    match pair.call_input() {
        Some(input) => pair
            .call_name()
            .filter(|name| is_command_tool(name))
            .and_then(|_| command_detail(input))
            .unwrap_or_else(|| format_tool_input(input)),
        None => String::new(),
    }
}

/// A tool's arguments and result as its open row shows them.
pub fn tool_detail(pair: &ToolPair<'_>) -> Value {
    json!({
        "input": clip_tool_body(&tool_input_text(pair)),
        "output": clip_tool_body(pair.result_output().unwrap_or_default()),
    })
}

/// What an open tool row's copy button puts on the clipboard: the tool's name, then its command
/// or input and its result under the labels the row draws, each in full rather than clipped to
/// [`crate::transcript::tool_rows::MAX_TOOL_RESULT_CHARS`] the way the row shows them.
///
/// CDXC:SessionChat 2026-10-05 DECISION: "need a button on the top right I can click to copy all in the tool call result and the command etc". Every open tool row (desktop and phone) has a copy button that copies this text.
pub fn tool_copy_text(pair: &ToolPair<'_>) -> String {
    let name = pair.call_name().unwrap_or("Result");
    let input = tool_input_text(pair);
    let output = pair.result_output().unwrap_or_default();
    let mut text = name.to_string();
    if !input.is_empty() {
        let label = if tool_glyph(name) == "terminal" {
            "Command"
        } else {
            "Input"
        };
        text.push_str(&format!("\n\n{label}:\n{input}"));
    }
    if !output.is_empty() {
        let label = if pair.call.is_some() {
            "Result"
        } else {
            "Output"
        };
        text.push_str(&format!("\n\n{label}:\n{output}"));
    }
    text
}

/// [`tool_detail`] for a row the renderer draws open, with the full text its copy button copies.
pub fn open_tool_detail(pair: &ToolPair<'_>) -> Value {
    let mut detail = tool_detail(pair);
    detail["copyText"] = tool_copy_text(pair).into();
    detail
}

/// One row per tool pair.
///
/// The arguments and result stay behind: GPUI asks for them only while the row is open
/// (`row_details.rs`). `durations` is how long each call ran, by call id (`work_summary.rs`).
pub fn tool_rows(
    pairs: &[ToolPair<'_>],
    agent_path: &str,
    working_directory: Option<&str>,
    durations: &HashMap<String, i64>,
) -> Vec<Value> {
    pairs
        .iter()
        .map(|pair| {
            let detail = tool_detail(pair);
            let has_detail = !detail["input"].as_str().unwrap_or_default().is_empty()
                || !detail["output"].as_str().unwrap_or_default().is_empty();
            let subagent = match tool_subagent(pair, agent_path) {
                Some(Value::Object(mut target)) => {
                    // `self` is a selector pointing back at the conversation being read: the GPUI
                    // heading chip renders it as plain text rather than a link, as React did.
                    let selector = target
                        .get("selector")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    let is_self = is_subagent_self(selector, agent_path);
                    target.insert("self".to_string(), is_self.into());
                    Value::Object(target)
                }
                // `subagent && {…}` keeps the falsy value, which is `null` here.
                _ => Value::Null,
            };
            let mut row = Map::new();
            row.insert("hasCall".to_string(), pair.call.is_some().into());
            // An answered question is conversation, not work: a standalone run renders the pair as
            // the exchange card instead of a tool row, so the raw `AskUserQuestion` row never shows
            // above it. Inside a disclosure or a turn's work fold the card is hoisted out and the
            // row does stay.
            row.insert(
                "exchange".to_string(),
                answered_question_exchange(pair).is_some().into(),
            );
            // A message sent to another agent is conversation too, drawn as its own card on the
            // same terms as an answered question.
            row.insert(
                "sentMessage".to_string(),
                (!sent_agent_messages(pair, &[]).is_empty()).into(),
            );
            row.insert(
                "name".to_string(),
                pair.call_name().unwrap_or("Result").into(),
            );
            row.insert(
                "preview".to_string(),
                tool_preview(pair, working_directory).into(),
            );
            // A call the agent described is named by that description in every mode, and its
            // command preview gives way to it (`tool_description`). A call it did not describe is
            // named by what it did (`tool_label`), drawn the same way; the call's own name is the
            // label only when neither reads.
            let input = pair.call_input().unwrap_or(&Value::Null);
            let mut description = tool_description(input);
            let mut glyph = tool_glyph(pair.call_name().unwrap_or_default());
            if description.is_empty() {
                if let Some(label) = pair.call_name().and_then(|name| {
                    tool_label(
                        name,
                        input,
                        working_directory,
                        pair.result.is_some() && !pair.result_is_error(),
                    )
                }) {
                    description = label.text;
                    glyph = label.glyph.unwrap_or(glyph);
                }
            }
            row.insert("glyph".to_string(), glyph.into());
            row.insert(
                "label".to_string(),
                if description.is_empty() {
                    pair.call_name().unwrap_or("Result").to_string()
                } else {
                    description.clone()
                }
                .into(),
            );
            row.insert("description".to_string(), description.into());
            // The file the call names, for the row's file menu (Copy Path and the folder row,
            // CDXC:SessionChat 2026-10-17 in apps/desktop/src/app/native_chat/file_actions.rs).
            if let Some(path) = crate::transcript::tool_summary::tool_file_path(
                pair.call_input().unwrap_or(&Value::Null),
            ) {
                row.insert("filePath".to_string(), path.into());
            }
            // A picture file the call opened shows on its row (`read_images.rs`).
            let images = crate::transcript::read_images::tool_read_images(pair, working_directory);
            if !images.is_empty() {
                row.insert("images".to_string(), Value::Array(images));
            }
            row.insert("failed".to_string(), pair.result_is_error().into());
            // The row's right edge: a check once the call answered, a cross when it failed, and
            // nothing while it runs; then what failed, and how long the call took.
            row.insert(
                "status".to_string(),
                match (pair.result.is_some(), pair.result_is_error()) {
                    (_, true) => "failed",
                    (true, false) => "done",
                    (false, false) => "running",
                }
                .into(),
            );
            row.insert(
                "failure".to_string(),
                if pair.result_is_error() {
                    failure_note(pair.call_name(), pair.result_output().unwrap_or_default())
                } else {
                    String::new()
                }
                .into(),
            );
            let duration = match pair.call {
                Some(ChatBlock::ToolCall {
                    call_id: Some(id), ..
                }) => durations.get(id).map(|ms| tool_duration_label(*ms)),
                _ => None,
            };
            row.insert("duration".to_string(), duration.unwrap_or_default().into());
            row.insert("hasDetail".to_string(), has_detail.into());
            row.insert("subagent".to_string(), subagent);
            // A Workflow call is drawn as its run's card (`workflow_card.rs`) wherever the
            // document has one for this id; the row stays only inside a turn's work fold.
            if let Some(call_id) = crate::transcript::workflow_card::workflow_call_id(pair) {
                row.insert("workflow".to_string(), call_id.into());
            }
            Value::Object(row)
        })
        .collect()
}

pub fn tool_fold(pairs: &[ToolPair<'_>]) -> Value {
    let exchanges: Vec<bool> = pairs
        .iter()
        .map(|pair| {
            answered_question_exchange(pair).is_some()
                || !sent_agent_messages(pair, &[]).is_empty()
                || crate::transcript::workflow_card::workflow_call_id(pair).is_some()
        })
        .collect();
    tool_run_fold(&exchanges)
}

/// One card per file a turn wrote.
///
/// Only the projected card ships: the raw result block would ship the whole write
/// output again.
pub fn file_rows(
    changes: &[FileChange<'_>],
    message_id: &str,
    working_directory: Option<&str>,
) -> Vec<Value> {
    changes
        .iter()
        .enumerate()
        .map(|(index, change)| {
            let counts = file_change_counts(&change.lines);
            let failed = change.result_is_error();
            // The same opt-out React's card took: the renderer shortens the folder half against
            // the row's real width, so a character budget on top of that only cuts folders that
            // would have fitted.
            let parts = file_change_path_parts(&change.path, working_directory, None);
            json!({
                "path": change.path,
                "action": change.action,
                // The diff stays behind: GPUI asks for it by these two only while the card shows it.
                "messageId": message_id,
                "index": index,
                "displayPath": parts.display_path,
                "parent": parts.parent,
                "filename": parts.filename,
                "added": counts.added,
                "removed": counts.removed,
                "codeLines": counts.code,
                "failed": failed,
                "error": if failed { change.result_output() } else { "" },
                "status": change.status,
                "bars": file_change_bars(counts.added, counts.removed),
                // The preview already shows everything a short change has; otherwise the row
                // offers this under it.
                "hasMore": file_change_expandable(counts, true, failed),
                "showAllLabel": file_change_show_all_label(counts.added + counts.removed),
            })
        })
        .collect()
}
