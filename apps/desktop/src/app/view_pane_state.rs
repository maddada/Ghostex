use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn project_editor_companion_is_visible(&self) -> bool {
        #[cfg(target_os = "macos")]
        if self.companion_reveal.as_ref().is_some_and(|reveal| {
            reveal.mode == self.active_mode && reveal.project_id == self.agents_workspace_project_id
        }) {
            return true;
        }
        self.project_editor_shell.left_companion_visible
    }

    pub(crate) fn saved_view_pane_state(&self, mode: TitlebarMode) -> GpuiViewPaneState {
        self.agents_workspace_project_id
            .as_ref()
            .and_then(|id| self.project_view_states_by_project.get(id))
            .and_then(|state| state.panes_by_mode.get(&mode.element_slug()))
            .copied()
            .unwrap_or_else(|| GpuiViewPaneState::default_for_mode(mode))
    }

    /// All mode entry routes capture the outgoing layout before restoring the incoming one.
    pub(crate) fn change_active_mode_with_pane_state(
        &mut self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) {
        if mode == self.active_mode {
            return;
        }
        #[cfg(target_os = "macos")]
        self.close_floating_companion(cx);
        self.capture_outgoing_project_view_state();
        self.active_mode = mode;
        self.apply_view_pane_state(cx);
    }

    pub(crate) fn apply_view_pane_state(&mut self, cx: &mut gpui::Context<Self>) {
        let panes = self.saved_view_pane_state(self.active_mode);
        self.sidebar_collapsed = panes.sidebar_collapsed;
        self.project_editor_shell.left_companion_visible = panes.companion_visible;
        self.cancel_sidebar_divider_interaction_state();
        self.project_editor_companion_drag = None;
        self.clear_project_editor_companion_divider_hover_state();
        self.update_sidebar_cef_surface_visibility(cx);
        self.apply_command_view_pane_state();
    }

    pub(crate) fn apply_command_view_pane_state(&mut self) {
        // Project selection swaps Agents and Commands independently. Restore Commands
        // only after its model belongs to the incoming project.
        if self.command_pane_project_id != self.agents_workspace_project_id {
            return;
        }
        let panes = self.saved_view_pane_state(self.active_mode);
        self.command_pane.mode = panes.command_mode;
        self.command_pane.last_expanded_mode = panes.command_last_expanded_mode;
        self.command_pane.resize_drag = None;
        self.clear_command_resize_hover_state();
    }
}
