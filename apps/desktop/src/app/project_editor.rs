// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: project-editor view wake/sleep lifecycle and auto-sleep policy

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
        let kept_awake = self.view_modes_kept_awake_by_parked_projects();
        let marked = self.project_editor_shell.mark_mode_awake(mode, &kept_awake);
        if marked {
            self.release_capped_view_surfaces(cx);
            self.schedule_project_editor_auto_sleep_for_inactive_modes(cx);
        }
        marked
    }

    /// The active views of the projects the user left while they were awake (`active_view_awake`):
    /// neither the idle timer nor the awake cap may sleep them. The live project's entry is stale
    /// until it is left again, and its own active view is already exempt as `active_mode`.
    pub(crate) fn view_modes_kept_awake_by_parked_projects(&self) -> Vec<TitlebarMode> {
        let live_project_id = self.agents_workspace_project_id.as_deref();
        let mut modes = Vec::new();
        for (project_id, state) in &self.project_view_states_by_project {
            if state.active_view_awake
                && live_project_id != Some(project_id.as_str())
                && !modes.contains(&state.active_mode)
            {
                modes.push(state.active_mode);
            }
        }
        modes
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// The awake cap is a promise about live pages, not just a flag: `mark_mode_awake` puts the
    /// oldest view over the cap to sleep, and this is what makes that cost nothing, by handing its
    /// CEF surface back the way the Sleep menu row does. Without it a tab strip of six views would
    /// keep six renderer processes alive with only the flag saying otherwise, which is exactly what
    /// the cap exists to prevent. A sleeping view that owns no surface is left alone, so waking one
    /// view does not walk every other one through the sleep path on every click.
    pub(crate) fn release_capped_view_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        for mode in self.project_editor_shell.lifecycle_modes() {
            if mode == self.active_mode || self.project_editor_shell.is_mode_awake(mode) {
                continue;
            }
            let owns_surface = match mode {
                TitlebarMode::Browser => !self.browser_surfaces.is_empty(),
                _ => {
                    ProjectWorkareaCefSurfaceSlotKey::for_titlebar_mode(mode).is_some_and(|slot| {
                        self.project_workarea_runtime_cef_surfaces
                            .contains_key(&slot)
                    })
                }
            };
            if owns_surface {
                self.sleep_titlebar_view(mode, cx);
            }
        }
    }

    /// The project the sidebar currently has selected.
    pub(crate) fn active_sidebar_project_id(&self) -> Option<String> {
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

    pub(crate) fn schedule_project_editor_auto_sleep_for_inactive_modes(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CodeEditor 2026-06-22-08:29:
        GPUI project-editor auto-sleep is shell lifecycle behavior, not placeholder teardown. Source, Browser, Kanban, and Manage each get independent runtime timers that are rescheduled on local mode and wake mutations; only inactive awake modes can sleep, and Browser sleep only hides CEF through the existing visibility gate instead of deleting tabs or surfaces.

        CDXC:CodeEditor 2026-06-22-09:49:
        GPUI must notice effective project-editor auto-sleep policy changes while running without native settings subscriptions or filesystem watchers. Keep a runtime-only per-mode duration snapshot derived from shared settings, poll it at a fixed shell interval, and reschedule only when the effective enabled/duration policy changes so unchanged timers can still fire.

        CDXC:CodeEditor 2026-06-22-09:49:
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
                if this
                    .view_modes_kept_awake_by_parked_projects()
                    .contains(&mode)
                {
                    return;
                }
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
        CDXC:CommandPane 2026-06-24-23:36:
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
