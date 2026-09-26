//! Puts an arrow-driven question list (Cursor Agent's AskQuestion, pi's
//! cursor_ask_question select, omp's ask dialog) on the question and row an
//! answer step expects, from the screen as it is when the step runs.

use std::time::Duration;

use crate::session_chat_send::{capture_session_terminal_text, write_session_chat_payload};

const PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const NEXT_ROW: &str = "\u{1b}[B"; // Down
const PREVIOUS_QUESTION: &str = "\u{1b}[D"; // Left
const NEXT_QUESTION: &str = "\u{1b}[C"; // Right
const TOGGLE: &str = " "; // Space ticks or unticks the highlighted row
const OPEN_CUSTOM_PROMPT: &str = "\r"; // Enter on omp's Other row
const OPEN_NOTE_PROMPT: &str = "n"; // n on an omp single-select row
const SUBMIT_CUSTOM_PROMPT: &str = "\r";
/// Ctrl+E then Ctrl+U: end of the line, then delete back to its start. Each
/// is its own write because Ink reads a burst as one unknown key.
pub(crate) const CLEAR_LINE_KEYS: [&str; 2] = ["\u{5}", "\u{15}"];
const SETTLE_MS: u64 = 250;
const KEY_GAP_MS: u64 = 80;
const REPAINT_POLL_MS: u64 = 100;
const REPAINT_POLLS: usize = 12;
/// Moves, toggles and one clear per wrapped line of leftover text.
const MAX_ATTEMPTS: usize = 24;

/// Which agent's list is on screen: they differ in the highlight marker and in
/// whether the user can move between questions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionListUi {
    /// `› [ ] Red`, ←/→ between questions.
    Cursor,
    /// `→ Red`, one question at a time.
    Pi,
    /// `❯ ○ Red`, ←/→ (or Tab) between question tabs.
    Omp,
}

impl QuestionListUi {
    fn marker(self) -> char {
        match self {
            QuestionListUi::Cursor => '›',
            QuestionListUi::Pi => '→',
            QuestionListUi::Omp => '❯',
        }
    }

    fn moves_between_questions(self) -> bool {
        !matches!(self, QuestionListUi::Pi)
    }
}

/// What the step does once the list shows the target question.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum QuestionRowAction {
    /// Put the highlight on `row`.
    #[default]
    Move,
    /// Leave exactly these options ticked (Space toggles each row that
    /// differs), then put the highlight on `row`.
    SetTicks(Vec<usize>),
    /// Empty the free-text row: Cursor's inline Other text, or omp's custom
    /// answer prompt, which the step before opened.
    ClearText,
    /// omp multi-select: untick the Other row (and drop its text) when the
    /// terminal left it ticked.
    UntickFreeRow,
    /// omp single-select: put the highlight on `row` and drop a note the
    /// terminal attached to it.
    DropNote,
}

/// Where an answer step needs the list: `row` is an option index, or
/// `labels.len()` for the free-text row after the options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionRowTarget {
    pub ui: QuestionListUi,
    /// Every question's text, in order, to tell which one is on screen.
    pub questions: Vec<String>,
    pub question: usize,
    pub labels: Vec<String>,
    pub row: usize,
    pub action: QuestionRowAction,
}

/// What the list shows right now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestionListScreen {
    pub question: usize,
    /// Read only on the target question: the labels belong to it.
    pub highlighted: Option<usize>,
    /// Per option, then the free-text row: whether its box is ticked, `None`
    /// when the row is not on screen or draws no box.
    pub ticked: Vec<Option<bool>>,
    /// The free-text row carries text the user typed (Cursor draws it inline).
    pub free_text: bool,
    /// Per option: omp marks the row holding a note with "✎ note".
    pub noted: Vec<bool>,
}

