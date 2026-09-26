// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: workspace terminal event dispatch (bell, title change, escape, first-prompt-title cancel, attention acknowledge) and the native-view prompt-editor shortcut.

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// Terminal BEL follows macOS ownership: the bell of a mapped Agents terminal becomes gxserver
    /// attention only when the Terminal setting "Show a notification on terminal bell" is on.
    ///
    /// Shells use BEL for routine feedback such as zsh Tab-completion misses, so the bell becomes
    /// gxserver attention only when the user opts in from Terminal settings, the same gate macOS
    /// applies to its terminalBell host event. Agent completion keeps its separate explicit
    /// attention path.
    pub(crate) fn dispatch_gpui_workspace_terminal_bell(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::app::gx_store::terminal_lifecycle::terminal_events::{
            local_workspace_ids_allowed, report_terminal_bell,
        };
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        if !local_workspace_ids_allowed(&key.project_id, &key.session_id)
            || !self.terminal_bell_notifications_enabled()
        {
            return;
        }
        let agent_name = self
            .gx_store_local_server_session(&key.project_id, &key.session_id)
            .and_then(|session| session.agent_name);
        cx.background_executor()
            .spawn(report_terminal_bell(
                key.project_id,
                key.session_id,
                agent_name,
            ))
            .detach();
    }

    /// The Windows GPUI terminal engine observes the same OSC 0/2 title stream as the native macOS
    /// Ghostty surface. gxserver stays the single owner of title trust, agent metadata
    /// reconciliation, persistence and presentation: a burst settles for 1.5 seconds per session
    /// before one `/api/ingestTerminalTitleEvent` call.
    #[cfg(target_os = "windows")]
    pub(crate) fn dispatch_gpui_workspace_terminal_title_changed(
        &mut self,
        shell_session_id: TerminalSessionId,
        raw_title: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::app::gx_store::terminal_lifecycle::terminal_events::{
            TERMINAL_TITLE_SETTLE_MS, ingest_terminal_title, local_workspace_ids_allowed,
            terminal_title_ingest_params,
        };
        if raw_title.is_empty()
            || raw_title.chars().count() > GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS
            || raw_title.chars().any(|ch| ch.is_control())
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
        if !local_workspace_ids_allowed(&key.project_id, &key.session_id) {
            return;
        }
        let generation = self.gx_store.terminal_title_settle.observe(
            &key.project_id,
            &key.session_id,
            raw_title,
        );
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(TERMINAL_TITLE_SETTLE_MS))
                .await;
            let params = this
                .update(cx, |this, _| {
                    let raw_title = this.gx_store.terminal_title_settle.take_settled(
                        &key.project_id,
                        &key.session_id,
                        generation,
                    )?;
                    let session =
                        this.gx_store_local_server_session(&key.project_id, &key.session_id)?;
                    terminal_title_ingest_params(&session, &raw_title, |title| {
                        crate::terminal_osc_title::visible_terminal_osc_title(title)
                    })
                })
                .ok()
                .flatten();
            if let Some(params) = params {
                ingest_terminal_title(params).await;
            }
        })
        .detach();
    }

    /// ESC follows the terminal input path first; the store then suppresses
    /// the session's completion sound, clears its attention and tells gxserver
    /// (gx_store/attention/).
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
        self.gx_store_terminal_escape(
            ghostex_gx_core::SessionKey::local(key.project_id, key.session_id),
            cx,
        );
    }

    /// Escape inside the blocking "Generating title" overlay cancels the gxserver first-prompt title
    /// job. The overlay and the terminal input suppression lift at once, before the call, instead
    /// of waiting for the next gxserver delta.
    pub(crate) fn dispatch_gpui_workspace_first_prompt_title_generation_cancel(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::app::gx_store::terminal_lifecycle::terminal_events::{
            cancel_first_prompt_title, local_workspace_ids_allowed,
        };
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        if !local_workspace_ids_allowed(&key.project_id, &key.session_id) {
            return;
        }
        let generating = self
            .gx_store_local_server_session(&key.project_id, &key.session_id)
            .is_some_and(|session| session.is_generating_first_prompt_title);
        if !generating {
            return;
        }
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == shell_session_id)
            && session.is_generating_first_prompt_title
        {
            session.is_generating_first_prompt_title = false;
            self.sync_gpui_engine_first_prompt_input_suppression(cx);
            cx.notify();
        }
        cx.background_executor()
            .spawn(cancel_first_prompt_title(key.project_id, key.session_id))
            .detach();
    }

    /// Direct workspace interaction acknowledges the mapped session's attention;
    /// the store decides when (gx_store/attention/).
    ///
    /// CDXC:FocusRouting 2026-09-19 WHY:
    /// A held "next tab" key evaluated one script in the sidebar runtime per tab it passed. The report now rides with the coalesced selection tell (gx_store/burst.rs), and a tab that is no longer in front when the tell goes out is not acknowledged, because the user never stopped on it.
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
        self.gx_store_queue_attention_acknowledge(key, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_workspace_terminal_escape_pressed_for_native_view(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(shell_session_id) =
            self.agents_terminal_session_id_containing_responder(native_view)
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
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_native_terminal_prompt_editor_shortcut(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let delivery_target = RemotePromptEditorDeliveryTarget::NativeView(native_view as usize);
        if let Some((key, connection_generation)) =
            self.remote_prompt_editor_context_for_delivery_target(delivery_target)
        {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        &key,
                        connection_generation,
                        delivery_target,
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
        if let Some(shell_session_id) =
            self.agents_terminal_session_id_containing_responder(native_view)
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
