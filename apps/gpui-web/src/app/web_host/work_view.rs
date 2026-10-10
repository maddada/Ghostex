//! The Work view in the browser: the page is a CEF page the desktop app hosts, so this build
//! shows the sidebar's briefcase but answers it with a toast, and a session card's chip opens its
//! link (apps/desktop/src/app/work_view/ is the desktop's answer).

use gpui::{Context, Window};
use serde_json::Value;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn work_view_showing(&self) -> bool {
        false
    }

    pub(crate) fn toggle_work_view(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Work opens in the Ghostex app",
            "The Work page (tickets, issues and PRs) is part of the desktop app.",
            cx,
        );
    }

    /// The chip's link opens instead.
    pub(crate) fn open_work_item(&mut self, _item: Value, _cx: &mut Context<Self>) -> bool {
        false
    }

    /// There is no Work page here to refresh after the Create Linear Ticket dialog.
    pub(crate) fn work_view_ticket_created(&mut self, _cx: &mut Context<Self>) {}

    /// There is no Work page here to tell which session is current.
    pub(crate) fn work_view_sync_current_session(&mut self, _cx: &mut Context<Self>) {}
}
