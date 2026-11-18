//! The `firstMessageCard` document key: a new coordinator's first message (its skill line, then
//! the user's first request), typed into the agent's input box and not sent yet, drawn as a card
//! at the top of the chat box with Show all, Edit and Start.
//!
//! gxserver keeps the message and decides everything about the agent's box
//! (server/src/session_chat_first_message.rs, where the user's decision is recorded as
//! CDXC:Coordinators 2026-11-18): Start hands it to the agent, Edit takes it out of the box and
//! answers its text, and Enter with typed text in the chat box starts it first on its own, so the
//! typed message goes behind it. The core draws the card from `firstMessage`, sends the two calls,
//! and puts an edited message into the chat box.
//!
//! SEE-ALSO: apps/desktop/src/app/native_chat/first_message_card.rs and
//! apps/mobile/app/src/chat/native/composer/FirstMessageCard.tsx draw the card from this key.

use ghostex_gx_protocol::Tri;
use serde_json::{json, Value};

use crate::action::{ActionKind, UserAction};
use crate::composer::transcript_menu::append_draft_text;
use crate::effect::Effect;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The card's words.
pub const TITLE: &str = "First message";
pub const START_LABEL: &str = "Start";
pub const EDIT_LABEL: &str = "Edit";
pub const SHOW_ALL_LABEL: &str = "Show all";
pub const SHOW_LESS_LABEL: &str = "Show less";
/// The line beside the title while the message sits in the agent's input box.
pub const NOTE_IN_BOX: &str = "In the agent's input box, not sent";
/// The line beside the title while it is not (the agent is still starting, or its box can't
/// take it): Start still sends it.
pub const NOTE_NOT_SENT: &str = "Not sent yet";
/// The chat box's placeholder while the card shows.
pub const PLACEHOLDER: &str = "Change the model and effort below if you like, then Start.";
/// The lines the card shows before Show all.
pub const COLLAPSED_LINES: u64 = 2;

/// The card's own state in the chat.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FirstMessageUi {
    /// Show all is open.
    pub expanded: bool,
    /// Start or Edit in flight.
    pub pending: Option<Pending>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub request_id: u64,
    pub step: Step,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Starting,
    /// Edit: gxserver is taking the message out of the agent's box.
    Editing,
    /// Edit: the chat box's text is being read, so the message is added to it.
    Reading(String),
}

/// Takes gxserver's `firstMessage` when the frame or read carries one. When it goes, Show all closes.
pub fn apply(state: &mut ChatState, incoming: &Tri<Value>) {
    if matches!(incoming, Tri::Absent) || &state.session.first_message == incoming {
        return;
    }
    state.session.first_message = incoming.clone();
    if !waiting(state) {
        state.composer.first_message.expanded = false;
    }
    state.core.request_publish();
}

fn message(state: &ChatState) -> Option<&Value> {
    match &state.session.first_message {
        Tri::Value(value) if value.get("text").and_then(Value::as_str).is_some() => Some(value),
        _ => None,
    }
}

/// Whether a first message waits for Start.
pub fn waiting(state: &ChatState) -> bool {
    message(state).is_some()
}

/// The card, or `None` while no first message waits (or once Start has gone out).
pub fn card(state: &ChatState) -> Option<Value> {
    let message = message(state)?;
    let ui = &state.composer.first_message;
    if ui
        .pending
        .as_ref()
        .is_some_and(|pending| pending.step == Step::Starting)
    {
        return None;
    }
    let text = message
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let in_box = matches!(
        message.get("state").and_then(Value::as_str),
        Some("typed" | "lifted")
    );
    // The collapsed card shows the message without its blank lines, so the skill line's blank
    // line does not take one of its two lines from the request under it.
    let preview = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let lines = preview.lines().count() as u64;
    Some(json!({
        "title": TITLE,
        "note": if in_box { NOTE_IN_BOX } else { NOTE_NOT_SENT },
        "text": text,
        "preview": preview,
        "inBox": in_box,
        "expanded": ui.expanded,
        "collapsedLines": COLLAPSED_LINES,
        "lines": lines,
        "busy": ui.pending.is_some(),
        "startLabel": START_LABEL,
        "editLabel": EDIT_LABEL,
        "showAllLabel": SHOW_ALL_LABEL,
        "showLessLabel": SHOW_LESS_LABEL,
        "placeholder": PLACEHOLDER,
    }))
}

/// `firstMessageStart`, `firstMessageEdit` and `firstMessageToggle`.
pub fn act(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    if !waiting(state) {
        return Vec::new();
    }
    let (method, step) = match action.kind {
        ActionKind::FirstMessageToggle => {
            state.composer.first_message.expanded = !state.composer.first_message.expanded;
            state.core.request_publish();
            return Vec::new();
        }
        ActionKind::FirstMessageStart => {
            (ChatRpcMethod::StartSessionChatFirstMessage, Step::Starting)
        }
        ActionKind::FirstMessageEdit => (ChatRpcMethod::EditSessionChatFirstMessage, Step::Editing),
        _ => return Vec::new(),
    };
    if state.composer.first_message.pending.is_some() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.composer.first_message.pending = Some(Pending { request_id, step });
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method,
        params: Box::new(json!({})),
    }]
}

/// Moves Start or Edit on when `request_id` is the answer it waits on.
pub fn advance(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Vec<Effect> {
    let Some(pending) = state.composer.first_message.pending.take() else {
        return Vec::new();
    };
    if pending.request_id != request_id {
        state.composer.first_message.pending = Some(pending);
        return Vec::new();
    }
    state.core.request_publish();
    let result = match outcome {
        RpcOutcome::Ok { result } => result,
        RpcOutcome::Err { message, .. } => {
            state.core.fail(message, None);
            return Vec::new();
        }
    };
    match pending.step {
        Step::Starting => Vec::new(),
        Step::Editing => {
            let text = result
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if text.trim().is_empty() {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            state.composer.first_message.pending = Some(Pending {
                request_id,
                step: Step::Reading(text),
            });
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::ReadNativeComposer,
                params: Box::new(json!({})),
            }]
        }
        Step::Reading(text) => {
            let current = result.as_str().unwrap_or_default();
            let content = append_draft_text(current, &text);
            let caret = content.encode_utf16().count();
            vec![Effect::SetComposerText {
                content,
                caret: Some(caret),
                from_history: false,
            }]
        }
    }
}
