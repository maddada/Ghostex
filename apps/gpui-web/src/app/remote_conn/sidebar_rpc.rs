//! The browser's answer to the desktop's remote request door (`apps/desktop/src/app/remote_conn/sidebar_rpc.rs`): the same names, and a refusal, because this build has no tunnels.
use std::time::Duration;

use crate::GhostexGpuiApp;

/// The sentence a refused remote request answers with here.
pub(crate) const WEB_REMOTE_MACHINES_UNAVAILABLE: &str =
    "Remote machines are not available in the browser.";

/// The desktop's failure sentence for a remote request, which the account pages answer with when a group names no reachable machine.
pub(crate) const GPUI_REMOTE_GXSERVER_REQUEST_FAILED: &str = "Remote gxserver request failed.";
pub(crate) const GPUI_REMOTE_HOOKS_UPDATE_REQUIRED: &str = "Remote Ghostex needs an update.";

/// What a waited caller's failure toast says, as on the desktop: here always the refusal.
pub(crate) fn gpui_remote_sidebar_rpc_failure_reason(error: &str, _fallback: &str) -> String {
    error.to_string()
}

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

/// The desktop's machine-tab update mark (`apps/desktop/src/app/remote_conn/app_update.rs`); the web has no remote machines.
pub(crate) struct RemoteAppUpdateBadge {
    pub(crate) busy: bool,
    pub(crate) tooltip: String,
    pub(crate) menu_label: Option<String>,
}

impl GhostexGpuiApp {
    pub(crate) fn remote_app_update_tab_badge(
        &self,
        _machine_id: &str,
    ) -> Option<RemoteAppUpdateBadge> {
        None
    }

    /// The desktop's remote computers' systems (`apps/desktop/src/app/remote_conn/app_update.rs`);
    /// the web has no remote connections, so its computers keep the generic remote icon.
    pub(crate) fn remote_machine_systems(&self) -> std::collections::HashMap<String, &'static str> {
        std::collections::HashMap::new()
    }

    /// The desktop's computer marks (`apps/desktop/src/app/gx_store/computer_icons.rs`): drawn
    /// only while more than one computer is in the sidebar, which a page never has.
    pub(crate) fn sidebar_computer_mark(&self, _session_id: &str) -> Option<(&'static str, &str)> {
        None
    }

    pub(crate) fn sidebar_machine_icon_asset(&self, _machine_id: &str) -> Option<&'static str> {
        None
    }
}
