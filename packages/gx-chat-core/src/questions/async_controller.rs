//! The async question strip: selection, navigation, explicit submission and retirement.
//!
//! Port of `packages/shared/session-chat-controller/async-questions.ts`. The TypeScript controller
//! is promise based; here the same steps are one state transition plus the effects the host
//! performs, and the answers arrive back as events.

use crate::state::ChatRow;

use crate::document::{AsyncQuestions, PendingAsyncQuestion, QuestionDraft};
use crate::effect::Effect;
use crate::questions::async_answers::pending_async_questions;
use crate::questions::drafts::{async_drafts_key, encode_drafts, encode_retired, retired_key};
use crate::state::AsyncQuestionsState;

/// The copy the strip shows when a stored answer could not be written.
pub const SAVE_FAILED: &str =
    "Your answer could not be saved on this computer. Keep this view open until saving succeeds.";
/// The copy the strip shows when the saved answers could not be read back.
pub const RESTORE_FAILED: &str =
    "Your saved answers could not be restored. Keep this view open until saving succeeds.";
/// The refusal copy for a send that failed without a message of its own.
pub const SEND_FAILED: &str = "Could not send your answer. Please try again.";
/// The refusal copy for a skip that failed without a message of its own.
pub const SKIP_FAILED: &str = "Could not skip this question. Please try again.";

/// The questions still open, after this client's and the server's retirements.
pub fn pending(
    state: &AsyncQuestionsState,
    messages: &[ChatRow],
    retired_ids: &[String],
) -> Vec<PendingAsyncQuestion> {
    pending_async_questions(messages)
        .into_iter()
        .filter(|question| {
            !state.retired.contains(&question.key) && !retired_ids.contains(&question.key)
        })
        .collect()
}

/// The strip as the renderer draws it.
pub fn project(
    state: &AsyncQuestionsState,
    messages: &[ChatRow],
    can_send: bool,
    working: bool,
    retired_ids: &[String],
) -> AsyncQuestions {
    let pending = pending(state, messages, retired_ids);
    // `Math.max(0, findIndex(...))`: an active key that is gone falls back to the first question.
    let index = state
        .active_key
        .as_ref()
        .and_then(|key| pending.iter().position(|question| &question.key == key))
        .unwrap_or(0);
    let question = pending.get(index);
    let draft = match question {
        Some(question) => state
            .drafts
            .get(&question.key)
            .cloned()
            // A question with no saved answer opens on its first option.
            .unwrap_or(QuestionDraft {
                indices: vec![0],
                other: String::new(),
                ..QuestionDraft::default()
            }),
        None => QuestionDraft::default(),
    };
    let typed = draft.other.trim();
    let answer = if !typed.is_empty() {
        typed.to_string()
    } else {
        question
            .and_then(|question| question.options.as_ref())
            .and_then(|options| options.get(*draft.indices.first().unwrap_or(&0) as usize))
            .cloned()
            .unwrap_or_default()
    };
    let held = state.submitting || state.saving_images || state.loading;
    AsyncQuestions {
        question: question.cloned(),
        index: index as u32,
        count: pending.len() as u32,
        answer,
        disabled: !can_send || held,
        can_send,
        working,
        collapsed: state.collapsed,
        submitting: state.submitting,
        loading: state.loading,
        error: state
            .error
            .clone()
            .filter(|error| !error.is_empty())
            .unwrap_or_else(|| state.save_error.clone()),
        previous_disabled: held || index == 0,
        // `index === pending.length - 1`: with nothing pending that is `0 === -1`, false.
        next_disabled: held || index as i64 == pending.len() as i64 - 1,
        selected: if typed.is_empty() {
            draft.indices.clone()
        } else {
            Vec::new()
        },
        previous_key: index
            .checked_sub(1)
            .and_then(|at| pending.get(at))
            .map(|question| question.key.clone()),
        next_key: pending.get(index + 1).map(|question| question.key.clone()),
        draft,
    }
}

/// Reads the saved answers and this client's retirements back.
///
/// One effect, not two: `composer('asyncQuestionRead')` hands back `{drafts, retired}` from the two
/// stores in a single host call, and the number of round trips is part of the contract.
pub fn load(state: &mut AsyncQuestionsState, session_key: &str) -> Vec<Effect> {
    state.pending_reads = 2;
    vec![Effect::ReadStorageBatch {
        keys: vec![async_drafts_key(session_key), retired_key(session_key)],
    }]
}

/// One of the two reads answered; the strip unlocks once both have.
pub fn read_answered(state: &mut AsyncQuestionsState) {
    state.pending_reads = state.pending_reads.saturating_sub(1);
    if state.pending_reads == 0 {
        state.loading = false;
    }
}

/// Folds the strip open or shut.
pub fn toggle(state: &mut AsyncQuestionsState) {
    state.collapsed = !state.collapsed;
}

