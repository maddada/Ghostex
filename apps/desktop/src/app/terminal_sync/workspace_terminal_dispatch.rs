// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: workspace terminal event dispatch (bell, title change, escape, first-prompt-title cancel, attention acknowledge) and the native-view prompt-editor shortcut.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// Terminal BEL follows macOS ownership: Rust forwards only the bounded
    /// gxserver project/session identity of the rung Agents terminal; the
    /// sidebar runtime gates on `showNotificationOnTerminalBell` and commits
    /// the gxserver attention transition.
    pub(crate) fn dispatch_gpui_workspace_terminal_bell(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_bell_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// The Windows GPUI terminal engine observes the same OSC 0/2 title stream
    /// as the native macOS Ghostty surface. Forward the bounded raw observation
    /// to the sidebar runtime so gxserver remains the single owner of title
    /// trust, agent metadata reconciliation, persistence, and presentation.
    /// The sidebar settles bursts before calling `/api/ingestTerminalTitleEvent`.
    #[cfg(target_os = "windows")]
    pub(crate) fn dispatch_gpui_workspace_terminal_title_changed(
        &mut self,
        shell_session_id: TerminalSessionId,
        raw_title: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if raw_title.is_empty()
            || raw_title.chars().count() > GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS
            || raw_title.contains('\0')
        {
            return;
        }
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "rawTitle": raw_title,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_title_changed_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// ESC follows the terminal input path first; Rust forwards only the
    /// bounded gxserver project/session identity so the sidebar runtime can
    /// apply escape suppression and sync gxserver for
    /// `ghostex.gpui.sidebar.workspaceTerminalEscapePressed`.
    pub(crate) fn dispatch_gpui_workspace_terminal_escape_pressed(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionInterrupt.escapeTargetResolved",
            serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
                "shellSessionId": format!("{:?}", shell_session_id),
            }),
        );
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_escape_pressed_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// Escape inside the blocking "Generating title" overlay cancels the
    /// gxserver first-prompt title job. Rust only reports the bounded
    /// project/session identity; the sidebar runtime owns the cancel decision
    /// and the `/api/cancelFirstPromptAutoTitle` call for
    /// `ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel`.
    pub(crate) fn dispatch_gpui_workspace_first_prompt_title_generation_cancel(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION,
        });
        let script = gpui_workspace_first_prompt_title_generation_cancel_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// Rust reports only the mapped gxserver identity for direct workspace
    /// interaction; the sidebar runtime owns the actual attention decision and
    /// gxserver acknowledgement for
    /// `ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge`.
    pub(crate) fn dispatch_gpui_workspace_session_attention_acknowledge(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION,
        });
        let script = gpui_workspace_session_attention_acknowledge_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_workspace_terminal_escape_pressed_for_native_view(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let companion_session_id =
            self.project_editor_companion_terminal_session_id_containing_responder(native_view);
        let Some(shell_session_id) = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or(companion_session_id)
        else {
            return;
        };
        // Temporary input-stealing diagnosis (2026-07-09): correlate terminal
        // Escape dispatches with first-responder churn in the same log.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.terminalEscapeDispatched",
            serde_json::json!({ "shellSessionId": format!("{:?}", shell_session_id) }),
        );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionInterrupt.nativeEscapeRouted",
            serde_json::json!({ "shellSessionId": format!("{:?}", shell_session_id) }),
        );
        self.dispatch_gpui_workspace_terminal_escape_pressed(shell_session_id, cx);
        // Escape is terminal input, not a companion-focus exit. Reassert the
        // exact mounted companion host after the sidebar attention sideband so
        // AppKit keeps subsequent keys on the same terminal surface.
        if companion_session_id == Some(shell_session_id)
            && matches!(
                self.shell_focus,
                ShellFocusTarget::ProjectEditorCompanion(mode) if mode == self.active_mode
            )
        {
            self.begin_programmatic_focus();
            self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
                true,
            );
            self.end_programmatic_focus();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_native_terminal_prompt_editor_shortcut(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_shell_session_id = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or_else(|| {
                self.project_editor_companion_terminal_session_id_containing_responder(native_view)
            });
        let remote_context = remote_shell_session_id.and_then(|shell_session_id| {
            self.remote_prompt_editor_context_for_shell_session(shell_session_id)
                .map(|(key, connection_generation)| (shell_session_id, key, connection_generation))
        });
        if let Some((shell_session_id, key, connection_generation)) = remote_context {
            let native_view = native_view as usize;
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        shell_session_id,
                        &key,
                        connection_generation,
                        RemotePromptEditorDeliveryTarget::NativeView(native_view),
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        let Some(originating_session_id) =
            self.prompt_editor_originating_session_id_for_native_view(native_view)
        else {
            let _ =
                terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(native_view);
            return;
        };
        let native_view = native_view as usize;
        cx.spawn(async move |this, cx| {
            let fronted = cx
                .background_executor()
                .spawn(
                    async move { gpui_ghostex_editor_daemon_front(Some(&originating_session_id)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if fronted {
                    if !this.prompt_editor_daemon_open {
                        this.prompt_editor_daemon_open = true;
                        cx.notify();
                    }
                } else {
                    let _ = terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(
                        native_view as *mut std::ffi::c_void,
                    );
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn prompt_editor_originating_session_id_for_native_view(
        &self,
        native_view: *mut std::ffi::c_void,
    ) -> Option<String> {
        if let Some(shell_session_id) = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or_else(|| {
                self.project_editor_companion_terminal_session_id_containing_responder(native_view)
            })
        {
            let key = self.local_workspace_session_mappings.iter().find_map(
                |(key, mapped_session_id)| (*mapped_session_id == shell_session_id).then_some(key),
            )?;
            return Some(format!("{}:{}", key.project_id, key.session_id));
        }
        let command_session_id =
            self.command_terminal_session_id_containing_responder(native_view)?;
        let key = self
            .command_gxserver_session_mappings
            .get(&command_session_id)?;
        Some(format!("{}:{}", key.project_id, key.session_id))
    }
}
