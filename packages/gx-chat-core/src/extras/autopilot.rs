//! Autopilot's pill in the chat box's strip. gxserver puts `autopilot: { on, covered }` on every
//! chat frame (server/src/autopilot/mod.rs). A coordinator's chat always has the pill, as
//! Autopilot's own switch (on or off for everything it covers); every other chat has it only while
//! Autopilot is on, as that session's switch, so a session can be taken out of it (or put back).
//! The desktop and the web build draw it as "Autopilot" with a green dot while on and as
//! "Autopilot off" while off; a click flips it (native_chat/chat_pills.rs).
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: Autopilot lives "in the coordinator chat (a switch/pill there)", and "add the pills for all sessions when this feature is enabled so it can be disabled on a session". Clicking the pill flips it at once; gxserver saves it (`/api/autopilot`), and a refusal flips it back with a toast.

use serde_json::{json, Value};

use crate::document::ChatPill;
use crate::effect::Effect;
use crate::state::ChatState;

/// The pill's id in `chatPills`.
pub const AUTOPILOT_PILL: &str = "autopilot";
const LABEL: &str = "Autopilot";

fn flag(state: &ChatState, key: &str) -> Option<bool> {
    state
        .session
        .autopilot
        .as_ref()
        .and_then(|autopilot| autopilot.get(key))
        .and_then(Value::as_bool)
}

fn is_coordinator(state: &ChatState) -> bool {
    state.session.coordinator_threads.is_some()
}

/// The pill: Autopilot's switch on a coordinator, the session's own switch elsewhere while
/// Autopilot is on. `None` before gxserver said anything, or on an older gxserver.
pub fn pill(state: &ChatState) -> Option<ChatPill> {
    let on = flag(state, "on")?;
    let (checked, aria_label) = if is_coordinator(state) {
        (
            on,
            if on {
                "Autopilot is on: it keeps every covered session going. Click to turn it off."
            } else {
                "Autopilot is off. Click to turn it on: it keeps sessions going while you are away."
            },
        )
    } else {
        if !on {
            return None;
        }
        let covered = flag(state, "covered").unwrap_or(false);
        (
            covered,
            if covered {
                "Autopilot keeps this session going. Click to leave it out."
            } else {
                "Autopilot leaves this session alone. Click to put it back."
            },
        )
    };
    Some(ChatPill {
        id: AUTOPILOT_PILL.to_string(),
        label: if checked {
            LABEL.to_string()
        } else {
            format!("{LABEL} off")
        },
        aria_label: aria_label.to_string(),
        switch: Some(checked),
        ..ChatPill::default()
    })
}

/// `toggleAutopilot`: flips the pill at once and asks gxserver to save it: Autopilot itself on a
/// coordinator, this session elsewhere.
pub fn toggle(state: &mut ChatState, request_id: u64) -> Vec<Effect> {
    let Some(on) = flag(state, "on") else {
        return Vec::new();
    };
    let previous = state.session.autopilot.clone();
    let params = if is_coordinator(state) {
        state.session.autopilot = Some(json!({
            "on": !on,
            "covered": flag(state, "covered").unwrap_or(false),
        }));
        json!({ "op": "set", "patch": { "enabled": !on }, "limit": 0 })
    } else {
        let covered = flag(state, "covered").unwrap_or(false);
        state.session.autopilot = Some(json!({ "on": on, "covered": !covered }));
        json!({
            "op": "setSession",
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "on": !covered,
            "limit": 0,
        })
    };
    state.extras.panels.autopilot_request = Some((request_id, previous));
    vec![Effect::SendRpc {
        request_id,
        method: crate::wire::ChatRpcMethod::Autopilot,
        params: Box::new(params),
    }]
}

/// Takes gxserver's answer to [`toggle`]: saved, nothing more (the next frame confirms it);
/// refused, the pill flips back and a toast says why. `None` when `request_id` is not that call.
pub fn settle_toggle(
    state: &mut ChatState,
    request_id: u64,
    answer: Result<&Value, String>,
) -> Option<Vec<Effect>> {
    let (pending, previous) = state.extras.panels.autopilot_request.clone()?;
    if pending != request_id {
        return None;
    }
    state.extras.panels.autopilot_request = None;
    Some(match answer {
        Ok(_) => Vec::new(),
        Err(message) => {
            state.session.autopilot = previous;
            vec![Effect::Toast {
                level: "error".to_string(),
                message: if message.is_empty() {
                    "Couldn't change Autopilot.".to_string()
                } else {
                    message
                },
            }]
        }
    })
}
