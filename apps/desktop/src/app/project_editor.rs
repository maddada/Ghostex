// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-editor companion terminal selection and auto-sleep policy

use std::collections::HashMap;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;
impl GhostexGpuiApp {
    pub(crate) fn mark_project_editor_mode_awake(
        &mut self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let marked = self.project_editor_shell.mark_mode_awake(mode);
        if marked {
            self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        }
        marked
    }

    pub(crate) fn project_editor_companion_terminal_session_is_eligible(
        &self,
        session_id: TerminalSessionId,
    ) -> bool {
        self.agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Running
                    && self
                        .agents_terminal_runtime_sessions
                        .runtime_session_id_for_shell_session(session_id)
                        .is_some()
            })
    }

    pub(crate) fn project_editor_companion_active_project_id(&self) -> Option<String> {
        gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
            .map(str::to_string)
    }

    pub(crate) fn workspace_terminal_key_for_shell_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<GpuiWorkspaceTerminalSessionKey> {
        if let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == session_id).then(|| key.clone()))
        {
            return Some(GpuiWorkspaceTerminalSessionKey::Local(key));
        }
        let active_project_id = self.agents_workspace_project_id.as_deref()?;
        let remote_project = gpui_remote_project_reference_from_project_id(active_project_id)?;
        self.remote_attach_sessions
            .iter()
            .find_map(|(key, mapped_session_id)| {
                (*mapped_session_id == session_id
                    && key.remote_machine_id == remote_project.remote_machine_id
                    && key.project_id == remote_project.project_id)
                    .then(|| GpuiWorkspaceTerminalSessionKey::Remote(key.clone()))
            })
    }

    pub(crate) fn project_editor_companion_terminal_key_for_slot(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<GpuiWorkspaceTerminalSessionKey> {
        self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id)
            .then(|| self.workspace_terminal_key_for_shell_session(slot_id.session_id))?
    }

    pub(crate) fn project_editor_companion_remote_attach_attempt_for_slot(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<GpuiProjectEditorCompanionRemoteAttachAttempt> {
        let GpuiWorkspaceTerminalSessionKey::Remote(remote_key) =
            self.project_editor_companion_terminal_key_for_slot(slot_id)?
        else {
            return None;
        };
        let connection_generation = self
            .remote_gxserver_connect_generations
            .get(remote_key.remote_machine_id.as_str())
            .copied()
            .unwrap_or(0);
        Some(GpuiProjectEditorCompanionRemoteAttachAttempt {
            connection_generation,
            remote_key,
        })
    }

    pub(crate) fn project_editor_companion_remote_attach_attempt_is_current(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        attempt: &GpuiProjectEditorCompanionRemoteAttachAttempt,
    ) -> bool {
        self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id)
            && self
                .project_editor_companion_remote_attach_attempt_for_slot(slot_id)
                .as_ref()
                == Some(attempt)
    }

    pub(crate) fn record_project_editor_companion_remote_attach_unavailable(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        attempt: GpuiProjectEditorCompanionRemoteAttachAttempt,
        message: String,
    ) {
        if !self.project_editor_companion_remote_attach_attempt_is_current(slot_id, &attempt)
            || !self
                .project_editor_companion_remote_attach_states
                .get(&slot_id)
                .is_some_and(|state| state.attempt() == &attempt)
        {
            return;
        }
        self.project_editor_companion_remote_attach_states.insert(
            slot_id,
            GpuiProjectEditorCompanionRemoteAttachState::Unavailable { attempt, message },
        );
    }

    pub(crate) fn clear_project_editor_companion_remote_attach_state_for_key(
        &mut self,
        remote_key: &GpuiRemoteAttachSessionKey,
    ) {
        self.project_editor_companion_remote_attach_states
            .retain(|_, state| &state.attempt().remote_key != remote_key);
    }

    pub(crate) fn clear_project_editor_companion_remote_attach_states_for_machine(
        &mut self,
        remote_machine_id: &str,
    ) {
        self.project_editor_companion_remote_attach_states
            .retain(|_, state| state.attempt().remote_key.remote_machine_id != remote_machine_id);
    }

    pub(crate) fn prune_project_editor_companion_remote_attach_states(&mut self) {
        let current_attempts = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .filter_map(|slot_id| {
                self.project_editor_companion_remote_attach_attempt_for_slot(slot_id)
                    .map(|attempt| (slot_id, attempt))
            })
            .collect::<HashMap<_, _>>();
        self.project_editor_companion_remote_attach_states
            .retain(|slot_id, state| current_attempts.get(slot_id) == Some(state.attempt()));
    }

    pub(crate) fn project_editor_companion_remote_attach_unavailable_message(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<String> {
        let state = self
            .project_editor_companion_remote_attach_states
            .get(&slot_id)?;
        let GpuiProjectEditorCompanionRemoteAttachState::Unavailable { attempt, message } = state
        else {
            return None;
        };
        self.project_editor_companion_remote_attach_attempt_is_current(slot_id, attempt)
            .then(|| message.clone())
    }

    pub(crate) fn shell_session_for_workspace_terminal_key(
        &self,
        key: &GpuiWorkspaceTerminalSessionKey,
    ) -> Option<TerminalSessionId> {
        match key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => {
                self.local_workspace_session_mappings.get(key).copied()
            }
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                self.remote_attach_sessions.get(key).copied()
            }
        }
    }

    pub(crate) fn project_editor_companion_active_terminal_key(
        &self,
    ) -> Option<GpuiWorkspaceTerminalSessionKey> {
        let active_project_id = self.project_editor_companion_active_project_id()?;
        let focus_state = &self.sidebar_gxserver_presentation_focus_state;
        if focus_state.active_project_id.as_deref() != Some(active_project_id.as_str()) {
            return None;
        }
        let focused_session_id = focus_state.focused_session_id.as_deref()?;
        if let Some(reference) =
            gpui_remote_attach_session_reference_from_project_id(focused_session_id)
        {
            let key = GpuiRemoteAttachSessionKey::from(&reference);
            return (gpui_remote_scoped_project_id(
                key.remote_machine_id.as_str(),
                key.project_id.as_str(),
            ) == active_project_id)
                .then_some(GpuiWorkspaceTerminalSessionKey::Remote(key));
        }
        if gpui_remote_project_reference_from_project_id(active_project_id.as_str()).is_some() {
            return None;
        }
        Some(GpuiWorkspaceTerminalSessionKey::Local(
            GpuiLocalWorkspaceSessionKey {
                project_id: active_project_id,
                session_id: focused_session_id.to_string(),
            },
        ))
    }

    pub(crate) fn project_editor_companion_active_terminal_session_id(
        &mut self,
    ) -> Option<TerminalSessionId> {
        let key = self.project_editor_companion_active_terminal_key()?;
        let session_id = self.shell_session_for_workspace_terminal_key(&key)?;
        self.project_editor_companion_terminal_session_is_active_project_eligible(session_id)
            .then_some(session_id)
    }

    pub(crate) fn shell_session_belongs_to_active_project(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(active_project_id) = self.project_editor_companion_active_project_id() else {
            return false;
        };
        let Some(key) = self.workspace_terminal_key_for_shell_session(session_id) else {
            return false;
        };
        key.scoped_project_id() == active_project_id
    }

    pub(crate) fn project_editor_companion_terminal_session_is_active_project_eligible(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        self.project_editor_companion_terminal_session_is_eligible(session_id)
            && self.shell_session_belongs_to_active_project(session_id)
    }

    pub(crate) fn project_editor_companion_focused_terminal_session_id(
        &self,
    ) -> Option<TerminalSessionId> {
        match self.project_editor_companion_focused_terminal_slot {
            ProjectEditorCompanionTerminalSlot::Top => {
                self.project_editor_companion_terminal_session_id
            }
            ProjectEditorCompanionTerminalSlot::Bottom => {
                self.project_editor_companion_secondary_terminal_session_id
            }
        }
    }

    pub(crate) fn project_editor_companion_active_title(&self, _mode: TitlebarMode) -> String {
        self.project_editor_companion_focused_terminal_session_id()
            .map(|session_id| self.agents_workspace_tab_display_title(session_id))
            .unwrap_or_else(|| "Companion".to_string())
    }

    pub(crate) fn project_editor_companion_recent_terminal_sessions(
        &mut self,
    ) -> Vec<TerminalSessionId> {
        let active_project_id = self.project_editor_companion_active_project_id();
        let mut keys = self
            .sidebar_gxserver_presentation_focus_state
            .active_project_tab_sessions
            .as_ref()
            .into_iter()
            .flatten()
            .filter_map(|session| {
                let key = session.key.as_local()?;
                (active_project_id.as_deref() == Some(key.project_id.as_str())).then(|| key.clone())
            })
            .collect::<Vec<_>>();
        if let Some(latest_key) = self.local_workspace_latest_focus_key.clone()
            && active_project_id.as_deref() == Some(latest_key.project_id.as_str())
        {
            keys.retain(|key| key != &latest_key);
            keys.push(latest_key);
        }
        let mut mapped_session_ids = keys
            .into_iter()
            .rev()
            .filter_map(|key| self.local_workspace_session_mappings.get(&key).copied())
            .collect::<Vec<_>>();
        let mut remote_session_ids = self
            .remote_attach_sessions
            .iter()
            .filter_map(|(key, session_id)| {
                (active_project_id.as_deref()
                    == Some(
                        gpui_remote_scoped_project_id(
                            key.remote_machine_id.as_str(),
                            key.project_id.as_str(),
                        )
                        .as_str(),
                    ))
                .then_some(*session_id)
            })
            .collect::<Vec<_>>();
        remote_session_ids.sort_by_key(|session_id| std::cmp::Reverse(session_id.0));
        mapped_session_ids.extend(remote_session_ids);
        if let Some(active_session_id) = self.project_editor_companion_active_terminal_session_id()
        {
            mapped_session_ids.retain(|session_id| *session_id != active_session_id);
            mapped_session_ids.insert(0, active_session_id);
        }
        mapped_session_ids
            .into_iter()
            .filter(|session_id| {
                self.project_editor_companion_terminal_session_is_active_project_eligible(
                    *session_id,
                )
            })
            .collect()
    }

    pub(crate) fn sync_project_editor_companion_terminal_selection(&mut self) -> bool {
        let previous = (
            self.project_editor_companion_terminal_session_id,
            self.project_editor_companion_secondary_terminal_session_id,
            self.project_editor_companion_focused_terminal_slot,
        );
        let mut top = self
            .project_editor_companion_terminal_session_id
            .filter(|session_id| {
                self.project_editor_companion_terminal_session_is_active_project_eligible(
                    *session_id,
                )
            });
        let mut bottom = self
            .project_editor_shell
            .left_companion_split_enabled
            .then_some(self.project_editor_companion_secondary_terminal_session_id)
            .flatten()
            .filter(|session_id| {
                self.project_editor_companion_terminal_session_is_active_project_eligible(
                    *session_id,
                ) && Some(*session_id) != top
            });
        let recent = self.project_editor_companion_recent_terminal_sessions();

        if let Some(active_session_id) = recent.first().copied() {
            if !self.project_editor_shell.left_companion_split_enabled {
                top = Some(active_session_id);
                self.project_editor_companion_focused_terminal_slot =
                    ProjectEditorCompanionTerminalSlot::Top;
            } else if top == Some(active_session_id) {
                self.project_editor_companion_focused_terminal_slot =
                    ProjectEditorCompanionTerminalSlot::Top;
            } else if bottom == Some(active_session_id) {
                self.project_editor_companion_focused_terminal_slot =
                    ProjectEditorCompanionTerminalSlot::Bottom;
            } else if bottom.is_some()
                && self.project_editor_companion_focused_terminal_slot
                    == ProjectEditorCompanionTerminalSlot::Bottom
            {
                bottom = Some(active_session_id);
            } else {
                top = Some(active_session_id);
                self.project_editor_companion_focused_terminal_slot =
                    ProjectEditorCompanionTerminalSlot::Top;
            }
        }

        if self.project_editor_shell.left_companion_split_enabled
            && bottom.is_none()
            && top == recent.first().copied()
            && recent.len() > 1
        {
            bottom = top;
            top = recent.get(1).copied();
            self.project_editor_companion_focused_terminal_slot =
                ProjectEditorCompanionTerminalSlot::Bottom;
        } else {
            if top.is_none() {
                top = recent
                    .iter()
                    .copied()
                    .find(|session_id| Some(*session_id) != bottom);
            }
            if self.project_editor_shell.left_companion_split_enabled && bottom.is_none() {
                bottom = recent
                    .iter()
                    .copied()
                    .find(|session_id| Some(*session_id) != top);
            }
        }

        if top.is_none() && bottom.is_some() {
            top = bottom.take();
        }
        if bottom.is_none() {
            self.project_editor_companion_focused_terminal_slot =
                ProjectEditorCompanionTerminalSlot::Top;
        }
        self.project_editor_companion_terminal_session_id = top;
        self.project_editor_companion_secondary_terminal_session_id = bottom;
        previous
            != (
                self.project_editor_companion_terminal_session_id,
                self.project_editor_companion_secondary_terminal_session_id,
                self.project_editor_companion_focused_terminal_slot,
            )
    }

    pub(crate) fn project_editor_companion_terminal_slot_for_mode(
        &self,
        mode: TitlebarMode,
    ) -> Option<ProjectEditorCompanionTerminalBodyMountSlotId> {
        if self.active_mode != mode
            || !mode.is_project_editor_mode()
            || !self.project_editor_shell.left_companion_visible
            || !self.project_editor_shell.is_mode_awake(mode)
        {
            return None;
        }
        let session_id = self.project_editor_companion_focused_terminal_session_id()?;
        self.project_editor_companion_terminal_session_is_eligible(session_id)
            .then_some(ProjectEditorCompanionTerminalBodyMountSlotId { mode, session_id })
    }

    pub(crate) fn current_project_editor_companion_terminal_body_mount_slots(
        &self,
    ) -> Vec<ProjectEditorCompanionTerminalBodyMountSlotId> {
        let mode = self.active_mode;
        if !mode.is_project_editor_mode()
            || !self.project_editor_shell.left_companion_visible
            || !self.project_editor_shell.is_mode_awake(mode)
        {
            return Vec::new();
        }
        [
            self.project_editor_companion_terminal_session_id,
            self.project_editor_companion_secondary_terminal_session_id,
        ]
        .into_iter()
        .flatten()
        .filter(|session_id| {
            self.project_editor_companion_terminal_session_is_eligible(*session_id)
        })
        .map(|session_id| ProjectEditorCompanionTerminalBodyMountSlotId { mode, session_id })
        .collect()
    }

    pub(crate) fn is_current_project_editor_companion_terminal_body_mount_slot(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> bool {
        self.current_project_editor_companion_terminal_body_mount_slots()
            .contains(&slot_id)
    }

    pub(crate) fn schedule_project_editor_auto_sleep_for_inactive_modes(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIProjectEditorLifecycle 2026-06-22-08:29:
        GPUI project-editor auto-sleep is shell lifecycle behavior, not placeholder teardown. Source, Browser, Kanban, and Manage each get independent runtime timers that are rescheduled on local mode and wake mutations; only inactive awake modes can sleep, and Browser sleep only hides CEF through the existing visibility gate instead of deleting tabs or surfaces.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-09:49:
        GPUI must notice effective project-editor auto-sleep policy changes while running without native settings subscriptions or filesystem watchers. Keep a runtime-only per-mode duration snapshot derived from shared settings, poll it at a fixed shell interval, and reschedule only when the effective enabled/duration policy changes so unchanged timers can still fire.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-09:49:
        Policy rescheduling is shell lifecycle behavior only: it invalidates pending Source, Browser, Kanban, and Manage auto-sleep epochs, restarts timers for inactive awake modes using the new effective duration, leaves active and sleeping modes in their current state, keeps Browser CEF and placeholder surfaces intact, defaults missing or malformed settings to enabled with five idle minutes, and never logs or persists raw settings values, paths, project names, browser titles, command text, tokens, or user content.
        */
        for mode in project_editor_modes() {
            self.schedule_project_editor_auto_sleep(mode, cx);
        }
    }

    pub(crate) fn schedule_project_editor_auto_sleep(
        &mut self,
        mode: TitlebarMode,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(token) = self.project_editor_auto_sleep_epochs.bump(mode) else {
            return;
        };
        if self.active_mode == mode || !self.project_editor_shell.is_mode_awake(mode) {
            return;
        }
        let Some(duration) = self
            .project_editor_auto_sleep_policy
            .duration_for_mode(mode)
        else {
            return;
        };

        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(duration).await;

            let _ = this.update(cx, |this, cx| {
                this.sleep_project_editor_mode_from_timer(mode, token, cx);
            });
        })
        .detach();
    }

    pub(crate) fn start_project_editor_auto_sleep_policy_polling(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(PROJECT_EDITOR_AUTO_SLEEP_POLICY_POLL_INTERVAL)
                    .await;

                if this
                    .update(cx, |this, cx| {
                        this.reschedule_project_editor_auto_sleep_if_policy_changed(cx);
                        this.refresh_titlebar_actions_in_background(cx);
                        let runtime_settings_changed =
                            this.refresh_sidebar_runtime_settings_if_changed(cx);
                        let gxserver_bootstrap_changed =
                            this.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
                        let command_pane_sessions_changed =
                            this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                        if runtime_settings_changed
                            || gxserver_bootstrap_changed
                            || command_pane_sessions_changed
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn start_prompt_editor_daemon_polling(&mut self, cx: &mut gpui::Context<Self>) {
        // The standalone GhostexEditor daemon is opened by terminal-side
        // Ctrl+G, so this shell only learns about open editor windows by
        // asking the daemon socket. Only `openCount > 0` is stored; session
        // titles, paths, and draft content never enter the shell.
        cx.spawn(async move |this, cx| {
            loop {
                let open = cx
                    .background_executor()
                    .spawn(async move { gpui_ghostex_editor_daemon_open_count() > 0 })
                    .await;

                if this
                    .update(cx, |this, cx| {
                        if this.prompt_editor_daemon_open != open {
                            this.prompt_editor_daemon_open = open;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }

                cx.background_executor()
                    .timer(GHOSTEX_EDITOR_DAEMON_POLL_INTERVAL)
                    .await;
            }
        })
        .detach();
    }

    pub(crate) fn start_command_action_status_polling(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUICommandPane 2026-06-24-23:36:
        Command-pane Action status polling is bounded to GPUI-owned session-state files while live action runs exist. It updates only safe tab activity metadata and never reads command output, terminal content, paths from renderer payloads, logs, shell-state JSON, or persisted command text.
        */
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(COMMAND_ACTION_STATUS_POLL_INTERVAL)
                    .await;

                if this
                    .update(cx, |this, cx| {
                        if !this.command_pane.has_active_action_runs() {
                            return;
                        }
                        let refresh = this
                            .command_pane
                            .refresh_action_run_states_from_status_files();
                        let has_completions = !refresh.completions.is_empty();
                        this.dispatch_gpui_command_action_completions(refresh.completions, cx);
                        if refresh.changed || has_completions {
                            this.sync_gpui_keep_awake_automation_from_current_settings(cx);
                        }
                        let close_after_done_changed =
                            this.refresh_gpui_command_close_after_done_timers(cx);
                        if refresh.changed || close_after_done_changed {
                            this.persist_shell_layout_state();
                        }
                        if refresh.changed || has_completions || close_after_done_changed {
                            this.refresh_sidebar_command_pane_sessions_if_changed(cx);
                        }
                        if refresh.changed || close_after_done_changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn reschedule_project_editor_auto_sleep_if_policy_changed(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        self.reschedule_project_editor_auto_sleep_if_policy_changed_from_shared_settings(
            &settings, cx,
        );
    }

    pub(crate) fn reschedule_project_editor_auto_sleep_if_policy_changed_from_shared_settings(
        &mut self,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
        cx: &mut gpui::Context<Self>,
    ) {
        let next_policy = ProjectEditorAutoSleepPolicySnapshot::from_shared_settings(settings);
        if self.project_editor_auto_sleep_policy == next_policy {
            return;
        }

        self.project_editor_auto_sleep_policy = next_policy;
        self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
    }
}
