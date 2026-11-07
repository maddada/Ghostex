//! The reader's side of a question form (a ```questions block, `transcript/question_forms.rs`):
//! picking choices, the note or text answer, sending the answers to the agent, and the saved picks.
//!
//! A choice question's picks follow the page kit (`server/src/visual_pages/kit.rs`): nothing starts
//! picked, clicking a pick again clears it, one of N replaces the pick while many of N toggles, and
//! "Other / none of these" (the row after the last choice) is a pick of its own whose note says
//! what instead. A question left blank when the form is sent goes to the agent as `"skipped"`, and
//! Skip all (`skip_all`) sends at once. The `<ghostex-page-answers>` message uses the normal chat
//! send unless the Advanced setting asks to wait for the turn to end. The form reads Answered once
//! the transcript carries the message.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::document::QuestionDraft;
use crate::effect::Effect;
use crate::event::StorageKey;
use crate::questions::drafts::{decode_drafts, encode_drafts, form_drafts_key, AnswerDrafts};
use crate::state::{ChatContext, ChatState, FormSend, FormState, PendingSend};
use crate::transcript::question_forms::{
    answers_message, notes_key, takes_notes, QuestionForm, NOTE_SUFFIX, OTHER_CHOICE, SKIPPED,
};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// Every form the chat can act on: the transcript's, and the one the Resources modal read back
/// for an item older than the loaded rows (`questions/resource_modal.rs`).
fn all_forms(state: &ChatState) -> impl Iterator<Item = &QuestionForm> {
    state
        .transcript_view
        .question_forms
        .iter()
        .chain(crate::questions::resource_modal::fetched_form(state))
}

pub(crate) fn form<'a>(state: &'a ChatState, id: &str) -> Option<&'a QuestionForm> {
    all_forms(state).find(|form| form.id == id && form.error.is_empty())
}

/// The answers the transcript already carries for this form.
pub(crate) fn received_answers<'a>(state: &'a ChatState, id: &str) -> Option<&'a Value> {
    state
        .transcript_view
        .page_answers
        .iter()
        .find(|answers| answers.page_id == id)
        .or_else(|| crate::questions::resource_modal::fetched_answers(state, id))
        .map(|answers| &answers.answers)
        .or_else(|| crate::questions::recorded_answers::answers(state, id))
        .or_else(|| {
            let entry = state
                .questions
                .forms
                .get(id)
                .filter(|entry| entry.confirmed)?;
            entry
                .send
                .as_ref()
                .map(|send| &send.answers)
                .or(entry.sent.as_ref())
        })
}

/// Whether the session's resource index records the form answered, also when its answers are
/// outside the loaded rows or wait in the queue (`questions/open_questions.rs`, `settled_by_index`).
pub(crate) fn answered_by_index(state: &ChatState, id: &str) -> bool {
    crate::questions::resource_index::settled_by_index(&state.session.resource_index, "form")
        .get(id)
        == Some(&"answered")
}

/// Whether the session's queue holds answers for the form.
pub(crate) fn queued_in_queue(state: &ChatState, id: &str) -> bool {
    state.session.queue_prompts.as_ref().is_some_and(|queue| {
        queue.iter().any(|prompt| {
            prompt
                .get("text")
                .and_then(Value::as_str)
                .and_then(crate::transcript::page_answers::answers_page_id)
                .as_deref()
                == Some(id)
        })
    })
}

/// Whether the form's answers are still on their way: this client is sending them or they wait in
/// the queue, and no real transcript row records them yet. The pending "You answered" card already
/// carries them as a row, so the transcript's answers alone cannot tell this apart from answered.
///
/// CDXC:SessionChat 2026-11-02 WHY:
/// A form answered while the agent worked, with "Wait for turn end before sending form answers" on, read Answered in its header while the card under it said "Waiting for the agent to finish its turn…": the pending card's row counted as the transcript carrying the answers. The header reads Queued (or Sending) until a real transcript row, this client's confirmation or the session's resource index records them.
fn awaiting_delivery(state: &ChatState, id: &str) -> bool {
    if state.questions.form_answer_index.holds(id)
        || crate::questions::resource_index::answered_in_transcript(
            &state.session.resource_index,
            id,
        )
    {
        return false;
    }
    let local =
        state.questions.forms.get(id).is_some_and(|entry| {
            !entry.confirmed && (entry.send.is_some() || entry.sent.is_some())
        });
    local || queued_in_queue(state, id)
}

/// Whether the reader can still change the form. It never waits for the saved picks: the rows
/// that find a new form are rebuilt after this family's sync, so their read can leave an event
/// late, and picks the reader makes first win over it.
pub(crate) fn open(state: &ChatState, id: &str) -> bool {
    received_answers(state, id).is_none()
        && !answered_by_index(state, id)
        && !crate::questions::later::removed_ids(state).contains(id)
        && !crate::questions::resource_modal::is_withdrawn(state, id)
        && state
            .questions
            .forms
            .get(id)
            .is_some_and(|entry| entry.send.is_none() && entry.sent.is_none())
}

/// The reader took a form's answers out of the queue before they were typed, so the form is open
/// again with its saved picks.
pub fn reopen_after_removed_answers(state: &mut ChatState, queued_prompt_id: &str) {
    let page = state
        .pending
        .sends
        .iter()
        .find(|entry| entry.queued_prompt_id.as_deref() == Some(queued_prompt_id))
        .and_then(|entry| crate::transcript::page_answers::answers_page_id(&entry.text));
    if let Some(entry) = page.and_then(|page| state.questions.forms.get_mut(&page)) {
        entry.sent = None;
        entry.sent_waiting_for_idle = false;
        entry.confirmation_boundary = None;
        entry.confirmed = false;
        state.core.request_publish();
    }
}

