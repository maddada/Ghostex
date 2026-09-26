//! Family c's part of the document: `prompt`, `terminalNotice`, `questionCard`, `asyncQuestions`
//! and the two notice flags.
//!
//! Port of the question, notice and async-strip slice of `publish` in
//! `packages/shared/session-chat-controller/native-host.ts`.

use ghostex_gx_protocol::Tri;
use serde_json::{json, Value};

use crate::document::{Document, QuestionCard, QuestionDraft};
use crate::questions::async_controller;
use crate::questions::gates;
use crate::questions::interactive::question_answer_controls;
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_choices::{collapsed_choice_label, COLLAPSED_CHOICE_COUNT};
use crate::questions::terminal_prompts::{
    terminal_dialog_presentation, terminal_notice_action_answer, terminal_notice_choice_answer,
};
use crate::state::{ChatContext, ChatState};

/// Writes family c's keys into `into`.
///
/// `prompt` and `terminalNotice` are folded by family a with CLEARED-on-omission semantics; this
/// is where they are projected for drawing. `prompt` goes out exactly as it was folded, and
/// `terminalNotice` is the folded value plus the decorations the card draws, added onto the
/// original object so a field this build does not model still reaches the renderer.
pub fn document(state: &ChatState, _context: &ChatContext, into: &mut Document) {
    let questions = &state.questions;
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());

    into.prompt = match state.session.prompt.as_ref() {
        Some(value) => Tri::Value(value.clone()),
        None => Tri::Null,
    };
    into.terminal_notice = match (state.session.terminal_notice.as_ref(), notice.as_ref()) {
        (Some(raw), Some(notice)) => Tri::Value(decorate_notice(raw, notice)),
        _ => Tri::Null,
    };
    into.notice_visible = gates::notice_visible(state);
    into.notice_error = match questions.notice_error.as_ref() {
        Some(error) => Tri::Value(error.clone()),
        None => Tri::Absent,
    };

    let card = gates::card_projection(state, prompt.as_ref());
    into.question_card = QuestionCard {
        visible: gates::question_card_visible(state, prompt.as_ref(), notice.as_ref()),
        question_index: card.index as u32,
        controls: question_answer_controls(
            &card.drafts,
            card.index,
            prompt
                .as_ref()
                .filter(|prompt| prompt.is_question())
                .map(|prompt| prompt.questions().len())
                .unwrap_or(0),
            card.answering,
        ),
        drafts: card.drafts,
        answering: card.answering,
        busy: card.answering || card.transition,
        loading: card.loading,
        approval_ask: match prompt.as_ref() {
            Some(InteractivePrompt::Approval { tool, .. }) => approval_ask(tool).to_string(),
            _ => String::new(),
        },
    };

    into.async_questions = async_controller::project(
        &questions.async_questions,
        &state.messages.composed,
        gates::async_questions_can_send(state, prompt.as_ref(), notice.as_ref()),
        gates::working(state),
        &state.session.retired_async_question_ids,
    );
}

/// What an approval card asks, by the kind of tool waiting: a file write is an edit, not a command.
fn approval_ask(tool: &str) -> &'static str {
    match crate::transcript::tool_rows::tool_glyph(tool) {
        "edit" => "Allow this edit?",
        "terminal" => "Allow this command?",
        "web" => "Allow this request?",
        "file" => "Allow reading this?",
        _ => "Allow this tool?",
    }
}

/// The folded notice plus what the card needs: the collapsed row count, the dialog's own
/// presentation, and the answer each row and each action sends.
fn decorate_notice(raw: &Value, notice: &TerminalNotice) -> Value {
    let mut object = raw.as_object().cloned().unwrap_or_default();
    object.insert(
        "collapsedChoiceCount".to_string(),
        json!(COLLAPSED_CHOICE_COUNT),
    );
    // The visible row Escape answers, so the card puts its hint on that row.
    object.insert(
        "secondaryChoice".to_string(),
        json!(notice.secondary_choice_position()),
    );
    match notice.dialog.as_ref() {
        Some(dialog) => {
            let mut decorated = raw
                .get("dialog")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            decorated.insert(
                "presentation".to_string(),
                terminal_dialog_presentation(dialog),
            );
            object.insert("dialog".to_string(), Value::Object(decorated));
        }
        None => {
            object.remove("dialog");
        }
    }
    match notice.choices.as_ref() {
        Some(choices) => {
            let raw_choices = raw.get("choices").and_then(Value::as_array);
            let rows: Vec<Value> = choices
                .iter()
                .enumerate()
                .filter(|(_, choice)| !choice.label.trim().is_empty())
                .map(|(position, choice)| {
                    let mut row = raw_choices
                        .and_then(|rows| rows.get(position))
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    // A dialog row's own label is the terminal's wording; the notice choice is a
                    // summary of it, so the collapsed button follows the dialog when there is one.
                    let label = notice
                        .dialog
                        .as_ref()
                        .and_then(|dialog| {
                            usize::try_from(choice.index)
                                .ok()
                                .and_then(|index| dialog.rows.get(index))
                        })
                        .map(|dialog_row| dialog_row.label.clone())
                        .unwrap_or_else(|| choice.label.clone());
                    row.insert(
                        "collapsedLabel".to_string(),
                        json!(collapsed_choice_label(&label)),
                    );
                    row.insert(
                        "answer".to_string(),
                        terminal_notice_choice_answer(Some(notice), choice.index),
                    );
                    Value::Object(row)
                })
                .collect();
            object.insert("choices".to_string(), Value::Array(rows));
        }
        None => {
            object.remove("choices");
        }
    }
    match notice.actions.as_ref() {
        Some(actions) => {
            let raw_actions = raw.get("actions").and_then(Value::as_array);
            let rows: Vec<Value> = actions
                .iter()
                .enumerate()
                .filter_map(|(position, action)| {
                    let answer = terminal_notice_action_answer(notice, action);
                    if action.kind != "switchToTerminal" && answer.is_none() {
                        return None;
                    }
                    let mut row = raw_actions
                        .and_then(|rows| rows.get(position))
                        .and_then(Value::as_object)
                        .cloned()
                        .unwrap_or_default();
                    row.insert("answer".to_string(), answer.unwrap_or(Value::Null));
                    Some(Value::Object(row))
                })
                .collect();
            object.insert("actions".to_string(), Value::Array(rows));
        }
        None => {
            object.remove("actions");
        }
    }
    Value::Object(object)
}

/// The empty drafts a question set opens with.
pub(crate) fn blank_drafts(prompt: Option<&InteractivePrompt>) -> Vec<QuestionDraft> {
    match prompt.filter(|prompt| prompt.is_question()) {
        Some(prompt) => prompt
            .questions()
            .iter()
            .map(|_| QuestionDraft::default())
            .collect(),
        None => Vec::new(),
    }
}
