// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: GPUI-engine terminal reconciliation (agents + command), startup spawn, and engine terminal view/agent action event handling.

use std::sync::atomic::Ordering;
use std::time::Instant;

use gpui::AppContext as _;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /*
    CDXC:GPUITerminalGpuiEngine 2026-07-04:
    GPUI-engine terminal reconciliation runs before native host sync so
    engine-claimed sessions exclude the retained native pipeline in the same
    frame on every OS. Exit consumption mirrors the native process-exit path
    (close the exact tab, then reconcile/persist), and Mounting sessions that
    still own a live engine view (sleep→wake placeholders) promote straight
    back to Running because the composited element needs no native remount.
    */
    pub(crate) fn sync_agents_gpui_engine_terminals(&mut self, cx: &mut gpui::Context<Self>) {
        // Prune records whose shell session or runtime identity is gone;
        // dropping a record kills the child through the model. Sleeping
        // sessions drop their record too (mirroring the command pane): a
        // gxserver sleep zmx-kills the daemon, so the local attach client is
        // dead or dying, and keeping it would let the exit poll close the
        // whole tab instead of leaving the sleeping placeholder in place.
        {
            let workspace = &self.agents_workspace;
            let runtime_sessions = &self.agents_terminal_runtime_sessions;
            self.agents_gpui_engine_terminals
                .retain(|session_id, record| {
                    workspace.has_session(*session_id)
                        && runtime_sessions.runtime_session_id_for_shell_session(*session_id)
                            == Some(record.runtime_session_id)
                        && !workspace.session(*session_id).is_some_and(|session| {
                            session.presentation_state == TerminalSessionPresentationState::Sleeping
                        })
                });
        }
        #[cfg(target_os = "macos")]
        self.remote_attach_askpass_scripts.retain(|key, _| {
            self.remote_attach_sessions
                .get(key)
                .is_some_and(|session_id| self.agents_workspace.has_session(*session_id))
        });

        {
            let workspace = &self.agents_workspace;
            let records = &self.agents_gpui_engine_terminals;
            self.agents_gpui_engine_close_confirms.retain(|slot_id| {
                records.contains_key(&slot_id.session_id)
                    && workspace.is_current_terminal_body_mount_slot(*slot_id)
            });
        }

        // Consume exits like native process-exit polling: close the exact
        // tab; `wait_after_command` keeps the exited contents readable.
        let exited_session_ids = self
            .agents_gpui_engine_terminals
            .iter()
            .filter(|(_, record)| {
                !record.wait_after_command && record.view.read(cx).exit_status().is_some()
            })
            .map(|(session_id, _)| *session_id)
            .collect::<Vec<_>>();
        let mut shell_state_changed = false;
        for session_id in exited_session_ids {
            self.agents_gpui_engine_terminals.remove(&session_id);
            let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
                continue;
            };
            if self.agents_workspace.close_tab(pane_id, session_id) {
                self.forget_local_workspace_mappings_for_shell_session(session_id, cx);
                shell_state_changed = true;
            }
        }
        if shell_state_changed {
            self.agents_terminal_runtime_sessions
                .reconcile_with_workspace(&self.agents_workspace);
            self.set_shell_focus(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
            self.persist_shell_layout_state();
            cx.notify();
        }

        // Wake: a Mounting session that still owns a live engine terminal
        // becomes Running again without native startup/reattach machinery.
        let mounting_session_ids = self
            .agents_gpui_engine_terminals
            .keys()
            .copied()
            .filter(|session_id| self.agents_workspace.session_is_mounting(*session_id))
            .collect::<Vec<_>>();
        for session_id in mounting_session_ids {
            if self
                .agents_workspace
                .transition_terminal_session_presentation_state(
                    session_id,
                    TerminalSessionPresentationState::Mounting,
                    TerminalSessionPresentationState::Running,
                )
            {
                cx.notify();
            }
        }

        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        if settings.enabled {
            for slot_id in self.agents_workspace.rendered_terminal_body_mount_slots() {
                if self
                    .agents_gpui_engine_terminals
                    .contains_key(&slot_id.session_id)
                {
                    continue;
                }
                let Some(runtime_session_id) = self
                    .agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(slot_id.session_id)
                else {
                    continue;
                };
                let Some(payload) = self
                    .agents_terminal_launch_payload_source
                    .take_explicit_payload_for_mount_slot(runtime_session_id, slot_id)
                else {
                    continue;
                };
                if let Some(record) = self.spawn_gpui_engine_terminal_record(
                    GpuiEngineTerminalEventTarget::Agents(slot_id.session_id),
                    runtime_session_id,
                    payload.working_directory,
                    payload.command,
                    payload.env_vars,
                    payload.initial_input,
                    payload.wait_after_command,
                    &settings,
                    cx,
                ) {
                    self.agents_gpui_engine_terminals
                        .insert(slot_id.session_id, record);
                    cx.notify();
                } else if self
                    .agents_workspace
                    .close_tab(slot_id.pane_id, slot_id.session_id)
                {
                    // Spawn failure closes the tab honestly instead of
                    // leaving a Running session with no process behind it.
                    self.forget_local_workspace_mappings_for_shell_session(slot_id.session_id, cx);
                    self.persist_shell_layout_state();
                    cx.notify();
                }
            }
        }

        self.sync_gpui_engine_first_prompt_input_suppression(cx);
        self.sync_gpui_engine_search_totals(cx);
    }

    /*
    CDXC:GPUITerminalGpuiEngine 2026-07-06:
    Engine startup consumption: consume the same startup launch plans the
    retained native hidden-host path understands, but resolve them on every OS
    by spawning the composited GPUI-engine terminal and applying the shared
    startup result.
    Ready flows through `apply_agents_terminal_startup_result`'s
    cross-platform coordinator arm (Mounting → Running plus startup-state
    cleanup, including payload retirement); a spawn failure applies Failed so
    the tab shows the honest StartupFailed retry card instead of hanging in
    Mounting. This is the only selected startup renderer path on every OS.
    */
    pub(crate) fn spawn_agents_terminal_startup_gpui_engine_terminals(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let plans = self
            .agents_terminal_startup_coordinator
            .startup_launch_plans();
        if plans.is_empty() {
            return;
        }
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        for plan in plans {
            if self
                .agents_gpui_engine_terminals
                .contains_key(&plan.shell_session_id)
            {
                continue;
            }
            let Some(completion_intent) = self
                .agents_terminal_startup_coordinator
                .startup_completion_intents_by_runtime_session
                .get(&plan.runtime_session_id)
                .copied()
            else {
                continue;
            };
            let payload = self
                .agents_terminal_startup_launch_payload_source
                .explicit_payload_for_launch_plan(plan)
                .cloned();
            let (working_directory, command, env_vars, initial_input, wait_after_command) =
                match payload {
                    Some(payload) => (
                        payload.working_directory,
                        payload.command,
                        payload.env_vars,
                        payload.initial_input,
                        payload.wait_after_command,
                    ),
                    // No explicit payload means a plain new terminal: the
                    // engine spawn config resolves the user's default shell.
                    None => (None, None, Vec::new(), None, false),
                };
            let result = if let Some(record) = self.spawn_gpui_engine_terminal_record(
                GpuiEngineTerminalEventTarget::Agents(plan.shell_session_id),
                plan.runtime_session_id,
                working_directory,
                command,
                env_vars,
                initial_input,
                wait_after_command,
                &settings,
                cx,
            ) {
                self.agents_gpui_engine_terminals
                    .insert(plan.shell_session_id, record);
                AgentsTerminalStartupResult::Ready { completion_intent }
            } else {
                AgentsTerminalStartupResult::Failed { completion_intent }
            };
            if self.apply_agents_terminal_startup_result(result) {
                cx.notify();
            }
        }
    }

    pub(crate) fn sync_command_gpui_engine_terminals(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUICommandPaneQuitPersistence 2026-07-10:
        During app quit, dropping a composited terminal record intentionally
        detaches its zmx client. Do not reinterpret that renderer exit as the
        user's shell exiting: the normal exit path removes the command tab and
        explicitly closes the daemon-owned gxserver/zmx session.
        */
        if GPUI_APP_QUIT_IN_PROGRESS.load(Ordering::Acquire) {
            return;
        }
        {
            /*
            CDXC:GPUITerminalGpuiEngine 2026-07-04-12:40:
            Native command-tab Sleep is a renderer AND process teardown
            (TerminalWorkspaceView closeTerminal with preserveLayoutPlaceholder:
            command sessions have no persistence provider, so the shell dies and
            wake starts a fresh terminal). The engine path previously kept the
            record — and therefore a live, invisible shell — for sleeping
            command sessions because retention only checked session existence.
            Drop the local renderer/attach process when its session sleeps, but
            keep the command gxserver mapping so the daemon-side zmx session is
            not killed. Wake re-claims the slot through the daemon attach flow.
            */
            let command_pane = &self.command_pane;
            self.command_gpui_engine_terminals.retain(|session_id, _| {
                command_pane.has_session(*session_id)
                    && !command_pane
                        .session(*session_id)
                        .is_some_and(|session| session.is_sleeping)
            });
        }

        {
            let command_pane = &self.command_pane;
            let records = &self.command_gpui_engine_terminals;
            self.command_gpui_engine_close_confirms.retain(|slot_id| {
                records.contains_key(&slot_id.session_id)
                    && command_pane.is_current_terminal_body_mount_slot(*slot_id)
            });
        }
        self.prune_command_gxserver_sessions_for_command_model(cx);

        // Exit consumption mirrors the native command path, including
        // Action-run completion feedback for mapped Action sessions.
        let exited_session_ids = self
            .command_gpui_engine_terminals
            .iter()
            .filter(|(_, record)| {
                !record.wait_after_command && record.view.read(cx).exit_status().is_some()
            })
            .map(|(session_id, _)| *session_id)
            .collect::<Vec<_>>();
        let mut shell_state_changed = false;
        let mut completions = Vec::new();
        for session_id in exited_session_ids {
            self.command_gpui_engine_terminals.remove(&session_id);
            let Some((group_id, _)) = self
                .command_pane
                .flat_tab_ids()
                .into_iter()
                .find(|(_, tab_session_id)| *tab_session_id == session_id)
            else {
                continue;
            };
            let completion = self
                .command_pane
                .take_action_run_completion_for_exited_session(group_id, session_id);
            if self.command_pane.close_session(group_id, session_id) {
                self.forget_command_gxserver_session_for_closed_tab(session_id, cx);
                shell_state_changed = true;
                if let Some(completion) = completion {
                    completions.push(completion);
                }
            }
        }
        if shell_state_changed {
            self.dispatch_gpui_command_action_completions(completions, cx);
            self.prune_gpui_command_delayed_send_timers_for_command_model();
            self.prune_gpui_command_close_after_done_timers_for_command_model();
            self.clear_command_resize_hover_state_if_command_pane_hidden();
            if self.command_pane.has_sessions() {
                self.set_shell_focus(ShellFocusTarget::CommandPane);
                self.scroll_focused_command_active_tab();
            } else {
                self.restore_previous_non_command_focus_or_default();
            }
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
            self.persist_shell_layout_state();
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            cx.notify();
        }

        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        for slot_id in self.command_pane.rendered_terminal_body_mount_slots() {
            if self
                .command_gpui_engine_terminals
                .contains_key(&slot_id.session_id)
            {
                continue;
            }
            let Some(payload) = self
                .command_terminal_launch_payload_source
                .take_explicit_payload_for_mount_slot(slot_id)
            else {
                if !self
                    .command_gxserver_attach_pending
                    .contains(&slot_id.session_id)
                {
                    let title = self
                        .command_pane
                        .session(slot_id.session_id)
                        .map(|session| session.title.clone())
                        .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
                    self.start_command_terminal_gxserver_attach_for_slot(
                        slot_id, title, None, None, None, cx,
                    );
                }
                continue;
            };
            let Some(runtime_session_id) = self
                .command_gxserver_session_key_for_command_tab(slot_id.session_id)
                .as_ref()
                .map(command_terminal_runtime_session_id_from_gxserver_key)
                .or_else(|| {
                    #[cfg(target_os = "windows")]
                    if matches!(
                        windows_terminal_backend::resolve_current(),
                        Ok(windows_terminal_backend::ResolvedWindowsTerminalBackend::PowerShell)
                    ) {
                        return Some(command_terminal_runtime_session_id(slot_id));
                    }
                    None
                })
            else {
                self.close_command_terminal_after_gxserver_attach_failure(
                    slot_id,
                    "GPUI command terminal attach state was lost before launch.",
                    cx,
                );
                continue;
            };
            if let Some(record) = self.spawn_gpui_engine_terminal_record(
                GpuiEngineTerminalEventTarget::Command(slot_id.session_id),
                runtime_session_id,
                payload.working_directory,
                payload.command,
                payload.env_vars,
                payload.initial_input,
                payload.wait_after_command,
                &settings,
                cx,
            ) {
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.terminalEngine.commandSpawned",
                    serde_json::json!({
                        "groupId": slot_id.group_id.0,
                        "sessionId": slot_id.session_id.0,
                    }),
                );
                self.command_gpui_engine_terminals
                    .insert(slot_id.session_id, record);
                cx.notify();
            } else if self
                .command_pane
                .close_session(slot_id.group_id, slot_id.session_id)
            {
                self.forget_command_gxserver_session_for_closed_tab(slot_id.session_id, cx);
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.terminalEngine.commandSpawnFailedClosedTab",
                    serde_json::json!({
                        "groupId": slot_id.group_id.0,
                        "sessionId": slot_id.session_id.0,
                    }),
                );
                self.persist_shell_layout_state();
                cx.notify();
            }
        }

        /*
        CDXC:GPUITerminalGpuiEngineDiagnostics 2026-07-04-12:40:
        Grid breadcrumbs for command engine terminals under the existing
        `native.terminal.focus` scenario gate. The element resizes the model
        from prepaint bounds, so the applied cols/rows sequence recorded here
        is a faithful trace of the body rectangle the terminal actually got:
        a terminal that never leaves rows<=1 while its panel is expanded is
        rendering into a collapsed rectangle, while a terminal with no grid
        entries after spawn is never being rendered at all. Numeric ids and
        cell counts only; no terminal content, commands, paths, or keys.
        */
        {
            let records = &self.command_gpui_engine_terminals;
            self.command_gpui_engine_grid_log_states
                .retain(|session_id, _| records.contains_key(session_id));
        }
        let grid_changes = self
            .command_gpui_engine_terminals
            .iter()
            .filter_map(|(session_id, record)| {
                let grid = record.view.read(cx).model().size();
                (self.command_gpui_engine_grid_log_states.get(session_id) != Some(&grid))
                    .then_some((*session_id, grid))
            })
            .collect::<Vec<_>>();
        for (session_id, grid) in grid_changes {
            self.command_gpui_engine_grid_log_states
                .insert(session_id, grid);
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalEngine.commandGridChanged",
                serde_json::json!({
                    "sessionId": session_id.0,
                    "cols": grid.0,
                    "rows": grid.1,
                }),
            );
        }
    }

    /// Spawn one GPUI-engine terminal from launch-payload fields, wiring the
    /// view's events back into the app's runtime OSC/bell/url paths.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn spawn_gpui_engine_terminal_record(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        working_directory: Option<String>,
        command: Option<String>,
        env_vars: Vec<(String, String)>,
        initial_input: Option<String>,
        wait_after_command: bool,
        settings: &shared_settings::SharedGpuiTerminalEngineSettings,
        cx: &mut gpui::Context<Self>,
    ) -> Option<terminal_gpui_engine::GpuiEngineTerminalRecord> {
        #[cfg(target_os = "macos")]
        let mut engine_config = {
            let config_path = match shared_settings::selected_ghostty_config_path() {
                Ok(path) => path,
                Err(error) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.terminalEngine.configLoadFailed",
                        serde_json::json!({ "error": format!("{error:?}") }),
                    );
                    return None;
                }
            };
            match terminal_ghostty_surface::load_ghostty_terminal_engine_config_from_path(
                &config_path,
                terminal_gpui_engine::ghostty_theme_source(&settings.ghostty_theme),
            ) {
                Ok(config) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.terminalEngine.configLoaded",
                        serde_json::json!({
                            "hasColors": config.colors.is_some(),
                            "scrollbackLimit": config.scrollback_limit_bytes,
                            "optionAsAlt": format!("{:?}", config.option_as_alt),
                        }),
                    );
                    config
                }
                Err(error) => {
                    support_logs::append(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "gpui.terminalEngine.configLoadFailed",
                        serde_json::json!({ "error": format!("{error:?}") }),
                    );
                    return None;
                }
            }
        };
        #[cfg(not(target_os = "macos"))]
        let mut engine_config =
            terminal_gpui_engine::GpuiTerminalEngineConfig::from_shared(settings);
        #[cfg(target_os = "macos")]
        if let Some(background) = settings.terminal_background_rgb {
            engine_config.apply_terminal_background(background);
        }
        engine_config.view.scroll_to_bottom_when_typing = settings.scroll_to_bottom_when_typing;
        engine_config.view.background_image =
            terminal_gpui_engine::terminal_background_image_from_settings(settings);
        let spawn_config = terminal_gpui_engine::gpui_engine_terminal_spawn_config(
            working_directory,
            command,
            env_vars,
            engine_config.scrollback_limit_bytes,
        );
        let font = terminal_gpui_engine::gpui_engine_terminal_font_config(&engine_config);
        let (sink, event_rx) = terminal_element::TerminalView::event_channel();
        let spawn_started = Instant::now();
        let mut model = terminal_model::TerminalModel::spawn(spawn_config, sink).ok()?;
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.remoteNewTerminal.engineProcessSpawned",
            serde_json::json!({
                "durationMs": spawn_started.elapsed().as_millis() as u64,
            }),
        );
        model.set_option_as_alt(engine_config.option_as_alt);
        if let Some(colors) = &engine_config.colors {
            model
                .set_default_colors(
                    colors.foreground,
                    colors.background,
                    colors.cursor,
                    &colors.palette,
                )
                .ok()?;
        }
        let view_settings = engine_config.view.clone();
        let confirm_close_behavior =
            terminal_gpui_engine::gpui_engine_confirm_close_behavior(&engine_config);
        let view = cx.new(|cx| {
            let mut view = terminal_element::TerminalView::from_model(model, event_rx, font, cx);
            view.apply_settings(view_settings);
            if let Some(initial_input) = &initial_input {
                let _ = view.model().write_input(initial_input.as_bytes());
            }
            view
        });
        let subscription = cx.subscribe(
            &view,
            move |this: &mut Self, _view, event: &terminal_element::TerminalViewEvent, cx| {
                this.handle_gpui_engine_terminal_view_event(target, event, cx);
            },
        );
        Some(terminal_gpui_engine::GpuiEngineTerminalRecord {
            view,
            runtime_session_id,
            wait_after_command,
            confirm_close_behavior,
            _subscription: subscription,
        })
    }

    pub(crate) fn handle_gpui_engine_terminal_view_event(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        event: &terminal_element::TerminalViewEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        use terminal_element::TerminalViewEvent;

        let (runtime_session_id, agents_shell_session_id) = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                let Some(record) = self.agents_gpui_engine_terminals.get(&session_id) else {
                    return;
                };
                (record.runtime_session_id, Some(session_id))
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                let Some(record) = self.command_gpui_engine_terminals.get(&session_id) else {
                    return;
                };
                (record.runtime_session_id, None)
            }
        };
        if matches!(event, TerminalViewEvent::PromptEditorShortcutRequested) {
            self.handle_gpui_engine_prompt_editor_shortcut(target, runtime_session_id, cx);
            return;
        }
        let osc_states = if agents_shell_session_id.is_some() {
            &mut self.agents_terminal_runtime_osc_states
        } else {
            &mut self.command_terminal_runtime_osc_states
        };
        match event {
            TerminalViewEvent::TitleChanged(title) => {
                if title == TEMP_REMOTE_LOCAL_READY_TITLE || title == TEMP_REMOTE_SSH_READY_TITLE {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        if title == TEMP_REMOTE_LOCAL_READY_TITLE {
                            "TEMP.remoteNewTerminal.localWrapperReady"
                        } else {
                            "TEMP.remoteNewTerminal.remoteCommandReady"
                        },
                        serde_json::json!({ "engine": "gpui" }),
                    );
                }
                osc_states.entry(runtime_session_id).or_default().title = if title.is_empty() {
                    None
                } else {
                    Some(title.clone())
                };
                #[cfg(target_os = "windows")]
                if let Some(shell_session_id) = agents_shell_session_id {
                    self.dispatch_gpui_workspace_terminal_title_changed(
                        shell_session_id,
                        title,
                        cx,
                    );
                }
                cx.notify();
            }
            TerminalViewEvent::PwdChanged(pwd) => {
                osc_states.entry(runtime_session_id).or_default().pwd = if pwd.is_empty() {
                    None
                } else {
                    Some(pwd.clone())
                };
                cx.notify();
            }
            TerminalViewEvent::Bell => {
                let state = osc_states.entry(runtime_session_id).or_default();
                state.bell_count = state.bell_count.wrapping_add(1);
                if let Some(shell_session_id) = agents_shell_session_id {
                    self.dispatch_gpui_workspace_terminal_bell(shell_session_id, cx);
                }
                cx.notify();
            }
            TerminalViewEvent::OpenUrlRequested(url) => {
                let working_directory = osc_states
                    .get(&runtime_session_id)
                    .and_then(|state| state.pwd.clone());
                self.open_gpui_engine_terminal_action_url(url, working_directory.as_deref(), cx);
            }
            TerminalViewEvent::PasteRequested => {
                let _ = self.paste_into_focused_terminal_from_clipboard(cx);
            }
            TerminalViewEvent::ControlVRequested => {
                let _ = self.paste_image_or_send_control_v(cx);
            }
            TerminalViewEvent::PathsDropped(paths) => {
                self.insert_paths_into_gpui_engine_terminal(target, paths, cx);
            }
            TerminalViewEvent::AttachPathsRequested => {
                if let Some(attachment_target) =
                    self.gpui_terminal_attachment_target_for_engine_target(target)
                {
                    self.request_gpui_engine_terminal_attachment_paths(
                        attachment_target,
                        runtime_session_id,
                        cx,
                    );
                }
            }
            TerminalViewEvent::AgentActionRequested(action) => {
                self.handle_gpui_engine_terminal_agent_action(target, *action, cx);
            }
            TerminalViewEvent::EscapePressed => {
                if let Some(shell_session_id) = agents_shell_session_id {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.gpui.sessionInterrupt.compositedEscapeRouted",
                        serde_json::json!({
                            "shellSessionId": format!("{:?}", shell_session_id),
                        }),
                    );
                    self.dispatch_gpui_workspace_terminal_escape_pressed(shell_session_id, cx);
                }
            }
            TerminalViewEvent::FirstPromptTitleGenerationCancelRequested => {
                if let Some(shell_session_id) = agents_shell_session_id {
                    support_logs::append_temporary(
                        support_logs::GpuiSupportLog::TerminalFocus,
                        "TEMP.gpui.sessionInterrupt.titleCancelRouted",
                        serde_json::json!({
                            "shellSessionId": format!("{:?}", shell_session_id),
                        }),
                    );
                    self.dispatch_gpui_workspace_first_prompt_title_generation_cancel(
                        shell_session_id,
                        cx,
                    );
                }
            }
            TerminalViewEvent::PromptEditorShortcutRequested => {}
            TerminalViewEvent::FocusChanged { focused } => {
                #[cfg(target_os = "macos")]
                update_gpui_keyboard_router_composited_terminal_focus(
                    self.parent_ns_view,
                    target,
                    *focused,
                    self.first_responder_target,
                );
                let (surface, session_id) = match target {
                    GpuiEngineTerminalEventTarget::Agents(session_id) => ("agents", session_id.0),
                    GpuiEngineTerminalEventTarget::Command(session_id) => ("command", session_id.0),
                };
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.terminalEngine.focusChanged",
                    serde_json::json!({
                        "surface": surface,
                        "sessionId": session_id,
                        "focused": focused,
                        "activeMode": format!("{:?}", self.active_mode),
                        "shellFocus": format!("{:?}", self.shell_focus),
                        "firstResponderTarget": format!("{:?}", self.first_responder_target),
                    }),
                );
            }
            TerminalViewEvent::KeyRouteDiagnostic(route) => {
                support_logs::append(
                    support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.terminalEngine.keyDispatched",
                    serde_json::json!({
                        "action": route.action,
                        "accepted": route.accepted,
                        "consumedMods": route.consumed_mods,
                        "key": route.key_name,
                        "keyCodepoint": route.key_codepoint,
                        "keyCharCodepoint": route.key_char_codepoint,
                        "kittyKeyboardFlags": route.kitty_keyboard_flags,
                        "mods": route.mods,
                        "optionAsAltTranslation": route.option_as_alt_translation,
                        "surface": if agents_shell_session_id.is_some() { "agents" } else { "command" },
                        "terminalSessionId": agents_shell_session_id.map(|session_id| session_id.0),
                        "utf8Codepoint": route.utf8_codepoint,
                    }),
                );
            }
            // Exit consumption stays in the sync pass so ordering matches
            // the native process-exit path.
            TerminalViewEvent::Exited(_) => cx.notify(),
        }
    }

    pub(crate) fn handle_gpui_engine_terminal_agent_action(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        action: terminal_element::TerminalAgentActionRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        use terminal_element::TerminalAgentActionRequest;

        let GpuiEngineTerminalEventTarget::Agents(session_id) = target else {
            return;
        };
        if self.focused_agents_or_companion_shell_session_id() != Some(session_id) {
            return;
        }
        match action {
            TerminalAgentActionRequest::Rename => {
                let _ = self.open_gpui_rename_session_modal_for_focused_agents_session(cx);
            }
            TerminalAgentActionRequest::Sleep => {
                let Some(pane_id) = self.agents_workspace.pane_id_for_session(session_id) else {
                    return;
                };
                let _ = self.sleep_agents_tabs_for_scope(
                    pane_id,
                    session_id,
                    AgentsWorkspaceTabSleepScope::Sleep,
                    cx,
                );
            }
            TerminalAgentActionRequest::DelayedActions => {
                let _ = self.open_gpui_delayed_send_modal_for_focused_agents_session(cx);
            }
            TerminalAgentActionRequest::Fork => {
                let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                    "forkSession",
                    session_id,
                    cx,
                );
            }
            TerminalAgentActionRequest::FullReload => {
                let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                    "fullReloadSession",
                    session_id,
                    cx,
                );
            }
            /*
            CDXC:ExportTranscript 2026-08-20:
            The transcript file only exists on the machine that runs the agent,
            so the export is a daemon call, not a local read. Route it through
            the same sidebar-runtime lifecycle path Fork uses: the runtime owns
            the gxserver client for the local daemon and the authenticated
            tunnel for remote machines, and it opens the result dialog once the
            daemon answers with the written path.
            */
            TerminalAgentActionRequest::ExportTranscript => {
                let _ = self.dispatch_gpui_workspace_terminal_runtime_action(
                    "exportTranscript",
                    session_id,
                    cx,
                );
            }
            TerminalAgentActionRequest::StashPrompt => {
                self.request_gpui_stash_prompt_for_active_input(session_id, cx);
            }
            TerminalAgentActionRequest::StashedPrompts => {
                let _ = self.open_gpui_stashed_prompts_modal_for_focused_agents_session(cx);
            }
            TerminalAgentActionRequest::ToggleChatView => {
                self.handoff_agents_session_chat_mode(session_id, cx);
            }
        }
    }

}
