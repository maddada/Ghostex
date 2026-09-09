use crate::*;

impl GhostexGpuiApp {
    /// CDXC:SessionChat 2026-09-06 DECISION:
    /// User: a focused chat pane keeps the composer's focus outline and a steady caret even when the input itself is blurred.
    /// Share the native pane-focus rules with the page independently of the optional pane outline and attention colors.
    pub(crate) fn sync_session_chat_pane_focus(
        &self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
        force: bool,
    ) {
        #[cfg(target_os = "macos")]
        if let Some(reveal) = self.companion_reveal.as_ref()
            && cef_parent_native_view(window).ok() != Some(reveal.native_view)
        {
            return;
        }
        for (session_id, surface) in &self.agents_chat_surfaces {
            let focused = window.is_window_active()
                && self.agents_chat_mode_sessions.contains(session_id)
                && if self.active_mode == TitlebarMode::Agents {
                    self.agents_workspace
                        .pane_id_for_session(*session_id)
                        .and_then(|pane_id| self.agents_workspace.find_leaf(pane_id))
                        .is_some_and(|leaf| {
                            leaf.tab_group.active_session_id() == Some(*session_id)
                                && (self.sidebar_focus_border_handoff_holds_pane(leaf.pane_id)
                                    || self
                                        .should_show_focused_agents_leaf_border(leaf, window, cx))
                        })
                } else {
                    self.active_mode.is_project_editor_mode()
                        && self.project_editor_companion_is_visible()
                        && self.project_editor_companion_focused_terminal_session_id()
                            == Some(*session_id)
                        && self.project_editor_companion_border_state(self.active_mode, window)
                            == WorkspacePaneBorderState::Focused
                };
            surface.update(cx, |surface, _| {
                surface.set_session_chat_pane_focused(focused, force);
            });
        }
    }
}