fn compact(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn screen_lines(screen: &str) -> Vec<String> {
    screen
        .lines()
        .map(|line| {
            crate::session_chat_options::strip_ansi_sgr(line)
                .trim()
                .trim_matches(['│', '┃'])
                .trim()
                .to_string()
        })
        .collect()
}

/// The row text without its checkbox or radio glyph.
fn row_text(line: &str) -> &str {
    let mut text = line.trim_start();
    loop {
        if let Some(rest) = text.strip_prefix('[').and_then(|rest| {
            let (inside, rest) = rest.split_once(']')?;
            (inside.chars().count() <= 1).then_some(rest)
        }) {
            text = rest.trim_start();
        } else if let Some(rest) = text.strip_prefix(['○', '●', '◉', '◯', '☐', '☑', '☒', '✓', '✔'])
        {
            text = rest.trim_start();
        } else {
            return text;
        }
    }
}

/// Whether the row's box is ticked, `None` when it draws none.
fn row_ticked(line: &str) -> Option<bool> {
    let text = line.trim_start();
    if let Some(rest) = text.strip_prefix('[') {
        let (inside, _) = rest.split_once(']')?;
        return match inside.trim() {
            "" => Some(false),
            "x" | "X" | "✓" | "✔" | "*" => Some(true),
            _ => None,
        };
    }
    match text.chars().next()? {
        '☐' | '○' | '◯' => Some(false),
        '☑' | '☒' | '✓' | '✔' | '●' | '◉' => Some(true),
        _ => None,
    }
}

/// Cursor draws typed Other text inline ("Other: my text"), and the
/// placeholder "Other: (type to answer)" when there is none.
fn cursor_free_row_has_text(text: &str) -> bool {
    text.split_once(':').is_some_and(|(_, rest)| {
        let rest = rest.trim();
        !rest.is_empty() && rest != "(type to answer)"
    })
}

fn question_on_line(line: &str, question: &str) -> bool {
    // Cursor numbers its questions ("1. Which color?").
    let line = line
        .split_once(". ")
        .filter(|(number, _)| number.parse::<usize>().is_ok())
        .map_or(line, |(_, rest)| rest);
    let (line, question) = (compact(line), compact(question));
    !line.is_empty() && (line == question || (line.len() >= 16 && question.starts_with(&line)))
}

/// Reads the live list. `None` when no question of `target` is on screen or
/// its highlight cannot be placed on an option or the free-text row.
pub fn read_question_list_screen(
    target: &QuestionRowTarget,
    screen: &str,
) -> Option<QuestionListScreen> {
    let lines = screen_lines(screen);
    // Earlier questions can still be on screen (omp previews them all in its
    // tool call); the live one is the lowest.
    let (heading, question) = target
        .questions
        .iter()
        .enumerate()
        .filter_map(|(index, question)| {
            lines
                .iter()
                .rposition(|line| question_on_line(line, question))
                .map(|line| (line, index))
        })
        .max()?;
    if question != target.question {
        return Some(QuestionListScreen {
            question,
            ..QuestionListScreen::default()
        });
    }
    let labels: Vec<String> = target.labels.iter().map(|label| compact(label)).collect();
    let marker = target.ui.marker();
    let mut screen = QuestionListScreen {
        question,
        ticked: vec![None; labels.len() + 1],
        noted: vec![false; labels.len()],
        ..QuestionListScreen::default()
    };
    let mut seen_option = false;
    let mut free_row_seen = false;
    for line in &lines[heading + 1..] {
        let marked = line.starts_with(marker);
        let unmarked = line.trim_start_matches(marker).trim_start();
        let raw_text = row_text(unmarked);
        let text = compact(raw_text);
        // The longest label wins, so "Blue green" is not read as "Blue".
        let option = labels
            .iter()
            .enumerate()
            .filter(|(_, label)| !label.is_empty() && text.starts_with(label.as_str()))
            .max_by_key(|(_, label)| label.len())
            .map(|(index, _)| index);
        let ticked = row_ticked(unmarked);
        // The free-text row follows the options and draws a box (or is the
        // highlighted row, for lists that draw none).
        let free_row =
            option.is_none() && seen_option && !free_row_seen && (marked || ticked.is_some());
        if let Some(index) = option {
            screen.ticked[index] = ticked;
            screen.noted[index] = target.ui == QuestionListUi::Omp && raw_text.ends_with("✎ note");
        } else if free_row {
            free_row_seen = true;
            screen.ticked[labels.len()] = ticked;
            screen.free_text =
                target.ui == QuestionListUi::Cursor && cursor_free_row_has_text(raw_text);
        }
        if marked {
            screen.highlighted = Some(match option {
                Some(index) => index,
                None if free_row => labels.len(),
                None => return None,
            });
        }
        seen_option |= option.is_some();
        if free_row_seen {
            break;
        }
    }
    screen.highlighted.is_some().then_some(screen)
}

/// Writes `writes` in order, then waits until the screen differs from
/// `before` (or a short timeout passes), so the next read cannot act on a
/// stale frame: a toggle read twice would be undone.
pub(crate) async fn write_keys_and_await_repaint(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    writes: &[String],
    before: &str,
) -> Result<(), String> {
    for (index, keys) in writes.iter().enumerate() {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(KEY_GAP_MS)).await;
        }
        write_session_chat_payload(project_id, session_id, zmx_name, source, keys).await?;
    }
    tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;
    for _ in 0..REPAINT_POLLS {
        if capture_session_terminal_text(zmx_name)
            .await
            .is_some_and(|screen| screen != before)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(REPAINT_POLL_MS)).await;
    }
    Ok(())
}