fn answered(draft: &QuestionDraft) -> bool {
    draft.skipped
        || !draft.indices.is_empty()
        || draft.other_picked
        || !draft.other.trim().is_empty()
}

/// The key the form's notes are saved under in its drafts record, beside the questions' indices.
const NOTES_DRAFT: &str = "notes";

fn save(state: &ChatState, id: &str) -> Vec<Effect> {
    let Some(entry) = state.questions.forms.get(id) else {
        return Vec::new();
    };
    let mut drafts: AnswerDrafts = entry
        .drafts
        .iter()
        .enumerate()
        .filter(|(_, draft)| answered(draft))
        .map(|(index, draft)| (index.to_string(), draft.clone()))
        .collect();
    if !entry.notes.trim().is_empty() {
        drafts.insert(
            NOTES_DRAFT.to_string(),
            QuestionDraft {
                other: entry.notes.clone(),
                ..QuestionDraft::default()
            },
        );
    }
    vec![Effect::WriteStorage {
        key: form_drafts_key(&state.identity.session_key, id),
        value: encode_drafts(&drafts),
        durable: false,
    }]
}

/// `formChoice {form, question, choice}`: a click on a choice, where `choice` one past the last
/// choice is "Other / none of these".
pub fn pick(
    state: &mut ChatState,
    context: &ChatContext,
    id: &str,
    question: usize,
    choice: usize,
) -> Vec<Effect> {
    if !open(state, id) {
        return Vec::new();
    }
    let Some(asked) = form(state, id)
        .and_then(|form| form.questions.get(question))
        .cloned()
    else {
        return Vec::new();
    };
    if asked.text || choice > asked.choices.len() {
        return Vec::new();
    }
    let Some(entry) = state.questions.forms.get_mut(id) else {
        return Vec::new();
    };
    entry.touched = true;
    let Some(draft) = entry.drafts.get_mut(question) else {
        return Vec::new();
    };
    // A pick takes back a Skip all that failed to send.
    draft.skipped = false;
    let index = choice as u32;
    if choice == asked.choices.len() {
        draft.other_picked = !draft.other_picked;
        if draft.other_picked && !asked.multiple {
            draft.indices.clear();
        }
    } else if asked.multiple {
        match draft.indices.iter().position(|picked| *picked == index) {
            Some(at) => {
                draft.indices.remove(at);
            }
            None => {
                draft.indices.push(index);
                draft.indices.sort_unstable();
            }
        }
    } else if draft.indices == [index] {
        draft.indices.clear();
    } else {
        draft.indices = vec![index];
        draft.other_picked = false;
    }
    let picked_choice = choice < asked.choices.len() && draft.indices == [index];
    state.core.request_publish();
    let mut effects = save(state, id);
    // CDXC:SessionChat 2026-10-16 DECISION: User: "please add a setting to enable the behavior ... but keep it disabled by default. and in case this is enabled and here's only 1 question in the card then just seelcting sends in that case", "make it an advanced setting". With Send when you pick an answer on (`chatSendOnPick`), picking a choice sends the form at once, but only for a form of exactly one single-choice question: never two or more, a multi-select, "Other / none of these" or a text question. It goes through `send`, the form's one send path. The same rule sends an agent's async question card (`async_question_option_picked` in questions/actions.rs).
    if state.core.send_on_pick_enabled
        && picked_choice
        && !asked.multiple
        && form(state, id).is_some_and(|form| form.questions.len() == 1)
    {
        effects.extend(send(state, context, id));
    }
    effects
}

/// `formText {form, question, text}`: the question's note, or a text question's answer.
pub fn write(state: &mut ChatState, id: &str, question: usize, text: &str) -> Vec<Effect> {
    if !open(state, id) {
        return Vec::new();
    }
    // Typing a text question's answer takes back a Skip all that failed to send; a choice question's note does not.
    let unskips = asked_text(state, id, question) && !text.trim().is_empty();
    let Some(entry) = state.questions.forms.get_mut(id) else {
        return Vec::new();
    };
    entry.touched = true;
    let Some(draft) = entry.drafts.get_mut(question) else {
        return Vec::new();
    };
    draft.other = text.to_string();
    if unskips {
        draft.skipped = false;
    }
    state.core.request_publish();
    save(state, id)
}

fn asked_text(state: &ChatState, id: &str, question: usize) -> bool {
    form(state, id)
        .and_then(|form| form.questions.get(question))
        .is_some_and(|asked| asked.text)
}

/// `formSkipAll {form}`: sends the form at once with every question as `"skipped"` and no notes,
/// whatever the reader picked or typed in it (see `send_answers`).
pub fn skip_all(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    send_answers(state, context, id, true)
}

/// `formNotes {form, text}`: the form's own notes box, under the last question.
///
/// CDXC:SessionChat 2026-10-14 DECISION: User: "please add a text area below the last question when asking questions inline in the chat view if there's more than 1 questino being asked which lets the user add more notes there if he wants (make it consisnt with the rest of the question card UI pls)". A form of two or more questions ends with an optional notes box (desktop and phone, drawn like a question's note box); its text reaches the agent last in the answers, under "Additional notes", is kept with the form's saved picks, and an empty box sends exactly what the form sent before.
pub fn write_notes(state: &mut ChatState, id: &str, text: &str) -> Vec<Effect> {
    if !open(state, id) || !form(state, id).is_some_and(takes_notes) {
        return Vec::new();
    }
    let Some(entry) = state.questions.forms.get_mut(id) else {
        return Vec::new();
    };
    entry.touched = true;
    entry.notes = text.to_string();
    state.core.request_publish();
    save(state, id)
}

