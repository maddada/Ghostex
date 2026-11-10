//! Autopilot's pill in the chat box's strip. gxserver puts `autopilot: { on, covered, offline }`
//! on every chat frame (server/src/autopilot/mod.rs). A coordinator's chat always has the pill, as
//! Autopilot's own switch (on or off for everything it covers); every other chat has it only while
//! Autopilot is on, as that session's switch, so a session can be taken out of it (or put back).
//! The desktop and the web build draw it as "Autopilot" with a green dot while on and as
//! "Autopilot off" while off; a click opens its small menu (native_chat/chat_pills.rs), whose rows
//! are the pill's `menu`.
//!
//! CDXC:Autopilot 2026-11-10 DECISION:
//! User: "just say 'Autopilot is on/off' for this tooltip for the pill above the composer". The pill's tooltip (its `ariaLabel`) is just "Autopilot is on" or "Autopilot is off"; a session's own switch says "Autopilot is on for this session" or "... off for this session", and the offline one "Autopilot is waiting for the network". No summary sentence, no "Opens its menu".
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: Autopilot lives "in the coordinator chat (a switch/pill there)", and "add the pills for all sessions when this feature is enabled so it can be disabled on a session". The switch flips at once; gxserver saves it (`/api/autopilot`), and a refusal flips it back with a toast. (The click that flipped it is superseded by the next decision.)
//!
//! CDXC:Autopilot 2026-11-07 DECISION:
//! User: "please make clicking on the autopilot button show a context menu to toggle it or go to settings so i can jump to its settings (clicking shouldn't toggle it like it does now)". A click (a right-click too) on the pill opens a menu in the sidebar menu's style with two rows: the toggle that fits where you are ("Turn Autopilot off for this session" / "Turn Autopilot on for this session" in a session's chat; "Turn Autopilot off" / "Turn Autopilot on" in a coordinator's) and "Autopilot settings...", which opens Settings > Autopilot. The pill itself never toggles. The phone's tap opens a sheet with the same rows (its settings row opens Settings > Autopilot). Supersedes the click-to-toggle one.

use serde_json::{json, Value};

use crate::document::ChatPill;
use crate::effect::Effect;
use crate::state::ChatState;

/// The pill's id in `chatPills`.
pub const AUTOPILOT_PILL: &str = "autopilot";
const LABEL: &str = "Autopilot";
/// The pill's words while Autopilot is on and this computer is offline (gxserver's
/// `network_status::OFFLINE_LABEL`, CDXC:OfflineHold).
const OFFLINE_LABEL: &str = "Offline: waiting for the network";
const OFFLINE_ARIA: &str = "Autopilot is waiting for the network";
/// The menu row that opens Settings > Autopilot: a renderer handles this command itself (the core
/// has nothing to do for it).
pub const OPEN_SETTINGS_COMMAND: &str = "openAutopilotSettings";

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
    let (checked, aria_label, toggle_label) = if is_coordinator(state) {
        (
            on,
            if on {
                "Autopilot is on"
            } else {
                "Autopilot is off"
            },
            if on {
                "Turn Autopilot off"
            } else {
                "Turn Autopilot on"
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
                "Autopilot is on for this session"
            } else {
                "Autopilot is off for this session"
            },
            if covered {
                "Turn Autopilot off for this session"
            } else {
                "Turn Autopilot on for this session"
            },
        )
    };
    let waiting = checked && flag(state, "offline").unwrap_or(false);
    Some(ChatPill {
        id: AUTOPILOT_PILL.to_string(),
        label: if waiting {
            OFFLINE_LABEL.to_string()
        } else if checked {
            LABEL.to_string()
        } else {
            format!("{LABEL} off")
        },
        aria_label: if waiting { OFFLINE_ARIA } else { aria_label }.to_string(),
        switch: Some(checked),
        menu: vec![
            json!({
                "label": toggle_label,
                "iconPath": "titlebar/plane.svg",
                "command": { "type": "toggleAutopilot" },
            }),
            json!({ "separator": true }),
            json!({
                "label": "Autopilot settings\u{2026}",
                "iconPath": "titlebar/settings.svg",
                "command": { "type": OPEN_SETTINGS_COMMAND },
            }),
        ],
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
            "offline": flag(state, "offline").unwrap_or(false),
        }));
        json!({ "op": "set", "patch": { "enabled": !on }, "limit": 0 })
    } else {
        let covered = flag(state, "covered").unwrap_or(false);
        state.session.autopilot = Some(json!({
            "on": on,
            "covered": !covered,
            "offline": flag(state, "offline").unwrap_or(false),
        }));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(pill: &ChatPill) -> Vec<String> {
        pill.menu
            .iter()
            .filter_map(|row| row["label"].as_str().map(str::to_owned))
            .collect()
    }

    #[test]
    fn a_session_s_menu_toggles_that_session_and_opens_settings() {
        let mut state = ChatState::default();
        state.session.autopilot = Some(json!({"on": true, "covered": true}));
        let pill = pill(&state).expect("pill");
        assert_eq!(
            labels(&pill),
            [
                "Turn Autopilot off for this session",
                "Autopilot settings\u{2026}"
            ]
        );
        state.session.autopilot = Some(json!({"on": true, "covered": false}));
        assert_eq!(
            labels(&super::pill(&state).expect("pill"))[0],
            "Turn Autopilot on for this session"
        );
        assert_eq!(pill.menu[2]["command"]["type"], OPEN_SETTINGS_COMMAND);
    }

    #[test]
    fn the_pill_says_it_waits_for_the_network_while_offline() {
        let mut state = ChatState::default();
        *state.session.coordinator_threads = Some(json!({}).into());
        state.session.autopilot = Some(json!({"on": true, "covered": false, "offline": true}));
        let pill = pill(&state).expect("pill");
        assert_eq!(pill.label, "Offline: waiting for the network");
        assert_eq!(pill.switch, Some(true));
        assert_eq!(labels(&pill)[0], "Turn Autopilot off");
        // Off, the pill says it is off: nothing is waiting.
        state.session.autopilot = Some(json!({"on": false, "covered": false, "offline": true}));
        assert_eq!(super::pill(&state).expect("pill").label, "Autopilot off");
        // A session it covers says so too; one it leaves alone does not.
        *state.session.coordinator_threads = None;
        state.session.autopilot = Some(json!({"on": true, "covered": true, "offline": true}));
        assert_eq!(
            super::pill(&state).expect("pill").label,
            "Offline: waiting for the network"
        );
        state.session.autopilot = Some(json!({"on": true, "covered": false, "offline": true}));
        assert_eq!(super::pill(&state).expect("pill").label, "Autopilot off");
        // Back online: the usual label.
        state.session.autopilot = Some(json!({"on": true, "covered": true, "offline": false}));
        assert_eq!(super::pill(&state).expect("pill").label, "Autopilot");
    }

    #[test]
    fn a_coordinator_s_menu_turns_autopilot_on_or_off() {
        let mut state = ChatState::default();
        *state.session.coordinator_threads = Some(json!({}).into());
        state.session.autopilot = Some(json!({"on": false, "covered": false}));
        assert_eq!(labels(&pill(&state).expect("pill"))[0], "Turn Autopilot on");
        state.session.autopilot = Some(json!({"on": true, "covered": false}));
        assert_eq!(
            labels(&pill(&state).expect("pill"))[0],
            "Turn Autopilot off"
        );
    }
}
