//! Family c's user actions: answering, moving between questions, the async question strip, and
//! the notices.
//!
//! Port of the question, notice and async-strip arms of the `action` switch in
//! `packages/shared/session-chat-controller/native-host.ts`. The TypeScript awaits each call; here
//! an action leaves a request behind and `crate::questions::events` finishes the work when the
//! answer arrives.

use serde_json::{json, Map, Value};

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::questions::async_controller;
use crate::questions::drafts::{
    drafts_key, encode_drafts, notice_key, remaining_drafts, AnswerDrafts,
};
use crate::questions::gates;
use crate::questions::interactive::{is_answered, select_question_option};
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::dismissed_notice_state;
use crate::questions::sync::sync;
use crate::questions::terminal_prompts::{
    terminal_notice_action_answer, terminal_notice_action_shortcut_eligible,
    terminal_notice_choice_answer,
};
use crate::state::{AnswerRequest, AsyncSubmit, ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// Handles one action family c owns.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    let mut effects = sync(state);
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    match action.kind {
        ActionKind::DismissNotice => {
            effects.extend(dismiss_notice(state, notice.as_ref(), context));
        }
        ActionKind::NoticePrimary | ActionKind::NoticeSecondary => {
            let primary = action.kind == ActionKind::NoticePrimary;
            if let Some(answer) = notice_answer(state, notice.as_ref(), primary) {
                effects.extend(answer_prompt(state, &answer));
            }
        }
        ActionKind::Answer => {
            if let Some(answer) = action.param("answer") {
                let answer = answer.clone();
                effects.extend(answer_prompt(state, &answer));
            }
        }
        ActionKind::QuestionText => {
            effects.extend(question_text(
                state,
                action.param("text").and_then(Value::as_str).unwrap_or(""),
            ));
        }
        ActionKind::QuestionBack => question_back(state),
        ActionKind::QuestionOption | ActionKind::QuestionNext => {
            let index = action.param("index").and_then(Value::as_u64);
            effects.extend(question_step(
                state,
                prompt.as_ref(),
                if action.kind == ActionKind::QuestionOption {
                    index
                } else {
                    None
                },
                action.kind == ActionKind::QuestionOption,
            ));
        }
        ActionKind::QuestionCancel => effects.extend(question_cancel(state)),
        ActionKind::AsyncQuestionToggle => {
            async_controller::toggle(&mut state.questions.async_questions);
        }
        ActionKind::AsyncQuestionNavigate => {
            let projection = async_projection(state, prompt.as_ref(), notice.as_ref());
            let previous = action.param("direction").and_then(Value::as_str) == Some("previous");
            let key = if previous {
                projection.previous_key
            } else {
                projection.next_key
            };
            async_controller::navigate(&mut state.questions.async_questions, key.as_deref());
        }
        ActionKind::AsyncQuestionText => {
            if let Some(key) = action.param("key").and_then(Value::as_str) {
                let text = action.param("text").and_then(Value::as_str).unwrap_or("");
                let session_key = state.identity.session_key.clone();
                effects.extend(async_controller::edit(
                    &mut state.questions.async_questions,
                    &session_key,
                    key,
                    text,
                ));
                // `save()` publishes at once and chains its write without awaiting it.
                state.core.effects_not_awaited = true;
            }
        }
        ActionKind::AsyncQuestionOption => {
            let projection = async_projection(state, prompt.as_ref(), notice.as_ref());
            let key = action.param("key").and_then(Value::as_str).unwrap_or("");
            // `options[command.index]`: an index that is absent, negative or past the end reads
            // `undefined` in the TypeScript and the arm does nothing.
            //
            // CDXC:SessionChat 2026-09-22 WHY:
            // The narrowing used to be `as u32` with a `0` default, so an index of 4294967296
            // WRAPPED to 0, passed the length test below and answered the question with the first
            // option on the user's behalf. The bound is checked before the cast now.
            let index = action
                .param("index")
                .and_then(Value::as_u64)
                .and_then(|value| u32::try_from(value).ok());
            let offered = index.is_some_and(|index| {
                projection.question.as_ref().is_some_and(|question| {
                    question.key == key
                        && question
                            .options
                            .as_ref()
                            .is_some_and(|options| (index as usize) < options.len())
                })
            });
            if let (false, true, Some(index)) = (projection.disabled, offered, index) {
                let session_key = state.identity.session_key.clone();
                effects.extend(async_controller::select(
                    &mut state.questions.async_questions,
                    &session_key,
                    key,
                    index,
                ));
                state.core.effects_not_awaited = true;
            }
        }
        ActionKind::AsyncQuestionSend | ActionKind::AsyncQuestionSkip => {
            let skip = action.kind == ActionKind::AsyncQuestionSkip;
            let before = effects.len();
            effects.extend(async_submit(state, prompt.as_ref(), notice.as_ref(), skip));
            // `submit()` sets `submitting` and calls `changed()` BEFORE it awaits the delivery, so
            // the controls lock on the gesture's own turn and the arm publishes again when the
            // retire write lands.
            if effects.len() > before {
                state.core.request_publish();
            }
        }
        _ => {}
    }
    crate::questions::sync::refresh_gates(state);
    effects
}

