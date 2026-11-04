//! The panel's reads: one when it comes on screen, then a slow poll while it stays there.
//!
//! CDXC:SessionDetails 2026-10-25 DECISION:
//! User (Q8): "make the update really slow and only happen for the currently shown session". A chat reads only while its panel is on screen (`visible`), every `POLL_MS`, and the poll timer is one a hidden chat holds (`session/timers.rs`). gxserver decides when Claude's breakdown is counted again (server/src/session_details/probe.rs); while a count runs the poll comes back sooner so its result shows without waiting a full period.

use serde_json::{json, Value};

use crate::effect::Effect;
use crate::extras::session_details::state::{collapsed_key, read_collapsed};
use crate::extras::session_details::SESSION_DETAILS_POLL;
use crate::state::{ChatContext, ChatState};
use crate::wire::{ChatRpcMethod, RpcOutcome};

/// How often an open panel reads again.
const POLL_MS: f64 = 10_000.0;
/// How soon it reads again while Claude's breakdown is being counted.
const COUNTING_POLL_MS: f64 = 2_500.0;

/// The panel applies to this chat: the extension is on and the chat knows its session's agent.
///
/// CDXC:SessionDetails 2026-10-25 DECISION:
/// User (Q555): every chat agent gets the panel, with Environment, Changes, Context, Skills and Tasks wherever the data exists; it was Claude's alone before. What each agent shows is decided where its data is read (server/src/session_details/agents.rs) and in `document.rs`, which leaves out a section an agent has nothing for (Q557).
pub fn enabled(state: &ChatState) -> bool {
    state.core.session_details_enabled && state.session.agent.is_some()
}

/// The session is Claude's, whose panel has every section, the breakdown and its refresh button.
pub fn claude(state: &ChatState) -> bool {
    state.session.agent.as_deref() == Some("claude")
}

fn read(state: &mut ChatState, refresh: bool, request_id: u64) -> Effect {
    let details = &mut state.extras.session_details;
    details.request = Some(request_id);
    details.refreshing = refresh;
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionDetails,
        params: Box::new(json!({ "breakdown": if refresh { "refresh" } else { "auto" } })),
    }
}

/// The panel came on screen or left it.
pub fn set_visible(
    state: &mut ChatState,
    visible: bool,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<Effect> {
    let details = &mut state.extras.session_details;
    if details.visible == visible {
        return Vec::new();
    }
    details.visible = visible;
    state.core.request_publish();
    if !visible {
        // The Tasks card comes back in its short form the next time the panel is shown.
        state.extras.session_details.tasks_step = 0;
        state.core.timers.cancel(SESSION_DETAILS_POLL);
        return Vec::new();
    }
    if !enabled(state) {
        return Vec::new();
    }
    let mut effects = Vec::new();
    if !state.extras.session_details.collapsed_read {
        state.extras.session_details.collapsed_read = true;
        effects.push(Effect::ReadStorage {
            key: collapsed_key(),
        });
    }
    if state.extras.session_details.request.is_none() {
        effects.push(read(state, false, next_request_id()));
    }
    effects
}

/// The refresh button: a read that asks Claude to count its context again.
pub fn refresh(state: &mut ChatState, mut next_request_id: impl FnMut() -> u64) -> Vec<Effect> {
    if !enabled(state) || !claude(state) {
        return Vec::new();
    }
    if state.extras.session_details.request.is_some() {
        state.extras.session_details.refresh_queued = true;
        return Vec::new();
    }
    state.core.request_publish();
    vec![read(state, true, next_request_id())]
}

fn counting(data: Option<&Value>) -> bool {
    data.and_then(|data| data.pointer("/context/breakdownRunning")) == Some(&Value::Bool(true))
}

/// An answer to one of the panel's requests, or `None` when it was not the panel's.
pub fn settle_rpc(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
    context: &ChatContext,
    mut next_request_id: impl FnMut() -> u64,
) -> Option<Vec<Effect>> {
    if let Some(at) = state
        .extras
        .session_details
        .stops
        .iter()
        .position(|(id, _)| *id == request_id)
    {
        let (_, task) = state.extras.session_details.stops.remove(at);
        state.core.claim_refusal();
        state.core.request_publish();
        return Some(match outcome {
            RpcOutcome::Ok { .. } => Vec::new(),
            RpcOutcome::Err { message, .. } => {
                state
                    .extras
                    .session_details
                    .stopping
                    .retain(|id| *id != task);
                vec![Effect::Toast {
                    level: "error".to_string(),
                    message: if message.trim().is_empty() {
                        "The background command couldn't be stopped.".to_string()
                    } else {
                        message.clone()
                    },
                }]
            }
        });
    }
    if state.extras.session_details.request != Some(request_id) {
        return None;
    }
    state.core.claim_refusal();
    let details = &mut state.extras.session_details;
    details.request = None;
    details.refreshing = false;
    match outcome {
        RpcOutcome::Ok { result } => {
            details.data = Some(result.clone());
            details.error = None;
        }
        // An older gxserver has no `readSessionDetails` and refuses it without a sentence.
        RpcOutcome::Err { message, .. } if !message.chars().any(char::is_alphabetic) => {
            details.error = Some(
                "Ghostex on this computer is too old for the Session details panel. Update it and try again."
                    .to_string(),
            );
        }
        RpcOutcome::Err { message, .. } => details.error = Some(message.clone()),
    }
    state.core.request_publish();
    let mut effects = Vec::new();
    if state.extras.session_details.visible {
        if std::mem::take(&mut state.extras.session_details.refresh_queued) {
            effects.push(read(state, true, next_request_id()));
        } else {
            let delay = if counting(state.extras.session_details.data.as_ref()) {
                COUNTING_POLL_MS
            } else {
                POLL_MS
            };
            state
                .core
                .timers
                .arm(SESSION_DETAILS_POLL, context.now_ms, delay);
        }
    }
    Some(effects)
}

/// The poll timer fired: read again while the panel is still on screen.
pub fn settle_tick(state: &mut ChatState, next_request_id: impl FnOnce() -> u64) -> Vec<Effect> {
    if !state.core.timer_fired(SESSION_DETAILS_POLL)
        || !state.extras.session_details.visible
        || state.extras.session_details.request.is_some()
        || !enabled(state)
    {
        return Vec::new();
    }
    vec![read(state, false, next_request_id())]
}

/// The remembered folds arrived.
pub fn settle_collapsed(state: &mut ChatState, value: Option<&str>) {
    state.extras.session_details.collapsed = read_collapsed(value);
    state.core.request_publish();
}

/// Background commands asked to stop leave the list once they are no longer running.
pub fn prune_stopping(state: &mut ChatState) {
    if state.extras.session_details.stopping.is_empty() {
        return;
    }
    let running: Vec<String> = state
        .session
        .terminal_activity
        .as_ref()
        .and_then(|activity| activity.pointer("/background/tasks"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|task| task.get("id").and_then(Value::as_str).map(str::to_string))
        .collect();
    let pending: Vec<String> = state
        .extras
        .session_details
        .stops
        .iter()
        .map(|(_, task)| task.clone())
        .collect();
    state
        .extras
        .session_details
        .stopping
        .retain(|id| running.contains(id) || pending.contains(id));
}
