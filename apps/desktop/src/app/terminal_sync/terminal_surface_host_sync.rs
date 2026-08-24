// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: layout-bounds bookkeeping for terminal mount slots plus command/project-editor-companion terminal surface host sync.

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::atomic::Ordering;

use gpui::Bounds;
use gpui::Pixels;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn record_workspace_leaf_layout_bounds(
        &mut self,
        pane_id: WorkspacePaneId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.workspace_leaf_layout_bounds.insert(pane_id, bounds);
        }
    }

    pub(crate) fn record_agents_terminal_mount_slot_bounds(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUILibghosttyMountBounds 2026-06-22-22:45:
        Store exact terminal body bounds only after validating the pane/session is still one of the current rendered running mount slots. This record is runtime-only and must not be serialized, logged, or represented by a hidden hitbox; the existing body div remains the click/drop owner.
        */
        let current_slot_ids = self.agents_workspace.rendered_terminal_body_mount_slots();
        let slot_is_current = self
            .agents_workspace
            .is_current_terminal_body_mount_slot(slot_id);
        // Surfaced/resize detection reads the render-persistent refresh map,
        // not the per-render-cleared geometry map above — the geometry map is
        // always empty at first record of a frame, which made every frame
        // look freshly surfaced (one zmx subprocess per slot per frame) and
        // left the resize-debounce arm unreachable
        // (CDXC:GPUIZmxPersistenceRefresh 2026-07-11).
        let previous_bounds = self
            .agents_terminal_zmx_refresh_recorded_bounds
            .get(&slot_id)
            .copied();
        self.agents_terminal_mount_slot_bounds
            .retain(|stored_slot_id, _| current_slot_ids.contains(stored_slot_id));
        if slot_is_current {
            self.agents_terminal_mount_slot_bounds
                .insert(slot_id, bounds);
            self.agents_terminal_zmx_refresh_recorded_bounds
                .insert(slot_id, bounds);
        } else {
            self.agents_terminal_mount_slot_bounds.remove(&slot_id);
            self.agents_terminal_zmx_refresh_recorded_bounds
                .remove(&slot_id);
        }
        self.sync_agents_terminal_surface_host(scale_factor, cx);
        /*
        CDXC:GPUIZmxPersistenceRefresh 2026-07-06:
        A slot recording bounds for the first time was just surfaced (tab
        switch, mode switch, wake, restore) and may face a zmx daemon grid
        another client resized while it was hidden — refresh it conditionally
        after host sync so a reattached surface reports its real size. A
        same-slot body size change instead re-arms the trailing-edge resize
        debounce, mirroring macOS surfaced-pane and resize-settled refreshes.
        */
        if self.active_mode == TitlebarMode::Agents && slot_is_current {
            match previous_bounds {
                None => self.refresh_zmx_persistence_agents_terminal_if_stale(slot_id, cx),
                Some(previous) if previous.size != bounds.size => {
                    self.schedule_zmx_persistence_refresh_after_resize(cx);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn record_agents_terminal_startup_body_slot_bounds(
        &mut self,
        slot_id: AgentsTerminalStartupBodySlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
        Record Mounting startup body geometry only for the current visible selected Mounting slot and keep it keyed by `AgentsTerminalStartupBodySlotId`, not the Running mount slot. The existing placeholder body remains the click/drop owner, and this runtime map is cleared or pruned before it can become stale, persisted, logged, or used to create a Ghostty surface.
        */
        record_agents_terminal_startup_body_slot_geometry(
            self.active_mode == TitlebarMode::Agents,
            &self.agents_workspace,
            &mut self.agents_terminal_startup_body_slot_geometries,
            slot_id,
            bounds,
            scale_factor,
        );
        self.sync_agents_terminal_surface_host(scale_factor, cx);
    }

    pub(crate) fn record_agents_terminal_parked_owner_body_slot_bounds(
        &mut self,
        slot_id: AgentsTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Non-startup Mounting wake/reattach bodies may record only runtime geometry for exact parked-owner transfer. The record is accepted from the normal placeholder body, stays out of shell-state JSON/logs, does not create startup candidates or launch payloads, and cannot mark Running without an exact parked owner move.
        */
        record_agents_terminal_parked_owner_body_slot_geometry(
            self.active_mode == TitlebarMode::Agents,
            &self.agents_workspace,
            &mut self.agents_terminal_parked_owner_body_slot_geometries,
            slot_id,
            bounds,
            scale_factor,
        );
        self.sync_agents_terminal_surface_host(scale_factor, cx);
    }

    pub(crate) fn record_command_terminal_mount_slot_bounds(
        &mut self,
        slot_id: CommandTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
        Command terminal bounds are recorded from the command body element itself, never from the enclosing group or titlebar. The record is accepted only for a current expanded visible active command session and stays runtime-only so shell-state JSON, command titles, paths, commands, env, output, and terminal content cannot leak into launch payload or persistence.
        */
        let current_slot_ids = self.command_pane.rendered_terminal_body_mount_slots();
        let slot_is_current = self
            .command_pane
            .is_current_terminal_body_mount_slot(slot_id);
        // Render-persistent refresh map, not the per-render-cleared geometry
        // map — see the Agents bounds hook
        // (CDXC:GPUIZmxPersistenceRefresh 2026-07-11).
        let previous_bounds = self
            .command_terminal_zmx_refresh_recorded_bounds
            .get(&slot_id)
            .copied();
        self.command_terminal_mount_slot_bounds
            .retain(|stored_slot_id, _| current_slot_ids.contains(stored_slot_id));
        if slot_is_current {
            self.command_terminal_mount_slot_bounds
                .insert(slot_id, bounds);
            self.command_terminal_zmx_refresh_recorded_bounds
                .insert(slot_id, bounds);
        } else {
            self.command_terminal_mount_slot_bounds.remove(&slot_id);
            self.command_terminal_zmx_refresh_recorded_bounds
                .remove(&slot_id);
        }
        self.sync_command_terminal_surface_host(scale_factor, cx);
        // See the Agents bounds hook: first-record = surfaced refresh, body
        // size change = trailing-edge resize debounce (CDXC:GPUIZmxPersistenceRefresh).
        if slot_is_current {
            match previous_bounds {
                None => self.refresh_zmx_persistence_command_terminal_if_stale(slot_id, cx),
                Some(previous) if previous.size != bounds.size => {
                    self.schedule_zmx_persistence_refresh_after_resize(cx);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn record_project_editor_companion_terminal_mount_slot_bounds(
        &mut self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        bounds: Bounds<Pixels>,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        let current_slot_ids = self.current_project_editor_companion_terminal_body_mount_slots();
        let slot_is_current =
            self.is_current_project_editor_companion_terminal_body_mount_slot(slot_id);
        // Render-persistent refresh map, not the per-render-cleared geometry
        // map — see the Agents bounds hook
        // (CDXC:GPUIZmxPersistenceRefresh 2026-07-11).
        let previous_bounds = self
            .project_editor_companion_zmx_refresh_recorded_bounds
            .get(&slot_id)
            .copied();
        self.project_editor_companion_terminal_mount_slot_bounds
            .retain(|stored_slot_id, _| current_slot_ids.contains(stored_slot_id));
        if slot_is_current {
            self.project_editor_companion_terminal_mount_slot_bounds
                .insert(slot_id, bounds);
            self.project_editor_companion_zmx_refresh_recorded_bounds
                .insert(slot_id, bounds);
        } else {
            self.project_editor_companion_terminal_mount_slot_bounds
                .remove(&slot_id);
            self.project_editor_companion_zmx_refresh_recorded_bounds
                .remove(&slot_id);
        }
        self.sync_project_editor_companion_terminal_surface_host(scale_factor, cx);
        // See the Agents bounds hook: first-record = surfaced refresh, body
        // size change = trailing-edge resize debounce (CDXC:GPUIZmxPersistenceRefresh).
        if slot_is_current {
            match previous_bounds {
                None => {
                    self.refresh_zmx_persistence_companion_terminal_if_stale(slot_id.mode, cx);
                }
                Some(previous) if previous.size != bounds.size => {
                    self.schedule_zmx_persistence_refresh_after_resize(cx);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn sync_command_terminal_surface_host(
        &mut self,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        if GPUI_APP_QUIT_IN_PROGRESS.load(Ordering::Acquire) {
            return;
        }
        /*
        CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
        Command terminal host sync is a command-pane-only runtime pipeline. It reconciles only expanded visible active command body slots plus exact body bounds, creates App-owned NSView/Ghostty surfaces through the shared terminal host stack with a source-only launch request boundary, and never touches Agents workspace maps, Agents runtime registries, startup launch payloads, shell-state JSON, logs, overlays, or hidden hit regions.

        CDXC:GPUICommandTerminalSurface 2026-06-23-05:18:
        Command terminal native sync must be driven from render-start and body-canvas bounds records because Ghostty content scale and pixel size require the current window scale factor. Command model mutations should schedule render with `cx.notify()` and avoid eager sync calls with placeholder scale values or previous-frame body bounds.

        CDXC:GPUICommandTerminalGhosttyClose 2026-06-23-05:21:
        Confirmed command Ghostty close callbacks are consumed before command host reconciliation computes current slots. Shell removal goes through the command model first, then the existing render/bounds-driven host cleanup drops stale command Ghostty surfaces and AppKit hosts without eager sync calls, Agents map mutation, startup map mutation, shell-state runtime fields, logs, overlays, or hit-test routing.

        CDXC:GPUICommandTerminalProcessExit 2026-06-23-05:30:
        Command process-exited polling runs after confirmed-close callback consumption and before command host reconciliation. The command model removes exited sessions first, and stale command hosts/surfaces then detach through the existing render/bounds-driven cleanup path without requesting Ghostty close or forcing an eager placeholder-scale sync.

        CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
        Command close-confirm sync is separate from Agents runtime/startup sync. Confirmation-needed callbacks create only command pending state before confirmed-close consumption, and stale command pending entries prune through command model/surface ownership without mutating Agents maps.

        CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-23-16:46:
        Command config preparation may attach launch data only from `CommandTerminalLaunchPayloadSource` for the exact current command body mount slot and derived command runtime id. Invalid explicit payloads suppress/prune the request instead of falling back to command titles, shell status, delayed-send state, project/workspace data, paths, cwd, command args, env, initial input, wait policy, stdout/stderr, terminal content, or helper detection.

        CDXC:GPUICommandTerminalLaunchPayloadSource 2026-06-27-04:47:
        Config preparation owns the one-shot drain point for Action and plain command startup payloads. Consume the exact slot payload while building the Ghostty request so native-view remounts can recreate surfaces without replaying old command text or cwd payloads.

        CDXC:GPUICommandPaneResize 2026-06-27-03:21:
        Mounted command runtime close confirmation and process-exit cleanup can remove the final command tab without using direct tab-close handlers. After those model mutations, clear runtime resize hover chrome only if the command pane is now empty so stale panel cursors cannot survive hidden/collapsed command-panel state.

        CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
        Command Sleep/Wake preserves runtime ownership by parking exact command host/surface owners before a sleep-driven detach and moving them back before the wake mount can create a replacement. Collapse, close, stale slots, invalid payloads, and mismatches keep the normal detach/drop path.
        */
        #[cfg(target_os = "macos")]
        {
            self.command_terminal_close_confirms
                .sync_from_confirmation_needed_callbacks(
                    &self.command_pane,
                    &self.command_terminal_ghostty_surfaces,
                );
            let mut shell_state_changed = consume_confirmed_command_terminal_ghostty_surface_closes(
                &mut self.command_pane,
                &mut self.command_terminal_close_confirms,
                &self.command_terminal_ghostty_surfaces,
            );
            let exit_cleanup = consume_exited_command_terminal_ghostty_surfaces(
                &mut self.command_pane,
                &self.command_terminal_ghostty_surfaces,
            );
            if exit_cleanup.changed {
                shell_state_changed = true;
                self.dispatch_gpui_command_action_completions(exit_cleanup.completions, cx);
                self.command_terminal_close_confirms
                    .prune_stale(&self.command_pane, &self.command_terminal_ghostty_surfaces);
            }

            if shell_state_changed {
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
            }
        }
        self.sync_command_gpui_engine_terminals(cx);
        let gpui_engine_enabled = shared_settings::shared_sidebar_settings_snapshot()
            .gpui_terminal_engine_settings()
            .enabled;
        let current_slot_ids = if gpui_engine_enabled {
            Vec::new()
        } else {
            self.command_pane
                .rendered_terminal_body_mount_slots()
                .into_iter()
                .filter(|slot_id| {
                    !self
                        .command_gpui_engine_terminals
                        .contains_key(&slot_id.session_id)
                })
                .collect::<Vec<_>>()
        };
        self.command_terminal_mount_slot_bounds
            .retain(|slot_id, _| current_slot_ids.contains(slot_id));
        /*
        CDXC:GPUIWorkspaceTabDragVisibility 2026-07-03:
        Workspace/Agents tab drags can drop into expanded command groups, so mounted command terminals hide-and-park for the whole drag exactly like a command-panel collapse. The reattach pass must pause with the same gate; otherwise the body canvas keeps recording bounds during the drag and parked command owners would reattach and detach every frame.
        */
        let command_terminal_native_views_may_be_visible =
            self.command_pane.is_expanded() && !self.workspace_tab_drag_active;
        let commands = self
            .command_terminal_surface_host
            .sync_visible_command_slots(
                command_terminal_native_views_may_be_visible,
                &current_slot_ids,
                &self.command_terminal_mount_slot_bounds,
            );
        let decisions = self
            .command_terminal_surface_lifecycle
            .reconcile_host_commands(&commands);
        #[cfg(target_os = "macos")]
        {
            self.drop_command_terminal_ghostty_surface_before_host_detach(&commands);
            if command_terminal_native_views_may_be_visible {
                let reattach_plans = command_terminal_parked_owner_reattach_plans(
                    &self.command_pane,
                    &self.command_terminal_parked_runtime_owners,
                    &self.command_terminal_mount_slot_bounds,
                );
                for reattach_plan in reattach_plans {
                    transfer_command_terminal_parked_runtime_owner_reattach(
                        &self.command_pane,
                        &mut self.command_terminal_parked_runtime_owners,
                        &mut self.command_terminal_mount_slot_bounds,
                        &mut self.command_terminal_host_native_views,
                        &mut self.command_terminal_ghostty_surfaces,
                        &self.command_terminal_ghostty_surface_config_requests,
                        reattach_plan,
                    );
                }
            }
            prune_command_terminal_parked_runtime_owners(
                &self.command_pane,
                &mut self.command_terminal_parked_runtime_owners,
                &self.command_terminal_host_native_views,
                &self.command_terminal_ghostty_surfaces,
            );
            let frame_operations =
                terminal_native_view::reconcile_app_owned_terminal_host_native_view(
                    &mut self.command_terminal_host_native_views,
                    &mut self.command_terminal_surface_lifecycle,
                    self.parent_ns_view,
                    &commands,
                    &decisions,
                    terminal_native_view::TerminalHostNativeViewFactory::create,
                );
            terminal_native_view::execute_app_owned_terminal_host_frame_operations(
                &self.command_terminal_host_native_views,
                &frame_operations,
            );
            let terminal_config = current_gpui_terminal_ghostty_surface_config();
            let mut invalid_launch_payload_slot_ids = HashSet::new();
            let mut config_requests = HashMap::new();
            for (slot_id, host_view) in self.command_terminal_host_native_views.iter() {
                let Some(request) =
                    terminal_native_view::ghostty_surface_config_request_for_app_owned_terminal_host_native_view(
                        Some(host_view),
                        f64::from(scale_factor),
                    )
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                let request = request.with_terminal_config(terminal_config);
                match command_terminal_config_request_with_launch_payload_source(
                    *slot_id,
                    request,
                    &mut self.command_terminal_launch_payload_source,
                ) {
                    Ok(request) => {
                        config_requests.insert(*slot_id, request);
                    }
                    Err(_) => {
                        invalid_launch_payload_slot_ids.insert(*slot_id);
                    }
                }
            }
            self.command_terminal_ghostty_surface_config_requests = config_requests;
            for slot_id in invalid_launch_payload_slot_ids.iter().copied() {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.command_terminal_host_native_views.get(&slot_id),
                    false,
                );
            }
            self.command_terminal_host_native_views
                .retain(|slot_id, _| !invalid_launch_payload_slot_ids.contains(slot_id));
            self.sync_command_terminal_ghostty_surfaces(cx);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (decisions, scale_factor);
    }

    pub(crate) fn sync_project_editor_companion_terminal_surface_host(
        &mut self,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        self.sync_project_editor_companion_terminal_selection();
        self.prune_project_editor_companion_remote_attach_states();
        let logical_slot_ids = self.current_project_editor_companion_terminal_body_mount_slots();
        let settings =
            shared_settings::shared_sidebar_settings_snapshot().gpui_terminal_engine_settings();
        if settings.enabled {
            for slot_id in logical_slot_ids.iter().copied() {
                if self
                    .agents_gpui_engine_terminals
                    .contains_key(&slot_id.session_id)
                {
                    if let Some(runtime_session_id) = self
                        .agents_terminal_runtime_sessions
                        .runtime_session_id_for_shell_session(slot_id.session_id)
                    {
                        let _ = self
                            .project_editor_companion_terminal_launch_payload_source
                            .take_explicit_payload_for_mount_slot(runtime_session_id, slot_id);
                    }
                    continue;
                }
                let Some(runtime_session_id) = self
                    .agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(slot_id.session_id)
                else {
                    continue;
                };
                let Some(payload) = self
                    .project_editor_companion_terminal_launch_payload_source
                    .take_explicit_payload_for_mount_slot(runtime_session_id, slot_id)
                else {
                    self.request_project_editor_companion_terminal_attach_payload(slot_id, cx);
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
                }
            }
        }
        let current_slot_ids = if settings.enabled {
            Vec::new()
        } else {
            logical_slot_ids
                .iter()
                .copied()
                .filter(|slot_id| {
                    !self
                        .agents_gpui_engine_terminals
                        .contains_key(&slot_id.session_id)
                        && self
                            .project_editor_companion_remote_attach_unavailable_message(*slot_id)
                            .is_none()
                })
                .collect::<Vec<_>>()
        };
        self.project_editor_companion_terminal_mount_slot_bounds
            .retain(|slot_id, _| current_slot_ids.contains(slot_id));
        self.project_editor_companion_terminal_launch_payload_source
            .retain_current_mount_slots(&logical_slot_ids, &self.agents_terminal_runtime_sessions);
        self.project_editor_companion_terminal_attach_plan_pending
            .retain(|slot_id| logical_slot_ids.contains(slot_id));
        let companion_native_views_may_be_visible = self
            .project_editor_companion_terminal_slot_for_mode(self.active_mode)
            .is_some();
        let commands = self
            .project_editor_companion_terminal_surface_host
            .sync_visible_slots(
                companion_native_views_may_be_visible,
                &current_slot_ids,
                &self.project_editor_companion_terminal_mount_slot_bounds,
            );
        let decisions = self
            .project_editor_companion_terminal_surface_lifecycle
            .reconcile_host_commands(&commands);
        #[cfg(target_os = "macos")]
        {
            for command in &commands {
                if let terminal_surface_host::NativeTerminalSurfaceHostCommand::HideAndDetach {
                    plan,
                } = *command
                {
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .remove(&plan.slot_id);
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.project_editor_companion_terminal_host_native_views
                            .get(&plan.slot_id),
                        false,
                    );
                }
            }
            let frame_operations =
                terminal_native_view::reconcile_app_owned_terminal_host_native_view(
                    &mut self.project_editor_companion_terminal_host_native_views,
                    &mut self.project_editor_companion_terminal_surface_lifecycle,
                    self.parent_ns_view,
                    &commands,
                    &decisions,
                    terminal_native_view::TerminalHostNativeViewFactory::create,
                );
            terminal_native_view::execute_app_owned_terminal_host_frame_operations(
                &self.project_editor_companion_terminal_host_native_views,
                &frame_operations,
            );
            let terminal_config = current_gpui_terminal_ghostty_surface_config();
            let mut config_requests = HashMap::new();
            let mut attach_payload_needed_slot_ids = Vec::new();
            for (slot_id, host_view) in self
                .project_editor_companion_terminal_host_native_views
                .iter()
            {
                let Some(request) =
                    terminal_native_view::ghostty_surface_config_request_for_app_owned_terminal_host_native_view(
                        Some(host_view),
                        f64::from(scale_factor),
                    )
                    .ok()
                    .flatten()
                else {
                    continue;
                };
                let Some(runtime_session_id) = self
                    .agents_terminal_runtime_sessions
                    .runtime_session_id_for_shell_session(slot_id.session_id)
                else {
                    continue;
                };
                let request = request.with_terminal_config(terminal_config);
                if self
                    .project_editor_companion_terminal_ghostty_surfaces
                    .contains_key(slot_id)
                {
                    config_requests.insert(*slot_id, request);
                    continue;
                }
                /*
                CDXC:GPUIProjectEditorCompanionAttach 2026-07-06:
                A companion slot without a live surface may only mount with the
                daemon-built zmx attach payload for its session; otherwise the
                slot stays unmounted while the attach plan is fetched, instead
                of spawning a default shell that is not the user's session.
                */
                match self
                    .project_editor_companion_terminal_launch_payload_source
                    .take_payload_for_mount_slot(runtime_session_id, *slot_id)
                {
                    Ok(Some(launch_payload)) => {
                        config_requests
                            .insert(*slot_id, request.with_launch_payload(launch_payload));
                    }
                    Ok(None) => attach_payload_needed_slot_ids.push(*slot_id),
                    Err(_) => {}
                }
            }
            self.project_editor_companion_terminal_ghostty_surface_config_requests =
                config_requests;
            for slot_id in attach_payload_needed_slot_ids {
                self.request_project_editor_companion_terminal_attach_payload(slot_id, cx);
            }
            self.sync_project_editor_companion_terminal_ghostty_surfaces(cx);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (decisions, scale_factor, cx);
    }
}
