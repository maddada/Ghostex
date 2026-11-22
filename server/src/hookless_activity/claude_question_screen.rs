//! The questions Claude draws on screen, read back as the `AskUserQuestion` input its hook sends.
//!
//! CDXC:AgentHooks 2026-11-12 WHY:
//! Seen live on Claude Code 2.1.322 without hooks: while an AskUserQuestion waits, the session record says `waiting` / `input needed`, but the transcript holds nothing of the asking call (Claude writes the assistant message only once the question is answered), so the observer had no question to send and the chat showed no card, only the session needing input. Ghostex's hook got the questions from the PreToolUse payload. No other file of Claude's has them either: the session record carries only `waitingFor`, the messaging socket (`/tmp/cc-socks/<pid>.sock`) only carries messages between agents, and the job folders only daemon-hosted conversations' state. The screen is the one other place they are. One question is drawn as a `☐ <header>` chip, the question text, numbered option rows with their descriptions under them, then "Type something." and "Enter to select · … · Esc to cancel".
//! CDXC:AgentHooks 2026-11-12 WHY:
//! Several questions are drawn one tab at a time under `←  ☐ Color  ☐ Size  ✔ Submit  →`, so the other tabs are not on screen. With hooks the card showed every question, so the observer walks the tabs the way the answer path does: Right arrows to the Review tab ("Review your answers", which tells which tab it started on), Left arrows back over every tab, reading each, then Right arrows to the tab it started on. Arrows only move between tabs; they choose nothing, and a multi-select tab keeps its ticks. Each step waits for the screen to change and stops at once if the dialog is gone (answered or cancelled in the terminal meanwhile), so a key never lands anywhere else. The terminal shows the tabs flick past for about a second, once per question (drive.rs keeps the result).
//! SEE-ALSO: server/src/session_chat_question_liveness.rs and session_chat_send/ask_answer_keys.rs (the answer path reads and drives the same dialog), server/src/hookless_activity/drive.rs (where the card is sent).

use serde_json::{json, Value};

/// Right arrow: the next tab.
pub(super) const NEXT_TAB: &str = "\u{1b}[C";
/// Left arrow: the previous tab.
pub(super) const PREVIOUS_TAB: &str = "\u{1b}[D";
/// The heading of the last tab of a several-question dialog.
const REVIEW_HEADING: &str = "Review your answers";

/// What the question dialog on screen shows.
#[derive(Debug, PartialEq)]
enum Dialog {
    /// A dialog asking one question: its `AskUserQuestion` question.
    Single(Value),
    /// One tab of a dialog asking several: every tab's header, and this tab's question (`None`
    /// on the Review tab).
    Tab {
        headers: Vec<String>,
        question: Option<Value>,
    },
}

/// The `AskUserQuestion` tool input for the one question on screen, or `None` when the screen
/// shows no such dialog, or one asking several questions.
pub(super) fn single_question_on_screen(screen: &str) -> Option<Value> {
    match dialog(screen)? {
        Dialog::Single(question) => Some(json!({ "questions": [question] })),
        Dialog::Tab { .. } => None,
    }
}

/// Whether the screen shows a dialog asking several questions (whose tabs `questions_on_screen`
/// has to walk).
pub(super) fn several_questions_on_screen(screen: &str) -> bool {
    matches!(dialog(screen), Some(Dialog::Tab { .. }))
}

