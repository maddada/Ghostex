use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
use serde_json::Value;
use std::time::{Duration, Instant};

/// How long a terminal-only placeholder waits before it is taken down.
const AGENT_LAUNCH_PLACEHOLDER_TIMEOUT: Duration = Duration::from_secs(20);

/// A tab opened by an agent launcher before gxserver has created the session.
pub(crate) struct AgentLaunchPlaceholder {
    project_id: String,
    shell_session_id: TerminalSessionId,
    staged_at: Instant,
}

impl GhostexGpuiApp {
    /// CDXC:AgentLauncher 2026-09-22 WHY:
    /// The sidebar publishes its session list before the create operation's focus message. Reconciling that list first deleted the unmapped composer and allocated a different tab for the new server ID, losing text typed during launch. Keep the local tab until the focus message binds its identity.
    pub(crate) fn has_unbound_agent_chat_launch(&self) -> bool {
        self.agent_launch_placeholders.iter().any(|placeholder| {
            self.agents_workspace_project_id.as_deref() == Some(placeholder.project_id.as_str())
                && self
                    .native_chat_views
                    .contains_key(&placeholder.shell_session_id)
                && self
                    .agents_workspace
                    .pane_id_for_session(placeholder.shell_session_id)
                    .is_some()
        })
    }

