/*
CDXC:AgentScreenDetection 2026-10-01 WHY:
Antigravity CLI's tool permission panel, read off the screen (measured against
agy 1.2.14 with `toolPermission: "request-review"` on a zmx pty):

    Command
    ──────────────────
    Requesting permission for:
       echo approval-test > /tmp/agy-approval.txt
    Run this command?
    > 1. Yes, run command
      2. Yes, and always allow in this conversation for commands that start with '…'
      3. Yes, and always allow for commands that start with '…' (Persist to settings.json)
      4. No, cancel
      ↑/↓ Navigate · tab Amend · ctrl+g edit/expand command
    esc to cancel                                         Gemini 3.8 Flash · high

agy fires no hook that names the request, so the screen is the only source for
an approval card. A file write shows the path and its diff instead of the
"Requesting permission for:" line, over "Allow creation of this file?" and
"Yes, allow creation" / "No, deny creation" rows; the request is whatever sits
between the panel's rule and its question. The panel becomes the same `permissionPrompt` terminal notice
Claude's dialog does, with every row carried verbatim, and is answered by the
row's printed number: a digit picks the row at once with no Enter (measured: `4`
declined the call). A long row wraps onto unnumbered lines, which are joined back
into the row above. Liveness is the Navigate hint directly under the rows with
only agy's status line below it; the TUI erases the panel once it is answered.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_resume_prompt::{
    parse_picker_row, SessionChatTerminalPicker, SessionChatTerminalPickerKind,
    SessionChatTerminalPickerRow,
};

const REQUEST_LEAD: &str = "Requesting permission for:";
const NAVIGATE_HINT: &str = "↑/↓ Navigate";
/// The panel plus the command it quotes; a long command wraps over several lines.
const SCAN_LINES: usize = 40;
/// Only agy's status line (and a custom statusline) may follow the hint.
const TAIL_LINES: usize = 3;
const DETAIL_MAX_CHARS: usize = 400;
/// How far above the question the request is looked for.
const REQUEST_MAX_LINES: usize = 12;

fn is_rule(line: &str) -> bool {
    let line = line.trim();
    !line.is_empty() && line.chars().all(|ch| ch == '─')
}

/// The live permission panel, or `None` when the screen shows none.
pub fn detect_antigravity_permission_picker(text: &str) -> Option<SessionChatTerminalPicker> {
    let mut window: Vec<String> = text
        .lines()
        .rev()
        .map(|raw| {
            normalize_spaces(&strip_ansi_sgr(raw))
                .trim_end()
                .to_string()
        })
        .filter(|line| !line.trim().is_empty())
        .take(SCAN_LINES)
        .collect();
    window.reverse();
    let hint = window
        .iter()
        .rposition(|line| line.contains(NAVIGATE_HINT))?;
    if window.len() - hint > TAIL_LINES {
        return None;
    }
    let first_row = (0..hint).rev().find(|&index| {
        parse_picker_row(&window[index]).is_some_and(|(_, number, _)| number == 1)
    })?;
    let heading = window.get(first_row.checked_sub(1)?)?.trim().to_string();
    if !heading.ends_with('?') {
        return None;
    }
    // The request sits between the panel's rule (or the command panel's lead line) and the
    // question; a file panel shows the path and its diff there.
    let start = (0..first_row - 1)
        .rev()
        .take(REQUEST_MAX_LINES)
        .find(|&index| window[index].trim() == REQUEST_LEAD || is_rule(&window[index]))
        .map_or(first_row - 1, |index| index + 1);
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
                row.label.push_str(line.trim());
            }
        }
    }
    // agy's ask_question panel (`Question 1/n:` over its options and a trailing "Write-in..."
    // row) is answered by its own question card, even when its options read Yes and No.
    if !rows.iter().any(|row| row.label.starts_with("Yes"))
        || !rows.iter().any(|row| row.label.starts_with("No"))
        || rows.iter().any(|row| row.label.starts_with("Write-in"))
    {
        return None;
    }
    let selected_index = rows.iter().position(|row| row.selected)?;
    let request = window[start..first_row - 1]
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join(" ");
    let mut detail = format!("{heading} {request}").trim().to_string();
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
        kind: SessionChatTerminalPickerKind::AntigravityPermission,
        detail: Some(detail),
        rows,
        selected_index,
    })
}
