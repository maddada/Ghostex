//! The question Claude draws on screen, read back as the `AskUserQuestion` input its hook sends.
//!
//! CDXC:AgentHooks 2026-11-12 WHY:
//! Seen live on Claude Code 2.1.322 without hooks: while an AskUserQuestion waits, the session record says `waiting` / `input needed`, but the transcript holds nothing of the asking call (Claude writes the assistant message only once the question is answered), so the observer had no question to send and the chat showed no card, only the session needing input. Ghostex's hook got the questions from the PreToolUse payload. The screen is the one other place they are: one question is drawn as a `☐ <header>` chip, the question text, numbered option rows with their descriptions under them, then "Type something." and "Enter to select · … · Esc to cancel". Only a dialog that asks one question is read: with several, the other tabs' questions are not on screen, and a card that knew only one of them would answer the wrong tab.
//! SEE-ALSO: server/src/session_chat_question_liveness.rs (the answer path reads the same dialog), server/src/hookless_activity/drive.rs (where the card is sent).

use serde_json::{json, Value};

/// The `AskUserQuestion` tool input for the one question on screen, or `None` when the screen
/// shows no such dialog, or one asking several questions.
pub(super) fn single_question_on_screen(screen: &str) -> Option<Value> {
    let screen = crate::session_chat_resume_prompt::strip_claude_side_panel(screen);
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let hints = lines.iter().rposition(|line| {
        let line = line.trim();
        line.starts_with("Enter to select") && line.contains("Esc to cancel")
    })?;
    let lines = &lines[..hints];
    let tab_row = crate::session_chat_question_liveness::claude_question_tab_row(lines)?;
    let tab = lines[tab_row].trim();
    // `←  ☐ Color  ☐ Size  ✔ Submit  →`: several questions (or a multi-select's Submit step).
    if tab.starts_with('←') || tab.contains("Submit") || tab.matches(['☐', '☒']).count() != 1
    {
        return None;
    }
    let header = tab.trim_start_matches(['☐', '☒']).trim().to_string();
    let mut question_lines = Vec::new();
    let mut options: Vec<(String, Vec<String>)> = Vec::new();
    let mut multi_select = false;
    let mut typed_row = false;
    for line in &lines[tab_row + 1..] {
        if let Some((number, label)) = option_row(line) {
            if label.trim_end_matches('.') == "Type something" {
                typed_row = number == options.len() + 1;
                break;
            }
            if number != options.len() + 1 {
                return None;
            }
            let label = match label.strip_prefix("[ ] ").or_else(|| {
                ["[✔] ", "[x] ", "[✓] "]
                    .iter()
                    .find_map(|mark| label.strip_prefix(mark))
            }) {
                Some(label) => {
                    multi_select = true;
                    label
                }
                None => label,
            };
            options.push((label.trim().to_string(), Vec::new()));
            continue;
        }
        let text = line.trim_start();
        let text = text.strip_prefix('│').unwrap_or(text).trim();
        match options.last_mut() {
            Some((_, description)) if !text.is_empty() => description.push(text.to_string()),
            Some(_) => {}
            None if !text.is_empty() => question_lines.push(text.to_string()),
            None => {}
        }
    }
    if !typed_row || options.is_empty() || question_lines.is_empty() {
        return None;
    }
    let options: Vec<Value> = options
        .into_iter()
        .map(|(label, description)| {
            let mut option = json!({ "label": label });
            if !description.is_empty() {
                option["description"] = json!(description.join(" "));
            }
            option
        })
        .collect();
    Some(json!({
        "questions": [{
            "question": question_lines.join("\n"),
            "header": header,
            "multiSelect": multi_select,
            "options": options,
        }]
    }))
}

/// `❯ 1. Red` or `  2. Blue`: the row's number and label.
fn option_row(line: &str) -> Option<(usize, &str)> {
    let line = line.trim_start();
    let line = line.strip_prefix('❯').unwrap_or(line).trim_start();
    let (number, label) = line.split_once(". ")?;
    let number = number.parse::<usize>().ok().filter(|number| *number >= 1)?;
    Some((number, label.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The dialog as Claude Code 2.1.322 drew it on 2026-11-12, with the prompt above it.
    const SCREEN: &str = "❯ Use the AskUserQuestion tool to ask me one question: red or blue?
────────────────────────────────────────────────────────────
 ☐ Color

Red or blue?

❯ 1. Red
     Choose red.
  2. Blue
     Choose blue.
  3. Type something.
────────────────────────────────────────────────────────────
  4. Chat about this

Enter to select · ↑/↓ to navigate · Esc to cancel
";

    #[test]
    fn one_question_on_screen_reads_as_the_hooks_tool_input() {
        let input = single_question_on_screen(SCREEN).expect("question");
        assert_eq!(
            input,
            json!({"questions": [{
                "question": "Red or blue?",
                "header": "Color",
                "multiSelect": false,
                "options": [
                    {"label": "Red", "description": "Choose red."},
                    {"label": "Blue", "description": "Choose blue."},
                ],
            }]})
        );
    }

    #[test]
    fn several_questions_an_approval_or_a_plain_prompt_read_as_none() {
        let several = SCREEN.replace(" ☐ Color", "←  ☐ Color  ☐ Size  ✔ Submit  →");
        assert_eq!(single_question_on_screen(&several), None);
        let approval = " Read file\n\n Read(/Users/me/notes.md)\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, allow reading from /Users/me during this session\n   3. No\n\n Esc to cancel · Tab to amend\n";
        assert_eq!(single_question_on_screen(approval), None);
        assert_eq!(single_question_on_screen("❯ \n  ⏸ manual mode on\n"), None);
    }
}
