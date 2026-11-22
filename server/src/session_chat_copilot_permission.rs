/*
CDXC:AgentScreenDetection 2026-10-17 WHY:
Copilot CLI's tool approval panel, read off the screen (measured against Copilot 1.0.98 in
Manual Approval mode on a zmx pty):

    ╭──────────────────────────────────────────╮
    │ Edit file                                │
    │ ──────────────────────────────────────── │
    │ copilot_flow.txt +1 -0                   │
    │ …the diff…                               │
    │ Do you want to edit /path/copilot_flow.txt? │
    │                                          │
    │ ❯ 1. Yes                                 │
    │   2. Yes, and don't ask again for file operations when in this repo (…) │
    │   3. No, and tell Copilot what to do differently (Esc to stop) │
    │                                          │
    │ ↑/↓ to navigate · enter to select · esc to cancel │
    ╰──────────────────────────────────────────╯

The command panel ("Run command", "Do you want to run this command?") and the path panel ("Allow
path access", "Do you want to allow this?", "3. No (Esc)") are the same component. Copilot's
PermissionRequest hook fires for auto-approved tools too and its Notification carries only a
one-line message, so the panel is the source of the approval card. It becomes the same
`permissionPrompt` notice Claude's dialog does. Keys, measured: a Yes row's digit picks and
commits it at once when the highlight is elsewhere, and Enter commits the highlighted row; the
No row commits on neither (its digit only moves the highlight), so it is answered with Escape,
which the row itself names. Liveness is the key hint as the panel's last row, directly above
its bottom border.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_resume_prompt::{
    parse_picker_row, SessionChatTerminalPicker, SessionChatTerminalPickerKind,
    SessionChatTerminalPickerRow,
};

const NAVIGATE_HINT: &str = "↑/↓ to navigate";
const SELECT_HINT: &str = "enter to select";
/// The panel's title, its diff or command, the question and the rows.
const SCAN_LINES: usize = 80;
/// The hint is the panel's last row; only its bottom border and the composer below may follow.
const TAIL_LINES: usize = 8;
const DETAIL_MAX_CHARS: usize = 400;
const REQUEST_MAX_LINES: usize = 12;
const KEY_ENTER: &str = "\r";

/// A line without the panel's borders, and without a quoted command's own box inside it.
fn inner(line: &str) -> &str {
    const BORDERS: &[char] = &['\u{2502}', '|'];
    let mut text = line.trim();
    loop {
        let stripped = text
            .trim_start_matches(BORDERS)
            .trim_end_matches(BORDERS)
            .trim();
        if stripped.len() == text.len() {
            return stripped;
        }
        text = stripped;
    }
}

fn is_rule(line: &str) -> bool {
    let line = inner(line);
    !line.is_empty() && line.chars().all(|ch| ch == '─')
}

fn is_top_border(line: &str) -> bool {
    line.trim_start().starts_with('╭')
}

/// The live approval panel, or `None` when the screen shows none.
pub fn detect_copilot_permission_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    let mut window: Vec<String> = text
        .lines()
        .rev()
        .map(|raw| {
            normalize_spaces(&strip_ansi_sgr(raw))
                .trim_end()
                .to_string()
        })
        // The panel's blank rows are bare borders (`│ … │`).
        .filter(|line| !inner(line).is_empty())
        .take(SCAN_LINES)
        .collect();
    window.reverse();
    let hint = window
        .iter()
        .rposition(|line| line.contains(NAVIGATE_HINT) && line.contains(SELECT_HINT))?;
    if window.len() - hint > TAIL_LINES
        || !window
            .get(hint + 1)
            .is_some_and(|line| line.trim_start().starts_with('╰'))
    {
        return None;
    }
    let first_row = (0..hint).rev().find(|&index| {
        parse_picker_row(&window[index]).is_some_and(|(_, number, _)| number == 1)
    })?;
    let heading = inner(window.get(first_row.checked_sub(1)?)?).to_string();
    if !heading.ends_with('?') {
        return None;
    }
    let mut rows: Vec<SessionChatTerminalPickerRow> = Vec::new();
    for line in &window[first_row..hint] {
        match parse_picker_row(line) {
            Some((selected, number, label)) => rows.push(SessionChatTerminalPickerRow {
                number,
                label,
                selected,
            }),
            None => {
                let row = rows.last_mut()?;
                row.label.push(' ');
                row.label.push_str(inner(line));
            }
        }
    }
    if !rows.iter().any(|row| row.label.starts_with("Yes"))
        || !rows.iter().any(|row| row.label.starts_with("No"))
    {
        return None;
    }
    let selected_index = rows.iter().position(|row| row.selected)?;
    let top = (0..first_row)
        .rev()
        .find(|&index| is_top_border(&window[index]))?;
    let title = window.get(top + 1).map(|line| inner(line).to_string());
    // What is asked about sits between the title's rule and the question.
    let request_start = (top + 1..first_row - 1)
        .find(|&index| is_rule(&window[index]))
        .map_or(first_row - 1, |index| index + 1);
    let request = window[request_start..first_row - 1]
        .iter()
        .map(|line| inner(line))
        .filter(|line| !line.is_empty() && !line.chars().all(|ch| "╭╮╰╯─".contains(ch)))
        .take(REQUEST_MAX_LINES)
        .collect::<Vec<_>>()
        .join(" ");
    let mut detail = [title.unwrap_or_default(), request, heading]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    if detail.chars().count() > DETAIL_MAX_CHARS {
        detail = format!(
            "{}…",
            detail
                .chars()
                .take(DETAIL_MAX_CHARS)
                .collect::<String>()
                .trim_end()
        );
    }
    Some(SessionChatTerminalPicker {
        kind: SessionChatTerminalPickerKind::CopilotPermission,
        detail: Some(detail),
        rows,
        selected_index,
    })
}

/// The keys that answer row `target` of a Copilot approval panel (see the comment at the top).
pub(crate) fn copilot_permission_answer_key(
    picker: &SessionChatTerminalPicker,
    target: usize,
) -> Option<String> {
    let row = picker.rows.get(target)?;
    if row.label.starts_with("No") {
        return Some(crate::session_chat_send::SESSION_CHAT_INTERRUPT.to_string());
    }
    if target == picker.selected_index {
        return Some(KEY_ENTER.to_string());
    }
    (1..=9)
        .contains(&row.number)
        .then(|| row.number.to_string())
}
