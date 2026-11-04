//! Reconciling workspace tabs with the sidebar, attaching and resuming surfaced local and remote terminals, and per-project view state.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn reconcile_local_workspace_tabs_with_sidebar(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(tab_sessions) = focus_state.active_project_tab_sessions.as_deref() else {
            return false;
        };
        if self.agents_workspace_project_id.as_deref() != focus_state.active_project_id.as_deref() {
            return false;
        }
        // An empty list clears every tab of the project, so the Rust store has to agree that the project has none (gx_store/local_focus.rs, `confirms_empty_tab_list`).
        if !self.gx_store_allows_tab_reconcile(
            focus_state.active_project_id.as_deref(),
            tab_sessions.len(),
        ) {
            return false;
        }
        /*
        CDXC:Workarea 2026-08-01:
        A tab just dragged out of the command pane is live locally but its
        gxserver row is still on the `commands` surface, so it is absent from
        this projection by definition. Reconciling against that projection
        would delete the session and take the running terminal with it, so
        skip the pass entirely while any transfer is mid-flight. The hold is
        released on daemon confirmation or after a bounded retry budget, and
        the next sidebar patch reconciles normally.
        */
        if !self.agents_sessions_pending_surface_transfer.is_empty() {
            return false;
        }
        if self.has_unbound_agent_chat_launch() {
            return false;
        }
        let keyboard_owner_before = self.keyboard_owner_session();
        let changed = self.agents_workspace.reconcile_with_sidebar_tab_sessions(
            focus_state.active_project_id.as_deref(),
            tab_sessions,
            &mut self.local_workspace_session_mappings,
            &mut self.remote_attach_sessions,
        );
        if changed {
            self.agents_terminal_startup_body_slot_geometries
                .retain(|slot_id, _| {
                    self.agents_workspace
                        .is_current_terminal_startup_body_slot(*slot_id)
                });
            /*
            CDXC:FocusRouting 2026-10-10 WHY:
            A session closed from its sidebar row, its menu or another client leaves the workspace here, and the terminal that held the keyboard goes with it. Nothing handed the keyboard on, so GPUI focus pointed at a dropped view and keys and hotkeys did nothing until the user clicked the window. The focused pane takes it, as it does when a tab's process exits (`follow_shell_focus_after_surface_removed`), and only when the removed tab was the keyboard owner.
            */
            if self.keyboard_owner_session_was_removed(keyboard_owner_before) {
                self.follow_shell_focus_after_surface_removed(
                    ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                    keyboard_owner_before,
                    cx,
                );
            }
        }
        self.attach_surfaced_local_workspace_terminals(focus_state, cx);
        changed
    }

    pub(crate) fn attach_surfaced_local_workspace_terminals(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-07-24 (revised 2026-08-07):
        Every terminal the restored split layout currently surfaces — not only
        the focused pane — needs its own process-local gxserver attach payload
        after restart. "Surfaced" is read from the live workspace model (a
        rendered pane's active tab), which is the only current statement of
        what is on screen. It used to be gated on the presentation snapshot's
        visible session ids as well, but that set is a snapshot of what *was*
        surfaced when it was published: after a session closed while the app
        was shut down, reconcile promotes a background tab to active and that
        promoted tab is absent from the stale set, so the pane the user is
        looking at stayed empty until clicked. Prepare those exact active
        pane/session slots without selecting a tab, moving pane focus, or
        publishing a new focused session.
        */
        /*
        The remote and wake passes read the live workspace model rather than
        the local projection, so they run before its guard: a remote workspace
        or a fully-sleeping project must resume even on a snapshot that carries
        no local tab rows.
        */
        if let Some(remote_machine_id) = self
            .agents_workspace_project_id
            .as_deref()
            .and_then(gpui_remote_project_reference_from_project_id)
            .map(|remote_project| remote_project.remote_machine_id)
        {
            self.attach_surfaced_remote_workspace_terminals(&remote_machine_id, cx);
        }
        self.resume_restored_workspace_surfaced_terminals(focus_state, cx);
        let Some(tab_sessions) = focus_state.active_project_tab_sessions.as_deref() else {
            return;
        };
        let rendered_pane_ids = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .collect::<HashSet<_>>();
        let candidates = tab_sessions
            .iter()
            .filter_map(|session| {
                let key = session.key.as_local()?;
                if session.presentation_state != TerminalSessionPresentationState::Running {
                    return None;
                }
                let shell_session_id = self.local_workspace_session_mappings.get(key).copied()?;
                let pane_id = self
                    .agents_workspace
                    .pane_id_for_session(shell_session_id)?;
                (rendered_pane_ids.contains(&pane_id)
                    && self.agents_workspace.active_session_in_pane(pane_id)
                        == Some(shell_session_id)
                    && self.agents_tab_selected_local_runtime_missing(pane_id, shell_session_id))
                .then(|| (key.clone(), pane_id))
            })
            .collect::<Vec<_>>();

        for (key, pane_id) in candidates {
            self.spawn_local_workspace_attach_plan(
                key,
                GpuiLocalWorkspaceAttachIntent::Attach,
                pane_id,
                true,
                GpuiLocalWorkspaceAttachOrigin::SurfacedRestore,
                cx,
            );
        }
    }

    pub(crate) fn resume_restored_workspace_surfaced_terminals(
        &mut self,
        focus_state: &GpuiGxserverPresentationFocusState,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Workarea 2026-09-23 WHY:
        A restored workspace keeps the sessions its panes surfaced at quit, but their daemon providers may have gone to sleep (auto-sleep, or the machine rebooted) while the app was closed, and attach alone cannot show a session with no zmx provider. So the first visit to each restored project wakes its panes' surfaced-but-sleeping sessions once; later sleeps are user decisions, so the project key is consumed on that pass and never re-armed.
        Only a return to the project does this. It runs on the first focus snapshot that describes this project, and when that snapshot heads for a session no pane surfaces (a new agent, or a background row) it wakes nothing: the requested session owns the visit and the covered sessions stay asleep until clicked. Waking them anyway respawned sessions the user had not asked for, and before R7 the wake's result also took focus from the new agent.
        Supersedes the 2026-08-07 note, which woke on the first authoritative pass whatever it was for.
        With Click to Wake Sleeping Panes on, the pass wakes nothing at all and those panes show their wake placeholder; see the SessionSleep decision on `select_sleeping_local_workspace_tab`.
        */
        let Some(project_id) = self.agents_workspace_project_id.clone() else {
            return;
        };
        if focus_state.active_project_id.as_deref() != Some(project_id.as_str())
            || !self.startup_restore_wake_pending.remove(&project_id)
            || gpui_click_to_wake_sleeping_sessions_from_shared_settings(
                &shared_settings::shared_sidebar_settings_snapshot(),
            )
        {
            return;
        }
        let surfaced = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                self.agents_workspace
                    .active_session_in_pane(pane_id)
                    .map(|session_id| (pane_id, session_id))
            })
            .collect::<Vec<_>>();
        if let Some(requested_session_id) = focus_state.focused_session_id.as_deref()
            && !surfaced.iter().any(|(_, session_id)| {
                match self.workspace_terminal_key_for_shell_session(*session_id) {
                    Some(GpuiWorkspaceTerminalSessionKey::Local(key)) => {
                        key.session_id == requested_session_id
                    }
                    Some(GpuiWorkspaceTerminalSessionKey::Remote(key)) => {
                        gpui_remote_scoped_session_id(
                            &key.remote_machine_id,
                            &key.project_id,
                            &key.session_id,
                        ) == requested_session_id
                    }
                    None => false,
                }
            })
        {
            return;
        }
        let focused_pane_id = self.agents_workspace.focused_pane;
        for (pane_id, session_id) in surfaced {
            if self
                .agents_workspace
                .session(session_id)
                .is_none_or(|session| {
                    session.presentation_state != TerminalSessionPresentationState::Sleeping
                })
            {
                continue;
            }
            let mutation_kind = if pane_id == focused_pane_id {
                GpuiLocalWorkspaceLifecycleMutationKind::DirectWake
            } else {
                GpuiLocalWorkspaceLifecycleMutationKind::RestoreWake
            };
            self.request_mapped_sleeping_agents_terminal_wake(
                pane_id,
                session_id,
                mutation_kind,
                cx,
            );
        }
    }

    pub(crate) fn current_project_view_state(&self) -> GpuiProjectViewState {
        let shown_mode = self.available_titlebar_mode_or_agents(self.active_mode);
        // While a restore waits for this project's scope, the panel is closed only provisionally:
        // a capture on leaving or a shell state write in that window keeps the saved view. It is
        // not on screen, so it is not held awake for the way back.
        let active_mode = match &self.pending_project_view_restore {
            Some(pending)
                if self.active_mode == TitlebarMode::Agents
                    && self.agents_workspace_project_id.as_ref() == Some(&pending.project_id) =>
            {
                pending.saved_mode
            }
            _ => shown_mode,
        };
        GpuiProjectViewState {
            active_mode,
            open_views: self.open_views.clone(),
            view_strip_layout: self.view_strip_layout.clone(),
            last_view_mode: self.last_open_view_mode,
            workarea_split_ratio: self.project_editor_shell.workarea_split_ratio,
            active_view_awake: self.project_editor_shell.is_mode_awake(shown_mode),
            code_windows: self.code_windows.clone(),
        }
    }

    pub(crate) fn project_view_states_for_shell_state(
        &self,
    ) -> HashMap<String, GpuiProjectViewState> {
        /*
        CDXC:Navigation 2026-08-07:
        The live project's view is only in the app fields, never in the map, so
        the writer folds it in at serialization time. Without this, quitting
        while on a project would persist that project's view as of the last
        time the user switched away from it.
        */
        let mut states = self.project_view_states_by_project.clone();
        if let Some(project_id) = self.agents_workspace_project_id.clone() {
            states.insert(project_id, self.current_project_view_state());
        }
        states
    }

    pub(crate) fn capture_outgoing_project_view_state(&mut self) {
        self.capture_view_pane_layout();
        if let Some(project_id) = self.agents_workspace_project_id.clone() {
            let state = self.current_project_view_state();
            self.project_view_states_by_project
                .insert(project_id, state);
        }
    }

    pub(crate) fn apply_project_view_state_for_active_project(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:Navigation 2026-08-07:
        Restore the incoming project's own view: which one its panel shows, the one it last had open,
        and how wide its sessions column is beside it.
        */
        self.pending_project_view_restore = None;
        let Some(state) = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| self.project_view_states_by_project.get(project_id))
            .cloned()
        else {
            /*
            CDXC:Workarea 2026-09-20 WHY:
            A project this app has not seen before starts with no tabs at all, not with the previous
            project's strip: its tabs belong to it, and inheriting them would open pages in a project
            the user never asked to open them in.

            CDXC:Workarea 2026-10-25 WHY:
            The outgoing project's view goes with its tabs. Keeping `active_mode` showed that view in
            the new project with no tab in the strip (a lone ×), and a later switch to the same view,
            such as `ghostex edit` landing a file in Code, was a no-op that never added its tab. With
            no view here the panel follows the same rule as a project whose saved view is Agents.
            */
            self.open_views.clear();
            self.view_strip_layout = GpuiViewStripLayout::default();
            self.code_windows = CodeWindows::default();
            self.view_panel_maximized = false;
            self.active_mode = TitlebarMode::Agents;
            self.last_open_view_mode = None;
            self.apply_view_pane_state(cx);
            self.seed_terminal_view_for_open(cx);
            self.focus_default_surface_for_active_mode(cx);
            self.update_active_mode_cef_child_visibility(cx);
            return;
        };
        let workarea_span = self
            .workarea_split_layout_metrics
            .map(|metrics| metrics.content_span);
        self.project_editor_shell
            .restore_workarea_split_ratio(state.workarea_split_ratio, workarea_span);
        let target_mode = self.available_titlebar_mode_or_agents(state.active_mode);
        if target_mode != state.active_mode {
            self.pending_project_view_restore =
                self.agents_workspace_project_id.clone().map(|project_id| {
                    PendingProjectViewRestore {
                        project_id,
                        saved_mode: state.active_mode,
                        at_launch: false,
                    }
                });
        }
        self.last_open_view_mode = state
            .last_view_mode
            .filter(|mode| *mode != TitlebarMode::Agents)
            .or(self.open_view_mode_for(target_mode));
        // The outgoing project was already captured before the workspace swap.
        // Do not record its live pane values under the incoming project here.
        self.open_views = state
            .open_views
            .iter()
            .copied()
            .filter(|mode| *mode != TitlebarMode::Agents)
            .collect();
        if target_mode != TitlebarMode::Agents && !self.open_views.contains(&target_mode) {
            self.open_views.push(target_mode);
        }
        self.view_strip_layout = state.view_strip_layout.clone();
        self.code_windows = state.code_windows.clone();
        self.active_mode = target_mode;
        // The picker belongs to the panel, not to a project, so it survives a switch only while the
        // incoming project has no view of its own to show.
        self.view_panel_picker_open =
            self.view_panel_picker_open && target_mode == TitlebarMode::Agents;
        // A saved view that hands over after the switch keeps the panel expanded, as closing its
        // tab does.
        self.view_panel_maximized = self.view_panel_maximized
            && (target_mode != TitlebarMode::Agents
                || self.view_panel_picker_open
                || self.pending_project_view_restore.is_some());
        self.apply_view_pane_state(cx);
        self.seed_terminal_view_for_open(cx);
        self.focus_shell_target(
            default_shell_focus_for_mode(
                target_mode,
                &self.agents_workspace,
                &self.project_editor_shell,
            ),
            cx,
        );
        self.update_active_mode_cef_child_visibility(cx);
    }

    pub(crate) fn agents_remote_connect_status_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<(&'static str, Option<&'static str>)> {
        /*
        CDXC:RemoteMachines 2026-08-07:
        A remote tab can only show content once its machine's tunnel is up, so
        while that machine has no live connection the body states why instead
        of rendering an empty rectangle. Local sessions never qualify, and a
        connected machine returns nothing so the overlay disappears the moment
        the attach can proceed.

        CDXC:RemoteMachines 2026-08-14:
        The launch wrapper now keeps a remote-attach terminal's process alive
        and reconnects its own SSH session after drops, so a Running tab can
        have live, typeable content while the machine-level tunnel is still
        re-establishing. The overlay explains an empty body; it must never
        cover a Running terminal that the user can already interact with.
        */
        let key = match self.workspace_terminal_key_for_shell_session(session_id)? {
            GpuiWorkspaceTerminalSessionKey::Remote(key) => key,
            GpuiWorkspaceTerminalSessionKey::Local(_) => return None,
        };
        if self
            .agents_workspace
            .session(session_id)
            .is_some_and(|session| {
                session.presentation_state == TerminalSessionPresentationState::Running
            })
        {
            return None;
        }
        if self
            .remote_gxserver_connections
            .contains_key(&key.remote_machine_id)
            && self
                .remote_machine_connect_states
                .get(&key.remote_machine_id)
                .map(String::as_str)
                == Some(GpuiRemoteGxserverConnectState::Connected.wire_status_state())
        {
            return None;
        }
        // A connected Cloud Box needs no overlay; its pane is re-armed as it connects.
        if gpui_is_cloud_box_machine(&key.remote_machine_id)
            && self
                .gpui_remote_gxserver_request_target(&key.remote_machine_id)
                .is_some()
        {
            return None;
        }
        Some(gpui_remote_connect_overlay_labels(
            self.remote_machine_connect_states
                .get(&key.remote_machine_id)
                .map(String::as_str),
        ))
    }

    pub(crate) fn attach_surfaced_remote_workspace_terminals(
        &mut self,
        remote_machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:RemoteMachines 2026-08-07:
        The remote counterpart of `attach_surfaced_local_workspace_terminals`.
        Parking a workspace always kills its SSH clients, so every restored
        remote tab is dead by definition and used to need a tab-strip or
        sidebar click to come back. Re-arm each surfaced remote tab of the live
        remote workspace as soon as its machine reports connected — the same
        "prepare a plan, insert the payload into that exact mount slot" shape
        the local path uses. This deliberately does not select tabs, move pane
        focus, change the titlebar mode, or publish presentation focus: the
        restored layout already says what is surfaced, and a split's other
        panes must re-arm without fighting each other for focus.
        Parked projects reuse shell ids, so both plan lookup and completion must
        resolve the complete remote key in the currently active project.
        */
        let Some(active_project_id) = self.agents_workspace_project_id.clone() else {
            return;
        };
        let Some(remote_project) =
            gpui_remote_project_reference_from_project_id(&active_project_id)
        else {
            return;
        };
        if remote_project.remote_machine_id != remote_machine_id {
            return;
        }
        let Some(target) = self.gpui_remote_gxserver_request_target(remote_machine_id) else {
            return;
        };
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        // A Cloud Box's restored pane re-arms through its gxserver too (helpers/remote/
        // cloud_box_attach.rs), once gx_store/cloud_boxes.rs has connected it.
        let Some(config) =
            gpui_remote_attach_machine_config(settings_snapshot.object(), remote_machine_id)
        else {
            return;
        };
        let candidates = self
            .agents_workspace
            .rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                self.agents_workspace
                    .active_session_in_pane(pane_id)
                    .map(|session_id| (pane_id, session_id))
            })
            .filter(|(pane_id, session_id)| {
                self.agents_tab_selected_local_runtime_missing(*pane_id, *session_id)
            })
            .filter_map(|(pane_id, session_id)| {
                self.agents_chat_remote_key_for_session(session_id)
                    .map(|key| (pane_id, session_id, key))
            })
            .collect::<Vec<_>>();

        for (pane_id, session_id, key) in candidates {
            let reference = GpuiRemoteAttachSessionReference {
                remote_machine_id: key.remote_machine_id.clone(),
                project_id: key.project_id.clone(),
                session_id: key.session_id.clone(),
            };
            self.prepare_gpui_remote_attach_request(
                reference,
                config.clone(),
                target.clone(),
                crate::app::remote_conn::attach_request::RemoteAttachRequestIntent::Restore {
                    pane_id,
                    session_id,
                },
                cx,
            );
        }
    }

    pub(crate) fn arm_surfaced_remote_workspace_terminal(
        &mut self,
        key: &GpuiRemoteAttachSessionKey,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        plan: GpuiRemoteAttachTerminalPlan,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        The SSH round trip runs in the background, so the layout may have moved
        on. Mirror the local `SurfacedRestore` completion guard exactly: the
        mapping, the pane placement, and the empty-runtime condition must all
        still hold, otherwise this payload belongs to a slot that no longer
        exists.
        */
        if self.agents_chat_remote_key_for_session(session_id).as_ref() != Some(key)
            || self.agents_workspace.pane_id_for_session(session_id) != Some(pane_id)
            || self.agents_workspace.active_session_in_pane(pane_id) != Some(session_id)
            || !self.agents_tab_selected_local_runtime_missing(pane_id, session_id)
        {
            return;
        }
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        let env_vars = gpui_remote_ssh_terminal_environment(plan.askpass.as_ref());
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let env_vars = Vec::new();
        let payload = AgentsTerminalExplicitLaunchPayload {
            working_directory: None,
            command: Some(plan.terminal_command),
            env_vars,
            initial_input: None,
            wait_after_command: false,
        };
        if payload.validate_launch_strings().is_err() {
            return;
        }
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        self.agents_terminal_launch_payload_source
            .insert_explicit_payload_for_mount_slot(
                runtime_session_id,
                AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id,
                },
                payload,
            );
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        if let Some(askpass) = plan.askpass {
            self.remote_attach_askpass_scripts
                .insert(key.clone(), askpass);
        }
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.remoteAttach.terminalOpened",
            serde_json::json!({
                "machineId": key.remote_machine_id,
                "mode": "surfacedRestore",
                "sessionId": key.session_id,
            }),
        );
        self.persist_shell_layout_state();
        cx.notify();
    }
}