/// The strip as it stands right now, which is what the navigate and option arms read.
fn async_projection(
    state: &ChatState,
    prompt: Option<&InteractivePrompt>,
    notice: Option<&TerminalNotice>,
) -> crate::document::AsyncQuestions {
    async_controller::project(
        &state.questions.async_questions,
        &state.messages.composed,
        gates::async_questions_can_send(state, prompt, notice),
        gates::working(state),
        &state.session.retired_async_question_ids,
    )
}

/// Closing a notice that only describes a state. A notice with a picker or a dialog has to be
/// answered instead, so the button does nothing there.
fn dismiss_notice(
    state: &mut ChatState,
    notice: Option<&TerminalNotice>,
    context: &ChatContext,
) -> Vec<Effect> {
    let Some(notice) = notice else {
        return Vec::new();
    };
    if notice.choices.as_ref().is_some_and(|rows| !rows.is_empty()) || notice.dialog.is_some() {
        return Vec::new();
    }
    let dismissed = dismissed_notice_state(notice, context.now_millis());
    let value = serde_json::to_string(&dismissed).ok();
    state.questions.dismissed_notice = Some(dismissed);
    vec![Effect::WriteStorage {
        key: notice_key(&state.identity.session_key),
        value,
        durable: true,
    }]
}

/// What the notice card's primary or secondary button sends.
fn notice_answer(
    state: &ChatState,
    notice: Option<&TerminalNotice>,
    primary: bool,
) -> Option<Value> {
    if !gates::notice_visible(state) {
        return None;
    }
    let notice = notice?;
    let choices = notice.visible_choices();
    match choices.get(if primary { 0 } else { 1 }) {
        Some(choice) => Some(terminal_notice_choice_answer(Some(notice), choice.index)),
        None if primary => notice
            .actions
            .iter()
            .flatten()
            .filter(|action| terminal_notice_action_shortcut_eligible(action))
            .find_map(|action| terminal_notice_action_answer(notice, action)),
        None => None,
    }
}

/// Sends one answer, whatever asked for it.
fn answer_prompt(state: &mut ChatState, answer: &Value) -> Vec<Effect> {
    if state.questions.answering {
        return Vec::new();
    }
    let kind = answer
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    state.questions.answering = true;
    state.questions.notice_error = None;
    // A picker answer hides the card at once: gxserver keeps reporting the detection until the
    // agent's screen catches up, and the user has already chosen.
    if kind == "terminalChoice"
        || (kind == "terminalDialog" && answer.get("choiceIndex").is_some_and(Value::is_number))
    {
        state.questions.answered_notice_key = state.questions.active_notice_key.clone();
    }
    let request_id = allocate(state);
    state.questions.answer_request = Some(AnswerRequest {
        request_id,
        prompt_key: state.questions.prompt_key.clone(),
        answer_kind: kind,
        submitted: AnswerDrafts::new(),
        content_key: None,
    });
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(with_identity(state, answer.clone())),
    }]
}

