//! The Mode menu's "Restart in Bypass when idle" switch: the row under a greyed Bypass, the mode
//! pill's words while it waits, and the action that turns it on or off.
//!
//! CDXC:SessionChat 2026-11-14 DECISION:
//! User: "we need to have a toggle right there and the user can pick 'reload with bypass permissions when agent is idle' so we apply bypass permissions". Under the greyed Bypass row the Mode menu has a switch row; while it waits, its line and the mode pill both say so, and turning it off cancels. gxserver owns the restart and says whether the switch is offered (`UnavailableMode::restart`, server/src/session_chat_mode_restart.rs), so the renderers only draw the row.
//! SEE-ALSO: menus/option_menus.rs (the row's place), apps/desktop/src/app/native_chat/option_menu/render.rs (the desktop draws `toggle` rows as the app's switch), apps/mobile/app/src/chat/native/composer/ModelMenuSheet.tsx (the phone's Mode page).

use ghostex_gx_protocol::{ModeRestart, UnavailableMode};
use serde_json::{json, Map, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::menus::option_menus::NativeChatMenuItem;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

/// The switch row's label.
pub const RESTART_LABEL: &str = "Restart in Bypass when idle";
/// What the mode pill adds while the restart waits.
pub const PILL_SUFFIX: &str = "Bypass when idle";

/// The switch for a greyed mode, when gxserver offers one.
pub fn restart_row(mode: &UnavailableMode) -> Option<NativeChatMenuItem> {
    let restart = mode.restart.as_ref()?;
    let mut command = Map::new();
    command.insert("type".to_string(), json!("restartInMode"));
    command.insert("mode".to_string(), json!(mode.value));
    command.insert("enabled".to_string(), json!(!restart.pending));
    Some(NativeChatMenuItem {
        id: format!("mode:{}:restart", mode.value),
        toggle: Some(true),
        label: Some(RESTART_LABEL.to_string()),
        description: Some(restart_line(restart)),
        checked: Some(restart.pending),
        disabled: Some(false),
        command: Some(command),
        ..NativeChatMenuItem::default()
    })
}

fn restart_line(restart: &ModeRestart) -> String {
    match (&restart.failed, restart.pending) {
        (_, true) => "Waiting for Claude to be idle. Turn off to cancel.".to_string(),
        (Some(why), false) => format!("Didn't restart: {}.", why.trim_end_matches('.')),
        (None, false) => "Claude picks up where it left off.".to_string(),
    }
}

/// Whether a restart into another mode is waiting, for the mode pill.
pub fn restart_pending(state: &ChatState) -> bool {
    state
        .session
        .unavailable_modes
        .iter()
        .any(|mode| mode.restart.as_ref().is_some_and(|restart| restart.pending))
}

/// The pill's words while the restart waits: the current mode, then what comes.
pub fn pill_label(current: Option<&str>) -> String {
    match current.filter(|label| !label.is_empty()) {
        Some(label) => format!("{label} · {PILL_SUFFIX}"),
        None => PILL_SUFFIX.to_string(),
    }
}

/// `restartInMode`: turns the switch on or off. The row moves at once; gxserver's next frame
/// says where it really stands.
pub fn toggle(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let mode = action
        .param("mode")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let enabled = action.param("enabled").and_then(Value::as_bool) == Some(true);
    let Some(entry) = state
        .session
        .unavailable_modes
        .iter_mut()
        .find(|entry| entry.value == mode && entry.restart.is_some())
    else {
        return Vec::new();
    };
    entry.restart = Some(ModeRestart {
        pending: enabled,
        failed: None,
    });
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::RestartSessionInMode,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "mode": mode,
            "enabled": enabled,
        })),
    }]
}
