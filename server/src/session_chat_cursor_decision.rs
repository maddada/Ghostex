//! Cursor CLI's keyed decision panels, answered from the chat.
//!
//! cursor-agent 2026.10.01 draws one dropdown (`prompt/decision-dropdown.tsx`) for every choice that owns
//! its input: the plan's "Ready to build?" (`1. Yes, build locally (b)`, `2. Yes, build in cloud (c)` in a
//! repository, `No, propose changes (p or Esc)`), the agent's "Switch to <mode> mode?" request
//! (`Approve mode switch (y)`, `Reject (n or esc)`, rejected on its own when the timer bar runs out) and the
//! debug flow's reproduction steps (`1. Proceed (enter)`, `2. Mark as fixed (f)`, `3. Write a follow-up (w)`).
//! Each row ends with its key in parentheses and the highlighted row starts with `→`, so the rows, the
//! highlight and the key that answers each row are read off the screen.
//!
//! CDXC:AgentScreenDetection 2026-10-07 DECISION:
//! User: "make the chat interface work perfectly for all the cases we already support in claude code". Claude's plan approval ("Ready to code?") is answered from the chat, so Cursor's plan approval and its other keyed decisions are a choice card too, answered by each row's own key rather than by arrows (the key commits the row wherever the highlight is).
//! SEE-ALSO: server/src/session_chat_send/answer_http.rs (the `terminalChoice` answer), server/src/session_chat_notice/classify.rs.

use crate::session_chat_notice::{
    session_chat_terminal_screen_tail, SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
    SessionChatTerminalNoticeChoice, SessionChatTerminalNoticeSeverity,
    SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED,
};
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const SCAN_LINES: usize = 80;
/// Lines the panel may draw below its last row: its padding, its closing border and the timer footer.
const MAX_LINES_AFTER_ROWS: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorDecisionRow {
    pub label: String,
    /// What the terminal is sent to pick the row: its letter, or Enter.
    pub key: String,
    pub selected: bool,
    /// Escape answers this row too (`Reject (n or esc)`, `Skip … (esc or n)`).
    pub dismisses: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorDecision {
    /// The panel's question ("Ready to build?", "Switch to Agent mode?"), when it shows one.
    pub question: Option<String>,
    /// The lines between the question and the rows (the mode switch's explanation).
    pub explanation: Vec<String>,
    pub rows: Vec<CursorDecisionRow>,
    /// The plan file the panel names ("Saved to …"), on a plan's "Ready to build?".
    pub plan_file: Option<String>,
    /// The command a "Run this command?" approval is about (`$  New-Item … in .`).
    pub command: Option<String>,
}

fn panel_text(line: &str) -> String {
    const BORDERS: &[char] = &['\u{2502}', '\u{2503}', '|'];
    normalize_spaces(&strip_ansi_sgr(line))
        .trim()
        .trim_start_matches(BORDERS)
        .trim_end_matches(BORDERS)
        .trim()
        .to_string()
}

fn is_rule(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|character| matches!(character, '\u{2500}'..='\u{259f}') || character == ' ')
}

/// The bytes one key name of a row's hint types.
fn hint_key(name: &str) -> Option<&'static str> {
    Some(match name {
        "enter" => "\r",
        "tab" => "\t",
        "shift+tab" => "\u{1b}[Z",
        "esc" => "\u{1b}",
        _ => return None,
    })
}

/// `→ 1. Yes, build locally (b)`, `  Reject (n or esc)` or `  Run Everything (shift+tab)`.
///
/// CDXC:AgentScreenDetection 2026-10-17 WHY: Cursor's command approval ("Run this command?", drawn whenever Run Everything is off, which is the default without "Agent approvals: accept all") keys its rows `(y)`, `(tab)`, `(shift+tab)` and `(esc or n)`. Only letters and Enter were read, so the panel never parsed, no card reached the chat and a new user's Cursor sat on "Orbiting…" until someone answered in the terminal (agent flow tests, Windows, 2026-10-17). A row's letter wins over its other keys, so `(esc or n)` types `n`.
fn parse_row(line: &str) -> Option<CursorDecisionRow> {
    let selected = line.starts_with('\u{2192}');
    let rest = line.trim_start_matches('\u{2192}').trim();
    let open = rest.rfind(" (")?;
    let hint = rest[open + 2..].strip_suffix(')')?.to_ascii_lowercase();
    let names: Vec<&str> = hint.split(" or ").map(str::trim).collect();
    let letter = names
        .iter()
        .find(|name| name.len() == 1 && name.chars().all(|c| c.is_ascii_lowercase()));
    let key = match letter {
        Some(letter) => letter.to_string(),
        None => hint_key(names.first()?)?.to_string(),
    };
    let dismisses = names.contains(&"esc");
    let label = rest[..open].trim();
    let label = match label.split_once(". ") {
        Some((number, text))
            if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) =>
        {
            text
        }
        _ => label,
    };
    (!label.is_empty()).then(|| CursorDecisionRow {
        label: label.to_string(),
        key,
        selected,
        dismisses,
    })
}