/// Typing into the card's free-text row.
fn question_text(state: &mut ChatState, text: &str) -> Vec<Effect> {
    let questions = &mut state.questions;
    if questions.answering
        || questions.question_transition
        || questions.question_drafts_loading
        || questions
            .question_drafts
            .get(questions.question_index)
            .is_none()
    {
        return Vec::new();
    }
    let index = questions.question_index;
    questions.question_drafts[index].other = text.to_string();
    // `publish(chat)` before the write: the card is module state, so the typed character shows on
    // this turn and the arm publishes again when the write answers.
    state.core.request_publish();
    write_card_drafts(state, false)
}

/// Stepping back through the set. The saved answers stay, so stepping forward again shows them.
fn question_back(state: &mut ChatState) {
    let questions = &mut state.questions;
    if questions.answering || questions.question_transition || questions.question_drafts_loading {
        return;
    }
    questions.question_index = questions.question_index.saturating_sub(1);
}

/// Picking an option, or pressing Next or Send answer.
fn question_step(
    state: &mut ChatState,
    prompt: Option<&InteractivePrompt>,
    option_index: Option<u64>,
    is_option: bool,
) -> Vec<Effect> {
    let Some(prompt) = prompt.filter(|prompt| prompt.is_question()) else {
        return Vec::new();
    };
    {
        let questions = &state.questions;
        if questions.answering || questions.question_transition || questions.question_drafts_loading
        {
            return Vec::new();
        }
    }
    let index = state.questions.question_index;
    let Some(question) = prompt.questions().get(index).cloned() else {
        return Vec::new();
    };
    if is_option {
        let Some(option_index) = option_index else {
            return Vec::new();
        };
        if question.options.get(option_index as usize).is_none() {
            return Vec::new();
        }
        state.questions.question_drafts = select_question_option(
            &state.questions.question_drafts,
            index,
            question.multi_select,
            option_index as u32,
        );
        state.questions.question_transition = true;
        // `questionTransition = true; publish(chat)` before the write locks the card at once.
        state.core.request_publish();
        // A multi-select question stays put: the write lands and the card waits for more picks.
        return write_card_drafts(state, !question.multi_select);
    }
    advance(state, prompt)
}

/// Moves to the next question, or sends the whole set when this was the last one.
pub(crate) fn advance(state: &mut ChatState, prompt: &InteractivePrompt) -> Vec<Effect> {
    let count = prompt.questions().len();
    if state.questions.question_index + 1 < count {
        state.questions.question_index += 1;
        return Vec::new();
    }
    if !state
        .questions
        .question_drafts
        .iter()
        .any(|draft| is_answered(Some(draft)))
    {
        return Vec::new();
    }
    let drafts = state.questions.question_drafts.clone();
    let selections: Vec<Value> = drafts
        .iter()
        .map(|draft| {
            let mut selection = Map::new();
            selection.insert("indices".to_string(), json!(draft.indices));
            let other = draft.other.trim();
            if !other.is_empty() {
                selection.insert("other".to_string(), json!(other));
            }
            Value::Object(selection)
        })
        .collect();
    state.questions.answering = true;
    // `answering = true; publish(chat)` before the answer goes out.
    state.core.request_publish();
    let request_id = allocate(state);
    state.questions.answer_request = Some(AnswerRequest {
        request_id,
        prompt_key: state.questions.prompt_key.clone(),
        answer_kind: "question".to_string(),
        submitted: indexed_drafts(&drafts),
        content_key: state.questions.question_content_key.clone(),
    });
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(with_identity(
            state,
            json!({ "kind": "question", "selections": selections }),
        )),
    }]
}

