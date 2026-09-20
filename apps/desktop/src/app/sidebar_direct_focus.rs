use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::native_sidebar::model::NativeSidebarSession;
use crate::*;
use std::sync::Arc;

/// What a row click did in process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeSidebarClickReaction {
    /// The session's tab has a live terminal and was selected.
    InProcess,
    /// The session got a staged tab; the runtime's wake and attach fill it.
    Staged,
    /// Not a local session row (a browser or remote row, or one the sidebar snapshot does not hold): the runtime's route owns it.
    NotApplied,
}

impl GhostexGpuiApp {
    /// A row click's whole in-process reaction: the tab switch when the session already has a live tab, otherwise the staged tab the runtime's wake and attach will fill.
    pub(crate) fn react_to_native_sidebar_session_click(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> NativeSidebarClickReaction {
        let switched_project =
            self.swap_native_sidebar_click_workspace_project(sidebar_session_id, cx);
        let keep_view = switched_project && self.native_sidebar_click_keeps_remembered_view();
        let reaction =
            if !keep_view && self.focus_native_sidebar_session_in_process(sidebar_session_id, cx) {
                NativeSidebarClickReaction::InProcess
            } else if self.stage_native_sidebar_session_tab(sidebar_session_id, keep_view, cx) {
                NativeSidebarClickReaction::Staged
            } else {
                NativeSidebarClickReaction::NotApplied
            };
        let applied = reaction != NativeSidebarClickReaction::NotApplied;
        if applied && let Some(key) = gpui_combined_presentation_session_key(sidebar_session_id) {
            // The runtime routes the same click and sends its own focus request for the session. It is applied while this click is still the newest selection (it attaches a staged tab) and dropped once the user has moved on (gx_store/local_focus.rs).
            self.gx_store_expect_click_echo(&key);
        }
        reaction
    }

    /// CDXC:Sidebar 2026-09-20 WHY:
    /// A row click on another project's session reached the workspace only after the runtime had published the project change, so nothing on the pane moved until then and the previous project's session stayed on screen for the whole wake and attach. Everything the swap needs is already in the model, so the click's own frame runs it and the runtime's later project change lands on the project that is already active; a stale active-project payload produced before this selection is refused by the store's focus stamp (gx_store/local_focus.rs), which the reaction below advances.
    /// Browser rows and remote sessions keep the bridge path, which owns their own activation.
    fn swap_native_sidebar_click_workspace_project(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = gpui_combined_presentation_session_key(sidebar_session_id) else {
            return false;
        };
        if self.agents_workspace_project_id.as_deref() == Some(key.project_id.as_str()) {
            return false;
        }
        let Some(row) = self.native_sidebar_session_row(sidebar_session_id) else {
            return false;
        };
        if row.is_browser() {
            return false;
        }
        self.swap_agents_workspace_to_project_id(Some(key.project_id.clone()), cx);
        self.agents_workspace_project_id.as_deref() == Some(key.project_id.as_str())
    }

    /// CDXC:Navigation 2026-09-20 WHY:
    /// The 2026-09-11 decision in `focus_local_workspace_terminal_from_message` (workspace_events.rs) is that landing on another project keeps that project's remembered view. The swap above restored that view, so a click that changed the project reads it here: when the destination came back to a view rather than to Agents, the click selects its session in the background and leaves the view holding the keyboard.
    fn native_sidebar_click_keeps_remembered_view(&self) -> bool {
        self.active_mode != TitlebarMode::Agents
    }

    /// CDXC:Sidebar 2026-09-19 WHY:
    /// A row click reached the workspace only after the service thread ran the sidebar command, the runtime routed the focus, and the bridge message came back, so the pane switched one service-thread turn after the row highlight even when the session already had a tab; Waku switches in the click's own frame.
    /// A local, awake session of the active project whose tab already has a live terminal is selected here, synchronously, through the same tab selection the bridge path ends in. The runtime still receives the command so presentation focus, attention acknowledgement and the sidebar snapshot follow. Another project's session reaches this path once `swap_native_sidebar_click_workspace_project` has made it the active one, and only when the destination's remembered view is Agents; remote and browser sessions keep the bridge path, which owns their activation and gxserver attach plans.
    /// The selection is a store intent (gx_store/local_focus.rs). The runtime's focus message for the same click is recognised by the store's stamp order, not by time: it is applied while the click is still the newest selection and dropped once the user has moved on. This supersedes the `sidebar_in_process_focus` marker and its three second echo window of earlier the same day.
    pub(crate) fn focus_native_sidebar_session_in_process(
        &mut self,
        sidebar_session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = gpui_combined_presentation_session_key(sidebar_session_id) else {
            return false;
        };
        if self.agents_workspace_project_id.as_deref() != Some(key.project_id.as_str()) {
            return false;
        }
        let Some(shell_session_id) = self.local_workspace_session_mappings.get(&key).copied()
        else {
            return false;
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return false;
        };
        if !self.local_workspace_terminal_can_focus_existing(pane_id, shell_session_id) {
            return false;
        }
        // The same preamble `focus_local_workspace_terminal_from_message` runs before selecting an existing tab.
        self.begin_sidebar_focus_border_handoff(cx);
        self.local_workspace_latest_focus_key = Some(key.clone());
        self.advance_presentation_focus_to_in_process_click(&key);
        // The bootstrap carries the focused session to the sidebar runtime, one script per change. A held previous or next session key must not send one per row; the tell that ends the burst refreshes it (gx_store/burst.rs).
        if !self.gx_store_key_is_held() && !self.gx_store_selection_is_settling() {
            self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        }
        if !self.focus_existing_gpui_local_workspace_terminal(&key, cx) {
            return false;
        }
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.inProcessFocusCompleted",
            serde_json::json!({
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": key.project_id,
                "sessionId": key.session_id,
            }),
        );
        true
    }

