//! Claude's live Ink panels, including nested menus and editable fields.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::ops::RangeInclusive;

use crate::domain::DomainStateError;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text_vt, execute_session_chat_send, SessionChatSendStep,
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};
use crate::session_chat_terminal_dialog::{TerminalDialog, TerminalDialogRow};

/*
CDXC:AgentScreenDetection 2026-09-27 DECISION:
User: every Claude Code dialog that asks for a choice must reach the chat as a card whose buttons answer it, like the model switch confirmation.
Only the fullscreen renderer draws panels under a `▔` rule; the default one draws them in the input slot under the `─` rule that otherwise tops the composer, so tool prompts such as "Enter plan mode?", the setup screens (trust, new MCP servers, external imports, API key) and the resume, nudge and onboarding choosers were never read.
Such a panel is the last `─` rule followed by an indented `❯` row and no composer; the question tool (answered through its own card) and the offers a send closes with Escape (session_chat_claude_popups.rs, whose DECISION keeps Enter always sending) stay out.
*/
fn is_bottom_slot_dialog(text: &str, after: &[String]) -> bool {
    let highlighted = after
        .iter()
        .any(|line| line.starts_with(' ') && line.trim_start().starts_with("❯ "));
    let composer = after
        .iter()
        .any(|line| line.starts_with('❯') || line.starts_with("╭─"));
    let title_is_row = after
        .iter()
        .find(|line| line.chars().any(char::is_alphanumeric))
        .is_some_and(|line| {
            let line = line.trim_start().trim_start_matches('❯').trim_start();
            line.split_once(". ")
                .is_some_and(|(number, _)| number.parse::<u32>().is_ok())
        });
    let question_tool = after
        .iter()
        .any(|line| line.contains('☐') || line.contains('☒') || line.contains("✔ Submit"));
    highlighted
        && !composer
        && !title_is_row
        && !question_tool
        && crate::session_chat_claude_popups::claude_escape_safe_popup(text).is_none()
}

/// CDXC:AgentScreenDetection 2026-09-28 WHY:
/// Claude's first-run screens (text style, login method, the sign-in code, the "Press Enter to continue/retry" notes, terminal setup) draw under its "Welcome to Claude Code" banner with no panel rule, so a new install stopped at "finish it in the terminal" and Claude could not be set up from Ghostex. The banner line starts them when no transcript follows and a highlighted row, the sign-in code field or an Enter prompt is waiting.
fn first_run_start(lines: &[String]) -> Option<usize> {
    let welcome = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with("Welcome to Claude Code"))?;
    let after = &lines[welcome + 1..];
    let transcript = after
        .iter()
        .any(|line| line.starts_with('⏺') || line.starts_with('❯') || line.starts_with("╭─"));
    let waiting = after.iter().any(|line| {
        (line.starts_with(' ') && line.trim_start().starts_with("❯ "))
            || first_run_code_field(line).is_some()
            || line.to_ascii_lowercase().contains("press enter to")
    });
    (!transcript && waiting).then_some(welcome)
}

/// What is typed after "Paste code here if prompted >", the field Claude's browser sign-in reads its code from.
fn first_run_code_field(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("Paste code here if prompted")?;
    Some(rest.trim_start().strip_prefix('>').unwrap_or(rest).trim())
}

/// A first-run screen as the card shows it: without the theme step's colour preview (a sample diff between
/// `╌` rules), and with the sign-in link the terminal wrapped at its width joined back into one line.
fn first_run_content(content: &[String]) -> Vec<String> {
    let end = content
        .iter()
        .position(|line| {
            let line = line.trim();
            line.chars().count() >= 20 && line.chars().all(|c| c == '╌')
        })
        .unwrap_or(content.len());
    let mut lines: Vec<String> = Vec::with_capacity(end);
    let mut in_link = false;
    for line in &content[..end] {
        let wrapped = in_link && !line.is_empty() && !line.contains(char::is_whitespace);
        match lines.last_mut() {
            Some(link) if wrapped => link.push_str(line),
            _ => {
                in_link = line.starts_with("https://");
                lines.push(line.clone());
            }
        }
    }
    lines
}

