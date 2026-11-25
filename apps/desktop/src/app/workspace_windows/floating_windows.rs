//! Closing a workspace window's floating popups when the window is resized.

use gpui::{Context, Window};

use crate::*;

impl GhostexGpuiApp {
    /// Closes the floating, transient windows drawn over this workspace window. Runs from its
    /// bounds observer when its size changes (a drag of an edge, maximize, restore, full screen);
    /// the titlebar dropdowns and panels close there on any frame change.
    ///
    /// CDXC:AppModal 2026-10-08 DECISION:
    /// User: "also please when we resize the screen always close all windows that are floating like for example this one in the files sidepanel". A resize closes Quick Access, the agent picker, the sidebar's menu, the floating files list (drawer or peek), the floating sessions panel, an extension's titlebar popup, the agent action bar's menu and the tooltip, with the titlebar dropdowns, Resources and Tips that already closed on a frame change. The dialogs that hold the user's input (Settings, Git commit, rename, notes and the other app modals) stay open, and so do the chat's own windows, which follow their pane (CDXC:SessionChat 2026-09-24 in native_chat/child_window.rs). A move alone closes none of these: the windows a workspace window owns move with it (CDXC:AppModal 2026-10-04 in owned_windows.rs).
    pub(super) fn close_floating_windows_on_resize(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_gpui_quick_access_window(cx);
        self.dismiss_native_sidebar_menu(cx);
        self.native_docs_close_transient(cx);
        self.close_floating_reveal(cx);
        if self.titlebar_extension_popup.is_some() {
            self.close_titlebar_extension_popup(window, cx);
        }
        self.close_terminal_agent_action_bar_menu(cx);
        crate::app::window::frosted_host::hide_frosted_host(
            crate::app::window::frosted_host::FrostedHostKind::Tooltip,
            cx,
        );
    }
}
