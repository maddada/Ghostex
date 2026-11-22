//! Ghostex's approval card: an agent's connector call that would change something waits here
//! until the user answers the card in that session's chat (desktop or phone), the agent cancels
//! the call, or 30 minutes pass.
//!
//! CDXC:Connectors 2026-10-16 WHY:
//! The card is the session's stored chat prompt (a question card whose `toolUseId` is `connectorcard:<id>`), so the desktop and phone chats draw it with no new card type, and answering it never types into the terminal: the agent waits inside an MCP tool call, not on a screen prompt. Cards live only in this gxserver, since a restart ends the waiting call anyway; one left on screen by a restart goes away when it is answered.
//! SEE-ALSO: server/src/session_chat_send/answer_http.rs (the early `connectorcard:` branch), server/src/open_dots/approvals.rs on origin/feat/opendots (the card this one is ported from).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use rusqlite::{Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};
use tokio::sync::oneshot;

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt, SessionChatQuestion,
    SessionChatQuestionOption,
};
use crate::storage::open_gxserver_database;

pub(crate) const CARD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// How often a waiting card checks it is still on screen (a hook may have cleared it).
const RESHOW_EVERY: Duration = Duration::from_secs(10);
const TOOL_USE_PREFIX: &str = "connectorcard:";
const CARD_TOOL_NAME: &str = "ghostex_connector_approval";
const HEADER: &str = "Needs your approval";
const APPROVE_LABEL: &str = "Approve";
const ALWAYS_LABEL: &str = "Always allow";
const DENY_LABEL: &str = "Deny";

/// How a card ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    Approve,
    /// Approve, and allow this tool in this workspace from now on.
    Always,
    Deny,
    Expired,
    /// The agent stopped the call (the user pressed Stop or Escape).
    Cancelled,
    /// gxserver is shutting down.
    Restarting,
}

struct Card {
    id: String,
    project_id: String,
    session_id: String,
    question: String,
    call_id: Option<String>,
    sender: Option<oneshot::Sender<Answer>>,
}

/// Every waiting card, oldest first; a session shows its oldest.
static CARDS: Mutex<Vec<Card>> = Mutex::new(Vec::new());

/// How the last cards ended, newest last, so a coordinator's relayed copy of a card can say
/// whether it was answered in its thread, expired or cancelled (coordinators/relay.rs).
static ENDED: Mutex<Vec<(String, Answer)>> = Mutex::new(Vec::new());
const ENDED_KEPT: usize = 64;

/// How the card `tool_use_id` (`connectorcard:<id>`) ended, while it is still remembered.
pub(crate) fn connector_card_outcome(tool_use_id: &str) -> Option<Answer> {
    let card_id = tool_use_id.strip_prefix(TOOL_USE_PREFIX)?;
    ENDED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .rev()
        .find(|(id, _)| id == card_id)
        .map(|(_, answer)| *answer)
}

/// Whether a session's stored chat prompt is one of these cards.
pub(crate) fn is_connector_card(prompt: &SessionChatInteractivePrompt) -> bool {
    card_id_of(prompt).is_some()
}

/// The card's buttons in order, which a relayed answer names by position.
pub(crate) const CARD_LABELS: [&str; 3] = [APPROVE_LABEL, ALWAYS_LABEL, DENY_LABEL];

fn with_cards<T>(f: impl FnOnce(&mut Vec<Card>) -> T) -> T {
    f(&mut CARDS.lock().unwrap_or_else(PoisonError::into_inner))
}

fn is_waiting(card_id: &str) -> bool {
    with_cards(|cards| cards.iter().any(|card| card.id == card_id))
}

fn chat_prompt(card_id: &str, question: &str) -> SessionChatInteractivePrompt {
    SessionChatInteractivePrompt::Question {
        questions: vec![SessionChatQuestion {
            question: question.to_string(),
            header: Some(HEADER.to_string()),
            multi_select: false,
            allow_custom: Some(false),
            tool_name: Some(CARD_TOOL_NAME.to_string()),
            recommended: None,
            preview_layout: false,
            options: CARD_LABELS
                .into_iter()
                .map(|label| SessionChatQuestionOption {
                    label: label.to_string(),
                    description: None,
                    preview: None,
                })
                .collect(),
        }],
        tool_use_id: Some(format!("{TOOL_USE_PREFIX}{card_id}")),
    }
}

fn card_id_of(prompt: &SessionChatInteractivePrompt) -> Option<&str> {
    prompt.tool_use_id()?.strip_prefix(TOOL_USE_PREFIX)
}