/// Whether a chooser is Claude's own usage-limit menu ("You've reached your Fable limit"), which
/// must stay a usage-limit notice so account switching and queued-delivery holds still see it.
pub(crate) fn is_claude_usage_limit_chooser(dialog: &TerminalDialog) -> bool {
    !dialog.rows.is_empty()
        && [
            "You've reached your",
            "You've hit your",
            "You're out of usage credits",
        ]
        .iter()
        .any(|lead| dialog.title.starts_with(lead))
}

/// "No, keep planning  shift+tab to approve with this feedback": a key the chat card has no use
/// for, not a description of the row it sits beside or under.
fn is_key_hint(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    ["shift+tab to ", "tab to ", "ctrl+", "esc to "]
        .iter()
        .any(|lead| lower.starts_with(lead))
}

/// The options a Claude chooser draws: the run of rows around the highlighted `❯` one, with the
/// lines each row wraps onto, and where that run sits in `remainder`. Unselected rows are indented to
/// the highlighted row's label and wrapped lines deeper. Numbers are optional (`hideIndexes`), so a
/// row's number is its printed one or its position; numbered lines outside the run (a plan's own
/// list above "Ready to code?"'s options) are not options.
fn option_rows(remainder: &[String]) -> (Vec<TerminalDialogRow>, RangeInclusive<usize>) {
    let indent = |line: &str| line.chars().take_while(|c| *c == ' ').count();
    /*
    CDXC:AgentScreenDetection 2026-10-15 WHY:
    A list that marks its current value (`/theme`'s "✔ Light mode") keeps a two-column mark slot on every row, so unmarked rows sit two columns deeper than the `❯` row's mark and were read as wrapped lines: only the marked row was clickable. Rows are aligned by where their label starts, after the mark.
    */
    let mark = |line: &str| {
        let text = line.trim_start();
        if text.starts_with("✔ ") || text.starts_with("✓ ") {
            2
        } else {
            0
        }
    };
    let Some(selected) = remainder
        .iter()
        .rposition(|line| line.trim_start().starts_with('❯'))
    else {
        return (Vec::new(), RangeInclusive::new(1, 0));
    };
    let after_pointer = remainder[selected].trim_start().trim_start_matches('❯');
    let label_column =
        indent(&remainder[selected]) + 1 + indent(after_pointer) + mark(after_pointer);
    let label_start = |line: &str| indent(line) + mark(line);
    let is_row = |index: usize| {
        index == selected
            || (!remainder[index].trim().is_empty()
                && label_start(&remainder[index]) == label_column)
    };
    let is_wrap = |index: usize| {
        !remainder[index].trim().is_empty() && label_start(&remainder[index]) > label_column
    };
    /*
    CDXC:AgentScreenDetection 2026-10-15 WHY:
    `/plugin`'s Marketplaces tab puts a blank line between its bulleted entries ("+ Add Marketplace", "● Anthropic Directory"), so only the highlighted entry was read. A bulleted list's run crosses blank lines; an unbulleted one does not, because a wrapped paragraph above it ("will work as intended…" over `/plugin`'s install scope) sits at the label column too.
    */
    let bulleted = after_pointer
        .trim_start()
        .trim_start_matches(['✔', '✓'])
        .trim_start()
        .split_once(' ')
        .is_some_and(|(bullet, _)| ["+", "●", "○", "◯", "◉", "•"].contains(&bullet));
    let is_blank = |index: usize| remainder[index].trim().is_empty();
    let member = |index: usize| is_row(index) || is_wrap(index);
    let mut first = selected;
    loop {
        let mut above = first;
        while bulleted && above > 0 && is_blank(above - 1) {
            above -= 1;
        }
        if above > 0 && member(above - 1) {
            first = above - 1;
        } else {
            break;
        }
    }
    while first < selected && !is_row(first) {
        first += 1;
    }
    let mut last = selected;
    loop {
        let mut below = last + 1;
        while bulleted && below < remainder.len() && is_blank(below) {
            below += 1;
        }
        if below < remainder.len() && member(below) {
            last = below;
        } else {
            break;
        }
    }
    let mut rows: Vec<TerminalDialogRow> = Vec::new();
    for index in first..=last {
        let line = remainder[index].trim();
        if line.is_empty() {
            continue;
        }
        if !is_row(index) {
            if is_key_hint(line) {
                continue;
            }
            if let Some(row) = rows.last_mut() {
                let detail = row.description.get_or_insert_with(String::new);
                if !detail.is_empty() {
                    detail.push(' ');
                }
                detail.push_str(line);
            }
            continue;
        }
        let text = line.trim_start_matches(['❯', '↓', '↑']).trim();
        let (number, text) = text
            .split_once(". ")
            .and_then(|(number, rest)| number.parse::<u32>().ok().map(|number| (number, rest)))
            .unwrap_or((rows.len() as u32 + 1, text));
        let (label, description) = text
            .trim()
            .split_once("  ")
            .map(|(label, description)| (label, Some(description.trim().to_string())))
            .unwrap_or((text.trim(), None));
        let description = description.filter(|description| !is_key_hint(description));
        rows.push(TerminalDialogRow {
            number,
            label: label.to_string(),
            description,
            selected: index == selected,
        });
    }
    (rows, first..=last)
}

