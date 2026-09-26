use std::collections::HashMap;
use std::collections::HashSet;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn agents_session_chat_transcript_agent(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&'static str> {
        let session = self.agents_workspace.session(session_id)?;
        match session.agent_icon {
            Some("antigravity-cli") => Some("antigravity"),
            Some("claude") => Some("claude"),
            Some("openclaude") => Some("claude"),
            Some("codex") => Some("codex"),
            Some("cursor-cli") => Some("cursor"),
            Some("grok-build") => Some("grok"),
            Some("hermes-agent") => Some("hermes-agent"),
            Some("pi") => Some("pi"),
            Some("omp") => Some("omp"),
            Some("zcode") => Some("zcode"),
            _ => None,
        }
    }

    pub(crate) fn agents_chat_local_key_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiLocalWorkspaceSessionKey> {
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))?;
        // Remote attach sessions use machine-scoped workspace keys and their
        // own tunneled gxserver bootstrap rather than the local daemon.
        if key.project_id.starts_with("remote:") || key.session_id.starts_with("remote:") {
            return None;
        }
        Some(key)
    }

    pub(crate) fn agents_chat_remote_key_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiRemoteAttachSessionKey> {
        let scoped_project_id = self.agents_workspace_project_id.as_deref()?;
        let remote_project = gpui_remote_project_reference_from_project_id(scoped_project_id)?;
        self.remote_attach_sessions
            .iter()
            .find_map(|(key, mapped)| {
                (*mapped == session_id
                    && key.remote_machine_id == remote_project.remote_machine_id
                    && key.project_id == remote_project.project_id)
                    .then(|| key.clone())
            })
    }

    /*
    CDXC:Drafts 2026-08-28:
    "This projected row can carry the chat view." A conversation proves that
    with its provider conversation id, but a DRAFT has none to give: its CLI
    publishes one only after it boots, and switching the draft's agent takes it
    away again for the length of the swap. A draft is a real gxserver row whose
    chat page addresses it by project/session id alone, so it qualifies on the
    draft marker instead — otherwise the action bar answers a click on Chat
    View with the "install hooks" settings toast, and `show_agents_session_chat_mode`
    refuses to bring the pane back, for a session that is chatting perfectly
    well. Membership in `agents_chat_mode_sessions` never depended on this (only
    session teardown and the user's own toggle remove a session from it), so
    this is about ENTERING chat, not about staying there.
    */
    fn gpui_projected_tab_session_is_chat_eligible(
        session: &GpuiSidebarWorkspaceTabSession,
    ) -> bool {
        // CDXC:SessionChat 2026-09-15 WHY:
        // A manually launched Codex can defer its first hook/session identity until the first prompt (reproduced with native Windows Codex 0.154).
        // The live terminal already accepts that prompt, so chat must be able to open before a transcript exists, just as it does for sidebar-created drafts.
        session.is_draft
            || (session.presentation_state.is_running() && session.agent_icon == Some("codex"))
            || session
                .agent_session_id
                .as_deref()
                .is_some_and(|agent_session_id| !agent_session_id.trim().is_empty())
    }

    pub(crate) fn agents_session_chat_eligible(&self, session_id: TerminalSessionId) -> bool {
        if self
            .agents_session_chat_transcript_agent(session_id)
            .is_none()
        {
            return false;
        }
        if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            return self
                .sidebar_gxserver_presentation_focus_state
                .active_project_tab_sessions
                .as_deref()
                .and_then(|sessions| {
                    sessions.iter().find(|session| {
                        session.key == GpuiWorkspaceTerminalSessionKey::Local(key.clone())
                    })
                })
                .is_some_and(Self::gpui_projected_tab_session_is_chat_eligible);
        }
        let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
            return false;
        };
        let has_chat_eligible_projection = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_deref()
            .and_then(|sessions| {
                sessions.iter().find(|session| {
                    session.key == GpuiWorkspaceTerminalSessionKey::Remote(key.clone())
                })
            })
            .is_some_and(Self::gpui_projected_tab_session_is_chat_eligible);
        has_chat_eligible_projection
            && self
                .gpui_remote_gxserver_request_target(key.remote_machine_id.as_str())
                .is_some()
    }

    pub(crate) fn toggle_agents_session_chat_mode_for_focused_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return;
        };
        self.handoff_agents_session_chat_mode(session_id, cx);
    }

    pub(crate) fn handoff_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            self.request_session_chat_handoff_to_terminal(session_id, cx);
        } else {
            self.request_terminal_handoff_to_session_chat(session_id, cx);
        }
    }

    pub(crate) fn toggle_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        // Toggling back to the terminal must always work, even if eligibility
        // inputs (agent icon, session mapping) changed while chat was showing.
        if self.agents_chat_mode_sessions.remove(&session_id) {
            // Chat's CEF child owns keyboard focus while visible. Queue the
            // canonical terminal focus handoff for the exact shell-focused
            // slot so the terminal reclaims first responder as it remounts.
            match self.focused_terminal_text_mount_target() {
                Some(FocusedTerminalTextMountTarget::Agents(slot_id))
                    if slot_id.session_id == session_id =>
                {
                    self.request_agents_terminal_text_focus_handoff(slot_id);
                }
                _ => {}
            }
            self.reconcile_agents_pane_surfaces(cx);
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }
        let _ = self.show_agents_session_chat_mode(session_id, cx);
    }

    /// Session-level handoff request: the occupant (terminal or chat composer) is resolved when the handoff runs. A chat-mode session needs its page to exist first.
    pub(crate) fn request_agents_session_text_focus_handoff(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.request_agents_terminal_text_focus_handoff(slot_id);
        if self.agents_chat_mode_sessions.contains(&slot_id.session_id) {
            self.reconcile_agents_pane_surfaces(cx);
        }
    }

    /*
    CDXC:Drafts 2026-08-18:
    Background terminal → chat draft transfer for every view switch. Automatic,
    manual, local, and remote switches all show Chat first; draft capture must
    never keep the user trapped on a terminal startup/permission prompt or on
    an agent version that cannot answer its prompt-editor handshake.

    Chat is shown immediately and the captured draft lands in the composer when
    the daemon's prompt-editor handshake answers, so a slow or unanswerable
    capture costs the user nothing but the text staying where they typed it.
    That is also why failures are silent here: the user did not ask for a
    transfer, so a warning toast would be noise about an operation they never
    requested.
    */
    pub(crate) fn request_session_chat_draft_transfer(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.deliver_pending_session_chat_received_draft(session_id, cx);
        if self
            .pending_session_chat_draft_handoffs
            .contains(&session_id)
            || self
                .session_chat_draft_capture_in_flight
                .contains(&session_id)
            || self
                .pending_session_chat_received_drafts
                .contains_key(&session_id)
        {
            return;
        }
        let expected_session = self.workspace_terminal_key_for_shell_session(session_id);
        let request = if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_gxserver_rpc_result(
                    "/api/handoffSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        } else {
            let Some(key) = self.agents_chat_remote_key_for_session(session_id) else {
                return;
            };
            let Some(target) = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)
            else {
                return;
            };
            let params = serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
            });
            cx.background_executor().spawn(async move {
                gpui_remote_gxserver_rpc_result(
                    &target,
                    "/api/handoffSessionChatDraft",
                    &params,
                    GPUI_SESSION_CHAT_DRAFT_TRANSFER_TIMEOUT,
                )
            })
        };
        self.session_chat_draft_capture_in_flight.insert(session_id);
        cx.spawn(async move |this, cx| {
            let result = request.await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace_terminal_key_for_shell_session(session_id) != expected_session {
                    return;
                }
                this.session_chat_draft_capture_in_flight
                    .remove(&session_id);
                let Ok(result) = result else {
                    return;
                };
                let content = result
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if content.is_empty() {
                    return;
                }
                if result
                    .get("handoffId")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    // Launch drafts already live in durable session state.
                    this.deliver_session_chat_composer_insert(session_id, content, cx);
                    return;
                }
                if !this.agents_chat_mode_sessions.contains(&session_id) {
                    let id = format!(
                        "{}-return",
                        result
                            .get("handoffId")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default()
                    );
                    this.pending_session_terminal_composer_insert.insert(
                        session_id,
                        crate::app::session_chat::GpuiSessionChatDraftHandoff {
                            content,
                            handoff_id: id,
                            draft_version: result.get("draftVersion").cloned(),
                            stashed_prompt_id: None,
                        },
                    );
                    this.deliver_pending_session_terminal_composer_insert(session_id, cx);
                    return;
                }
                this.pending_session_chat_received_drafts
                    .insert(session_id, result);
                this.deliver_pending_session_chat_received_draft(session_id, cx);
                this.schedule_session_chat_received_draft_delivery(session_id, cx);
            });
        })
        .detach();
    }

    pub(crate) fn start_session_chat_queued_count_polling(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_SESSION_CHAT_QUEUE_COUNT_POLL_INTERVAL)
                    .await;

                if this
                    .update(cx, |this, cx| {
                        this.refresh_session_chat_queued_counts(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    /// Sessions whose queued-prompt count the terminal chrome needs right now,
    /// each with the daemon that owns it (`None` is this Mac's local daemon)
    /// and that daemon's own project/session ids.
    pub(crate) fn session_chat_queued_count_requests(
        &self,
    ) -> Vec<(
        TerminalSessionId,
        Option<GpuiRemoteGxserverRequestTarget>,
        String,
        String,
    )> {
        if !self.agents_workspace_visible() {
            return Vec::new();
        }
        self.agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
            .filter(|session_id| !self.agents_chat_mode_sessions.contains(session_id))
            .filter(|session_id| {
                self.agents_session_chat_transcript_agent(*session_id)
                    .is_some()
            })
            .filter_map(|session_id| {
                if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
                    return Some((session_id, None, key.project_id, key.session_id));
                }
                let key = self.agents_chat_remote_key_for_session(session_id)?;
                let target = self.gpui_remote_gxserver_request_target(&key.remote_machine_id)?;
                Some((session_id, Some(target), key.project_id, key.session_id))
            })
            .collect()
    }

    pub(crate) fn refresh_session_chat_queued_counts(&mut self, cx: &mut gpui::Context<Self>) {
        if self.session_chat_queued_count_refresh_in_flight {
            return;
        }
        let requests = self.session_chat_queued_count_requests();
        if requests.is_empty() {
            if !self.session_chat_queued_counts.is_empty() {
                self.session_chat_queued_counts.clear();
                cx.notify();
            }
            return;
        }
        self.session_chat_queued_count_refresh_in_flight = true;
        cx.spawn(async move |this, cx| {
            let reads = cx
                .background_executor()
                .spawn(async move {
                    requests
                        .into_iter()
                        .map(|(session_id, target, project_id, gxserver_session_id)| {
                            let params = serde_json::json!({
                                "projectId": project_id,
                                "sessionId": gxserver_session_id,
                            });
                            let result = match target.as_ref() {
                                Some(target) => gpui_remote_gxserver_rpc_result(
                                    target,
                                    "/api/readSessionChatQueue",
                                    &params,
                                    GPUI_SESSION_CHAT_QUEUE_COUNT_TIMEOUT,
                                ),
                                None => gpui_gxserver_rpc_result(
                                    "/api/readSessionChatQueue",
                                    &params,
                                    GPUI_SESSION_CHAT_QUEUE_COUNT_TIMEOUT,
                                ),
                            };
                            (
                                session_id,
                                result.ok().map(|value| {
                                    gpui_session_chat_queued_counts_from_result(&value)
                                }),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.session_chat_queued_count_refresh_in_flight = false;
                this.apply_session_chat_queued_counts(reads, cx);
            });
        })
        .detach();
    }

    /// A read that failed (dead tunnel, daemon restart, a daemon that predates
    /// the queue) keeps the previous count instead of blanking the chip, so a
    /// single lost round trip cannot make a pane's queue look emptied.
    pub(crate) fn apply_session_chat_queued_counts(
        &mut self,
        reads: Vec<(TerminalSessionId, Option<GpuiSessionChatQueuedCounts>)>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut counts = HashMap::new();
        for (session_id, read) in reads {
            let read = match read {
                Some(read) => read,
                None => self
                    .session_chat_queued_counts
                    .get(&session_id)
                    .copied()
                    .unwrap_or_default(),
            };
            if read.total > 0 {
                counts.insert(session_id, read);
            }
        }
        if self.session_chat_queued_counts != counts {
            self.session_chat_queued_counts = counts;
            cx.notify();
        }
    }

    /// Puts transferred draft text in the chat composer, or parks it until the
    /// composer reports itself ready. The park is not an edge case: an
    /// automatic switch starts the transfer and the surface load in the same
    /// tick, so either can win.
    pub(crate) fn deliver_session_chat_composer_insert(
        &mut self,
        session_id: TerminalSessionId,
        content: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.agents_chat_mode_sessions.contains(&session_id) {
            return;
        }
        if self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
            && self.insert_prompt_into_session_chat(session_id, &content, cx)
        {
            return;
        }
        self.pending_session_chat_composer_insert
            .insert(session_id, content);
    }

    pub(crate) fn show_agents_session_chat_mode(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_chat_mode_sessions.contains(&session_id) {
            return true;
        }
        if !self.agents_session_chat_eligible(session_id) {
            return false;
        }
        self.agents_chat_mode_sessions.insert(session_id);
        self.request_keyboard_handoff_for_session(session_id);
        /*
        CDXC:Drafts 2026-08-24:
        A handed-off draft that never reached the terminal follows the user
        back into chat instead of leaving them an alarmingly empty composer
        while it waits, invisible, for another terminal switch. The Saved
        Prompts row stays — only a confirmed terminal paste may delete it.
        */
        if let Some(handoff) = self
            .pending_session_terminal_composer_insert
            .remove(&session_id)
        {
            self.deliver_session_chat_composer_insert(session_id, handoff.content, cx);
        }
        self.reconcile_agents_pane_surfaces(cx);
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    /// CDXC:SessionChat 2026-09-24 DECISION:
    /// User: a session opened by clicking its sidebar row or picking it from Cmd+P (which always wakes it) must not show the "Resume" pill in its chat view. The automatic Chat switch waits for a live terminal, so a sleeping tab whose agent prefers Chat drew its sleeping terminal body until the wake finished; the selection now opens it in Chat at once, like a first click on an unmapped session (sidebar_direct_focus.rs). A session the automatic switch already considered keeps the view it has, so a tab the user moved back to Terminal stays there.
    pub(crate) fn adopt_preferred_chat_view_on_selection(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&session_id)
            || self
                .agents_chat_auto_switch_observed_sessions
                .contains_key(&session_id)
        {
            return;
        }
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let interface = gpui_effective_preferred_agent_interface_for_agent_icon(
            settings.object(),
            self.agents_workspace
                .session(session_id)
                .and_then(|session| session.agent_icon),
        );
        if interface != GpuiPreferredAgentInterface::Chat {
            return;
        }
        if self.show_agents_session_chat_mode(session_id, cx) {
            self.agents_chat_auto_switch_observed_sessions
                .insert(session_id, interface);
        }
    }

    pub(crate) fn agents_terminal_runtime_is_live_for_chat_launch(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        if self.agents_gpui_engine_terminals.contains_key(&session_id)
            || (self.agents_terminal_has_detachable_viewer(session_id)
                && self
                    .agents_workspace
                    .session(session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                    }))
        {
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            return self
                .agents_terminal_ghostty_surfaces
                .keys()
                .any(|slot_id| slot_id.session_id == session_id);
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    pub(crate) fn activate_preferred_agents_chat_launch_intent(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) else {
            return false;
        };
        if !self.pending_agents_chat_launch_intents.contains(&key) {
            return false;
        }
        // The projected agent icon is the compatibility authority available
        // before the hidden terminal runtime starts. Unsupported terminals
        // keep their normal terminal body and focus behavior.
        if self
            .agents_session_chat_transcript_agent(session_id)
            .is_none()
        {
            self.pending_agents_chat_launch_intents.remove(&key);
            return false;
        }

        self.pending_agents_chat_launch_intents.remove(&key);
        self.agents_chat_mode_sessions.insert(session_id);
        self.request_keyboard_handoff_for_session(session_id);
        // A staged first-input draft (Handoff / Export's transcript mention)
        // belongs in the chat composer the user is about to see, not in the
        // terminal this launch parks. See `request_session_chat_launch_draft`.
        self.request_session_chat_launch_draft(session_id, cx);
        self.reconcile_agents_pane_surfaces(cx);
        true
    }

    pub(crate) fn reconcile_preferred_agents_chat_launch_intents(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let intents = self
            .pending_agents_chat_launch_intents
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for key in intents {
            let shell_session_id = match &key {
                GpuiWorkspaceTerminalSessionKey::Local(local_key) => self
                    .local_workspace_session_mappings
                    .get(local_key)
                    .copied(),
                GpuiWorkspaceTerminalSessionKey::Remote(remote_key) => {
                    self.remote_attach_sessions.get(remote_key).copied()
                }
            };
            let Some(shell_session_id) = shell_session_id else {
                continue;
            };
            // The icon is the existing chat capability authority. Once the
            // terminal exists, an unsupported launcher keeps Terminal view.
            if self
                .agents_session_chat_transcript_agent(shell_session_id)
                .is_none()
            {
                self.pending_agents_chat_launch_intents.remove(&key);
                continue;
            }
            if !self.agents_terminal_runtime_is_live_for_chat_launch(shell_session_id)
                || !self.agents_session_chat_eligible(shell_session_id)
            {
                continue;
            }
            self.pending_agents_chat_launch_intents.remove(&key);
            if self.show_agents_session_chat_mode(shell_session_id, cx) {
                /*
                CDXC:Drafts 2026-08-18:
                This intent waits for the agent to become chat-eligible, which
                can take the whole of its boot. The terminal is live and
                focused that entire time, so a user who started typing before
                the switch landed must not lose what they wrote.
                */
                self.request_session_chat_draft_transfer(shell_session_id, cx);
            }
        }
    }

    pub(crate) fn agents_session_chat_gxserver_bootstrap(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<cef::SidebarGxserverBootstrap> {
        if self.agents_chat_local_key_for_session(session_id).is_some() {
            return self.sidebar_gxserver_bootstrap.clone();
        }
        let key = self.agents_chat_remote_key_for_session(session_id)?;
        let target = self.gpui_remote_gxserver_request_target(key.remote_machine_id.as_str())?;
        Some(cef::SidebarGxserverBootstrap {
            base_url: format!("http://127.0.0.1:{}", target.local_port),
            auth_token: target.token,
            protocol_version: GPUI_GXSERVER_PROTOCOL_VERSION as i32,
            client_id: format!("{GPUI_SIDEBAR_GXSERVER_CLIENT_ID}-chat-{}", session_id.0),
            initial_active_project_id: Some(key.project_id),
            focused_session_id: Some(key.session_id.clone()),
            visible_session_ids: vec![key.session_id],
        })
    }

    /*
    CDXC:PromptSearch 2026-08-23:
    Search by Prompt is a native child-window page, matching the Settings
    ownership model instead of replacing a pane body. Prompt history is
    machine-wide, so the page URL carries only the current visual theme while
    the child surface receives the local gxserver bootstrap separately.
    */
    pub(crate) fn agents_find_runtime_url(&self) -> Option<String> {
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_FIND_URL", "find.html").ok()?;
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        Some(append_url_query_params(
            base_url,
            &[
                (
                    "theme",
                    if gpui_session_chat_uses_light_theme(settings.object()) {
                        "light"
                    } else {
                        "dark"
                    }
                    .to_string(),
                ),
                (
                    "fontFamily",
                    gpui_session_chat_font_family_from_settings(settings.object()),
                ),
            ],
        ))
    }

    pub(crate) fn receive_find_prompts_modal_host_action(
        &mut self,
        message: &serde_json::Value,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = message.get("action").and_then(serde_json::Value::as_str) else {
            return;
        };
        match action {
            "ready" => {
                if let Some(handle) = self.app_modal_window.clone() {
                    let _ = handle.update(cx, |host, modal_window, cx| {
                        modal_window.activate_window();
                        if let Some(surface) = &host.surface {
                            surface.update(cx, |surface, _| surface.focus());
                        }
                    });
                }
            }
            "close" => {
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
            }
            "focusSession" => {
                let project_id = message
                    .get("projectId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let session_id = message
                    .get("sessionId")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if project_id.is_empty() || session_id.is_empty() {
                    return;
                }
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                // CDXC:PromptSearch 2026-09-10 WHY:
                // Direct native focus skipped the sidebar presentation update and project-switch coordination, so a result could attach into the outgoing workspace or leave its sidebar row hidden.
                // Use the modal session activation route, then the same reveal request as the titlebar button to expand and scroll the owning sidebar containers.
                let sidebar_session_id =
                    gpui_combined_presentation_session_id(&project_id, &session_id);
                if self.dispatch_gpui_command_palette_session_focus(&sidebar_session_id, cx) {
                    self.reveal_sidebar_session(&sidebar_session_id, cx);
                } else {
                    self.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not open session",
                        "The sidebar is not ready. Try opening the search result again.",
                        cx,
                    );
                }
            }
            "launchSession" => {
                let command = message
                    .get("command")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let cwd = message
                    .get("cwd")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if command.is_empty() || cwd.is_empty() {
                    return;
                }
                let title = message
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                self.close_gpui_app_modal_window_and_restore_command_focus(cx);
                self.dispatch_gpui_os_integration_command_message(
                    serde_json::json!({
                        "action": "createQuickTerminal",
                        "command": command,
                        "cwd": cwd,
                        "title": title,
                    }),
                    cx,
                );
            }
            _ => {}
        }
    }

    /// Reconcile the per-session Chat surfaces that can occupy a workspace pane.
    /// CDXC:SessionChat 2026-09-16 WHY:
    /// One session switch used to run the chat reconcile four times back to back (bootstrap refresh, keyboard handoff, text-focus handoff, CEF visibility sync), each re-walking every surface.
    /// Requests through this entry coalesce into a single pass at the end of the current effect cycle, which still lands before the next frame paints.
    /// Callers that read the surface map right after reconciling must call `reconcile_agents_chat_surfaces` directly.
    /// CDXC:SessionChat 2026-09-19 WHY:
    /// While a held "next tab" key moves through tabs the pass waits until the selection settles (gx_store/burst.rs), so a chat view is not created for a tab the user only passes. A chat view that already exists is drawn by the pane body without this pass.
    pub(crate) fn reconcile_agents_pane_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        if self.gx_store_selection_is_settling() {
            self.gx_store_defer_chat_reconcile();
            return;
        }
        if self.agents_chat_reconcile_scheduled {
            return;
        }
        self.agents_chat_reconcile_scheduled = true;
        let app = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = app.update(cx, |app, cx| {
                app.agents_chat_reconcile_scheduled = false;
                app.reconcile_agents_chat_surfaces(cx);
            });
        });
    }

    pub(crate) fn reconcile_agents_chat_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        // Drop chat state for sessions that no longer exist in the shell.
        let live_session_ids = self
            .agents_workspace
            .terminal_session_ids()
            .into_iter()
            .collect::<HashSet<_>>();
        self.agents_chat_mode_sessions
            .retain(|session_id| live_session_ids.contains(session_id));
        let stale_native = self
            .native_chat_views
            .keys()
            .copied()
            .filter(|id| !live_session_ids.contains(id))
            .collect::<Vec<_>>();
        for id in stale_native {
            self.remove_agents_chat_surface_for_session(id, cx);
        }
        let drag_active = self.workspace_tab_drag_active
            || self.browser_tab_drag_active
            || self.command_tab_drag_active;
        /*
        CDXC:SessionChat 2026-09-20 WHY:
        A chat page is visible exactly when its Agents pane is rendered, whatever the view panel
        shows, because that column is on screen in every view. This supersedes the 2026-08-02 rule
        that also enumerated the companion side pane's chat slots.
        */
        let visible_session_ids = if drag_active || !self.agents_workspace_visible() {
            HashSet::new()
        } else {
            self.agents_workspace
                .rendered_leaf_order()
                .into_iter()
                .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
                .filter(|session_id| self.agents_chat_mode_sessions.contains(session_id))
                .collect::<HashSet<_>>()
        };
        for session_id in &visible_session_ids {
            self.ensure_native_chat(*session_id, cx);
        }
        // CDXC:SessionChat 2026-09-24 WHY:
        // A parked chat view keeps applying state without repainting (notify_if_shown), so its cached paint is from whenever it was last visible. Without a notify when it becomes visible again, a session switch's first frame was that stale paint and the follow-tail reposition landed frames later: the flicker of switching between chats.
        for session_id in visible_session_ids.difference(&self.native_chat_visible_sessions) {
            if let Some(view) = self.native_chat_views.get(session_id) {
                view.update(cx, |_, cx| cx.notify());
            }
        }
        self.dismiss_native_chat_windows_leaving_view(&visible_session_ids, cx);
        self.restore_native_chat_windows_entering_view(&visible_session_ids, cx);
        self.native_chat_visible_sessions = visible_session_ids.clone();
        self.resume_visible_native_chat_runtimes(cx);
        self.schedule_native_chat_prewarm(cx);
    }

    #[track_caller]
    pub(crate) fn remove_agents_chat_surface_for_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(view) = self.native_chat_views.remove(&session_id) {
            view.update(cx, |view, cx| view.dismiss_windows_for_hidden_pane(cx));
        }
        let caller = std::panic::Location::caller();
        let file = std::path::Path::new(caller.file())
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        self.record_session_chat_lifecycle(
            session_id,
            "sessionChat.nativePageRemoved",
            &format!("{file}:{}", caller.line()),
        );
        if let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) {
            self.account_switch_progress.remove(&key);
            self.forget_session_chat_presentation(&key);
        }
        self.agents_chat_mode_sessions.remove(&session_id);
        self.session_chat_composer_ready_sessions
            .remove(&session_id);
        self.session_chat_composer_empty_reports.remove(&session_id);
        if let Some(state) = self.agents_chat_page_states.remove(&session_id) {
            if let Some(key) = state.account_key.as_ref() {
                self.forget_session_chat_presentation(key);
            }
        }
        self.drop_pending_keyboard_handoff_for_session(session_id);
        self.pending_session_chat_composer_insert
            .remove(&session_id);
        /*
        CDXC:Drafts 2026-08-24:
        A handed-off draft that never reached its terminal is dropped here with
        no way to hand it anywhere else — the chat surface that owned it is
        going away in the same call. That is survivable only because the record
        is not the text's only home: the transient Saved Prompts row created
        before the composer was cleared is deleted solely on a confirmed paste,
        so this drop leaves the draft recoverable from Prompts.
        */
        self.pending_session_terminal_composer_insert
            .remove(&session_id);
        self.pending_session_chat_draft_handoffs.remove(&session_id);
        self.session_chat_draft_capture_in_flight
            .remove(&session_id);
        self.pending_session_chat_received_drafts
            .remove(&session_id);
    }

    /*
    CDXC:SessionChat 2026-08-26:
    An active-project switch parks the outgoing project's chat views instead of
    destroying them, the same treatment the terminal runtime already gets on
    that path, so returning to the project does not rebuild every chat pane.

    The views and every companion map keyed by the same project-local shell
    session ids leave together in one bundle, so the incoming project's colliding
    ids can never read the outgoing project's composer state. Chat-mode
    membership is cleared here and reinstated by the caller from the incoming
    project's parked shell-state JSON.

    Default-view observations travel with the project so restoring its saved
    Terminal view does not trigger another automatic Chat switch.
    The pending launch intents are switch-scoped, and the two
    draft-handoff records are dropped under the same contract as the per-session
    teardown above (every dropped handoff still has its Saved Prompts row, which
    only a confirmed terminal paste deletes).
    */
    pub(crate) fn park_all_agents_chat_surfaces(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> ParkedAgentsChatRuntime {
        self.agents_chat_mode_sessions.clear();
        self.pending_agents_chat_launch_intents.clear();
        self.pending_session_terminal_composer_insert.clear();
        self.pending_session_chat_draft_handoffs.clear();
        self.session_chat_draft_capture_in_flight.clear();
        self.pending_session_chat_received_drafts.clear();
        for view in self.native_chat_views.values() {
            view.update(cx, |view, cx| view.dismiss_windows_for_hidden_pane(cx));
        }
        ParkedAgentsChatRuntime {
            auto_switch_observed_sessions: std::mem::take(
                &mut self.agents_chat_auto_switch_observed_sessions,
            ),
            page_states: std::mem::take(&mut self.agents_chat_page_states),
            native_views: std::mem::take(&mut self.native_chat_views),
            composer_ready_sessions: std::mem::take(&mut self.session_chat_composer_ready_sessions),
            composer_empty_reports: std::mem::take(&mut self.session_chat_composer_empty_reports),
            pending_composer_insert: std::mem::take(&mut self.pending_session_chat_composer_insert),
        }
    }

    /// Reinstall a project's parked chat views as the live ones. The caller has
    /// already restored that project's `WorkspaceModel` and session mappings, so
    /// the restored views match live session ids and `reconcile_agents_chat_surfaces`
    /// shows them again without rebuilding them.
    pub(crate) fn restore_parked_agents_chat_surfaces(&mut self, parked: ParkedAgentsChatRuntime) {
        self.agents_chat_auto_switch_observed_sessions = parked.auto_switch_observed_sessions;
        self.agents_chat_page_states = parked.page_states;
        self.native_chat_views = parked.native_views;
        self.session_chat_composer_ready_sessions = parked.composer_ready_sessions;
        self.session_chat_composer_empty_reports = parked.composer_empty_reports;
        self.pending_session_chat_composer_insert = parked.pending_composer_insert;
    }
}
