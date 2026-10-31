use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    #[cfg(target_os = "windows")]
    pub(crate) fn request_windows_agent_chat_launch(
        &mut self,
        message: GpuiSidebarCreateProjectAgentMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let request_id = message.request_id;
            let created = background.spawn(async move {
                gpui_create_local_project_workspace_agent_record(&message.project_id, &message.agent_id, message.account_id.as_deref())
            }).await;
            let mut attach_key = None;
            let result = match created {
                Ok(key) => {
                    attach_key = Some(key.clone());
                    // CDXC:AgentLauncher 2026-10-04 WHY:
                    // The agent CLI's own startup is the longest wait before a new chat's composer can show its model and status line, so the provider start (inside the attach plan) runs beside the read that mounts the chat instead of after it.
                    let plan_key = key.clone();
                    let plan = background.spawn(async move {
                        gpui_prepare_local_workspace_attach_terminal_plan(&plan_key, GpuiLocalWorkspaceAttachIntent::Attach).map(|plan| (plan_key, plan))
                    });
                    let preview_key = key.clone();
                    let metadata = background.spawn(async move {
                        gpui_gxserver_rpc_result("/api/attachSessionMetadata", &serde_json::json!({
                            "projectId": preview_key.project_id, "sessionId": preview_key.session_id,
                        }), std::time::Duration::from_secs(15))
                    }).await;
                    let _ = this.update(cx, |this, cx| {
                        this.gx_store_take_created_session(&key, cx);
                        this.local_workspace_latest_focus_key = Some(key.clone());
                        this.local_workspace_attach_pending.insert(key.clone());
                        let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key.clone());
                        this.pending_agents_chat_launch_intents.insert(workspace_key.clone());
                        if let Ok(metadata) = metadata {
                            this.show_pending_agents_chat_launch(workspace_key, &metadata, this.agents_workspace.focused_pane, true, cx);
                        }
                    });
                    plan.await
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(key) = attach_key.as_ref() {
                    this.local_workspace_attach_pending.remove(key);
                }
                let outcome = match result {
                    Ok((key, plan)) => {
                        if this.local_workspace_latest_focus_key.as_ref() != Some(&key) {
                            return;
                        }
                        let opened = this.open_gpui_local_workspace_terminal(key, plan, this.agents_workspace.focused_pane, false, cx);
                        if opened {
                            Ok(())
                        } else {
                            Err("Ghostex could not open the new agent session.".to_string())
                        }
                    }
                    Err(error) => Err(error),
                };
                if let Err(error) = &outcome {
                    this.dispatch_gpui_app_modal_toast("warning", "Agent unavailable", error, cx);
                }
                if let Some(request_id) = request_id.as_deref() {
                    this.dispatch_gpui_first_launch_create_project_session_result(request_id, outcome.is_ok(), outcome.as_ref().err().map(String::as_str), cx);
                }
            });
        }).detach();
    }

    /// CDXC:Drafts 2026-09-25 WHY:
    /// A create that opens in Chat arms its launch intent here rather than leaving it to `focus_local_workspace_terminal_from_message`, which arms it only for a session with no tab so a tab the user put back in Terminal stays there. The store's selection of the created session runs first and publishes the tab list, so the session already has its tab when that rule looks; the intent was skipped, the chat opened through the selection's Chat adoption, which never claims the staged first-input draft, and gxserver typed Handoff / Export's transcript link into the hidden terminal instead of the chat composer. That tab is the create's own, not a view the user chose.
    pub(crate) fn arm_created_session_chat_launch_intent(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
    ) {
        self.pending_agents_chat_launch_intents
            .insert(GpuiWorkspaceTerminalSessionKey::Local(key));
    }

    /// CDXC:SessionChat 2026-09-30 DECISION:
    /// User: when the Default Agent View is Chat, open chat right away wherever the app opens a session, not only for a sidebar agent launch ("check for other cases where we can open chat right away"). An open that names no view (Find's resume and fork, a Ghostex Capture send, a Project Board start or jump, reopening a stashed prompt's conversation, a fork, a Previous Sessions restore) arms the Chat launch intent at once, so the chat is on screen while the terminal starts behind it instead of after the agent is recognised. Which agent the session runs is only known once gxserver's attach metadata or the attach plan names it, so the intent is dropped there when that agent's view is Terminal (`chat_launch_intent_declined_by_default_view`).
    pub(crate) fn arm_default_view_chat_launch_intent(
        &mut self,
        key: GpuiWorkspaceTerminalSessionKey,
    ) {
        self.pending_agents_chat_launch_follow_view
            .insert(key.clone());
        self.pending_agents_chat_launch_intents.insert(key);
    }

    /// `arm_default_view_chat_launch_intent` for a local session the store opened (a create or a fork).
    pub(crate) fn arm_local_default_view_chat_launch_intent(
        &mut self,
        key: GpuiLocalWorkspaceSessionKey,
    ) {
        self.arm_default_view_chat_launch_intent(GpuiWorkspaceTerminalSessionKey::Local(key));
    }

    /// Whether an intent armed by `arm_default_view_chat_launch_intent` gives way because the
    /// agent behind `icon` opens in the terminal; the intent is dropped when it does. An intent
    /// armed with an explicit view is never declined here.
    pub(crate) fn chat_launch_intent_declined_by_default_view(
        &mut self,
        key: &GpuiWorkspaceTerminalSessionKey,
        icon: Option<&str>,
    ) -> bool {
        if !self.pending_agents_chat_launch_follow_view.remove(key) {
            return false;
        }
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        if gpui_effective_preferred_agent_interface_for_agent_icon(settings.object(), icon)
            == GpuiPreferredAgentInterface::Chat
        {
            return false;
        }
        self.pending_agents_chat_launch_intents.remove(key);
        true
    }

    /// CDXC:SessionChat 2026-09-09 DECISION:
    /// User: chat opens immediately when creating an agent; terminal startup runs in the background and sending waits for the agent's input box.
    /// This tab owns chat before it has a terminal launch payload. The ordinary attach completion fills that same tab.
    pub(crate) fn show_pending_agents_chat_launch(
        &mut self,
        key: GpuiWorkspaceTerminalSessionKey,
        metadata: &serde_json::Value,
        requested_pane_id: WorkspacePaneId,
        keep_view: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.pending_agents_chat_launch_intents.contains(&key) {
            return;
        }
        let expected_project = match &key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => key.project_id.clone(),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                gpui_remote_scoped_project_id(&key.remote_machine_id, &key.project_id)
            }
        };
        if self.agents_workspace_project_id.as_deref() != Some(expected_project.as_str()) {
            return;
        }
        if let GpuiWorkspaceTerminalSessionKey::Remote(remote) = &key {
            let focused = gpui_remote_scoped_session_id(
                &remote.remote_machine_id,
                &remote.project_id,
                &remote.session_id,
            );
            if self
                .sidebar_gxserver_presentation_focus_state
                .focused_session_id
                .as_deref()
                != Some(focused.as_str())
            {
                return;
            }
        }
        let Some(attach) = metadata
            .get("attach")
            .and_then(serde_json::Value::as_object)
        else {
            return;
        };
        if gpui_validate_local_workspace_attach_not_restore_blocked(attach).is_err() {
            return;
        }
        let icon = gpui_workspace_attach_agent_icon(attach);
        if icon
            .and_then(crate::app::session_chat_surfaces::session_chat_agent_for_icon)
            .is_none()
        {
            return;
        }
        if self.chat_launch_intent_declined_by_default_view(&key, icon) {
            return;
        }
        let mapped = match &key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => {
                self.local_workspace_session_mappings.get(key)
            }
            GpuiWorkspaceTerminalSessionKey::Remote(key) => self.remote_attach_sessions.get(key),
        }
        .copied();
        let session_id = match mapped {
            Some(id) if self.agents_workspace.pane_id_for_session(id).is_some() => id,
            _ => {
                let Some(id) = self
                    .agents_workspace
                    .add_mounting_session_to_pane(requested_pane_id)
                else {
                    return;
                };
                let session = self
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == id)
                    .unwrap();
                session.title = gpui_workspace_attach_title(attach);
                session.agent_icon = icon;
                session.set_presentation_state_with_startup_eligibility(
                    TerminalSessionPresentationState::Mounting,
                    false,
                );
                match &key {
                    GpuiWorkspaceTerminalSessionKey::Local(key) => {
                        self.local_workspace_session_mappings
                            .insert(key.clone(), id);
                    }
                    GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                        self.remote_attach_sessions.insert(key.clone(), id);
                    }
                }
                id
            }
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.agent_icon = icon;
        }
        self.agents_workspace.select_tab(pane_id, session_id);
        let _ = keep_view;
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }
}
