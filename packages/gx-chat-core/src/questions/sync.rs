//! Reconciling the card and the notice with whatever the wire last folded in.
//!
//! `publish` in `packages/shared/session-chat-controller/native-host.ts` did this at the top of
//! every publish: a new notice forgets the answered key and the refusal, a new prompt forgets the
//! dismissal, and a new prompt body resets the card and re-reads its saved answers. The core
//! cannot mutate state while assembling a document, so the seam calls this after every event and
//! before assembling.

use crate::effect::Effect;
use crate::event::Event;
use crate::questions::async_controller;
use crate::questions::document::blank_drafts;
use crate::questions::drafts::drafts_key;
use crate::questions::gates::{content_key, prompt_key};
use crate::questions::model::{InteractivePrompt, TerminalNotice};
use crate::questions::notice_state::{notice_dismiss_key, DismissedNotice};
use crate::state::{ChatContext, ChatState};

/// Brings family c's state in line with `state.session`, and asks for what a new card needs.
///
/// Safe to call as often as the seam likes: every branch is keyed off a value that only changes
/// when the wire changes.
pub fn sync(state: &mut ChatState) -> Vec<Effect> {
    let mut effects = Vec::new();
    // This is the top of `publish`, and `publish` is only ever called as
    // `if (controller) publish(controller.current())`. Before the composer boot read answers there
    // is no controller, so neither the card's saved-answers read nor `asyncQuestions.load()` has
    // been issued yet: issuing them at `Event::Start` put both storage round trips ahead of the
    // TypeScript's and the async strip stayed `loading` for two more drains.
    if !state.core.controller_started {
        refresh_gates(state);
        return effects;
    }
    let prompt = InteractivePrompt::parse(state.session.prompt.as_ref());
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());

    let next_notice_key = notice_dismiss_key(notice.as_ref());
    if state.questions.active_notice_key != next_notice_key {
        state.questions.active_notice_key = next_notice_key;
        state.questions.answered_notice_key = None;
        state.questions.notice_error = None;
    }

    let next_prompt_key = prompt_key(prompt.as_ref());
    if state.questions.prompt_key != next_prompt_key {
        state.questions.prompt_key = next_prompt_key;
        state.questions.dismissed_prompt = None;
    }

    let next_content_key = content_key(state);
    if state.questions.question_content_key != next_content_key {
        let questions = &mut state.questions;
        questions.question_content_key = next_content_key.clone();
        questions.question_index = 0;
        questions.question_transition = false;
        questions.answering = false;
        questions.answer_request = None;
        questions.draft_write_content_key = None;
        questions.advance_after_write = false;
        questions.question_drafts = blank_drafts(prompt.as_ref());
        let is_question = prompt.as_ref().is_some_and(InteractivePrompt::is_question);
        questions.question_drafts_loading = is_question;
        if is_question {
            if let Some(key) = next_content_key.as_deref() {
                effects.push(Effect::ReadStorage {
                    key: drafts_key(&state.identity.session_key, key),
                });
            }
        }
    }

    // The async strip reads its saved answers once per chat, the way `start` kicks off
    // `asyncQuestions.load()`.
    if !state.questions.async_questions.load_started {
        state.questions.async_questions.load_started = true;
        let session_key = state.identity.session_key.clone();
        effects.extend(async_controller::load(
            &mut state.questions.async_questions,
            &session_key,
        ));
    }
    refresh_gates(state);
    effects
}

/// Recomputes the two gates other families read off `ChatState::questions`.
///
/// Called at the end of every path family c can change, so `notice_visible` and `notice_retired`
/// are never one event behind what the document says.
pub fn refresh_gates(state: &mut ChatState) {
    let notice = TerminalNotice::parse(state.session.terminal_notice.as_ref());
    state.questions.notice_visible = crate::questions::gates::notice_visible(state);
    state.questions.notice_retired = notice.is_some()
        && notice_dismiss_key(notice.as_ref()) == state.questions.retired_notice_key;
}

/// Adopts the dismissal read back from client storage.
pub fn adopt_dismissed_notice(state: &mut ChatState, dismissed: Option<DismissedNotice>) {
    state.questions.dismissed_notice = dismissed;
}

/// The mutating half of `publish` for family c, run once per event before the document is
/// assembled.
///
/// Three of the four things family c waits for arrive as events rather than as the return value
/// of the call that asked for them, so the `try`, `catch` and `finally` bodies of the TypeScript
/// handlers run here (`crate::questions::events`), and the reconciliation [`sync`] does then runs
/// over the result.
pub fn settle(state: &mut ChatState, event: &Event, _context: &ChatContext) -> Vec<Effect> {
    let mut effects = match event {
        Event::RpcSettled {
            request_id,
            outcome,
        } => crate::questions::events::rpc_settled(state, *request_id, outcome).unwrap_or_default(),
        Event::StorageLoaded { key, value } => {
            crate::questions::events::storage_loaded(state, key, value.as_deref());
            Vec::new()
        }
        Event::StorageWritten { key, error } => {
            crate::questions::events::storage_written(state, key, error.as_deref())
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    effects.extend(sync(state));
    effects
}
