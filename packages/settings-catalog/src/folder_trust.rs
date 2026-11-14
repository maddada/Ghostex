//! Trust project folders automatically: whether Ghostex marks a session's folder trusted for its
//! agent before it starts, and who may turn that on.
//!
//! CDXC:AgentProviders 2026-11-14 DECISION:
//! User: "can we make it so we show the interface to trust a folder when the agent shows it to us? should be easy to trust folders too. we can have toggle in settings to auto trust folders (can't be enabled without confirmation with pin/fingerprint etc on the machine i'm on when i change that setting (like we did for the collab feature for ex))". So the switch is off by default, for existing installs too: it counts as on only while `trustProjectFoldersConfirmedAt` is set beside it, and only the Settings window of the desktop app on that computer sets it, after the system's own prompt there (Touch ID or the password, Windows Hello or the Windows password, polkit). A saved `true` without it (the old default, a sync, a hand edit) is off. Every write through gxserver's API (the CLI, the phone, another computer, an agent) may turn it off but never on. Supersedes the 2026-10-17 decision that trusted every project folder by default.
//! SEE-ALSO: server/src/agent_folder_trust/launch.rs (what "on" does), server/src/settings_agent_writable.rs (the API refusal), apps/desktop/src/app/window/settings_modal/tabs/agents/folder_trust_switch.rs (the switch and its prompt).

use serde_json::{Map, Value};

/// The Settings > Agents switch.
pub const TRUST_PROJECT_FOLDERS_SETTING: &str = "trustProjectFoldersAutomatically";
/// When the switch was last turned on after the system's prompt on this computer (an RFC 3339
/// time); empty or missing means it was never confirmed here.
pub const TRUST_PROJECT_FOLDERS_CONFIRMED_SETTING: &str = "trustProjectFoldersConfirmedAt";

/// What a write through gxserver's API that would turn the switch on is told.
pub const TURN_ON_REFUSAL: &str = "\"Trust project folders automatically\" turns on only in Settings > Agents in Ghostex on that computer, after Touch ID, Windows Hello or your password there. It can be turned off from anywhere.";

/// Whether Ghostex trusts project folders for agents on its own: the switch is on and was
/// confirmed on this computer.
pub fn trust_project_folders_on(settings: &Map<String, Value>) -> bool {
    settings.get(TRUST_PROJECT_FOLDERS_SETTING) == Some(&Value::Bool(true))
        && settings
            .get(TRUST_PROJECT_FOLDERS_CONFIRMED_SETTING)
            .and_then(Value::as_str)
            .is_some_and(|at| !at.trim().is_empty())
}

/// Whether writing `value` to `key` through the API would turn the switch on, or set its
/// confirmation: only turning it off passes.
pub fn api_write_turns_on_folder_trust(key: &str, value: &Value) -> bool {
    match key {
        TRUST_PROJECT_FOLDERS_SETTING => value != &Value::Bool(false),
        TRUST_PROJECT_FOLDERS_CONFIRMED_SETTING => {
            value.as_str().is_none_or(|at| !at.trim().is_empty())
        }
        _ => false,
    }
}