/// Writes (`Some`) or clears (`None`) the session's stored chat prompt and pushes it to open
/// chats. Clearing only removes the card `clear_only` names; writing never covers the agent's own
/// question or another card that is still waiting.
///
/// CDXC:Connectors 2026-10-16 WHY:
/// The read, change and write of `runtimeSettings` happen in one immediate transaction, because the update writes the whole object back and a hook or chat write landing between a separate read and write would be erased.
fn set_session_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt: Option<&SessionChatInteractivePrompt>,
    clear_only: Option<&str>,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let server_id = state.metadata.server_id.as_str();
    let updated = {
        let Ok(transaction) = Transaction::new_unchecked(&db, TransactionBehavior::Immediate)
        else {
            return;
        };
        let repository = DomainRepository::new(&transaction, server_id);
        let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
            return;
        };
        let stored = crate::agents::session_chat_prompt_setting(&session)
            .as_deref()
            .and_then(parse_stored_session_chat_prompt);
        let stored_card = stored.as_ref().and_then(card_id_of).map(str::to_string);
        if let Some(card_id) = clear_only {
            if stored_card.as_deref() != Some(card_id) {
                return;
            }
        } else if stored.is_some() {
            // The agent's own question, or a card still waiting, stays; this card shows next.
            let Some(stored_card) = stored_card else {
                return;
            };
            let replacing_itself = prompt.and_then(card_id_of) == Some(stored_card.as_str());
            if !replacing_itself && is_waiting(&stored_card) {
                return;
            }
        }
        let mut runtime = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let activity = runtime
            .entry("agentActivity".to_string())
            .or_insert_with(|| json!({}));
        let Some(activity) = activity.as_object_mut() else {
            return;
        };
        match prompt {
            Some(prompt) => {
                activity.insert(
                    "sessionChatPrompt".to_string(),
                    Value::String(serde_json::to_string(prompt).unwrap_or_default()),
                );
            }
            None => {
                activity.remove("sessionChatPrompt");
            }
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("sessionId".to_string(), json!(session_id));
        update.insert("runtimeSettings".to_string(), Value::Object(runtime));
        // `update_session` joins the open transaction, so the read above and this write are one.
        let Ok(updated) = repository.update_session(&update) else {
            return;
        };
        if transaction.commit().is_err() {
            return;
        }
        updated
    };
    let repository = DomainRepository::new(&db, server_id);
    crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &updated);
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        project_id,
        session_id,
    );
}

/// Shows the session's oldest waiting card, unless something else is on screen.
fn show_next(state: &AppState, project_id: &str, session_id: &str) {
    let next = with_cards(|cards| {
        cards
            .iter()
            .find(|card| card.project_id == project_id && card.session_id == session_id)
            .map(|card| (card.id.clone(), card.question.clone()))
    });
    if let Some((id, question)) = next {
        set_session_prompt(
            state,
            project_id,
            session_id,
            Some(&chat_prompt(&id, &question)),
            None,
        );
    }
}

/// Takes a card out of the waiting list, clears it from its chat and shows the session's next.
/// Blocking.
fn finish(state: &AppState, card_id: &str, answer: Answer) {
    let card = with_cards(|cards| {
        let index = cards.iter().position(|card| card.id == card_id)?;
        Some(cards.remove(index))
    });
    let Some(mut card) = card else {
        return;
    };
    {
        let mut ended = ENDED.lock().unwrap_or_else(PoisonError::into_inner);
        ended.push((card.id.clone(), answer));
        let excess = ended.len().saturating_sub(ENDED_KEPT);
        ended.drain(..excess);
    }
    if let Some(sender) = card.sender.take() {
        let _ = sender.send(answer);
    }
    set_session_prompt(
        state,
        &card.project_id,
        &card.session_id,
        None,
        Some(&card.id),
    );
    show_next(state, &card.project_id, &card.session_id);
}

/// Ends a card whose wait stopped in any way, including when the MCP process exited and the HTTP
/// handler was dropped mid-wait.
struct WaitGuard {
    state: Arc<AppState>,
    card_id: String,
}

impl Drop for WaitGuard {
    fn drop(&mut self) {
        if !is_waiting(&self.card_id) {
            return;
        }
        let state = self.state.clone();
        let card_id = self.card_id.clone();
        std::thread::spawn(move || finish(&state, &card_id, Answer::Cancelled));
    }
}

