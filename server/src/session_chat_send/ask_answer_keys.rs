use super::*;

// Ask-answer keystrokes (upstream chat spec §8.4/§8.5).
const ASK_ENTER: &str = "\r";
const ASK_NEXT_TAB: &str = "\u{1b}[C"; // Right arrow → next question / Submit tab
const ASK_NEXT_ROW: &str = "\u{1b}[B"; // Down
const ASK_NOTES: &str = "\t"; // Tab → open notes (Codex)
const ASK_DELETE: &str = "\u{7f}"; // DEL — clear/skip a Codex row
const ASK_TAB: &str = "\t"; // Tab → next question tab (omp's ask dialog)
const ASK_SPACE: &str = " "; // Space → toggle a multi-select row (omp)
const ASK_PREVIEW_NOTES: &str = "n"; // n → notes on the highlighted row (Claude preview layout)

// ---------------------------------------------------------------------------
// Ask-answer keystroke builders (upstream chat spec §8.4/§8.5/§8.6)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskAnswerKeyGroup {
    /// Written verbatim (arrows, digits, Enter, Tab, DEL).
    Raw(String),
    /// Free text; goes through the paste sanitizer when written.
    Text(String),
    /// Put Codex's question overlay on this question (and row, for a note)
    /// from the screen as it is when the step runs.
    AlignCodexQuestion { question: usize, row: Option<usize> },
    /// Put an arrow-driven list (Cursor, pi, omp) on a question and row from
    /// the screen as it is when the step runs.
    AlignQuestionRow(crate::session_chat_question_row_align::QuestionRowTarget),
    /// Empty Claude's "Type something" field, set a multi-select tab's ticks
    /// and bring the highlight to the tab's first row, from the screen as it
    /// is when the step runs.
    PrepareClaudeQuestion(crate::session_chat_claude_question_prep::ClaudeQuestionPrep),
}

/// The step that puts `ui`'s list on `row` of question `question`.
pub(super) fn align_question_row(
    ui: crate::session_chat_question_row_align::QuestionListUi,
    questions: &[SessionChatQuestion],
    question: usize,
    row: usize,
) -> AskAnswerKeyGroup {
    question_row_step(
        ui,
        questions,
        question,
        row,
        crate::session_chat_question_row_align::QuestionRowAction::Move,
    )
}

/// The step that runs `action` on question `question` of `ui`'s list, leaving
/// the highlight on `row` (see QuestionRowAction for where each ends).
fn question_row_step(
    ui: crate::session_chat_question_row_align::QuestionListUi,
    questions: &[SessionChatQuestion],
    question: usize,
    row: usize,
    action: crate::session_chat_question_row_align::QuestionRowAction,
) -> AskAnswerKeyGroup {
    AskAnswerKeyGroup::AlignQuestionRow(crate::session_chat_question_row_align::QuestionRowTarget {
        ui,
        questions: questions
            .iter()
            .map(|question| question.question.clone())
            .collect(),
        question,
        labels: questions
            .get(question)
            .map(|question| {
                question
                    .options
                    .iter()
                    .map(|option| option.label.clone())
                    .collect()
            })
            .unwrap_or_default(),
        row,
        action,
    })
}

fn selection_other(selection: Option<&SessionChatQuestionSelection>) -> &str {
    selection
        .and_then(|selection| selection.other.as_deref())
        .unwrap_or_default()
        .trim()
}

