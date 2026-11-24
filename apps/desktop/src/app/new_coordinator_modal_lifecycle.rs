//! Coordinator draft entry point and Make Coordinator dialog lifecycle.
//!
//! New coordinators open straight into the Native chat draft view with Coordinator mode toggled on
//! (no modal or agent picker), and name themselves from their first prompt. Existing sessions can
//! still be promoted to coordinators via `open_gpui_make_coordinator_modal`.

use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Routes any `newCoordinator` modal open message straight into a Native draft with
    /// Coordinator mode toggled on in the requested project group.
    pub(crate) fn open_gpui_new_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let group_id = message
            .get("groupId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        self.start_new_coordinator_draft(group_id, cx);
    }

    /// Opens the Make Coordinator dialog for the `makeCoordinator` modal: `sessionId` (the sidebar
    /// row's id) and `sessionTitle`. An open without a session is dropped.
    pub(crate) fn open_gpui_make_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(sidebar_session_id) = text("sessionId") else {
            return;
        };
        let session_title = text("sessionTitle").unwrap_or_else(|| "This session".to_string());
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let MakeCoordinatorModalCommand::Make { goal } = command {
                app.gx_store_promote_coordinator(&sidebar_session_id, &goal, cx);
            }
            app.release_native_app_modal_window(GpuiAppModalKind::MakeCoordinator, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::MakeCoordinator,
            NEW_COORDINATOR_MODAL_WIDTH,
            MAKE_COORDINATOR_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiMakeCoordinatorModalWindow::new(session_title, palette, host, window, cx)
                })
            },
            cx,
        );
    }
}
