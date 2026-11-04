//! Close for a Resources panel session row, whether or not a pane owns it.

use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn close_gpui_titlebar_resource_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if let Some(shell_session_id) = self.gpui_titlebar_resource_shell_session_id(session_id)
            && let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id)
        {
            return self.close_agents_tab(pane_id, shell_session_id, "runningSessions", cx);
        }
        if let Some(key) = gpui_combined_presentation_session_key(session_id) {
            // The door for gxserver's close log, taken by the close it starts.
            self.gx_store.close_source = Some("runningSessions");
            let closed =
                self.dispatch_gpui_workspace_session_key_runtime_action("closeSession", &key, cx);
            self.gx_store.close_source = None;
            return closed;
        }
        false
    }
}
