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
    // Claude Code's files side panel (`strip_claude_side_panel`) runs into every line otherwise.
    let screen_text = &crate::session_chat_resume_prompt::strip_claude_side_panel(screen_text);
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
            comparable(&prefix)
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
            let row = comparable(row);
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

/// CDXC:AgentScreenDetection 2026-10-29 WHY: GitHub #350 (Windows 11): a step-4-of-4 question card refused its answer with "where its highlight sits could not be read" now and then. The question on screen is read back through the terminal's own grid, and a character that grid draws differently from the text Claude was given (a curly quote, a dash, a symbol the Windows console maps to another glyph) broke the exact 40-character match, so the answer was refused although the question was right there. Questions are compared on their letters and digits only, and a heading that still matches no question is placed by the option rows under it.
fn comparable(text: &str) -> String {
    text.chars().filter(|c| c.is_alphanumeric()).collect()
}

/// CDXC:AgentScreenDetection 2026-09-25 WHY: Claude Code 2.1.281 draws a question longer than 80 columns, or one holding a newline, behind a dim left border ("│ " on every line, blank ones included), so the heading read off the screen matches the question text only without it. Observed 2026-09-25: a four-question card whose first question ran to 169 characters refused every answer with "where its highlight sits could not be read".
fn without_question_gutter(line: &str) -> &str {
    let line = line.trim_start();
    line.strip_prefix('│').unwrap_or(line)
}

/// Leading characters of a question's text (whitespace removed) that pick its tab.
const HEADING_MATCH_CHARS: usize = 40;

/// CDXC:AgentScreenDetection 2026-09-26 DECISION: User: "how about we just compare starting x num of chars", choosing to match the question on screen by its first 40 characters and to look further only when two questions start the same. Claude may draw the rest of a long question differently from the text it was given (it clips past 2000 characters with "…" and replaces odd characters), and that must not stop the answer; a shared start is broken by the longest run the heading shares with each question, and a tie there reads as "not placed" rather than a guess, because keys sent to the wrong tab answer the wrong question.
fn claude_question_tab_for_heading(
    questions: &[SessionChatQuestion],
    heading: &str,
) -> Option<usize> {
    let texts: Vec<String> = questions
        .iter()
        .map(|question| comparable(&question.question))
        .collect();
    let starts_like_heading = |text: &str| {
        text.chars()
            .take(HEADING_MATCH_CHARS)
            .eq(heading.chars().take(HEADING_MATCH_CHARS))
    };
    let candidates: Vec<usize> = if !heading.is_empty() {
        {
            (0..texts.len())
                .filter(|index| starts_like_heading(&texts[*index]))
                .collect()
        }
    } else {
        Default::default()
    };
    if let [only] = candidates.as_slice() {
        return Some(*only);
    }
    let shared = |index: usize| {
        texts[index]
            .chars()
            .zip(heading.chars())
            .take_while(|(text, shown)| text == shown)
            .count()
    };
    let longest = candidates.iter().map(|index| shared(*index)).max()?;
    let mut best = candidates
        .into_iter()
        .filter(|index| shared(*index) == longest);
    let first = best.next()?;
    best.next().is_none().then_some(first)
}

/// The tab on screen and the line its option rows start from.
///
/// CDXC:AgentScreenDetection 2026-10-23 WHY: Claude draws a question taller than its terminal (a 45-line JSON approval, observed 2026-10-23) with the tab row and the question's start scrolled off the top, so a reader that needs the tab row refused every answer with "where its highlight sits could not be read". With no tab row on screen the tab is read from what is still there: the option rows, every label in order with the "Type something" row after them, and the end of that question's text right above them. A question whose rows and text both fit is the tab; two that fit, or none, read as "not placed", as a tie does in `claude_question_tab_for_heading`.
pub(crate) fn claude_question_tab_and_rows(
    questions: &[SessionChatQuestion],
    lines: &[&str],
) -> Option<(usize, usize)> {
    let Some(tab_row) = claude_question_tab_row(lines) else {
        return claude_question_tab_scrolled_off(questions, lines);
    };
    let below = &lines[tab_row + 1..];
    let heading_start = below.iter().position(|line| !line.trim().is_empty())?;
    let heading_len = below[heading_start..]
        .iter()
        .position(|line| line.trim().is_empty())
        .unwrap_or(below.len() - heading_start);
    let heading_lines = &below[heading_start..heading_start + heading_len];
    let heading: String = heading_lines
        .iter()
        .map(|line| comparable(without_question_gutter(line)))
        .collect();
    let tab = if heading_lines.first().map(|line| line.trim()) == Some(CLAUDE_REVIEW_TAB_HEADING) {
        questions.len()
    } else if !lines[tab_row].trim_start().starts_with('←') && questions.len() == 1 {
        // A lone chip: the dialog asks one question.
        0
    } else {
        let rows_from = tab_row + 1 + heading_start + heading_len;
        match claude_question_tab_for_heading(questions, &heading) {
            Some(tab) => tab,
            None => claude_question_tab_by_rows(questions, &lines[rows_from..])?,
        }
    };
    Some((tab, tab_row + 1 + heading_start + heading_len))
}

/// The only question whose option rows (every label in order, then its "Type something" row)
/// are on `lines`; `None` when none or several are. See CDXC:AgentScreenDetection on `comparable`.
fn claude_question_tab_by_rows(questions: &[SessionChatQuestion], lines: &[&str]) -> Option<usize> {
    let mut fits = questions
        .iter()
        .enumerate()
        .filter(|(_, question)| !question.preview_layout && !question.options.is_empty())
        .filter(|(_, question)| claude_option_rows_span(question, lines).is_some())
        .map(|(index, _)| index);
    let only = fits.next()?;
    fits.next().is_none().then_some(only)
}

