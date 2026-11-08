//! The rules a run of tool rows renders by: which glyph a row carries, what its one-line preview
//! says, and which rows survive the "+N previous tool calls" fold.
//!
//! Ported from `packages/shared/session-chat-presentation/tool-rows.ts`.
//!
//! CDXC:SessionChat 2026-09-18 SEE-ALSO:
//! `apps/desktop/src/app/native_chat/tool_run.rs` reads these off the wire through the projection.
//! A rule for picking, previewing, or folding rows belongs here, never in a renderer.

use serde_json::{json, Value};

use crate::transcript::jsstr::{ascii_lower, js_trim, utf16_len, utf16_take};
use crate::transcript::tool_fold::ToolPair;
use crate::transcript::tool_summary::{
    summarize_command_input, summarize_primary_argument, summarize_tool_input,
    truncate_tool_preview,
};

/// The first line of a result is the preview when the call has nothing to say.
pub const TOOL_RESULT_PREVIEW_LENGTH: usize = 120;

/// How much of an expanded tool body either renderer paints before it stops.
pub const MAX_TOOL_RESULT_CHARS: usize = 4000;

pub fn clip_tool_body(text: &str) -> String {
    if utf16_len(text) > MAX_TOOL_RESULT_CHARS {
        format!("{}\u{2026}", utf16_take(text, MAX_TOOL_RESULT_CHARS))
    } else {
        text.to_string()
    }
}

/// The one glyph on this surface that says WHAT ran rather than "this expands".
///
/// The renderers map these names onto their own icon sets (the bundled titlebar SVGs in GPUI);
/// the classification itself lives here.
pub fn tool_glyph(name: &str) -> &'static str {
    let normalized = ascii_lower(name);
    let has = |needles: &[&str]| needles.iter().any(|needle| normalized.contains(needle));
    if has(&["edit", "write", "patch", "replace"]) {
        return "edit";
    }
    if has(&["read", "file", "glob", "list"]) {
        return "file";
    }
    if has(&["exec", "command", "shell", "terminal", "bash"]) {
        return "terminal";
    }
    if has(&["web", "search", "browser", "fetch", "url"]) {
        return "web";
    }
    "tool"
}

pub fn is_command_tool(name: &str) -> bool {
    let normalized = ascii_lower(name);
    ["exec", "command", "shell", "terminal", "bash"]
        .iter()
        .any(|needle| normalized.contains(needle))
}

/// The compact text beside a tool row's name.
pub fn tool_preview(pair: &ToolPair<'_>, working_directory: Option<&str>) -> String {
    if let Some(name) = pair.call_name() {
        if is_command_tool(name) {
            return summarize_command_input(pair.call_input().unwrap_or(&Value::Null));
        }
    }
    let input = match pair.call_input() {
        Some(input) => summarize_primary_argument(input, working_directory)
            .unwrap_or_else(|| summarize_tool_input(input)),
        None => String::new(),
    };
    if !input.is_empty() {
        return input;
    }
    let first_line = js_trim(
        pair.result_output()
            .unwrap_or_default()
            .split('\n')
            .next()
            .unwrap_or_default(),
    );
    truncate_tool_preview(first_line, TOOL_RESULT_PREVIEW_LENGTH)
}

/// True when a message's tool run is nested inside a disclosure that already owns collapsing it.
///
/// The reasoning row and the assistant commentary heading both open onto their run, so neither the
/// "+N previous tool calls" fold nor simple mode's "N tool calls" group may appear a second time
/// inside them. A run only stands alone when the message has no prose to open it.
pub fn tool_run_shows_all_rows(has_prose: bool) -> bool {
    has_prose
}

/// Marks a row drawn inside a turn's work fold ("Worked for 45s", "Previous turn", summary mode's
/// "Active work") so the fold reads as one flat, chronological list.
///
/// CDXC:SessionChat 2026-10-21 DECISION: User, opening "Worked for 45s" onto a collapsed "8 tool calls": "please remove this unnecessary middle one (8 tool calls) and just show me the list of tool calls there", then, seeing the agent's sentence between two runs fold the calls after it: "in this specific case those 2 last tool calls shouldn't be under that collapse area". Inside an opened turn fold nothing collapses a second time: a tool-only row adds neither simple mode's "N tool calls" group nor the "+N previous tool calls" fold (`toolsShowAllRows`), and the agent's in-between sentence or thought is plain text with its calls listed after it at the same level, no chevron (`toolsFlat`; a thought keeps its own thinking fold for its text only). Each call keeps its own row, duration, status and details. Outside turn folds the 2026-09-13 heading that owns its calls stays. The renderers read both keys (desktop `transcript.rs` and `tool_run.rs`, phone `MessageRow.tsx` and `ToolRows.tsx`), and Find does too (`extras/transcript_search.rs`).
pub fn list_tools_in_work_fold(row: &mut Value) {
    if let Value::Object(fields) = row {
        fields.insert("toolsShowAllRows".to_string(), Value::Bool(true));
        fields.insert("toolsFlat".to_string(), Value::Bool(true));
    }
}

