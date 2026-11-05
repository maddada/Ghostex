//! The reader's side of a one-click answer (`transcript/inline_answers.rs`): a click on one of the
//! answers under a question sends it, and Change lets the reader pick another after it went.
//!
//! A one-click question is a one-question form, so a first click is that form's send
//! (`forms::dispatch_answers`): the same `<ghostex-page-answers>` message, the same queued and
//! sending states, the same answers card. A form whose answers are on their way or arrived takes no
//! second click, so an answer never goes twice; every client reads the same state from the
//! transcript and the queue.
//!
//! A change follows the SessionChat 2026-11-05 DECISION in transcript/inline_answers.rs (Q1247 B):
//!   * answers still in Ghostex's queue are rewritten there (`updateSessionChatQueuedPrompt`);
//!   * answers the agent holds in its own queue are taken back out of it
//!     (`takeBackSessionChatQueuedMessage`, the queued bubble's Back to composer) and the new pick
//!     goes in their place;
//!   * answers the agent already read stay, and the new pick goes after them with a note saying
//!     what it replaces (`"Q901 (note)": "Changed from Yes"`).

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use crate::effect::Effect;
use crate::session::constants::PENDING_ID_PREFIX;
use crate::state::{ChatContext, ChatState};
use crate::transcript::question_forms::{answers_message, QuestionForm, NOTE_SUFFIX};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// A take-back in flight for a change: the answers that go once the old ones are out.
#[derive(Clone, Debug, PartialEq)]
pub struct InlineTakeBack {
    pub request_id: u64,
    pub form: String,
    /// What a change sends when the agent took the old answers already.
    pub correction: Value,
    /// What it sends once the old answers are out of the agent's queue.
    pub replacement: Value,
}

/// What this client is doing with one-click answers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineAnswersState {
    /// The forms whose sent answer the reader asked to change: their other answers take a click.
    pub changing: BTreeSet<String>,
    pub take_back: Option<InlineTakeBack>,
}