/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: drive Claude's commands through zmx and make their interactions usable in chat, as for Codex.
/// Claude's panel boundary survives nested menus and clipped footers; a later composer means the panel is historical.
/// SEE-ALSO: apps/desktop/src/app/native_chat/terminal_dialog.rs.
pub fn detect_claude_dialog(text: &str) -> Option<TerminalDialog> {
    let text = &crate::session_chat_resume_prompt::strip_claude_side_panel(text);
    if let Some(dialog) = crate::session_chat_claude_effort_notice::detect_effort_notice(text) {
        return Some(dialog);
    }
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    let rule = |fill: char| {
        lines.iter().rposition(|line| {
            let line = line.trim();
            line.chars().count() >= 20 && line.chars().all(|c| c == fill)
        })
    };
    let modal = rule('▔');
    let slot = rule('─').filter(|&index| is_bottom_slot_dialog(text, &lines[index + 1..]));
    let first_run = modal.is_none() && slot.is_none();
    // A `▔` panel keeps its own inner `─` rules ("Ready to code?"), so it wins unless transcript
    // rows between the two show it is an answered panel left on screen.
    let start = match (modal, slot) {
        (Some(modal), Some(slot))
            if slot > modal
                && lines[modal + 1..slot]
                    .iter()
                    .any(|line| line.starts_with('⏺') || line.starts_with('❯')) =>
        {
            slot
        }
        (Some(modal), _) => modal,
        (None, Some(slot)) => slot,
        (None, None) => first_run_start(&lines)?,
    } + 1;
    /*
    CDXC:AgentScreenDetection 2026-09-29 WHY:
    `/login` in a running session draws the same browser sign-in step inside its "Login" panel, so the sign-in code field was not read there: the card showed the panel's raw text, clipped the link the terminal wrapped at its width, and had no field for the code. That panel is read like the first-run step, with its "Login" heading skipped.
    */
    let login_panel = !first_run
        && lines[start..]
            .iter()
            .any(|line| first_run_code_field(line).is_some());
    let sign_in_screen = first_run || login_panel;
    let first_run_lines: Vec<String>;
    let content: &[String] = if sign_in_screen {
        first_run_lines = first_run_content(&lines[start..]);
        &first_run_lines
    } else {
        &lines[start..]
    };
    // The composer has no left indent. Selection markers inside Ink panels do.
    if content
        .iter()
        .any(|line| line.starts_with('❯') || line.starts_with("╭─"))
    {
        return None;
    }
    let is_heading = |line: &String| {
        line.chars().any(|c| c.is_alphanumeric())
            && !(first_run && line.trim() == "Let's get started.")
    };
    let heading = content.iter().position(is_heading)?;
    let heading = if login_panel && content[heading].trim() == "Login" {
        heading + 1 + content[heading + 1..].iter().position(is_heading)?
    } else {
        heading
    };
    let mut title = content[heading].trim().to_string();
    let mut title_hint = None;
    if sign_in_screen {
        // The card shows the link itself; "c" copies it only in the terminal.
        title = title.trim_end_matches("(c to copy)").trim_end().to_string();
        // "Login successful. Press Enter to continue…" is one line: the hint becomes the button.
        if let Some(at) = title.find("Press Enter to").filter(|at| *at > 0) {
            title_hint = Some(title[at..].to_string());
            title = title[..at].trim_end().to_string();
        }
    }
    if title.len() > 240 || title.starts_with("Question ") {
        return None;
    }
    let remainder = &content[heading + 1..];
    let is_hint = |line: &str| {
        let lower = line.to_ascii_lowercase();
        lower.contains("esc to")
            || lower.contains("esc cancel")
            || lower.contains("enter to")
            || lower.contains(" to search")
            || lower.contains(" to filter")
            || lower.contains(" to sort")
            || lower.contains("←/→")
            || lower.contains("↑/↓")
            || lower.contains("d to day")
            || lower.contains("press y to confirm")
    };
    let footer = title_hint
        .as_deref()
        .into_iter()
        .chain(
            remainder
                .iter()
                .filter(|line| is_hint(line))
                .map(|line| line.trim()),
        )
        .collect::<Vec<_>>()
        .join("\n");
    let lower = footer.to_ascii_lowercase();
    // A text field is boxed on both sides; a quote (the message `/rewind` restores to) only has
    // the left bar.
    let boxed = remainder.iter().find_map(|line| {
        let line = line.trim();
        let value = line.strip_prefix('│')?;
        (value.ends_with('│') || value.trim_start().starts_with('⌕'))
            .then(|| value.trim_end_matches('│').trim())
    });
    let search = boxed.is_some_and(|line| line.starts_with('⌕'));
    let feedback_field = remainder
        .iter()
        .position(|line| line.trim() == "Describe the issue below:")
        .map(|i| {
            remainder[i + 1..]
                .iter()
                .take_while(|line| !is_hint(line))
                .map(|line| line.trim())
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_string()
        });
    let plan_feedback = (title == "Ready to code?"
        && remainder
            .iter()
            .any(|line| line.contains("shift+tab to approve with this feedback")))
    .then(|| {
        remainder.iter().rev().find_map(|line| {
            let row = line.trim().strip_prefix("❯ ")?;
            let (_, value) = row.split_once(". ")?;
            (!value.starts_with("Yes,")).then(|| {
                if value == "Tell Claude what to change" {
                    ""
                } else {
                    value
                }
            })
        })
    })
    .flatten();
    let text_field = remainder
        .iter()
        .find_map(|line| line.trim().strip_prefix("> "))
        .or_else(|| {
            sign_in_screen
                .then(|| remainder.iter().find_map(|line| first_run_code_field(line)))
                .flatten()
        });
    let input = if search {
        Some("search")
    } else if boxed.is_some()
        || text_field.is_some()
        || feedback_field.is_some()
        || plan_feedback.is_some()
    {
        Some("text")
    } else {
        None
    };
    let field = plan_feedback
        .or(feedback_field.as_deref())
        .or(text_field)
        .or(boxed)
        .unwrap_or_default()
        .trim_start_matches('⌕')
        .trim();
    let input_value = if field.ends_with('…') {
        String::new()
    } else {
        field.to_string()
    };
    let (mut rows, option_lines) = option_rows(remainder);
    /*
    CDXC:AgentScreenDetection 2026-10-15 DECISION:
    User picked: keep option rows on tabbed Claude panels (`/plugin`'s install scope, an installed plugin's actions, Marketplaces), which a "title has a tab strip" check used to drop, so the scope could only be chosen in the terminal.
    User then picked option D, "Numbered choices" (packages/gx-chat-core/src/questions/dialog_rows.rs), which lists the plugin list's plugins as numbered rows too: a searchable or checkbox list ("Type to search · Space to toggle · Enter to view") and one scrolled with a "↓ more below" marker keep their visible rows, since a click moves the highlight by arrows and presses Enter. This replaces this comment's earlier rule that dropped rows for a search field, checkboxes or a scrolled list. Still without rows: a text field, a numbered list cut mid-way ("↓ 10. …"), and the `/resume` picker, which has its own card.
    SEE-ALSO: server/src/session_chat_claude_panel.rs (the panel's blocks beside the rows), packages/gx-chat-core/src/questions/terminal_prompts.rs.
    */
    let picker = crate::session_chat_resume_picker::claude_resume_picker(&title, remainder);
    let numbered = matches!(input, None | Some("search"))
        && picker.is_none()
        && rows.iter().filter(|r| r.selected).count() == 1
        // A scrolled numbered list marks its cut-off row "↓ 5. …"; "↓ more below" only says
        // there are more rows, and a lone arrow at the right edge scrolls a plan or text viewport.
        && !remainder.iter().any(|line| {
            let line = line.trim();
            line.starts_with(['↓', '↑'])
                && line[3..]
                    .trim_start()
                    .starts_with(|c: char| c.is_ascii_digit())
        });
    if !numbered {
        rows.clear();
    }
    let body_lines: Vec<&String> = remainder
        .iter()
        .enumerate()
        .filter(|(index, line)| {
            !is_hint(line)
                && (!numbered || !option_lines.contains(index))
                && !(sign_in_screen && first_run_code_field(line).is_some())
        })
        .map(|(_, line)| line)
        .collect();
    // The panel's own left padding is not the text's: without it, lines the terminal wrapped
    // ("faster. With / your current settings") read as one paragraph instead of indented ones.
    let margin = body_lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.chars().take_while(|c| *c == ' ').count())
        .min()
        .unwrap_or(0);
    let body = body_lines
        .iter()
        .map(|line| line.get(margin..).unwrap_or_default().to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    let mut actions = Vec::new();
    if input != Some("text")
        && (remainder
            .iter()
            .any(|line| line.trim().starts_with(['❯', '↓', '↑']))
            || lower.contains("↑/↓")
            || lower.contains("↓ to")
            || title.starts_with("Help ")
            || title.starts_with("Settings "))
    {
        actions.extend(["up", "down"]);
    }
    if lower.contains("←/→")
        || title.starts_with("Help ")
        || title.starts_with("Settings ")
        || title.starts_with("Plugins ")
    {
        actions.extend(["left", "right"]);
    }
    if lower.contains("tab") {
        actions.push("tab");
    }
    if lower.contains("space") {
        actions.push("toggle");
    }
    if lower.contains("session only") {
        actions.push("sessionOnly");
    }
    if lower.contains("ctrl+a") {
        actions.push("projects");
    }
    if lower.contains("ctrl+b") {
        actions.push("branch");
    }
    if lower.contains("t to sort") {
        actions.push("sort");
    }
    if lower.contains("r reset") {
        actions.push("reset");
    }
    if lower.contains("d to day") {
        actions.extend(["day", "week"]);
    }
    if lower.contains("enter") || !rows.is_empty() {
        actions.push("confirm");
    }
    if lower.contains("f to fork") {
        actions.push("fork");
    }
    // "Press y to confirm or n to cancel" (removing a marketplace): the chat lists the two keys as
    // its Yes and No rows (dialog_rows.rs in the chat core).
    if lower.contains("press y to confirm") {
        actions.extend(["yes", "no"]);
    }
    // First-run setup has nothing to go back to.
    if !first_run {
        actions.push("cancel");
    }
    // CDXC:SessionChat 2026-09-27 WHY: the `/btw` panel animates a spinner glyph beside "Answering…", which changed the id on every capture, so Close pressed while Claude was still answering was always refused as "the dialog changed". Blank rows under a short panel are left out too: the probe's capture and the answer's capture disagree on them, which refused every answer to the Settings panel.
    let painted = content
        .iter()
        .rposition(|line| !line.trim().is_empty())
        .map_or(0, |last| last + 1);
    let identity: Vec<&str> = content[..painted]
        .iter()
        .map(|line| {
            let trimmed = line.trim_end();
            if trimmed.ends_with("Answering…") {
                "Answering…"
            // `/rewind`'s confirmation quotes the message with its age, "│ (14s ago)", which
            // ticks between the card's capture and the answer's.
            } else if trimmed.ends_with(" ago)") && trimmed.trim_start().starts_with("│ (") {
                "│ (ago)"
            } else {
                line.as_str()
            }
        })
        .collect();
    // The picker's footer names no Enter key, but Enter resumes the highlighted conversation.
    if picker.is_some() && !actions.contains(&"confirm") {
        actions.push("confirm");
    }
    Some(TerminalDialog {
        id: hex::encode(Sha256::digest(identity.join("\n").as_bytes())),
        title,
        body,
        footer,
        rows,
        input: input.map(str::to_string),
        input_value,
        actions: actions.into_iter().map(str::to_string).collect(),
        side_question: None,
        blocks: None,
        picker,
    })
}

