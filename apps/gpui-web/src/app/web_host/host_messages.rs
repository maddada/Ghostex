//! The sidebar host messages the app's dialogs send back (rename, note, Delayed Send edits, a worktree dialog's confirm): the desktop's `dispatch_gpui_sidebar_host_message`, answered by the same store handlers.
use serde_json::{Map, Value};

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn dispatch_gpui_sidebar_host_message(
        &mut self,
        message: Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.gx_store_claim_sidebar_host_message(&message, cx) {
            return true;
        }
        self.gx_store_run_session_edit(&message, cx)
    }

    /// The Rename dialog's submit. The desktop also renames its local command tabs here; the page has none, so every id is a sidebar session.
    pub(crate) fn handle_gpui_rename_command_session_command(
        &mut self,
        command: &Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_sidebar_host_message(Value::Object(command.clone()), cx);
    }
}