fn arrows_to(highlighted: usize, row: usize) -> Option<Vec<String>> {
    match row.cmp(&highlighted) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(vec![PREVIOUS_ROW.repeat(highlighted - row)]),
        std::cmp::Ordering::Greater => Some(vec![NEXT_ROW.repeat(row - highlighted)]),
    }
}

/// The writes that bring the list closer to `target`, or `None` once it is
/// there. `Err` when the list shows another question it cannot leave, or a
/// row the action needs is not on screen.
pub fn next_question_row_keys(
    target: &QuestionRowTarget,
    screen: &QuestionListScreen,
) -> Result<Option<Vec<String>>, ()> {
    if screen.question != target.question {
        if !target.ui.moves_between_questions() {
            return Err(());
        }
        return Ok(Some(vec![if target.question < screen.question {
            PREVIOUS_QUESTION.repeat(screen.question - target.question)
        } else {
            NEXT_QUESTION.repeat(target.question - screen.question)
        }]));
    }
    let highlighted = screen.highlighted.ok_or(())?;
    let free_row = target.labels.len();
    let keys = |keys: &[&str]| Some(keys.iter().map(|key| key.to_string()).collect());
    Ok(match &target.action {
        QuestionRowAction::Move => arrows_to(highlighted, target.row),
        QuestionRowAction::SetTicks(wanted) => {
            for row in 0..free_row {
                let ticked = screen.ticked.get(row).copied().flatten().ok_or(())?;
                // omp drops a row's note when it is unticked, so a picked row
                // holding a terminal note is unticked once and ticked again.
                let noted = screen.noted.get(row).copied().unwrap_or(false);
                if ticked != (wanted.contains(&row) && !(ticked && noted)) {
                    return Ok(if highlighted == row {
                        keys(&[TOGGLE])
                    } else {
                        arrows_to(highlighted, row)
                    });
                }
            }
            arrows_to(highlighted, target.row)
        }
        QuestionRowAction::ClearText if !screen.free_text => None,
        QuestionRowAction::ClearText if highlighted == free_row => keys(&CLEAR_LINE_KEYS),
        QuestionRowAction::ClearText => arrows_to(highlighted, free_row),
        QuestionRowAction::UntickFreeRow if screen.ticked.get(free_row) != Some(&Some(true)) => {
            None
        }
        QuestionRowAction::UntickFreeRow if highlighted == free_row => keys(&[OPEN_CUSTOM_PROMPT]),
        QuestionRowAction::UntickFreeRow => arrows_to(highlighted, free_row),
        QuestionRowAction::DropNote if highlighted != target.row => {
            arrows_to(highlighted, target.row)
        }
        QuestionRowAction::DropNote if screen.noted.get(target.row) == Some(&true) => {
            keys(&[OPEN_NOTE_PROMPT])
        }
        QuestionRowAction::DropNote => None,
    })
}

