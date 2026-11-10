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

/// The tag a one-click form asks, when `form` is one.
fn inline_tag(form: &QuestionForm) -> Option<&str> {
    let question = form
        .questions
        .first()
        .filter(|_| form.inline && form.questions.len() == 1)?;
    (!question.tag.is_empty()).then_some(question.tag.as_str())
}

/// The one-click forms in the transcript that ask the same tag as `id`, oldest first: the copies of
/// a question the agent asked again. Just `id` when it is no one-click question in the transcript.
///
/// CDXC:SessionChat 2026-11-05 DECISION:
/// User, of an unanswered one-click question drawn as Withdrawn after the coordinator asked the same tag again: a question is Withdrawn only once a ```questions block's `"withdraw"` names it; asking it again never withdraws it. Every copy of a tag stays live and acts as one question: a click on any copy answers the newest copy (the one the ? pill counts, so the tag is answered once and its count goes), every copy shows that copy's state and answer ("Sent · Change" on all of them once answered), and an older copy still waiting says "Asked again below". Supersedes treating a later copy as replacing the earlier one for one-click questions; ```questions forms and pages keep it.
pub(crate) fn copies(state: &ChatState, id: &str) -> Vec<String> {
    let tag = state
        .transcript_view
        .question_forms
        .iter()
        .find(|form| form.id == id && form.error.is_empty())
        .and_then(inline_tag);
    let Some(tag) = tag else {
        return vec![id.to_string()];
    };
    let mut copies: Vec<String> = Vec::new();
    for form in &state.transcript_view.question_forms {
        let same = form.error.is_empty()
            && inline_tag(form).is_some_and(|known| known.eq_ignore_ascii_case(tag));
        if same && !copies.contains(&form.id) {
            copies.push(form.id.clone());
        }
    }
    copies
}

/// Whether the one-click form `id` holds an answer: in the transcript or the resource index, or on
/// its way from this client.
fn holds_answer(state: &ChatState, id: &str) -> bool {
    crate::questions::forms::received_answers(state, id).is_some()
        || crate::questions::forms::answered_by_index(state, id)
        || state
            .questions
            .forms
            .get(id)
            .is_some_and(|entry| entry.send.is_some() || entry.sent.is_some())
        || crate::questions::forms::queued_in_queue(state, id)
}

/// The copy every copy of `id`'s question acts on: the newest one that holds an answer, else the
/// newest one. A reader who answered an older copy before the agent asked it again has answered
/// the question (the SessionChat 2026-11-07 DECISION on `transcript/open_questions.rs`
/// `share_answers`), so every copy shows that answer and a new pick goes as its change.
pub(crate) fn owner(state: &ChatState, id: &str) -> String {
    let mut copies = copies(state, id);
    match copies.iter().rposition(|copy| holds_answer(state, copy)) {
        Some(at) => copies.swap_remove(at),
        None => copies.pop().unwrap_or_else(|| id.to_string()),
    }
}

/// The newest one-click form that asks `tag`.
pub(crate) fn newest_for_tag(state: &ChatState, tag: &str) -> Option<String> {
    state
        .transcript_view
        .question_forms
        .iter()
        .rev()
        .find(|form| {
            form.error.is_empty()
                && inline_tag(form).is_some_and(|known| known.eq_ignore_ascii_case(tag))
        })
        .map(|form| form.id.clone())
}

/// `inlineAnswer {form, choice}`: a click on one of a question's answers, on any copy of it.
pub fn pick(state: &mut ChatState, context: &ChatContext, id: &str, choice: usize) -> Vec<Effect> {
    let Some(picked) = crate::questions::forms::form(state, id)
        .and_then(|form| form.questions.first())
        .and_then(|question| question.choices.get(choice))
        .map(|choice| choice.value.clone())
    else {
        return Vec::new();
    };
    let owner = owner(state, id);
    answer(state, context, &owner, &picked, false)
}

/// `inlineAnswerText {form | tag, text}`: the reader's own words as the answer (the Reply popover a
/// one-click question's ✎ or its own Q-tag opens). Sent like a pick, and as a change once the
/// question was answered.
pub fn answer_text(
    state: &mut ChatState,
    context: &ChatContext,
    id: Option<&str>,
    tag: Option<&str>,
    text: &str,
) -> Vec<Effect> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let Some(id) = id
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .or_else(|| tag.and_then(|tag| newest_for_tag(state, tag)))
    else {
        return Vec::new();
    };
    let owner = owner(state, &id);
    answer(state, context, &owner, text, true)
}

