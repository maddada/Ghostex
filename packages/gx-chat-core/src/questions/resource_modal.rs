//! A form or chart of the Chat Resources list opened in a floating modal, without scrolling the
//! chat; its "Go to chat" button, which does scroll, loading earlier messages first when the item
//! is older than the loaded rows.
//!
//! CDXC:SessionChat 2026-10-31 DECISION:
//! User, of a question form older than the loaded rows: "Please show it floating so I can answer it without scrolling all the way up to it, or I can see the answer that I did without scrolling all the way up to it", "Even in the regular case, when I click on it from the resources, just don't scroll me up. Just show it floating in a modal ... Make sure the modal is not too tall or wide. The size of it should make sense. Also, it needs to have a button to go to the actual chat." So a click on a form or chart in the list opens it in a modal and the chat stays where it is. The modal draws the form's own card (`questionForms[id]`), so the picks, Other, notes, Send, For later and Remove are the card's, over the card's own answer path; an answered or Canceled form reads read-only; a withdrawn one says so. A form older than the loaded rows has no row to draw, so its reply (and its answers message) is read back on demand by byte offset (`readSessionChatResource`, server/src/session_chat_resources.rs): two seeks, no frame grows, and no transcript is read. When that read cannot find it, the modal says why. Pages keep opening in their page window.
//! "Go to chat" scrolls to the form; when it is older than the loaded rows, it reads the window of history that ends at that reply (a jump read: `beforeOffset` just past the reply's offset, which leaves the chat's own "Load earlier" cursor where it was) and scrolls once the reply is among the rows, instead of paging in every message between, which a 626 MB transcript cannot afford. The Previous question pill, with only older questions waiting, goes the same way.

use ghostex_gx_protocol::ChatMessage;
use serde_json::{json, Value};

use crate::effect::Effect;
use crate::state::{ChatContext, ChatState};
use crate::transcript::page_answers::{page_answers_in, PageAnswers};
use crate::transcript::question_forms::{question_forms, QuestionForm};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// How far past a reply a jump read's window reaches (bytes) when the index does not know where the
/// reply's turn ends.
pub const JUMP_CONTEXT_BYTES: u64 = 262_144;

/// Where a jump read for the entry should end: a history page ends at a turn boundary before the
/// offset it is given and leaves out the turn the offset falls in, so it is the start of the next
/// user message (the end of the reply's turn), else a little past the reply.
fn jump_end(entry: &crate::questions::resource_index::ResourceEntry) -> Option<u64> {
    let offset = entry.offset?;
    Some(entry.turn_end.unwrap_or(offset + JUMP_CONTEXT_BYTES))
}

/// What the modal shows of its item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceModal {
    pub id: String,
    /// `form` or `chart`.
    pub kind: String,
    /// The item is being read back from the transcript.
    pub loading: bool,
    /// Why it could not be read, when it could not.
    pub missing: Option<String>,
    /// The form, read back for an item older than the loaded rows (a loaded form is the
    /// transcript's own).
    pub form: Option<QuestionForm>,
    /// The answers that came with it.
    pub answers: Option<PageAnswers>,
    /// A chart's source (the fence's body).
    pub source: Option<String>,
    /// The read in flight.
    pub request_id: Option<u64>,
}

/// A "Go to chat" or a Previous question jump waiting for its item to be among the loaded rows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResourceGo {
    pub id: String,
    /// Where the page of history that holds the item's reply should end (bytes into the file).
    pub offset: Option<u64>,
    /// The jump read for it went out.
    pub read_issued: bool,
}

fn loaded_form(state: &ChatState, id: &str) -> bool {
    state
        .transcript_view
        .question_forms
        .iter()
        .any(|form| form.id == id && form.error.is_empty())
}

/// Whether the chat holds the item's reply among its loaded rows.
fn loaded(state: &ChatState, id: &str) -> bool {
    holder_message(state, id).is_some()
}

/// The loaded reply that holds the item. The transcript's askers and content are refreshed after
/// an event settles, so a window that was just read in is found by reading the loaded messages
/// themselves.
fn holder_message(state: &ChatState, id: &str) -> Option<String> {
    if let Some(asker) = state.transcript_view.askers.iter().find(|asker| asker.id == id) {
        return Some(asker.message_id.clone());
    }
    if let Some(item) = state.questions.content.items.iter().find(|item| item.id == id) {
        return Some(item.message_id.clone());
    }
    state
        .messages
        .list
        .iter()
        .find(|message| {
            crate::questions::resource_index::scan_message(message)
                .iter()
                .any(|held| held.id == id)
        })
        .map(|message| message.id.clone())
}

