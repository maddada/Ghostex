use serde_json::Value;

use crate::*;

impl GhostexGpuiApp {
    /// The armed Delayed Send / Close After Done labels for a chat's session, from the native sidebar clock.
    /// CDXC:SessionChat 2026-09-19 SEE-ALSO: packages/gx-core/src/sidebar_view/armed_actions.rs builds the labels; apps/desktop/src/app/gx_store/sidebar_clock.rs rebuilds them every second for all sessions, because the sidebar list omits rows hidden by machine, space, or tag filters.
    pub(crate) fn session_chat_armed_actions(&self, session_id: TerminalSessionId) -> Value {
        let sidebar_session_id = match self.workspace_terminal_key_for_shell_session(session_id) {
            Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                gpui_combined_presentation_session_id(&key.project_id, &key.session_id)
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => gpui_remote_scoped_session_id(
                &key.remote_machine_id,
                &key.project_id,
                &key.session_id,
            ),
            None => return Value::Array(Vec::new()),
        };
        self.native_sidebar
            .armed_actions
            .get(&sidebar_session_id)
            .cloned()
            .unwrap_or_else(|| Value::Array(Vec::new()))
    }

    /// Hand changed labels to every open chat view.
    pub(crate) fn sync_session_chat_armed_actions(&mut self, cx: &mut gpui::Context<Self>) {
        let views = self
            .native_chat_views
            .iter()
            .map(|(session_id, view)| (*session_id, view.clone()))
            .collect::<Vec<_>>();
        for (session_id, view) in views {
            let actions = self.session_chat_armed_actions(session_id);
            view.update(cx, |view, cx| {
                if view.armed_actions != actions {
                    view.armed_actions = actions;
                    cx.notify();
                }
            });
        }
    }
}
