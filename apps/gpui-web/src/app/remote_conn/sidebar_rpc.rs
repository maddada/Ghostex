//! The browser's answer to the desktop's remote request door (`apps/desktop/src/app/remote_conn/sidebar_rpc.rs`): the same names, and a refusal, because this build has no tunnels.
use std::time::Duration;

use crate::GhostexGpuiApp;

/// The sentence a refused remote request answers with here.
pub(crate) const WEB_REMOTE_MACHINES_UNAVAILABLE: &str =
    "Remote machines are not available in the browser.";

/// Who reads the answer, as on the desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // matched by the shared executor files
pub(crate) enum GpuiRemoteSidebarRpcMode {
    Awaited,
    FireAndForget,
}

impl GhostexGpuiApp {
    /// Refused: the page has no tunnel to the machine. A fire-and-forget caller gets the toast the desktop gives a machine that is not connected.
    pub(crate) fn start_gpui_remote_sidebar_rpc(
        &mut self,
        _remote_machine_id: &str,
        _path: &str,
        _params: Option<serde_json::Value>,
        _timeout: Duration,
        mode: GpuiRemoteSidebarRpcMode,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<serde_json::Value, String>> {
        if mode == GpuiRemoteSidebarRpcMode::FireAndForget {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Remote action unavailable",
                WEB_REMOTE_MACHINES_UNAVAILABLE,
                cx,
            );
        }
        gpui::Task::ready(Err(WEB_REMOTE_MACHINES_UNAVAILABLE.to_string()))
    }

    /// The machine tab's connect icon. The web draws only this computer's tab, whose icon never shows, so nothing reaches this.
    pub(crate) fn remote_reconnect_from_sidebar(
        &mut self,
        _machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Remote machine unavailable",
            WEB_REMOTE_MACHINES_UNAVAILABLE,
            cx,
        );
    }
}
