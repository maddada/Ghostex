//! The per-message rows the GPUI transcript paints: tool runs, their fold, file change cards, and
//! the pending terminal tool card.
//!
//! Ported from `packages/shared/session-chat-controller/native-transcript-rows.ts`. Every
//! classification here comes from the same presentation rules React read, so GPUI only lays the
//! rows out (`apps/desktop/src/app/native_chat/tool_run.rs`, `file_change_card.rs`,
//! `terminal_tool_row.rs`).

use serde_json::{json, Map, Value};

use crate::transcript::file_change_rows::{
    file_change_counts, file_change_expandable, file_change_path_parts,
};
use crate::transcript::file_changes::FileChange;
use crate::transcript::question_exchange::answered_question_exchange;
use crate::transcript::subagent::{is_subagent_self, tool_subagent};
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_rows::{
    clip_tool_body, is_command_tool, tool_glyph, tool_preview, tool_run_fold,
};
use crate::transcript::tool_summary::{command_detail, format_tool_input};

/// A tool's arguments and result as its open row shows them.
pub fn tool_detail(pair: &ToolPair<'_>) -> Value {
    let input = match pair.call_input() {
        Some(input) => pair
            .call_name()
            .filter(|name| is_command_tool(name))
            .and_then(|_| command_detail(input))
            .unwrap_or_else(|| format_tool_input(input)),
        None => String::new(),
    };
    json!({
        "input": clip_tool_body(&input),
        "output": clip_tool_body(pair.result_output().unwrap_or_default()),
    })
}

/// One row per tool pair.
///
/// The arguments and result stay behind: GPUI asks for them only while the row is open
/// (`row_details.rs`).
pub fn tool_rows(
    pairs: &[ToolPair<'_>],
    agent_path: &str,
    working_directory: Option<&str>,
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
            row.insert(
                "name".to_string(),
                pair.call_name().unwrap_or("Result").into(),
            );
            row.insert(
                "glyph".to_string(),
                tool_glyph(pair.call_name().unwrap_or_default()).into(),
            );
            row.insert(
                "preview".to_string(),
                tool_preview(pair, working_directory).into(),
            );
            row.insert("failed".to_string(), pair.result_is_error().into());
            row.insert("hasDetail".to_string(), has_detail.into());
            row.insert("subagent".to_string(), subagent);
            Value::Object(row)
        })
        .collect()
}

pub fn tool_fold(pairs: &[ToolPair<'_>]) -> Value {
    let exchanges: Vec<bool> = pairs
        .iter()
        .map(|pair| answered_question_exchange(pair).is_some())
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
                // GPUI owns the "previews enabled" setting, so it only needs the half of the rule
                // it cannot see.
                "expandableWithPreviews": file_change_expandable(counts, true, failed),
            })
        })
        .collect()
}