/// Moves to another question, unless a send or a read is holding the controls.
pub fn navigate(state: &mut AsyncQuestionsState, key: Option<&str>) {
    let Some(key) = key else { return };
    if state.submitting || state.saving_images || state.loading {
        return;
    }
    state.active_key = Some(key.to_string());
    state.error = None;
}

/// Replaces one question's typed answer.
pub fn edit(
    state: &mut AsyncQuestionsState,
    session_key: &str,
    key: &str,
    text: &str,
) -> Vec<Effect> {
    if state.submitting || state.loading {
        return Vec::new();
    }
    let mut draft = state.drafts.get(key).cloned().unwrap_or_default();
    draft.other = text.to_string();
    let mut drafts = state.drafts.clone();
    drafts.insert(key.to_string(), draft);
    save(state, session_key, drafts)
}

/// Picks one question's option, which replaces any typed answer.
pub fn select(
    state: &mut AsyncQuestionsState,
    session_key: &str,
    key: &str,
    index: u32,
) -> Vec<Effect> {
    if state.submitting || state.saving_images || state.loading {
        return Vec::new();
    }
    let mut drafts = state.drafts.clone();
    drafts.insert(
        key.to_string(),
        QuestionDraft {
            indices: vec![index],
            other: String::new(),
            ..QuestionDraft::default()
        },
    );
    save(state, session_key, drafts)
}

/// The host is writing pasted pictures, so a send would leave them out.
pub fn images_pending(state: &mut AsyncQuestionsState, value: bool) {
    state.saving_images = value;
}

fn save(
    state: &mut AsyncQuestionsState,
    session_key: &str,
    drafts: crate::questions::drafts::AnswerDrafts,
) -> Vec<Effect> {
    let value = encode_drafts(&drafts);
    state.drafts = drafts;
    vec![Effect::WriteStorage {
        key: async_drafts_key(session_key),
        value,
        durable: true,
    }]
}

/// A draft write settled: a failure is shown until the next write succeeds.
pub fn write_settled(state: &mut AsyncQuestionsState, error: Option<&str>) {
    state.save_error = match error {
        Some(_) => SAVE_FAILED.to_string(),
        None => String::new(),
    };
}

/// The answer the send or skip carries, or `None` when nothing may be sent right now.
pub fn submission(
    state: &AsyncQuestionsState,
    messages: &[ChatRow],
    can_send: bool,
    skip: bool,
    retired_ids: &[String],
) -> Option<(String, String)> {
    // The TypeScript projects with `working: false` here; only the gate and the answer matter.
    let projection = project(state, messages, can_send, false, retired_ids);
    let question = projection.question?;
    if projection.disabled || (!skip && projection.answer.trim().is_empty()) {
        return None;
    }
    Some((question.key, projection.answer.trim().to_string()))
}

/// Marks a send as in flight.
pub fn begin_submit(state: &mut AsyncQuestionsState) {
    state.submitting = true;
    state.error = None;
}

/// The send landed: the question is retired here and in storage, and its draft is dropped.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// Codex can accept an answer into its own queue before writing the user transcript. Server
/// retirement must reach every client, so this client's own list is only half of it and the
/// document also honours `retiredAsyncQuestionIds` from the wire.
pub fn submit_succeeded(
    state: &mut AsyncQuestionsState,
    session_key: &str,
    key: &str,
    submitted: &crate::questions::drafts::AnswerDrafts,
) -> Vec<Effect> {
    state.submitting = false;
    let retired_value = encode_retired(&state.retired, key);
    if !state.retired.iter().any(|entry| entry == key) {
        state.retired.push(key.to_string());
    }
    state.drafts.remove(key);
    // Delivery already succeeded. A storage failure must not offer to send it twice, so the
    // remaining drafts are written from what is left rather than from what was sent.
    let remaining = crate::questions::drafts::remaining_drafts(&state.drafts, submitted);
    state.drafts = remaining;
    // `composer('asyncQuestionRetire')` writes both records in one call.
    vec![Effect::WriteStorageBatch {
        writes: vec![
            crate::effect::StorageWrite {
                key: async_drafts_key(session_key),
                value: encode_drafts(&state.drafts),
                durable: true,
            },
            crate::effect::StorageWrite {
                key: retired_key(session_key),
                value: Some(retired_value),
                durable: true,
            },
        ],
    }]
}

/// The send was refused: the strip shows why and the question stays open.
pub fn submit_failed(state: &mut AsyncQuestionsState, message: Option<&str>, skip: bool) {
    state.submitting = false;
    state.error = Some(
        match message.map(str::trim).filter(|text| !text.is_empty()) {
            Some(message) => message.to_string(),
            None if skip => SKIP_FAILED.to_string(),
            None => SEND_FAILED.to_string(),
        },
    );
}
