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
        "collapsedLabel": tool_fold_label(hidden_count),
        "expandedLabel": TOOL_FOLD_EXPANDED_LABEL,
    })
}
