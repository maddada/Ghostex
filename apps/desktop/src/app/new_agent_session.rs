//! New Agent Session (Cmd+Shift+O) and going back to a chat an agent launcher opened that is still empty.
//! SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs (CDXC:Hotkeys 2026-09-25), apps/desktop/src/app/hotkeys.rs.
use super::native_chat::state::NativeChatView;
use super::new_thread_picker_lifecycle::order_new_thread_picker_agents;
use crate::app::window::NewThreadPickerAgent;
use crate::*;
use serde_json::json;

/// A chat an agent launcher opened in which nothing has been typed or sent yet.
pub(crate) struct UntouchedAgentChat {
    shell_session_id: TerminalSessionId,
    /// Shell ids are per project workspace, so the view identity tells this chat apart from
    /// another project's tab that reuses its id.
    view_id: gpui::EntityId,
    project_id: String,
    agent_id: String,
}

/// Anything typed, sent, queued, or changed makes the chat the user's.
fn agent_chat_untouched(view: &NativeChatView) -> bool {
    view.is_untouched_draft()
}

impl GhostexGpuiApp {
    /// New session (sidebar button, Cmd+N, Cmd+Shift+O): immediately creates a draft in the open
    /// session's project (or Main Hub when there is none), selects it under Drafts, and opens its
    /// empty chat with no picker.
    pub(crate) fn start_new_agent_session(&mut self, cx: &mut gpui::Context<Self>) {
        let target_group_id = self.gx_store_new_session_default_group_id();
        let Some(agent) = self.last_used_agent_for_group(target_group_id.as_deref()) else {
            return;
        };
        match self.new_agent_session_box_default(&agent, target_group_id.as_deref(), cx) {
            // No status has been answered yet (just after start): decide once the read finishes.
            Some(default) if crate::app::gx_store::cached_agentbox_locations().is_none() => {
                self.with_agentbox_locations(
                    move |this, _, cx| {
                        this.start_new_agent_session_at(agent, target_group_id, Some(default), cx)
                    },
                    cx,
                );
            }
            default => self.start_new_agent_session_at(agent, target_group_id, default, cx),
        }
    }

    /// CDXC:AgentBox 2026-10-01 WHY:
    /// The Cloud Boxes default reads "Where new threads run unless you pick another location", so New Agent Session starts where the New Thread picker would, in that default location. A box default that this agent cannot use (agentbox runs only Claude, Codex, OpenCode and Pi) or that the status did not report ready starts on this computer with a toast saying why; before the first status answer the start waits for it rather than guessing. A project on another machine always starts there, because this computer's status says nothing about its boxes.
    fn new_agent_session_box_default(
        &mut self,
        agent: &NewThreadPickerAgent,
        target_group_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        let default =
            shared_settings::shared_sidebar_settings_snapshot().agentbox_default_location();
        let target_is_remote = target_group_id
            .and_then(ghostex_gx_core::ProjectKey::parse_sidebar_group_id)
            .map(|project| !project.machine.is_local())
            .unwrap_or_else(|| self.gx_store_active_project_is_remote());
        if !ghostex_gx_core::is_agentbox_run_location(Some(&default)) || target_is_remote {
            return None;
        }
        if !crate::app::window::runs_in_box(agent) {
            let title = format!(
                "{} can't run in a box, so it started on this computer",
                agent.name
            );
            self.dispatch_gpui_workspace_action_toast("info", &title, "", cx);
            return None;
        }
        Some(default)
    }

    /// Starts `agent` in `target_group_id` (and `box_default` when ready), or focuses the already
    /// open untouched draft if the user is already sitting on one in that project.
    fn start_new_agent_session_at(
        &mut self,
        agent: NewThreadPickerAgent,
        target_group_id: Option<String>,
        box_default: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let run_location = box_default.filter(|default| {
            let ready = crate::app::gx_store::cached_agentbox_locations()
                .is_some_and(|locations| locations.find(default).is_some());
            if !ready {
                let title = format!(
                    "{} isn't ready, so it started on this computer",
                    ghostex_gx_core::agentbox_location_label(default)
                );
                self.dispatch_gpui_workspace_action_toast("info", &title, "", cx);
            }
            ready
        });
        let target_local_project_id = target_group_id
            .as_deref()
            .and_then(ghostex_gx_core::ProjectKey::parse_sidebar_group_id)
            .filter(|project| project.machine.is_local())
            .map(|project| project.project_id);
        // If already on an untouched draft in the target project for the same agent, focus its
        // composer instead of closing and recreating it.
        if run_location.is_none()
            && self.focus_untouched_agent_chat(
                &agent.agent_id,
                target_local_project_id.as_deref(),
                cx,
            )
        {
            return;
        }
        self.launch_agent_in_group(agent.agent_id, target_group_id, None, run_location, cx);
    }

    /// Every `runSidebarAgent` launch calls this as it stores the launcher keys
    /// (gx_store/create/agent.rs), so this order follows every launcher, not only the picker.
    pub(crate) fn note_primary_agent_launcher(
        &mut self,
        agent_id: &str,
        choices: &ghostex_gx_core::AgentChoices,
    ) {
        self.sidebar_primary_agent_launcher_id = Some(agent_id.to_string());
        self.gx_store_note_agent_choices(choices);
    }

