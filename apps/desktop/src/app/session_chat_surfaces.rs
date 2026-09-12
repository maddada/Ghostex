use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Entity;
use gpui::Window;
use gpui::rgb;

use crate::app::consts::*;
use crate::app::element::*;
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
        session.is_draft
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
                Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                    if slot_id.session_id == session_id =>
                {
                    self.request_project_editor_companion_terminal_text_focus_handoff(slot_id);
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

    pub(crate) fn request_project_editor_companion_session_text_focus_handoff(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) {
        self.request_project_editor_companion_terminal_text_focus_handoff(slot_id);
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
        if self.active_mode != TitlebarMode::Agents {
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

    pub(crate) fn agents_terminal_runtime_is_live_for_chat_launch(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        if self.agents_gpui_engine_terminals.contains_key(&session_id) {
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

    pub(crate) fn agents_session_chat_runtime_url(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<String> {
        let agent = self.agents_session_chat_transcript_agent(session_id)?;
        let (project_id, gxserver_session_id, remote) =
            if let Some(key) = self.agents_chat_local_key_for_session(session_id) {
                (key.project_id, key.session_id, false)
            } else {
                let key = self.agents_chat_remote_key_for_session(session_id)?;
                (key.project_id, key.session_id, true)
            };
        let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_CHAT_URL", "chat.html").ok()?;
        let mut params = vec![
            ("projectId", project_id),
            ("sessionId", gxserver_session_id),
            ("agentId", agent.to_string()),
            (
                "hideAccountEmails",
                shared_settings::shared_sidebar_settings_snapshot()
                    .object()
                    .get("hideAccountEmails")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                    .to_string(),
            ),
            (
                "theme",
                gpui_session_chat_theme_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "fontFamily",
                gpui_session_chat_font_family_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                ),
            ),
            (
                "customTranscriptWidthEnabled",
                gpui_session_chat_custom_transcript_width_enabled_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "transcriptWidthPercent",
                gpui_session_chat_transcript_width_percent_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "fileEditPreviews",
                gpui_session_chat_file_edit_previews_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "verboseMode",
                gpui_session_chat_verbose_mode_from_settings(
                    shared_settings::shared_sidebar_settings_snapshot().object(),
                )
                .to_string(),
            ),
            (
                "hotkeys",
                shared_settings::shared_sidebar_settings_snapshot()
                    .object()
                    .get("hotkeys")
                    .cloned()
                    .unwrap_or_else(|| serde_json::Value::Object(Default::default()))
                    .to_string(),
            ),
        ];
        if remote {
            if let Some(key) = self.agents_chat_remote_key_for_session(session_id) {
                params.push(("remoteMachineId", key.remote_machine_id));
            }
            params.push(("remote", "true".to_string()));
        }
        Some(append_url_query_params(base_url, &params))
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
    pub(crate) fn reconcile_agents_pane_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        self.reconcile_agents_chat_surfaces(cx);
    }

    /// The Chat CEF surface currently occupying a session's pane.
    pub(crate) fn agents_pane_cef_surface(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<&Entity<CefSurface>> {
        self.agents_chat_surfaces.get(&session_id)
    }

    pub(crate) fn ensure_agents_chat_surface(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<Entity<CefSurface>> {
        if let Some(surface) = self.agents_chat_surfaces.get(&session_id) {
            return Some(surface.clone());
        }
        // The chat page cannot do anything without its owning gxserver
        // bootstrap; later local bootstrap or remote reconnect availability
        // retries through the normal visibility reconciliation path.
        let bootstrap = self.agents_session_chat_gxserver_bootstrap(session_id)?;
        let url = self.agents_session_chat_runtime_url(session_id)?;
        let force_fresh_renderer = self
            .agents_chat_page_states
            .get(&session_id)
            .is_some_and(|state| state.force_fresh_renderer);
        let mut page_state = SessionChatPageState::new();
        page_state.account_key = self.workspace_terminal_key_for_shell_session(session_id);
        let url = append_url_query_params(
            url,
            &[("pageGeneration", page_state.generation.to_string())],
        );
        self.expire_reusable_chat_renderers();
        let preferred_renderer = page_state
            .account_key
            .as_ref()
            .and_then(|key| {
                self.reusable_chat_renderers
                    .iter()
                    .rposition(|(_, _, previous_key, _)| previous_key.as_ref() == Some(key))
            })
            .or_else(|| self.reusable_chat_renderers.len().checked_sub(1));
        if !force_fresh_renderer && let Some(index) = preferred_renderer {
            let (surface, renderer_id, _, _) = self.reusable_chat_renderers.remove(index);
            page_state.renderer_id = renderer_id;
            page_state.awaiting_activation = true;
            let activation_generation = page_state.generation;
            let generation = activation_generation.to_string();
            self.agents_chat_page_states.insert(session_id, page_state);
            self.agents_chat_surfaces
                .insert(session_id, surface.clone());
            surface.update(cx, |surface, _| {
                surface.set_session_chat_pane_focused(false, true);
                surface.activate_session_chat(&url, &generation, bootstrap);
            });
            self.watch_session_chat_activation(activation_generation, cx);
            self.record_session_chat_lifecycle(
                session_id,
                "sessionChat.nativePageReused",
                "ensure",
            );
            return Some(surface);
        }
        let host_action_handler =
            self.session_chat_host_bridge_event_handler(page_state.renderer_id, cx);
        let light_chat_theme = gpui_session_chat_uses_light_theme(
            shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let prepaint_background = if light_chat_theme {
            CEF_LIGHT_PREPAINT_BACKGROUND_COLOR
        } else {
            CEF_SESSION_CHAT_DARK_PREPAINT_BACKGROUND_COLOR
        };
        let background = if light_chat_theme {
            rgb(0xfdfdfd).into()
        } else {
            rgb(0x0d0d0d).into()
        };
        /*
        CDXC:ContextMenus 2026-08-21:
        The first-party chat composer owns a shadcn context menu instead of
        exposing Chromium's page/developer menu. Copy and Cut still use the
        browser clipboard writer, while Paste is routed through CEF's native
        edit command because Chromium does not consider this windowed page
        focused. Grant only this bundled chat origin the same bounded clipboard
        capability that the app-owned Source surface receives.
        */
        let trusted_clipboard_origin = Some(url.clone());
        #[cfg(target_os = "macos")]
        let chat_parent = self.companion_native_parent();
        #[cfg(not(target_os = "macos"))]
        let chat_parent = self.parent_ns_view;
        let surface = match CefSurface::try_new(
            format!(
                "ghostex-gpui-session-chat-renderer-{}",
                page_state.renderer_id
            ),
            chat_parent,
            url,
            "session-chat".to_string(),
            prepaint_background,
            false,
            background,
            trusted_clipboard_origin,
            true,
            None,
            None,
            None,
            None,
            Some(bootstrap),
            None,
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::SessionChat),
            Some(host_action_handler),
            None,
            cx,
        ) {
            Ok(surface) => surface,
            Err(error) => {
                // Ensure-style reconcile: skip this pass, retried on the next
                // visibility sync (CDXC:CefRuntime 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({ "surface": "sessionChat", "error": error }),
                );
                return None;
            }
        };
        self.agents_chat_page_states.insert(session_id, page_state);
        self.agents_chat_surfaces
            .insert(session_id, surface.clone());
        self.record_session_chat_lifecycle(session_id, "sessionChat.nativePageCreated", "ensure");
        Some(surface)
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
        /*
        CDXC:Diagnostics 2026-08-28:
        Only surfaces whose SESSION is gone are destroyed here. A live session
        toggled back to the terminal view used to lose its page too, so every
        chat↔terminal toggle reloaded chat.html from scratch — the visible
        blank → "Loading conversation…" → chat flash on the way back. The
        toggled-away page now just hides: the visibility loop below stamps its
        hidden clock, and the RAM ceiling stays enforced by the eviction pass,
        which is also the safer destroyer (it refuses pages holding unsent
        composer text, which this teardown would have dropped). Dead sessions
        cannot take that route — eviction treats an unknown session as
        not-evictable — so they are still destroyed here, with the pending
        draft-handoff guard keeping a mid-handoff page alive long enough to
        answer.
        */
        let stale_surface_ids = self
            .agents_chat_surfaces
            .keys()
            .copied()
            .filter(|session_id| {
                !live_session_ids.contains(session_id)
                    && !self
                        .pending_session_chat_draft_handoffs
                        .contains(session_id)
            })
            .collect::<Vec<_>>();
        for session_id in stale_surface_ids {
            self.record_session_chat_lifecycle(
                session_id,
                "sessionChat.nativePageRemoved",
                "sessionNoLongerInShell",
            );
            self.session_chat_composer_ready_sessions
                .remove(&session_id);
            self.session_chat_composer_empty_reports.remove(&session_id);
            self.agents_chat_surface_hidden_since.remove(&session_id);
            self.agents_chat_page_states.remove(&session_id);
            if let Some(surface) = self.agents_chat_surfaces.remove(&session_id) {
                surface.update(cx, |surface, _| surface.set_visible(false));
            }
        }

        let drag_active = self.workspace_tab_drag_active
            || self.browser_tab_drag_active
            || self.command_tab_drag_active;
        let visible_session_ids = if drag_active {
            HashSet::new()
        } else if self.active_mode == TitlebarMode::Agents {
            self.agents_workspace
                .rendered_leaf_order()
                .into_iter()
                .filter_map(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
                .filter(|session_id| self.agents_chat_mode_sessions.contains(session_id))
                .collect::<HashSet<_>>()
        } else if self.active_mode.is_project_editor_mode() {
            // CDXC:SessionChat 2026-08-02: the companion side pane
            // shows chat-mode sessions in Code/Browser/Kanban/Automate/Docs
            // too. The mount-slot enumeration already gates on companion
            // visibility, mode wakefulness, and slot eligibility.
            self.current_project_editor_companion_terminal_body_mount_slots()
                .into_iter()
                .map(|slot_id| slot_id.session_id)
                .filter(|session_id| self.agents_chat_mode_sessions.contains(session_id))
                .collect::<HashSet<_>>()
        } else {
            HashSet::new()
        };
        for session_id in &visible_session_ids {
            let _ = self.ensure_agents_chat_surface(*session_id, cx);
        }
        let mut visibility_changed = false;
        for (session_id, surface) in &self.agents_chat_surfaces {
            let visible = visible_session_ids.contains(session_id)
                && self
                    .agents_chat_page_states
                    .get(session_id)
                    .is_some_and(|state| !state.awaiting_activation)
                && self
                    .session_account_switch_placeholder_progress(*session_id)
                    .is_none();
            surface.update(cx, |surface, _| surface.set_visible(visible));
            /*
            CDXC:SessionChat 2026-08-24:
            The hidden clock the RAM eviction pass reads. A surface that is
            already aging must keep its original
            stamp across every later hidden pass, and only a pass that actually
            showed it clears the clock. Reconcile runs on drags, mode switches,
            and pane edits, so overwriting here would keep resetting the timer
            and nothing would ever expire.
            */
            if visible {
                visibility_changed |= self
                    .agents_chat_surface_hidden_since
                    .remove(session_id)
                    .is_some();
                if let Some(state) = self.agents_chat_page_states.get_mut(session_id) {
                    state.pending_probe = None;
                }
            } else if !self
                .agents_chat_surface_hidden_since
                .contains_key(session_id)
            {
                visibility_changed = true;
                self.agents_chat_surface_hidden_since
                    .insert(*session_id, Instant::now());
                self.session_chat_composer_empty_reports.remove(session_id);
            }
        }
        if visibility_changed {
            self.evict_expired_hidden_agents_chat_surfaces(cx);
        }
    }

    /// Whether a hidden chat surface holds nothing that would be destroyed with
    /// its page. See `evict_expired_hidden_agents_chat_surfaces`.
    pub(crate) fn agents_chat_surface_evictable(
        &self,
        session_id: TerminalSessionId,
        require_empty: bool,
    ) -> bool {
        // Only an idle agent is evictable. A working or attention session is
        // producing output the user is coming back to, and a session the shell
        // no longer knows about has an unknown status rather than an idle one.
        let Some(session) = self.agents_workspace.session(session_id) else {
            return false;
        };
        if session.activity != AgentTerminalActivity::Idle
            || self
                .agents_chat_page_states
                .get(&session_id)
                .is_none_or(|state| state.pending_native_requests != 0)
            || self
                .pending_session_chat_image_saves
                .keys()
                .any(|(id, _)| *id == session_id)
            || self.session_account_switch_progress(session_id).is_some()
        {
            return false;
        }
        // A composer with typed text or attached images is unsent user content
        // that lives in the page. The page reports its emptiness on mount, on
        // every empty↔non-empty transition, and again on composer blur (the
        // moment the surface is hidden). Eviction requires an explicit "empty"
        // report: a missing entry means the report was lost or the page never
        // finished loading, and unknown must never read as "empty" to a pass
        // that destroys pages. The ready check keeps the page's bridge
        // registration as a second precondition.
        if !self
            .session_chat_composer_ready_sessions
            .contains(&session_id)
            || (require_empty
                && self.session_chat_composer_empty_reports.get(&session_id) != Some(&true))
        {
            return false;
        }
        // An armed delayed send is a promise to type into this session later.
        if self.agents_delayed_send_timers.contains_key(&session_id)
            || self
                .agents_send_when_stopped_watchers
                .contains_key(&session_id)
        {
            return false;
        }
        // In-flight handoffs and one-shot composer messages all terminate at a
        // page that has to still be there to receive them.
        if self
            .pending_session_chat_draft_handoffs
            .contains(&session_id)
            || self
                .pending_session_terminal_composer_insert
                .contains_key(&session_id)
            || self
                .pending_session_chat_received_drafts
                .contains_key(&session_id)
            || self
                .session_chat_draft_capture_in_flight
                .contains(&session_id)
            || self.pending_keyboard_handoff_targets_session(session_id)
            || self
                .pending_session_chat_composer_insert
                .contains_key(&session_id)
        {
            return false;
        }
        true
    }

    pub(crate) fn start_agents_chat_surface_eviction_polling(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_AGENTS_CHAT_SURFACE_EVICT_POLL_INTERVAL)
                    .await;

                if this
                    .update(cx, |this, cx| {
                        this.evict_expired_hidden_agents_chat_surfaces(cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    #[track_caller]
    pub(crate) fn remove_agents_chat_surface_for_session(
        &mut self,
        session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
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
        }
        self.agents_chat_mode_sessions.remove(&session_id);
        self.session_chat_composer_ready_sessions
            .remove(&session_id);
        self.session_chat_composer_empty_reports.remove(&session_id);
        self.agents_chat_surface_hidden_since.remove(&session_id);
        self.agents_chat_page_states.remove(&session_id);
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
        if let Some(surface) = self.agents_chat_surfaces.remove(&session_id) {
            surface.update(cx, |surface, _| surface.set_visible(false));
        }
    }

    /*
    CDXC:SessionChat 2026-08-26:
    An active-project switch parks the outgoing project's chat pages instead of
    destroying them, the same treatment the terminal runtime already gets on
    that path. Destroying them closed every Chromium browser and made the next
    reconcile reload chat.html from scratch, which is the visible kill + reload
    on every project switch.

    The surfaces and every companion map keyed by the same project-local shell
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
        let protected_sessions = self
            .agents_delayed_send_timers
            .keys()
            .chain(self.agents_send_when_stopped_watchers.keys())
            .chain(self.pending_session_chat_draft_handoffs.iter())
            .chain(self.pending_session_terminal_composer_insert.keys())
            .chain(
                self.pending_session_chat_image_saves
                    .keys()
                    .map(|(session_id, _)| session_id),
            )
            .copied()
            .collect();
        self.agents_chat_mode_sessions.clear();
        self.pending_agents_chat_launch_intents.clear();
        self.pending_session_terminal_composer_insert.clear();
        self.pending_session_chat_draft_handoffs.clear();
        self.session_chat_draft_capture_in_flight.clear();
        self.pending_session_chat_received_drafts.clear();
        for (session_id, surface) in &self.agents_chat_surfaces {
            self.record_session_chat_lifecycle(
                *session_id,
                "sessionChat.nativePageParked",
                "projectSwitch",
            );
            surface.update(cx, |surface, _| surface.set_visible(false));
            // A parked surface is hidden by definition, so it must carry the
            // eviction clock into the park or it would age forever. Same
            // `or_insert_with` contract as the reconcile pass: a surface that is
            // already aging keeps its original stamp.
            self.agents_chat_surface_hidden_since
                .entry(*session_id)
                .or_insert_with(Instant::now);
        }
        ParkedAgentsChatRuntime {
            auto_switch_observed_sessions: std::mem::take(
                &mut self.agents_chat_auto_switch_observed_sessions,
            ),
            page_states: std::mem::take(&mut self.agents_chat_page_states),
            protected_sessions,
            surfaces: std::mem::take(&mut self.agents_chat_surfaces),
            surface_hidden_since: std::mem::take(&mut self.agents_chat_surface_hidden_since),
            composer_ready_sessions: std::mem::take(&mut self.session_chat_composer_ready_sessions),
            composer_empty_reports: std::mem::take(&mut self.session_chat_composer_empty_reports),
            pending_composer_insert: std::mem::take(&mut self.pending_session_chat_composer_insert),
        }
    }

    /// Reinstall a project's parked chat pages as the live ones. The caller has
    /// already restored that project's `WorkspaceModel` and session mappings, so
    /// the restored surfaces match live session ids and `reconcile_agents_chat_surfaces`
    /// makes them visible again through `ensure_agents_chat_surface` without
    /// recreating a browser.
    pub(crate) fn restore_parked_agents_chat_surfaces(
        &mut self,
        parked: ParkedAgentsChatRuntime,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_chat_auto_switch_observed_sessions = parked.auto_switch_observed_sessions;
        self.agents_chat_page_states = parked.page_states;
        self.agents_chat_surfaces = parked.surfaces;
        self.agents_chat_surface_hidden_since = parked.surface_hidden_since;
        self.session_chat_composer_ready_sessions = parked.composer_ready_sessions;
        self.session_chat_composer_empty_reports = parked.composer_empty_reports;
        self.pending_session_chat_composer_insert = parked.pending_composer_insert;
        /*
        CDXC:SessionChat 2026-07-31 (extended 2026-08-26):
        A parked page still holds whichever gxserver bootstrap it had when it
        went hidden, and a remote chat page points at an SSH tunnel whose local
        port and token can be rebuilt while its project is away. Re-push each
        restored page's bootstrap from its own session identity, so a restored
        remote chat cannot keep talking to a dead tunnel.
        */
        for (session_id, surface) in &self.agents_chat_surfaces {
            let bootstrap = self.agents_session_chat_gxserver_bootstrap(*session_id);
            self.record_session_chat_lifecycle(
                *session_id,
                "sessionChat.nativePageRestored",
                "projectSwitch",
            );
            surface.update(cx, |surface, _| {
                surface.refresh_session_chat_gxserver_bootstrap(bootstrap);
            });
        }
    }
}
