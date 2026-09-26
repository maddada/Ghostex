//! What happens when the work an action started comes back.
//!
//! The TypeScript awaited inside the action handler; the core cannot, so the same `try`, `catch`
//! and `finally` bodies live here and run when the host reports the answer. Each entry point
//! matches on the request id or the storage key family c recorded, and answers `None` for
//! anything that is not its own.

use crate::effect::Effect;
use crate::event::StorageKey;
use crate::questions::actions::clear_card_drafts;
use crate::questions::async_controller;
use crate::questions::document::blank_drafts;
use crate::questions::drafts::{
    decode_drafts, decode_retired, DRAFTS_STORE, NOTICES_STORE, RETIRED_STORE,
};
use crate::questions::model::InteractivePrompt;
use crate::questions::model::TerminalNotice;
use crate::questions::notice_state::{decode_dismissed_notice, notice_dismiss_key};
use crate::questions::sync::refresh_gates;
use crate::state::ChatState;
use crate::wire::RpcOutcome;

/// The suffix the async strip's own draft record uses.
use crate::questions::drafts::{decode_drafts_suffix, is_async_drafts_key};

/// The prompt key a drafts record belongs to, which is the second half of its suffix.
fn drafts_prompt_key(key: &StorageKey) -> String {
    decode_drafts_suffix(&key.suffix)
        .map(|(_, prompt)| prompt)
        .unwrap_or_default()
}

/// Finishes a gxserver call family c asked for, or answers `None` when the id is not one of its
/// own.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state
        .questions
        .answer_request
        .as_ref()
        .is_some_and(|request| request.request_id == request_id)
    {
        return Some(answer_settled(state, outcome));
    }
    if state.questions.cancel_request == Some(request_id) {
        state.questions.cancel_request = None;
        if let RpcOutcome::Err { message, code, .. } = outcome {
            state.core.fail(message.clone(), code.clone());
        }
        return Some(Vec::new());
    }
    if state
        .questions
        .async_questions
        .submit
        .as_ref()
        .is_some_and(|submit| submit.request_id == request_id)
    {
        return Some(async_settled(state, outcome));
    }
    None
}

fn answer_settled(state: &mut ChatState, outcome: &RpcOutcome) -> Vec<Effect> {
    let Some(request) = state.questions.answer_request.take() else {
        return Vec::new();
    };
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    let mut effects = Vec::new();
    match outcome {
        RpcOutcome::Ok { .. } => {
            let permission_prompt = notice
                .as_ref()
                .is_some_and(|notice| notice.kind == "permissionPrompt");
            if request.answer_kind == "approval"
                || request.answer_kind == "question"
                || permission_prompt
            {
                state.questions.dismissed_prompt = request.prompt_key.clone();
            }
            // The card's own send clears its saved answers once gxserver has them.
            if let Some(content_key) = request.content_key.as_deref() {
                effects.push(clear_card_drafts(state, content_key));
            }
        }
        RpcOutcome::Err {
            message,
            code,
            endpoint: _,
        } => {
            state.questions.answered_notice_key = None;
            // A refused picker answer stops blocking the composer: the row is gone from the
            // agent's screen, or it never was what the card thought.
            if request.answer_kind == "terminalChoice" {
                if let Some(notice) = notice.as_ref().filter(|notice| notice.dialog.is_none()) {
                    state.questions.retired_notice_key = notice_dismiss_key(Some(notice));
                }
            }
            // A picker refusal belongs on the notice card, not on the composer's error line.
            if request.answer_kind == "terminalChoice" || request.answer_kind == "terminalDialog" {
                state.questions.notice_error = Some(message.clone());
                state.core.claim_refusal();
            } else {
                state.core.fail(message.clone(), code.clone());
            }
        }
    }
    let still_current = match request.content_key.as_ref() {
        Some(content_key) => state.questions.question_content_key.as_ref() == Some(content_key),
        None => true,
    };
    if still_current {
        state.questions.answering = false;
    }
    refresh_gates(state);
    // The arm is not over: `await composer('questionClear', …)` follows the answer, and the
    // action's closing publish runs after IT, not after the answer.
    state.core.publish_after(&effects);
    effects
}

