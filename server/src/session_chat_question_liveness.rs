//! Whether the question a chat card answers is still a live selector on the
//! agent's screen, where its highlight stands, and how the answer is
//! delivered when it is not.

use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};
use crate::session_chat_send::AskAnswerKeyGroup;

const SELECTOR_PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const SELECTOR_PREVIOUS_TAB: &str = "\u{1b}[D"; // Left
/// The heading of the Submit tab Claude shows after the last question.
const CLAUDE_REVIEW_TAB_HEADING: &str = "Review your answers";

/// Long labels can be clipped or wrapped by the selector, so only this many
/// leading characters of the first option have to be on screen.
const LIVE_LABEL_PREFIX_CHARS: usize = 24;

/*
CDXC:AgentScreenDetection 2026-09-21 WHY:
Claude's AskUserQuestion answer is a run of selector digits, which only mean
something while the selector is on screen. A resumed Claude process cancels the
pending question ("[Request interrupted by user for tool use]") and shows its
plain composer, while the chat card can still be up for the few seconds the
resume takes. Observed 2026-09-21: a two-question card answered right after a
resume typed "1", "1", Enter into the composer and submitted the prompt "11".
The digits are therefore written only when a capture taken right now shows the
selector's first row ("1. <first option>"); otherwise the question is no longer
being asked and the answer goes to the agent as an ordinary message.
*/
pub fn claude_question_selector_on_screen(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> bool {
    // The selector can be on any question's tab (the user may have moved
    // through them in the terminal) or on the Submit tab after the last one.
    if claude_question_selector_position(questions, screen_text).is_some() {
        return true;
    }
    let first_labels: Vec<String> = questions
        .iter()
        .filter_map(|question| question.options.first())
        .map(|option| {
            let prefix: String = option
                .label
                .trim()
                .chars()
                .take(LIVE_LABEL_PREFIX_CHARS)
                .collect();
            prefix.trim_end().to_string()
        })
        .filter(|prefix| !prefix.is_empty())
        .collect();
    if first_labels.is_empty() {
        // Nothing to look for: the card has no numbered rows to compare.
        return true;
    }
    // Multi-select rows put a checkbox between the number and the label.
    screen_text.lines().any(|line| {
        line.split_once("1. ").is_some_and(|(_, row)| {
            first_labels
                .iter()
                .any(|prefix| row.contains(prefix.as_str()))
        })
    })
}

/// Where Claude's AskUserQuestion selector stands on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClaudeQuestionSelectorPosition {
    /// The tab on screen: a question index, or `questions.len()` for the
    /// Submit tab that follows the last question.
    pub tab: usize,
    /// How many Up presses bring the highlight to the tab's first row.
    pub rows_below_first: usize,
    /// Rows the tab's "Type something" field sits below its first row, when
    /// the tab has one.
    pub text_row: Option<usize>,
}

/// The selector's tab row: "←  ☐ Color  ☐ Fruit  ✔ Submit  →" with several
/// questions, or a lone header chip (" ☐ Size") with one.
pub(crate) fn claude_question_tab_row(lines: &[&str]) -> Option<usize> {
    lines.iter().rposition(|line| {
        let line = line.trim_start();
        line.starts_with('←') || line.starts_with('☐') || line.starts_with('☒')
    })
}

