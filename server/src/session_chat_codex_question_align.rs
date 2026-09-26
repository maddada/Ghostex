//! Puts Codex's request_user_input overlay on the question and row an answer
//! step expects, from the screen as it is when the step runs.

use std::time::Duration;

use crate::session_chat_send::{capture_session_terminal_text, write_session_chat_payload};

const PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const NEXT_ROW: &str = "\u{1b}[B"; // Down
const PREVIOUS_QUESTION: &str = "\u{1b}[D"; // Left
const NEXT_QUESTION: &str = "\u{1b}[C"; // Right
const CLOSE_NOTES: &str = "\t"; // Tab clears and closes the notes field
const SETTLE_MS: u64 = 250;
const MAX_ATTEMPTS: usize = 6;

/// What the overlay shows right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CodexQuestionScreen {
    /// 0-based question on screen.
    pub question: usize,
    /// Rows the question offers, "None of the above" included.
    pub rows: usize,
    /// 0-based highlighted row.
    pub highlighted: usize,
    /// The notes field is open and takes every typed key.
    pub notes_open: bool,
}

fn screen_lines(screen: &str) -> Vec<String> {
    screen
        .lines()
        .map(|line| {
            crate::session_chat_options::strip_ansi_sgr(line)
                .trim()
                .trim_matches('│')
                .trim()
                .to_string()
        })
        .collect()
}

fn question_counter(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("Question ")?;
    let (current, rest) = rest.split_once('/')?;
    let total: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let (current, total) = (current.parse::<usize>().ok()?, total.parse::<usize>().ok()?);
    (current >= 1 && current <= total).then_some((current - 1, total))
}

/// Reads the live overlay (a bottom pane, gone from the screen once answered).
/// `None` when the screen shows no question overlay with a highlighted row.
pub fn read_codex_question_screen(screen: &str) -> Option<CodexQuestionScreen> {
    let lines = screen_lines(screen);
    let header = lines
        .iter()
        .rposition(|line| question_counter(line).is_some())?;
    let (question, _) = question_counter(&lines[header])?;
    let mut rows = 0;
    let mut highlighted = None;
    let mut notes_open = false;
    for line in &lines[header + 1..] {
        let marked = line.starts_with('›');
        let row = line.trim_start_matches('›').trim_start();
        let number = row
            .split_once(". ")
            .and_then(|(number, _)| number.parse::<usize>().ok());
        if number == Some(rows + 1) {
            if marked {
                highlighted = Some(rows);
            }
            rows += 1;
        } else if line.contains("to clear notes") {
            notes_open = true;
        }
    }
    Some(CodexQuestionScreen {
        question,
        rows,
        highlighted: highlighted?,
        notes_open,
    })
}

/// Keys for the shortest move between rows; the list wraps both ways.
fn row_keys(from: usize, to: usize, rows: usize) -> String {
    let down = (to + rows - from) % rows;
    let up = (from + rows - to) % rows;
    if up < down {
        PREVIOUS_ROW.repeat(up)
    } else {
        NEXT_ROW.repeat(down)
    }
}

/// The one write that brings the overlay closer to `question`/`row`, or
/// `None` once it is there.
pub fn next_codex_question_keys(
    screen: &CodexQuestionScreen,
    question: usize,
    row: Option<usize>,
) -> Option<String> {
    if screen.notes_open {
        // Left/Right and digits are text while notes are open.
        return Some(CLOSE_NOTES.to_string());
    }
    if screen.question != question {
        return Some(if question < screen.question {
            PREVIOUS_QUESTION.repeat(screen.question - question)
        } else {
            NEXT_QUESTION.repeat(question - screen.question)
        });
    }
    row.filter(|row| *row < screen.rows && *row != screen.highlighted)
        .map(|row| row_keys(screen.highlighted, row, screen.rows))
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: "make gxserver move the highlight back before answering", and later the same day: notes the user already typed in the terminal must be cleared before the chat's answer goes in. Codex keeps each question's own highlight while the user moves between questions, and its notes field swallows digits, so an answer plan written ahead of time cannot know where a later question's highlight is (a note meant for Green landed on Red). Each question's keys therefore start with this step, which reads the overlay when the step runs, closes an open notes field, moves to the question the plan expects and, for a note, to the row it belongs to. Closing is also the clear: Codex 0.156 shows a note left on a question as an open field again when the user returns to it, and Tab (twice when the field is shown but not focused) empties it, so a terminal note is never submitted beside or in front of the chat's. Codex asks single-select questions only, so there are no ticks to reconcile. A screen it cannot read stops the answer instead of typing blind.
/// SEE-ALSO: build_codex_ask_answer_keys in server/src/session_chat_send.rs.
pub(crate) async fn align_codex_question(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    question: usize,
    row: Option<usize>,
    superseded: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    for _ in 0..MAX_ATTEMPTS {
        if superseded() {
            return Err("The answer was superseded before it reached Codex.".to_string());
        }
        let screen = capture_session_terminal_text(zmx_name)
            .await
            .as_deref()
            .and_then(read_codex_question_screen)
            .ok_or_else(|| {
                "Codex's question is no longer on screen, so the answer was not sent.".to_string()
            })?;
        let Some(keys) = next_codex_question_keys(&screen, question, row) else {
            return Ok(());
        };
        write_session_chat_payload(project_id, session_id, zmx_name, source, &keys).await?;
        tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;
    }
    Err("Codex's question did not move to the answer's row. Answer it in the terminal.".to_string())
}
