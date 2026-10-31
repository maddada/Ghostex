//! The reader's side of a question form (a ```questions block, `transcript/question_forms.rs`):
//! picking choices, the note or text answer, sending the answers to the agent, and the saved picks.
//!
//! A choice question's picks follow the page kit (`server/src/visual_pages/kit.rs`): nothing starts
//! picked, clicking a pick again clears it, one of N replaces the pick while many of N toggles, and
//! "Other / none of these" (the row after the last choice) is a pick of its own whose note says
//! what instead. A question left blank when the form is sent goes to the agent as `"skipped"`, and
//! Skip all (`skip_all`) sends at once. The answers go into the session's chat queue as the
//! `<ghostex-page-answers>` message, which gxserver types once the agent is idle; the form reads Queued until then and
//! Answered once the transcript carries the message.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::document::QuestionDraft;
use crate::effect::Effect;
use crate::event::StorageKey;
use crate::questions::drafts::{decode_drafts, encode_drafts, form_drafts_key, AnswerDrafts};
use crate::state::{ChatContext, ChatState, FormSend, FormState};
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

fn form<'a>(state: &'a ChatState, id: &str) -> Option<&'a QuestionForm> {
    all_forms(state).find(|form| form.id == id && form.error.is_empty())
}

/// The answers the transcript already carries for this form.
fn received_answers<'a>(state: &'a ChatState, id: &str) -> Option<&'a Value> {
    state
        .transcript_view
        .page_answers
        .iter()
        .find(|answers| answers.page_id == id)
        .or_else(|| crate::questions::resource_modal::fetched_answers(state, id))
        .map(|answers| &answers.answers)
}

/// Whether the reader can still change the form. It never waits for the saved picks: the rows
/// that find a new form are rebuilt after this family's sync, so their read can leave an event
/// late, and picks the reader makes first win over it.
fn open(state: &ChatState, id: &str) -> bool {
    received_answers(state, id).is_none()
        && !crate::questions::later::removed_ids(state).contains(id)
        && !crate::questions::resource_modal::is_withdrawn(state, id)
        && state
            .questions
            .forms
            .get(id)
            .is_some_and(|entry| entry.send.is_none() && entry.sent.is_none())
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
    // See the SessionChat 2026-10-11 DECISION in server/src/visual_pages/answers.rs: answers go
    // into the session's chat queue, which types them once the agent is idle, like a page's.
    let request_id = state.core.allocate_request_id();
    let params = json!({
        "text": answers_message(&form, &answers),
        "sendRequestId": format!("question-form-{id}-{}", context.now_millis()),
    });
    let effect = Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::QueueSessionChatPrompt,
        params: Box::new(params.clone()),
    };
    if let Some(entry) = state.questions.forms.get_mut(id) {
        entry.error = None;
        entry.send = Some(FormSend {
            request_id,
            answers,
            params,
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
        // Queued: the saved picks stay until the transcript carries the answers (`sync`), so a
        // queued message the user removes again leaves the form as it was after a reload.
        RpcOutcome::Ok { result } => {
            if let Some(entry) = state.questions.forms.get_mut(&id) {
                entry.sent = Some(sent.answers);
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
    for (id, entry) in state.questions.forms.iter_mut() {
        if !entry.cleared && answered.contains(id) {
            entry.cleared = true;
            effects.push(Effect::WriteStorage {
                key: form_drafts_key(&session_key, id),
                value: None,
                durable: false,
            });
        }
    }
    let open_forms: Vec<(String, usize)> = all_forms(state)
        .filter(|form| form.error.is_empty() && received_answers(state, &form.id).is_none())
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
    let later = crate::questions::open_questions::later_waiting(state);
    let removed = crate::questions::later::removed_ids(state);
    all_forms(state)
        .map(|form| {
            let entry = state.questions.forms.get(&form.id);
            let fallback = FormState::default();
            let entry = entry.unwrap_or(&fallback);
            let sent = received_answers(state, &form.id).or(entry.sent.as_ref());
            let (phase, drafts, notes) = match (sent, entry.send.as_ref()) {
                (Some(answers), _) if received_answers(state, &form.id).is_some() => (
                    "answered",
                    drafts_from_answers(form, answers),
                    notes_from_answers(form, answers),
                ),
                (Some(answers), _) => (
                    "queued",
                    drafts_from_answers(form, answers),
                    notes_from_answers(form, answers),
                ),
                (None, Some(send)) => (
                    "sending",
                    drafts_from_answers(form, &send.answers),
                    notes_from_answers(form, &send.answers),
                ),
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
            } else if phase == "open" && crate::questions::resource_modal::is_withdrawn(state, &form.id) {
                // Only the modal's form reads Withdrawn: it opens to be read and takes no answer.
                "withdrawn"
            } else {
                phase
            };
            let takes_notes = takes_notes(form);
            let mut value = serde_json::to_value(form).unwrap_or_default();
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
                object.insert("takesNotes".to_string(), json!(takes_notes));
                object.insert("notes".to_string(), json!(notes));
                object.insert(
                    "sendError".to_string(),
                    json!(entry.error.clone().unwrap_or_default()),
                );
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
    if received_answers(state, id).is_none() {
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
            method: ChatRpcMethod::QueueSessionChatPrompt,
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
}