/// The live decision panel, or `None` when no keyed dropdown owns Cursor's input.
pub fn detect_cursor_decision(text: &str) -> Option<CursorDecision> {
    let lines: Vec<String> = text
        .lines()
        .rev()
        .map(panel_text)
        .filter(|line| !line.is_empty())
        .take(SCAN_LINES)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let last_row = lines.iter().rposition(|line| parse_row(line).is_some())?;
    // Below the rows only the panel's own padding, border and timer footer may follow: a composer or
    // more transcript means the panel is scrollback.
    let after = &lines[last_row + 1..];
    if after.len() > MAX_LINES_AFTER_ROWS
        || after
            .iter()
            .any(|line| line.starts_with('\u{2192}') || line.contains('\u{2584}'))
    {
        return None;
    }
    let mut first_row = last_row;
    while first_row > 0 && parse_row(&lines[first_row - 1]).is_some() {
        first_row -= 1;
    }
    let rows: Vec<CursorDecisionRow> = lines[first_row..=last_row]
        .iter()
        .filter_map(|line| parse_row(line))
        .collect();
    if rows.len() < 2 || rows.iter().filter(|row| row.selected).count() != 1 {
        return None;
    }
    let above: Vec<&String> = lines[first_row.saturating_sub(4)..first_row]
        .iter()
        .filter(|line| !is_rule(line))
        .collect();
    let question_at = above.iter().rposition(|line| line.ends_with('?'));
    let question = question_at.map(|index| above[index].to_string());
    let explanation = question_at
        .map(|index| {
            above[index + 1..]
                .iter()
                .map(|line| line.to_string())
                .collect()
        })
        .unwrap_or_default();
    let plan_file = lines[first_row.saturating_sub(8)..first_row]
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix("Saved to "))
        .map(|path| path.trim().to_string());
    let command = lines[first_row.saturating_sub(6)..first_row]
        .iter()
        .rev()
        .find_map(|line| line.strip_prefix('$'))
        .map(|command| {
            let command = command.trim();
            command
                .rsplit_once(" in ")
                .map_or(command, |(command, _)| command)
                .trim()
                .to_string()
        })
        .filter(|command| !command.is_empty());
    Some(CursorDecision {
        question,
        explanation,
        rows,
        plan_file,
        command,
    })
}