/// Closing the card without answering, which also stops the agent waiting for it.
fn question_cancel(state: &mut ChatState) -> Vec<Effect> {
    {
        let questions = &state.questions;
        if questions.answering || questions.question_transition || questions.question_drafts_loading
        {
            return Vec::new();
        }
    }
    state.questions.dismissed_prompt = state.questions.prompt_key.clone();
    let mut effects = Vec::new();
    if let Some(content_key) = state.questions.question_content_key.clone() {
        effects.push(clear_card_drafts(state, &content_key));
    }
    let request_id = allocate(state);
    state.questions.cancel_request = Some(request_id);
    effects.push(Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::InterruptSessionChat,
        params: Box::new(with_identity(state, Value::Object(Map::new()))),
    });
    effects
}

/// Writes every draft of the open card, keyed by question index the way `questionWrite` does.
fn write_card_drafts(state: &mut ChatState, advance_after: bool) -> Vec<Effect> {
    let Some(content_key) = state.questions.question_content_key.clone() else {
        return Vec::new();
    };
    let drafts = indexed_drafts(&state.questions.question_drafts);
    state.questions.draft_write_content_key = Some(content_key.clone());
    state.questions.advance_after_write = advance_after;
    vec![Effect::WriteStorage {
        key: drafts_key(&state.identity.session_key, &content_key),
        value: encode_drafts(&drafts),
        durable: true,
    }]
}

/// Drops the saved answers this card no longer needs, which is what `questionClear` does.
pub(crate) fn clear_card_drafts(state: &ChatState, content_key: &str) -> Effect {
    let submitted = indexed_drafts(&state.questions.question_drafts);
    let remaining = remaining_drafts(&submitted, &submitted);
    Effect::WriteStorage {
        key: drafts_key(&state.identity.session_key, content_key),
        value: encode_drafts(&remaining),
        durable: true,
    }
}

/// `Object.fromEntries(drafts.map((draft, index) => [index, draft]))`.
pub(crate) fn indexed_drafts(drafts: &[crate::document::QuestionDraft]) -> AnswerDrafts {
    drafts
        .iter()
        .enumerate()
        .map(|(index, draft)| (index.to_string(), draft.clone()))
        .collect()
}

/// Sending or skipping the async question on screen.
fn async_submit(
    state: &mut ChatState,
    prompt: Option<&InteractivePrompt>,
    notice: Option<&TerminalNotice>,
    skip: bool,
) -> Vec<Effect> {
    let can_send = gates::async_questions_can_send(state, prompt, notice);
    let retired = state.session.retired_async_question_ids.clone();
    let Some((key, answer)) = async_controller::submission(
        &state.questions.async_questions,
        &state.messages.composed,
        can_send,
        skip,
        &retired,
    ) else {
        return Vec::new();
    };
    let mut submitted = AnswerDrafts::new();
    if let Some(draft) = state.questions.async_questions.drafts.get(&key) {
        submitted.insert(key.clone(), draft.clone());
    }
    async_controller::begin_submit(&mut state.questions.async_questions);
    let request_id = allocate(state);
    state.questions.async_questions.submit = Some(AsyncSubmit {
        request_id,
        key: key.clone(),
        skip,
        submitted,
    });
    let params = if skip {
        json!({ "kind": "dismissAsyncQuestion", "questionId": key })
    } else {
        json!({ "kind": "asyncQuestion", "questionId": key, "text": answer })
    };
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::AnswerSessionChatPrompt,
        params: Box::new(with_identity(state, params)),
    }]
}

/// Every gxserver call carries the session it is about.
fn with_identity(state: &ChatState, params: Value) -> Value {
    let mut object = match params {
        Value::Object(object) => object,
        _ => Map::new(),
    };
    if state.session.agent.as_deref() == Some("opencode") {
        if let Some(id) = state.session.prompt.as_ref().and_then(|p|p.get("toolUseId")).and_then(Value::as_str) {
            object.insert("toolUseId".into(), json!(id));
        }
    }
    object.insert(
        "projectId".to_string(),
        json!(state.identity.project_id.clone()),
    );
    object.insert(
        "sessionId".to_string(),
        json!(state.identity.session_id.clone()),
    );
    Value::Object(object)
}

/// The next request id, from the core's one allocator.
fn allocate(state: &mut ChatState) -> u64 {
    state.core.allocate_request_id()
}
