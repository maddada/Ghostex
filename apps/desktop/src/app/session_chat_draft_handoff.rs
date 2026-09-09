use crate::app::consts::GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT;
use crate::app::session_chat::GpuiSessionChatDraftHandoff;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn append_recovered_prompt_into_session_chat(
        &mut self,
        session_id: TerminalSessionId,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return false;
        };
        if !self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
        {
            return false;
        }
        let literal = serde_json::json!({"content":content,"append":true})
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        surface.update(cx, |surface, _| surface.execute_app_owned_script(&format!("(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatInsertPromptRequested==='function'){{ns.onSessionChatInsertPromptRequested({literal});}}}})(); undefined;")));
        true
    }

    /// CDXC:SessionChat 2026-09-08 SEE-ALSO:
    /// server/src/session_chat_input_replace.rs owns draft replacement for sends, rewind cleanup and this view handoff. Native paste bypassed the session queue and appended the chat draft to the prompt rewind left behind.
    pub(crate) fn dispatch_session_chat_terminal_draft_handoff(
        &mut self,
        session_id: TerminalSessionId,
        handoff: GpuiSessionChatDraftHandoff,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let expected_session = self.workspace_terminal_key_for_shell_session(session_id);
        let request = if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            let params = serde_json::json!({
                "projectId": key.project_id, "sessionId": key.session_id,
                "content": handoff.content,
                "handoffId": handoff.handoff_id,
                "draftVersion": handoff.draft_version,
            });
            cx.background_executor().spawn(async move {
                gpui_gxserver_rpc_result(
                    "/api/replaceSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        } else {
            let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
                return false;
            };
            let Some(target) = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)
            else {
                return false;
            };
            let params = serde_json::json!({
                "projectId": key.project_id, "sessionId": key.session_id,
                "content": handoff.content,
                "handoffId": handoff.handoff_id,
                "draftVersion": handoff.draft_version,
            });
            cx.background_executor().spawn(async move {
                gpui_remote_gxserver_rpc_result(
                    &target,
                    "/api/replaceSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        };
        self.pending_session_chat_draft_handoffs.insert(session_id);
        // Claim once: remount and focus drains must not queue another replacement.
        self.pending_session_terminal_composer_insert
            .remove(&session_id);
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace_terminal_key_for_shell_session(session_id) != expected_session { return; }
                this.pending_session_chat_draft_handoffs.remove(&session_id);
                match result {
                    Ok(_) => {
                        this.release_session_chat_draft_handoff_stash(handoff, cx);
                        if this.agents_chat_mode_sessions.contains(&session_id) {
                            // A return to Chat may have raced the outgoing RPC. Capture after
                            // placement so the session queue hands the draft to its current owner.
                            this.request_session_chat_draft_transfer(session_id, cx);
                        }
                    }
                    Err(error) => {
                        this.pending_session_chat_received_drafts.insert(session_id, serde_json::json!({"content":handoff.content,"draftVersion":handoff.draft_version,"handoffId":handoff.handoff_id}));
                        this.deliver_pending_session_chat_received_draft(session_id, cx);
                        this.schedule_session_chat_received_draft_delivery(session_id, cx);
                        this.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Draft handoff failed",
                            &error,
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
        true
    }
    /// CDXC:Drafts 2026-09-10 WHY:
    /// Executing a page script is not receipt of text. Keep the payload until the editor confirms its durable save, retrying after reloads and leaving hidden destinations pending.
    pub(crate) fn deliver_pending_session_chat_received_draft(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        let Some(payload) = self.pending_session_chat_received_drafts.get(&session_id) else {
            return;
        };
        let Some(surface) = self.agents_chat_surfaces.get(&session_id).cloned() else {
            return;
        };
        let literal = payload
            .to_string()
            .replace('\u{2028}', "\\u2028")
            .replace('\u{2029}', "\\u2029");
        surface.update(cx, |surface, _| {
            surface.execute_app_owned_script(&format!("(function(){{var ns=window.ghostexGpui;if(ns&&typeof ns.onSessionChatInsertPromptRequested==='function'){{ns.onSessionChatInsertPromptRequested({literal});}}}})(); undefined;"));
        });
    }

    pub(crate) fn schedule_session_chat_received_draft_delivery(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let expected_session = self.workspace_terminal_key_for_shell_session(session_id);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                match this.update(cx, |this, cx| {
                    if this.workspace_terminal_key_for_shell_session(session_id) != expected_session
                    {
                        return false;
                    }
                    if !this
                        .pending_session_chat_received_drafts
                        .contains_key(&session_id)
                    {
                        return false;
                    }
                    this.deliver_pending_session_chat_received_draft(session_id, cx);
                    true
                }) {
                    Ok(true) => {}
                    _ => return,
                }
            }
        })
        .detach();
    }
}