    /// CDXC:AgentLauncher 2026-09-19 DECISION:
    /// User: creating an agent session from the sidebar must show it instantly; all the processing runs in the background, and a send that comes too early already waits for the agent's input box.
    /// The click used to reach the pane only after the hook status check, the create call, the focus bridge and the attach metadata read. Now the header click itself opens a mounting tab carrying the agent's name and mark, in Chat when that agent prefers it, and the created session adopts that tab when its focus message arrives (`adopt_agent_launch_placeholder`), so the ordinary attach completion fills it in place. Terminal-only placeholders expire; an editable chat stays available through slow creation. Remote projects and Windows keep their own launch paths.
    pub(crate) fn stage_agent_launch_placeholder(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let picker_launch = command["type"] == "runSidebarAgent";
        let launch = picker_launch
            || matches!(
                (command["type"].as_str(), command["action"].as_str()),
                (Some("projectAction"), Some("agent")) | (Some("agentAccounts"), Some("launch"))
            );
        let Some(agent_id) = command["agentId"].as_str() else {
            return;
        };
        if !launch || cfg!(target_os = "windows") {
            return;
        }
        self.agent_launch_placeholders.retain(|placeholder| {
            self.agents_workspace
                .pane_id_for_session(placeholder.shell_session_id)
                .is_some()
        });
        let Some(snapshot) = self.native_sidebar.snapshot.clone() else {
            return;
        };
        let Some(group) = snapshot.groups.iter().find(|group| {
            match command["groupId"].as_str() {
                Some(group_id) => group.group_id == group_id,
                // The New Thread picker targets the active group.
                None => picker_launch && group.is_active,
            }
        }) else {
            return;
        };
        if group.remote_machine_context.is_some() {
            return;
        }
        let Some(project_id) = group
            .project_context
            .as_ref()
            .and_then(|project| project["editor"]["projectId"].as_str())
            .map(str::to_owned)
        else {
            return;
        };
        if self.agents_workspace_project_id.as_deref() != Some(project_id.as_str()) {
            return;
        }
        let agent = snapshot.hud["agents"]
            .as_array()
            .and_then(|agents| agents.iter().find(|agent| agent["agentId"] == agent_id));
        let name = agent
            .and_then(|agent| agent["name"].as_str())
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or("Agent");
        let icon = gpui_sidebar_agent_icon(agent.and_then(|agent| agent["icon"].as_str()));
        let focused_pane = self.agents_workspace.focused_pane;
        let Some(shell_session_id) = self
            .agents_workspace
            .add_mounting_session_to_pane(focused_pane)
        else {
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == shell_session_id)
        {
            session.title = format!("{name} Session");
            session.agent_icon = icon;
            // Not a startup-pipeline candidate: the created session's attach owns this tab.
            session.set_presentation_state_with_startup_eligibility(
                TerminalSessionPresentationState::Mounting,
                false,
            );
        }
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let interface =
            gpui_preferred_agent_interface_override_from_settings(settings.object(), agent_id)
                .unwrap_or_else(|| {
                    gpui_effective_preferred_agent_interface_for_agent_icon(settings.object(), icon)
                });
        if interface == GpuiPreferredAgentInterface::Chat
            && self
                .agents_session_chat_transcript_agent(shell_session_id)
                .is_some()
        {
            self.agents_chat_mode_sessions.insert(shell_session_id);
            self.stage_native_chat_launch(shell_session_id, &project_id, agent_id, name, icon, cx);
            self.track_untouched_agent_chat(
                shell_session_id,
                project_id.clone(),
                agent_id.to_string(),
                cx,
            );
        }
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return;
        };
        // CDXC:Workarea 2026-09-20 WHY:
        // The launch keeps the view panel open (CDXC:AgentLauncher 2026-09-14) and the new session's
        // pane is on screen beside it either way, so the pane takes focus without a view switch.
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.reconcile_agents_pane_surfaces(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.agent_launch_placeholders
            .push_back(AgentLaunchPlaceholder {
                project_id,
                shell_session_id,
                staged_at: Instant::now(),
            });
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.agentLaunchPlaceholderStaged",
            serde_json::json!({
                "epochMs": support_logs::temporary_epoch_ms(),
                "agentId": agent_id,
                "chat": self.agents_chat_mode_sessions.contains(&shell_session_id),
            }),
        );
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(AGENT_LAUNCH_PLACEHOLDER_TIMEOUT)
                .await;
            let _ = this.update(cx, |this, cx| {
                this.expire_agent_launch_placeholder(shell_session_id, cx)
            });
        })
        .detach();
    }

    /// The created session's first focus message takes over the placeholder staged for its project, so the attach reuses that tab instead of opening a second one.
    pub(crate) fn adopt_agent_launch_placeholder(
        &mut self,
        message: &GpuiSidebarWorkspaceTerminalFocusMessage,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agent_launch_placeholders.is_empty()
            || !message.keep_view
            || message.placement != GpuiWorkspaceTerminalFocusPlacement::Tab
            || message.placement_target_session_id.is_some()
            || message.force_remount
            || message.startup_restore
        {
            return;
        }
        let key = GpuiLocalWorkspaceSessionKey::from(message);
        if self.local_workspace_session_mappings.contains_key(&key) {
            return;
        }
        let Some(index) = self
            .agent_launch_placeholders
            .iter()
            .position(|placeholder| {
                placeholder.project_id == message.project_id
                    && (placeholder.staged_at.elapsed() < AGENT_LAUNCH_PLACEHOLDER_TIMEOUT
                        || self
                            .native_chat_views
                            .contains_key(&placeholder.shell_session_id))
                    && self
                        .agents_workspace
                        .pane_id_for_session(placeholder.shell_session_id)
                        .is_some()
            })
        else {
            return;
        };
        let Some(placeholder) = self.agent_launch_placeholders.remove(index) else {
            return;
        };
        self.local_workspace_session_mappings
            .insert(key.clone(), placeholder.shell_session_id);
        self.local_app_shot_session_mappings
            .insert(key.session_id.clone(), placeholder.shell_session_id);
        // Bind the composer created by the click as soon as the server identity arrives.
        if self
            .agents_chat_mode_sessions
            .contains(&placeholder.shell_session_id)
        {
            self.reconcile_agents_pane_surfaces(cx);
            cx.notify();
        }
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.agentLaunchPlaceholderAdopted",
            serde_json::json!({
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": key.project_id,
                "sessionId": key.session_id,
                "stagedMs": placeholder.staged_at.elapsed().as_millis() as u64,
            }),
        );
    }

    fn expire_agent_launch_placeholder(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(index) = self
            .agent_launch_placeholders
            .iter()
            .position(|placeholder| placeholder.shell_session_id == shell_session_id)
        else {
            return;
        };
        if self.native_chat_views.contains_key(&shell_session_id) {
            return;
        }
        self.agent_launch_placeholders.remove(index);
        if self
            .local_workspace_session_mappings
            .values()
            .any(|mapped| *mapped == shell_session_id)
        {
            return;
        }
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return;
        };
        self.agents_chat_mode_sessions.remove(&shell_session_id);
        self.remove_agents_chat_surface_for_session(shell_session_id, cx);
        self.agents_workspace.close_tab(pane_id, shell_session_id);
        self.persist_shell_layout_state();
        cx.notify();
    }
}
