//! The agent the sidebar's launchers and the New Thread picker treat as the default: read from
//! client storage at start, and written whenever an agent is launched.
//!
//! CDXC:AgentLauncher 2026-09-09 WHY:
//! A launch from the New Thread picker or a sidebar launcher must also become the highlighted
//! default agent. The key is the historic `ghostex-sidebar-project-terminal-launcher` so existing
//! choices carry forward.
//!
//! CDXC:AgentLauncher 2026-09-25 WHY:
//! The QuickJS app runtime wrote this key on every `runSidebarAgent` host message and posted
//! `primaryAgentLauncherChanged` back to Rust, which is how Rust's own field learned it. Rust sends
//! that host message itself, so it writes the key and its field at the same moment, and the runtime's
//! writer, its start-up post and the Rust receiver are gone.
//! SEE-ALSO: packages/client-storage/catalog.ts (the `launcher` store).

use super::sidebar_ui_storage::read_preference_value;

const PRIMARY_AGENT_LAUNCHER_KEY: &str = "ghostex-sidebar-project-terminal-launcher";

/// `readPrimaryAgentLauncherId`: the stored agent, trimmed, `None` when empty or unreadable.
pub(crate) fn read_primary_agent_launcher_id() -> Option<String> {
    let stored = read_preference_value(PRIMARY_AGENT_LAUNCHER_KEY).ok()??;
    let agent_id = stored.trim();
    (!agent_id.is_empty() && agent_id.len() <= 128).then(|| agent_id.to_string())
}