fn answer_labels(
    question: &SessionChatQuestion,
    selection: Option<&SessionChatQuestionSelection>,
) -> Vec<String> {
    let mut labels: Vec<String> = selection
        .map(|selection| {
            selection
                .indices
                .iter()
                .map(|index| {
                    question
                        .options
                        .get(*index)
                        .map(|option| option.label.clone())
                        .unwrap_or_default()
                })
                .filter(|label| !label.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let other = selection_other(selection);
    if !other.is_empty() {
        labels.push(other.to_string());
    }
    labels
}

/*
Claude's AskUserQuestion is an arrow-navigate selector: a bare Enter commits
the HIGHLIGHTED default and pasted label text does NOT move the highlight
(bug STA-1860 delivered every non-first pick as the first option). Drive it
by each option's stable 1-based number, which matches the card's badge.
Groups are paced NATIVE_CHAT_QUESTION_STEP_MS apart by the queue because a
navigation keystroke batched with Enter commits before the selector applied
it. Every question's keys start with a step that reads its tab when it runs
(session_chat_claude_question_prep.rs): it empties leftover "Type something"
text, sets a multi-select tab's ticks to exactly the picked options, and puts
the highlight on the tab's first row.
*/
pub fn build_claude_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let multi_question = questions.len() > 1;
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let type_something = (question.options.len() + 1).to_string(); // the "Type something" row
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        groups.push(AskAnswerKeyGroup::PrepareClaudeQuestion(
            crate::session_chat_claude_question_prep::ClaudeQuestionPrep {
                questions: questions.to_vec(),
                question: question_index,
                // Each digit TOGGLES a checkbox, so the step ticks only the
                // rows that differ from the pick.
                ticked: question.multi_select.then(|| {
                    indices
                        .iter()
                        .copied()
                        .filter(|index| *index < question.options.len())
                        .collect()
                }),
            },
        ));
        if question.multi_select {
            if !other.is_empty() {
                /*
                CDXC:SessionChat 2026-09-23 WHY: Claude Code 2.1.280 edits the multi-select "Type something" row in place while it is highlighted: its digit only ticks the box, so text typed after it was dropped, and the Enter that followed toggled the highlighted first option off (Cheese + Peppers + a note arrived as "Peppers"). Typing on the highlighted row ticks it; Right is a caret move there, so the row below it (Next, or Submit on the last question) takes the Enter that moves on.
                */
                groups.push(AskAnswerKeyGroup::Raw(
                    ASK_NEXT_ROW.repeat(question.options.len()),
                ));
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_ROW.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            } else {
                // Multi-select never auto-advances; step to next/Submit tab.
                groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
            }
        } else if question.preview_layout {
            // A preview question has no "Type something" row; a digit moves the
            // highlight and Enter commits it (with its note, when one is typed).
            if let Some(first) = indices.first() {
                groups.push(AskAnswerKeyGroup::Raw((first + 1).to_string()));
                if !other.is_empty() {
                    groups.push(AskAnswerKeyGroup::Raw(ASK_PREVIEW_NOTES.to_string()));
                    groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                }
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            } else if multi_question {
                groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
            }
        } else if !other.is_empty() {
            // Single-select carries one value, so route ANY answer containing
            // free text through "Type something" as one joined string.
            groups.push(AskAnswerKeyGroup::Raw(type_something));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
        } else if let Some(first) = indices.first() {
            // Selects AND commits; auto-advances in multi-question.
            groups.push(AskAnswerKeyGroup::Raw((first + 1).to_string()));
        } else if multi_question {
            // Unanswered question: step past it.
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
        }
    }
    if groups
        .iter()
        .all(|group| matches!(group, AskAnswerKeyGroup::PrepareClaudeQuestion(_)))
    {
        // Nothing to answer: leave the selector as the terminal has it.
        return Vec::new();
    }
    let ends_on_submit_tab = multi_question || (questions.len() == 1 && questions[0].multi_select);
    if ends_on_submit_tab {
        // Final Submit confirmation.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/// A preview question answered with typed text alone: Claude draws no free-text row for it, so there is nothing the text could be typed into.
pub(crate) fn claude_preview_question_without_option<'a>(
    questions: &'a [SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Option<&'a SessionChatQuestion> {
    questions.iter().enumerate().find_map(|(index, question)| {
        let selection = selections.get(index);
        let has_option = selection.is_some_and(|selection| !selection.indices.is_empty());
        (question.preview_layout && !has_option && !selection_other(selection).is_empty())
            .then_some(question)
    })
}

/*
Codex's request_user_input overlay submits on the final option digit and
attaches free text as NOTES to the highlighted row. Each question keeps its
own highlight, so every question's keys start with an alignment step that
reads the screen when it runs (see session_chat_codex_question_align.rs):
it lands on the question and, for a note, on its row WITHOUT committing.
*/
pub fn build_codex_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let mut has_unanswered = false;
    let last_index = questions.len().saturating_sub(1);
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let selected_index = selection.and_then(|selection| selection.indices.first().copied());
        let note = selection_other(selection);
        // A note goes on its option's row; the notes row (one past the last
        // option) takes a note with no option.
        let note_row = (!note.is_empty()).then(|| selected_index.unwrap_or(question.options.len()));
        groups.push(AskAnswerKeyGroup::AlignCodexQuestion {
            question: question_index,
            row: note_row,
        });
        if note_row.is_some() {
            groups.push(AskAnswerKeyGroup::Raw(ASK_NOTES.to_string()));
            groups.push(AskAnswerKeyGroup::Text(note.to_string()));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(selected_index) = selected_index {
            // Digit commits.
            groups.push(AskAnswerKeyGroup::Raw((selected_index + 1).to_string()));
            continue;
        }
        has_unanswered = true;
        groups.push(AskAnswerKeyGroup::Raw(ASK_DELETE.to_string()));
        groups.push(AskAnswerKeyGroup::Raw(if question_index < last_index {
            ASK_NEXT_TAB.to_string()
        } else {
            ASK_ENTER.to_string()
        }));
    }
    if has_unanswered {
        // Codex opens a confirmation; Proceed is highlighted by default.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/*
Cursor Agent's AskQuestion panel is a checkbox list. Up/down move the
highlight, Space toggles the current option, Enter advances or submits, and
Escape skips. The final row is the free-text Other choice; typing while it is
highlighted opens the input. Rows are reached through the screen-reading
alignment step, not counted from where the list opened.
*/
pub fn build_cursor_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if indices.is_empty() && other.is_empty() {
            groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
            break;
        }

        let ui = crate::session_chat_question_row_align::QuestionListUi::Cursor;
        let other_row = question.options.len();
        let picked: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|index| *index < question.options.len())
            .collect();
        // Other text left in the terminal would be joined to the typed answer
        // or submitted beside the picked option.
        groups.push(question_row_step(
            ui,
            questions,
            question_index,
            other_row,
            QuestionRowAction::ClearText,
        ));
        if question.multi_select {
            // Space toggles, so the step ticks only rows that differ from the
            // pick. Enter ticks the highlighted row, so it rests on a picked
            // row, or on Other when it takes the typed answer.
            let rest = match picked.first() {
                Some(first) if other.is_empty() => *first,
                _ => other_row,
            };
            groups.push(question_row_step(
                ui,
                questions,
                question_index,
                rest,
                QuestionRowAction::SetTicks(picked),
            ));
        } else {
            for index in picked {
                groups.push(align_question_row(ui, questions, question_index, index));
                groups.push(AskAnswerKeyGroup::Raw(ASK_SPACE.to_string()));
            }
        }
        if !other.is_empty() {
            groups.push(align_question_row(ui, questions, question_index, other_row));
            groups.push(AskAnswerKeyGroup::Text(other.to_string()));
        }
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/*
Pi's cursor_ask_question renders pi-tui selects (ctx.ui.select): ↑/↓ move the
highlight (wrapping), Enter commits, Esc/Ctrl+C cancels — no digit shortcuts
and no skip. Questions show ONE AT A TIME, so the groups for question N+1 only
land after question N's Enter; the queue's per-group pacing covers the repaint.
A question that allows a custom answer appends one "Type a custom answer" row
after its options, and committing that row opens a one-line input
(ctx.ui.input) that submits on Enter. An optionless question shows that bare
input directly. Cancelling any question ends pi's whole question loop, so an
unanswered question emits the cancel and nothing after it. Rows are reached
through the screen-reading alignment step, not counted from the first row.
*/
pub fn build_pi_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let selected_index = selection.and_then(|selection| selection.indices.first().copied());
        let allows_custom = question.allow_custom != Some(false);
        if question.options.is_empty() {
            let answer = answer_labels(question, selection).join(", ");
            if answer.is_empty() {
                groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
                break;
            }
            groups.push(AskAnswerKeyGroup::Text(answer));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if !other.is_empty() && allows_custom {
            // Single-value answer: any picked labels join the free text as one
            // string through the custom-answer input (the Claude single-select
            // rule). The custom row sits one past the last option.
            groups.push(align_question_row(
                crate::session_chat_question_row_align::QuestionListUi::Pi,
                questions,
                question_index,
                question.options.len(),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(index) = selected_index {
            groups.push(align_question_row(
                crate::session_chat_question_row_align::QuestionListUi::Pi,
                questions,
                question_index,
                index,
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
        break;
    }
    groups
}

/*
Hermes' clarify panel numbers every row: in single-select mode a digit (1-9,
then 0 for the 10th row) SUBMITS that choice directly — and in batch
(multi-question) mode it locks the active question's answer and auto-advances
to the next unanswered one, so the same digit sequence drives both layouts.
The row one past the last choice is "Other (type your answer)": its digit
switches the composer into freetext mode, where typed text + Enter submits
(or locks, in batch mode). Multi-select rows are checkboxes — digits TOGGLE
and Enter confirms; checking Other routes through freetext the same way.
There is no skip key and Esc is not bound to the panel, so an unanswered
question simply stops the key plan and leaves the panel to the terminal.
*/
pub fn build_hermes_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    // Panel row digit for 0-based row `index`: 1-9, then 0 for the 10th row.
    // Clarify caps choices at 4, so Other is at worst row 5 — the guards only
    // matter if that cap ever moves.
    fn row_digit(index: usize) -> Option<String> {
        match index {
            0..=8 => Some((index + 1).to_string()),
            9 => Some("0".to_string()),
            _ => None,
        }
    }
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if question.options.is_empty() {
            // Open-ended question: the panel is already in freetext mode.
            let answer = answer_labels(question, selection).join(", ");
            if answer.is_empty() {
                break;
            }
            groups.push(AskAnswerKeyGroup::Text(answer));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        let other_row = question.options.len();
        if question.multi_select {
            if indices.is_empty() && other.is_empty() {
                break;
            }
            for index in indices {
                let Some(digit) = row_digit(*index) else {
                    continue;
                };
                // Each digit TOGGLES a checkbox.
                groups.push(AskAnswerKeyGroup::Raw(digit));
            }
            if !other.is_empty() {
                let Some(digit) = row_digit(other_row) else {
                    break;
                };
                groups.push(AskAnswerKeyGroup::Raw(digit)); // check Other
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // → freetext
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if !other.is_empty() {
            // Single-value answer: any picked labels join the free text as one
            // string through Other's freetext (the Claude single-select rule).
            let Some(digit) = row_digit(other_row) else {
                break;
            };
            groups.push(AskAnswerKeyGroup::Raw(digit));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(digit) = indices.first().copied().and_then(row_digit) {
            // Submits (single) / locks and advances (batch).
            groups.push(AskAnswerKeyGroup::Raw(digit));
            continue;
        }
        break;
    }
    groups
}

/// The text Hermes' gateway takes as the answer to a one-question clarify (`clarify.respond`
/// frame, CDXC:HermesGateway 2026-11-05 in hermes_gateway.rs): the picked choices and any typed
/// text, joined the way the panel's Other freetext joins them. `None` for a batch or no answer.
pub fn hermes_clarify_answer_text(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Option<String> {
    let [question] = questions else {
        return None;
    };
    let answer = answer_labels(question, selections.first()).join(", ");
    (!answer.trim().is_empty()).then_some(answer)
}

/*
CDXC:SessionChat 2026-10-01 WHY: Antigravity's ask_question panel (agy 1.2.14) shows one
question at a time (`Question 1/3: …`) and its highlight starts on the first row of every
question. Rows are numbered and a digit is an absolute address: on a single-select question it
picks the row and advances (on the last question it submits everything); on a multi-select
question it toggles the row. The row one past the last option is "Write-in...", whose digit
opens a one-line answer field where Enter commits the question the same way. A multi-select
question is left with → (Enter only on the last, where it reads "Submit All"), and Esc skips a
question, recorded as `User Skipped`. Digits stop at 9, so a row past the ninth is reached with
arrows from wherever the last digit left the highlight. Typing the answer into the composer
instead (the old fallback) did nothing: the panel owns the input while it is open.
*/
pub fn build_antigravity_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    const ASK_UP: &str = "\u{1b}[A";
    const ASK_SKIP: &str = "\u{1b}";
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let last = question_index + 1 == questions.len();
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices: Vec<usize> = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(|index| *index < question.options.len())
            .collect();
        let write_in_row = question.options.len();
        // Where the highlight sits; every question opens on its first row.
        let mut cursor = 0usize;
        // Puts the highlight on `row`; `then` is the key that acts on it when it had to be
        // reached by arrows (a digit acts by itself).
        let mut reach = |groups: &mut Vec<AskAnswerKeyGroup>, row: usize, then: &str| {
            if row < 9 {
                groups.push(AskAnswerKeyGroup::Raw((row + 1).to_string()));
            } else {
                let key = if row > cursor { ASK_NEXT_ROW } else { ASK_UP };
                groups.push(AskAnswerKeyGroup::Raw(key.repeat(row.abs_diff(cursor))));
                groups.push(AskAnswerKeyGroup::Raw(then.to_string()));
            }
            cursor = row;
        };
        if indices.is_empty() && other.is_empty() {
            groups.push(AskAnswerKeyGroup::Raw(ASK_SKIP.to_string()));
            continue;
        }
        if question.multi_select {
            for index in &indices {
                reach(&mut groups, *index, ASK_SPACE);
            }
            if !other.is_empty() {
                reach(&mut groups, write_in_row, ASK_ENTER);
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            } else if last {
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            } else {
                groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
            }
            continue;
        }
        if !other.is_empty() {
            // Single-value answer: picked labels join the write-in text as one string (the
            // Claude single-select rule).
            reach(&mut groups, write_in_row, ASK_ENTER);
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        reach(&mut groups, indices[0], ASK_ENTER);
    }
    groups
}

/// Whether Antigravity's ask_question panel is on screen at its first question, which is where
/// the key plan starts. Esc on a screen without the panel would interrupt the agent's turn.
pub fn antigravity_question_panel_at_start(
    questions: &[SessionChatQuestion],
    screen_text: &str,
) -> bool {
    let header = format!("Question 1/{}:", questions.len());
    screen_text
        .lines()
        .any(|line| line.trim_start().starts_with(&header))
        && screen_text.lines().any(|line| line.contains("esc Skip"))
}

/*
omp's built-in `ask` tool opens its rich dialog (one tab per question plus a
Submit tab whenever there is more than one question or any multi question).
The cursor opens on the `recommended` row and ↑/↓ move it WITHOUT wrapping;
rows are the options in order plus a trailing "Other (type your own)" row,
reached through the screen-reading alignment step. Single-select Enter picks
the row and auto-advances (submitting a single-question dialog directly);
Enter on Other opens a custom-answer prompt that submits on Enter. Multi-select Space
toggles, and the plan advances with Tab instead of Enter because Enter on the
Other row would re-open the prompt. The final Enter confirms the Submit tab.
Esc cancels the whole dialog, so it is only sent for an unanswered
single-question dialog, which has no tab to step past.
*/
pub fn build_omp_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let ui = crate::session_chat_question_row_align::QuestionListUi::Omp;
    let has_submit_tab =
        questions.len() > 1 || questions.iter().any(|question| question.multi_select);
    let mut cancelled = false;
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if indices.is_empty() && other.is_empty() {
            if has_submit_tab {
                if question.multi_select {
                    // Rows the terminal ticked would still answer it.
                    let other_row = question.options.len();
                    groups.push(question_row_step(
                        ui,
                        questions,
                        question_index,
                        other_row,
                        QuestionRowAction::UntickFreeRow,
                    ));
                    groups.push(question_row_step(
                        ui,
                        questions,
                        question_index,
                        0,
                        QuestionRowAction::SetTicks(Vec::new()),
                    ));
                }
                // Skip: step to the next question tab, leaving no answer.
                groups.push(AskAnswerKeyGroup::Raw(ASK_TAB.to_string()));
                continue;
            }
            groups.push(AskAnswerKeyGroup::Raw(SESSION_CHAT_INTERRUPT.to_string()));
            cancelled = true;
            break;
        }
        let other_row = question.options.len();
        if question.multi_select {
            let picked: Vec<usize> = indices
                .iter()
                .copied()
                .filter(|index| *index < question.options.len())
                .collect();
            // An Other answer left in the terminal is dropped first; the
            // chat's own text goes in below.
            groups.push(question_row_step(
                ui,
                questions,
                question_index,
                other_row,
                QuestionRowAction::UntickFreeRow,
            ));
            // Space toggles, so the step ticks only rows that differ from
            // the pick.
            groups.push(question_row_step(
                ui,
                questions,
                question_index,
                picked.first().copied().unwrap_or(other_row),
                QuestionRowAction::SetTicks(picked),
            ));
            if !other.is_empty() {
                groups.push(align_question_row(ui, questions, question_index, other_row));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // open the prompt
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // prompt submits
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_TAB.to_string()));
            continue;
        }
        if !other.is_empty() {
            // Single-value answer: picked labels join the free text as one
            // string through the custom-answer prompt (the Claude rule). The
            // prompt reopens holding an earlier answer, so it is emptied first.
            groups.push(align_question_row(ui, questions, question_index, other_row));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // open the prompt
            groups.push(question_row_step(
                ui,
                questions,
                question_index,
                other_row,
                QuestionRowAction::ClearText,
            ));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // submit + advance
            continue;
        }
        let index = (*indices.first().expect("indices checked non-empty")).min(other_row);
        // A note the terminal left on the picked row would go with it.
        let action = if index < other_row {
            QuestionRowAction::DropNote
        } else {
            QuestionRowAction::Move
        };
        groups.push(question_row_step(
            ui,
            questions,
            question_index,
            index,
            action,
        ));
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string())); // pick + advance/submit
    }
    if has_submit_tab && !cancelled && !groups.is_empty() {
        // Final Submit confirmation.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/// Non-stepping agents (Grok): one line per question, IN ORDER; empty answers
/// stay empty lines so N lines === N questions.
pub fn format_ask_answer(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> String {
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| answer_labels(question, selections.get(index)).join(", "))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn has_ask_answer(selections: &[SessionChatQuestionSelection]) -> bool {
    selections.iter().any(|selection| {
        !selection.indices.is_empty()
            || !selection
                .other
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
    })
}
