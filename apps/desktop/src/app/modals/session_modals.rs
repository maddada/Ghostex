//! Delayed Send and Rename Session modals for agents sessions and command pane tabs.

use std::time::SystemTime;

use gpui::Window;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn gpui_command_delayed_send_open_message(
        &self,
        session_id: CommandSessionId,
        title: &str,
    ) -> serde_json::Value {
        let mut message = serde_json::json!({
            "closeAfterDoneActive": self
                .command_pane
                .session(session_id)
                .is_some_and(|session| session.close_after_done_armed),
            "modal": GpuiAppModalKind::DelayedSend.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "title": title,
            "type": "open",
        });
        if let Some(timer) = self.command_delayed_send_timers.get(&session_id).copied() {
            let remaining_ms = timer.remaining_ms(SystemTime::now());
            message["delayedSendDeadlineAt"] =
                serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
            message["delayedSendRemainingLabel"] =
                serde_json::json!(gpui_command_delayed_send_countdown_label(remaining_ms));
        }
        message
    }

    pub(crate) fn gpui_command_delayed_send_remaining_label_for_session(
        &self,
        session_id: CommandSessionId,
    ) -> Option<String> {
        self.command_delayed_send_timers
            .get(&session_id)
            .copied()
            .and_then(|timer| {
                gpui_command_delayed_send_body_badge_label(Some(timer), SystemTime::now())
            })
    }

    pub(crate) fn command_pane_tab_exists(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        self.command_pane
            .find_leaf(group_id)
            .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
            && self.command_pane.session(session_id).is_some()
    }

    pub(crate) fn open_gpui_delayed_send_modal_for_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-25-17:37:
        Delayed Send from a clicked command tab is session-scoped like native, but GPUI can only execute the later Return through a visible mounted command body. Selecting, expanding, and waking the clicked tab before opening the modal is the normal-layout equivalent of targeting that native command session.
        */
        if !self.command_pane_tab_exists(group_id, session_id) {
            return false;
        }
        let expand_pane = self.command_pane.dock_for_group(group_id)
            == Some(CommandPaneDock::Panel)
            && !self.command_pane.is_expanded();
        if !self.select_command_pane_tab(group_id, session_id, expand_pane, window, cx) {
            return false;
        }
        self.reveal_command_group_dock(group_id, cx);
        if self
            .command_pane
            .session(session_id)
            .is_some_and(|session| session.is_sleeping)
        {
            self.wake_command_pane_session(group_id, session_id, cx);
        }
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let open_message = self.gpui_command_delayed_send_open_message(session_id, &title);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn focus_command_pane_tab_for_context_session_action(
        &mut self,
        action: CommandPaneTabSessionAction,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:ContextMenus 2026-06-25-18:33:
        Legacy clicked-tab action handlers focus the clicked terminal before Rename and Close After Done dispatch. Select and focus the clicked GPUI command tab without expanding a collapsed strip so the action target becomes the command-pane focus while left-click remains the only hidden-open gesture.

        CDXC:Notifications 2026-06-25-19:58:
        Primary clicked-tab context actions use that same focus path, so they acknowledge only the clicked Attention command session before opening Rename or toggling Close After Done.

        CDXC:ContextMenus 2026-06-27-01:55:
        Command-tab right-click no longer exposes these retained handlers because native command-panel payloads are panel-only. Keep the helper for non-menu command-tab dispatch paths without reintroducing visible primary menu rows.
        */
        if command_pane_tab_context_session_action_focus_policy(action)
            != CommandPaneTabContextFocusPolicy::SelectAndFocus
        {
            return false;
        }
        if !self.command_pane_tab_exists(group_id, session_id)
            || !self
                .command_pane
                .select_session_in_group(group_id, session_id)
        {
            return false;
        }

        self.command_pane
            .acknowledge_attention_for_session_activation(session_id);
        self.focus_command_pane(cx);
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    /// Cmd+R with a focused Agents-view or companion-pane terminal renames
    /// the focused mapped gxserver session through the shared Rename Session
    /// modal, matching macOS `promptRenameFocusedNativeHotkeySession`.
    /// Unmapped local placeholder tabs have no gxserver identity to rename
    /// and no-op.
    pub(crate) fn open_gpui_rename_session_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        self.open_gpui_rename_session_modal_for_agents_tab(shell_session_id, cx)
    }

    pub(crate) fn open_gpui_rename_session_modal_for_agents_tab(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            // A remote session's Rename is its sidebar row's (CDXC:RemoteMachines in
            // terminal_sync/workspace_dispatch_and_focus.rs).
            return self.dispatch_gpui_workspace_terminal_runtime_action(
                "rename",
                shell_session_id,
                cx,
            );
        };
        let Some(title) = self
            .agents_workspace
            .session(shell_session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        // A rename the agent never confirmed is offered again, as the sidebar's Rename does
        // (`plan_modal_action` in packages/gx-core/src/sidebar_actions/modals.rs).
        let title = self
            .gx_store
            .core
            .presentation()
            .loaded(&ghostex_gx_core::MachineId::Local)
            .and_then(|loaded| loaded.server_session(&key.project_id, &key.session_id))
            .and_then(|session| session.rename_failed_title.clone())
            .filter(|attempt| !attempt.trim().is_empty())
            .unwrap_or(title);
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_combined_presentation_session_id(
                &key.project_id,
                &key.session_id,
            ),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_rename_session_modal_for_focused_command_pane(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session_id)) =
            focused_command_pane_rename_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_delayed_send_modal_for_focused_command_pane(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some((_group_id, session_id)) =
            focused_command_pane_rename_target(self.shell_focus, &self.command_pane)
        else {
            return false;
        };
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::DelayedSend;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let open_message = self.gpui_command_delayed_send_open_message(session_id, &title);
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn open_gpui_rename_session_modal_for_command_pane_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self.focus_command_pane_tab_for_context_session_action(
            CommandPaneTabSessionAction::Rename,
            group_id,
            session_id,
            cx,
        ) {
            return false;
        }
        let Some(title) = self
            .command_pane
            .session(session_id)
            .map(|session| session.title.clone())
        else {
            return false;
        };
        let modal = GpuiAppModalKind::RenameSession;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTitle": title,
            "modal": modal.modal_id(),
            "sessionId": gpui_command_session_external_id(session_id),
            "type": "open",
        });
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }
}