/// The plan Cursor saved for its "Ready to build?" panel: the Markdown of a `*.plan.md` file under
/// `~/.cursor/plans`, without the leading id comment and the front matter that holds its to-dos.
fn saved_plan_markdown(path: &str) -> Option<String> {
    let plans = crate::resume_lookup::home_dir()
        .join(".cursor")
        .join("plans");
    let path = std::path::Path::new(path);
    let inside = path
        .parent()
        .zip(plans.to_str())
        .is_some_and(|(parent, plans)| {
            parent
                .to_string_lossy()
                .eq_ignore_ascii_case(plans.trim_end_matches(['\\', '/']))
        });
    if !inside || !path.to_string_lossy().ends_with(".plan.md") {
        return None;
    }
    if std::fs::metadata(path).ok()?.len() > 256 * 1024 {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let mut text = text.trim_start();
    if let Some((_, rest)) = text
        .strip_prefix("<!--")
        .and_then(|rest| rest.split_once("-->"))
    {
        text = rest.trim_start();
    }
    if let Some((_, rest)) = text
        .strip_prefix("---")
        .and_then(|rest| rest.split_once("\n---"))
    {
        text = rest.trim_start();
    }
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The key that picks row `index` of the live panel.
pub fn cursor_decision_answer_key(text: &str, index: usize) -> Option<String> {
    detect_cursor_decision(text)?
        .rows
        .get(index)
        .map(|row| row.key.clone())
}

/// Whether row `index` of the live panel is the command approval's "Skip & tell the agent what to
/// do instead", which the chat shows as "Skip" and answers with Escape and then a fixed reason in
/// the box it opens (`SessionChatSendStep::SkipCursorInsteadBox`).
///
/// CDXC:SessionChat 2026-10-17 DECISION: Asked whether the chat's Skip should leave Cursor asking "what to do instead" or skip outright, the user picked "skip outright": the card's Skip, labelled "Skip", skips straight away, so Cursor never waits in its "Tell the agent what to do instead" box and the next chat message is a normal new message, with no false "might not have reached the agent" warning. The box gets the fixed reason "The user skipped this command. Don't run it again." rather than an empty Enter, because an empty skip told the model nothing and Grok 4.7 retried the same command.
pub fn cursor_decision_skips_outright(text: &str, index: usize) -> bool {
    detect_cursor_decision(text).is_some_and(|decision| {
        decision
            .rows
            .get(index)
            .is_some_and(|row| is_outright_skip(&decision, row))
    })
}

/// What the chat's Skip types into Cursor's "Tell the agent what to do instead" box.
pub const CURSOR_SKIP_REASON: &str = "The user skipped this command. Don't run it again.";

fn is_outright_skip(decision: &CursorDecision, row: &CursorDecisionRow) -> bool {
    decision.command.is_some() && row.dismisses && row.label.starts_with("Skip")
}

/// What Stop types to close the live panel: Escape, when one of its rows takes it. Cursor ignores
/// Ctrl+C while a panel owns its input, so Stop wrote "Interrupted the agent" and left the
/// approval on screen.
pub fn cursor_decision_dismiss_key(text: &str) -> Option<&'static str> {
    detect_cursor_decision(text)?
        .rows
        .iter()
        .any(|row| row.dismisses)
        .then_some("\u{1b}")
}

/// A decision panel owns Cursor's input. Its rows answer to keys the chat also sends for other
/// reasons (Shift+Tab is the Mode pill's cycle key and the approval's "Run Everything").
pub fn cursor_decision_visible(text: &str) -> bool {
    detect_cursor_decision(text).is_some()
}

/// The live panel as a choice card.
pub fn detect_cursor_decision_notice(text: &str) -> Option<SessionChatTerminalNotice> {
    let decision = detect_cursor_decision(text)?;
    let question = decision.question.clone().unwrap_or_default();
    let (title, detail) = if question == "Ready to build?" {
        let cloud = decision.rows.iter().any(|row| row.key == "c");
        let mut detail = if cloud {
            "Build it here, build it in the cloud, or propose changes and Cursor revises the plan."
                .to_string()
        } else {
            "Build it here, or propose changes and Cursor revises the plan.".to_string()
        };
        // Cursor writes the turn to its transcript only once it ends, so until this is answered
        // the plan exists only on screen and in the file the panel names.
        if let Some(plan) = decision.plan_file.as_deref().and_then(saved_plan_markdown) {
            detail.push_str("\n\n");
            detail.push_str(&plan);
        }
        ("Cursor's plan is ready".to_string(), detail)
    } else if question == "Run this command?" {
        let mut detail = decision
            .command
            .as_deref()
            .map(|command| format!("```\n{command}\n```"))
            .unwrap_or_default();
        let reason = decision.explanation.join(" ");
        if !reason.is_empty() {
            if !detail.is_empty() {
                detail.push_str("\n\n");
            }
            detail.push_str(&reason);
        }
        ("Cursor wants to run a command".to_string(), detail)
    } else if question.starts_with("Switch to ") {
        let mut detail = decision.explanation.join(" ");
        if !detail.is_empty() {
            detail.push(' ');
        }
        detail.push_str("Cursor rejects the switch by itself when its timer runs out.");
        (question, detail)
    } else if question.is_empty() {
        (
            "Cursor is waiting for your decision".to_string(),
            decision.explanation.join(" "),
        )
    } else {
        (question, decision.explanation.join(" "))
    };
    Some(
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_CURSOR_INPUT_BLOCKED,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            title,
        )
        .with_detail(detail)
        .with_screen_tail(session_chat_terminal_screen_tail(text))
        .with_choices({
            // A collapsed card shows its first two rows, so building in the cloud goes last and
            // "No, propose changes" (the row Esc answers) stays one click away; a command
            // approval's allowlist rows (Tab, Shift+Tab) follow its Skip row for the same reason.
            // Each row keeps its screen index, which is what its answer sends.
            let mut choices: Vec<SessionChatTerminalNoticeChoice> = decision
                .rows
                .iter()
                .enumerate()
                .map(|(index, row)| SessionChatTerminalNoticeChoice {
                    index,
                    // The chat's Skip never asks what to do instead (the DECISION above).
                    label: if is_outright_skip(&decision, row) {
                        "Skip".to_string()
                    } else {
                        row.label.clone()
                    },
                    selected: row.selected,
                })
                .collect();
            choices.sort_by_key(|choice| {
                let key = decision.rows[choice.index].key.as_str();
                (key == "c", matches!(key, "\t" | "\u{1b}[Z"))
            });
            choices
        })
        .with_actions(vec![SessionChatTerminalNoticeAction::switch_to_terminal(
            "Open terminal",
        )]),
    )
}
