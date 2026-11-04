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

/// Anything typed, sent, or queued makes the chat the user's, so a new launch no longer reuses it.
fn agent_chat_untouched(view: &NativeChatView) -> bool {
    view.draft.trim().is_empty()
        && view.items.is_empty()
        && view.snapshot["queue"]["prompts"]
            .as_array()
            .is_none_or(Vec::is_empty)
}

impl GhostexGpuiApp {
    /// New Agent Session: the New Thread picker's first row (the last-used agent) without the
    /// picker. Before any agent is known the picker opens instead, so the key still does something.
    pub(crate) fn start_new_agent_session(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(agent) = self.last_used_agent() else {
            self.open_gpui_new_thread_picker(cx);
            return;
        };
        match self.new_agent_session_box_default(&agent, cx) {
            // No status has been answered yet (just after start): decide once the read finishes.
            Some(default) if crate::app::gx_store::cached_agentbox_locations().is_none() => {
                self.with_agentbox_locations(
                    move |this, _, cx| this.start_new_agent_session_at(agent, Some(default), cx),
                    cx,
                );
            }
            default => self.start_new_agent_session_at(agent, default, cx),
        }
    }

    /// CDXC:AgentBox 2026-10-01 WHY:
    /// The Cloud Boxes default reads "Where new threads run unless you pick another location", so New Agent Session starts where the New Thread picker would, in that default location. A box default that this agent cannot use (agentbox runs only Claude, Codex, OpenCode and Pi) or that the status did not report ready starts on this computer with a toast saying why; before the first status answer the start waits for it rather than guessing. A project on another machine always starts there, because this computer's status says nothing about its boxes.
    fn new_agent_session_box_default(
        &mut self,
        agent: &NewThreadPickerAgent,
        cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        let default =
            shared_settings::shared_sidebar_settings_snapshot().agentbox_default_location();
        if !ghostex_gx_core::is_agentbox_run_location(Some(&default))
            || self.gx_store_active_project_is_remote()
        {
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

    /// Starts `agent` in `box_default` when the status reports it ready, otherwise on this computer.
    fn start_new_agent_session_at(
        &mut self,
        agent: NewThreadPickerAgent,
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
        // An empty chat left by a launcher is a session on this computer, so only a local start
        // goes back to it.
        if run_location.is_none() && self.focus_untouched_agent_chat(&agent.agent_id, cx) {
            return;
        }
        self.launch_agent_in_active_project(agent.agent_id, None, run_location, cx);
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
        let place = self.gx_store_agent_choice_place(None);
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

    fn last_used_agent(&self) -> Option<NewThreadPickerAgent> {
        let hud_agents = self.new_thread_picker_agents.clone().or_else(|| {
            self.native_sidebar
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.hud["agents"].as_array().cloned())
        })?;
        let primary = self.active_project_agent_id(&hud_agents);
        order_new_thread_picker_agents(&hud_agents, primary.as_deref())
            .into_iter()
            .next()
    }

    /// Starts an agent in the active project the way a New Thread picker row does: the tab opens
    /// at once and gxserver creates the session behind it, on this computer or in the agentbox box
    /// `run_location` names.
    pub(crate) fn launch_agent_in_active_project(
        &mut self,
        agent_id: String,
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
        // A session the Unified header's New session aimed somewhere (gx_store/unified_new_session.rs).
        if let Some(group_id) = self.gx_store.unified_sidebar.picker_target.take() {
            message["groupId"] = json!(group_id);
        }
        self.stage_agent_launch_placeholder(&message, cx);
        self.focus_staged_chat_after_picker(cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// CDXC:AgentLauncher 2026-10-15 DECISION:
    /// User: "don't close on leave, you should be able to have 1 session that doesn't have anything in it that's fine, i just don't like having multiple like that basically". Leaving a chat an agent launcher opened never closes it, whatever it holds and whatever the settings say; starting the same agent again in that project while it is still empty goes back to it instead of stacking a second empty one. This supersedes the 2026-09-24 decision that closed such a chat as soon as the user left it, which issue #236 found closing sessions with `closeEmptySessionsOnNew` off. The only empty-session cleanup left is that opt-in setting's (CDXC:Sessions 2026-10-09 in server/src/empty_session_cleanup.rs).
    /// Terminal-interface agents and plain terminals are not tracked: there is no reliable "nothing sent yet" signal for them.
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

    fn focus_untouched_agent_chat(&mut self, agent_id: &str, cx: &mut gpui::Context<Self>) -> bool {
        let active_project_id = self.agents_workspace_project_id.as_deref();
        let Some(chat) = self.untouched_agent_chats.iter().find(|chat| {
            active_project_id == Some(chat.project_id.as_str()) && chat.agent_id == agent_id
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
