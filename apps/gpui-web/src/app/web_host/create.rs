//! The page's answers to what the desktop's create files (`gx_store/create/`) hand to its native side: a browser open becomes a new browser tab of the page's own browser, a Windows or onboarding create path the page never takes is answered honestly, and the agent launch placeholder (a desktop pane tab) is not drawn.
use gpui::{Context, Window};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::model::{
    GpuiSidebarOpenBrowserUrlMessage, GpuiSidebarWorkspaceTerminalFocusMessage,
};

impl GhostexGpuiApp {
    /// New Browser Tab and Quick Browser Tab open a Browser pane on the desktop; the page opens the URL in a new tab of the browser it runs in.
    pub(crate) fn open_browser_url_from_renderer_command(
        &mut self,
        message: GpuiSidebarOpenBrowserUrlMessage,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if let Some(window) = web_sys::window() {
            let _ = window.open_with_url_and_target(&message.url, "_blank");
        }
    }

    /// The page switches projects at once (`project_switch_pending_requests` is never filled), so there is nothing to land.
    pub(crate) fn flush_coalesced_project_switch_requests(
        &mut self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The Windows (WSL) project agent create: `cfg!(target_os = "windows")` is false in a page.
    pub(crate) fn receive_sidebar_create_project_agent_payload(
        &mut self,
        _payload: &str,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast("warning", "Agent unavailable", "", cx);
    }

    /// The Windows (WSL) project terminal create: `cfg!(target_os = "windows")` is false in a page.
    pub(crate) fn receive_sidebar_create_project_terminal_payload(
        &mut self,
        _payload: &str,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast("warning", "Terminal unavailable", "", cx);
    }

    /// The first-launch onboarding's result; the page has no onboarding.
    pub(crate) fn dispatch_gpui_first_launch_create_project_session_result(
        &mut self,
        _request_id: &str,
        _ok: bool,
        _error: Option<&str>,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The Project Board's answer goes back to its CEF page; the page has no board.
    pub(crate) fn receive_sidebar_project_board_conversation_response_payload(
        &mut self,
        _payload: &str,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The desktop draws a placeholder tab while an agent starts; the page shows the session once it opens.
    pub(crate) fn adopt_agent_launch_placeholder(
        &mut self,
        _message: &GpuiSidebarWorkspaceTerminalFocusMessage,
        _cx: &mut Context<Self>,
    ) {
    }

    /// The desktop drops its cached menu inputs after a write; the page builds its menu inputs on every update.
    pub(crate) fn gx_store_note_menu_host_write(&mut self, _command: &Value) {}
}

impl GhostexGpuiApp {
    /// A daemon toast (Load Sessions); the page shows it like any other.
    pub(crate) fn show_gpui_gxserver_bootstrap_toast(
        &mut self,
        level: &str,
        title: &str,
        description: &str,
        _persistent: bool,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(level, title, description, cx);
    }

    /// Starting the local gxserver is the app's job; a page is served by a running one and cannot start it.
    pub(crate) fn start_gpui_local_gxserver_bootstrap(
        &mut self,
        _show_loading_toast: bool,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "warning",
            "Start gxserver from the Ghostex app",
            "A browser page cannot start the Ghostex daemon.",
            cx,
        );
    }

    /// The desktop's Resources panel; the page has none.
    pub(crate) fn dispatch_gpui_titlebar_resources_project_state_update(
        &mut self,
        _cx: &mut Context<Self>,
    ) {
    }

    /// A Settings patch (a machine tab's Hide Machine). gxserver has no settings write the page could call, so the change is refused with a toast.
    pub(crate) fn handle_gpui_app_modal_update_settings_patch_message(
        &mut self,
        _message: &Value,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Change this in the Ghostex app",
            "Settings are not saved from the browser.",
            cx,
        );
    }

    /// The desktop's agent launch placeholder tab; the page opens the session when it exists.
    pub(crate) fn stage_agent_launch_placeholder(
        &mut self,
        _command: &Value,
        _cx: &mut Context<Self>,
    ) {
    }
}