/// The `AskUserQuestion` tool input for the dialog on screen, one question or several. `step`
/// writes one key to the terminal and returns the screen once it has changed (`None` when it
/// could not be read or did not change); a dialog of several questions is read by walking its
/// tabs and is left on the tab it was on.
pub(super) fn questions_on_screen(
    screen: &str,
    mut step: impl FnMut(&str) -> Option<String>,
) -> Option<Value> {
    let (headers, start_question) = match dialog(screen)? {
        Dialog::Single(question) => return Some(json!({ "questions": [question] })),
        Dialog::Tab { headers, question } => (headers, question?),
    };
    let count = headers.len();
    // Right to the Review tab: how many steps it took says which tab the dialog was on.
    let mut rights = 0;
    loop {
        if rights > count {
            return None;
        }
        let Some(Dialog::Tab {
            headers: seen,
            question,
        }) = step(NEXT_TAB).as_deref().and_then(dialog)
        else {
            return None;
        };
        rights += 1;
        if seen.len() != count {
            return None;
        }
        if question.is_none() {
            break;
        }
    }
    let start = count.checked_sub(rights)?;
    // Left over every tab, last first.
    let mut questions = vec![Value::Null; count];
    for index in (0..count).rev() {
        let Some(Dialog::Tab {
            question: Some(question),
            ..
        }) = step(PREVIOUS_TAB).as_deref().and_then(dialog)
        else {
            return None;
        };
        questions[index] = question;
    }
    // Back to the tab it was on, which must show the question it showed.
    let mut back = None;
    for _ in 0..start {
        back = step(NEXT_TAB);
        back.as_ref()?;
    }
    let shown = match back.as_deref() {
        Some(screen) => match dialog(screen)? {
            Dialog::Tab { question, .. } => question?,
            Dialog::Single(_) => return None,
        },
        None => questions[0].clone(),
    };
    if shown["question"] != start_question["question"] {
        return None;
    }
    for (question, header) in questions.iter_mut().zip(headers) {
        question["header"] = json!(header);
    }
    Some(json!({ "questions": questions }))
}

fn dialog(screen: &str) -> Option<Dialog> {
    let screen = crate::session_chat_resume_prompt::strip_claude_side_panel(screen);
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let tab_row = crate::session_chat_question_liveness::claude_question_tab_row(&lines)?;
    let tab = lines[tab_row].trim();
    let below = &lines[tab_row + 1..];
    if tab.starts_with('←') {
        let headers = tab_headers(tab);
        if headers.len() < 2 {
            return None;
        }
        let heading = below
            .iter()
            .map(|line| line.trim())
            .find(|line| !line.is_empty())?;
        if heading == REVIEW_HEADING {
            return Some(Dialog::Tab {
                headers,
                question: None,
            });
        }
        let question = tab_question(below)?;
        return Some(Dialog::Tab {
            headers,
            question: Some(question),
        });
    }
    // `☐ Color`: one question.
    if tab.matches(['☐', '☒']).count() != 1 || tab.contains("Submit") {
        return None;
    }
    let mut question = tab_question(below)?;
    question["header"] = json!(tab.trim_start_matches(['☐', '☒']).trim());
    Some(Dialog::Single(question))
}

/// `←  ☐ Color  ☐ Size  ✔ Submit  →`: the questions' headers, without the Submit step.
fn tab_headers(tab: &str) -> Vec<String> {
    let mut headers: Vec<String> = tab
        .trim_start_matches('←')
        .trim_end_matches('→')
        .split(['☐', '☒', '✔', '✓', '☑'])
        .map(str::trim)
        .filter(|header| !header.is_empty())
        .map(str::to_string)
        .collect();
    if headers.last().map(String::as_str) == Some("Submit") {
        headers.pop();
    }
    headers
}

/// One tab's question from the lines under the tab row, up to the dialog's key hints.
fn tab_question(lines: &[&str]) -> Option<Value> {
    let hints = lines.iter().position(|line| {
        let line = line.trim();
        line.starts_with("Enter to select") && line.contains("Esc to cancel")
    })?;
    let mut question_lines = Vec::new();
    let mut options: Vec<(String, Vec<String>)> = Vec::new();
    let mut multi_select = false;
    let mut typed_row = false;
    for line in &lines[..hints] {
        if let Some((number, label)) = option_row(line) {
            let (ticked, label) = without_checkbox(label);
            multi_select |= ticked;
            if label.trim_end_matches('.') == "Type something" {
                typed_row = number == options.len() + 1;
                break;
            }
            if number != options.len() + 1 {
                return None;
            }
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
        "question": question_lines.join("\n"),
        "multiSelect": multi_select,
        "options": options,
    }))
}

