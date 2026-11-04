// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: workspace terminal event dispatch (bell, title change, escape, first-prompt-title cancel, attention acknowledge) and the native-view prompt-editor shortcut.

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
    ///
    /// CDXC:Notifications 2026-10-28 WHY: a pane showing a remote machine's session looked only for
    /// a local mapping, so clicking into it cleared the pane's own attention border while the
    /// sidebar's blue dot stayed and the machine never heard (the user on Windows, with a Mac added
    /// as a remote: "i have blue border around a chat then i click on it then it's not clearing the
    /// blue dot"). A remote pane acknowledges its session on its own machine, through the store,
    /// which reports it down that machine's tunnel (gx_store/attention/host.rs) and asks it to mark
    /// an orchestrator's finished threads seen.
    pub(crate) fn dispatch_gpui_workspace_session_attention_acknowledge(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        match self.workspace_terminal_key_for_shell_session(shell_session_id) {
            Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                self.gx_store_queue_attention_acknowledge(key, cx);
            }
            Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                self.gx_store_acknowledge_attention(key.attention_session_key(), cx);
            }
            None => {}
        }
    }
}
