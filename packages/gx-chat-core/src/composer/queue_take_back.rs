//! Back to composer on a message the agent CLI queued behind its running turn.
//!
//! The message lives in the agent's own queue, not in Ghostex's queue strip, so gxserver takes it
//! out (`/api/takeBackSessionChatQueuedMessage`, server/src/session_chat_send/claude_queue_pull.rs)
//! and answers with its text; the core then reads the composer and appends that text to it.
//!
//! CDXC:SessionChat 2026-10-11 DECISION:
//! User: "when a message is still queued while the agent is working i want a button (and a hotkey when i hover on it) that lets me return it to the chat composer please (does the hotkey that does this in the claude code cli". The button sits in a queued bubble's actions on the desktop and the phone; the hotkey is Claude Code's own, Up in an empty composer, which takes back the newest queued message. The text and its pictures are appended to whatever draft the composer holds, never replacing it.

use serde_json::{json, Value};

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

use crate::action::UserAction;
use crate::composer::references::renumber_image_references;
use crate::composer::transcript_menu::append_draft_text;
use crate::effect::Effect;
use crate::session::constants::PENDING_ID_PREFIX;
use crate::session::sends;
use crate::state::ChatState;
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// The key the composer takes a queued message back with, as tooltips name it.
pub const TAKE_BACK_HOTKEY_LABEL: &str = "↑";

/// The step a take-back is waiting on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueTakeBackStage {
    /// gxserver is taking the message out of the agent's queue.
    TakingBack,
    /// The composer's current text, so the message is appended to it.
    Reading,
}

/// A take-back in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueTakeBack {
    pub request_id: u64,
    pub stage: QueueTakeBackStage,
    /// The bubble the take-back started from, drawn busy until it ends.
    pub message_id: String,
    /// What gxserver handed back, once it has.
    pub text: String,
}

/// Agents whose own queue gxserver can take a message back out of (Claude Code with Up and its
/// transcript, Grok Build with its queue panel's `x`).
fn agent_supports_take_back(agent: Option<&str>) -> bool {
    matches!(agent, Some("claude" | "grok"))
}

/// A user message the agent holds in its own queue: Claude's enqueue row, or chat's echo of a send
/// made while the agent worked. A row still waiting in Ghostex's queue (`startupDelivery`) has its
/// own Edit.
fn is_takeable(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && message.queued
        && message.startup_delivery.is_none()
        && !message_text(message).trim().is_empty()
}

/// Every message Back to composer is offered on, oldest first; the last is the one the hotkey
/// takes back. Empty when the agent's queue cannot give messages back.
pub fn takeable_ids(state: &ChatState) -> Vec<String> {
    if !agent_supports_take_back(state.session.agent.as_deref()) {
        return Vec::new();
    }
    state
        .messages
        .composed
        .iter()
        .filter(|message| is_takeable(message))
        .map(|message| message.id.clone())
        .collect()
}

/// The message as gxserver matches it against the agent's queue: its pictures as the `[Image #N](path)`
/// references Claude records them with, then its text.
fn message_text(message: &ChatMessage) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut images = 0;
    for block in &message.blocks {
        match block {
            ChatBlock::ImageRef { path, url, .. } => {
                if let Some(target) = path.as_deref().or(url.as_deref()).filter(|t| !t.is_empty()) {
                    images += 1;
                    parts.push(format!("[Image #{images}]({target})"));
                }
            }
            ChatBlock::Text { text } => parts.push(text.clone()),
            _ => {}
        }
    }
    parts.join(" ")
}

/// `takeBackQueued`: the named message, or the newest one the agent holds.
pub fn begin(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    if state.composer.queue_take_back.is_some() {
        return Vec::new();
    }
    let ids = takeable_ids(state);
    let wanted = action.param("messageId").and_then(Value::as_str);
    let Some(id) = (match wanted {
        Some(wanted) => ids.iter().find(|id| id.as_str() == wanted),
        None => ids.last(),
    }) else {
        return Vec::new();
    };
    let Some(message) = state
        .messages
        .composed
        .iter()
        .find(|message| &message.id == id)
    else {
        return Vec::new();
    };
    let text = message_text(message);
    // An echo's send may still be on its way to the terminal; gxserver runs the take-back behind
    // it in the session's send queue, so by then the agent has queued it.
    let message_id = id.clone();
    let request_id = state.core.allocate_request_id();
    state.composer.queue_take_back = Some(QueueTakeBack {
        request_id,
        stage: QueueTakeBackStage::TakingBack,
        message_id,
        text: String::new(),
    });
    state.core.request_publish();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::TakeBackSessionChatQueuedMessage,
        params: Box::new(json!({ "text": text })),
    }]
}

/// Moves the take-back on when `request_id` is the answer it waits on.
pub fn advance(state: &mut ChatState, request_id: u64, outcome: &RpcOutcome) -> Vec<Effect> {
    let Some(take_back) = state.composer.queue_take_back.take() else {
        return Vec::new();
    };
    if take_back.request_id != request_id {
        state.composer.queue_take_back = Some(take_back);
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
    match take_back.stage {
        QueueTakeBackStage::TakingBack => {
            let text = result
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if result.get("takenBack") != Some(&Value::Bool(true)) {
                state.core.fail(
                    "The agent already took this message, so it stays sent.",
                    None,
                );
            } else if let Some(pending_id) = take_back.message_id.strip_prefix(PENDING_ID_PREFIX) {
                // Grok never writes a queued message to its transcript, so nothing else would
                // replace the chat's own echo of it once it is out of the queue.
                sends::drop_send(state, pending_id);
            }
            if text.trim().is_empty() {
                return Vec::new();
            }
            let request_id = state.core.allocate_request_id();
            state.composer.queue_take_back = Some(QueueTakeBack {
                request_id,
                stage: QueueTakeBackStage::Reading,
                text,
                ..take_back
            });
            vec![Effect::SendRpc {
                request_id,
                method: ChatRpcMethod::ReadNativeComposer,
                params: Box::new(json!({})),
            }]
        }
        QueueTakeBackStage::Reading => {
            let current = result.as_str().unwrap_or_default();
            let content = append_draft_text(
                current,
                &renumber_image_references(current, &take_back.text),
            );
            let caret = content.encode_utf16().count();
            vec![Effect::SetComposerText {
                content,
                caret: Some(caret),
                from_history: false,
            }]
        }
    }
}