/// The answers as the agent receives them, keyed like the page kit's. gxserver's voice agent
/// builds its answers with this too (server/src/voice_agent/question_forms.rs), so they match a
/// form sent from the chat.
pub fn answers(form: &QuestionForm, drafts: &[QuestionDraft], notes: &str) -> Value {
    let mut out = Map::new();
    for (asked, draft) in form.questions.iter().zip(drafts) {
        let note = draft.other.trim();
        if draft.skipped {
            out.insert(asked.key.clone(), json!(SKIPPED));
            if !asked.text && !note.is_empty() {
                out.insert(format!("{}{NOTE_SUFFIX}", asked.key), json!(note));
            }
            continue;
        }
        if asked.text {
            out.insert(
                asked.key.clone(),
                if note.is_empty() {
                    Value::Null
                } else {
                    json!(note)
                },
            );
            continue;
        }
        let mut picked: Vec<Value> = draft
            .indices
            .iter()
            .filter_map(|index| asked.choices.get(*index as usize))
            .map(|choice| json!(choice.value))
            .collect();
        if draft.other_picked {
            picked.push(json!(OTHER_CHOICE));
        }
        let value = if asked.multiple {
            if picked.is_empty() {
                Value::Null
            } else {
                Value::Array(picked)
            }
        } else {
            picked.into_iter().next().unwrap_or(Value::Null)
        };
        out.insert(asked.key.clone(), value);
        if !note.is_empty() {
            out.insert(format!("{}{NOTE_SUFFIX}", asked.key), json!(note));
        }
    }
    let notes = notes.trim();
    if takes_notes(form) && !notes.is_empty() {
        out.insert(notes_key(form), json!(notes));
    }
    Value::Object(out)
}

/// `formSend {form}`: the answers go to the agent as a chat message.
pub fn send(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    send_answers(state, context, id, false)
}

/// Sends the form: the reader's answers, or with `skip_everything` (Skip all) every question as
/// `"skipped"` and no notes, never a pick.
///
/// CDXC:SessionChat 2026-11-01 DECISION: User: "No we shouldn't send right away, we should just send normally, it'll send the message after tool call finishes". This supersedes the 2026-10-11 queue-until-idle rule; the Advanced toggle keeps it available.
fn send_answers(
    state: &mut ChatState,
    context: &ChatContext,
    id: &str,
    skip_everything: bool,
) -> Vec<Effect> {
    if !open(state, id) {
        return Vec::new();
    }
    let Some(form) = form(state, id).cloned() else {
        return Vec::new();
    };
    let (drafts, notes) = if skip_everything {
        let skipped = QuestionDraft {
            skipped: true,
            ..QuestionDraft::default()
        };
        (vec![skipped; form.questions.len()], String::new())
    } else {
        state
            .questions
            .forms
            .get(id)
            .map(|entry| (entry.drafts.clone(), entry.notes.clone()))
            .unwrap_or_default()
    };
    // A question left blank is sent as `"skipped"` (the SessionChat 2026-10-16 DECISION on `SKIPPED` in packages/gx-visual/src/questions.rs), so a form may be sent with any number of questions unanswered, none included (then it is Skip all).
    let mut drafts = drafts;
    drafts.resize(form.questions.len(), QuestionDraft::default());
    for draft in drafts.iter_mut().filter(|draft| !answered(draft)) {
        draft.skipped = true;
    }
    let answers = answers(&form, &drafts, &notes);
    dispatch_answers(state, context, &form, answers)
}

/// Sends `answers` for `form` as the agent's `<ghostex-page-answers>` message and records the send
/// on the form's entry, whatever the form's state: `send_answers` checks the form is open first,
/// and a one-click answer that changes a sent one sends again (`questions/inline_answers.rs`).
pub(crate) fn dispatch_answers(
    state: &mut ChatState,
    context: &ChatContext,
    form: &QuestionForm,
    answers: Value,
) -> Vec<Effect> {
    let id = form.id.as_str();
    let queue_until_idle = state.core.answers_wait_for_idle;
    let request_id = state.core.allocate_request_id();
    let params = json!({
        "text": answers_message(form, &answers),
        "sendRequestId": format!("question-form-{id}-{}", context.now_millis()),
    });
    let effect = Effect::SendRpc {
        request_id,
        method: if queue_until_idle {
            ChatRpcMethod::QueueSessionChatPrompt
        } else {
            ChatRpcMethod::SendSessionChatMessage
        },
        params: Box::new(params.clone()),
    };
    let last_message = state.messages.list.last();
    let boundary = PendingSend {
        sent_at_ms: context.now_millis(),
        after_message_id: last_message.map(|message| message.id.clone()),
        after_message_timestamp: last_message.map_or(Some(i64::MIN), |message| message.timestamp),
        // A bounded replacement can bring an earlier confirmed answer back after the boundary
        // row has left; it must not hide an identical, genuinely new send.
        matching_occurrence: state.questions.forms.get(id).and_then(|form| {
            let used = form.confirmed_message_ids.len()
                + form
                    .confirmed_queue_answers
                    .iter()
                    .filter(|(_, value, row)| {
                        *value == answers && !form.confirmed_message_ids.contains(row)
                    })
                    .count();
            (used != 0).then(|| u32::try_from(used).unwrap_or(u32::MAX).saturating_add(1))
        }),
        ..PendingSend::default()
    };
    if let Some(entry) = state.questions.forms.get_mut(id) {
        entry.error = None;
        entry.confirmation_boundary = Some(boundary);
        entry.confirmed = false;
        entry.queued_prompt_id = None;
        entry.send = Some(FormSend {
            request_id,
            answers,
            params,
            queue_until_idle,
            retry: Default::default(),
        });
    }
    state.core.request_publish();
    vec![effect]
}