fn async_settled(state: &mut ChatState, outcome: &RpcOutcome) -> Vec<Effect> {
    let Some(submit) = state.questions.async_questions.submit.take() else {
        return Vec::new();
    };
    let effects = match outcome {
        RpcOutcome::Ok { .. } => {
            let session_key = state.identity.session_key.clone();
            async_controller::submit_succeeded(
                &mut state.questions.async_questions,
                &session_key,
                &submit.key,
                &submit.submitted,
            )
        }
        RpcOutcome::Err { message, .. } => {
            async_controller::submit_failed(
                &mut state.questions.async_questions,
                Some(message.as_str()),
                submit.skip,
            );
            // `AsyncQuestions.submit` has its own `catch`: the refusal is the strip's `error`
            // line and never reaches the action's `catch`, so the composer's error bar stays
            // clear (`async-questions.ts:162`).
            state.core.claim_refusal();
            Vec::new()
        }
    };
    // `AsyncQuestions.submit` awaits `persistence.retire(...)` after the delivery and only then
    // leaves `submitting` and calls `changed()`, so the strip's publish is after the WRITE.
    state.core.publish_after(&effects);
    effects
}

/// Adopts a stored record family c asked for, or answers `false` when the key is not its own.
pub fn storage_loaded(state: &mut ChatState, key: &StorageKey, value: Option<&str>) -> bool {
    match key.store.as_str() {
        RETIRED_STORE => {
            state.questions.async_questions.retired = value.map(decode_retired).unwrap_or_default();
            async_controller::read_answered(&mut state.questions.async_questions);
            // `load().finally(() => { this.loading = false; this.changed(); })`: the strip is a
            // controller, not React state, so its own publish is this one.
            state.core.request_publish();
            true
        }
        NOTICES_STORE => {
            state.questions.dismissed_notice = value.and_then(decode_dismissed_notice);
            refresh_gates(state);
            true
        }
        DRAFTS_STORE if is_async_drafts_key(key) => {
            state.questions.async_questions.drafts = value.map(decode_drafts).unwrap_or_default();
            async_controller::read_answered(&mut state.questions.async_questions);
            state.core.request_publish();
            true
        }
        DRAFTS_STORE if state.questions.question_content_key == Some(drafts_prompt_key(key)) => {
            let stored = value.map(decode_drafts).unwrap_or_default();
            let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
            let mut drafts = blank_drafts(prompt.as_ref());
            for (index, draft) in drafts.iter_mut().enumerate() {
                if let Some(saved) = stored.get(&index.to_string()) {
                    *draft = saved.clone();
                }
            }
            state.questions.question_drafts = drafts;
            state.questions.question_drafts_loading = false;
            true
        }
        // A read for a card that has already moved on: its answer belongs to nothing.
        DRAFTS_STORE => true,
        _ => false,
    }
}

/// Finishes a stored write family c asked for, or answers `None` when the key is not its own.
pub fn storage_written(
    state: &mut ChatState,
    key: &StorageKey,
    error: Option<&str>,
) -> Option<Vec<Effect>> {
    match key.store.as_str() {
        DRAFTS_STORE if is_async_drafts_key(key) => {
            async_controller::write_settled(&mut state.questions.async_questions, error);
            // `save()`'s own `changed()` on the write settling, which is what clears or raises the
            // "could not be saved on this computer" line.
            state.core.request_publish();
            Some(Vec::new())
        }
        DRAFTS_STORE => {
            let prompt_key = drafts_prompt_key(key);
            if state.questions.draft_write_content_key != Some(prompt_key.clone()) {
                return Some(Vec::new());
            }
            state.questions.draft_write_content_key = None;
            let advance = state.questions.advance_after_write;
            state.questions.advance_after_write = false;
            // The write only releases the card it belongs to; a later prompt already reset it.
            let current = state.questions.question_content_key == Some(prompt_key.clone());
            if current {
                state.questions.question_transition = false;
            }
            if !advance || !current {
                return Some(Vec::new());
            }
            let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
            match prompt.filter(InteractivePrompt::is_question) {
                Some(prompt) => {
                    let effects = crate::questions::actions::advance(state, &prompt);
                    // Still the same `questionOption` arm: `await composer('questionWrite')` has
                    // answered and the arm walked on to `await chat.answerPrompt(...)`, so the
                    // CLOSING publish is owed to that call, not to this one. Without this the
                    // answer's own turn looked like the end of the arm and shipped a snapshot the
                    // TypeScript ships one round trip later.
                    state.core.publish_after(&effects);
                    Some(effects)
                }
                None => Some(Vec::new()),
            }
        }
        RETIRED_STORE | NOTICES_STORE => Some(Vec::new()),
        _ => None,
    }
}