fn without_whitespace(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

/// CDXC:AgentScreenDetection 2026-09-25 WHY: Claude Code 2.1.281 draws a question longer than 80 columns, or one holding a newline, behind a dim left border ("│ " on every line, blank ones included), so the heading read off the screen matches the question text only without it. Observed 2026-09-25: a four-question card whose first question ran to 169 characters refused every answer with "where its highlight sits could not be read".
fn without_question_gutter(line: &str) -> &str {
    let line = line.trim_start();
    line.strip_prefix('│').unwrap_or(line)
}

/// Reads the selector's current tab and highlighted row from a screen capture.
/// `None` when the capture shows no AskUserQuestion selector for these
/// questions, or its highlight cannot be placed.
pub fn claude_question_selector_position(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> Option<ClaudeQuestionSelectorPosition> {
    let lines: Vec<&str> = screen_text.lines().map(str::trim_end).collect();
    let tab_row = claude_question_tab_row(&lines)?;
    let below = &lines[tab_row + 1..];
    let heading_start = below.iter().position(|line| !line.trim().is_empty())?;
    let heading_len = below[heading_start..]
        .iter()
        .position(|line| line.trim().is_empty())
        .unwrap_or(below.len() - heading_start);
    let heading_lines = &below[heading_start..heading_start + heading_len];
    let heading: String = heading_lines
        .iter()
        .map(|line| without_whitespace(without_question_gutter(line)))
        .collect();
    let tab = if heading_lines.first().map(|line| line.trim()) == Some(CLAUDE_REVIEW_TAB_HEADING) {
        questions.len()
    } else if !lines[tab_row].trim_start().starts_with('←') {
        // A lone chip: the dialog asks one question.
        (questions.len() == 1).then_some(0)?
    } else {
        // Wrapping only splits the question at spaces; clipping keeps a prefix.
        let texts: Vec<String> = questions
            .iter()
            .map(|question| without_whitespace(&question.question))
            .collect();
        texts.iter().position(|text| *text == heading).or_else(|| {
            (!heading.is_empty())
                .then(|| texts.iter().position(|text| text.starts_with(&heading)))
                .flatten()
        })?
    };
    let highlight = below[heading_start + heading_len..]
        .iter()
        .find_map(|line| line.strip_prefix('❯'))?
        .trim();
    let number = highlight
        .split_once(". ")
        .and_then(|(number, _)| number.parse::<usize>().ok())
        .filter(|number| *number >= 1);
    let rows_below_first = match (questions.get(tab), number) {
        // Multi-select draws an unnumbered Submit/Next row between
        // "Type something" and "Chat about this".
        (Some(question), Some(number))
            if question.multi_select && number > question.options.len() + 1 =>
        {
            number
        }
        (_, Some(number)) => number - 1,
        (Some(question), None) if question.multi_select => question.options.len() + 1,
        _ => return None,
    };
    let text_row = questions
        .get(tab)
        .filter(|question| !question.preview_layout)
        .map(|question| question.options.len());
    Some(ClaudeQuestionSelectorPosition {
        tab,
        rows_below_first,
        text_row,
    })
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: "make gxserver move the highlight back before answering", and later the same day: text the user already typed into "Type something" in the terminal must be cleared before the chat's answer is typed, and a multi-select tab must end with exactly the chat's picks. The answer plan types option digits and arrow runs that assume the first question's tab is open with its first row highlighted. A highlight left on "Type something" turned the digits into custom text (the answer arrived as "1"), and a tab left on a later question sent the answers to the wrong questions. So the plan starts from the capture it already takes: Up to the tab's first row (Up from the first row wraps, so the count is exact, never a blind run), then Left to the first question, where Claude opens every tab on its first row. Each question's keys then start with a step that reads that tab when it runs (session_chat_claude_question_prep.rs): it empties leftover field text (otherwise joined to the typed answer, or submitted beside a picked option), presses the digit only for rows whose tick differs from the chat's pick (a digit toggles, so a row already ticked in the terminal was unticked), and brings the highlight back to the first row.
/// CDXC:SessionChat 2026-09-25 WHY: Claude Code 2.1.280 drops the rest of an Up burst once the highlight lands on the "Type something" field (five Ups from "Chat about this" stopped in the field and the option digits became its text), so the Ups that reach the field are a group of their own and the queue's pacing lets the field settle before the rest.
pub fn claude_question_selector_reset_keys(
    position: ClaudeQuestionSelectorPosition,
) -> Vec<AskAnswerKeyGroup> {
    let mut groups = Vec::new();
    let mut remaining = position.rows_below_first;
    if let Some(text_row) = position.text_row.filter(|row| remaining > *row && *row > 0) {
        groups.push(AskAnswerKeyGroup::Raw(
            SELECTOR_PREVIOUS_ROW.repeat(remaining - text_row),
        ));
        remaining = text_row;
    }
    if remaining > 0 {
        groups.push(AskAnswerKeyGroup::Raw(
            SELECTOR_PREVIOUS_ROW.repeat(remaining),
        ));
    }
    if position.tab > 0 {
        groups.push(AskAnswerKeyGroup::Raw(
            SELECTOR_PREVIOUS_TAB.repeat(position.tab),
        ));
    }
    groups
}

/// The answer as a message the agent can read without the selector: every
/// question with its picked labels, in order.
pub fn format_ask_answer_message(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> String {
    let lines: Vec<String> = questions
        .iter()
        .enumerate()
        .filter_map(|(index, question)| {
            let selection = selections.get(index)?;
            let mut labels: Vec<String> = selection
                .indices
                .iter()
                .filter_map(|option_index| question.options.get(*option_index))
                .map(|option| option.label.trim().to_string())
                .filter(|label| !label.is_empty())
                .collect();
            let other = selection.other.as_deref().unwrap_or_default().trim();
            if !other.is_empty() {
                labels.push(other.to_string());
            }
            (!labels.is_empty()).then(|| {
                format!(
                    "- {} Answer: {}",
                    question.question.trim(),
                    labels.join(", ")
                )
            })
        })
        .collect();
    format!("My answers to your questions:\n{}", lines.join("\n"))
}