/// The send's answer, or `None` when the id is not a form's.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let id = state
        .questions
        .forms
        .iter()
        .find(|(_, entry)| {
            entry
                .send
                .as_ref()
                .is_some_and(|send| send.request_id == request_id)
        })
        .map(|(id, _)| id.clone())?;
    let mut sent = state.questions.forms.get_mut(&id)?.send.take()?;
    state.core.request_publish();
    if crate::questions::unreachable_retry::Retry::wait_out(&mut sent.retry, outcome) {
        // gxserver never answered: the answers stay on their way and go again once it is back.
        if let Some(entry) = state.questions.forms.get_mut(&id) {
            entry.send = Some(sent);
        }
        return Some(Vec::new());
    }
    match outcome {
        // The saved picks stay until the transcript carries the answers (`sync`).
        RpcOutcome::Ok { result } => {
            // A Ghostex Collab guest's answers wait for the chat's owner (Ask me): they are not on
            // their way to the agent, so no echo stands for them and the chat is not working.
            if let Some(held_id) = result
                .pointer("/collab/heldId")
                .and_then(Value::as_str)
                .filter(|_| result.pointer("/collab/held") == Some(&Value::Bool(true)))
            {
                let echo = format!("answers:{id}");
                state.pending.sends.retain(|entry| entry.id != echo);
                state.messages.new_composition_identity();
                if let Some(entry) = state.questions.forms.get_mut(&id) {
                    entry.sent = Some(sent.answers);
                    entry.sent_waiting_for_idle = false;
                    entry.held_for_owner = Some(held_id.to_string());
                }
                return Some(Vec::new());
            }
            let queued_prompt_id = result
                .get("queuedPromptId")
                .and_then(Value::as_str)
                .or_else(|| {
                    sent.queue_until_idle
                        .then(|| result.pointer("/prompt/id").and_then(Value::as_str))
                        .flatten()
                })
                .filter(|prompt_id| !prompt_id.is_empty());
            if !sent.queue_until_idle {
                // The normal send may reach a busy agent's own input before its transcript
                // records it. Keep the optimistic answers card through that interval.
                if let Some(echo) = state
                    .pending
                    .sends
                    .iter_mut()
                    .find(|echo| echo.id == format!("answers:{id}"))
                {
                    if let Some(delivery) = echo.startup_delivery.as_mut() {
                        delivery.state = "sending".to_string();
                    }
                }
            }
            if let Some(entry) = state.questions.forms.get_mut(&id) {
                entry.sent = Some(sent.answers);
                entry.sent_waiting_for_idle = sent.queue_until_idle;
                if let Some(prompt_id) = queued_prompt_id {
                    entry.queued_prompt_id = Some(prompt_id.to_string());
                    crate::session::answer_echoes::remember_current_receipt(entry);
                }
            }
            if let Some(prompt_id) = queued_prompt_id {
                let mut detached = None;
                if let Some(echo) = state.pending.sends.iter_mut().find(|echo| {
                    echo.id == format!("answers:{id}")
                        && echo.queued_prompt_id.as_deref() != Some(prompt_id)
                }) {
                    // The queue broadcast may have paired an identical, different send first.
                    // Keep its receipt even if it has already left the queue.
                    if let Some(other_id) = echo.queued_prompt_id.as_deref() {
                        let mut other = echo.clone();
                        other.id = format!("startup:{other_id}");
                        other.matching_occurrence =
                            Some(other.matching_occurrence.unwrap_or(1).saturating_add(1));
                        detached = Some(other);
                    }
                    echo.queued_prompt_id = Some(prompt_id.to_string());
                    echo.left_queue_at_ms = None;
                    if let Some(delivery) = echo.startup_delivery.as_mut() {
                        delivery.prompt_id = prompt_id.to_string();
                        delivery.state = "sending".to_string();
                        delivery.error_message = None;
                        delivery.wait_reason = None;
                    }
                }
                if let Some(other) = detached.filter(|other| {
                    !state
                        .pending
                        .sends
                        .iter()
                        .any(|entry| entry.queued_prompt_id == other.queued_prompt_id)
                }) {
                    state.pending.sends.push(other);
                }
            }
            // The queue answers with its whole new strip, which carries the answers' row: the card
            // the send drew keeps its place instead of waiting for the queue's broadcast.
            if let Some(queue) = result.get("queue").and_then(Value::as_array) {
                state.session.queue_prompts = Some(queue.clone());
                state.messages.new_composition_identity();
            }
            Some(Vec::new())
        }
        RpcOutcome::Err { message, .. } => {
            if let Some(entry) = state.questions.forms.get_mut(&id) {
                entry.error = Some(message.clone());
            }
            Some(Vec::new())
        }
    }
}

/// Adopts a form's saved picks.
pub fn storage_loaded(state: &mut ChatState, id: &str, value: Option<&str>) {
    let saved = value.map(decode_drafts).unwrap_or_default();
    if let Some(entry) = state
        .questions
        .forms
        .get_mut(id)
        .filter(|entry| !entry.touched)
    {
        for (index, draft) in entry.drafts.iter_mut().enumerate() {
            if let Some(stored) = saved.get(&index.to_string()) {
                *draft = stored.clone();
            }
        }
        if let Some(stored) = saved.get(NOTES_DRAFT) {
            entry.notes = stored.other.clone();
        }
    }
    state.core.request_publish();
}

