//! Provider-agnostic ACP session support (`ghostex acp-host` running inside `zmx`).
//!
//! CDXC:AgentProviders 2026-11-22 DECISION:
//! User: keep CLI + `zmx` as the default for every agent, trial Antigravity on ACP behind an
//! opt-in per-agent toggle (with Codex expected next), preserve reload survival across `gxserver`
//! reloads and app rebuilds, and show a confirmation clarifying that switching an ACP session to
//! Terminal takes a moment and interrupts any running turn.
//!
//! Running `ghostex acp-host` inside an ordinary `zmx` session keeps every existing session
//! lifecycle contract (sleep, wake, close, Windows/Linux/macOS, remote hosts, older phones)
//! intact while giving `gxserver` structured send, cancel, approval, model/mode, and in-place
//! CLI handoff doors over the existing `/api/agentChatChannel` long-poll channel.

use serde_json::Value;

pub(crate) mod channel;
pub mod host;
pub(crate) mod spec;

/// Settings key (object map of `<agentFamily>: bool`) storing per-agent ACP opt-in.
pub const ACP_MODE_SETTING_KEY: &str = "agentAcpMode";

/// Session `runtimeSettings` key marking a session currently running through `ghostex acp-host`.
pub const SESSION_ACP_ACTIVE_KEY: &str = "acpMode";

/// Session `runtimeSettings` key naming the ACP agent family (`antigravity`, future `codex`, ...).
pub const SESSION_ACP_FAMILY_KEY: &str = "acpFamily";

/// Whether Ghostex currently offers an ACP transport for `agent_family`.
pub fn supports_acp(agent_family: &str) -> bool {
    spec::spec_for_family(agent_family).is_some()
}

/// Whether the user has opted `agent_family` into ACP mode in Settings.
pub fn agent_acp_enabled(settings: &serde_json::Map<String, Value>, agent_family: &str) -> bool {
    if !supports_acp(agent_family) {
        return false;
    }
    settings
        .get(ACP_MODE_SETTING_KEY)
        .and_then(Value::as_object)
        .and_then(|map| map.get(agent_family))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Reads `agentAcpMode.<agent_family>` from Ghostex's shared settings file (`native-sidebar-settings.json`).
pub fn agent_acp_enabled_in_shared_settings(agent_family: &str) -> bool {
    if !supports_acp(agent_family) {
        return false;
    }
    let paths = crate::paths::get_gxserver_paths(None);
    crate::shared_settings_file::read_shared_settings(&paths)
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|map| agent_acp_enabled(&map, agent_family))
}

/// True while this session is running in ACP mode (cleared once switched to interactive CLI).
pub fn session_is_acp(session: &Value) -> bool {
    session
        .pointer("/runtimeSettings/acpMode")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The ACP family for a session currently running in ACP mode.
pub fn session_acp_family(session: &Value) -> Option<String> {
    if !session_is_acp(session) {
        return None;
    }
    session
        .pointer("/runtimeSettings/acpFamily")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(session))
}