/// A multi-select row's `[ ] ` or `[✔] `: whether it had one, and the label without it.
fn without_checkbox(label: &str) -> (bool, &str) {
    for mark in ["[ ] ", "[✔] ", "[x] ", "[✓] ", "[X] "] {
        if let Some(rest) = label.strip_prefix(mark) {
            return (true, rest);
        }
    }
    (false, label)
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

    const CHIPS: &str = "←  ☐ Color  ☐ Size  ☐ Toppings  ✔ Submit  →";
    const HINTS: &str = "Enter to select · Tab/Arrow keys to navigate · Esc to cancel";

    /// The four tabs of a three-question dialog as Claude Code 2.1.322 drew them on 2026-11-12.
    fn tabs() -> Vec<String> {
        let rule = "──────────────────────────────";
        vec![
            format!("{CHIPS}\nRed or blue?\n❯ 1. Red\n     Choose red\n  2. Blue\n     Choose blue\n  3. Type something.\n{rule}\n  4. Chat about this\n{HINTS}\n"),
            format!("{CHIPS}\nWhich size do you want?\n❯ 1. Small\n     Small size\n  2. Medium\n     Medium size\n  3. Large\n     Large size\n  4. Type something.\n{rule}\n  5. Chat about this\n{HINTS}\n"),
            format!("{CHIPS}\nWhich toppings?\n❯ 1. [ ] Cheese\n         Add cheese\n  2. [ ] Olives\n         Add olives\n  3. [ ] Type something\n     Submit\n{rule}\n  4. Chat about this\n{HINTS}\n"),
            format!("{CHIPS}\nReview your answers\n⚠ You have not answered all questions\nReady to submit your answers?\n❯ 1. Submit answers\n  2. Cancel\n"),
        ]
    }

    /// A terminal that moves between `tabs` on the arrow keys, recording them.
    fn walk_from(start: usize) -> (Option<Value>, Vec<&'static str>, usize) {
        let tabs = tabs();
        let mut at = start;
        let mut keys = Vec::new();
        let input = questions_on_screen(&tabs[start], |key| {
            keys.push(if key == NEXT_TAB { "→" } else { "←" });
            at = if key == NEXT_TAB {
                (at + 1).min(tabs.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            Some(tabs[at].clone())
        });
        (input, keys, at)
    }

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
        // One question needs no key.
        assert_eq!(
            questions_on_screen(SCREEN, |_| panic!("no key for one question")),
            Some(input)
        );
    }

    #[test]
    fn several_questions_are_read_by_walking_the_tabs_and_left_where_they_were() {
        for start in 0..3 {
            let (input, keys, at) = walk_from(start);
            let input = input.expect("questions");
            assert_eq!(at, start, "back on the tab it started on");
            assert_eq!(keys.len(), 6, "{keys:?}");
            let questions = input["questions"].as_array().expect("array");
            assert_eq!(questions.len(), 3);
            assert_eq!(questions[0]["header"], "Color");
            assert_eq!(questions[1]["question"], "Which size do you want?");
            assert_eq!(questions[1]["options"].as_array().unwrap().len(), 3);
            assert_eq!(
                questions[2],
                json!({
                    "question": "Which toppings?",
                    "header": "Toppings",
                    "multiSelect": true,
                    "options": [
                        {"label": "Cheese", "description": "Add cheese"},
                        {"label": "Olives", "description": "Add olives"},
                    ],
                })
            );
        }
        assert!(several_questions_on_screen(&tabs()[1]));
        assert_eq!(single_question_on_screen(&tabs()[1]), None);
    }

    #[test]
    fn the_walk_stops_at_once_when_the_dialog_goes() {
        let tabs = tabs();
        let mut keys = 0;
        let input = questions_on_screen(&tabs[0], |_| {
            keys += 1;
            Some("❯ \n  ⏸ manual mode on\n".to_string())
        });
        assert_eq!(input, None);
        assert_eq!(keys, 1);
        // A screen that did not change stops it too.
        let mut keys = 0;
        assert_eq!(
            questions_on_screen(&tabs[0], |_| {
                keys += 1;
                None
            }),
            None
        );
        assert_eq!(keys, 1);
    }

    #[test]
    fn an_approval_or_a_plain_prompt_read_as_none() {
        let approval = " Read file\n\n Read(/Users/me/notes.md)\n\n Do you want to proceed?\n ❯ 1. Yes\n   2. Yes, allow reading from /Users/me during this session\n   3. No\n\n Esc to cancel · Tab to amend\n";
        assert_eq!(single_question_on_screen(approval), None);
        assert!(!several_questions_on_screen(approval));
        assert_eq!(single_question_on_screen("❯ \n  ⏸ manual mode on\n"), None);
    }
}