/// Gives every open form its blank picks and asks for the saved ones, once, and forgets the saved
/// picks of a form whose answers the transcript now carries.
pub fn sync(state: &mut ChatState) -> Vec<Effect> {
    let mut effects = Vec::new();
    let session_key = state.identity.session_key.clone();
    let answered: Vec<String> = state
        .transcript_view
        .page_answers
        .iter()
        .map(|answers| answers.page_id.clone())
        .chain(
            state
                .questions
                .resource_modal
                .iter()
                .filter(|modal| modal.answers.is_some())
                .map(|modal| modal.id.clone()),
        )
        .collect();
    let index = &state.session.resource_index;
    for (id, entry) in state.questions.forms.iter_mut() {
        let in_transcript = answered.contains(id)
            || crate::questions::resource_index::answered_in_transcript(index, id);
        if !entry.cleared && in_transcript {
            entry.cleared = true;
            effects.push(Effect::WriteStorage {
                key: form_drafts_key(&session_key, id),
                value: None,
                durable: false,
            });
        }
    }
    let open_forms: Vec<(String, usize)> = all_forms(state)
        .filter(|form| {
            form.error.is_empty()
                && received_answers(state, &form.id).is_none()
                && !answered_by_index(state, &form.id)
        })
        .map(|form| (form.id.clone(), form.questions.len()))
        .collect();
    for (id, count) in open_forms {
        let entry = state.questions.forms.entry(id.clone()).or_default();
        if entry.load_requested {
            continue;
        }
        entry.load_requested = true;
        entry.drafts = vec![QuestionDraft::default(); count];
        effects.push(Effect::ReadStorage {
            key: form_drafts_key(&session_key, &id),
        });
    }
    effects
}

/// The answers as the form shows them once sent: the picks they name.
fn drafts_from_answers(form: &QuestionForm, answers: &Value) -> Vec<QuestionDraft> {
    form.questions
        .iter()
        .map(|asked| {
            let value = answers.get(&asked.key).unwrap_or(&Value::Null);
            let note = answers
                .get(format!("{}{NOTE_SUFFIX}", asked.key))
                .and_then(Value::as_str)
                .unwrap_or_default();
            if value.as_str() == Some(SKIPPED) {
                return QuestionDraft {
                    other: if asked.text {
                        String::new()
                    } else {
                        note.to_string()
                    },
                    skipped: true,
                    ..QuestionDraft::default()
                };
            }
            if asked.text {
                return QuestionDraft {
                    other: value.as_str().unwrap_or_default().to_string(),
                    ..QuestionDraft::default()
                };
            }
            let values: Vec<&str> = match value {
                Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
                Value::String(text) => vec![text.as_str()],
                _ => Vec::new(),
            };
            QuestionDraft {
                indices: asked
                    .choices
                    .iter()
                    .enumerate()
                    .filter(|(_, choice)| values.contains(&choice.value.as_str()))
                    .map(|(index, _)| index as u32)
                    .collect(),
                other: note.to_string(),
                other_picked: values.contains(&OTHER_CHOICE),
                skipped: false,
            }
        })
        .collect()
}