/// Sends `picked` as the answer of the one-click form `id`: at once while it waits, as a change
/// (`changing`, or `change` for the reader's own words) once it was sent.
fn answer(
    state: &mut ChatState,
    context: &ChatContext,
    id: &str,
    picked: &str,
    change: bool,
) -> Vec<Effect> {
    let Some(form) = crate::questions::forms::form(state, id).cloned() else {
        return Vec::new();
    };
    let index = form.questions.first().and_then(|question| {
        question
            .choices
            .iter()
            .position(|choice| choice.value == picked)
    });
    if crate::questions::forms::open(state, id) {
        state.questions.inline_answers.changing.remove(id);
        if let Some(entry) = state.questions.forms.get_mut(id) {
            entry.touched = true;
            if let Some(draft) = entry.drafts.first_mut() {
                draft.indices = index.map(|index| vec![index as u32]).unwrap_or_default();
            }
        }
        return crate::questions::forms::dispatch_answers(
            state,
            context,
            &form,
            answers_for(&form, picked, None),
        );
    }
    // Sent already: only a change takes another pick, and only one at a time.
    if !(change || state.questions.inline_answers.changing.contains(id))
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
    if previous.as_deref() == Some(picked) {
        return Vec::new();
    }
    let replacement = answers_for(&form, picked, None);
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
    let correction = answers_for(&form, picked, previous.as_deref());
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

/// The one-click answers a chat box message gives in the reader's own words, in its order: each
/// question's form and the answers its pill would send. `None` unless the whole message is such
/// answers, each starting a line with a one-click question's tag and `:`, `=`, `-` or an arrow
/// ("Q12: only on errors"), later lines belonging to the answer above them. An answer that names
/// one of the question's choices (its label or value, or "recommended") is that choice, as a click.
///
/// CDXC:SessionChat 2026-11-06 DECISION:
/// User, of two one-click questions answered in their own words that showed one as a "You answered" card and the other as a plain "Q1385: how u gonna fix this??" bubble, and reached the agent as an answers envelope and as plain text: every way of answering a one-click question (a click, the Reply popover's Send answer, its Add to prompt then Enter, typing "Q12: words" in the chat box, while the agent works or idle) shows the same answered card and reaches the agent as the same `<ghostex-page-answers>` envelope. A chat box message that is only such answers is sent as their envelopes (`composer::send::begin`); a message with other words in it stays the reader's message, and its "Q12: words" lines still count as answers (`transcript/open_questions.rs` `said_in`).
pub fn typed_answers(state: &ChatState, text: &str) -> Option<Vec<(QuestionForm, Value)>> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in text.trim().lines() {
        match typed_answer_line(line) {
            // A tag that is no one-click question's (a ```questions form's, say) leaves the
            // message the reader's own.
            Some((tag, answer)) => found.push((newest_for_tag(state, tag)?, answer.to_string())),
            None => {
                let (_, answer) = found.last_mut()?;
                answer.push('\n');
                answer.push_str(line);
            }
        }
    }
    let mut out = Vec::new();
    for (id, answer) in found {
        let answer = answer.trim();
        let form = crate::questions::forms::form(state, &id)?.clone();
        let question = form.questions.first()?;
        let named = question.choices.iter().find(|choice| {
            choice.label.eq_ignore_ascii_case(answer)
                || choice.value.eq_ignore_ascii_case(answer)
                || (choice.recommended && answer.eq_ignore_ascii_case("recommended"))
        });
        let value = named
            .map_or(answer, |choice| choice.value.as_str())
            .to_string();
        // An answer the agent has already: the new one says what it replaces, as a change does.
        let previous = sent_value(state, &form).filter(|previous| *previous != value);
        let answers = answers_for(&form, &value, previous.as_deref());
        out.push((form, answers));
    }
    (!out.is_empty()).then_some(out)
}

/// The tag a line starts with and the answer after its `:`, `=`, `-` or arrow.
fn typed_answer_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let bytes = line.as_bytes();
    let letters = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_alphabetic())
        .count();
    if letters == 0 || letters > 8 {
        return None;
    }
    let mut end = letters + usize::from(bytes.get(letters) == Some(&b'-'));
    let digits = bytes[end..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > 6 {
        return None;
    }
    end += digits;
    let rest = line[end..].trim_start();
    ["->", "\u{2192}", ":", "=", "-"]
        .iter()
        .find_map(|separator| {
            let answer = rest.strip_prefix(separator)?.trim();
            (!answer.is_empty()).then_some((&line[..end], answer))
        })
}