/// Scrolls the chat to the item's loaded reply, and closes the modal.
fn scroll_to(state: &mut ChatState, context: &ChatContext, id: &str, message_id: &str) -> Vec<Effect> {
    close(state);
    state.questions.resource_go = None;
    crate::questions::open_questions::jump_to_row(state, context, message_id, id, None)
}

fn entry_of(state: &ChatState, id: &str) -> Option<crate::questions::resource_index::ResourceEntry> {
    crate::questions::open_questions::indexed(state)
        .into_iter()
        .find(|entry| entry.id == id)
}

/// A chart's source, from a loaded reply.
fn loaded_chart_source(state: &ChatState, id: &str) -> Option<String> {
    let item = state.questions.content.items.iter().find(|item| item.id == id)?;
    let message = state
        .messages
        .composed
        .iter()
        .find(|message| message.id == item.message_id)?;
    chart_source(message, id)
}

fn chart_source(message: &ChatMessage, id: &str) -> Option<String> {
    let markdown = crate::transcript::prose::prose_markdown(&message.blocks);
    crate::transcript::question_forms::content_fences(&markdown)
        .into_iter()
        .filter(|fence| fence.kind == crate::transcript::question_forms::ContentKind::Chart)
        .find(|fence| crate::questions::content::chart_id(&fence.body) == id)
        .map(|fence| fence.body)
}

/// `openResource {id}`: opens a form or chart in the modal, without moving the chat. A form or
/// chart the chat has loaded needs no read; an older one is read back by its offset. Anything else
/// (a page) is not the modal's: the caller opens it as before.
pub fn open(state: &mut ChatState, id: &str) -> Vec<Effect> {
    let known_kind = if loaded_form(state, id) {
        Some("form".to_string())
    } else if state.questions.content.items.iter().any(|item| {
        item.id == id && item.kind == crate::transcript::question_forms::ContentKind::Chart
    }) {
        Some("chart".to_string())
    } else {
        entry_of(state, id)
            .filter(|entry| entry.kind != "page")
            .map(|entry| entry.kind)
    };
    let Some(kind) = known_kind else {
        return Vec::new();
    };
    let mut modal = ResourceModal {
        id: id.to_string(),
        kind: kind.clone(),
        ..ResourceModal::default()
    };
    let mut effects = Vec::new();
    if kind == "chart" {
        modal.source = loaded_chart_source(state, id);
    }
    let needs_read = (kind == "form" && !loaded_form(state, id)) || (kind == "chart" && modal.source.is_none());
    if needs_read {
        let request_id = state.core.allocate_request_id();
        modal.loading = true;
        modal.request_id = Some(request_id);
        effects.push(Effect::SendRpc {
            request_id,
            method: ChatRpcMethod::ReadSessionChatResource,
            params: Box::new(json!({
                "projectId": state.identity.project_id,
                "sessionId": state.identity.session_id,
                "id": id,
            })),
        });
    }
    state.questions.resource_modal = Some(modal);
    state.core.request_publish();
    effects
}

/// `closeResource`: the modal goes away; the form's picks stay where the card keeps them, so a
/// half-filled answer is not lost.
pub fn close(state: &mut ChatState) {
    if state.questions.resource_modal.take().is_some() {
        state.core.request_publish();
    }
}

/// The modal's read settled: the item's reply (and its answers message), or why not.
pub fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
) -> Option<Vec<Effect>> {
    let modal = state.questions.resource_modal.as_mut()?;
    if modal.request_id != Some(request_id) {
        return None;
    }
    modal.request_id = None;
    modal.loading = false;
    match outcome {
        RpcOutcome::Ok { result } => {
            let messages: Vec<ChatMessage> = result
                .get("messages")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or_default();
            let id = modal.id.clone();
            if modal.kind == "chart" {
                modal.source = messages.iter().find_map(|message| chart_source(message, &id));
                modal.missing = modal
                    .source
                    .is_none()
                    .then(|| "Ghostex could not find that chart in the transcript.".to_string());
            } else {
                modal.form = question_forms(&messages)
                    .into_iter()
                    .find(|form| form.id == id && form.error.is_empty());
                modal.answers = page_answers_in(&messages)
                    .into_iter()
                    .find(|answers| answers.page_id == id);
                modal.missing = modal
                    .form
                    .is_none()
                    .then(|| "Ghostex could not find those questions in the transcript.".to_string());
            }
        }
        RpcOutcome::Err { message, .. } => {
            modal.missing = Some(if message.trim().is_empty() {
                "Ghostex could not read that from the transcript.".to_string()
            } else {
                message.clone()
            });
        }
    }
    state.core.request_publish();
    Some(Vec::new())
}