fn invalid() -> DomainStateError {
    DomainStateError {
        code: "invalidParams",
        message: "That action is not offered by this Claude dialog.".to_string(),
    }
}

/// CDXC:AgentScreenDetection 2026-09-05 WHY:
/// Claude 2.1.260 ignored legacy arrows in live zmx menus; explicit unmodified CSI keys moved the highlight correctly.
/// Text editing and submission use separate stdin writes so Ink does not interpret a combined control/paste burst as literal input.
pub(crate) fn claude_dialog_steps(
    dialog: &TerminalDialog,
    params: &Map<String, Value>,
) -> Result<Vec<SessionChatSendStep>, DomainStateError> {
    let mut steps = vec![SessionChatSendStep::VerifyTerminalDialog {
        agent: "claude".to_string(),
        id: dialog.id.clone(),
    }];
    if let Some(index) = params.get("choiceIndex").and_then(Value::as_u64) {
        let index = usize::try_from(index).map_err(|_| invalid())?;
        dialog.rows.get(index).ok_or_else(invalid)?;
        let selected = dialog
            .rows
            .iter()
            .position(|row| row.selected)
            .ok_or_else(invalid)?;
        let moves = index.abs_diff(selected);
        for step in 0..moves {
            steps.push(SessionChatSendStep::Write(
                if index > selected {
                    "\x1b[1;1B"
                } else {
                    "\x1b[1;1A"
                }
                .to_string(),
            ));
            /*
            CDXC:AgentScreenDetection 2026-10-27 WHY:
            Claude's newer dialogs ("We recommend compacting earlier", its confirm prompts) refuse an Enter that lands within 250ms of an arrow key and then ignore every key for 600ms (Claude Code 2.1.309, `fo`/`tfe` beside `ga`), so the second row, picked as an arrow then Enter 80ms later, was refused and the dialog stayed open. The Enter waits out that window; arrows between themselves keep the short gap.
            */
            steps.push(SessionChatSendStep::SleepMs(if step + 1 == moves {
                CLAUDE_ENTER_AFTER_ARROW_MS
            } else {
                80
            }));
        }
        if dialog.rows[index].label != "Tell Claude what to change" {
            steps.push(SessionChatSendStep::Write("\r".to_string()));
        }
    } else {
        let action = params
            .get("dialogAction")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if (action == "text" && dialog.input.as_deref() == Some("search"))
            || (action == "submit" && dialog.input.as_deref() == Some("text"))
        {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            if text.len() > 8192
                || text.chars().any(|c| {
                    c.is_control() && !(dialog.title == "Submit feedback / bug report" && c == '\n')
                })
            {
                return Err(invalid());
            }
            if dialog.footer.contains("/ to search") {
                steps.push(SessionChatSendStep::Write("/".to_string()));
                steps.push(SessionChatSendStep::SleepMs(100));
            }
            let clear = if dialog.input.as_deref() == Some("search") {
                "\x1b[101;5u\x1b[117;5u".to_string()
            } else {
                crate::session_chat_send::build_agent_tui_clear_input_for_text(&dialog.input_value)
                    .replace('\u{15}', "\x1b[117;5u")
                    .replace('\u{b}', "\x1b[107;5u")
            };
            steps.push(SessionChatSendStep::Write(clear));
            steps.push(SessionChatSendStep::SleepMs(100));
            if !text.is_empty() {
                steps.push(SessionChatSendStep::Write(
                    crate::session_chat_send::wrap_terminal_bracketed_paste_text(text),
                ));
                steps.push(SessionChatSendStep::SleepMs(150));
            }
            if action == "submit" {
                steps.push(SessionChatSendStep::Write("\r".to_string()));
            }
        } else if action == "selectTab" {
            // A tab strip moves one tab per arrow; the client sends how far to go.
            let delta = params
                .get("tabDelta")
                .and_then(Value::as_i64)
                .filter(|delta| *delta != 0 && delta.abs() <= 12)
                .ok_or_else(invalid)?;
            let key = if delta > 0 { "right" } else { "left" };
            if !dialog.actions.iter().any(|a| a == key) {
                return Err(invalid());
            }
            for index in 0..delta.unsigned_abs() {
                if index > 0 {
                    steps.push(SessionChatSendStep::SleepMs(80));
                }
                steps.push(SessionChatSendStep::Write(
                    if delta > 0 { "\x1b[1;1C" } else { "\x1b[1;1D" }.to_string(),
                ));
            }
        } else {
            if !dialog.actions.iter().any(|a| a == action) {
                return Err(invalid());
            }
            let payload = match action {
                "up" => "\x1b[1;1A",
                "down" => "\x1b[1;1B",
                "left" => "\x1b[1;1D",
                "right" => "\x1b[1;1C",
                "tab" => "\t",
                "toggle" => " ",
                "confirm" => "\r",
                "sessionOnly" => "s",
                "projects" => "\x1b[97;5u",
                "branch" => "\x1b[98;5u",
                "sort" => "t",
                "reset" => "r",
                "day" => "d",
                "week" => "w",
                "fork" => "f",
                "yes" => "y",
                "no" => "n",
                "cancel" => SESSION_CHAT_INTERRUPT,
                _ => return Err(invalid()),
            };
            steps.push(SessionChatSendStep::Write(payload.to_string()));
        }
    }
    // CDXC:SessionChat 2026-09-16 DECISION: User requested prompt Escape and terminal controls; single-key actions must not hold the send queue for a fixed post-send delay.
    if params.contains_key("choiceIndex")
        || matches!(
            params.get("dialogAction").and_then(Value::as_str),
            Some("text" | "submit")
        )
    {
        steps.push(SessionChatSendStep::SleepMs(250));
    }
    Ok(steps)
}

