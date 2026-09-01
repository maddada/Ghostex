/*
Cursor keeps its ordinary half-block composer frame on screen while `/model`
owns input. The row inside the frame becomes a filter, and the picker paints
its controls below the closing rule:

    ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
      → /model GPT-5.6 Terra
    ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀
     Models matching "GPT-5.6 Terra"                 Max mode: ON
      → GPT-5.6 Terra  272K Medium (Tab to modify)
     Edit prompt to filter • Enter to select • Tab to edit

The nested parameter editor uses the same retained frame and ends with its own
navigation row. Frame-only composer detection therefore cannot distinguish
either picker from an ordinary message box.

This classifier anchors every decision after the LATEST complete half-block
frame, so an old picker in scrollback is retired as soon as Cursor paints a new
composer. It intentionally ignores the text after `→`: Cursor changes that
placeholder between turns and accepts arbitrary user input there. The stable
evidence is the picker-owned control grammar below the frame.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const CURSOR_BLOCKING_SCAN_LINES: usize = 120;
const MIN_FRAME_CHARS: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CursorBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

fn scan_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= CURSOR_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn is_half_block_rule(line: &str, frame: char) -> bool {
    let mut frame_chars = 0usize;
    let mut non_space = 0usize;
    for character in line.chars().filter(|character| !character.is_whitespace()) {
        non_space += 1;
        if character == frame {
            frame_chars += 1;
        }
    }
    non_space >= MIN_FRAME_CHARS && frame_chars * 10 >= non_space * 9
}

fn is_cursor_input_row(line: &str) -> bool {
    line.trim_start().starts_with('\u{2192}')
}

fn latest_frame_foot(lines: &[String]) -> Option<usize> {
    (0..lines.len().saturating_sub(2))
        .rev()
        .find(|index| {
            is_half_block_rule(&lines[*index], '\u{2584}')
                && is_cursor_input_row(&lines[*index + 1])
                && is_half_block_rule(&lines[*index + 2], '\u{2580}')
        })
        .map(|index| index + 2)
}

fn has_model_picker_controls(lines: &[String]) -> bool {
    lines.iter().any(|line| {
        let line = line.trim();
        line.contains("Edit prompt to filter")
            && line.contains("Enter to select")
            && line.contains("Tab to edit")
    })
}

fn has_parameter_editor_controls(lines: &[String]) -> bool {
    let has_editor_title = lines
        .iter()
        .any(|line| line.contains("\u{2014} Edit Parameters"));
    let has_sections = lines.iter().any(|line| line.trim() == "Context")
        && lines.iter().any(|line| line.trim() == "Reasoning");
    let has_navigation = lines.iter().any(|line| {
        let line = line.trim();
        line.contains("to navigate")
            && line.contains("Enter to select")
            && line.contains("Esc to go back")
    });
    has_editor_title && has_sections && has_navigation
}

pub fn detect_cursor_blocking_screen(text: &str) -> Option<CursorBlockingScreen> {
    let lines = scan_lines(text);
    let frame_foot = latest_frame_foot(&lines)?;
    let owned_tail = &lines[frame_foot + 1..];
    if has_model_picker_controls(owned_tail) {
        return Some(CursorBlockingScreen {
            title: "Cursor is waiting for a model selection",
            detail: "Cursor's model picker owns the input field. Select a model or close the picker in the terminal before sending another message.",
        });
    }
    has_parameter_editor_controls(owned_tail).then_some(CursorBlockingScreen {
        title: "Cursor is waiting for model parameters",
        detail: "Cursor's context, reasoning effort, and Fast settings own the input field. Finish or close the parameter editor in the terminal before sending another message.",
    })
}
