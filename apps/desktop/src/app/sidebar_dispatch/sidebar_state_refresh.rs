//! Workspace lifecycle request ids and the command pane, delayed send, run state and action completion updates pushed to the sidebar.

use std::time::Instant;
use std::time::SystemTime;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn next_local_workspace_lifecycle_request_id(&mut self) -> Option<u64> {
        let (request_id, next_request_id) =
            next_available_gpui_local_workspace_lifecycle_request_id(
                self.next_local_workspace_lifecycle_request_id,
                |candidate| {
                    self.local_workspace_lifecycle_requests
                        .contains_key(&candidate)
                },
            )?;
        self.next_local_workspace_lifecycle_request_id = next_request_id;
        Some(request_id)
    }

    pub(crate) fn dispatch_gpui_workspace_terminal_lifecycle_request(
        &mut self,
        message: serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:Workarea 2026-06-26-07:25:
        Native GPUI workspace tab lifecycle: the request carries only request id, action, bounded gxserver project/session ids, and optional replacement ids; the store performs the gxserver half (gx_store/terminal_lifecycle/lifecycle_requests.rs) while the workspace keeps pane/tab ownership local.
        */
        self.gx_store_run_tab_lifecycle_request(&message, cx)
    }

    /// Called after every command-pane change; returns whether a tab's summary (status, focus,
    /// timer labels) moved, which the policy poll and the ready replay use to repaint. The summaries
    /// no longer cross to the app runtime: its HUD's command-session indicators had no reader once
    /// the HUD became Rust's (ledger R032).
    pub(crate) fn refresh_sidebar_command_pane_sessions_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.schedule_close_terminal_view_if_empty(cx);
        let sessions = self.command_pane.sidebar_command_session_sources(
            self.shell_focus == ShellFocusTarget::CommandPane,
            &self.command_delayed_send_timers,
            &self.command_close_after_done_timers,
            SystemTime::now(),
        );
        let snapshot = sessions.to_string();
        if self.sidebar_command_pane_sessions_snapshot == snapshot {
            return false;
        }
        self.sidebar_command_pane_sessions_snapshot = snapshot;
        true
    }

    pub(crate) fn refresh_sidebar_agents_delayed_sends_if_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        self.prune_local_workspace_session_mappings();
        let now_system = SystemTime::now();
        let now_instant = Instant::now();
        let mut sessions = self
            .local_workspace_session_mappings
            .iter()
            .filter_map(|(key, shell_session_id)| {
                let external_session_id =
                    gpui_combined_presentation_session_id(&key.project_id, &key.session_id);
                if let Some(timer) = self
                    .agents_delayed_send_timers
                    .get(shell_session_id)
                    .copied()
                {
                    let remaining_ms = timer.remaining_ms(now_system);
                    return Some(serde_json::json!({
                        "delayedSendDeadlineAt": gpui_iso8601_utc(timer.deadline_at),
                        "delayedSendRemainingLabel":
                            gpui_command_delayed_send_countdown_label(remaining_ms),
                        "delayedSendRemainingMs": remaining_ms,
                        "sessionId": external_session_id,
                    }));
                }
                let watcher = self
                    .agents_send_when_stopped_watchers
                    .get(shell_session_id)?;
                let is_working = self.gpui_agents_send_when_stopped_scope_is_working(
                    *shell_session_id,
                    &watcher.scope,
                )?;
                Some(serde_json::json!({
                    "delayedSendRemainingLabel": gpui_agents_send_when_stopped_remaining_label(
                        watcher,
                        is_working,
                        now_instant,
                    ),
                    "sendWhenAllProjectSessionsStopActive": matches!(
                        &watcher.scope,
                        GpuiAgentsSendWhenStoppedScope::Project(_)
                    ),
                    "sendWhenAgentStopsActive": matches!(
                        &watcher.scope,
                        GpuiAgentsSendWhenStoppedScope::Session
                    ),
                    "sessionId": external_session_id,
                }))
            })
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| {
            left.get("sessionId")
                .and_then(serde_json::Value::as_str)
                .cmp(&right.get("sessionId").and_then(serde_json::Value::as_str))
        });
        let sessions = serde_json::Value::Array(sessions);
        let snapshot = sessions.to_string();
        if self.sidebar_agents_delayed_sends_snapshot == snapshot {
            return false;
        }
        self.gx_store_set_local_delayed_sends(&sessions, cx);
        self.sidebar_agents_delayed_sends_snapshot = snapshot;
        true
    }

    pub(crate) fn dispatch_gpui_sidebar_command_run_state(
        &mut self,
        command_id: &str,
        run_id: &str,
        state: GpuiSidebarCommandRunState,
    ) {
        self.sidebar_command_run_feedback_states
            .entry(command_id.to_string())
            .or_default()
            .apply_run_state(run_id, state);
    }

    pub(crate) fn dispatch_gpui_command_action_completions(
        &mut self,
        completions: Vec<CommandPaneActionRunCompletion>,
        cx: &mut gpui::Context<Self>,
    ) {
        for completion in completions {
            self.dispatch_gpui_sidebar_command_run_state(
                &completion.command_id,
                &completion.run_id,
                completion.run_state(),
            );
            if completion.should_play_completion_sound() {
                let _ = gpui_play_completion_sound(gpui_action_completion_sound_from_settings());
            }
            self.close_completed_gpui_command_action_tab_if_requested(&completion, cx);
        }
    }

    pub(crate) fn close_completed_gpui_command_action_tab_if_requested(
        &mut self,
        completion: &CommandPaneActionRunCompletion,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        /*
        CDXC:CommandPane 2026-06-26-04:59:
        Native command-pane Actions keep completed tabs reusable even when older Action definitions requested close-on-exit. Keep this completion close helper as a stale-record guard only; current runtime completions normalize close-on-exit to false and must not remove the Action-owned command tab after sidebar feedback.
        */
        let keyboard_owner_before = self.keyboard_owner_session();
        let Some(completed_tab) = self.command_pane.close_completed_action_run_tab(completion)
        else {
            return false;
        };
        self.forget_command_gxserver_session_for_closed_tab(completed_tab.session_id, cx);
        self.prune_gpui_command_delayed_send_timers_for_command_model();
        self.prune_gpui_command_close_after_done_timers_for_command_model();
        if self.command_pane.has_sessions() {
            self.follow_shell_focus_after_surface_removed(
                ShellFocusTarget::CommandPane,
                keyboard_owner_before,
                cx,
            );
        } else {
            self.restore_non_command_focus_after_surface_removed(keyboard_owner_before, cx);
        }
        self.scroll_command_group_active_tab(completed_tab.group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
        true
    }

    pub(crate) fn sleep_project_editor_mode_from_timer(
        &mut self,
        mode: TitlebarMode,
        token: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_editor_auto_sleep_epochs.epoch(mode) != Some(token)
            || self.active_mode == mode
            || !self.project_editor_shell.is_mode_awake(mode)
        {
            return;
        }

        if !self.project_editor_shell.mark_mode_sleeping(mode) {
            return;
        }
        self.project_editor_auto_sleep_epochs.bump(mode);
        if mode == TitlebarMode::Browser {
            self.pending_startup_browser_reload = false;
            self.pending_startup_browser_remote_machines.clear();
            self.update_active_mode_cef_child_visibility(cx);
        }
        if mode == TitlebarMode::Source {
            /*
            macOS `stopCodeServerRuntimeIfEveryEditorSleeping` parity: when the
            last awake Source surface sleeps, the shared code-server process
            exits instead of idling hidden. GPUI's single workspace window has
            exactly one Source surface, so Source-mode sleep IS "every editor
            sleeping"; the click-to-wake path relaunches the runtime through
            the existing ensure/start pipeline.
            */
            self.stop_source_code_server_runtime(cx);
        }
        self.persist_shell_layout_state();
        cx.notify();
    }
}