    /// CDXC:FocusRouting 2026-09-19 WHY:
    /// The click is the new focus: record it here, and the runtime's own focus state confirms it a moment later.
    fn advance_presentation_focus_to_in_process_click(
        &mut self,
        key: &GpuiLocalWorkspaceSessionKey,
    ) {
        let focus_state = &mut self.sidebar_gxserver_presentation_focus_state;
        if focus_state.active_project_id.as_deref() == Some(key.project_id.as_str()) {
            focus_state.focused_session_id = Some(key.session_id.clone());
        }
    }

    /// CDXC:Sidebar 2026-09-19 DECISION:
    /// User: clicking a sleeping session, or one not opened recently, must react on the pane at once and show that session, with a skeleton until its chat transcript is ready, instead of waiting for the wake and attach while the current session stays on screen; people flip between sessions quickly.
    /// The session's tab is selected now, or created now as a mounting placeholder mapped to the session, so the pane switches in the click's frame. The runtime's wake and attach then fill that same tab: the attach completion reuses a mapped tab in place. A session whose agent prefers Chat and that already has a transcript gets its chat surface immediately, so the chat runtime boots while the daemon is still waking the session.
    fn stage_native_sidebar_session_tab(
        &mut self,
        sidebar_session_id: &str,
        keep_view: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = gpui_combined_presentation_session_key(sidebar_session_id) else {
            return false;
        };
        if self.agents_workspace_project_id.as_deref() != Some(key.project_id.as_str()) {
            return false;
        }
        let mapped = self
            .local_workspace_session_mappings
            .get(&key)
            .copied()
            .filter(|shell| self.agents_workspace.pane_id_for_session(*shell).is_some());
        let shell_session_id = match mapped {
            Some(shell) => shell,
            None => {
                let Some(row) = self.native_sidebar_session_row(sidebar_session_id) else {
                    return false;
                };
                if row.is_browser() {
                    return false;
                }
                let focused_pane = self.agents_workspace.focused_pane;
                let Some(shell) = self
                    .agents_workspace
                    .add_mounting_session_to_pane(focused_pane)
                else {
                    return false;
                };
                let icon = gpui_sidebar_agent_icon(row.agent_icon.as_deref());
                if let Some(session) = self
                    .agents_workspace
                    .terminal_sessions
                    .iter_mut()
                    .find(|session| session.id == shell)
                {
                    session.title = row.title().to_owned();
                    session.agent_icon = icon;
                    // Not a startup-pipeline candidate: the attach owns this tab.
                    session.set_presentation_state_with_startup_eligibility(
                        TerminalSessionPresentationState::Mounting,
                        false,
                    );
                }
                self.local_workspace_session_mappings
                    .insert(key.clone(), shell);
                self.local_app_shot_session_mappings
                    .insert(key.session_id.clone(), shell);
                let has_transcript = row.is_draft
                    || row
                        .details
                        .get("agentSessionId")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|id| !id.trim().is_empty());
                let settings = shared_settings::shared_sidebar_settings_snapshot();
                if has_transcript
                    && self.agents_session_chat_transcript_agent(shell).is_some()
                    && gpui_effective_preferred_agent_interface_for_agent_icon(
                        settings.object(),
                        icon,
                    ) == GpuiPreferredAgentInterface::Chat
                {
                    self.agents_chat_mode_sessions.insert(shell);
                }
                shell
            }
        };
        let Some(pane_id) = self.agents_workspace.pane_id_for_session(shell_session_id) else {
            return false;
        };
        self.agents_workspace.select_tab(pane_id, shell_session_id);
        // The sidebar highlight reads the store, so the staged tab is a selection like any other.
        self.gx_store_select_local_session(&key, false, false, cx);
        // CDXC:Workarea 2026-09-20 WHY:
        // Staging a tab in the Agents column is enough now: the column is on screen whatever the
        // view panel shows, so there is no companion to retarget and no view to leave. What a
        // kept view still owns is the keyboard: a click that landed on another project leaves
        // focus where it is, the way `select_local_workspace_terminal_keeping_view` does.
        if !keep_view {
            self.focus_shell_target(ShellFocusTarget::AgentsPane(pane_id), cx);
        }
        self.scroll_workspace_pane_active_tab(pane_id);
        self.reconcile_agents_pane_surfaces(cx);
        self.update_active_mode_cef_child_visibility(cx);
        self.persist_shell_layout_state();
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionSwitchLatency.inProcessTabStaged",
            serde_json::json!({
                "epochMs": support_logs::temporary_epoch_ms(),
                "projectId": key.project_id,
                "sessionId": key.session_id,
                "mapped": mapped.is_some(),
            }),
        );
        cx.notify();
        true
    }

    fn native_sidebar_session_row(
        &self,
        sidebar_session_id: &str,
    ) -> Option<Arc<NativeSidebarSession>> {
        self.native_sidebar
            .snapshot
            .as_ref()?
            .groups
            .iter()
            .flat_map(|group| group.sessions.iter())
            .find(|session| session.session_id == sidebar_session_id)
            .cloned()
    }
}
