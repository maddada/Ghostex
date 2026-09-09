use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn request_focused_session_model_picker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return self.open_terminal_model_picker(session_id, cx);
        }
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return false;
        };
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(
                "document.documentElement.dataset.ghostexModelPickerRequested = 'true'; window.dispatchEvent(new CustomEvent('ghostex-open-model-picker')); undefined;",
            );
        });
        true
    }

    /// CDXC:Hotkeys 2026-09-08 DECISION:
    /// User: offer the quick model and effort picker for Claude and Codex in terminal view, with a setting to turn it off (enabled by default).
    /// This supersedes the chat-only shortcut rule; disabled or unsupported terminals keep their own bindings.
    /// CDXC:Hotkeys 2026-09-09 DECISION: User: extend the quick picker to Cursor, Grok Build and Antigravity.
    pub(crate) fn terminal_model_picker_session(&self) -> Option<TerminalSessionId> {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        if !settings
            .object()
            .get("showQuickModelPickerInTerminal")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true)
        {
            return None;
        }
        let session_id = self.focused_agents_or_companion_shell_session_id()?;
        if self.agents_chat_mode_sessions.contains(&session_id)
            || !matches!(
                self.agents_session_chat_transcript_agent(session_id),
                Some("claude" | "codex" | "cursor" | "grok" | "antigravity")
            )
        {
            return None;
        }
        self.workspace_terminal_key_for_shell_session(session_id)?;
        Some(session_id)
    }

    fn open_terminal_model_picker(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.terminal_model_picker_session() != Some(session_id) {
            return false;
        }
        let Some(bootstrap) = self.agents_session_chat_gxserver_bootstrap(session_id) else {
            return false;
        };
        let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) else {
            return false;
        };
        let provider = self.agents_session_chat_transcript_agent(session_id);
        let (project_id, session_id) = match key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => (key.project_id, key.session_id),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => (key.project_id, key.session_id),
        };
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let modal = GpuiAppModalKind::ModelPicker;
        let message = serde_json::json!({
            "type": "open", "modal": modal.modal_id(),
            "projectId": project_id, "sessionId": session_id, "provider": provider,
            "hotkeys": settings.object().get("hotkeys"),
            "connection": { "baseUrl": bootstrap.base_url, "authToken": bootstrap.auth_token,
                "protocolVersion": bootstrap.protocol_version },
        });
        self.open_gpui_app_modal_window(modal, message, serde_json::Value::Null, None, cx);
        true
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_terminal_model_picker_keyboard_scope(&self) {
        let session = self.terminal_model_picker_session();
        GPUI_KEYBOARD_ROUTER_TARGETS.with(|targets| {
            if let Some(target) = targets
                .borrow_mut()
                .get_mut(&(self.parent_ns_view as usize))
            {
                target.terminal_model_picker_session = session;
            }
        });
    }
}