/// `inlineAnswerChange {form}`: Change on a sent answer wakes its other answers, and a second
/// click (or a pick of the same answer) puts them back to sleep. Any copy changes the newest.
pub fn toggle_change(state: &mut ChatState, id: &str) {
    let id = owner(state, id);
    if crate::questions::forms::open(state, &id) {
        return;
    }
    let changing = &mut state.questions.inline_answers.changing;
    if !changing.remove(&id) {
        changing.insert(id);
    }
    state.core.request_publish();
}

/// The document's one-click forms (`questionForms`), made to agree across the copies of a question
/// asked again (the SessionChat 2026-11-05 DECISION on [`copies`]): `tagWithdrawn` only when a
/// withdraw block names the tag after it was last asked, each older copy showing the newest copy's
/// state, picks and change (`askedAgain`), and `ownAnswer` the sent answer when it is none of the
/// copy's choices.
pub(crate) fn project_copies(
    state: &ChatState,
    projected: &mut std::collections::BTreeMap<String, Value>,
) {
    use crate::transcript::open_questions::{tag_state, TagState};
    for form in &state.transcript_view.question_forms {
        let Some(tag) = inline_tag(form) else {
            continue;
        };
        let owner_id = owner(state, &form.id);
        let owner_form = crate::questions::forms::form(state, &owner_id);
        let mirrored = (owner_id != form.id)
            .then(|| projected.get(&owner_id).cloned())
            .flatten();
        // "Asked again below" names a newer copy, whichever copy holds the answer.
        let asked_again = newest_for_tag(state, tag).is_some_and(|newest| newest != form.id);
        let tag_now = tag_state(&state.transcript_view.askers, tag);
        let withdrawn = tag_now == Some(TagState::Withdrawn);
        // No copy holds an answer, but a ```questions form or page answered the tag: every copy
        // shows it answered, with the form's answer when the chat has it.
        let elsewhere = (matches!(tag_now, Some(TagState::Answered | TagState::Skipped))
            && !holds_answer(state, &owner_id))
        .then(|| answer_elsewhere(state, tag));
        let own_answer = owner_form
            .and_then(|owner_form| sent_value(state, owner_form))
            .or_else(|| elsewhere.clone().flatten())
            .filter(|value| {
                !form.questions.first().is_some_and(|question| {
                    question.choices.iter().any(|choice| &choice.value == value)
                })
            });
        let Some(value) = projected.get_mut(&form.id).and_then(Value::as_object_mut) else {
            continue;
        };
        value.insert("tagWithdrawn".to_string(), json!(withdrawn));
        value.insert("askedAgain".to_string(), json!(asked_again));
        value.insert(
            "ownAnswer".to_string(),
            json!(own_answer.unwrap_or_default()),
        );
        if let Some(answer) = elsewhere {
            value.insert("state".to_string(), json!("answered"));
            value.insert("canSend".to_string(), json!(false));
            let index = answer.and_then(|answer| {
                form.questions
                    .first()?
                    .choices
                    .iter()
                    .position(|choice| choice.value == answer)
            });
            value.insert(
                "drafts".to_string(),
                json!([{ "indices": index.map(|index| vec![index]).unwrap_or_default() }]),
            );
            continue;
        }
        let (Some(mirrored), Some(owner_form)) = (mirrored, owner_form) else {
            continue;
        };
        for key in ["state", "canSend", "changing", "sendError", "reconnecting"] {
            if let Some(field) = mirrored.get(key) {
                value.insert(key.to_string(), field.clone());
            }
        }
        // The owner copy's pick, found among this copy's answers by its value.
        let picked_value = mirrored["drafts"][0]["indices"][0]
            .as_u64()
            .and_then(|index| owner_form.questions.first()?.choices.get(index as usize))
            .map(|choice| choice.value.clone());
        let index = picked_value.and_then(|picked| {
            form.questions
                .first()?
                .choices
                .iter()
                .position(|choice| choice.value == picked)
        });
        value.insert(
            "drafts".to_string(),
            json!([{ "indices": index.map(|index| vec![index]).unwrap_or_default() }]),
        );
    }
}