    /// The agent the active project starts, among `hud_agents`: the one remembered for it (per
    /// project, per Space or everywhere, as Settings > Agents says), else the one picked last.
    pub(crate) fn active_project_agent_id(
        &self,
        hud_agents: &[serde_json::Value],
    ) -> Option<String> {
        self.project_agent_id_for_group(hud_agents, None)
    }

    /// The remembered agent for `group_id` (falling back to the active project when `None`),
    /// among `hud_agents`.
    fn project_agent_id_for_group(
        &self,
        hud_agents: &[serde_json::Value],
        group_id: Option<&str>,
    ) -> Option<String> {
        let place = self.gx_store_agent_choice_place(group_id);
        self.gx_store_agent_choices()
            .candidates(&place)
            .into_iter()
            .chain(self.sidebar_primary_agent_launcher_id.as_deref())
            .find(|agent_id| {
                hud_agents
                    .iter()
                    .any(|agent| agent["agentId"].as_str() == Some(*agent_id))
            })
            .map(str::to_string)
    }

    fn last_used_agent_for_group(&self, group_id: Option<&str>) -> Option<NewThreadPickerAgent> {
        let hud_agents = self
            .new_thread_picker_agents
            .clone()
            .or_else(|| {
                self.native_sidebar
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.hud["agents"].as_array().cloned())
            })
            .or_else(|| {
                self.gx_store_launch_hud()
                    .and_then(|hud| hud["agents"].as_array().cloned())
            })?;
        let primary = self.project_agent_id_for_group(&hud_agents, group_id);
        order_new_thread_picker_agents(&hud_agents, primary.as_deref())
            .into_iter()
            .next()
    }

    /// Starts an agent in the active project (or the picker's target): the tab opens at once and
    /// gxserver creates the session behind it, on this computer or in `run_location`.
    pub(crate) fn launch_agent_in_active_project(
        &mut self,
        agent_id: String,
        account_id: Option<String>,
        run_location: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let group_id = self.gx_store.unified_sidebar.picker_target.take();
        self.launch_agent_in_group(agent_id, group_id, account_id, run_location, cx);
    }

    /// Starts an agent in `group_id` (falling back to `gx_store_new_session_default_group_id` when
    /// `None`).
    pub(crate) fn launch_agent_in_group(
        &mut self,
        agent_id: String,
        group_id: Option<String>,
        account_id: Option<String>,
        run_location: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut message = json!({
            "agentId": agent_id,
            "type": "runSidebarAgent",
        });
        if let Some(account_id) = account_id {
            message["accountId"] = json!(account_id);
        }
        if let Some(run_location) = run_location {
            message["runLocation"] = json!(run_location);
        }
        if let Some(group_id) = group_id {
            message["groupId"] = json!(group_id);
        }
        self.stage_agent_launch_placeholder(&message, cx);
        self.focus_staged_chat_after_picker(cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// Tracks an empty agent chat staged by a launcher so pressing New session again while
    /// already sitting on it focuses its composer instead of replacing it.
    pub(crate) fn track_untouched_agent_chat(
        &mut self,
        shell_session_id: TerminalSessionId,
        project_id: String,
        agent_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = self.native_chat_views.get(&shell_session_id).cloned() else {
            return;
        };
        // Drop chats whose tab closed or that the user has written in: they are never reused.
        let tracked = std::mem::take(&mut self.untouched_agent_chats);
        self.untouched_agent_chats = tracked
            .into_iter()
            .filter(|chat| {
                chat.shell_session_id != shell_session_id
                    && self
                        .tracked_agent_chat_view(chat)
                        .is_some_and(|view| agent_chat_untouched(view.read(cx)))
            })
            .collect();
        self.untouched_agent_chats.push(UntouchedAgentChat {
            shell_session_id,
            view_id: view.entity_id(),
            project_id,
            agent_id,
        });
    }

    fn tracked_agent_chat_view(&self, chat: &UntouchedAgentChat) -> Option<Entity<NativeChatView>> {
        self.native_chat_views
            .get(&chat.shell_session_id)
            .filter(|view| view.entity_id() == chat.view_id)
            .cloned()
    }

    fn focus_untouched_agent_chat(
        &mut self,
        agent_id: &str,
        target_project_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let target_project_id = target_project_id.or(self.agents_workspace_project_id.as_deref());
        if self.agents_workspace_project_id.as_deref() != target_project_id {
            return false;
        }
        let Some(chat) = self.untouched_agent_chats.iter().find(|chat| {
            target_project_id == Some(chat.project_id.as_str()) && chat.agent_id == agent_id
        }) else {
            return false;
        };
        let shell_session_id = chat.shell_session_id;
        let Some(view) = self.tracked_agent_chat_view(chat) else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return false;
        };
        if !agent_chat_untouched(view.read(cx)) {
            return false;
        }
        self.select_agents_tab(pane_id, shell_session_id, cx);
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        view.update(cx, |view, cx| {
            view.focus_requested = true;
            cx.notify();
        });
        cx.notify();
        true
    }
}