/// Whether the modal is open on this form.
pub fn is_open_for(state: &ChatState, id: &str) -> bool {
    state
        .questions
        .resource_modal
        .as_ref()
        .is_some_and(|modal| modal.id == id)
}

/// Whether the modal's item is a form the agent withdrew before it was answered.
pub fn is_withdrawn(state: &ChatState, id: &str) -> bool {
    if state
        .questions
        .resource_modal
        .as_ref()
        .is_none_or(|modal| modal.id != id)
    {
        return false;
    }
    if let Some(asker) = state.transcript_view.askers.iter().find(|asker| asker.id == id) {
        return asker.obsolete && !asker.answered;
    }
    entry_of(state, id).is_some_and(|entry| entry.status == "withdrawn")
}

/// The form the modal holds that the transcript does not (an item older than the loaded rows).
pub fn fetched_form(state: &ChatState) -> Option<&QuestionForm> {
    state
        .questions
        .resource_modal
        .as_ref()
        .and_then(|modal| modal.form.as_ref())
        .filter(|form| !loaded_form(state, &form.id))
}

/// The answers the modal holds that the transcript does not.
pub fn fetched_answers<'a>(state: &'a ChatState, id: &str) -> Option<&'a PageAnswers> {
    state
        .questions
        .resource_modal
        .as_ref()
        .filter(|modal| modal.id == id)
        .and_then(|modal| modal.answers.as_ref())
}

/// `goToResource {id}`: scrolls the chat to the form or chart. One the chat has loaded is scrolled
/// to at once and the modal closes; an older one is read in by a jump read first (the modal's
/// button shows it loading), then scrolled to when it is among the rows.
pub fn go_to(state: &mut ChatState, context: &ChatContext, id: &str) -> Vec<Effect> {
    if let Some(message_id) = holder_message(state, id) {
        return scroll_to(state, context, id, &message_id);
    }
    let offset = entry_of(state, id).and_then(|entry| jump_end(&entry));
    state.questions.resource_go = Some(ResourceGo {
        id: id.to_string(),
        offset,
        read_issued: false,
    });
    state.core.request_publish();
    advance(state, context)
}

/// Brings a waiting "Go to chat" along: issues the jump read, and scrolls once the item is among
/// the loaded rows. Runs on the click and after every event.
pub fn advance(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let Some(go) = state.questions.resource_go.clone() else {
        return Vec::new();
    };
    if go.read_issued {
        // The jump read is out or settled: `after_refresh` scrolls once the rows are rebuilt from
        // it (scrolling now would run against rows that do not hold the new messages yet).
        return Vec::new();
    }
    if let Some(message_id) = holder_message(state, &go.id) {
        return scroll_to(state, context, &go.id, &message_id);
    }
    if state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return Vec::new();
    }
    let Some(offset) = go.offset else {
        state.questions.resource_go = None;
        state.core.request_publish();
        return vec![Effect::Toast {
            level: "info".to_string(),
            message: "Ghostex has not recorded where that sits in the chat yet. Try again in a moment.".to_string(),
        }];
    };
    if let Some(waiting) = state.questions.resource_go.as_mut() {
        waiting.read_issued = true;
    }
    state.core.request_publish();
    crate::session::actions::load_window_ending_at(state, context, offset)
}

/// Runs after the core rebuilt the rows (`Core::republish`): the window a jump read brought in is
/// in them now, so the chat scrolls to the item; when it is still not there, the modal says so
/// instead of waiting for ever.
pub fn after_refresh(state: &mut ChatState, context: &ChatContext) {
    let Some(go) = state.questions.resource_go.clone() else {
        return;
    };
    if !go.read_issued || state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return;
    }
    if let Some(message_id) = holder_message(state, &go.id) {
        let _ = scroll_to(state, context, &go.id, &message_id);
        return;
    }
    state.questions.resource_go = None;
    if let Some(modal) = state.questions.resource_modal.as_mut().filter(|modal| modal.id == go.id) {
        modal.missing = Some("Ghostex could not bring that part of the chat in.".to_string());
    }
    state.core.request_publish();
}

