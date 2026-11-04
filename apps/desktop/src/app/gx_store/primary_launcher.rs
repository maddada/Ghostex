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
//! SEE-ALSO: packages/client-storage/catalog.ts (the `launcher` and `launcherChoices` stores),
//! packages/gx-core/src/sidebar_menu/agent_choice.rs (which agent a project starts).

use ghostex_gx_core::{
    AGENT_CHOICE_SCOPE_SETTING_KEY, AGENT_CHOICES_STORAGE_KEY, AgentChoicePlace, AgentChoiceScope,
    AgentChoices, ProjectKey,
};

use super::sidebar_ui_storage::{read_preference_value, write_client_document_value};
use crate::GhostexGpuiApp;

const PRIMARY_AGENT_LAUNCHER_KEY: &str = "ghostex-sidebar-project-terminal-launcher";

/// `readPrimaryAgentLauncherId`: the stored agent, trimmed, `None` when empty or unreadable.
pub(crate) fn read_primary_agent_launcher_id() -> Option<String> {
    let stored = read_preference_value(PRIMARY_AGENT_LAUNCHER_KEY).ok()??;
    let agent_id = stored.trim();
    (!agent_id.is_empty() && agent_id.len() <= 128).then(|| agent_id.to_string())
}

/// The agents the user picked, last anywhere, per project and per Space, with Settings > Agents
/// "Remember the picked agent" (CDXC:AgentLauncher 2026-10-30 in gx-core's `agent_choice.rs`).
pub(crate) fn read_agent_choices() -> AgentChoices {
    try_read_agent_choices()
        .unwrap_or_else(|_| AgentChoices::from_storage(read_agent_choice_scope(), None, None))
}

/// [`read_agent_choices`], with a storage read failure reported rather than read as no choices.
pub(crate) fn try_read_agent_choices() -> Result<AgentChoices, &'static str> {
    let last = read_preference_value(PRIMARY_AGENT_LAUNCHER_KEY)?;
    let document = read_preference_value(AGENT_CHOICES_STORAGE_KEY)?;
    Ok(AgentChoices::from_storage(
        read_agent_choice_scope(),
        last.as_deref(),
        document.as_deref(),
    ))
}

fn read_agent_choice_scope() -> AgentChoiceScope {
    AgentChoiceScope::from_setting(
        crate::shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get(AGENT_CHOICE_SCOPE_SETTING_KEY)
            .and_then(serde_json::Value::as_str),
    )
}

/// Stores the choices: the last pick under its historic key, the rest as one document.
pub(crate) fn write_agent_choices(choices: &AgentChoices) {
    let _ = write_client_document_value(PRIMARY_AGENT_LAUNCHER_KEY, choices.last.as_deref());
    let _ = write_client_document_value(AGENT_CHOICES_STORAGE_KEY, Some(&choices.document_json()));
}

impl GhostexGpuiApp {
    /// Where a launch from `group_id` is remembered, or from the active project when it names no
    /// group (New Agent Session, the New Thread picker, the command palette).
    pub(crate) fn gx_store_agent_choice_place(&self, group_id: Option<&str>) -> AgentChoicePlace {
        let view = self.gx_store.sidebar_list.view();
        match group_id {
            Some(group_id) => AgentChoicePlace::for_group_id(view, group_id),
            None => self
                .gx_store
                .core
                .focus()
                .active_project
                .as_ref()
                .map(|project| AgentChoicePlace::for_project(view, project))
                .unwrap_or_default(),
        }
    }

    /// Where a launch in `project` is remembered.
    pub(crate) fn gx_store_project_agent_choice_place(
        &self,
        project: &ProjectKey,
    ) -> AgentChoicePlace {
        AgentChoicePlace::for_project(self.gx_store.sidebar_list.view(), project)
    }
}
