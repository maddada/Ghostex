//! Readies one tab of Claude's AskUserQuestion selector for its answer keys,
//! from the screen as it is when the step runs.

use crate::session_chat::SessionChatQuestion;
use crate::session_chat_question_liveness::{
    claude_question_selector_position, claude_question_tab_row, ClaudeQuestionSelectorPosition,
};
use crate::session_chat_question_row_align::{write_keys_and_await_repaint, CLEAR_LINE_KEYS};
use crate::session_chat_send::capture_session_terminal_text;

const PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const NEXT_ROW: &str = "\u{1b}[B"; // Down
const SETTLE_MS: u64 = 250;
/// Moves, toggles and one clear per wrapped line of leftover text.
const MAX_ATTEMPTS: usize = 30;
/// Reads that may still show the previous tab while the next one paints.
const TAB_WAITS: usize = 4;

/// The step's target: which tab, and for a multi-select tab the options that
/// must end ticked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeQuestionPrep {
    pub questions: Vec<SessionChatQuestion>,
    pub question: usize,
    /// `Some` for a multi-select tab: the option indices the chat picked.
    pub ticked: Option<Vec<usize>>,
}

/// What one tab shows right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeQuestionTab {
    pub position: ClaudeQuestionSelectorPosition,
    /// Per option, then the "Type something" row: whether its box is ticked,
    /// `None` when the row draws no box or is not on screen.
    pub ticked: Vec<Option<bool>>,
    /// The "Type something" field holds text instead of its placeholder.
    pub field_text: bool,
}

fn checkbox(text: &str) -> (Option<bool>, &str) {
    let Some(rest) = text.strip_prefix('[') else {
        return (None, text);
    };
    let Some((inside, rest)) = rest.split_once(']') else {
        return (None, text);
    };
    let ticked = match inside.trim() {
        "" => Some(false),
        "✔" | "✓" | "x" | "X" => Some(true),
        _ => return (None, text),
    };
    (ticked, rest.trim_start())
}

/// Reads the tab on screen. `None` when no selector for these questions is
/// on screen or its highlight cannot be placed.
pub fn read_claude_question_tab(
    questions: &[SessionChatQuestion],
    screen: &str,
) -> Option<ClaudeQuestionTab> {
    let position = claude_question_selector_position(questions, screen)?;
    let mut tab = ClaudeQuestionTab {
        position,
        ticked: Vec::new(),
        field_text: false,
    };
    // The Submit tab has no rows of its own.
    let Some(question) = questions.get(position.tab) else {
        return Some(tab);
    };
    let rows = question.options.len() + 1;
    tab.ticked = vec![None; rows];
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let tab_row = claude_question_tab_row(&lines)?;
    for line in &lines[tab_row + 1..] {
        let line = line.trim_start().trim_start_matches('❯').trim_start();
        let Some((number, rest)) = line.split_once(". ") else {
            continue;
        };
        let Some(index) = number
            .parse::<usize>()
            .ok()
            .and_then(|number| number.checked_sub(1))
            .filter(|index| *index < rows)
        else {
            continue;
        };
        let (ticked, text) = checkbox(rest);
        tab.ticked[index] = ticked;
        if index == rows - 1 && !question.preview_layout {
            // The placeholder is "Type something." (single-select) or
            // "Type something" (multi-select).
            let text = text.trim();
            tab.field_text = !text.is_empty() && text.trim_end_matches('.') != "Type something";
        }
    }
    Some(tab)
}

/// The writes that bring the tab closer to ready, or `None` once it is.
/// `Err` when a row the step must tick is not on screen.
pub fn next_claude_question_prep_keys(
    prep: &ClaudeQuestionPrep,
    tab: &ClaudeQuestionTab,
) -> Result<Option<Vec<String>>, ()> {
    let question = prep.questions.get(prep.question).ok_or(())?;
    let highlight = tab.position.rows_below_first;
    let field = (!question.preview_layout).then_some(question.options.len());
    if let Some(field) = field.filter(|_| tab.field_text) {
        return Ok(Some(match highlight.cmp(&field) {
            std::cmp::Ordering::Equal => {
                CLEAR_LINE_KEYS.iter().map(|key| key.to_string()).collect()
            }
            std::cmp::Ordering::Less => vec![NEXT_ROW.repeat(field - highlight)],
            std::cmp::Ordering::Greater => vec![PREVIOUS_ROW.repeat(highlight - field)],
        }));
    }
    if highlight > 0 {
        // Claude drops the rest of an Up burst that lands on the field, so
        // reaching the field is a write of its own (see the WHY in
        // session_chat_question_liveness.rs).
        let ups = match field {
            Some(field) if highlight > field && field > 0 => highlight - field,
            _ => highlight,
        };
        return Ok(Some(vec![PREVIOUS_ROW.repeat(ups)]));
    }
    if let Some(wanted) = &prep.ticked {
        // The field row counts too: a digit ticks it even while it is empty.
        for row in 0..=question.options.len() {
            let ticked = tab.ticked.get(row).copied().flatten().ok_or(())?;
            if ticked != wanted.contains(&row) {
                return Ok(Some(vec![(row + 1).to_string()]));
            }
        }
    }
    Ok(None)
}

/// CDXC:SessionChat 2026-09-25 WHY: Claude Code 2.1.280 keeps text typed into "Type something" when the highlight leaves the field, shows it in place of the placeholder, and puts the caret back at the start when the field is highlighted again, so a chat answer typed there landed in front of the leftover text. Ctrl+E then Ctrl+U empties one wrapped line of the field (Ctrl+U alone does nothing with the caret at the start, and a burst of Ups inside wrapped text moves the caret instead of the highlight), so the step repeats it until the screen shows the placeholder. Emptying a multi-select field also unticks it; a digit ticks the field row even when it is empty.
/// SEE-ALSO: CDXC:SessionChat DECISION in session_chat_question_liveness.rs; build_claude_ask_answer_keys in server/src/session_chat_send.rs.
pub(crate) async fn prepare_claude_question(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    prep: &ClaudeQuestionPrep,
    superseded: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    let lost =
        || "Claude's question is no longer on screen, so the answer was not sent.".to_string();
    let mut tab_waits = 0;
    for _ in 0..MAX_ATTEMPTS {
        if superseded() {
            return Err("The answer was superseded before it reached Claude.".to_string());
        }
        let capture = capture_session_terminal_text(zmx_name)
            .await
            .ok_or_else(lost)?;
        let tab = read_claude_question_tab(&prep.questions, &capture).ok_or_else(lost)?;
        if tab.position.tab != prep.question {
            // The key that moved to this tab may not have repainted yet.
            tab_waits += 1;
            if tab_waits > TAB_WAITS {
                return Err(
                    "Claude's question did not move to the answer's tab. Answer it in the terminal."
                        .to_string(),
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(SETTLE_MS)).await;
            continue;
        }
        let Some(writes) = next_claude_question_prep_keys(prep, &tab).map_err(|()| lost())? else {
            return Ok(());
        };
        write_keys_and_await_repaint(project_id, session_id, zmx_name, source, &writes, &capture)
            .await?;
    }
    Err(
        "Claude's question could not be readied for the answer. Answer it in the terminal."
            .to_string(),
    )
}
