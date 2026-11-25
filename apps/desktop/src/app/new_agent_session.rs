//! One-click New Session (sidebar button, Cmd+N), cached launcher agents/accounts, and returning
//! to an untouched agent draft in the same project.
//! SEE-ALSO: packages/settings-catalog/src/hotkey_definitions.rs, apps/desktop/src/app/hotkeys.rs.
use super::native_chat::state::NativeChatView;
use crate::*;
use serde_json::{Value, json};
use std::time::Duration;

/// Minimal agent record ordered from the sidebar HUD for launching a new draft.
#[derive(Clone, Debug)]
struct LaunchableAgent {
    agent_id: String,
    name: String,
    icon: Option<String>,
}

/// Registered account entry extracted from `/api/agentAccounts` for Claude Cloud controls.
#[derive(Clone, Debug)]
pub(crate) struct CachedAgentAccount {
    pub(crate) id: String,
    pub(crate) provider: String,
    pub(crate) name: String,
    pub(crate) ready: bool,
}

fn launchable_agent_from_hud(value: &Value) -> Option<LaunchableAgent> {
    let agent_id = value.get("agentId")?.as_str()?.trim();
    let name = value.get("name")?.as_str()?.trim();
    if agent_id.is_empty() || name.is_empty() {
        return None;
    }
    let icon = value
        .get("icon")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .map(str::to_string);
    Some(LaunchableAgent {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        icon,
    })
}

fn order_launcher_agents(
    hud_agents: &[Value],
    primary_agent_id: Option<&str>,
) -> Vec<LaunchableAgent> {
    let mut agents: Vec<LaunchableAgent> = hud_agents
        .iter()
        .filter_map(launchable_agent_from_hud)
        .collect();
    if let Some(primary_index) = primary_agent_id
        .and_then(|primary| agents.iter().position(|agent| agent.agent_id == primary))
    {
        let primary = agents.remove(primary_index);
        agents.insert(0, primary);
    }
    agents
}

/// The registered accounts of `/api/agentAccounts` that the user has not disabled.
pub(crate) fn registered_agent_accounts(state: &Value) -> Vec<CachedAgentAccount> {
    state["accounts"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|account| {
            account["registered"].as_bool() == Some(true)
                && account["userDisabled"].as_bool() != Some(true)
        })
        .filter_map(|account| {
            let id = account["id"].as_str()?.to_string();
            let provider = account["provider"].as_str()?.to_string();
            Some(CachedAgentAccount {
                name: crate::app::titlebar::account_usage::account_display_name(account),
                ready: account["status"].as_str() == Some("ready"),
                id,
                provider,
            })
        })
        .collect()
}

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
    /// Reads the sidebar HUD agent buttons in the background and caches them.
    pub(crate) fn refresh_gpui_new_thread_picker_agents(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_agents_refresh_in_flight {
            return;
        }
        self.new_thread_picker_agents_refresh_in_flight = true;
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let agents = background
                .spawn(async move {
                    gpui_sidebar_hud_from_gxserver(
                        Duration::from_secs(5),
                        active_project_id.as_deref(),
                    )
                    .ok()
                    .and_then(|hud| hud.agents.as_array().cloned())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.new_thread_picker_agents_refresh_in_flight = false;
                let Some(agents) = agents else {
                    return;
                };
                this.new_thread_picker_agents = Some(agents);
                this.push_gpui_export_transcript_modal_agents(cx);
            });
        })
        .detach();
    }

    /// Reads the local gxserver accounts list in the background and caches it.
    pub(crate) fn refresh_gpui_new_thread_picker_accounts(&mut self, cx: &mut gpui::Context<Self>) {
        let mut params = json!({ "operation": "list" });
        if let Some(project_id) = self.gpui_app_modal_active_project_id() {
            params["projectId"] = json!(project_id);
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result: Result<Value, String> = background
                .spawn(async move {
                    gpui_gxserver_rpc_result("/api/agentAccounts", &params, Duration::from_secs(30))
                })
                .await;
            let _ = this.update(cx, |this, _cx| {
                if let Ok(state) = result {
                    this.new_thread_picker_accounts = Some(state);
                }
            });
        })
        .detach();
    }

    /// Opens Settings directly on `tab` (Accounts, Workspaces, Tools, Cloud Boxes).
    pub(crate) fn open_gpui_settings_tab_from_new_thread_picker(
        &mut self,
        tab: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = modal.open_message();
        open_message["initialTab"] = json!(tab);
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }

    /// New session (sidebar button, Cmd+N): immediately creates a draft in the open session's
    /// project (or Main Hub when there is none), selects it under Drafts, and opens its empty chat
    /// with no picker.
    pub(crate) fn start_new_agent_session(&mut self, cx: &mut gpui::Context<Self>) {
        let target_group_id = self.gx_store_new_session_default_group_id();
        self.start_new_agent_session_target(target_group_id, cx);
    }

    /// Starts a new agent draft aimed at `group_id` through the same one-click entry point as
    /// `start_new_agent_session`.
    pub(crate) fn start_new_agent_session_in_group(
        &mut self,
        group_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        self.start_new_agent_session_target(Some(group_id), cx);
    }

    fn start_new_agent_session_target(
        &mut self,
        target_group_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
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

    fn new_agent_session_box_default(
        &mut self,
        agent: &LaunchableAgent,
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
        if ghostex_gx_core::agentbox_agent_family(&agent.agent_id, agent.icon.as_deref()).is_none()
        {
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
        agent: LaunchableAgent,
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
    /// (gx_store/create/agent.rs).
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

    fn last_used_agent_for_group(&self, group_id: Option<&str>) -> Option<LaunchableAgent> {
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
        order_launcher_agents(&hud_agents, primary.as_deref())
            .into_iter()
            .next()
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
        let want_coordinator = self
            .gx_store
            .unified_sidebar
            .carry_coordinator_draft
            .replace(false);
        self.select_agents_tab(pane_id, shell_session_id, cx);
        self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        view.update(cx, |view, cx| {
            if want_coordinator {
                view.draft_coordinator_mode = true;
                view.draft_setup_changed = true;
            }
            view.focus_requested = true;
            cx.notify();
        });
        cx.notify();
        true
    }
}
