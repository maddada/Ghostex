use crate::*;

impl GhostexGpuiApp {
    /// CDXC:SessionChat 2026-09-06 DECISION:
    /// User: a focused chat pane keeps the composer's focus outline and a steady caret even when the input itself is blurred.
    /// Share the native pane-focus rules with the chat view independently of the optional pane outline and attention colors.
    pub(crate) fn sync_session_chat_pane_focus(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        for (session_id, view) in &self.native_chat_views {
            let focused = window.is_window_active()
                && self.agents_chat_mode_sessions.contains(session_id)
                && self
                    .agents_workspace
                    .pane_id_for_session(*session_id)
                    .and_then(|pane_id| self.agents_workspace.find_leaf(pane_id))
                    .is_some_and(|leaf| {
                        leaf.tab_group.active_session_id() == Some(*session_id)
                            && (self.sidebar_focus_border_handoff_holds_pane(leaf.pane_id)
                                || self.should_show_focused_agents_leaf_border(leaf, window, cx))
                    });
            view.update(cx, |view, cx| {
                if view.pane_focused != focused {
                    view.pane_focused = focused;
                    // Another pane took focus: a short pane's open chat box closes behind it.
                    if !focused && window.is_window_active() {
                        view.short_pane_composer_open = false;
                    }
                    view.sync_suggestion_window(cx);
                    cx.notify();
                }
            });
        }
        // The side chat on screen is focused while the view panel holds the shell focus.
        let side_chat_focused = window.is_window_active()
            && self.shell_focus == ShellFocusTarget::ProjectEditorSurface(TitlebarMode::SideChat);
        let on_screen = self.side_panel_chat_on_screen_shell_session();
        for view in self.side_panel_chat_views() {
            view.update(cx, |view, cx| {
                let focused = side_chat_focused && on_screen == Some(view.config.shell_session_id);
                if view.pane_focused != focused {
                    view.pane_focused = focused;
                    view.sync_suggestion_window(cx);
                    cx.notify();
                }
            });
        }
    }
}