/// The form's notes as the answers carry them.
fn notes_from_answers(form: &QuestionForm, answers: &Value) -> String {
    answers
        .get(notes_key(form))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// The document's `questionForms`: each form with what the reader picked, or what was sent.
pub fn project(state: &ChatState) -> BTreeMap<String, Value> {
    let mut projected = project_each(state);
    crate::questions::inline_answers::project_copies(state, &mut projected);
    projected
}

fn project_each(state: &ChatState) -> BTreeMap<String, Value> {
    let later = crate::questions::open_questions::later_waiting(state);
    let removed = crate::questions::later::removed_ids(state);
    let index = &state.session.resource_index;
    let on_the_way = crate::questions::open_questions::answers_on_the_way(state);
    all_forms(state)
        .map(|form| {
            let entry = state.questions.forms.get(&form.id);
            let fallback = FormState::default();
            let entry = entry.unwrap_or(&fallback);
            // A guest's answers the owner still holds, also after the page reloaded.
            let held = crate::extras::collab::own_held_answers(state, &form.id);
            let sent = received_answers(state, &form.id)
                .or(entry.sent.as_ref())
                .or(held.as_ref());
            let (phase, drafts, notes) = match (sent, entry.send.as_ref()) {
                (Some(answers), _)
                    if received_answers(state, &form.id).is_some()
                        && !awaiting_delivery(state, &form.id) =>
                {
                    (
                        "answered",
                        drafts_from_answers(form, answers),
                        notes_from_answers(form, answers),
                    )
                }
                (Some(answers), _) => (
                    // Recorded in the transcript where this client's confirmation missed it.
                    if crate::questions::resource_index::answered_in_transcript(index, &form.id) {
                        "answered"
                    } else if entry.held_for_owner.is_some()
                        || (entry.sent.is_none() && held.is_some())
                    {
                        "held"
                    } else if entry.sent_waiting_for_idle || queued_in_queue(state, &form.id) {
                        "queued"
                    } else {
                        "sending"
                    },
                    drafts_from_answers(form, answers),
                    notes_from_answers(form, answers),
                ),
                (None, Some(send)) => (
                    "sending",
                    drafts_from_answers(form, &send.answers),
                    notes_from_answers(form, &send.answers),
                ),
                // Answered where this client does not see it: from another client's queue, or
                // outside the loaded rows while `recorded_answers` reads the picks back.
                (None, None) if answered_by_index(state, &form.id) => {
                    match on_the_way.iter().find(|answers| answers.page_id == form.id) {
                        Some(queued) => (
                            "queued",
                            drafts_from_answers(form, &queued.answers),
                            notes_from_answers(form, &queued.answers),
                        ),
                        None => (
                            "answered",
                            vec![QuestionDraft::default(); form.questions.len()],
                            String::new(),
                        ),
                    }
                }
                (None, None) if entry.drafts.len() == form.questions.len() => {
                    ("open", entry.drafts.clone(), entry.notes.clone())
                }
                (None, None) => (
                    "open",
                    vec![QuestionDraft::default(); form.questions.len()],
                    String::new(),
                ),
            };
            // A form the reader removed without answering reads Canceled: it opens to be read but
            // takes no answer (`questions/later.rs`).
            let phase = if phase == "open" && removed.contains(&form.id) {
                "canceled"
            } else if phase == "open"
                && crate::questions::resource_modal::is_withdrawn(state, &form.id)
            {
                // Only the modal's form reads Withdrawn: it opens to be read and takes no answer.
                "withdrawn"
            } else {
                phase
            };
            let takes_notes = takes_notes(form);
            let mut value = serde_json::to_value(form).unwrap_or_default();
            crate::questions::form_links::add_links(&mut value);
            if let Some(object) = value.as_object_mut() {
                object.insert("state".to_string(), json!(phase));
                object.insert("canSend".to_string(), json!(phase == "open"));
                // CDXC:SessionChat 2026-10-12 DECISION: User: "i see the answer twice fam" (the answered form showed the picks and "Sent to the agent" while the answer message below repeated them); picked option c of docs/2026-10-12/handover-colors-and-answers/index.html: once the transcript carries a form's answers it collapses to one line (a check, the title, "Answered", a chevron) that opens the full read-only form and closes it again, collapsed by default and after a reload; the "You answered" card below stays as the record.
                object.insert(
                    "collapsed".to_string(),
                    // The Resources modal shows the answers: it never starts collapsed.
                    json!(
                        phase == "answered"
                            && !entry.expanded
                            && !crate::questions::resource_modal::is_open_for(state, &form.id)
                    ),
                );
                object.insert("otherLabel".to_string(), json!(OTHER_CHOICE));
                // Put off For later while it still waits (`questions/later.rs`).
                object.insert(
                    "later".to_string(),
                    json!(phase == "open" && later.contains(&form.id)),
                );
                object.insert("drafts".to_string(), json!(drafts));
                // A one-click question's `tagWithdrawn`, `askedAgain` and `ownAnswer` are set after,
                // across its copies (`questions/inline_answers.rs`, `project_copies`).
                // A one-click answer the reader is changing (`questions/inline_answers.rs`).
                object.insert(
                    "changing".to_string(),
                    json!(state.questions.inline_answers.changing.contains(&form.id)),
                );
                object.insert("takesNotes".to_string(), json!(takes_notes));
                object.insert("notes".to_string(), json!(notes));
                object.insert(
                    "sendError".to_string(),
                    json!(entry.error.clone().unwrap_or_default()),
                );
                // A guest's held answers say who they wait for (`extras::collab`).
                if phase == "held" {
                    object.insert(
                        "heldFor".to_string(),
                        json!(crate::extras::collab::owner_name(state)),
                    );
                }
                object.insert(
                    "reconnecting".to_string(),
                    json!(entry
                        .send
                        .as_ref()
                        .is_some_and(|send| send.retry.reconnecting)),
                );
            }
            (form.id.clone(), value)
        })
        .collect()
}

/// `formToggle {form}`: opens an answered form's full view, or collapses it again.
pub fn toggle(state: &mut ChatState, id: &str) {
    if received_answers(state, id).is_none() && !answered_by_index(state, id) {
        return;
    }
    let entry = state.questions.forms.entry(id.to_string()).or_default();
    entry.expanded = !entry.expanded;
    state.core.request_publish();
}

/// Whether a storage key is a question form's, and which.
pub fn form_of_key(key: &StorageKey) -> Option<String> {
    crate::questions::drafts::form_of_drafts_key(key)
}

/// Sends again every form whose answers gxserver never took, once the retry timer fires.
pub fn resend_unreachable(state: &mut ChatState) -> Vec<Effect> {
    let ids: Vec<String> = state
        .questions
        .forms
        .iter()
        .filter(|(_, entry)| entry.send.as_ref().is_some_and(|send| send.retry.due()))
        .map(|(id, _)| id.clone())
        .collect();
    let mut effects = Vec::new();
    for id in ids {
        let request_id = state.core.allocate_request_id();
        let Some(send) = state
            .questions
            .forms
            .get_mut(&id)
            .and_then(|entry| entry.send.as_mut())
        else {
            continue;
        };
        send.request_id = request_id;
        send.retry.in_flight = true;
        effects.push(Effect::SendRpc {
            request_id,
            method: if send.queue_until_idle {
                ChatRpcMethod::QueueSessionChatPrompt
            } else {
                ChatRpcMethod::SendSessionChatMessage
            },
            params: Box::new(send.params.clone()),
        });
    }
    effects
}

/// Whether a form's answers wait for gxserver to come back.
pub fn any_waiting_for_server(state: &ChatState) -> bool {
    state
        .questions
        .forms
        .values()
        .any(|entry| entry.send.as_ref().is_some_and(|send| send.retry.due()))
}

#[cfg(test)]
mod canceled_tests {
    use super::*;
    use crate::transcript::question_forms::parse_form;

    fn state_with_form() -> (ChatState, String) {
        let form = parse_form(
            r#"{"title": "T", "questions": [{"title": "Pick?", "choices": ["Yes", "No"]}]}"#,
        );
        let id = form.id.clone();
        let mut state = ChatState::default();
        state.transcript_view.question_forms.push(form);
        state
            .questions
            .forms
            .insert(id.clone(), FormState::default());
        (state, id)
    }

    /// A removed form reads `canceled`: it can be read but takes no pick, note, Skip all or Send.
    #[test]
    fn a_canceled_form_is_read_only() {
        let (mut state, id) = state_with_form();
        assert_eq!(project(&state)[&id]["state"], "open");
        assert_eq!(project(&state)[&id]["canSend"], true);

        crate::questions::later::remove(&mut state, &id);
        let value = &project(&state)[&id];
        assert_eq!(value["state"], "canceled");
        assert_eq!(value["canSend"], false);
        assert_eq!(value["later"], false);
        assert_eq!(value["collapsed"], false, "it opens to be read");

        let context = ChatContext::default();
        assert!(pick(&mut state, &context, &id, 0, 0).is_empty());
        assert!(send(&mut state, &context, &id).is_empty());
        assert!(skip_all(&mut state, &context, &id).is_empty());
        assert!(state.questions.forms[&id].send.is_none());
        assert!(!state.questions.forms[&id].touched, "no pick landed");
    }

    #[test]
    fn an_indexed_answer_keeps_a_reloaded_form_read_only() {
        let (mut state, id) = state_with_form();
        state.session.resource_index = vec![json!({
            "id": id, "kind": "form", "status": "answered", "answersOffset": 300
        })];
        assert!(state.transcript_view.page_answers.is_empty());
        assert!(state.questions.forms[&id].sent.is_none());
        let form = &project(&state)[&id];
        assert_eq!(form["state"], "answered");
        assert_eq!(form["canSend"], false);
        assert!(send(&mut state, &ChatContext::default(), &id).is_empty());
        assert!(sync(&mut state)
            .iter()
            .all(|effect| !matches!(effect, Effect::ReadStorage { .. })));
    }

    /// The pending "You answered" card puts the answers among the transcript's rows; the form still
    /// reads queued until a real row or the index records them.
    #[test]
    fn a_queued_answer_with_its_pending_card_reads_queued() {
        let (mut state, id) = state_with_form();
        let answers = json!({"Pick?": "Yes"});
        let entry = state.questions.forms.get_mut(&id).unwrap();
        entry.sent = Some(answers.clone());
        entry.sent_waiting_for_idle = true;
        state
            .transcript_view
            .page_answers
            .push(crate::transcript::page_answers::PageAnswers {
                page_id: id.clone(),
                title: String::new(),
                answers,
            });
        assert_eq!(project(&state)[&id]["state"], "queued");
        assert_eq!(project(&state)[&id]["canSend"], false);
        state.session.resource_index =
            vec![json!({"id": id, "kind": "form", "status": "answered", "answersOffset": 900})];
        assert_eq!(
            project(&state)[&id]["state"],
            "answered",
            "the transcript recorded it"
        );
    }

    /// The index counts answers waiting in the queue as answered: the form still reads queued,
    /// and its saved picks stay, until the transcript records the answers.
    #[test]
    fn an_answer_only_in_the_queue_reads_queued() {
        let (mut state, id) = state_with_form();
        state.session.resource_index =
            vec![json!({"id": id, "kind": "form", "status": "answered"})];
        let form = &state.transcript_view.question_forms[0];
        let text =
            crate::transcript::question_forms::answers_message(form, &json!({"Which?": "A"}));
        state.session.queue_prompts = Some(vec![json!({"id": "q1", "text": text})]);
        let form = &project(&state)[&id];
        assert_eq!(form["state"], "queued", "sent from another client");
        assert_eq!(form["canSend"], false);
        assert!(sync(&mut state)
            .iter()
            .all(|effect| !matches!(effect, Effect::WriteStorage { value: None, .. })));
    }

    #[test]
    fn form_answers_use_normal_send_and_keep_one_echo_until_recorded() {
        let (mut state, id) = state_with_form();
        let context = ChatContext::default();
        let effects = send(&mut state, &context, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::SendSessionChatMessage,
                params,
            }] => {
                assert!(params["text"]
                    .as_str()
                    .unwrap()
                    .contains("ghostex-page-answers"));
                assert!(params["sendRequestId"]
                    .as_str()
                    .unwrap()
                    .starts_with("question-form-"));
                *request_id
            }
            other => panic!("expected normal send, got {other:?}"),
        };
        assert!(
            send(&mut state, &context, &id).is_empty(),
            "one answer in flight"
        );
        crate::session::answer_echoes::settle(&mut state, &context, &[]);
        assert_eq!(state.pending.sends.len(), 1);
        let answer_text = state.pending.sends[0].text.clone();
        rpc_settled(
            &mut state,
            request_id,
            &RpcOutcome::Ok { result: json!({}) },
        );
        crate::session::answer_echoes::settle(&mut state, &context, &[]);
        assert_eq!(
            state.pending.sends.len(),
            1,
            "accepted send keeps its answers card"
        );
        assert_eq!(project(&state)[&id]["state"], "sending");
        assert!(
            send(&mut state, &context, &id).is_empty(),
            "accepted answer cannot send twice"
        );
        let recorded = serde_json::from_value(json!({
            "id": "recorded", "role": "user", "source": "transcript",
            "blocks": [{"type": "text", "text": answer_text}],
        }))
        .unwrap();
        state.messages.replace_list(&[recorded]);
        crate::session::settle::before_compose(&mut state, &context);
        assert!(
            state.pending.sends.is_empty(),
            "transcript replaces the card"
        );
    }

    #[test]
    fn advanced_setting_keeps_queue_delivery_across_a_retry() {
        let (mut state, id) = state_with_form();
        state.core.answers_wait_for_idle = true;
        let context = ChatContext::default();
        let effects = send(&mut state, &context, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::QueueSessionChatPrompt,
                ..
            }] => *request_id,
            other => panic!("expected queue send, got {other:?}"),
        };
        state.core.answers_wait_for_idle = false;
        assert!(
            state.questions.forms[&id]
                .send
                .as_ref()
                .unwrap()
                .queue_until_idle
        );
        rpc_settled(
            &mut state,
            request_id,
            &RpcOutcome::Ok { result: json!({}) },
        );
        assert_eq!(project(&state)[&id]["state"], "queued");
    }

    #[test]
    fn normal_send_reply_before_echo_still_draws_one_answers_card() {
        let (mut state, id) = state_with_form();
        let context = ChatContext::default();
        let effects = send(&mut state, &context, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc { request_id, .. }] => *request_id,
            other => panic!("expected one send, got {other:?}"),
        };
        rpc_settled(
            &mut state,
            request_id,
            &RpcOutcome::Ok { result: json!({}) },
        );
        crate::session::answer_echoes::settle(&mut state, &context, &[]);
        crate::session::answer_echoes::settle(&mut state, &context, &[]);
        assert_eq!(state.pending.sends.len(), 1);
        assert_eq!(state.pending.sends[0].id, format!("answers:{id}"));
    }

    /// A Ghostex Collab guest's answers the owner holds: no echo, so the chat is not working, and
    /// the form waits for the owner; then it follows what the owner did.
    #[test]
    fn a_guests_held_answers_wait_for_the_owner() {
        for (outcome, phase) in [
            ("sent", "sending"),
            ("declined", "open"),
            ("edited", "open"),
        ] {
            let (mut state, id) = state_with_form();
            state.session.collab_guest = Some(json!({"viewerId": "v", "name": "Sam"}));
            state.session.collab =
                Some(json!({"shared": true, "owner": "Mohamad", "mode": "approve", "held": []}));
            let context = ChatContext::default();
            let effects = send(&mut state, &context, &id);
            let request_id = match &effects[..] {
                [Effect::SendRpc { request_id, .. }] => *request_id,
                other => panic!("expected one send, got {other:?}"),
            };
            crate::session::answer_echoes::settle(&mut state, &context, &[]);
            rpc_settled(
                &mut state,
                request_id,
                &RpcOutcome::Ok {
                    result: json!({"collab": {"held": true, "heldId": "held_1"}}),
                },
            );
            crate::session::answer_echoes::settle(&mut state, &context, &[]);
            assert!(state.pending.sends.is_empty(), "held answers draw no echo");
            assert!(!crate::session::working::is_working(&state));
            let form = &project(&state)[&id];
            assert_eq!(form["state"], "held");
            assert_eq!(form["heldFor"], "Mohamad");
            assert_eq!(form["canSend"], false);

            state.session.collab = Some(json!({
                "shared": true, "owner": "Mohamad", "mode": "approve", "held": [],
                "resolved": [{"id": "held_1", "outcome": outcome}],
            }));
            crate::extras::collab::settle_held_answers(&mut state);
            let form = &project(&state)[&id];
            assert_eq!(form["state"], phase, "after the owner {outcome} it");
            if phase == "open" {
                assert!(form["sendError"].as_str().unwrap().contains("Mohamad"));
                assert_eq!(form["canSend"], true);
            }
        }
    }

    /// A reloaded guest page reads its held answers from the chat's held list.
    #[test]
    fn a_reloaded_guest_page_still_waits_for_the_owner() {
        let (mut state, id) = state_with_form();
        let form = state.transcript_view.question_forms[0].clone();
        let text =
            crate::transcript::question_forms::answers_message(&form, &json!({"Pick?": "Yes"}));
        state.session.collab_guest = Some(json!({"viewerId": "v2", "name": "Sam"}));
        state.session.collab = Some(json!({
            "shared": true, "owner": "Mohamad", "mode": "approve",
            "held": [{"id": "held_1", "viewerId": "v1", "name": "Sam", "text": text}],
        }));
        let projected = &project(&state)[&id];
        assert_eq!(projected["state"], "held");
        assert_eq!(projected["canSend"], false);
        state.session.collab = Some(json!({
            "shared": true, "owner": "Mohamad", "mode": "approve", "held": [],
        }));
        assert_eq!(
            project(&state)[&id]["state"],
            "open",
            "declined: the form opens again"
        );
    }

    #[test]
    fn normal_send_that_falls_back_to_queue_reads_queued() {
        let (mut state, id) = state_with_form();
        let context = ChatContext::default();
        let effects = send(&mut state, &context, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc { request_id, .. }] => *request_id,
            other => panic!("expected one send, got {other:?}"),
        };
        let answer_text = state.questions.forms[&id].send.as_ref().unwrap().params["text"].clone();
        rpc_settled(
            &mut state,
            request_id,
            &RpcOutcome::Ok {
                result: json!({"queuedPromptId": "q"}),
            },
        );
        state.session.queue_prompts = Some(vec![
            json!({"id": "q", "text": answer_text, "state": "queued"}),
        ]);
        assert_eq!(project(&state)[&id]["state"], "queued");
    }
}