/// The answer a form that is no one-click question gave `tag`, newest first, when the chat holds
/// its answers.
fn answer_elsewhere(state: &ChatState, tag: &str) -> Option<String> {
    state
        .transcript_view
        .askers
        .iter()
        .rev()
        .filter(|asker| asker.answered)
        .find_map(|asker| {
            let (_, key) = asker
                .keys
                .iter()
                .find(|(known, _)| known.eq_ignore_ascii_case(tag))?;
            crate::questions::forms::received_answers(state, &asker.id)?
                .get(key)?
                .as_str()
                .map(str::to_string)
        })
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
        *state.session.queue_prompts = Some(vec![json!({"id": "p1", "text": first})]);
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

    fn reply_message(id: &str, text: &str) -> ghostex_gx_protocol::ChatMessage {
        serde_json::from_value(json!({
            "id": id,
            "role": "assistant",
            "source": "transcript",
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    fn reply(id: &str, text: &str) -> crate::state::ChatRow {
        std::sync::Arc::new(reply_message(id, text))
    }

    /// The chat after `replies`, as the transcript projection reads them: their one-click forms
    /// with open drafts, and the askers they make.
    fn state_after(replies: &[(&str, &str)]) -> ChatState {
        let messages: Vec<_> = replies.iter().map(|(id, text)| reply(id, text)).collect();
        let mut state = ChatState::default();
        for message in &messages {
            for form in crate::transcript::inline_answers::inline_forms(message) {
                if state
                    .transcript_view
                    .question_forms
                    .iter()
                    .any(|known| known.id == form.id)
                {
                    continue;
                }
                state.questions.forms.insert(
                    form.id.clone(),
                    FormState {
                        drafts: vec![Default::default()],
                        ..FormState::default()
                    },
                );
                state.transcript_view.question_forms.push(form);
            }
        }
        *state.transcript_view.askers = crate::transcript::open_questions::askers(&messages, &[]);
        state.messages.composed = messages;
        state
    }

    const FIRST: &str =
        "- Q7: Add the log line? [Add it](ghostex-answer:Q7) [Leave it](ghostex-answer:Q7)";
    const AGAIN: &str = "- Q7: Should Ghostex add the log line? [Add it](ghostex-answer:Q7) [Leave it](ghostex-answer:Q7)";

    fn ids(state: &ChatState) -> (String, String) {
        let forms = &state.transcript_view.question_forms;
        (forms[0].id.clone(), forms[1].id.clone())
    }

    #[test]
    fn asking_again_never_withdraws_and_every_copy_stays_live() {
        let state = state_after(&[("m1", FIRST), ("m2", AGAIN)]);
        let (old, new) = ids(&state);
        let projected = crate::questions::forms::project(&state);
        for id in [&old, &new] {
            assert_eq!(projected[id]["tagWithdrawn"], false, "{id}");
            assert_eq!(projected[id]["state"], "open", "{id}");
        }
        assert_eq!(projected[&old]["askedAgain"], true);
        assert_eq!(projected[&new]["askedAgain"], false);
    }

    #[test]
    fn a_withdraw_block_withdraws_every_copy() {
        let withdraw = "```questions\n{\"withdraw\": [\"Q7\"]}\n```";
        let state = state_after(&[("m1", FIRST), ("m2", AGAIN), ("m3", withdraw)]);
        let (old, new) = ids(&state);
        let projected = crate::questions::forms::project(&state);
        assert_eq!(projected[&old]["tagWithdrawn"], true);
        assert_eq!(projected[&new]["tagWithdrawn"], true);
    }

    #[test]
    fn answering_an_older_copy_answers_the_newest_once_and_every_copy_shows_it() {
        let mut state = state_after(&[("m1", FIRST), ("m2", AGAIN)]);
        let (old, new) = ids(&state);
        let context = ChatContext::default();
        let text = sent_text(&pick(&mut state, &context, &old, 1));
        assert!(text.contains(&format!("page=\"{new}\"")), "{text}");
        assert!(text.contains("\"Q7\": \"Leave it\""));
        // The newest copy now has it on its way, so no copy takes a second click.
        assert!(pick(&mut state, &context, &new, 0).is_empty());
        assert!(pick(&mut state, &context, &old, 0).is_empty());
        let projected = crate::questions::forms::project(&state);
        for id in [&old, &new] {
            assert_eq!(projected[id]["state"], "sending", "{id}");
            assert_eq!(projected[id]["drafts"][0]["indices"][0], 1, "{id}");
        }
    }

    #[test]
    fn the_readers_own_words_go_as_the_answer_and_show_on_the_pill() {
        let mut state = state_after(&[("m1", FIRST), ("m2", AGAIN)]);
        let (old, new) = ids(&state);
        let context = ChatContext::default();
        let text = sent_text(&answer_text(
            &mut state,
            &context,
            None,
            Some("q7"),
            " Only on errors ",
        ));
        assert!(text.contains(&format!("page=\"{new}\"")), "{text}");
        assert!(text.contains("\"Q7\": \"Only on errors\""));
        let projected = crate::questions::forms::project(&state);
        for id in [&old, &new] {
            assert_eq!(projected[id]["ownAnswer"], "Only on errors", "{id}");
            assert!(projected[id]["drafts"][0]["indices"]
                .as_array()
                .unwrap()
                .is_empty());
        }
        assert!(answer_text(&mut state, &context, Some(&old), None, "  ").is_empty());
    }

    #[test]
    fn a_tag_jump_goes_to_the_newest_reply_that_asks_it() {
        // Asked a third time in the first words: the same form as the first copy.
        let mut state = state_after(&[("m1", FIRST), ("m2", AGAIN), ("m3", FIRST)]);
        let (old, _) = ids(&state);
        assert_eq!(
            crate::questions::open_questions::newest_one_click_ask_for_test(&state, "Q7"),
            Some(("m3".to_string(), old))
        );
        let _ = &mut state;
    }

    #[test]
    fn answering_the_first_copy_of_q1652_shows_every_copy_answered() {
        use crate::transcript::inline_answers::tests::{Q1652_AGAIN, Q1652_FIRST};
        let mut state = state_after(&[("m1", Q1652_FIRST), ("m2", Q1652_AGAIN)]);
        let (old, new) = ids(&state);
        let old_form = state.transcript_view.question_forms[0].clone();
        assert_eq!(
            old_form.questions[0].title,
            "Q1652: Include an iOS TestFlight build with 11.0.0?"
        );
        // The reader answered the first copy, and the answers are in the transcript.
        state
            .transcript_view
            .page_answers
            .push(crate::transcript::page_answers::PageAnswers {
                page_id: old.clone(),
                title: old_form.title.clone(),
                answers: json!({"Q1652": "Yes, build iOS too"}),
            });
        *state.transcript_view.askers =
            crate::transcript::open_questions::askers(&state.messages.composed, &[old.clone()]);
        assert_eq!(
            owner(&state, &new),
            old,
            "every copy follows the answered one"
        );
        let projected = crate::questions::forms::project(&state);
        for id in [&old, &new] {
            assert_eq!(projected[id]["state"], "answered", "{id}");
            assert_eq!(projected[id]["drafts"][0]["indices"][0], 0, "{id}");
        }
        assert_eq!(projected[&old]["askedAgain"], true);
        assert_eq!(projected[&new]["askedAgain"], false);
        // A click on the later copy sends nothing, and a change goes against the answered copy.
        let context = ChatContext::default();
        assert!(pick(&mut state, &context, &new, 1).is_empty());
        toggle_change(&mut state, &new);
        let text = sent_text(&pick(&mut state, &context, &new, 1));
        assert!(text.contains(&format!("page=\"{old}\"")), "{text}");
        assert!(text.contains("Changed from Yes, build iOS too"), "{text}");
    }

    #[test]
    fn a_one_click_copy_of_a_tag_a_questions_form_answered_shows_that_answer() {
        let fenced = "```questions\n{\"questions\":[{\"tag\":\"Q3\",\"title\":\"Ship?\",\"choices\":[\"Yes\",\"No\"]}]}\n```";
        let mut state = state_after(&[
            ("m1", fenced),
            (
                "m2",
                "- Q3: Ship? [Yes](ghostex-answer:Q3) [No](ghostex-answer:Q3)",
            ),
        ]);
        let form = crate::transcript::question_forms::question_forms(&state.messages.composed)
            .into_iter()
            .find(|form| !form.inline)
            .unwrap();
        let key = form.questions[0].key.clone();
        state
            .transcript_view
            .page_answers
            .push(crate::transcript::page_answers::PageAnswers {
                page_id: form.id.clone(),
                title: form.title.clone(),
                answers: json!({ key: "No" }),
            });
        *state.transcript_view.askers =
            crate::transcript::open_questions::askers(&state.messages.composed, &[form.id.clone()]);
        let inline = state.transcript_view.question_forms[0].id.clone();
        let projected = crate::questions::forms::project(&state);
        assert_eq!(projected[&inline]["state"], "answered");
        assert_eq!(projected[&inline]["drafts"][0]["indices"][0], 1);
    }
}