/// What a model's two work-fold keys held before [`mark_work_fold`] set them.
#[derive(Clone, Debug, PartialEq)]
pub struct WorkFoldMarks {
    shows_all_rows: Option<Value>,
    flat: Option<Value>,
}

/// [`list_tools_in_work_fold`], remembering what it replaced, so [`unmark_work_fold`] can hand the
/// model back exactly as it was (`state::ProjectedMessage::in_work_fold`).
pub fn mark_work_fold(row: &mut Value) -> WorkFoldMarks {
    let marks = WorkFoldMarks {
        shows_all_rows: row.get("toolsShowAllRows").cloned(),
        flat: row.get("toolsFlat").cloned(),
    };
    list_tools_in_work_fold(row);
    marks
}

/// Takes [`mark_work_fold`]'s marks off again: a key it replaced gets its value back in its place,
/// and a key it added goes without moving the others.
pub fn unmark_work_fold(row: &mut Value, marks: WorkFoldMarks) {
    let Value::Object(fields) = row else {
        return;
    };
    for (key, before) in [
        ("toolsFlat", marks.flat),
        ("toolsShowAllRows", marks.shows_all_rows),
    ] {
        match before {
            Some(value) => {
                fields.insert(key.to_string(), value);
            }
            None => {
                fields.shift_remove(key);
            }
        }
    }
}

pub const TOOL_FOLD_EXPANDED_LABEL: &str = "Show fewer tool calls";

pub fn tool_fold_label(hidden_count: usize) -> String {
    format!(
        "+{hidden_count} previous tool {}",
        if hidden_count == 1 { "call" } else { "calls" }
    )
}

/// An answered question is conversation, not work: its card never folds behind the toggle. The fold
/// hides only the ordinary tool rows before the last pair. `exchanges[index]` is true for a pair
/// that renders as a question exchange.
///
/// CDXC:SessionChat 2026-10-22 DECISION: User, seeing "+4 previous tool calls" drawn under the call it ran before: "please in this case show the 'previous tool calls' above the 'Confirm folder contents...' tool call. the previous tool calls thing should always be before the last tool call not below it". The toggle sits right before the last call (`toggleBefore`, that row's index), closed or open, so the calls it reveals appear above it in the order they ran and the last call stays below. The renderers place it there (desktop `tool_run.rs`, phone `ToolRows.tsx`).
///
/// CDXC:SessionChat 2026-11-08 DECISION: User, with the detailed view's earlier call opened: "it's showing that prev tool call above the collapse header (Show fewer tool calls) ... we should show Show fewer tool calls above all other tool calls sir, not to be in between them". Closed, the toggle stays right before the last call; open, it moves to the top of the group (`toggleBeforeOpen`, 0) and every call, the revealed earlier ones then the always-visible last ones, follows below it in the order they ran. The revealed rows grow below the toggle, so it never moves under the pointer. [`tool_fold_toggle_before`] picks the slot for either state.
pub fn tool_run_fold(exchanges: &[bool]) -> Value {
    let visible: Vec<bool> = exchanges
        .iter()
        .enumerate()
        .map(|(index, exchange)| index + 1 == exchanges.len() || *exchange)
        .collect();
    let hidden_count = visible.iter().filter(|shown| !**shown).count();
    json!({
        "visible": visible,
        "hiddenCount": hidden_count,
        "toggleBefore": exchanges.len().saturating_sub(1),
        "toggleBeforeOpen": 0,
        "collapsedLabel": tool_fold_label(hidden_count),
        "expandedLabel": TOOL_FOLD_EXPANDED_LABEL,
    })
}

/// The row index the fold toggle is drawn before: the last call while the run is closed, the first
/// row while it is open, so the toggle never sits between the calls it reveals.
pub fn tool_fold_toggle_before(fold: &Value, open: bool) -> usize {
    let key = if open {
        "toggleBeforeOpen"
    } else {
        "toggleBefore"
    };
    fold[key]
        .as_u64()
        .map_or(if open { 0 } else { usize::MAX }, |index| index as usize)
}
