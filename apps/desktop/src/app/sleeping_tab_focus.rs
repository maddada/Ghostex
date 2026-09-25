use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:SessionSleep 2026-09-23 DECISION:
    /// User (reviewing PR 152, choosing the narrow version): a sleeping session wakes when you ask for it, and only then. Clicking its sidebar row and Advanced > Split Right still wake it; opening its project, and the first visit to a project after a restart, select it and show the "Press Any Key to Wake" placeholder instead, with Click to Wake Sleeping Panes on.
    /// Those indirect selections post a focus with `keep_sleeping`, and an already-mapped sleeping tab is then selected exactly like a tab-strip click, so `select_agents_tab` applies Click to Wake Sleeping Panes as it does there. A session with no tab yet still wakes, because attaching it is what starts its provider.
    /// SEE-ALSO: `resume_restored_workspace_surfaced_terminals` in apps/desktop/src/app/workspace_terminals.rs.
    pub(crate) fn select_sleeping_local_workspace_tab(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
        keep_view: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !gpui_click_to_wake_sleeping_sessions_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        ) {
            return false;
        }
        self.prune_local_workspace_session_mappings();
        let Some(shell_session_id) = self.local_workspace_session_mappings.get(key).copied() else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return false;
        };
        if !self.agents_terminal_session_is_mapped_sleeping(shell_session_id) {
            return false;
        }
        if keep_view {
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.finish_local_workspace_terminal_background_selection(
                key,
                pane_id,
                shell_session_id,
                cx,
            );
        } else {
            let pane_id = self.pull_workspace_session_into_focused_pane(pane_id, shell_session_id);
            self.local_app_shot_session_mappings
                .insert(key.session_id.clone(), shell_session_id);
            self.select_agents_tab(pane_id, shell_session_id, cx);
            self.set_sidebar_focus_border_handoff_target(shell_session_id);
            cx.notify();
        }
        true
    }
}