/// omp's custom-answer or note prompt ("Custom answer: <question>" or
/// "Note for <option>: <question>" over "> text"): `Some(true)` when it holds
/// text, `None` when neither is open.
pub fn read_omp_custom_prompt(screen: &str) -> Option<bool> {
    let lines = screen_lines(screen);
    let header = lines.iter().rposition(|line| {
        line.contains("Custom answer:") || (line.contains("Note for ") && line.contains(':'))
    })?;
    let legend = header
        + lines[header..]
            .iter()
            .position(|line| line.contains("esc cancel"))?;
    // The question list drawn again below means the prompt has closed.
    if lines[header..].iter().any(|line| line.contains("↑/↓ move")) {
        return None;
    }
    let body = &lines[header + 1..legend];
    let input = body.iter().position(|line| line.starts_with('>'))?;
    Some(body[input..].iter().enumerate().any(|(index, line)| {
        let text = if index == 0 {
            &line[1..]
        } else {
            line.as_str()
        };
        !text.trim().is_empty()
    }))
}

/// CDXC:SessionChat 2026-09-25 DECISION:
/// User: "make gxserver move the highlight back before answering", and later the same day: when the user already typed text into the free-text field in the terminal, clear it before typing, and "Space toggles options", so set each row to exactly the chat's selection instead of toggling. Cursor, pi and omp lists are driven with arrows, so every row these plans act on is reached through this step, which reads the list when the step runs and moves by the exact difference, never across the wrap. Multi-select plans tick only rows that are not ticked and untick rows the terminal ticked that the chat did not pick. Leftover free text is emptied first: Cursor's inline Other text and omp's custom-answer prompt (which reopens holding the earlier answer) are cleared with Ctrl+E then Ctrl+U until the screen shows them empty, and omp's Other row is unticked by submitting that prompt empty. A note the terminal left on an omp row the chat picks is dropped too (unticking a multi-select row drops its note; single-select submits the note prompt empty, which omp still reports as an empty note). Cursor has no key that unticks its Other row, so an emptied Other stays ticked there. A screen it cannot read stops the answer instead of typing blind.
/// SEE-ALSO: build_cursor_ask_answer_keys, build_pi_ask_answer_keys and build_omp_ask_answer_keys in server/src/session_chat_send.rs.
pub(crate) async fn align_question_row(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    target: &QuestionRowTarget,
    superseded: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), String> {
    let lost =
        || "The agent's question is no longer on screen, so the answer was not sent.".to_string();
    let clear_line = || CLEAR_LINE_KEYS.iter().map(|key| key.to_string()).collect();
    // omp edits free text in a prompt that replaces the list while it is open.
    let omp_prompt_action = target.ui == QuestionListUi::Omp
        && matches!(
            target.action,
            QuestionRowAction::ClearText
                | QuestionRowAction::UntickFreeRow
                | QuestionRowAction::DropNote
        );
    let mut prompt_waits = 0;
    for _ in 0..MAX_ATTEMPTS {
        if superseded() {
            return Err("The answer was superseded before it reached the agent.".to_string());
        }
        let capture = capture_session_terminal_text(zmx_name)
            .await
            .ok_or_else(lost)?;
        let prompt = omp_prompt_action
            .then(|| read_omp_custom_prompt(&capture))
            .flatten();
        let writes = match (prompt, &target.action) {
            (Some(true), _) => Some(clear_line()),
            // An empty answer unticks Other; an empty note is no note.
            (Some(false), QuestionRowAction::UntickFreeRow | QuestionRowAction::DropNote) => {
                Some(vec![SUBMIT_CUSTOM_PROMPT.to_string()])
            }
            (Some(false), _) => None,
            (None, QuestionRowAction::ClearText) if omp_prompt_action => {
                // The Enter that opens the prompt may not have repainted yet.
                prompt_waits += 1;
                if prompt_waits > 3 {
                    return Err(lost());
                }
                tokio::time::sleep(Duration::from_millis(SETTLE_MS)).await;
                continue;
            }
            (None, _) => {
                let screen = read_question_list_screen(target, &capture).ok_or_else(lost)?;
                next_question_row_keys(target, &screen).map_err(|()| lost())?
            }
        };
        let Some(writes) = writes else {
            return Ok(());
        };
        write_keys_and_await_repaint(project_id, session_id, zmx_name, source, &writes, &capture)
            .await?;
    }
    Err(
        "The agent's question did not move to the answer's row. Answer it in the terminal."
            .to_string(),
    )
}
