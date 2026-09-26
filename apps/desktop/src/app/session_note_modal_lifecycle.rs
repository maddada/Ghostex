//! Open, close, and bridge plumbing for the native Session Note dialog.
//! SEE-ALSO: apps/desktop/src/app/window/session_note_modal.rs (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the native dialog for the `open` message of the `sessionNote`
    /// modal kind: `sessionId` (required), `initialNote`, `projectId`, `sessionTitle`.
    /// A note open without a session id is dropped, as in the React host: the
    /// write would have no target.
    pub(crate) fn open_gpui_session_note_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .filter(|value| !value.trim().is_empty())
                .map(str::to_string)
        };
        let Some(session_id) = text("sessionId") else {
            return;
        };
        let project_id = text("projectId");
        let config = SessionNoteModalConfig {
            initial_note: message
                .get("initialNote")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            session_title: text("sessionTitle"),
            palette: self.gpui_native_modal_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            app.handle_gpui_session_note_modal_command(
                &session_id,
                project_id.as_deref(),
                command,
                cx,
            );
        });
        self.open_native_app_modal(
            GpuiAppModalKind::SessionNote,
            SESSION_NOTE_MODAL_WIDTH,
            SESSION_NOTE_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiSessionNoteModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// Hands the same `setSessionNote` command the React page posted to
    /// the Rust store (gx_store/terminal_lifecycle/session_edits.rs, which owns the
    /// gxserver call and the local/remote routing), with the checks the app-modal bridge applies,
    /// then releases the window the dialog already removed.
    fn handle_gpui_session_note_modal_command(
        &mut self,
        session_id: &str,
        project_id: Option<&str>,
        command: SessionNoteModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::SessionNote;
        if let SessionNoteModalCommand::Save { note } = command {
            let session_id = session_id.trim();
            if gpui_app_modal_sidebar_session_id_allowed(session_id) {
                let mut message = serde_json::json!({
                    "note": note,
                    "sessionId": session_id,
                    "type": "setSessionNote",
                });
                if let Some(project_id) = project_id
                    .map(str::trim)
                    .filter(|project_id| gpui_remote_sidebar_project_id_allowed(project_id))
                {
                    message["projectId"] = serde_json::json!(project_id);
                }
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
        }
        self.release_native_app_modal_window(kind, cx);
    }
}
