//! The Mode menu's "Claude in Chrome" switch: the last row of a Claude Code chat's Mode menu, and
//! the action that moves it.
//!
//! CDXC:SessionChat 2026-11-28 DECISION:
//! GitHub #790, the user's picks on the Chrome switch page: at the bottom of the Mode pill menu
//! under the approval levels (Q5313 B), labelled "Claude in Chrome" with "Browser tools for this
//! chat" under it (Q5314 B), restarting Claude when idle (Q5315 A), on the desktop, the browser
//! build and the phone alike. gxserver owns the restart and says whether the chat has the switch
//! (`ClaudeChromeSwitch`, server/src/session_chat_chrome_switch.rs), so the renderers only draw the
//! row: the desktop and the browser build draw `toggle` rows as the app's switch
//! (apps/desktop/src/app/native_chat/option_menu/render.rs), the phone's Mode page as a Switch
//! (apps/mobile/app/src/chat/native/composer/ModelMenuSheet.tsx).
//! SEE-ALSO: menus/mode_restart.rs (the Full permissions switch it follows).

use ghostex_gx_protocol::ClaudeChromeSwitch;
use serde_json::{json, Map, Value};

use crate::action::UserAction;
use crate::effect::Effect;
use crate::menus::option_menus::NativeChatMenuItem;
use crate::state::ChatState;
use crate::wire::ChatRpcMethod;

/// The switch row's label.
pub const LABEL: &str = "Claude in Chrome";
/// The switch row's line while nothing waits.
pub const LINE: &str = "Browser tools for this chat";
/// The row's icon (the desktop's and the browser build's).
const ICON: &str = "titlebar/world.svg";

/// The rows the Mode menu ends with when the chat has the switch: a separator and the switch.
pub fn rows(switch: &ClaudeChromeSwitch) -> Vec<NativeChatMenuItem> {
    if !switch.offered {
        return Vec::new();
    }
    let mut separator = NativeChatMenuItem::new("mode:chrome:separator");
    separator.separator = Some(true);
    let mut command = Map::new();
    command.insert("type".to_string(), json!("setClaudeChrome"));
    command.insert("on".to_string(), json!(!switch.on));
    let row = NativeChatMenuItem {
        id: "mode:chrome".to_string(),
        toggle: Some(true),
        label: Some(LABEL.to_string()),
        description: Some(line(switch)),
        icon_path: Some(ICON.to_string()),
        checked: Some(switch.on),
        disabled: Some(false),
        command: Some(command),
        ..NativeChatMenuItem::default()
    };
    vec![separator, row]
}

fn line(switch: &ClaudeChromeSwitch) -> String {
    match (&switch.failed, switch.pending) {
        (_, true) => "Waiting for Claude to be idle. Turn back to cancel.".to_string(),
        (Some(why), false) => format!("Didn't restart: {}.", why.trim_end_matches('.')),
        (None, false) => LINE.to_string(),
    }
}

/// What the mode pill adds while a Chrome restart waits, as the Full permissions switch's does
/// (menus/mode_restart.rs `PILL_SUFFIX`).
pub fn pill_suffix(state: &ChatState) -> Option<&'static str> {
    let switch = &state.session.claude_chrome;
    (switch.offered && switch.pending).then_some(if switch.on {
        "Chrome on when idle"
    } else {
        "Chrome off when idle"
    })
}

/// `setClaudeChrome`: moves the switch. The row moves at once; gxserver's next frame says where it
/// really stands (back to what the chat runs with cancels a waiting restart).
pub fn toggle(state: &mut ChatState, action: &UserAction) -> Vec<Effect> {
    let switch = &mut state.session.claude_chrome;
    if !switch.offered {
        return Vec::new();
    }
    let on = action.param("on").and_then(Value::as_bool) == Some(true);
    // Moving it back while a restart waits lands on what the chat already runs with.
    switch.pending = !switch.pending;
    switch.on = on;
    switch.failed = None;
    state.core.request_publish();
    let request_id = state.core.allocate_request_id();
    vec![Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SetSessionClaudeChrome,
        params: Box::new(json!({
            "projectId": state.identity.project_id,
            "sessionId": state.identity.session_id,
            "on": on,
        })),
    }]
}