/// The value the form's one question was answered with: the answers in the transcript, then what
/// this client sent.
fn sent_value(state: &ChatState, form: &QuestionForm) -> Option<String> {
    let key = &form.questions.first()?.key;
    crate::questions::forms::received_answers(state, &form.id)
        .or_else(|| {
            let entry = state.questions.forms.get(&form.id)?;
            entry
                .sent
                .as_ref()
                .or(entry.send.as_ref().map(|send| &send.answers))
        })
        .and_then(|answers| answers.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The answers a pick sends: the question's key and the value.
fn answers_for(form: &QuestionForm, value: &str, changed_from: Option<&str>) -> Value {
    let key = form
        .questions
        .first()
        .map(|question| question.key.clone())
        .unwrap_or_default();
    let mut out = Map::new();
    out.insert(key.clone(), json!(value));
    if let Some(previous) = changed_from {
        out.insert(
            format!("{key}{NOTE_SUFFIX}"),
            json!(format!("Changed from {previous}")),
        );
    }
    Value::Object(out)
}

/// The id of Ghostex's queued prompt that holds the form's answers, when one does.
fn ghostex_queued_prompt(state: &ChatState, id: &str) -> Option<String> {
    state
        .session
        .queue_prompts
        .as_ref()?
        .iter()
        .find_map(|prompt| {
            let text = prompt.get("text").and_then(Value::as_str)?;
            (crate::transcript::page_answers::answers_page_id(text).as_deref() == Some(id))
                .then(|| prompt.get("id").and_then(Value::as_str).map(str::to_string))
                .flatten()
        })
}

/// The text of the message the agent holds in its own queue with the form's answers, when it does.
fn agent_queued_text(state: &ChatState, id: &str) -> Option<(String, String)> {
    let takeable = crate::composer::queue_take_back::takeable_ids(state);
    state.messages.composed.iter().find_map(|message| {
        if !takeable.contains(&message.id) {
            return None;
        }
        let text = crate::transcript::prose::prose_markdown(&message.blocks);
        (crate::transcript::page_answers::answers_page_id(&text).as_deref() == Some(id))
            .then(|| (message.id.clone(), text))
    })
}

/// `inlineAnswer {form, choice}`: a click on one of a question's answers.
pub fn pick(state: &mut ChatState, context: &ChatContext, id: &str, choice: usize) -> Vec<Effect> {
    let Some(form) = crate::questions::forms::form(state, id).cloned() else {
        return Vec::new();
    };
    let Some(picked) = form
        .questions
        .first()
        .and_then(|question| question.choices.get(choice))
        .map(|choice| choice.value.clone())
    else {
        return Vec::new();
    };
    if crate::questions::forms::open(state, id) {
        state.questions.inline_answers.changing.remove(id);
        if let Some(entry) = state.questions.forms.get_mut(id) {
            entry.touched = true;
            if let Some(draft) = entry.drafts.first_mut() {
                draft.indices = vec![choice as u32];
            }
        }
        return crate::questions::forms::dispatch_answers(
            state,
            context,
            &form,
            answers_for(&form, &picked, None),
        );
    }
    // Sent already: only a change takes another pick, and only one at a time.
    if !state.questions.inline_answers.changing.contains(id)
        || state.questions.inline_answers.take_back.is_some()
        || state
            .questions
            .forms
            .get(id)
            .is_some_and(|entry| entry.send.is_some())
    {
        return Vec::new();
    }
    state.questions.inline_answers.changing.remove(id);
    state.core.request_publish();
    let previous = sent_value(state, &form);
    if previous.as_deref() == Some(picked.as_str()) {
        return Vec::new();
    }
    let replacement = answers_for(&form, &picked, None);
    if let Some(prompt_id) = ghostex_queued_prompt(state, id) {
        if let Some(entry) = state.questions.forms.get_mut(id) {
            entry.sent = Some(replacement.clone());
        }
        let request_id = state.core.allocate_request_id();
        return vec![Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::UpdateSessionChatQueuedPrompt,
            params: Box::new(json!({
                "promptId": prompt_id,
                "text": answers_message(&form, &replacement),
            })),
        }];
    }
    let correction = answers_for(&form, &picked, previous.as_deref());
    if let Some((_, text)) = agent_queued_text(state, id) {
        let request_id = state.core.allocate_request_id();
        state.questions.inline_answers.take_back = Some(InlineTakeBack {
            request_id,
            form: id.to_string(),
            correction,
            replacement,
        });
        return vec![Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::TakeBackSessionChatQueuedMessage,
            params: Box::new(json!({ "text": text })),
        }];
    }
    crate::questions::forms::dispatch_answers(state, context, &form, correction)
}

/// `inlineAnswerChange {form}`: Change on a sent answer wakes its other answers, and a second
/// click (or a pick of the same answer) puts them back to sleep.
pub fn toggle_change(state: &mut ChatState, id: &str) {
    if crate::questions::forms::open(state, id) {
        return;
    }
    let changing = &mut state.questions.inline_answers.changing;
    if !changing.remove(id) {
        changing.insert(id.to_string());
    }
    state.core.request_publish();
}