/// Reads the selector's current tab and highlighted row from a screen capture.
/// `None` when the capture shows no AskUserQuestion selector for these
/// questions, or its highlight cannot be placed.
pub fn claude_question_selector_position(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> Option<ClaudeQuestionSelectorPosition> {
    let screen_text = crate::session_chat_resume_prompt::strip_claude_side_panel(screen_text);
    let lines: Vec<&str> = screen_text.lines().map(str::trim_end).collect();
    let (tab, rows_from) = claude_question_tab_and_rows(questions, &lines)?;
    let highlight = lines[rows_from..]
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

/// How many lines may sit between the "Type something" row and the dialog's key hints: the rule,
/// "Chat about this", a blank line, and one spare.
const FOOTER_AFTER_FIELD_LINES: usize = 6;

/// Whether the bottom of the screen is one of these questions' own selector: its option rows (every
/// label in order, then "Type something") with Claude's "Enter to select · … · Esc to cancel" hints
/// right under them. Escape there cancels this question and nothing else.
///
/// CDXC:SessionChat 2026-10-29 WHY: GitHub #350: when even fresh reads cannot place the highlight, the answer must still reach Claude, never by typing keys whose effect depends on where the highlight is. Escape on a live AskUserQuestion selector closes it (Claude records that the questions were declined and its turn ends), and the answers then go as an ordinary message through the send path, which types only into a verified input box. A screen whose bottom is not this question (another dialog, a working turn, a selector left in the scrollback) gets no Escape, since Escape there would deny a permission or stop a turn.
pub fn claude_question_dialog_at_bottom(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> bool {
    let screen_text = crate::session_chat_resume_prompt::strip_claude_side_panel(screen_text);
    let lines: Vec<&str> = screen_text.lines().map(str::trim_end).collect();
    let Some(footer) = lines.iter().rposition(|line| !line.trim().is_empty()) else {
        return false;
    };
    // Claude's task list can sit under the dialog, after a blank line.
    let footer = lines[..=footer]
        .iter()
        .rposition(|line| line.contains("Enter to select") && line.contains("Esc to cancel"))
        .filter(|index| {
            lines[*index + 1..=footer]
                .iter()
                .all(|line| !line.contains('─') && !line.trim_start().starts_with('❯'))
        });
    let Some(footer) = footer else {
        return false;
    };
    questions
        .iter()
        .filter(|question| !question.preview_layout && !question.options.is_empty())
        .filter_map(|question| claude_option_rows_span(question, &lines[..footer]))
        .any(|(_, field)| footer - field <= FOOTER_AFTER_FIELD_LINES)
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

/// One numbered selector row: its number and its text, past the highlight
/// mark and a multi-select checkbox.
fn claude_option_row(line: &str) -> Option<(usize, &str)> {
    let line = line.trim_start().trim_start_matches('❯').trim_start();
    let (number, rest) = line.split_once(". ")?;
    let number = number.parse::<usize>().ok()?;
    let rest = match rest.strip_prefix('[').and_then(|rest| rest.split_once(']')) {
        Some((_, rest)) => rest,
        None => rest,
    };
    Some((number, rest.trim()))
}

/// The first and last line of the last run of rows on screen that shows every option of
/// `question` in order ("1. <label>" …) followed by its "Type something" row.
fn claude_option_rows_span(
    question: &SessionChatQuestion,
    lines: &[&str],
) -> Option<(usize, usize)> {
    let labels: Vec<String> = question
        .options
        .iter()
        .map(|option| {
            let prefix: String = option
                .label
                .trim()
                .chars()
                .take(LIVE_LABEL_PREFIX_CHARS)
                .collect();
            comparable(&prefix)
        })
        .collect();
    let run_end = |start: usize| {
        let mut next = 1;
        for (offset, line) in lines[start..].iter().enumerate() {
            let Some((number, text)) = claude_option_row(line) else {
                // A description under its option, or the rule before "Chat about this".
                if next == 1 {
                    return None;
                }
                continue;
            };
            if number != next {
                if next == 1 {
                    return None;
                }
                continue;
            }
            if labels
                .get(number - 1)
                .is_some_and(|label| !comparable(text).starts_with(label.as_str()))
            {
                return None;
            }
            if number == labels.len() + 1 {
                return Some(start + offset);
            }
            next += 1;
        }
        None
    };
    (0..lines.len())
        .rev()
        .find_map(|start| run_end(start).map(|end| (start, end)))
}

/// See CDXC:AgentScreenDetection on `claude_question_tab_and_rows`.
fn claude_question_tab_scrolled_off(
    questions: &[SessionChatQuestion],
    lines: &[&str],
) -> Option<(usize, usize)> {
    let fits: Vec<(usize, usize)> = questions
        .iter()
        .enumerate()
        .filter(|(_, question)| !question.preview_layout && !question.options.is_empty())
        .filter_map(|(index, question)| {
            let (rows, _) = claude_option_rows_span(question, lines)?;
            // The question's last lines: the block right above the rows, up to a blank line.
            let above = &lines[..rows];
            let end = above.iter().rposition(|line| !line.trim().is_empty())?;
            let start = above[..end]
                .iter()
                .rposition(|line| line.trim().is_empty())
                .map_or(0, |blank| blank + 1);
            // Claude clips a very long question with "…", which `comparable` drops.
            let shown: String = above[start..=end]
                .iter()
                .map(|line| comparable(without_question_gutter(line)))
                .collect();
            let tail: String = {
                let chars: Vec<char> = shown.chars().collect();
                chars[chars.len().saturating_sub(HEADING_MATCH_CHARS)..]
                    .iter()
                    .collect()
            };
            (!tail.is_empty() && comparable(&question.question).contains(&tail))
                .then_some((index, rows))
        })
        .collect();
    match fits.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}