/// Shows a card in the session's chat and waits for its answer.
pub(crate) async fn ask(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    call_id: Option<&str>,
    question: String,
) -> Answer {
    // Subscribed before anything can wait, so a shutdown while the card shows wakes it.
    let mut shutdown = state.shutdown_tx.subscribe();
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, mut receiver) = oneshot::channel();
    with_cards(|cards| {
        cards.push(Card {
            id: id.clone(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            question,
            call_id: call_id.map(str::to_string),
            sender: Some(sender),
        })
    });
    let _guard = WaitGuard {
        state: state.clone(),
        card_id: id.clone(),
    };
    let show = {
        let state = state.clone();
        let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
        move || show_next(&state, &project_id, &session_id)
    };
    let _ = tokio::task::spawn_blocking(show.clone()).await;
    let deadline = tokio::time::Instant::now() + CARD_TIMEOUT;
    loop {
        let wake = (tokio::time::Instant::now() + RESHOW_EVERY).min(deadline);
        let waited = tokio::select! {
            waited = tokio::time::timeout_at(wake, &mut receiver) => waited,
            _ = shutdown.recv() => {
                let state = state.clone();
                let id = id.clone();
                let _ = tokio::task::spawn_blocking(move || finish(&state, &id, Answer::Restarting)).await;
                return Answer::Restarting;
            }
        };
        match waited {
            Ok(Ok(answer)) => return answer,
            Ok(Err(_)) => return Answer::Cancelled,
            Err(_) if tokio::time::Instant::now() >= deadline => {
                let state = state.clone();
                let id = id.clone();
                let _ =
                    tokio::task::spawn_blocking(move || finish(&state, &id, Answer::Expired)).await;
                return Answer::Expired;
            }
            // A hook may have cleared the card; put it back when nothing else is on screen.
            Err(_) => {
                let _ = tokio::task::spawn_blocking(show.clone()).await;
            }
        }
    }
}

/// The card the session's agent is waiting on, when its stored chat prompt is one that is still
/// waiting in this gxserver.
pub(crate) fn waiting_card(session: &Value) -> Option<String> {
    let stored = crate::agents::session_chat_prompt_setting(session)
        .as_deref()
        .and_then(parse_stored_session_chat_prompt)?;
    let card_id = card_id_of(&stored)?;
    is_waiting(card_id).then(|| format!("{TOOL_USE_PREFIX}{card_id}"))
}

/// Puts a card in the waiting list the way `ask` does, without an agent call behind it, and
/// returns its id and the stored chat prompt that shows it. `release_waiting_card` ends it.
#[cfg(test)]
pub(crate) fn hold_waiting_card(project_id: &str, session_id: &str) -> (String, String) {
    let id = uuid::Uuid::new_v4().to_string();
    let prompt = serde_json::to_string(&chat_prompt(&id, "Send the email?")).expect("card prompt");
    with_cards(|cards| {
        cards.push(Card {
            id: id.clone(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            question: "Send the email?".to_string(),
            call_id: None,
            sender: None,
        })
    });
    (id, prompt)
}

#[cfg(test)]
pub(crate) fn release_waiting_card(card_id: &str) {
    with_cards(|cards| cards.retain(|card| card.id != card_id));
}

/// The agent cancelled an MCP call: its card goes away.
pub(crate) fn cancel_call(state: &AppState, call_id: &str) -> bool {
    let card_id = with_cards(|cards| {
        cards
            .iter()
            .find(|card| card.call_id.as_deref() == Some(call_id))
            .map(|card| card.id.clone())
    });
    match card_id {
        Some(card_id) => {
            finish(state, &card_id, Answer::Cancelled);
            true
        }
        None => false,
    }
}

/// The chat's answer to a connector card (`answerSessionChatPrompt` with `kind: "question"`).
/// `None` when the session's stored prompt is not one, so the answer goes the ordinary way.
/// Blocking.
///
/// CDXC:Connectors 2026-10-16 WHY:
/// Only a question answer can be meant for a card; other kinds answer the agent's own screen. A `toolUseId` in the answer that is not the stored card's means the user answered a card that is gone, so it is refused instead of settling the card that replaced it.
pub(crate) fn answer_from_chat(
    state: &AppState,
    session: &Value,
    params: &Map<String, Value>,
) -> Option<Result<Value, DomainStateError>> {
    if params.get("kind").and_then(Value::as_str) != Some("question") {
        return None;
    }
    let stored = crate::agents::session_chat_prompt_setting(session)
        .as_deref()
        .and_then(parse_stored_session_chat_prompt)?;
    let card_id = card_id_of(&stored)?.to_string();
    if let Some(answered) = params
        .get("toolUseId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
    {
        if Some(answered) != stored.tool_use_id() {
            return Some(Err(DomainStateError::bad_request(
                "That question changed before your answer arrived. Look at the chat again.",
            )));
        }
    }
    let project_id = session.get("projectId").and_then(Value::as_str)?;
    let session_id = session.get("sessionId").and_then(Value::as_str)?;
    if !is_waiting(&card_id) {
        // Its call already ended (a restart, a cancel): the card just goes away.
        set_session_prompt(state, project_id, session_id, None, Some(&card_id));
        show_next(state, project_id, session_id);
        return Some(Ok(json!({ "answered": true })));
    }
    let picked = params
        .get("selections")
        .and_then(Value::as_array)
        .and_then(|selections| selections.first())
        .and_then(|selection| selection.get("indices"))
        .and_then(Value::as_array)
        .and_then(|indices| indices.first())
        .and_then(Value::as_u64);
    let answer = match picked {
        Some(0) => Answer::Approve,
        Some(1) => Answer::Always,
        _ => Answer::Deny,
    };
    finish(state, &card_id, answer);
    Some(Ok(json!({ "answered": true })))
}
