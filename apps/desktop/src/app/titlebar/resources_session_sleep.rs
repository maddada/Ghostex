//! Sleep for a Resources panel session row, whether or not a pane owns it.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// A session row's moon and the section's Sleep Project. A session with a
    /// mounted pane sleeps through the pane-owned path so the tab, focus, and
    /// replacement logic stay local; a session the panel listed from the
    /// sidebar inventory without a pane is addressed by gxserver identity and
    /// slept by the sidebar runtime, which owns the daemon lifecycle.
    pub(crate) fn sleep_gpui_titlebar_resource_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(shell_session_id) = self.gpui_titlebar_resource_shell_session_id(session_id) {
            if let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) {
                self.sleep_agents_tabs_for_scope(
                    pane_id,
                    shell_session_id,
                    AgentsWorkspaceTabSleepScope::Sleep,
                    cx,
                );
                return;
            }
        }
        if let Some(key) = gpui_combined_presentation_session_key(session_id) {
            let _ =
                self.dispatch_gpui_workspace_session_key_runtime_action("sleepSession", &key, cx);
        }
    }
}
