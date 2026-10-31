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
use crate::questions::notice_choices::{
    collapsed_choice_label, collapsed_choice_positions, COLLAPSED_CHOICE_COUNT,
};
use crate::questions::notice_tone;
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
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let questions = &state.questions;
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());

    into.prompt = match state.session.prompt.as_ref() {
        Some(value) => Tri::Value(value.clone()),
        None => Tri::Null,
    };
    into.terminal_notice = match (state.session.terminal_notice.as_ref(), notice.as_ref()) {
        (Some(raw), Some(notice)) => {
            let mut decorated = decorate_notice(raw, notice);
            if let Some(object) = decorated.as_object_mut() {
                crate::questions::account_wait::decorate(
                    object,
                    state.core.hide_account_emails,
                    context.now_ms,
                    context.utc_offset_minutes,
                );
            }
            Tri::Value(decorated)
        }
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
        reconnecting: questions
            .answer_request
            .as_ref()
            .is_some_and(|request| request.retry.reconnecting),
        loading: card.loading,
        approval_ask: match prompt.as_ref() {
            Some(InteractivePrompt::Approval { tool, .. }) => approval_ask(tool).to_string(),
            _ => String::new(),
        },
        approval_title: match prompt.as_ref() {
            Some(InteractivePrompt::Approval { tool, .. }) => {
                notice_tone::approval_title(state.session.agent.as_deref(), tool)
            }
            _ => String::new(),
        },
        approval_detail: match prompt.as_ref() {
            Some(InteractivePrompt::Approval { tool, .. }) => notice_tone::approval_detail(
                tool,
                state.transcript_view.working_directory.as_deref(),
            ),
            _ => String::new(),
        },
    };

    into.question_forms = crate::questions::forms::project(state);
    into.open_questions = crate::questions::open_questions::project(state, context);
    into.resource_modal = crate::questions::resource_modal::project(state, context);
    into.later_questions = crate::questions::open_questions::later_waiting(state);

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
    object.insert("tone".to_string(), json!(notice_tone::notice_tone(notice)));
    object.insert(
        "collapsedChoiceCount".to_string(),
        json!(COLLAPSED_CHOICE_COUNT),
    );
    // The exact rows a folded card shows (positions into `choices`), so every renderer folds the same way.
    object.insert(
        "collapsedChoices".to_string(),
        json!(collapsed_choice_positions(notice)),
    );
    // The visible rows Cmd+Enter and Escape answer, so the card puts each hint on its row.
    object.insert(
        "primaryChoice".to_string(),
        json!(notice.primary_choice_position()),
    );
    object.insert(
        "secondaryChoice".to_string(),
        json!(notice.secondary_choice_position()),
    );
    // CDXC:SessionChat 2026-09-29 WHY:
    // A collapsed command approval showed only "Would you like to run the following command?" and its buttons, so "Yes, proceed" was one click away from a command the card never named. The `$ ` lines of the dialog body are what is being approved; the renderers draw them under the title while the card is collapsed.
    // A picker card with no dialog (Grok Build's) names its command in `approvalCommand` instead.
    // SEE-ALSO: apps/desktop/src/app/native_chat/notice.rs, apps/mobile/app/src/chat/native/cards/NoticeCard.tsx, server/src/session_chat_grok_permission.rs.
    let mut commands: Vec<String> = raw
        .get("dialog")
        .and_then(|dialog| dialog.get("body"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("$ "))
        .map(str::to_string)
        .collect();
    if commands.is_empty() {
        commands = raw
            .get("approvalCommand")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| format!("$ {line}"))
            .collect();
    }
    object.remove("approvalCommand");
    // The open terminal output shows only what is above the prompt the card draws (notice_tail.rs);
    // a permission prompt opens with its tool's header ("Bash command").
    let header = raw
        .get("permissionRequest")
        .and_then(|request| request.get("source"))
        .and_then(Value::as_str);
    match notice
        .screen_tail
        .as_deref()
        .and_then(|tail| crate::questions::notice_tail::above_the_prompt(tail, notice, header))
    {
        Some(tail) => {
            object.insert("screenTail".to_string(), json!(tail));
        }
        None => {
            object.remove("screenTail");
        }
    }
    // A Claude Code permission prompt read whole is drawn as the card's `approval` details, which
    // show its command already.
    let approval = raw.get("permissionRequest").and_then(|request| {
        crate::questions::permission_card::presentation(request, &notice.title)
    });
    object.remove("permissionRequest");
    match approval {
        Some(approval) => {
            object.insert("approval".to_string(), approval);
            commands.clear();
        }
        None => {
            object.remove("approval");
        }
    }
    if commands.is_empty() {
        object.remove("collapsedDetail");
    } else {
        object.insert("collapsedDetail".to_string(), json!(commands.join("\n")));
    }
    match crate::questions::terminal_picker::passthrough_presentation(notice) {
        Some(passthrough) => {
            object.insert("passthrough".to_string(), passthrough);
        }
        None => {
            object.remove("passthrough");
        }
    }
    match notice
        .plan
        .as_deref()
        .filter(|plan| !plan.trim().is_empty())
    {
        Some(plan) => {
            object.insert("plan".to_string(), plan_presentation(plan));
        }
        None => {
            object.remove("plan");
        }
    }
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
                        "label".to_string(),
                        json!(crate::questions::notice_choices::without_key_hint(
                            &choice.label
                        )),
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
    object.remove("numbered");
    object.remove("more");
    if let Some(dialog) = notice.dialog.as_ref() {
        crate::questions::dialog_rows::decorate(&mut object, dialog);
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

/// CDXC:SessionChat 2026-10-26 WHY: "Ready to code?" shows only a screenful of the plan in a pane the chat cannot scroll, so the card draws the plan file whole, as the side question card draws its answer: Markdown for the renderer, its file references, and the raw text Copy puts on the clipboard (GitHub #321).
/// SEE-ALSO: server/src/session_chat_claude_plan.rs, apps/desktop/src/app/native_chat/notice.rs, apps/mobile/app/src/chat/native/cards/NoticeCard.tsx.
fn plan_presentation(plan: &str) -> Value {
    let markdown = crate::transcript::native_markdown::native_markdown(
        &crate::transcript::line_breaks::agent_line_breaks(
            plan,
            crate::transcript::line_breaks::AgentLineBreaks::Every,
        ),
        false,
    );
    json!({
        "text": plan,
        "references": crate::transcript::markdown_links::markdown_references(&markdown),
        "markdown": markdown,
    })
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
