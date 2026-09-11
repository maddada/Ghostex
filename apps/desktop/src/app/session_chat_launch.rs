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
                    let preview_key = key.clone();
                    let metadata = background.spawn(async move {
                        gpui_gxserver_rpc_result("/api/attachSessionMetadata", &serde_json::json!({
                            "projectId": preview_key.project_id, "sessionId": preview_key.session_id,
                        }), std::time::Duration::from_secs(15))
                    }).await;
                    let _ = this.update(cx, |this, cx| {
                        this.swap_agents_workspace_to_project_id(Some(key.project_id.clone()), cx);
                        this.local_workspace_latest_focus_key = Some(key.clone());
                        this.local_workspace_attach_pending.insert(key.clone());
                        let workspace_key = GpuiWorkspaceTerminalSessionKey::Local(key.clone());
                        this.pending_agents_chat_launch_intents.insert(workspace_key.clone());
                        if let Ok(metadata) = metadata {
                            this.show_pending_agents_chat_launch(workspace_key, &metadata, this.agents_workspace.focused_pane, cx);
                        }
                    });
                    background.spawn(async move {
                        gpui_prepare_local_workspace_attach_terminal_plan(&key, GpuiLocalWorkspaceAttachIntent::Attach).map(|plan| (key, plan))
                    }).await
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
                        if this.open_gpui_local_workspace_terminal(key, plan, this.agents_workspace.focused_pane, false, cx) {
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

    /// CDXC:SessionChat 2026-09-09 DECISION:
    /// User: chat opens immediately when creating an agent; terminal startup runs in the background and sending waits for the agent's input box.
    /// This tab owns chat before it has a terminal launch payload. The ordinary attach completion fills that same tab.
    pub(crate) fn show_pending_agents_chat_launch(
        &mut self,
        key: GpuiWorkspaceTerminalSessionKey,
        metadata: &serde_json::Value,
        requested_pane_id: WorkspacePaneId,
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
        if !matches!(
            icon,
            Some(
                "antigravity-cli"
                    | "claude"
                    | "openclaude"
                    | "codex"
                    | "cursor-cli"
                    | "grok-build"
                    | "hermes-agent"
                    | "pi"
                    | "omp"
            )
        ) {
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
        self.change_active_mode_with_pane_state(TitlebarMode::Agents, cx);
        self.activate_preferred_agents_chat_launch_intent(session_id, cx);
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        cx.notify();
    }
}
