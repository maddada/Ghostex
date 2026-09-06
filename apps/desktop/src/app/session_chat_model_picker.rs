use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn request_focused_session_chat_model_picker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(
                "document.documentElement.dataset.ghostexModelPickerRequested = 'true'; window.dispatchEvent(new CustomEvent('ghostex-open-model-picker')); undefined;",
            );
        });
    }
}