/// The modal's view for the renderers (`resourceModal` on the document), or `Null` while it is
/// closed.
pub fn project(state: &ChatState, context: &ChatContext) -> Value {
    let Some(modal) = state.questions.resource_modal.as_ref() else {
        return Value::Null;
    };
    let rows = crate::questions::open_questions::list(state, context);
    let row = rows
        .iter()
        .flat_map(|section| &section.rows)
        .find(|row| row.id == modal.id);
    let status = row.map(|row| row.status.clone()).unwrap_or_default();
    let title = row
        .map(|row| row.title.clone())
        .filter(|title| !title.is_empty())
        .or_else(|| modal.form.as_ref().map(|form| form.title.clone()))
        .unwrap_or_default();
    json!({
        "id": modal.id,
        "kind": modal.kind,
        "title": title,
        "status": status,
        "loading": modal.loading,
        "missing": modal.missing.clone().unwrap_or_default(),
        "source": modal.source.clone().unwrap_or_default(),
        "going": state.questions.resource_go.as_ref().is_some_and(|go| go.id == modal.id),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index_entry(id: &str, kind: &str, offset: Option<u64>) -> Value {
        json!({"id": id, "kind": kind, "title": format!("About {id}"), "status": "waiting",
            "atMs": 5, "offset": offset})
    }

    fn reply(id: &str, text: &str) -> ChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": "assistant", "source": "transcript", "timestamp": 1,
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    const FORM: &str = "```questions\n{\"title\":\"Pick\",\"questions\":[{\"tag\":\"Q1\",\"title\":\"Which?\",\"choices\":[\"A\",\"B\"]}]}\n```";

    fn form_id() -> String {
        crate::transcript::question_forms::question_forms(&[reply("m", FORM)])[0].id.clone()
    }

    #[test]
    fn a_form_older_than_the_loaded_rows_is_read_back_and_shown() {
        let id = form_id();
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", Some(100))];
        let effects = open(&mut state, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc { request_id, method: ChatRpcMethod::ReadSessionChatResource, params }] => {
                assert_eq!(params["id"], id.as_str());
                *request_id
            }
            other => panic!("{other:?}"),
        };
        let modal = state.questions.resource_modal.as_ref().unwrap();
        assert!(modal.loading && modal.form.is_none());
        // The reply arrives (the server sends the reply as the chat wire carries it).
        let result = json!({"id": id, "messages": [serde_json::to_value(reply("m", FORM)).unwrap()]});
        assert!(rpc_settled(&mut state, request_id, &RpcOutcome::Ok { result }).is_some());
        let modal = state.questions.resource_modal.as_ref().unwrap();
        assert!(!modal.loading && modal.missing.is_none());
        assert_eq!(modal.form.as_ref().map(|form| form.id.as_str()), Some(id.as_str()));
        // The modal's form is what the forms code answers from.
        assert!(fetched_form(&state).is_some());
        let projected = crate::questions::forms::project(&state);
        assert_eq!(projected[&id]["state"], "open");
    }

    #[test]
    fn a_form_that_cannot_be_read_says_why() {
        let id = form_id();
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", None)];
        let request_id = match &open(&mut state, &id)[..] {
            [Effect::SendRpc { request_id, .. }] => *request_id,
            other => panic!("{other:?}"),
        };
        let outcome = RpcOutcome::Err {
            code: Some("notFound".to_string()),
            message: "Ghostex has not recorded where that sits yet.".to_string(),
            endpoint: None,
        };
        rpc_settled(&mut state, request_id, &outcome);
        let modal = state.questions.resource_modal.as_ref().unwrap();
        assert_eq!(modal.missing.as_deref(), Some("Ghostex has not recorded where that sits yet."));
        let view = project(&state, &ChatContext::default());
        assert_eq!(view["missing"], "Ghostex has not recorded where that sits yet.");
        assert_eq!(view["loading"], false);
    }

    #[test]
    fn a_loaded_form_opens_at_once_and_the_chat_does_not_move() {
        let id = form_id();
        let mut state = ChatState::default();
        state
            .transcript_view
            .question_forms
            .push(crate::transcript::question_forms::question_forms(&[reply("m", FORM)])[0].clone());
        assert!(open(&mut state, &id).is_empty(), "no read for a loaded form");
        let modal = state.questions.resource_modal.as_ref().unwrap();
        assert!(!modal.loading);
        assert!(state.questions.open_questions.jump.is_none(), "nothing scrolled");
        close(&mut state);
        assert!(state.questions.resource_modal.is_none());
    }

    #[test]
    fn go_to_chat_reads_the_window_ending_at_an_old_form_then_scrolls() {
        let id = form_id();
        let context = ChatContext::default();
        let mut state = ChatState::default();
        let mut entry = index_entry(&id, "form", Some(5_000));
        entry["turnEnd"] = json!(5_900);
        state.session.resource_index = vec![entry];
        state.messages.has_more = true;
        state.messages.before_offset = 90_000;
        let effects = go_to(&mut state, &context, &id);
        match &effects[..] {
            [Effect::SendRpc { method: ChatRpcMethod::ReadSessionChat, params, .. }] => {
                assert_eq!(params["beforeOffset"], 5_900, "where the reply's turn ends");
            }
            other => panic!("{other:?}"),
        }
        assert!(state.messages.loading_earlier, "the chat shows it loading");
        assert_eq!(state.messages.before_offset, 90_000, "the Load earlier cursor stays where it was");
        // Waiting on the read: nothing else is issued.
        assert!(advance(&mut state, &context).is_empty());
        assert!(state.questions.resource_go.is_some());
    }

    #[test]
    fn go_to_chat_without_a_recorded_position_says_so() {
        let id = form_id();
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", None)];
        let effects = go_to(&mut state, &ChatContext::default(), &id);
        assert!(matches!(&effects[..], [Effect::Toast { .. }]));
        assert!(state.questions.resource_go.is_none());
    }

    fn fetched_state(answers: Option<&str>) -> (ChatState, String) {
        let id = form_id();
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", Some(100))];
        let request_id = match &open(&mut state, &id)[..] {
            [Effect::SendRpc { request_id, .. }] => *request_id,
            other => panic!("{other:?}"),
        };
        let mut messages = vec![serde_json::to_value(reply("m", FORM)).unwrap()];
        if let Some(text) = answers {
            messages.push(
                serde_json::to_value(
                    serde_json::from_value::<ChatMessage>(json!({
                        "id": "a", "role": "user", "source": "transcript", "timestamp": 2,
                        "blocks": [{"type": "text", "text": text}],
                    }))
                    .unwrap(),
                )
                .unwrap(),
            );
        }
        let result = json!({"id": id, "messages": messages});
        rpc_settled(&mut state, request_id, &RpcOutcome::Ok { result });
        // The seam runs `sync` after every event: it gives the modal's form its blank picks.
        crate::questions::forms::sync(&mut state);
        (state, id)
    }

    /// Answering in the modal is the card's own path: the same pick and send, the same queue call,
    /// and the form reads Sending, then Queued, as its card does.
    #[test]
    fn a_fetched_form_is_answered_in_the_modal_by_the_cards_own_path() {
        let (mut state, id) = fetched_state(None);
        let context = ChatContext::default();
        assert_eq!(crate::questions::forms::project(&state)[&id]["canSend"], true);
        let _ = crate::questions::forms::pick(&mut state, &context, &id, 0, 1);
        let effects = crate::questions::forms::send(&mut state, &context, &id);
        let request_id = match &effects[..] {
            [Effect::SendRpc { request_id, method: ChatRpcMethod::QueueSessionChatPrompt, params }] => {
                let text = params["text"].as_str().unwrap();
                assert!(text.contains("ghostex-page-answers") && text.contains(&id), "{text}");
                assert!(text.contains("\"B\""), "the pick went with it: {text}");
                *request_id
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(crate::questions::forms::project(&state)[&id]["state"], "sending");
        crate::questions::forms::rpc_settled(&mut state, request_id, &RpcOutcome::Ok { result: json!({}) });
        let view = &crate::questions::forms::project(&state)[&id];
        assert_eq!(view["state"], "queued");
        assert_eq!(view["canSend"], false);
        // A second answer is refused: it already went.
        assert!(crate::questions::forms::send(&mut state, &context, &id).is_empty());
    }

    /// An answered form reads its answers, read-only, from the message that came with it.
    #[test]
    fn an_answered_old_form_shows_its_answers_and_takes_no_more() {
        let id = form_id();
        let answers = format!(
            "<ghostex-page-answers page=\"{id}\" title=\"Pick\">{{\"Which?\":\"A\"}}</ghostex-page-answers>"
        );
        let (mut state, id) = fetched_state(Some(&answers));
        let view = crate::questions::forms::project(&state)[&id].clone();
        assert_eq!(view["state"], "answered");
        assert_eq!(view["canSend"], false);
        assert_eq!(view["collapsed"], false, "the modal shows the answers open");
        assert_eq!(view["drafts"][0]["indices"], json!([0]), "the answer it carries is picked");
        let context = ChatContext::default();
        assert!(crate::questions::forms::pick(&mut state, &context, &id, 0, 1).is_empty());
        assert!(crate::questions::forms::send(&mut state, &context, &id).is_empty());
    }

    /// A withdrawn form opens to be read and says so; a Canceled one reads Canceled.
    #[test]
    fn a_withdrawn_form_and_a_canceled_one_open_read_only() {
        let (mut state, id) = fetched_state(None);
        state.session.resource_index[0]["status"] = json!("withdrawn");
        let view = crate::questions::forms::project(&state)[&id].clone();
        assert_eq!((view["state"].as_str(), view["canSend"].as_bool()), (Some("withdrawn"), Some(false)));
        let modal = project(&state, &ChatContext::default());
        assert_eq!(modal["status"], "withdrawn");
        assert!(crate::questions::forms::send(&mut state, &ChatContext::default(), &id).is_empty());
        state.session.removed_question_ids = vec![id.clone()];
        assert_eq!(crate::questions::forms::project(&state)[&id]["state"], "canceled");
    }

    /// The jump read merges the window that ends at the old form without moving the Load earlier
    /// cursor, and the chat scrolls to the form once its reply is among the rows.
    #[test]
    fn the_go_to_chat_read_brings_the_old_form_in_and_scrolls_to_it() {
        let id = form_id();
        let context = ChatContext::default();
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", Some(5_000))];
        state.messages.has_more = true;
        state.messages.before_offset = 90_000;
        let request_id = match &go_to(&mut state, &context, &id)[..] {
            [Effect::SendRpc { request_id, .. }] => *request_id,
            other => panic!("{other:?}"),
        };
        let old = serde_json::to_value(reply("old-form", FORM)).unwrap();
        let result = json!({
            "messages": [old], "hasMore": true, "beforeOffset": 4_000, "status": "ready",
        });
        let effects = crate::session::events::handle(
            &mut state,
            &crate::event::Event::RpcSettled { request_id, outcome: Box::new(RpcOutcome::Ok { result }) },
            &context,
        );
        let _ = effects;
        assert!(state.messages.list.iter().any(|message| message.id == "old-form"), "the reply is a loaded row");
        assert_eq!(state.messages.before_offset, 90_000, "the cursor did not move");
        assert!(!state.messages.loading_earlier);
        // The seam composes and refreshes the transcript, then advances the jump.
        crate::session::composition::compose_cached(&mut state, &[], None, false);
        crate::transcript::refresh(&mut state, &context);
        assert!(loaded(&state, &id), "the form is found among the loaded rows");
        after_refresh(&mut state, &context);
        assert!(state.questions.resource_go.is_none(), "the jump is done");
        assert!(state.questions.open_questions.jump.is_some(), "the chat was told to scroll");
    }

    /// Answering an old form from the modal moves it to Answered in the list and the count at once,
    /// and the Previous question pill, with only older questions waiting, goes to chat for them.
    #[test]
    fn an_answer_from_the_modal_updates_the_list_and_the_pill_goes_to_the_old_question() {
        let (mut state, id) = fetched_state(None);
        let context = ChatContext::default();
        let before = crate::questions::open_questions::project(&state, &context);
        assert_eq!((before.count, before.summary.as_str()), (1, "1 unanswered"));
        crate::questions::forms::send(&mut state, &context, &id);
        let after = crate::questions::open_questions::project(&state, &context);
        assert_eq!(after.count, 0, "it no longer waits");
        let row = after.sections.iter().flat_map(|s| &s.rows).find(|row| row.id == id).unwrap();
        assert_eq!(row.status, "answered");

        // A fresh chat with only an old waiting question: the pill goes to it.
        let mut state = ChatState::default();
        state.session.resource_index = vec![index_entry(&id, "form", Some(7_000))];
        match &crate::questions::open_questions::previous(&mut state, &context)[..] {
            [Effect::SendRpc { method: ChatRpcMethod::ReadSessionChat, params, .. }] => {
                assert_eq!(params["beforeOffset"], 7_000 + JUMP_CONTEXT_BYTES);
            }
            other => panic!("{other:?}"),
        }
    }
}