/// Ends a change's take-back, or answers `None` when the id is not one.
pub fn rpc_settled(
    state: &mut ChatState,
    context: &ChatContext,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    if state
        .questions
        .inline_answers
        .take_back
        .as_ref()
        .is_none_or(|take_back| take_back.request_id != request_id)
    {
        return None;
    }
    let take_back = state.questions.inline_answers.take_back.take()?;
    state.core.request_publish();
    let form = crate::questions::forms::form(state, &take_back.form).cloned()?;
    let taken = matches!(outcome, RpcOutcome::Ok { result } if result.get("takenBack") == Some(&Value::Bool(true)));
    if !taken {
        // The agent read the old answers meanwhile: the new pick goes as a correction.
        return Some(crate::questions::forms::dispatch_answers(
            state,
            context,
            &form,
            take_back.correction,
        ));
    }
    // The old answers never reached the agent: forget them and send the new pick in their place.
    if let Some((message_id, _)) = agent_queued_text(state, &form.id) {
        if let Some(pending_id) = message_id.strip_prefix(PENDING_ID_PREFIX) {
            crate::session::sends::drop_send(state, pending_id);
        }
    }
    if let Some(entry) = state.questions.forms.get_mut(&form.id) {
        entry.sent = None;
        entry.sent_waiting_for_idle = false;
        entry.confirmation_boundary = None;
        entry.confirmed = false;
        entry.queued_prompt_id = None;
    }
    Some(crate::questions::forms::dispatch_answers(
        state,
        context,
        &form,
        take_back.replacement,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::FormState;
    use crate::transcript::inline_answers::inline_questions;

    fn state_with_question() -> (ChatState, String) {
        let form = inline_questions("- Merge it? [Yes](ghostex-answer:Q9) [No](ghostex-answer:Q9)")
            [0]
        .form();
        let id = form.id.clone();
        let mut state = ChatState::default();
        state.transcript_view.question_forms.push(form);
        let entry = FormState {
            drafts: vec![Default::default()],
            ..FormState::default()
        };
        state.questions.forms.insert(id.clone(), entry);
        (state, id)
    }

    fn sent_text(effects: &[Effect]) -> String {
        match effects {
            [Effect::SendRpc { params, .. }] => params["text"].as_str().unwrap().to_string(),
            other => panic!("expected one send, got {other:?}"),
        }
    }

    #[test]
    fn a_click_sends_the_form_envelope_once() {
        let (mut state, id) = state_with_question();
        let context = ChatContext::default();
        let text = sent_text(&pick(&mut state, &context, &id, 0));
        assert!(text.starts_with(&format!(
            "<ghostex-page-answers page=\"{id}\" title=\"Merge it?\">"
        )));
        assert!(text.contains("\"Q9\": \"Yes\""));
        // A second click, on any answer, sends nothing while the first is on its way.
        assert!(pick(&mut state, &context, &id, 1).is_empty());
        let projected = crate::questions::forms::project(&state);
        assert_eq!(projected[&id]["state"], "sending");
        assert_eq!(projected[&id]["drafts"][0]["indices"][0], 0);
    }

    #[test]
    fn a_change_after_the_agent_read_it_goes_as_a_correction() {
        let (mut state, id) = state_with_question();
        let context = ChatContext::default();
        let effects = pick(&mut state, &context, &id, 0);
        let Effect::SendRpc { request_id, .. } = &effects[0] else {
            unreachable!()
        };
        crate::questions::forms::rpc_settled(
            &mut state,
            *request_id,
            &RpcOutcome::Ok { result: json!({}) },
        );
        // Without Change, the other answer takes no click.
        assert!(pick(&mut state, &context, &id, 1).is_empty());
        toggle_change(&mut state, &id);
        assert_eq!(
            crate::questions::forms::project(&state)[&id]["changing"],
            true
        );
        let text = sent_text(&pick(&mut state, &context, &id, 1));
        assert!(text.contains("\"Q9\": \"No\""));
        assert!(text.contains("\"Q9 (note)\": \"Changed from Yes\""));
        assert!(!state.questions.inline_answers.changing.contains(&id));
    }

    #[test]
    fn a_change_while_queued_in_ghostex_rewrites_the_queued_answers() {
        let (mut state, id) = state_with_question();
        let context = ChatContext::default();
        let first = sent_text(&pick(&mut state, &context, &id, 0));
        state.session.queue_prompts = Some(vec![json!({"id": "p1", "text": first})]);
        state.questions.forms.get_mut(&id).unwrap().send = None;
        state.questions.forms.get_mut(&id).unwrap().sent = Some(json!({"Q9": "Yes"}));
        toggle_change(&mut state, &id);
        let effects = pick(&mut state, &context, &id, 1);
        let [Effect::SendRpc { method, params, .. }] = &effects[..] else {
            panic!("expected one call, got {effects:?}")
        };
        assert_eq!(*method, ChatRpcMethod::UpdateSessionChatQueuedPrompt);
        assert_eq!(params["promptId"], "p1");
        assert!(params["text"].as_str().unwrap().contains("\"Q9\": \"No\""));
        assert!(!params["text"].as_str().unwrap().contains("Changed from"));
    }
}