/// How long an Enter waits after the last arrow: past the 250ms Claude refuses an Enter in.
pub(crate) const CLAUDE_ENTER_AFTER_ARROW_MS: u64 = 350;

/// CDXC:AgentScreenDetection 2026-10-27 WHY:
/// The chat's card is read from the VT capture (session_chat_options/detection.rs keeps Claude's bold for its headings) and turned to plain text, while the answer used to recheck the plain capture: any way the two formats disagree (blank cells written as cursor-forward in VT, GitHub #323) gave the same dialog two ids, and every pick on that card was refused as "Claude's dialog changed" with nothing in the chat to get out of it. The answer and its verify step read the dialog through the same capture as the card, so only a real change on screen refuses an answer.
/// SEE-ALSO: server/src/session_chat_send/worker.rs (`VerifyTerminalDialog`), server/src/session_chat_options/detection.rs.
pub(crate) async fn capture_claude_dialog(zmx_name: &str) -> Option<TerminalDialog> {
    let styled = capture_session_terminal_text_vt(zmx_name).await?;
    detect_claude_dialog(&crate::session_chat_screen_styles::plain_screen_text(
        &styled,
    ))
}

pub(crate) async fn answer_claude_dialog(
    target: &SessionChatSendTarget,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let stale = || {
        DomainStateError {
        code: "invalidState",
        message: "Claude's dialog changed. Review the current choices and try again, or answer it in the terminal.".to_string(),
    }
    };
    let dialog = capture_claude_dialog(&target.zmx_name)
        .await
        .ok_or_else(stale)?;
    if params.get("dialogId").and_then(Value::as_str) != Some(dialog.id.as_str()) {
        return Err(stale());
    }
    execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-dialog",
        claude_dialog_steps(&dialog, params)?,
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.message,
    })?;
    if params.contains_key("choiceIndex")
        && capture_claude_dialog(&target.zmx_name)
            .await
            .is_some_and(|current| current.id == dialog.id)
    {
        return Err(DomainStateError {
            code: "invalidState",
            message: "Claude kept this dialog open. Review its message and try again.".to_string(),
        });
    }
    Ok(json!({"queued": true}))
}
