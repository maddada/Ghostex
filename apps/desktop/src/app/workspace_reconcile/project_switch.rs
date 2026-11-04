//! Project context payloads, coalesced project switches and swapping the agents workspace and command pane to the active project.

use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;

use gpui::Window;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::view_strip_order::ViewStripEntry;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn receive_sidebar_project_context_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CefRuntime 2026-06-22-19:32:
        Live sidebar project context is accepted only through the slice-91 active-project contract and stored as runtime memory. Slice 92 intentionally does not replace titlebar availability, remove the strict env bridge, persist project facts, log raw JSON, or infer projects from .git, names, paths, fixtures, workspace labels, URLs, or filesystem markers.

        CDXC:CefRuntime 2026-06-22-19:44:
        Slice 93 makes valid stored sidebar snapshots the runtime availability source for App titlebar/workarea decisions while retaining the strict env bridge for startup/tests before any sidebar message arrives. Malformed payloads must not replace the previous snapshot or coerce the active mode.

        CDXC:Titlebar 2026-06-22-19:57:
        Slice 95 makes the visible titlebar project label runtime-only sidebar snapshot state. A valid active-project payload updates the label from the stored snapshot display name in memory; malformed payloads leave the existing label unchanged and must not persist, log, or derive display labels from env vars, repo folders, .git, workspace names, fixture names, paths, URLs, or sidebar titles.

        CDXC:CefRuntime 2026-06-23-06:53:
        Duplicate valid active-project payloads are accepted as a bridge heartbeat but are not a project change. Only a changed stored snapshot may refresh the titlebar label, coerce project-scoped mode availability, or notify GPUI, and malformed payloads still preserve the prior snapshot.

        CDXC:Workarea 2026-06-29-00:02:
        Active-project changes no longer reconcile Source/Browser/Kanban/Automate/Manage readiness stores. The stored snapshot immediately feeds titlebar availability and the direct runtime URL/CEF gates, so stale proof state cannot keep or block a workarea surface.
        */
        // Since 2026-09-25 the payload is the store's own (gx_store/focus_publish.rs), built from the
        // one focus there is, so nothing can be older than a newer local selection any more.
        let stored = store_latest_gpui_project_snapshot_from_sidebar_contract_json(
            &mut self.latest_sidebar_project_snapshot,
            payload,
            |_, _| true,
        );
        match stored {
            Ok(GpuiProjectSnapshotStoreResult::Changed) => {
                /*
                CDXC:Navigation 2026-07-29:
                The parsed snapshot is stored before the coalescing gate, so a
                collapsed request needs no payload: replaying it applies
                whatever the newest stored snapshot is, and duplicate
                heartbeats that store as `Unchanged` cannot strand the queue.
                */
                let target_project_id = gpui_active_project_id_from_snapshot(
                    self.latest_sidebar_project_snapshot.as_ref(),
                )
                .map(str::to_string);
                if self.project_switch_request_is_coalesced(
                    target_project_id.as_deref(),
                    GpuiProjectSwitchRequestKind::ActiveProjectContext,
                ) {
                    self.enqueue_coalesced_project_switch_request(
                        target_project_id,
                        GpuiPendingProjectSwitchPayload::ActiveProjectContext,
                        cx,
                    );
                    return;
                }
                self.apply_changed_active_project_snapshot(window, cx);
            }
            Ok(GpuiProjectSnapshotStoreResult::Unchanged) | Err(_) => {}
        }
    }

    pub(crate) fn apply_changed_active_project_snapshot(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.project_name = titlebar_project_label_from_latest_sidebar_snapshot(
            self.latest_sidebar_project_snapshot.as_ref(),
        );
        self.gx_store_git_active_project_changed(cx);
        self.restore_gpui_titlebar_project_selections();
        self.refresh_titlebar_actions_in_background(cx);
        self.swap_agents_workspace_for_active_project(cx);
        self.swap_command_pane_for_active_project(window, cx);
        self.swap_browser_tabs_for_active_project(cx);
        self.refresh_project_workarea_runtime_cef_surfaces_from_runtime_state(cx);
        self.refresh_sidebar_gxserver_bootstrap_if_changed(cx);
        self.finish_project_view_restore(cx);
        self.coerce_active_mode_to_available_project_context(cx);
        self.land_quick_automations_active_project_on_automate_mode(window, cx);
        self.land_pending_source_file_open_on_source_mode(window, cx);
        self.gx_store_land_pending_browser_open(window, cx);
        self.land_pending_parked_browser_open(window, cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.broadcast_extension_context_changes(cx);
        cx.notify();
    }

    /// Settles `pending_project_view_restore` once the restored project's workspace, command and
    /// Browser models, project context and sidebar HUD all belong to it (tried when the context is
    /// applied and when a sidebar snapshot is installed, whichever comes last): the saved view comes
    /// back when this project can show it after all, and otherwise hands over the way closing its
    /// tab does. A view opened in between wins; opening the picker or closing the panel cancels it.
    /// A restore a launch picked up shows its view asleep, as the launch would have.
    pub(crate) fn finish_project_view_restore(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(pending) = self.pending_project_view_restore.as_ref() else {
            return;
        };
        let project = Some(pending.project_id.as_str());
        if self.agents_workspace_project_id.as_deref() != project
            || self.command_pane_project_id.as_deref() != project
            || self.browser_tabs_project_id.as_deref() != project
            || !self.view_scope_describes_project(&pending.project_id)
        {
            return;
        }
        let (saved, at_launch) = (pending.saved_mode, pending.at_launch);
        self.pending_project_view_restore = None;
        if self.active_mode != TitlebarMode::Agents {
            return;
        }
        let successor = if self.titlebar_mode_available(saved) {
            Some(ViewStripEntry::View(saved))
        } else {
            self.leaving_view_successor(saved)
        };
        self.show_leaving_view_successor(successor, at_launch, cx);
        if self.active_mode == TitlebarMode::Browser {
            self.defer_in_main_window(cx, |app, window, cx| {
                app.sync_browser_address_input_if_shown(window, cx);
            });
        }
    }

    /*
    CDXC:Navigation 2026-07-29:
    The single decision point every sidebar-driven project switch passes
    through. Returns true when the caller must stop and hand its request to the
    trailing flush instead of executing it now.

    - Same-project requests bypass entirely: a different session inside the
      already-active project is a cheap intra-project focus change and must
      stay instant.
    - The first cross-project request while nothing is settling executes
      immediately (leading edge), so single clicks never get slower, and opens
      the settle window that collapses the clicks behind it.
    - An executing request is newer than everything queued behind it, so it
      drops the whole queue. That is what makes A -> B -> A collapse to zero
      extra work: the queued B request is discarded and its swap never runs.
    */
    pub(crate) fn project_switch_request_is_coalesced(
        &mut self,
        target_project_id: Option<&str>,
        kind: GpuiProjectSwitchRequestKind,
    ) -> bool {
        let same_project = target_project_id == self.agents_workspace_project_id.as_deref();
        if !same_project
            && self
                .project_switch_settling_until
                .is_some_and(|until| Instant::now() < until)
        {
            return true;
        }
        self.project_switch_pending_requests.clear();
        if !same_project && kind.opens_settle_window() {
            self.project_switch_settling_until =
                Some(Instant::now() + GPUI_PROJECT_SWITCH_SETTLE_WINDOW);
        }
        false
    }

    /// Queues the collapsed request. The queue keeps the newest request per
    /// bridge kind in arrival order, and only ever holds requests for one
    /// target project, so a newer target discards the stale ones.
    pub(crate) fn enqueue_coalesced_project_switch_request(
        &mut self,
        target_project_id: Option<String>,
        payload: GpuiPendingProjectSwitchPayload,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = payload.kind();
        self.project_switch_pending_requests.retain(|pending| {
            pending.target_project_id == target_project_id && pending.payload.kind() != kind
        });
        self.project_switch_pending_requests
            .push(GpuiPendingProjectSwitchRequest {
                target_project_id,
                payload,
            });
        let remaining = self
            .project_switch_settling_until
            .map(|until| until.saturating_duration_since(Instant::now()))
            .unwrap_or_default();
        // macOS TerminalFocusDebugLog parity (scenario native.terminal.focus):
        // bounded gxserver ids, kinds, counts, and durations only.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.projectSwitchCoalesced",
            serde_json::json!({
                "activeProjectId": self.agents_workspace_project_id,
                "pendingRequestCount": self.project_switch_pending_requests.len(),
                "requestKind": kind.breadcrumb_id(),
                "settleRemainingMs": remaining.as_millis() as u64,
                "targetProjectId": self
                    .project_switch_pending_requests
                    .last()
                    .and_then(|pending| pending.target_project_id.clone()),
            }),
        );
        self.schedule_coalesced_project_switch_flush(remaining, cx);
    }

    pub(crate) fn schedule_coalesced_project_switch_flush(
        &mut self,
        delay: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.project_switch_flush_scheduled {
            return;
        }
        self.project_switch_flush_scheduled = true;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.flush_coalesced_project_switch_requests(window, cx);
            });
        })
        .detach();
    }

    /// Trailing edge: replay the newest collapsed request per bridge kind, in
    /// arrival order, through the ordinary handlers.
    pub(crate) fn flush_coalesced_project_switch_requests(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.project_switch_flush_scheduled = false;
        if self.project_switch_pending_requests.is_empty() {
            // The backstop in the bridge dispatcher can land the queue before
            // this timer fires. A late no-op flush must not close a settle
            // window that a newer switch has since opened.
            return;
        }
        self.project_switch_settling_until = None;
        let pending = std::mem::take(&mut self.project_switch_pending_requests);
        for request in pending {
            let kind = request.payload.kind();
            // The queued target can have caught up with the live project (the
            // A -> B -> A shape), in which case the swap guards inside the
            // swap helpers make the replay a no-op instead of a second
            // teardown. Record which case this was.
            let redundant_swap =
                request.target_project_id.as_deref() == self.agents_workspace_project_id.as_deref();
            support_logs::append(
                support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.projectSwitchTrailingReplay",
                serde_json::json!({
                    "activeProjectId": self.agents_workspace_project_id,
                    "redundantSwap": redundant_swap,
                    "requestKind": kind.breadcrumb_id(),
                    "targetProjectId": request.target_project_id,
                }),
            );
            match request.payload {
                GpuiPendingProjectSwitchPayload::ActiveProjectContext => {
                    if !redundant_swap {
                        self.project_switch_settling_until =
                            Some(Instant::now() + GPUI_PROJECT_SWITCH_SETTLE_WINDOW);
                    }
                    self.apply_changed_active_project_snapshot(window, cx);
                }
                GpuiPendingProjectSwitchPayload::GxserverPresentationFocusState(state) => {
                    self.set_sidebar_gxserver_presentation_focus_state(state, cx);
                }
                GpuiPendingProjectSwitchPayload::WorkspaceTerminalFocus(message) => {
                    self.focus_local_workspace_terminal_from_message(&message, cx);
                }
            }
        }
    }

    pub(crate) fn swap_agents_workspace_for_active_project(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        self.swap_agents_workspace_to_project_id(new_project_id, cx)
    }

    pub(crate) fn swap_agents_workspace_to_project_id(
        &mut self,
        new_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.agents_workspace_project_id == new_project_id {
            return false;
        }
        self.source_code_server_runtime
            .pending_remote_prompt_editor_request = None;
        self.capture_outgoing_project_view_state();
        if self.agents_workspace_project_id.is_some() {
            self.park_project_workarea_surfaces_before_switch(cx);
        }
        if self.agents_workspace_project_id.is_none()
            && new_project_id.as_ref().is_some_and(|project_id| {
                !self
                    .parked_agents_workspaces_by_project
                    .contains_key(project_id)
                    && self
                        .local_workspace_session_mappings
                        .keys()
                        .all(|key| key.project_id == *project_id)
            })
        {
            self.agents_workspace_project_id = new_project_id;
            self.apply_project_view_state_for_active_project(cx);
            self.persist_shell_layout_state();
            return true;
        }

        /*
        CDXC:Workarea 2026-07-23:
        Agents split/tab topology is project/worktree-owned. Park the complete
        outgoing writer-owned shell model plus its canonical session mappings
        before activating another project. Reconciliation may then update only
        the incoming model, so it can never normalize away another project's
        split branches or delete its shell sessions.
        */
        let outgoing_state = agents_workspace_project_state_to_shell_state_json(
            &self.agents_workspace,
            self.agents_workspace_project_id.as_deref(),
            &self.local_workspace_session_mappings,
            &self.remote_attach_sessions,
            &self.agents_chat_mode_sessions,
            &self.agents_chat_auto_switch_observed_sessions,
            &self.agents_delayed_send_timers,
            &self.agents_send_when_stopped_watchers,
            SystemTime::now(),
        );
        let kept_alive_viewer_sessions = self.agents_terminal_keep_alive_viewer_sessions();
        self.park_agents_gpui_engine_terminal_zmx_clients(cx);
        self.release_unused_agents_gpui_terminal_viewers(true, &kept_alive_viewer_sessions, cx);
        let zmx_session_names = self
            .agents_workspace
            .terminal_sessions
            .iter()
            .filter(|session| {
                self.agents_gpui_engine_terminals.contains_key(&session.id)
                    || self
                        .agents_gpui_terminal_viewer_recipes
                        .contains_key(&session.id)
            })
            .filter_map(|session| {
                session
                    .zmx_session_name
                    .clone()
                    .map(|name| (session.id, name))
            })
            .collect();
        /*
        CDXC:SessionChat 2026-08-26:
        Chat pages park with the terminal owners rather than being destroyed and
        reloaded, so the outgoing project's Chromium browsers survive the switch.
        The bundle is taken before the ownership branch below because it also
        clears the switch-scoped chat state that must go regardless of whether
        there is a project id to park it under.
        */
        let parked_chat_runtime = self.park_all_agents_chat_surfaces(cx);
        match self.agents_workspace_project_id.take() {
            Some(old_project_id) => {
                self.parked_agents_workspaces_by_project
                    .insert(old_project_id.clone(), outgoing_state);
                self.parked_agents_terminal_runtimes_by_project.insert(
                    old_project_id.clone(),
                    ParkedAgentsTerminalRuntime {
                        zmx_session_names,
                        protected_viewer_sessions: self
                            .agents_gpui_engine_terminals
                            .keys()
                            .copied()
                            .filter(|id| self.agents_terminal_viewer_has_pending_work(*id))
                            .collect(),
                        viewer_recipes: std::mem::take(
                            &mut self.agents_gpui_terminal_viewer_recipes,
                        ),
                        runtime_sessions: std::mem::take(
                            &mut self.agents_terminal_runtime_sessions,
                        ),
                        gpui_engine_terminals: std::mem::take(
                            &mut self.agents_gpui_engine_terminals,
                        ),
                        runtime_osc_states: std::mem::take(
                            &mut self.agents_terminal_runtime_osc_states,
                        ),
                        gpui_engine_close_confirms: std::mem::take(
                            &mut self.agents_gpui_engine_close_confirms,
                        ),
                        kept_alive_viewer_sessions,
                        parked_at: Some(Instant::now()),
                    },
                );
                // A replaced parking's views drop here, which detaches them from the chat host.
                drop(
                    self.parked_agents_chat_runtimes_by_project
                        .insert(old_project_id, parked_chat_runtime),
                );
            }
            // No owning project id means there is nothing to park these pages
            // under and nothing that could ever restore them, so they are
            // destroyed here exactly as the pre-parking teardown did.
            None => {
                drop(parked_chat_runtime);
            }
        }

        let restored_state = new_project_id.as_ref().and_then(|project_id| {
            self.parked_agents_workspaces_by_project
                .remove(project_id)
                .and_then(|state| {
                    agents_workspace_project_state_from_shell_state(&state, Some(project_id))
                })
                .filter(|(_, mappings, _, _, _, _)| {
                    mappings.keys().all(|key| key.project_id == *project_id)
                })
        });
        let (
            workspace,
            mappings,
            remote_mappings,
            chat_mode_sessions,
            chat_view_observations,
            delayed_send_restore_intents,
        ) = restored_state.unwrap_or_else(|| {
            (
                WorkspaceModel::empty_default(),
                HashMap::new(),
                HashMap::new(),
                HashSet::new(),
                HashMap::new(),
                Vec::new(),
            )
        });
        self.agents_workspace = workspace;
        self.local_workspace_session_mappings = mappings;
        self.remote_attach_sessions.extend(remote_mappings);
        self.agents_workspace_project_id = new_project_id;
        let restored_terminal_runtime = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| {
                self.parked_agents_terminal_runtimes_by_project
                    .remove(project_id)
            })
            .unwrap_or_default();
        for session in &mut self.agents_workspace.terminal_sessions {
            session.zmx_session_name = restored_terminal_runtime
                .zmx_session_names
                .get(&session.id)
                .cloned();
        }
        self.agents_terminal_runtime_sessions = restored_terminal_runtime.runtime_sessions;
        self.agents_gpui_engine_terminals = restored_terminal_runtime.gpui_engine_terminals;
        self.agents_gpui_terminal_viewer_recipes = restored_terminal_runtime.viewer_recipes;
        self.agents_terminal_runtime_osc_states = restored_terminal_runtime.runtime_osc_states;
        self.agents_gpui_engine_close_confirms =
            restored_terminal_runtime.gpui_engine_close_confirms;
        // The incoming project's chat views come back with the workspace model
        // whose session ids they are keyed by, so `ensure_native_chat` finds
        // them and the reconcile pass below only has to make them visible.
        let restored_chat_runtime = self
            .agents_workspace_project_id
            .as_ref()
            .and_then(|project_id| {
                self.parked_agents_chat_runtimes_by_project
                    .remove(project_id)
            })
            .unwrap_or_default();
        self.restore_parked_agents_chat_surfaces(restored_chat_runtime);
        // A project first visited since the restart has no parked chat runtime; its saved
        // record keeps the views picked before the restart (`agents_chat_view_observations_from_shell_state`).
        for (session_id, interface) in chat_view_observations {
            self.agents_chat_auto_switch_observed_sessions
                .entry(session_id)
                .or_insert(interface);
        }

        /*
        Shell, pane, and runtime ids are intentionally project-local. Tear down
        the outgoing process-local attachment graph as one ownership unit before
        the incoming model can reuse numeric ids. This drops only local attach
        clients and views; daemon zmx sessions remain alive and reattach through
        the normal sidebar focus path.
        */
        self.local_workspace_attach_pending.clear();
        self.local_workspace_lifecycle_requests.clear();
        self.local_workspace_latest_focus_key = None;
        self.agents_chat_mode_sessions = chat_mode_sessions;
        self.terminal_agent_bar_sessions.clear();
        self.agents_terminal_startup_coordinator = AgentsTerminalStartupCoordinator::new();
        self.agents_terminal_startup_launch_payload_source =
            AgentsTerminalStartupLaunchPayloadSource::new_empty();
        self.agents_terminal_launch_payload_source = AgentsTerminalLaunchPayloadSource::new_empty();
        self.agents_terminal_startup_body_slot_geometries.clear();
        self.agents_terminal_mount_slot_bounds.clear();
        self.pending_terminal_paste_confirmation = None;
        self.terminal_paste_confirmation_dialog_open = false;
        self.agents_delayed_send_timers.clear();
        self.agents_send_when_stopped_watchers.clear();
        self.terminal_search_inputs.clear();
        self.terminal_search_input_subscriptions.clear();
        self.terminal_search_focus_pending = None;
        self.workspace_tab_scroll_handles.clear();
        self.workspace_leaf_layout_bounds.clear();
        self.workspace_split_layout_metrics.clear();
        self.workspace_split_drag = None;
        self.workspace_split_hovering = None;
        self.workspace_split_hover_visible = None;
        self.workspace_drop_feedback = None;
        self.workspace_tab_drag_active = false;

        if matches!(self.shell_focus, ShellFocusTarget::AgentsPane(_)) {
            self.focus_shell_target(
                ShellFocusTarget::AgentsPane(self.agents_workspace.focused_pane),
                cx,
            );
        }
        if matches!(
            self.previous_non_command_focus,
            Some(ShellFocusTarget::AgentsPane(_))
        ) {
            self.previous_non_command_focus = Some(ShellFocusTarget::AgentsPane(
                self.agents_workspace.focused_pane,
            ));
        }
        self.restore_gpui_agents_delayed_sends(delayed_send_restore_intents, cx);
        self.apply_project_view_state_for_active_project(cx);
        self.reconcile_agents_chat_surfaces(cx);
        self.ensure_project_keep_alive_expiry_scheduled(cx);
        self.persist_shell_layout_state();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        cx.notify();
        true
    }

    pub(crate) fn swap_command_pane_for_active_project(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:CommandPane 2026-07-10:
        macOS parity: command panels are per-project (`NativeProject.commandsPanel`
        in native-sidebar.tsx), so an active-project change swaps the whole
        command pane instead of sharing one global panel. The outgoing pane is
        parked as shell-state JSON keyed by its gxserver project id; parking
        must not kill daemon zmx sessions (only explicit close does), so the
        gxserver mappings are rebuilt from the incoming pane instead of being
        forgotten one-by-one. A legacy pane restored without a project id
        splits its tabs into their owning projects once. The project epoch
        invalidates in-flight attach completions from before the swap.
        */
        let new_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        if self.command_pane_project_id == new_project_id {
            return;
        }
        self.command_pane_project_epoch += 1;

        let live_pane_json = command_pane_model_to_shell_state_json_with_delayed_send_timers(
            &self.command_pane,
            &self.command_delayed_send_timers,
            SystemTime::now(),
        );
        match self.command_pane_project_id.take() {
            Some(old_project_id) => {
                self.parked_command_panes_by_project
                    .insert(old_project_id, live_pane_json);
            }
            None => {
                for (project_id, pane_json) in
                    split_command_pane_shell_state_json_by_gxserver_project(
                        &live_pane_json,
                        new_project_id.as_deref(),
                    )
                {
                    self.parked_command_panes_by_project
                        .insert(project_id, pane_json);
                }
            }
        }

        let content_height = command_pane_content_height(window);
        let command_default_height_px = command_pane_default_height_px_from_shared_settings(
            &shared_settings::shared_sidebar_settings_snapshot(),
        );
        let restored_pane = new_project_id.as_ref().and_then(|project_id| {
            self.parked_command_panes_by_project
                .remove(project_id)
                .and_then(|pane_json| {
                    command_pane_model_from_shell_state_with_default_height_px(
                        &pane_json,
                        content_height,
                        command_default_height_px,
                    )
                })
        });
        self.command_pane = restored_pane.unwrap_or_else(|| {
            CommandPaneModel::shell_default_with_default_height_px(
                content_height,
                command_default_height_px,
            )
        });
        self.command_pane_project_id = new_project_id;
        // The incoming pane keeps the mode saved in its own model (CDXC:Workarea 2026-09-21 in
        // model/view_pane_layouts.rs); only the outgoing pane's countdown and hover chrome go.
        self.command_pane_auto_minimize.idle_since = None;
        self.clear_command_resize_hover_state();

        /*
        CDXC:CommandPane 2026-07-10:
        Command session ids are per-pane counters, so ids can collide across
        projects. Every piece of live command runtime state belongs to the
        outgoing pane and is torn down wholesale here instead of pruned by
        session-id membership; dropping engine records and Ghostty owners
        kills only local attach shells and surfaces, never the daemon zmx
        sessions, and the incoming pane remounts through the normal attach
        flow. Delayed Send / Close After Done runtime timers clear like the
        command Sleep contract; their restart checkpoints and armed booleans
        live in the parked shell-state JSON.
        */
        self.command_gxserver_session_mappings =
            command_gxserver_session_mappings_from_command_model(&self.command_pane);
        /*
        CDXC:RemoteMachines 2026-08-29:
        Remote Action tabs park with their project exactly like local command
        tabs: the incoming pane's own remote identities replace the outgoing
        pane's, and the parked project's remote sessions stay alive on their
        machine until that tab is really closed. The askpass helpers belong to
        the torn-down local ssh clients, so they are dropped with them.
        */
        self.command_remote_action_sessions =
            command_remote_action_sessions_from_command_model(&self.command_pane);
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        self.command_remote_attach_askpass_scripts.clear();
        self.command_gxserver_attach_pending.clear();
        self.command_terminal_launch_payload_source
            .remove_all_payloads();
        self.command_gpui_engine_terminals.clear();
        self.command_gpui_terminal_viewer_recipes.clear();
        self.command_gpui_engine_close_confirms.clear();
        self.command_delayed_send_timers.clear();
        self.command_close_after_done_timers.clear();
        self.clear_command_resize_hover_state_if_command_pane_hidden();
        if self.shell_focus == ShellFocusTarget::CommandPane && !self.command_pane.has_sessions() {
            self.restore_previous_non_command_focus_or_default(cx);
        }
        self.seed_terminal_view_for_open(cx);
        self.scroll_focused_command_active_tab();
        self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        self.persist_shell_layout_state();
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        cx.notify();
    }
}
