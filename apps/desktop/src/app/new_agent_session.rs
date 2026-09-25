//! New Agent Session (Cmd+Shift+O) and the cleanup of the chats agent launchers leave empty.
//! SEE-ALSO: packages/shared/ghostex-hotkeys.ts (CDXC:Hotkeys 2026-09-25), apps/desktop/src/app/hotkeys.rs.
use super::native_chat::state::NativeChatView;
use super::new_thread_picker_lifecycle::order_new_thread_picker_agents;
use crate::app::helpers::*;
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
    /// The sidebar row id, known once gxserver has created the session.
    sidebar_session_id: Option<String>,
    _observer: gpui::Subscription,
}

/// Anything typed, sent, or queued makes the chat the user's; it is never closed after that.
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
        let Some(agent_id) = self.last_used_agent_id() else {
            self.open_gpui_new_thread_picker(cx);
            return;
        };
        if self.focus_untouched_agent_chat(&agent_id, cx) {
            return;
        }
        self.launch_agent_in_active_project(agent_id, None, cx);
    }

    fn last_used_agent_id(&self) -> Option<String> {
        let hud_agents = self.new_thread_picker_agents.clone().or_else(|| {
            self.native_sidebar
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.hud["agents"].as_array().cloned())
        })?;
        order_new_thread_picker_agents(
            &hud_agents,
            self.sidebar_primary_agent_launcher_id.as_deref(),
        )
        .into_iter()
        .next()
        .map(|agent| agent.agent_id)
    }

    /// Starts an agent in the active project the way a New Thread picker row does: the tab opens
    /// at once and gxserver creates the session behind it.
    pub(crate) fn launch_agent_in_active_project(
        &mut self,
        agent_id: String,
        account_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut message = json!({
            "agentId": agent_id,
            "type": "runSidebarAgent",
        });
        if let Some(account_id) = account_id {
            message["accountId"] = json!(account_id);
        }
        self.sidebar_primary_agent_launcher_id = Some(agent_id);
        self.stage_agent_launch_placeholder(&message, cx);
        self.focus_staged_chat_after_picker(cx);
        self.dispatch_gpui_sidebar_host_message(message, cx);
    }

    /// CDXC:AgentLauncher 2026-09-24 DECISION:
    /// User: a new session that stays empty must not linger in the sidebar. A chat an agent launcher opened is closed as soon as the user leaves it (another tab, session, or project takes its place) while nothing was typed or sent; once anything is typed it stays like any other session. Starting the same agent again while such a chat is open goes back to it instead of stacking a second empty one.
    /// Terminal-interface agents and plain terminals are left alone: an empty-looking terminal can still hold state, and there is no reliable "nothing sent yet" signal for them.
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
        let observer = cx.observe(&view, |this, _view, cx| {
            this.review_untouched_agent_chats(cx);
        });
        self.untouched_agent_chats
            .retain(|chat| chat.shell_session_id != shell_session_id);
        self.untouched_agent_chats.push(UntouchedAgentChat {
            shell_session_id,
            view_id: view.entity_id(),
            project_id,
            agent_id,
            sidebar_session_id: None,
            _observer: observer,
        });
    }

    /// Focus moves before the tab or project that replaced the chat has settled, so the review
    /// runs once this update is done.
    pub(crate) fn schedule_untouched_agent_chat_review(&mut self, cx: &mut gpui::Context<Self>) {
        if self.untouched_agent_chats.is_empty() {
            return;
        }
        let app = cx.entity();
        cx.defer(move |cx| {
            app.update(cx, |app, cx| app.review_untouched_agent_chats(cx));
        });
    }

    fn tracked_agent_chat_view(&self, chat: &UntouchedAgentChat) -> Option<Entity<NativeChatView>> {
        self.native_chat_views
            .get(&chat.shell_session_id)
            .filter(|view| view.entity_id() == chat.view_id)
            .cloned()
    }

    fn review_untouched_agent_chats(&mut self, cx: &mut gpui::Context<Self>) {
        let active_project_id = self.agents_workspace_project_id.clone();
        let mut close_ids = Vec::new();
        for mut chat in std::mem::take(&mut self.untouched_agent_chats) {
            if active_project_id.as_deref() == Some(chat.project_id.as_str()) {
                // A closed tab drops its view; a chat that can no longer be read is left as it is.
                let Some(view) = self.tracked_agent_chat_view(&chat) else {
                    continue;
                };
                if !agent_chat_untouched(view.read(cx)) {
                    continue;
                }
                if let Some(GpuiWorkspaceTerminalSessionKey::Local(key)) =
                    self.workspace_terminal_key_for_shell_session(chat.shell_session_id)
                {
                    chat.sidebar_session_id = Some(gpui_combined_presentation_session_id(
                        &key.project_id,
                        &key.session_id,
                    ));
                }
                let shown = self
                    .agents_workspace
                    .pane_id_for_session(chat.shell_session_id)
                    .and_then(|pane_id| self.agents_workspace.active_session_in_pane(pane_id))
                    == Some(chat.shell_session_id);
                if shown {
                    self.untouched_agent_chats.push(chat);
                    continue;
                }
            }
            match chat.sidebar_session_id.take() {
                Some(sidebar_session_id) => close_ids.push(sidebar_session_id),
                // Still being created: it is closed once its id arrives.
                None => self.untouched_agent_chats.push(chat),
            }
        }
        for sidebar_session_id in close_ids {
            self.dispatch_native_sidebar_command(
                json!({"type": "closeSession", "sessionId": sidebar_session_id}),
                cx,
            );
        }
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
