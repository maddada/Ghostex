//! Option+Up / Option+Down (Alt+Up / Alt+Down on Windows and Linux) move the current line, or the
//! lines a selection touches, up or down by one line in a file open in Files, like VS Code's Move
//! Line Up and Move Line Down.
//!
//! CDXC:Docs 2026-11-09 DECISION:
//! User (9 Nov): "in files please let me hold option/alt and do arrow up / down to move lines up /
//! down (just like in vscode basically". The selection moves with the text and keeps its columns;
//! a selection that ends at column 0 of a line does not drag that line along (VS Code's rule); at
//! the first or last line nothing happens; every press is one undo step; a read-only file is left
//! alone. Neither text editor bound Option+Up or Option+Down before (the Kit's editor binds
//! Cmd+Option+Up for "add cursor above", Zorite's Markdown editor binds neither), so no existing
//! key was replaced. The Files view answers the keys itself (`native_docs_key_down`) and both
//! editors take the move through their own public edit calls.

use std::ops::Range;

use gpui::{Context, KeyDownEvent, Window};
use gpui_component::input::EditorState;

/// Whether `event` is Option+Up (`Some(false)`) or Option+Down (`Some(true)`), Alt on Windows and
/// Linux, with no other modifier.
pub(crate) fn move_lines_key(event: &KeyDownEvent) -> Option<bool> {
    let modifiers = event.keystroke.modifiers;
    if !modifiers.alt || modifiers.control || modifiers.platform || modifiers.shift {
        return None;
    }
    match event.keystroke.key.as_str() {
        "up" => Some(false),
        "down" => Some(true),
        _ => None,
    }
}

/// One move of whole lines: replace `range` of the text with `text`, then select `selection`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LineMove {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
    pub(crate) selection: Range<usize>,
}

/// Where the line holding `offset` starts.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |newline| newline + 1)
}

/// Where the content of the line that starts at `start` ends, and how long its line break is
/// (0 on the last line, 2 for a CRLF).
fn line_end(text: &str, start: usize) -> (usize, usize) {
    match text[start..].find('\n') {
        None => (text.len(), 0),
        Some(found) => {
            let newline = start + found;
            if newline > start && text.as_bytes()[newline - 1] == b'\r' {
                (newline - 1, 2)
            } else {
                (newline, 1)
            }
        }
    }
}

/// The edit that moves the lines `selection` touches one line up (`down` false) or down. `None`
/// at the first line (up) or the last line (down). Offsets are byte offsets on char boundaries.
pub(crate) fn plan_move(text: &str, selection: Range<usize>, down: bool) -> Option<LineMove> {
    let start = selection.start.min(text.len());
    let end = selection.end.clamp(start, text.len());
    let first = line_start(text, start);
    let mut last = line_start(text, end);
    // A non-empty selection that ends at column 0 of a later line leaves that line out.
    if end > start && end == last && last > first {
        last = line_start(text, last - 1);
    }
    let (block_end, eol) = line_end(text, last);
    let block = &text[first..block_end];
    if down {
        if eol == 0 {
            return None;
        }
        let next_start = block_end + eol;
        let (next_end, _) = line_end(text, next_start);
        let shift = next_end - block_end;
        Some(LineMove {
            range: first..next_end,
            text: format!(
                "{}{}{}",
                &text[next_start..next_end],
                &text[block_end..next_start],
                block
            ),
            selection: start + shift..end + shift,
        })
    } else {
        if first == 0 {
            return None;
        }
        let prev_start = line_start(text, first - 1);
        let (prev_end, _) = line_end(text, prev_start);
        let shift = first - prev_start;
        Some(LineMove {
            range: prev_start..block_end,
            text: format!(
                "{}{}{}",
                block,
                &text[prev_end..first],
                &text[prev_start..prev_end]
            ),
            selection: start - shift..end - shift,
        })
    }
}

/// Moves the lines at a Markdown file's selection. One undo step.
pub(crate) fn move_lines_in_live(
    editor: &mut zorite_editor::EditorState,
    down: bool,
    cx: &mut Context<zorite_editor::EditorState>,
) {
    let selection = editor.selected_range();
    let Some(line_move) = plan_move(editor.text(), selection, down) else {
        return;
    };
    editor.replace_range(line_move.range, &line_move.text, cx);
    editor.select_range(line_move.selection, cx);
    // `replace_range` records the edit but emits nothing (the formatting bar and Replace emit it
    // themselves): without it the file stays clean and keeps no draft.
    cx.emit(zorite_editor::EditorEvent::Changed);
}

/// Moves the lines at a code or text file's selection. One undo step. False when the editor is
/// read-only.
pub(crate) fn move_lines_in_code(
    state: &mut EditorState,
    down: bool,
    window: &mut Window,
    cx: &mut Context<EditorState>,
) -> bool {
    if !state.is_editable() {
        return false;
    }
    let reversed = state.is_selection_reversed();
    let Some(line_move) = plan_move(&state.value(), state.selected_range(), down) else {
        return true;
    };
    // Selecting the span and replacing it is one atomic undo step.
    state.set_selected_range(line_move.range, cx);
    state.replace(line_move.text, window, cx);
    state.set_selected_range_with_direction(line_move.selection, reversed, cx);
    true
}
