// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: terminal surface/engine host synchronisation and mount-slot bookkeeping

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;
use gpui::Focusable as _;
use gpui::AnyElement;
use gpui::App;
use gpui::AppContext as _;
use gpui::Bounds;
use gpui::ClipboardItem;
use gpui::Entity;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::input::InputEvent;
use gpui_component::input::InputState;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::element::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
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

    /// Routes Stash Prompt to the input surface the user can currently see.
    /// Chat owns its React draft; terminal mode owns the agent CLI composer.
    pub(crate) fn request_gpui_stash_prompt_for_active_input(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            self.request_session_chat_stash_prompt(shell_session_id, cx);
        } else {
            self.request_gpui_stash_current_prompt(shell_session_id, cx);
        }
    }

    /// Terminal-mode stash reuses the Ctrl+G contract headlessly: write a
    /// one-shot marker, then send BEL. The agent CLI writes its composer to
    /// the editor file; `ghostex prompt-editor` saves it and clears the input.
    pub(crate) fn request_gpui_stash_current_prompt(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.local_workspace_key_for_shell_session(shell_session_id) else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Stash Prompt unavailable",
                "This terminal is not attached to a gxserver session.",
                cx,
            );
            return;
        };
        if !gpui_write_prompt_stash_request_marker(&key.project_id, &key.session_id, "1\n") {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Stash Prompt failed",
                "Could not write the stash request marker.",
                cx,
            );
            return;
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.send_text_input("\u{7}", cx));
            return;
        }
        #[cfg(target_os = "macos")]
        if self.send_text_bytes_to_focused_agents_terminal_surface(b"\x07") {
            return;
        }
        let _ = gpui_remove_prompt_stash_request_marker(&key.project_id, &key.session_id);
    }

    pub(crate) fn request_terminal_handoff_to_session_chat(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.agents_session_chat_eligible(shell_session_id) {
            return;
        }
        if !self.show_agents_session_chat_mode(shell_session_id, cx) {
            return;
        }
        /*
        CDXC:SessionChatViewSwitch 2026-08-21:
        Show Chat before asking the daemon to copy the terminal draft. Agent
        startup prompts, permission prompts, shell state, and older CLIs may
        not answer Ctrl+G; none of those terminal states may veto the view
        switch. The daemon handshake is already loss-safe: success clears and
        delivers the captured draft, while failure leaves it in the parked
        terminal.
        */
        if self.agents_session_chat_transcript_agent(shell_session_id) == Some("grok") {
            self.dispatch_gpui_app_modal_toast(
                "info",
                "Note: Prompt Text is left in the CLI for Grok Build (limitation).",
                "Please copy your prompt manually to the chat view.",
                cx,
            );
            return;
        }
        self.request_session_chat_draft_transfer(shell_session_id, cx);
    }

    /// The terminal "Prompts" overlay action opens the stashed-prompts recall
    /// modal scoped to the focused mapped gxserver session, so its default
    /// view is "this project and its worktrees" and a selected prompt can be
    /// inserted back into this exact terminal. Unmapped local placeholder tabs
    /// still open the modal in all-projects browse mode.
    pub(crate) fn open_gpui_stashed_prompts_modal_for_focused_agents_session(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(shell_session_id) = self.focused_agents_or_companion_shell_session_id() else {
            return false;
        };
        let key = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()));
        let modal = GpuiAppModalKind::StashedPrompts;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "modal": modal.modal_id(),
            "type": "open",
        });
        if let Some(key) = key {
            open_message["projectId"] = serde_json::Value::String(key.project_id.clone());
            open_message["sessionId"] = serde_json::Value::String(
                gpui_combined_presentation_session_id(&key.project_id, &key.session_id),
            );
        }
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
        true
    }

    pub(crate) fn handle_gpui_engine_prompt_editor_shortcut(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_context = match target {
            GpuiEngineTerminalEventTarget::Agents(shell_session_id) => self
                .remote_prompt_editor_context_for_shell_session(shell_session_id)
                .map(|(key, connection_generation)| (shell_session_id, key, connection_generation)),
            GpuiEngineTerminalEventTarget::Command(_) => None,
        };
        if let Some((shell_session_id, key, connection_generation)) = remote_context {
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        shell_session_id,
                        &key,
                        connection_generation,
                        RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                            target,
                            runtime_session_id,
                        },
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        self.warn_if_monaco_prompt_editor_helper_is_missing(cx);
        let Some(originating_session_id) =
            self.prompt_editor_originating_session_id_for_engine_target(target)
        else {
            self.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                target,
                runtime_session_id,
                cx,
            );
            return;
        };

        cx.spawn(async move |this, cx| {
            let fronted = cx
                .background_executor()
                .spawn(
                    async move { gpui_ghostex_editor_daemon_front(Some(&originating_session_id)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if fronted {
                    if !this.prompt_editor_daemon_open {
                        this.prompt_editor_daemon_open = true;
                        cx.notify();
                    }
                } else {
                    this.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                        target,
                        runtime_session_id,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /*
    The app is the only place that knows both that the user asked for the Monaco
    prompt editor and that no GhostexEditor daemon is installed to serve it. The
    terminal-side CLI only sees the negotiated `editor` capability, so it opens
    the machine editor (vi, for anyone with no $EDITOR) without a word. Say so
    where the user is looking instead of degrading silently.
    */
    pub(crate) fn warn_if_monaco_prompt_editor_helper_is_missing(&mut self, cx: &mut gpui::Context<Self>) {
        if !gpui_prompt_editor_backend_setting_is_monaco()
            || gpui_resolved_ghostex_editor_executable().is_some()
        {
            return;
        }
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| {
                this.upsert_gpui_app_toast(
                    GpuiAppToast {
                        id: GPUI_MISSING_MONACO_PROMPT_EDITOR_TOAST_ID.to_string(),
                        level: GpuiAppToastLevel::Warning,
                        title: "Monaco prompt editor unavailable".to_string(),
                        description: Some(
                            "The Ghostex Editor helper is missing from this build, so Ctrl+G opens the machine editor instead. Set GHOSTEX_EDITOR_APP or reinstall Ghostex."
                                .to_string(),
                        ),
                        loading: false,
                        persistent: false,
                        duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                        epoch: 0,
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn remote_prompt_editor_context_for_shell_session(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> Option<(GpuiRemoteAttachSessionKey, u64)> {
        let GpuiWorkspaceTerminalSessionKey::Remote(key) =
            self.workspace_terminal_key_for_shell_session(shell_session_id)?
        else {
            return None;
        };
        let connection_generation = self
            .remote_gxserver_connect_generations
            .get(key.remote_machine_id.as_str())
            .copied()?;
        Some((key, connection_generation))
    }

    pub(crate) fn queue_remote_prompt_editor_request(
        &mut self,
        shell_session_id: TerminalSessionId,
        key: &GpuiRemoteAttachSessionKey,
        connection_generation: u64,
        delivery_target: RemotePromptEditorDeliveryTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let context_is_current = self
            .remote_prompt_editor_context_for_shell_session(shell_session_id)
            .is_some_and(|(current_key, current_generation)| {
                current_key == *key && current_generation == connection_generation
            });
        let scoped_project_id =
            gpui_remote_scoped_project_id(key.remote_machine_id.as_str(), key.project_id.as_str());
        let source_target = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| {
                snapshot.active_project_id.as_ref().map(|id| id.0.as_str())
                    == Some(scoped_project_id.as_str())
            })
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot));
        let source_target_is_current = source_target.as_ref().is_some_and(|target| {
            matches!(
                &target.endpoint,
                SourceCodeServerRuntimeEndpoint::Remote {
                    connection_generation: target_generation,
                    remote_machine_id,
                    ..
                } if remote_machine_id == &key.remote_machine_id
                    && *target_generation == connection_generation
            )
        });
        if !context_is_current || !source_target_is_current {
            self.report_remote_prompt_editor_failure(
                "The remote project or session changed before its editor could open.",
                cx,
            );
            return false;
        }
        if !self.set_active_mode(TitlebarMode::Source, window, cx) {
            self.report_remote_prompt_editor_failure(
                "Code view is unavailable for this remote project.",
                cx,
            );
            return false;
        }
        self.focus_project_editor_surface(TitlebarMode::Source, window, cx);
        let Some(source_target) = source_target else {
            return false;
        };
        let runtime_is_owned_for_request = matches!(
            self.source_code_server_runtime.state,
            SourceCodeServerRuntimeLaunchState::Launching
                | SourceCodeServerRuntimeLaunchState::Ready
        ) && self.source_code_server_runtime.target.as_ref()
            == Some(&source_target);
        if !runtime_is_owned_for_request {
            self.report_remote_prompt_editor_failure(
                "Code view is unavailable for this remote project.",
                cx,
            );
            return false;
        }
        let source_runtime_generation = self.source_code_server_runtime.generation;
        self.source_code_server_runtime
            .queue_remote_prompt_editor_request(PendingRemotePromptEditorRequest {
                shell_session_id,
                remote_key: key.clone(),
                connection_generation,
                source_target,
                source_runtime_generation,
                delivery_target,
            });
        self.deliver_pending_remote_prompt_editor_request_if_ready(cx);
        true
    }

    pub(crate) fn deliver_pending_remote_prompt_editor_request_if_ready(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(request) = self
            .source_code_server_runtime
            .pending_remote_prompt_editor_request
            .clone()
        else {
            return false;
        };
        if !self
            .source_code_server_runtime
            .owns_ready_remote_prompt_editor_ipc(&request)
        {
            let still_waiting_for_owned_runtime = self.source_code_server_runtime.state
                == SourceCodeServerRuntimeLaunchState::Launching
                && self.source_code_server_runtime.generation == request.source_runtime_generation
                && self.source_code_server_runtime.target.as_ref() == Some(&request.source_target);
            if !still_waiting_for_owned_runtime {
                self.source_code_server_runtime
                    .pending_remote_prompt_editor_request = None;
            }
            return false;
        }

        let authoritative_session_is_current = self
            .remote_prompt_editor_context_for_shell_session(request.shell_session_id)
            .is_some_and(|(current_key, current_generation)| {
                current_key == request.remote_key
                    && current_generation == request.connection_generation
            });
        let authoritative_target_is_current = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| self.source_code_server_runtime_target(snapshot))
            .is_some_and(|target| target == request.source_target);
        let delivery_target_is_current = match request.delivery_target {
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::Agents(slot_id),
            ) => slot_id.session_id == request.shell_session_id,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id),
            ) => slot_id.session_id == request.shell_session_id,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(
                FocusedTerminalTextMountTarget::Command(_),
            ) => false,
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target: GpuiEngineTerminalEventTarget::Agents(shell_session_id),
                runtime_session_id,
            } => {
                shell_session_id == request.shell_session_id
                    && self
                        .agents_gpui_engine_terminals
                        .get(&shell_session_id)
                        .is_some_and(|record| record.runtime_session_id == runtime_session_id)
            }
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target: GpuiEngineTerminalEventTarget::Command(_),
                ..
            } => false,
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeView(native_view) => {
                self.agents_terminal_session_id_containing_responder(
                    native_view as *mut std::ffi::c_void,
                )
                .or_else(|| {
                    self.project_editor_companion_terminal_session_id_containing_responder(
                        native_view as *mut std::ffi::c_void,
                    )
                }) == Some(request.shell_session_id)
            }
        };
        if !authoritative_session_is_current
            || !authoritative_target_is_current
            || !delivery_target_is_current
        {
            self.source_code_server_runtime
                .pending_remote_prompt_editor_request = None;
            return false;
        }

        self.source_code_server_runtime
            .pending_remote_prompt_editor_request = None;
        match request.delivery_target {
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeTerminal(target) => {
                self.send_prompt_editor_shortcut_to_native_terminal_target(target)
            }
            RemotePromptEditorDeliveryTarget::GpuiEngineTerminal {
                target,
                runtime_session_id,
            } => {
                self.send_prompt_editor_shortcut_to_gpui_engine_terminal(
                    target,
                    runtime_session_id,
                    cx,
                );
                true
            }
            #[cfg(target_os = "macos")]
            RemotePromptEditorDeliveryTarget::NativeView(native_view) => {
                terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(
                    native_view as *mut std::ffi::c_void,
                )
            }
        }
    }

    pub(crate) fn report_remote_prompt_editor_failure(
        &mut self,
        description: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        self.upsert_gpui_app_toast(
            GpuiAppToast {
                id: "gpui-remote-prompt-editor-failed".to_string(),
                level: GpuiAppToastLevel::from_raw(Some("warning")),
                title: "Remote prompt editor unavailable".to_string(),
                description: Some(description.to_string()),
                loading: false,
                persistent: false,
                duration_ms: GPUI_APP_TOAST_DEFAULT_DURATION_MS,
                epoch: 0,
            },
            cx,
        );
    }

    pub(crate) fn prompt_editor_originating_session_id_for_engine_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<String> {
        let key = match target {
            GpuiEngineTerminalEventTarget::Agents(shell_session_id) => self
                .local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (*mapped_session_id == shell_session_id).then_some(key)
                }),
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                self.command_gxserver_session_mappings.get(&session_id)
            }
        }?;
        Some(format!("{}:{}", key.project_id, key.session_id))
    }

    pub(crate) fn send_prompt_editor_shortcut_to_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let record = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                self.agents_gpui_engine_terminals.get(&session_id)
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                self.command_gpui_engine_terminals.get(&session_id)
            }
        };
        let Some(view) = record
            .filter(|record| record.runtime_session_id == runtime_session_id)
            .map(|record| record.view.clone())
        else {
            return;
        };
        view.update(cx, |view, cx| view.send_text_input("\u{7}", cx));
    }

    pub(crate) fn open_gpui_engine_terminal_action_url(
        &mut self,
        value: &str,
        working_directory: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed.len() > 2048 {
            return;
        }
        let open_value = gpui_terminal_markdown_image_reference_path(trimmed).unwrap_or(trimmed);
        if gpui_terminal_link_is_web_url(open_value) {
            if !shared_settings::shared_sidebar_settings_snapshot().web_links_open_in_app() {
                let _ = gpui_open_terminal_action_url(open_value);
                return;
            }
            let Some(sidebar) = self.sidebar.clone() else {
                return;
            };
            let payload = serde_json::json!({
                "reuse": "similar",
                "type": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE,
                "url": open_value,
                "version": GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION,
            });
            let script = format!(
                "(function(){{const post=window.ghostexGpui?.postOpenBrowserUrl;if(typeof post==='function'){{post(JSON.stringify({payload}));}}}})(); undefined;"
            );
            sidebar.update(cx, |surface, _| {
                surface.execute_app_owned_script(&script);
            });
            return;
        }
        let Some(file_link_path) = gpui_terminal_file_link_path(open_value) else {
            let _ = gpui_open_terminal_action_url(open_value);
            return;
        };
        let file_path = if file_link_path.is_absolute() {
            file_link_path
        } else if let Some(working_directory) = working_directory
            .map(str::trim)
            .filter(|working_directory| !working_directory.is_empty())
        {
            PathBuf::from(working_directory).join(file_link_path)
        } else {
            file_link_path
        };
        let file_path = file_path.to_string_lossy().to_string();
        cx.spawn(async move |this, cx| {
            let _ = this.update_in(cx, |this, window, cx| {
                this.open_session_chat_file(&file_path, window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn gpui_terminal_attachment_target_for_engine_target(
        &self,
        target: GpuiEngineTerminalEventTarget,
    ) -> Option<GpuiTerminalAttachmentTarget> {
        let GpuiEngineTerminalEventTarget::Agents(session_id) = target else {
            return Some(GpuiTerminalAttachmentTarget::Terminal(target));
        };
        let Some(slot_id) = self
            .current_project_editor_companion_terminal_body_mount_slots()
            .into_iter()
            .find(|slot_id| slot_id.session_id == session_id)
        else {
            if self.active_mode.is_project_editor_mode() {
                return None;
            }
            return Some(GpuiTerminalAttachmentTarget::Terminal(target));
        };
        let session_key = self.project_editor_companion_terminal_key_for_slot(slot_id)?;
        Some(GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
            slot_id,
            session_key,
        })
    }

    pub(crate) fn request_gpui_engine_terminal_attachment_paths(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Attach File or Folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.attach_selected_path_to_gpui_engine_terminal(
                    target,
                    runtime_session_id,
                    path,
                    cx,
                );
            });
        })
        .detach();
    }

    pub(crate) fn attach_selected_path_to_gpui_engine_terminal(
        &mut self,
        target: GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        path: PathBuf,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gpui_terminal_attachment_target_matches_runtime(&target, runtime_session_id) {
            return;
        }

        let remote_machine_id = match &target {
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Agents(
                session_id,
            )) => self
                .remote_attach_sessions
                .iter()
                .find_map(|(key, mapped_session_id)| {
                    (mapped_session_id == session_id).then(|| key.remote_machine_id.clone())
                }),
            GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                session_key: GpuiWorkspaceTerminalSessionKey::Remote(remote_key),
                ..
            } => Some(remote_key.remote_machine_id.clone()),
            GpuiTerminalAttachmentTarget::Terminal(GpuiEngineTerminalEventTarget::Command(_))
            | GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                session_key: GpuiWorkspaceTerminalSessionKey::Local(_),
                ..
            } => None,
        };
        let Some(remote_machine_id) = remote_machine_id else {
            match gpui_local_terminal_attachment_reference(path.as_path()) {
                Ok(reference) => {
                    let text = gpui_terminal_attachment_markdown_text(&[reference]);
                    let _ = self.paste_text_into_gpui_engine_terminal_target(
                        target.engine_target(),
                        runtime_session_id,
                        text.as_str(),
                        cx,
                    );
                }
                Err(message) => self.dispatch_gpui_workspace_action_toast(
                    "warning",
                    "Attachment unavailable",
                    message.as_str(),
                    cx,
                ),
            }
            return;
        };

        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let Some(config) =
            gpui_remote_machine_config_from_settings(settings.object(), remote_machine_id.as_str())
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Attachment unavailable",
                "The saved remote machine is missing required SSH settings.",
                cx,
            );
            return;
        };
        let Some(remote_target) = self.gpui_remote_gxserver_request_target(&remote_machine_id)
        else {
            self.dispatch_gpui_workspace_action_toast(
                "warning",
                "Attachment unavailable",
                "Reconnect the remote machine before attaching a file or folder.",
                cx,
            );
            return;
        };

        self.dispatch_gpui_workspace_action_toast(
            "info",
            "Uploading attachment",
            "Uploading the selected item to the remote machine.",
            cx,
        );
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_upload_terminal_attachment_to_remote(
                        &config,
                        &remote_target.execution_target,
                        path.as_path(),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !this
                    .gpui_terminal_attachment_target_matches_runtime(&target, runtime_session_id)
                {
                    return;
                }
                match result {
                    Ok(reference) => {
                        let text = gpui_terminal_attachment_markdown_text(&[reference]);
                        if this.paste_text_into_gpui_engine_terminal_target(
                            target.engine_target(),
                            runtime_session_id,
                            text.as_str(),
                            cx,
                        ) {
                            this.dispatch_gpui_workspace_action_toast(
                                "success",
                                "Attachment uploaded",
                                "The remote attachment reference was pasted into the terminal.",
                                cx,
                            );
                        }
                    }
                    Err(message) => this.dispatch_gpui_workspace_action_toast(
                        "warning",
                        "Attachment upload failed",
                        message.as_str(),
                        cx,
                    ),
                }
            });
        })
        .detach();
    }

    pub(crate) fn gpui_terminal_attachment_target_matches_runtime(
        &self,
        target: &GpuiTerminalAttachmentTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        if !self
            .gpui_engine_terminal_target_matches_runtime(target.engine_target(), runtime_session_id)
        {
            return false;
        }
        match target {
            GpuiTerminalAttachmentTarget::Terminal(_) => true,
            GpuiTerminalAttachmentTarget::ProjectEditorCompanion {
                slot_id,
                session_key,
            } => {
                self.project_editor_companion_terminal_key_for_slot(*slot_id)
                    .as_ref()
                    == Some(session_key)
            }
        }
    }

    pub(crate) fn gpui_engine_terminal_target_matches_runtime(
        &self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> bool {
        match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .is_some_and(|record| record.runtime_session_id == runtime_session_id),
        }
    }

    pub(crate) fn paste_text_into_gpui_engine_terminal_target(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        text: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
        let record = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => {
                self.agents_gpui_engine_terminals.get(&session_id)
            }
            GpuiEngineTerminalEventTarget::Command(session_id) => {
                self.command_gpui_engine_terminals.get(&session_id)
            }
        };
        let Some(view) = record
            .filter(|record| record.runtime_session_id == runtime_session_id)
            .map(|record| record.view.clone())
        else {
            return false;
        };
        view.update(cx, |view, cx| view.paste_text(text, cx));
        true
    }

    pub(crate) fn perform_manage_files_bridge_side_effect(
        &mut self,
        side_effect: ManageFilesBridgeSideEffect,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        match side_effect {
            ManageFilesBridgeSideEffect::CopyFullPath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path));
                Ok(())
            }
            ManageFilesBridgeSideEffect::RevealInFinder(path) => gpui_reveal_path_in_finder(&path),
            ManageFilesBridgeSideEffect::AddToSessionContext(prompt) => {
                let session_id = self
                    .manage_session_context_target_session_id()
                    .ok_or_else(|| "No active agent session is available.".to_string())?;
                if self.insert_manage_file_context_into_agents_session(session_id, &prompt, cx) {
                    Ok(())
                } else {
                    Err("No active agent session is available.".to_string())
                }
            }
        }
    }

    pub(crate) fn manage_session_context_target_session_id(&self) -> Option<TerminalSessionId> {
        let mut candidates = Vec::new();
        if let Some(key) = self.project_editor_companion_active_terminal_key()
            && let Some(session_id) = self.shell_session_for_workspace_terminal_key(&key)
        {
            candidates.push(session_id);
        }
        if let Some(session_id) = self.focused_agents_or_companion_shell_session_id() {
            candidates.push(session_id);
        }
        if let (Some(active_project_id), Some(latest_key)) = (
            self.latest_sidebar_project_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.active_project_id.as_ref())
                .map(|project_id| project_id.0.as_str()),
            self.local_workspace_latest_focus_key.as_ref(),
        ) && latest_key.project_id == active_project_id
            && let Some(session_id) = self.local_workspace_session_mappings.get(latest_key)
        {
            candidates.push(*session_id);
        }
        let mut seen = HashSet::new();
        candidates.into_iter().find(|session_id| {
            seen.insert(*session_id)
                && self
                    .agents_workspace
                    .session(*session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                            && session.agent_icon.is_some()
                    })
        })
    }

    pub(crate) fn insert_manage_file_context_into_agents_session(
        &mut self,
        shell_session_id: TerminalSessionId,
        prompt: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if prompt.is_empty()
            || !self
                .agents_workspace
                .session(shell_session_id)
                .is_some_and(|session| {
                    session.presentation_state == TerminalSessionPresentationState::Running
                        && session.agent_icon.is_some()
                })
        {
            return false;
        }
        let companion_focused = matches!(
            self.focused_terminal_text_mount_target(),
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                if slot_id.session_id == shell_session_id
        );
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        if !companion_focused {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.paste_text(prompt, cx));
            cx.notify();
            return true;
        }
        #[cfg(target_os = "macos")]
        {
            let inserted = if companion_focused {
                self.send_text_bytes_to_focused_project_editor_companion_terminal_surface(
                    prompt.as_bytes(),
                )
            } else if let Some(pane_id) = pane_id {
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                };
                self.agents_terminal_ghostty_surface_matches(slot_id)
                    && self.send_text_bytes_to_focused_agents_terminal_surface(prompt.as_bytes())
            } else {
                false
            };
            if inserted {
                cx.notify();
                return true;
            }
        }
        false
    }

    /// Inserts a stashed prompt back into the mapped Agents input surface for a
    /// combined presentation session id ("P…:G…"). Chat sessions receive the
    /// prompt through their bounded composer callback; terminal sessions use
    /// their native paste semantics so multiline prompts do not submit.
    pub(crate) fn insert_stashed_prompt_into_agents_session(
        &mut self,
        combined_session_id: &str,
        content: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if content.is_empty() {
            return false;
        }
        let Some(shell_session_id) =
            self.local_workspace_session_mappings
                .iter()
                .find_map(|(key, mapped)| {
                    (gpui_combined_presentation_session_id(&key.project_id, &key.session_id)
                        == combined_session_id)
                        .then_some(*mapped)
                })
        else {
            return false;
        };
        if self.agents_chat_mode_sessions.contains(&shell_session_id) {
            return self.insert_prompt_into_session_chat(shell_session_id, content, cx);
        }
        // A session focused in a project-editor companion pane receives the
        // paste in place; switching the app into the Agents view just to
        // reveal a tab the user is already looking at would lose their editor
        // context.
        let companion_focused = matches!(
            self.focused_terminal_text_mount_target(),
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id))
                if slot_id.session_id == shell_session_id
        );
        let pane_id = self.agents_workspace.pane_id_for_session(shell_session_id);
        if !companion_focused {
            let Some(pane_id) = pane_id else {
                return false;
            };
            self.active_mode = TitlebarMode::Agents;
            self.agents_workspace.select_tab(pane_id, shell_session_id);
            self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
            self.scroll_workspace_pane_active_tab(pane_id);
        }
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&shell_session_id)
            .map(|record| record.view.clone())
        {
            view.update(cx, |view, cx| view.paste_text(content, cx));
            cx.notify();
            return true;
        }
        #[cfg(target_os = "macos")]
        if !companion_focused {
            if let Some(pane_id) = pane_id {
                let slot_id = AgentsTerminalBodyMountSlotId {
                    pane_id,
                    session_id: shell_session_id,
                };
                if self.agents_terminal_ghostty_surface_matches(slot_id)
                    && self.send_text_bytes_to_focused_agents_terminal_surface(content.as_bytes())
                {
                    cx.notify();
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn insert_paths_into_gpui_engine_terminal(
        &mut self,
        target: GpuiEngineTerminalEventTarget,
        paths: &[PathBuf],
        cx: &mut gpui::Context<Self>,
    ) {
        let mut next_image_number = 1usize;
        let text = paths
            .iter()
            .map(|path| {
                if is_project_board_image_file_path(path) {
                    let markdown =
                        terminal_clipboard_markdown_image_reference(path, next_image_number);
                    next_image_number += 1;
                    markdown
                } else {
                    path.to_string_lossy().into_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        if text.is_empty() {
            return;
        }
        let view = match target {
            GpuiEngineTerminalEventTarget::Agents(session_id) => self
                .agents_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
            GpuiEngineTerminalEventTarget::Command(session_id) => self
                .command_gpui_engine_terminals
                .get(&session_id)
                .map(|record| record.view.clone()),
        };
        if let Some(view) = view {
            view.update(cx, |view, cx| view.send_text_input(&text, cx));
        }
    }

    /// True while gxserver reports a first-prompt title job in flight for the
    /// mapped workspace session. Drives both terminal input suppression and
    /// the blocking "Generating title" pane overlay so they can never disagree.
    pub(crate) fn agents_session_is_generating_first_prompt_title(
        &self,
        shell_session_id: TerminalSessionId,
    ) -> bool {
        self.agents_workspace
            .session(shell_session_id)
            .is_some_and(|session| session.is_generating_first_prompt_title)
    }

    pub(crate) fn sync_gpui_engine_first_prompt_input_suppression(&mut self, cx: &mut gpui::Context<Self>) {
        let suppression_by_session = self
            .agents_gpui_engine_terminals
            .keys()
            .copied()
            .map(|shell_session_id| {
                let suppress =
                    self.agents_session_is_generating_first_prompt_title(shell_session_id);
                (shell_session_id, suppress)
            })
            .collect::<Vec<_>>();

        for (shell_session_id, suppress) in suppression_by_session {
            if let Some(record) = self.agents_gpui_engine_terminals.get(&shell_session_id) {
                record.view.update(cx, |view, cx| {
                    view.set_input_suppressed(suppress, cx);
                });
            }
        }
    }

    /// Mirror each open GPUI-engine find's totals into the shared search
    /// state so the search bar count label matches the native path.
    pub(crate) fn sync_gpui_engine_search_totals(&mut self, cx: &mut gpui::Context<Self>) {
        fn mirror_totals<'a>(
            records: impl Iterator<Item = &'a terminal_gpui_engine::GpuiEngineTerminalRecord>,
            osc_states: &mut HashMap<AgentsTerminalRuntimeSessionId, GpuiTerminalRuntimeOscState>,
            cx: &gpui::App,
        ) -> bool {
            let mut changed = false;
            for record in records {
                let Some((total, selected)) = record.view.read(cx).search_totals() else {
                    continue;
                };
                let Some(search) = osc_states
                    .get_mut(&record.runtime_session_id)
                    .and_then(|state| state.search.as_mut())
                else {
                    continue;
                };
                let total = Some(total as u64);
                let selected = Some(selected as u64);
                if search.total != total || search.selected != selected {
                    search.total = total;
                    search.selected = selected;
                    changed = true;
                }
            }
            changed
        }
        let mut changed = mirror_totals(
            self.agents_gpui_engine_terminals.values(),
            &mut self.agents_terminal_runtime_osc_states,
            cx,
        );
        changed |= mirror_totals(
            self.command_gpui_engine_terminals.values(),
            &mut self.command_terminal_runtime_osc_states,
            cx,
        );
        if changed {
            cx.notify();
        }
    }

    /// The GPUI-engine record backing a runtime session id, if any
    /// (Agents and command maps share the runtime-id namespace).
    pub(crate) fn gpui_engine_record_for_runtime_session_id(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    ) -> Option<&terminal_gpui_engine::GpuiEngineTerminalRecord> {
        self.agents_gpui_engine_terminals
            .values()
            .chain(self.command_gpui_engine_terminals.values())
            .find(|record| record.runtime_session_id == runtime_session_id)
    }

    pub(crate) fn agents_terminal_native_views_may_be_visible(&self) -> bool {
        /*
        CDXC:GPUIWorkspaceTabDragVisibility 2026-07-03:
        Workspace/Agents tab drags treat mounted Agents terminals like a mode switch away from Agents: Running host reconciliation, parked-owner reattach, and ready-startup handoff promotion all wait until the drag ends. This hides the native Ghostty child views for the whole drag so the GPUI drag ghost and pane-body drop-edge bands stay visible, while parked owners keep every runtime surface alive for hide/show-only restore on drop or cancel. Startup candidates, launch plans, and hidden startup hosts intentionally keep running during a drag; only promotion to a visible Running host is deferred.
        */
        self.active_mode == TitlebarMode::Agents && !self.workspace_tab_drag_active
    }

    pub(crate) fn sync_agents_terminal_surface_host(
        &mut self,
        scale_factor: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUTerminalSurfaceHost 2026-06-22-22:45:
        The GPUI App owns native terminal host plans as runtime-only slot-keyed maps, not persisted workspace model data. Recompute them only from current rendered running Agents mount slots, active Agents visibility, and runtime body bounds so stale tabs, sleeping/missing sessions, hidden leaves, and inactive tabs cannot retain or fabricate native terminal views.

        CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-21:17:
        Host sync feeds a runtime-only native-view lifecycle boundary. Waiting decisions stay inert until the App-owned host view exists for an exact `NeedsRealNativeView` plan, and the lifecycle state must never be logged, persisted, represented by fallback handles, or backed by overlapping hit-test regions.

        CDXC:GPUTerminalSurfaceHost 2026-06-22-21:27:
        Active Agents mode is the visibility gate for terminal host reconciliation. A render-start bounds clear while Agents is visible should only mark the current slot as awaiting this frame's body bounds; switching away from Agents or losing the current running slot still clears stale host and lifecycle state through the normal detach path.

        CDXC:GPUTerminalNativeView 2026-06-22-22:59:
        The App owns one runtime host NSView per eligible visible Agents slot. Host sync may create each normal child only after a matching `NeedsRealNativeView` lifecycle decision and must drop only stale slot-owned views after surfaces are released; Ghostty surface focus and AppKit first-responder handoff are mirrored by the focused-slot sync path, while event routing, logging, persistence, and fallback surfaces remain out of scope.

        CDXC:GPUTerminalNativeView 2026-06-22-22:59:
        Same-slot bounds changes for the App-owned host view may execute only a frame update after owner reconciliation proves the lifecycle real-view handle still matches that owned child. Visibility, Ghostty focus, and AppKit first-responder focus each use separate bounded helpers; keyboard/mouse event translation, process lifecycle, logging, persistence, overlays, and hit-test routing remain out of scope.

        CDXC:GPUTerminalGhosttySurfaceConfig 2026-06-22-22:17:
        Prepare runtime GhosttySurfaceConfigRequest state after App-owned host-view reconciliation, using that real owned NSView handle and the current GPUI window scale. Keep the request out of persistence/logging and never synthesize fallback hosts, overlays, hidden hit regions, or rerouted input.

        CDXC:GPUTerminalGhosttySurface 2026-06-22-22:59:
        Consume prepared requests for every App-owned host NSView backing a current visible running Agents slot. Preserve each same-slot surface across frame updates, resize it from exact body bounds and current scale, show each host only after surface creation succeeds, mirror shell focus onto mounted Agents Ghostty surfaces and their AppKit host first responder, and drop/hide stale surfaces before releasing host views; command/cwd/env lifecycle, keyboard/mouse event translation, command-pane terminals, fallbacks, persistence, logging, overlays, hidden hit regions, and hit-test routing remain out of scope.

        CDXC:GPUITerminalRuntimeIdentity 2026-06-22-23:24:
        Runtime session ids are reconciled from shell `TerminalSessionId` before terminal host and Ghostty surface sync. The shell id remains the persisted layout identity across moves/splits/restore, while the runtime id is private app-process state that prunes when shell sessions disappear and must never be serialized or used as a title.

        CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
        Host sync also refreshes runtime-only startup candidates for visible selected Mounting sessions before any Running-only surface work. This creates no process, no fake success, no mount slot, no placeholder overlay, no persistent runtime field, and no log entry; Ready/Failed results must cross the explicit startup result boundary later.

        CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
        Startup body geometry is pruned before coordinator sync and never feeds the Running-only host path by itself. Pending startup records may know only whether an exact current Mounting body bounds/scale record exists; real mount slots, Running AppKit host views, and Ghostty owners become available only through the later ready handoff that transfers existing startup ownership.

        CDXC:GPUITerminalStartupLaunchPlan 2026-06-23-00:22:
        Startup launch plans are derived after pending Mounting records refresh and before Running-only host reconciliation. This keeps future launch readiness tied to active Agents visibility, matching runtime/session identity, and exact startup body geometry while preventing Mounting plans alone from creating `AgentsTerminalBodyMountSlotId` Running host state, Ghostty surfaces, process launches, logs, persistence, or Ready/Failed results.

        CDXC:GPUITerminalStartupNativeHost 2026-06-23-00:32:
        Mounting may now prepare a hidden startup host/config request boundary from current launch plans, but that state is startup-only and runtime-only. It must stay separate from Running host/surface maps, remain hidden/unfocused, accept launch payloads only from the explicit runtime source boundary, and disappear with stale visibility, selection, lifecycle state, shell session, or runtime identity.

        CDXC:GPUITerminalStartupGhosttySurface 2026-06-23-03:33:
        Startup config requests may now create startup-owned Ghostty surfaces only inside the startup boundary. The startup surface map is keyed by `AgentsTerminalStartupBodySlotId`, preserves only through same-runtime geometry gaps, and may feed the Running `agents_terminal_ghostty_surfaces` map only through the exact ready handoff that transfers ownership without dropping or recreating the process.

        CDXC:GPUITerminalStartupHandoff 2026-06-23-04:25:
        Ready startup metadata must be consumed before Running host reconciliation so the current Mounting tab can become Running with its existing hidden host and Ghostty surface already re-owned by the Running maps. The handoff seeds Running body bounds from the startup launch plan and then lets the existing Running sync resize, show, and focus through normal mount-slot paths.

        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Sleeping wake and popped-out reattach use a separate parked-owner path before Running host reconciliation. Only an exact parked host/surface owner plus current non-startup Mounting body geometry may become Running; this path must not enter startup candidates, hidden startup hosts, launch payloads, completion intents, runtime-id rotation, or fallback surface creation.

        CDXC:GPUITerminalGhosttyClose 2026-06-23-04:49:
        Confirmed close callbacks are consumed before Running host reconciliation computes current slots. Removing the shell session first lets the existing hide/detach path drop the matching Ghostty surface before releasing the AppKit host, while confirmation-needed callbacks become runtime-only pending close-confirm state for the family-scoped normal-layout UI surface.

        CDXC:GPUITerminalCloseConfirm 2026-06-23-05:39:
        Running Agents close-confirm sync happens before confirmed-close consumption and process-exit polling so a confirmation-needed callback cannot delete shell state, and any stale pending prompt identity is pruned as soon as the current mount slot or surface owner no longer matches.

        CDXC:GPUITerminalProcessExit 2026-06-23-05:30:
        Mounted Running process-exited polling follows confirmed-close consumption and precedes Running host reconciliation. Exited sessions are removed from the workspace model first, runtime ids are reconciled immediately afterward, and stale hosts/surfaces detach through the normal render/bounds path without another Ghostty close request.

        CDXC:GPUTerminalGhosttySurface 2026-06-22-22:39:
        Ghostty surfaces borrow the App-owned host NSView through the C config, so any host detach/replacement must drop the surface before the AppKit owner releases the child view. Keep this as an explicit pre-reconcile step instead of relying on struct field drop order during runtime slot changes.

        CDXC:GPUITitlebarKeepAwake 2026-06-26-00:29:
        Agents startup ready handoff, parked-owner reattach, confirmed close, and process-exit cleanup can change whether an existing safe `AgentTerminalActivity::Working` row counts as running. Compare the working-session count across this lifecycle boundary and resync Keep Awake automation only from app-owned model state.
        */
        let working_session_count_before =
            gpui_keep_awake_working_session_count(&self.agents_workspace, &self.command_pane);
        self.agents_terminal_runtime_sessions
            .reconcile_with_workspace(&self.agents_workspace);
        self.sync_agents_gpui_engine_terminals(cx);
        /*
        Reconcile any Chat launch intent that could not be consumed when its
        shell mapping was first created (for example, until agent metadata
        arrived). The terminal runtime remains live behind the Chat surface.
        */
        self.reconcile_preferred_agents_chat_launch_intents(cx);
        prune_agents_terminal_startup_body_slot_geometries(
            self.active_mode == TitlebarMode::Agents,
            &self.agents_workspace,
            &mut self.agents_terminal_startup_body_slot_geometries,
        );
        prune_agents_terminal_parked_owner_body_slot_geometries(
            self.active_mode == TitlebarMode::Agents,
            &self.agents_workspace,
            &mut self.agents_terminal_parked_owner_body_slot_geometries,
        );
        #[cfg(target_os = "macos")]
        prune_agents_terminal_parked_runtime_owners(
            &self.agents_workspace,
            &self.agents_terminal_runtime_sessions,
            &mut self.agents_terminal_parked_runtime_owners,
            &self.agents_terminal_host_native_views,
            &self.agents_terminal_ghostty_surfaces,
        );
        self.agents_terminal_startup_coordinator
            .sync_visible_mounting_startup_candidates(
                self.active_mode == TitlebarMode::Agents,
                &self.agents_workspace,
                &mut self.agents_terminal_runtime_sessions,
                &self.agents_terminal_startup_body_slot_geometries,
            );
        self.agents_terminal_startup_coordinator
            .sync_startup_launch_plans(
                self.active_mode == TitlebarMode::Agents,
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &self.agents_terminal_startup_body_slot_geometries,
            );
        // CDXC:GPUITerminalGpuiEngine 2026-07-11: every startup launch plan
        // (new terminals, restored sessions, splits, retry, and materialize)
        // uses the GPUI-composited engine on every OS. The native hidden-host
        // implementation remains compiled but receives no runtime work.
        self.spawn_agents_terminal_startup_gpui_engine_terminals(cx);
        #[cfg(target_os = "macos")]
        self.reattach_agents_terminal_parked_runtime_owners();
        #[cfg(target_os = "macos")]
        {
            self.agents_terminal_close_confirms
                .sync_from_confirmation_needed_callbacks(
                    &self.agents_workspace,
                    &self.agents_terminal_runtime_sessions,
                    &self.agents_terminal_ghostty_surfaces,
                );
            let mut shell_state_changed =
                self.consume_confirmed_agents_terminal_ghostty_surface_closes(cx);
            if shell_state_changed {
                self.agents_terminal_runtime_sessions
                    .reconcile_with_workspace(&self.agents_workspace);
            }
            if consume_exited_agents_terminal_ghostty_surfaces(
                &mut self.agents_workspace,
                &mut self.agents_terminal_runtime_sessions,
                &self.agents_terminal_ghostty_surfaces,
            ) {
                shell_state_changed = true;
                self.agents_terminal_close_confirms.prune_stale(
                    &self.agents_workspace,
                    &self.agents_terminal_runtime_sessions,
                    &self.agents_terminal_ghostty_surfaces,
                );
            }

            if shell_state_changed {
                self.set_shell_focus(ShellFocusTarget::AgentsPane(
                    self.agents_workspace.focused_pane,
                ));
                self.persist_shell_layout_state();
            }
        }
        let working_session_count_after =
            gpui_keep_awake_working_session_count(&self.agents_workspace, &self.command_pane);
        if working_session_count_before != working_session_count_after {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        // GPUI-engine sessions render the composited element in the same
        // body slot; they must never create native hosts or surfaces.
        let gpui_engine_enabled = shared_settings::shared_sidebar_settings_snapshot()
            .gpui_terminal_engine_settings()
            .enabled;
        let current_slot_ids = if gpui_engine_enabled {
            Vec::new()
        } else {
            self.agents_workspace
                .rendered_terminal_body_mount_slots()
                .into_iter()
                .filter(|slot_id| {
                    !self
                        .agents_gpui_engine_terminals
                        .contains_key(&slot_id.session_id)
                })
                .collect::<Vec<_>>()
        };
        self.agents_terminal_launch_payload_source
            .retain_live_mount_slots(
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
            );
        let commands = self.agents_terminal_surface_host.sync_visible_agents_slots(
            self.agents_terminal_native_views_may_be_visible(),
            &current_slot_ids,
            &self.agents_terminal_mount_slot_bounds,
        );
        let decisions = self
            .agents_terminal_surface_lifecycle
            .reconcile_host_commands(&commands);
        #[cfg(target_os = "macos")]
        {
            self.park_or_drop_agents_terminal_ghostty_surface_before_host_detach(&commands);
            let frame_operations =
                terminal_native_view::reconcile_app_owned_terminal_host_native_view(
                    &mut self.agents_terminal_host_native_views,
                    &mut self.agents_terminal_surface_lifecycle,
                    self.parent_ns_view,
                    &commands,
                    &decisions,
                    terminal_native_view::TerminalHostNativeViewFactory::create,
                );
            terminal_native_view::execute_app_owned_terminal_host_frame_operations(
                &self.agents_terminal_host_native_views,
                &frame_operations,
            );
            let terminal_config = current_gpui_terminal_ghostty_surface_config();
            let mut invalid_launch_payload_slot_ids = HashSet::new();
            let mut config_requests = HashMap::new();
            let local_workspace_shell_session_ids = self
                .local_workspace_session_mappings
                .values()
                .copied()
                .collect::<HashSet<_>>();
            for (slot_id, host_view) in self.agents_terminal_host_native_views.iter() {
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
                if self.agents_terminal_ghostty_surfaces.contains_key(slot_id) {
                    config_requests.insert(*slot_id, request);
                    continue;
                }
                /*
                CDXC:GPUIWorkspaceSessionReattach 2026-07-10:
                A restored gxserver-backed Agents tab is persisted as Running
                presentation state, but its process-local attach payload and
                terminal owner do not survive an app restart. Match the macOS
                workspace boundary: the native Ghostty surface may be created
                only after the sidebar focus flow supplies gxserver's exact
                zmx attach command for this mapped session. Keeping the host
                empty while that payload is absent prevents a restored tab
                from spawning the user's default login shell and then being
                mistaken for an already-attached live terminal.
                */
                if local_workspace_shell_session_ids.contains(&slot_id.session_id) {
                    match self
                        .agents_terminal_launch_payload_source
                        .take_payload_for_mount_slot(runtime_session_id, *slot_id)
                    {
                        Ok(Some(launch_payload)) => {
                            config_requests
                                .insert(*slot_id, request.with_launch_payload(launch_payload));
                        }
                        Ok(None) => {}
                        Err(_) => {
                            invalid_launch_payload_slot_ids.insert(*slot_id);
                        }
                    }
                    continue;
                }
                match agents_terminal_config_request_with_launch_payload_source(
                    *slot_id,
                    runtime_session_id,
                    request,
                    &mut self.agents_terminal_launch_payload_source,
                ) {
                    Ok(request) => {
                        config_requests.insert(*slot_id, request);
                    }
                    Err(_) => {
                        invalid_launch_payload_slot_ids.insert(*slot_id);
                    }
                }
            }
            self.agents_terminal_ghostty_surface_config_requests = config_requests;
            for slot_id in invalid_launch_payload_slot_ids.iter().copied() {
                self.agents_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.agents_terminal_host_native_views.get(&slot_id),
                    false,
                );
            }
            self.agents_terminal_host_native_views
                .retain(|slot_id, _| !invalid_launch_payload_slot_ids.contains(slot_id));
            self.sync_agents_terminal_ghostty_surfaces(cx);
            self.reconcile_preferred_agents_chat_launch_intents(cx);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (decisions, scale_factor);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_startup_host_config_requests(&mut self) {
        let startup_launch_plans = self
            .agents_terminal_startup_coordinator
            .startup_launch_plans();
        let startup_host_preservation_keys = self
            .agents_terminal_startup_coordinator
            .startup_host_preservation_keys(
                self.active_mode == TitlebarMode::Agents,
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
            );
        drop_agents_terminal_startup_ghostty_surface_owners_before_host_reconcile(
            &mut self.agents_terminal_startup_ghostty_surfaces,
            &self.agents_terminal_startup_host_native_views,
            &startup_launch_plans,
            &startup_host_preservation_keys,
            self.parent_ns_view,
        );
        reconcile_agents_terminal_startup_host_config_requests(
            &mut self.agents_terminal_startup_host_native_views,
            Some(&mut self.agents_terminal_startup_ghostty_surfaces),
            &mut self.agents_terminal_startup_ghostty_surface_config_requests,
            &startup_launch_plans,
            &startup_host_preservation_keys,
            &self.agents_terminal_startup_launch_payload_source,
            current_gpui_terminal_ghostty_surface_config(),
            self.parent_ns_view,
            terminal_native_view::TerminalHostNativeViewFactory::create,
        );
        self.sync_agents_terminal_startup_ghostty_surfaces(
            &startup_launch_plans,
            &startup_host_preservation_keys,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_startup_ghostty_surfaces(
        &mut self,
        startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
        startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    ) {
        reconcile_agents_terminal_startup_ghostty_surface_owners(
            &mut self.agents_terminal_startup_ghostty_surfaces,
            &mut self.agents_terminal_ghostty_app,
            &self.agents_terminal_startup_ghostty_surface_config_requests,
            startup_launch_plans,
            startup_host_preservation_keys,
            terminal_ghostty_surface::GhosttyAppOwner::new,
        );
        let metadata_snapshots = agents_terminal_startup_surface_metadata_snapshots(
            &self.agents_terminal_startup_ghostty_surfaces,
        );
        let failed_results = failed_agents_terminal_startup_results_from_metadata(
            &self.agents_terminal_startup_coordinator,
            self.active_mode == TitlebarMode::Agents,
            &self.agents_workspace,
            &self.agents_terminal_runtime_sessions,
            metadata_snapshots.iter().copied(),
        );
        for failed_result in failed_results {
            self.apply_agents_terminal_startup_result(failed_result);
        }
        sync_agents_terminal_startup_readiness_signal_preparations(
            &mut self.agents_terminal_startup_coordinator,
            metadata_snapshots,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn promote_ready_agents_terminal_startup_handoffs(&mut self) {
        let handoff_plans = self
            .agents_terminal_startup_coordinator
            .startup_readiness_handoff_plans(
                self.agents_terminal_native_views_may_be_visible(),
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
            );

        for handoff_plan in handoff_plans {
            if transfer_ready_agents_terminal_startup_handoff(
                &mut self.agents_terminal_startup_coordinator,
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &mut self.agents_terminal_startup_body_slot_geometries,
                &mut self.agents_terminal_startup_host_native_views,
                &mut self.agents_terminal_startup_ghostty_surfaces,
                &mut self.agents_terminal_startup_ghostty_surface_config_requests,
                &mut self.agents_terminal_mount_slot_bounds,
                &mut self.agents_terminal_host_native_views,
                &mut self.agents_terminal_ghostty_surfaces,
                &self.agents_terminal_ghostty_surface_config_requests,
                handoff_plan,
            ) {
                self.agents_terminal_startup_launch_payload_source
                    .remove_payload_for_launch_plan(handoff_plan.startup_launch_plan);
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn reattach_agents_terminal_parked_runtime_owners(&mut self) {
        let reattach_plans = agents_terminal_parked_owner_reattach_plans(
            self.agents_terminal_native_views_may_be_visible(),
            &self.agents_workspace,
            &self.agents_terminal_runtime_sessions,
            &self.agents_terminal_parked_runtime_owners,
            &self.agents_terminal_parked_owner_body_slot_geometries,
            &self.agents_terminal_mount_slot_bounds,
        );

        for reattach_plan in reattach_plans {
            let _ = transfer_agents_terminal_parked_runtime_owner_reattach(
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &mut self.agents_terminal_parked_runtime_owners,
                &mut self.agents_terminal_parked_owner_body_slot_geometries,
                &mut self.agents_terminal_mount_slot_bounds,
                &mut self.agents_terminal_host_native_views,
                &mut self.agents_terminal_ghostty_surfaces,
                &self.agents_terminal_ghostty_surface_config_requests,
                reattach_plan,
            );
        }
    }

    #[allow(dead_code)]
    pub(crate) fn apply_agents_terminal_startup_result(
        &mut self,
        result: AgentsTerminalStartupResult,
    ) -> bool {
        #[cfg(target_os = "macos")]
        if let AgentsTerminalStartupResult::Ready { completion_intent } = result
            && !self
                .agents_gpui_engine_terminals
                .values()
                .any(|record| record.runtime_session_id == completion_intent.runtime_session_id)
        {
            // Engine-owned startups (the default pipeline) skip the native
            // hidden-host handoff and use the shared coordinator arm below;
            // only native opt-out startups transfer host/surface ownership.
            let Some(handoff_plan) = self
                .agents_terminal_startup_coordinator
                .startup_readiness_handoff_plan_for_runtime_session(
                    self.agents_terminal_native_views_may_be_visible(),
                    &self.agents_workspace,
                    &self.agents_terminal_runtime_sessions,
                    completion_intent.runtime_session_id,
                )
                .filter(|handoff_plan| handoff_plan.completion_intent == completion_intent)
            else {
                return false;
            };
            let transferred = transfer_ready_agents_terminal_startup_handoff(
                &mut self.agents_terminal_startup_coordinator,
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &mut self.agents_terminal_startup_body_slot_geometries,
                &mut self.agents_terminal_startup_host_native_views,
                &mut self.agents_terminal_startup_ghostty_surfaces,
                &mut self.agents_terminal_startup_ghostty_surface_config_requests,
                &mut self.agents_terminal_mount_slot_bounds,
                &mut self.agents_terminal_host_native_views,
                &mut self.agents_terminal_ghostty_surfaces,
                &self.agents_terminal_ghostty_surface_config_requests,
                handoff_plan,
            );
            if transferred {
                self.agents_terminal_startup_launch_payload_source
                    .remove_payload_for_launch_plan(handoff_plan.startup_launch_plan);
            }
            return transferred;
        }

        let completion_intent = result.completion_intent();
        let had_current_completion_intent = self
            .agents_terminal_startup_coordinator
            .startup_completion_intents_by_runtime_session
            .get(&completion_intent.runtime_session_id)
            .copied()
            == Some(completion_intent);
        let changed = self
            .agents_terminal_startup_coordinator
            .apply_startup_result(
                &mut self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                result,
            );
        let exact_completion_intent_pruned = had_current_completion_intent
            && !self
                .agents_terminal_startup_coordinator
                .startup_completion_intents_by_runtime_session
                .contains_key(&completion_intent.runtime_session_id);
        if changed || exact_completion_intent_pruned {
            /*
            CDXC:GPUITerminalStartupCompletion 2026-06-23-03:51:
            Applying Failed or stale-pruning an exact current startup result may retire startup-owned runtime state without creating Running ownership. Ready promotion on macOS must use the startup handoff path above so host/surface ownership moves before cleanup instead of dropping the process and recreating it later.
            */
            #[cfg(target_os = "macos")]
            {
                prune_agents_terminal_startup_runtime_state_for_completion_intent(
                    &mut self.agents_terminal_startup_body_slot_geometries,
                    &mut self.agents_terminal_startup_ghostty_surfaces,
                    &mut self.agents_terminal_startup_ghostty_surface_config_requests,
                    &mut self.agents_terminal_startup_host_native_views,
                    &mut self.agents_terminal_startup_launch_payload_source,
                    completion_intent,
                );
            }
            #[cfg(not(target_os = "macos"))]
            {
                prune_agents_terminal_startup_runtime_state_for_completion_intent(
                    &mut self.agents_terminal_startup_body_slot_geometries,
                    &mut self.agents_terminal_startup_launch_payload_source,
                    completion_intent,
                );
            }
        }
        changed
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn park_or_drop_agents_terminal_ghostty_surface_before_host_detach(
        &mut self,
        commands: &[terminal_surface_host::NativeTerminalSurfaceHostCommand],
    ) {
        for command in commands {
            let terminal_surface_host::NativeTerminalSurfaceHostCommand::HideAndDetach { plan } =
                *command
            else {
                continue;
            };

            if park_agents_terminal_runtime_owner_before_host_detach(
                &self.agents_workspace,
                &self.agents_terminal_runtime_sessions,
                &mut self.agents_terminal_parked_runtime_owners,
                &mut self.agents_terminal_host_native_views,
                &mut self.agents_terminal_ghostty_surfaces,
                plan,
            ) {
                self.agents_terminal_appkit_focused_host = None;
                self.sync_agents_terminal_ghostty_surface_focus();
                continue;
            }

            self.agents_terminal_ghostty_surfaces.remove(&plan.slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.agents_terminal_host_native_views.get(&plan.slot_id),
                false,
            );
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_ghostty_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        let host_slot_ids = self
            .agents_terminal_host_native_views
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let request_slot_ids = self
            .agents_terminal_ghostty_surface_config_requests
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let stale_surface_slot_ids = self
            .agents_terminal_ghostty_surfaces
            .keys()
            .copied()
            .filter(|slot_id| {
                !host_slot_ids.contains(slot_id) || !request_slot_ids.contains(slot_id)
            })
            .collect::<Vec<_>>();

        for slot_id in stale_surface_slot_ids {
            self.agents_terminal_ghostty_surfaces.remove(&slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.agents_terminal_host_native_views.get(&slot_id),
                false,
            );
        }

        self.sync_agents_terminal_ghostty_surface_focus();

        if self.agents_terminal_host_native_views.is_empty() {
            return;
        }

        if self.agents_terminal_ghostty_app.is_none() {
            let Ok(app) = terminal_ghostty_surface::GhosttyAppOwner::new() else {
                self.agents_terminal_ghostty_surfaces.clear();
                for host_view in self.agents_terminal_host_native_views.values() {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        Some(host_view),
                        false,
                    );
                }
                return;
            };
            self.agents_terminal_ghostty_app = Some(app);
        }

        let host_plans = self
            .agents_terminal_host_native_views
            .iter()
            .map(|(slot_id, host_view)| (*slot_id, host_view.attachment_plan()))
            .collect::<Vec<_>>();

        for (slot_id, plan) in host_plans {
            let Some(runtime_session_id) = self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(slot_id.session_id)
            else {
                self.agents_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.agents_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            };

            let Some(request) = self
                .agents_terminal_ghostty_surface_config_requests
                .get(&slot_id)
                .cloned()
            else {
                self.agents_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.agents_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            };

            if self
                .agents_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() != slot_id
                        || surface.runtime_session_id() != runtime_session_id
                })
            {
                self.agents_terminal_ghostty_surfaces.remove(&slot_id);
            }

            if !self.agents_terminal_ghostty_surfaces.contains_key(&slot_id) {
                let Some(app) = self.agents_terminal_ghostty_app.as_ref() else {
                    return;
                };
                let Ok(surface) = terminal_ghostty_surface::GhosttySurfaceOwner::new(
                    app,
                    slot_id,
                    runtime_session_id,
                    &request,
                ) else {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.agents_terminal_host_native_views.get(&slot_id),
                        false,
                    );
                    continue;
                };
                self.agents_terminal_ghostty_surfaces
                    .insert(slot_id, surface);
            }

            let update_failed = self
                .agents_terminal_ghostty_surfaces
                .get_mut(&slot_id)
                .is_some_and(|surface| {
                    surface
                        .update_content_scale_and_size(plan.bounds, request.scale_factor())
                        .is_err()
                });
            if update_failed {
                self.agents_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.agents_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            }

            let surface_mounted = self.agents_terminal_ghostty_surfaces.contains_key(&slot_id);
            if surface_mounted {
                if let (Some(host_view), Some(surface)) = (
                    self.agents_terminal_host_native_views.get(&slot_id),
                    self.agents_terminal_ghostty_surfaces.get(&slot_id),
                ) {
                    /*
                    CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
                    The AppKit key bridge is registered only after the exact Running Agents host view and Ghostty surface both exist. The registry carries only process-local pointers and function slots for synchronous key forwarding; it must not store typed text, terminal content, commands, paths, titles, logs, or shell state.
                    */
                    terminal_ghostty_surface::register_native_key_target(
                        host_view.native_view_handle(),
                        surface,
                    );
                }
            }
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.agents_terminal_host_native_views.get(&slot_id),
                surface_mounted,
            );
        }

        self.sync_agents_terminal_ghostty_surface_focus();

        if let Some(app) = self.agents_terminal_ghostty_app.as_ref() {
            if !self.agents_terminal_ghostty_surfaces.is_empty() {
                app.tick_if_woken();
                app.tick();
                self.drain_agents_terminal_runtime_clipboard_requests(cx);
            };
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_agents_terminal_runtime_clipboard_requests(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUITerminalClipboard 2026-06-23-19:07:
        Agents runtime clipboard handoff is authorized by exact mounted surface ownership, not focus. The drain snapshots current Agents mount keys, re-gets the still-mounted owner on the app thread, enables only standard clipboard for owner-local queued Ghostty requests, reads only explicit string entries, and writes only runtime-provided text without logging, persistence, selection clipboard support, or fallback requester inference.

        CDXC:GPUITerminalImagePaste 2026-06-27-10:28:
        Runtime Ghostty clipboard reads share the direct-paste previewable-image normalization so menu/Cmd paste and runtime clipboard requests convert the same validated image inputs while preserving the disabled setting as explicit-string-only.
        */
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = self
            .agents_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();

        let mut runtime_osc_state_changed = false;
        let mut bell_shell_session_ids = Vec::new();
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self.agents_terminal_ghostty_surfaces.get(&slot_id) else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if !action_events.is_empty() {
                let runtime_session_id = surface.runtime_session_id();
                terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                    let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } =
                        event
                    else {
                        return None;
                    };
                    Some((runtime_session_id, url.clone()))
                }));
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::RingBell
                    )
                }) {
                    bell_shell_session_ids.push(slot_id.session_id);
                }
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                    )
                }) {
                    self.terminal_search_focus_pending = Some(surface.runtime_session_id());
                }
                runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                    &mut self.agents_terminal_runtime_osc_states,
                    runtime_session_id,
                    action_events,
                );
            }
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .agents_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        for shell_session_id in bell_shell_session_ids {
            self.dispatch_gpui_workspace_terminal_bell(shell_session_id, cx);
        }
        if !self.agents_terminal_runtime_osc_states.is_empty() {
            let live_runtime_session_ids = self
                .agents_terminal_ghostty_surfaces
                .values()
                .map(|surface| surface.runtime_session_id())
                .chain(
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .values()
                        .map(|surface| surface.runtime_session_id()),
                )
                .chain(
                    self.agents_gpui_engine_terminals
                        .values()
                        .map(|record| record.runtime_session_id),
                )
                .collect::<HashSet<_>>();
            self.agents_terminal_runtime_osc_states
                .retain(|runtime_session_id, _| {
                    live_runtime_session_ids.contains(runtime_session_id)
                });
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surfaces(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let host_slot_ids = self
            .project_editor_companion_terminal_host_native_views
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let request_slot_ids = self
            .project_editor_companion_terminal_ghostty_surface_config_requests
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let stale_surface_slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .filter(|slot_id| {
                !host_slot_ids.contains(slot_id) || !request_slot_ids.contains(slot_id)
            })
            .collect::<Vec<_>>();

        for slot_id in stale_surface_slot_ids {
            self.project_editor_companion_terminal_ghostty_surfaces
                .remove(&slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
                false,
            );
        }

        self.sync_project_editor_companion_terminal_ghostty_surface_focus();

        if self
            .project_editor_companion_terminal_host_native_views
            .is_empty()
        {
            return;
        }

        if self.agents_terminal_ghostty_app.is_none() {
            let Ok(app) = terminal_ghostty_surface::GhosttyAppOwner::new() else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .clear();
                for host_view in self
                    .project_editor_companion_terminal_host_native_views
                    .values()
                {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        Some(host_view),
                        false,
                    );
                }
                return;
            };
            self.agents_terminal_ghostty_app = Some(app);
        }

        let host_plans = self
            .project_editor_companion_terminal_host_native_views
            .iter()
            .map(|(slot_id, host_view)| (*slot_id, host_view.attachment_plan()))
            .collect::<Vec<_>>();

        for (slot_id, plan) in host_plans {
            let Some(runtime_session_id) = self
                .agents_terminal_runtime_sessions
                .runtime_session_id_for_shell_session(slot_id.session_id)
            else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            };
            let Some(request) = self
                .project_editor_companion_terminal_ghostty_surface_config_requests
                .get(&slot_id)
                .cloned()
            else {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            };

            if self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() != slot_id
                        || surface.runtime_session_id() != runtime_session_id
                })
            {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
            }

            if !self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                let Some(app) = self.agents_terminal_ghostty_app.as_ref() else {
                    return;
                };
                let Ok(surface) = terminal_ghostty_surface::GhosttySurfaceOwner::new(
                    app,
                    slot_id,
                    runtime_session_id,
                    &request,
                ) else {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.project_editor_companion_terminal_host_native_views
                            .get(&slot_id),
                        false,
                    );
                    continue;
                };
                self.project_editor_companion_terminal_ghostty_surfaces
                    .insert(slot_id, surface);
            }

            let update_failed = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
                .is_some_and(|surface| {
                    surface
                        .update_content_scale_and_size(plan.bounds, request.scale_factor())
                        .is_err()
                });
            if update_failed {
                self.project_editor_companion_terminal_ghostty_surfaces
                    .remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    false,
                );
                continue;
            }

            let surface_mounted = self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id);
            if surface_mounted
                && let (Some(host_view), Some(surface)) = (
                    self.project_editor_companion_terminal_host_native_views
                        .get(&slot_id),
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .get(&slot_id),
                )
            {
                terminal_ghostty_surface::register_native_key_target(
                    host_view.native_view_handle(),
                    surface,
                );
            }
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
                surface_mounted,
            );
        }

        self.sync_project_editor_companion_terminal_ghostty_surface_focus();

        if let Some(app) = self.agents_terminal_ghostty_app.as_ref()
            && !self
                .project_editor_companion_terminal_ghostty_surfaces
                .is_empty()
        {
            app.tick_if_woken();
            app.tick();
            self.drain_project_editor_companion_terminal_runtime_requests(cx);
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_project_editor_companion_terminal_runtime_requests(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut runtime_osc_state_changed = false;
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)
            else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if action_events.is_empty() {
                continue;
            }
            let runtime_session_id = surface.runtime_session_id();
            terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } = event
                else {
                    return None;
                };
                Some((runtime_session_id, url.clone()))
            }));
            if action_events.iter().any(|event| {
                matches!(
                    event,
                    terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                )
            }) {
                self.terminal_search_focus_pending = Some(surface.runtime_session_id());
            }
            runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                &mut self.agents_terminal_runtime_osc_states,
                runtime_session_id,
                action_events,
            );
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .agents_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surface_focus(&mut self) {
        self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
            false,
        );
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        let mounted_slot_ids = self
            .project_editor_companion_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focused_slot_id = focused_project_editor_companion_terminal_surface_mount_slot(
            self.active_mode,
            self.shell_focus,
            self.project_editor_companion_focused_terminal_session_id(),
        )
        .filter(|slot_id| mounted_slot_ids.contains(slot_id));
        let app_has_focused_terminal_surface = focused_slot_id.is_some();

        for slot_id in mounted_slot_ids {
            if let Some(surface) = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get_mut(&slot_id)
            {
                surface.set_focus(Some(slot_id) == focused_slot_id);
            }
        }

        if let Some(app) = self.agents_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_slot_id.and_then(|slot_id| {
            if !self
                .project_editor_companion_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.project_editor_companion_terminal_host_native_views
                    .get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.project_editor_companion_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.project_editor_companion_terminal_appkit_focused_host = next_appkit_focus_identity
                .and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.project_editor_companion_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.project_editor_companion_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }

    /// Terminal BEL follows macOS ownership: Rust forwards only the bounded
    /// gxserver project/session identity of the rung Agents terminal; the
    /// sidebar runtime gates on `showNotificationOnTerminalBell` and commits
    /// the gxserver attention transition.
    pub(crate) fn dispatch_gpui_workspace_terminal_bell(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_BELL_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_bell_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// The Windows GPUI terminal engine observes the same OSC 0/2 title stream
    /// as the native macOS Ghostty surface. Forward the bounded raw observation
    /// to the sidebar runtime so gxserver remains the single owner of title
    /// trust, agent metadata reconciliation, persistence, and presentation.
    /// The sidebar settles bursts before calling `/api/ingestTerminalTitleEvent`.
    #[cfg(target_os = "windows")]
    pub(crate) fn dispatch_gpui_workspace_terminal_title_changed(
        &mut self,
        shell_session_id: TerminalSessionId,
        raw_title: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if raw_title.is_empty()
            || raw_title.chars().count() > GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_MAX_CHARS
            || raw_title.contains('\0')
        {
            return;
        }
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "rawTitle": raw_title,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_CHANGED_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_title_changed_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// ESC follows the terminal input path first; Rust forwards only the
    /// bounded gxserver project/session identity so the sidebar runtime can
    /// apply escape suppression and sync gxserver for
    /// `ghostex.gpui.sidebar.workspaceTerminalEscapePressed`.
    pub(crate) fn dispatch_gpui_workspace_terminal_escape_pressed(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionInterrupt.escapeTargetResolved",
            serde_json::json!({
                "projectId": key.project_id,
                "sessionId": key.session_id,
                "shellSessionId": format!("{:?}", shell_session_id),
            }),
        );
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_ESCAPE_PRESSED_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_escape_pressed_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// Escape inside the blocking "Generating title" overlay cancels the
    /// gxserver first-prompt title job. Rust only reports the bounded
    /// project/session identity; the sidebar runtime owns the cancel decision
    /// and the `/api/cancelFirstPromptAutoTitle` call for
    /// `ghostex.gpui.sidebar.workspaceFirstPromptTitleGenerationCancel`.
    pub(crate) fn dispatch_gpui_workspace_first_prompt_title_generation_cancel(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_FIRST_PROMPT_TITLE_CANCEL_MESSAGE_VERSION,
        });
        let script = gpui_workspace_first_prompt_title_generation_cancel_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    /// Rust reports only the mapped gxserver identity for direct workspace
    /// interaction; the sidebar runtime owns the actual attention decision and
    /// gxserver acknowledgement for
    /// `ghostex.gpui.sidebar.workspaceSessionAttentionAcknowledge`.
    pub(crate) fn dispatch_gpui_workspace_session_attention_acknowledge(
        &mut self,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, session_id)| (*session_id == shell_session_id).then(|| key.clone()))
        else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let message = serde_json::json!({
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_SESSION_ATTENTION_ACKNOWLEDGE_MESSAGE_VERSION,
        });
        let script = gpui_workspace_session_attention_acknowledge_script(&message);
        sidebar.update(cx, |surface, _| {
            surface.execute_app_owned_script(&script);
        });
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn dispatch_gpui_workspace_terminal_escape_pressed_for_native_view(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let companion_session_id =
            self.project_editor_companion_terminal_session_id_containing_responder(native_view);
        let Some(shell_session_id) = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or(companion_session_id)
        else {
            return;
        };
        // Temporary input-stealing diagnosis (2026-07-09): correlate terminal
        // Escape dispatches with first-responder churn in the same log.
        support_logs::append(
            support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.terminalFocus.terminalEscapeDispatched",
            serde_json::json!({ "shellSessionId": format!("{:?}", shell_session_id) }),
        );
        support_logs::append_temporary(
            support_logs::GpuiSupportLog::TerminalFocus,
            "TEMP.gpui.sessionInterrupt.nativeEscapeRouted",
            serde_json::json!({ "shellSessionId": format!("{:?}", shell_session_id) }),
        );
        self.dispatch_gpui_workspace_terminal_escape_pressed(shell_session_id, cx);
        // Escape is terminal input, not a companion-focus exit. Reassert the
        // exact mounted companion host after the sidebar attention sideband so
        // AppKit keeps subsequent keys on the same terminal surface.
        if companion_session_id == Some(shell_session_id)
            && matches!(
                self.shell_focus,
                ShellFocusTarget::ProjectEditorCompanion(mode) if mode == self.active_mode
            )
        {
            self.begin_programmatic_focus();
            self.sync_project_editor_companion_terminal_ghostty_surface_focus_with_appkit_handoff(
                true,
            );
            self.end_programmatic_focus();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn handle_native_terminal_prompt_editor_shortcut(
        &mut self,
        native_view: *mut std::ffi::c_void,
        cx: &mut gpui::Context<Self>,
    ) {
        let remote_shell_session_id = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or_else(|| {
                self.project_editor_companion_terminal_session_id_containing_responder(native_view)
            });
        let remote_context = remote_shell_session_id.and_then(|shell_session_id| {
            self.remote_prompt_editor_context_for_shell_session(shell_session_id)
                .map(|(key, connection_generation)| (shell_session_id, key, connection_generation))
        });
        if let Some((shell_session_id, key, connection_generation)) = remote_context {
            let native_view = native_view as usize;
            cx.spawn(async move |this, cx| {
                let _ = this.update_in(cx, |this, window, cx| {
                    this.queue_remote_prompt_editor_request(
                        shell_session_id,
                        &key,
                        connection_generation,
                        RemotePromptEditorDeliveryTarget::NativeView(native_view),
                        window,
                        cx,
                    );
                });
            })
            .detach();
            return;
        }
        let Some(originating_session_id) =
            self.prompt_editor_originating_session_id_for_native_view(native_view)
        else {
            let _ =
                terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(native_view);
            return;
        };
        let native_view = native_view as usize;
        cx.spawn(async move |this, cx| {
            let fronted = cx
                .background_executor()
                .spawn(
                    async move { gpui_ghostex_editor_daemon_front(Some(&originating_session_id)) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                if fronted {
                    if !this.prompt_editor_daemon_open {
                        this.prompt_editor_daemon_open = true;
                        cx.notify();
                    }
                } else {
                    let _ = terminal_ghostty_surface::send_native_prompt_editor_shortcut_for_view(
                        native_view as *mut std::ffi::c_void,
                    );
                }
            });
        })
        .detach();
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn prompt_editor_originating_session_id_for_native_view(
        &self,
        native_view: *mut std::ffi::c_void,
    ) -> Option<String> {
        if let Some(shell_session_id) = self
            .agents_terminal_session_id_containing_responder(native_view)
            .or_else(|| {
                self.project_editor_companion_terminal_session_id_containing_responder(native_view)
            })
        {
            let key = self.local_workspace_session_mappings.iter().find_map(
                |(key, mapped_session_id)| (*mapped_session_id == shell_session_id).then_some(key),
            )?;
            return Some(format!("{}:{}", key.project_id, key.session_id));
        }
        let command_session_id =
            self.command_terminal_session_id_containing_responder(native_view)?;
        let key = self
            .command_gxserver_session_mappings
            .get(&command_session_id)?;
        Some(format!("{}:{}", key.project_id, key.session_id))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn agents_terminal_slot_hovers_link(&self, slot_id: AgentsTerminalBodyMountSlotId) -> bool {
        self.agents_terminal_ghostty_surfaces
            .get(&slot_id)
            .map(|surface| surface.runtime_session_id())
            .and_then(|runtime_session_id| {
                self.agents_terminal_runtime_osc_states
                    .get(&runtime_session_id)
            })
            .is_some_and(|state| state.hovered_link_url.is_some())
    }

    // Hover state rides the native Ghostty surface map, which only exists on
    // macOS; without native surfaces no slot can report a hovered link (the
    // GPUI engine draws its own hover underline inside the element).
    #[cfg(not(target_os = "macos"))]
    pub(crate) fn agents_terminal_slot_hovers_link(&self, _slot_id: AgentsTerminalBodyMountSlotId) -> bool {
        false
    }

    pub(crate) fn command_terminal_slot_hovers_link(&self, slot_id: CommandTerminalBodyMountSlotId) -> bool {
        self.command_terminal_runtime_osc_states
            .get(&command_terminal_runtime_session_id(slot_id))
            .is_some_and(|state| state.hovered_link_url.is_some())
    }

    /// Cmd+F on a focused terminal surface triggers Ghostty's own
    /// `start_search` keybind action, matching the macOS surface-level key
    /// equivalent. The search bar itself opens when Ghostty answers with a
    /// START_SEARCH runtime action.
    /// Cmd+F on a focused GPUI-engine terminal opens the same search bar the
    /// native path uses, driving the element's viewport find instead of
    /// Ghostty binding actions.
    pub(crate) fn start_search_in_focused_gpui_engine_terminal(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let target = match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(target) => target,
            None => return false,
        };
        let (record, osc_states) = match target {
            FocusedTerminalTextTarget::Agents => {
                let Some(slot_id) = focused_agents_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    &self.agents_workspace,
                ) else {
                    return false;
                };
                let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.agents_terminal_runtime_osc_states)
            }
            FocusedTerminalTextTarget::Command => {
                let Some(slot_id) = focused_command_terminal_surface_mount_slot(
                    self.shell_focus,
                    &self.command_pane,
                ) else {
                    return false;
                };
                let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.command_terminal_runtime_osc_states)
            }
            FocusedTerminalTextTarget::ProjectEditorCompanion => {
                let Some(slot_id) = focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                ) else {
                    return false;
                };
                let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id)
                else {
                    return false;
                };
                (record, &mut self.agents_terminal_runtime_osc_states)
            }
        };
        let runtime_session_id = record.runtime_session_id;
        let view = record.view.clone();
        let state = osc_states.entry(runtime_session_id).or_default();
        if state.search.is_none() {
            state.search = Some(GpuiTerminalSearchState::default());
        }
        view.update(cx, |view, cx| view.set_search_needle("", cx));
        self.terminal_search_focus_pending = Some(runtime_session_id);
        cx.notify();
        true
    }

    /// Drive a GPUI-engine terminal's find from the shared search-bar action
    /// vocabulary (`search:<needle>`, `navigate_search:*`, `end_search`).
    pub(crate) fn perform_gpui_engine_terminal_search_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> Option<bool> {
        let record = self.gpui_engine_record_for_runtime_session_id(runtime_session_id)?;
        let view = record.view.clone();
        Some(match action {
            "navigate_search:next" => {
                view.update(cx, |view, cx| view.navigate_search(true, cx));
                true
            }
            "navigate_search:previous" => {
                view.update(cx, |view, cx| view.navigate_search(false, cx));
                true
            }
            "end_search" => {
                view.update(cx, |view, cx| view.clear_search(cx));
                true
            }
            _ => {
                if let Some(needle) = action.strip_prefix("search:") {
                    let needle = needle.to_string();
                    view.update(cx, |view, cx| view.set_search_needle(&needle, cx));
                    true
                } else {
                    false
                }
            }
        })
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn start_search_in_focused_terminal_surface(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if self.start_search_in_focused_gpui_engine_terminal(cx) {
            return true;
        }
        let started = match focused_terminal_text_target(self.active_mode, self.shell_focus) {
            Some(FocusedTerminalTextTarget::Agents) => focused_agents_terminal_surface_mount_slot(
                self.active_mode,
                self.shell_focus,
                &self.agents_workspace,
            )
            .and_then(|slot_id| self.agents_terminal_ghostty_surfaces.get(&slot_id))
            .is_some_and(|surface| surface.perform_binding_action("start_search")),
            Some(FocusedTerminalTextTarget::Command) => {
                focused_command_terminal_surface_mount_slot(self.shell_focus, &self.command_pane)
                    .and_then(|slot_id| self.command_terminal_ghostty_surfaces.get(&slot_id))
                    .is_some_and(|surface| surface.perform_binding_action("start_search"))
            }
            Some(FocusedTerminalTextTarget::ProjectEditorCompanion) => {
                focused_project_editor_companion_terminal_surface_mount_slot(
                    self.active_mode,
                    self.shell_focus,
                    self.project_editor_companion_focused_terminal_session_id(),
                )
                .and_then(|slot_id| {
                    self.project_editor_companion_terminal_ghostty_surfaces
                        .get(&slot_id)
                })
                .is_some_and(|surface| surface.perform_binding_action("start_search"))
            }
            None => false,
        };
        if started {
            cx.notify();
        }
        started
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn start_search_in_focused_terminal_surface(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        self.start_search_in_focused_gpui_engine_terminal(cx)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn perform_terminal_search_binding_action(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        action: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if let Some(handled) =
            self.perform_gpui_engine_terminal_search_action(runtime_session_id, action, cx)
        {
            return handled;
        }
        if let Some(surface) = self
            .agents_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        if let Some(surface) = self
            .command_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        if let Some(surface) = self
            .project_editor_companion_terminal_ghostty_surfaces
            .values()
            .find(|surface| surface.runtime_session_id() == runtime_session_id)
        {
            return surface.perform_binding_action(action);
        }
        false
    }

    /// Typing in the GPUI search bar mirrors macOS ownership: the local search
    /// state's needle is the source of truth updated from the field, then the
    /// needle is pushed into Ghostty via the `search:<needle>` keybind action.
    pub(crate) fn update_terminal_search_needle(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        needle: &str,
    ) -> bool {
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(search) = osc_states
                .get_mut(&runtime_session_id)
                .and_then(|state| state.search.as_mut())
            {
                search.needle = needle.to_string();
                return true;
            }
        }
        false
    }

    pub(crate) fn handle_terminal_search_input_event(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        input: &Entity<InputState>,
        event: &InputEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        #[cfg(target_os = "macos")]
        match event {
            InputEvent::Change => {
                let needle = input.read(cx).value().to_string();
                if self.update_terminal_search_needle(runtime_session_id, &needle) {
                    let _ = self.perform_terminal_search_binding_action(
                        runtime_session_id,
                        &format!("search:{needle}"),
                        cx,
                    );
                }
            }
            InputEvent::PressEnter { shift, .. } => {
                let action = if *shift {
                    "navigate_search:previous"
                } else {
                    "navigate_search:next"
                };
                let _ = self.perform_terminal_search_binding_action(runtime_session_id, action, cx);
            }
            InputEvent::Focus | InputEvent::Blur => {}
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (runtime_session_id, input, event, cx);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn close_terminal_search(
        &mut self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let _ = self.perform_terminal_search_binding_action(runtime_session_id, "end_search", cx);
        let mut closed = false;
        for osc_states in [
            &mut self.agents_terminal_runtime_osc_states,
            &mut self.command_terminal_runtime_osc_states,
        ] {
            if let Some(state) = osc_states.get_mut(&runtime_session_id) {
                closed |= state.search.take().is_some();
            }
        }
        if !closed {
            return;
        }
        let companion_slot_id = self
            .project_editor_companion_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.current_project_editor_companion_terminal_body_mount_slots()
                    .into_iter()
                    .find(|slot_id| {
                        self.agents_gpui_engine_terminals
                            .get(&slot_id.session_id)
                            .is_some_and(|record| record.runtime_session_id == runtime_session_id)
                    })
            });
        if let Some(slot_id) = companion_slot_id {
            self.focus_project_editor_companion_terminal_session(
                slot_id.mode,
                slot_id.session_id,
                window,
                cx,
            );
        } else if let Some(slot_id) = self
            .agents_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.agents_gpui_engine_terminals
                    .iter()
                    .find(|(_, record)| record.runtime_session_id == runtime_session_id)
                    .and_then(|(session_id, _)| {
                        let pane_id = self.agents_workspace.pane_id_for_session(*session_id)?;
                        Some(AgentsTerminalBodyMountSlotId {
                            pane_id,
                            session_id: *session_id,
                        })
                    })
            })
        {
            self.focus_agents_terminal_mount_slot(slot_id, window, cx);
        } else if let Some(slot_id) = self
            .command_terminal_ghostty_surfaces
            .iter()
            .find_map(|(slot_id, surface)| {
                (surface.runtime_session_id() == runtime_session_id).then_some(*slot_id)
            })
            .or_else(|| {
                self.command_gpui_engine_terminals
                    .iter()
                    .find(|(_, record)| record.runtime_session_id == runtime_session_id)
                    .and_then(|(session_id, _)| {
                        self.command_pane
                            .flat_tab_ids()
                            .into_iter()
                            .find(|(_, tab_session_id)| tab_session_id == session_id)
                            .map(|(group_id, session_id)| CommandTerminalBodyMountSlotId {
                                group_id,
                                session_id,
                            })
                    })
            })
        {
            self.focus_command_terminal_mount_slot(slot_id, window, cx);
        }
        cx.notify();
    }

    /// Whether any live terminal search input currently holds GPUI keyboard
    /// focus. Shell focus stays on the terminal pane while the search bar is
    /// open, so terminal key routing that is derived from shell focus must
    /// consult this before treating a keystroke as terminal input.
    pub(crate) fn terminal_search_input_owns_keyboard_focus(&self, window: &Window, cx: &App) -> bool {
        self.terminal_search_inputs
            .values()
            .any(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Keeps one live search input per terminal with an active Ghostty search
    /// state, mirrors Ghostty-provided needles into the field, and applies a
    /// pending open-focus so Cmd+F immediately types into the bar like macOS.
    pub(crate) fn sync_terminal_search_inputs(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let mut active_needles: HashMap<AgentsTerminalRuntimeSessionId, String> = HashMap::new();
        for (runtime_session_id, state) in self
            .agents_terminal_runtime_osc_states
            .iter()
            .chain(self.command_terminal_runtime_osc_states.iter())
        {
            if let Some(search) = &state.search {
                active_needles.insert(*runtime_session_id, search.needle.clone());
            }
        }
        self.terminal_search_inputs
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        self.terminal_search_input_subscriptions
            .retain(|runtime_session_id, _| active_needles.contains_key(runtime_session_id));
        for (runtime_session_id, needle) in active_needles {
            let input = match self.terminal_search_inputs.get(&runtime_session_id) {
                Some(input) => input.clone(),
                None => {
                    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
                    let subscription = cx.subscribe(
                        &input,
                        move |this: &mut Self, input, event: &InputEvent, cx| {
                            this.handle_terminal_search_input_event(
                                runtime_session_id,
                                &input,
                                event,
                                cx,
                            );
                        },
                    );
                    self.terminal_search_inputs
                        .insert(runtime_session_id, input.clone());
                    self.terminal_search_input_subscriptions
                        .insert(runtime_session_id, subscription);
                    input
                }
            };
            if input.read(cx).value().as_ref() != needle {
                input.update(cx, |input, cx| input.set_value(needle, window, cx));
            }
        }
        if let Some(runtime_session_id) = self.terminal_search_focus_pending.take() {
            if let Some(input) = self
                .terminal_search_inputs
                .get(&runtime_session_id)
                .cloned()
            {
                #[cfg(target_os = "macos")]
                self.begin_programmatic_focus();
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                cef::focus_gpui_root_view(self.parent_ns_view);
                input.update(cx, |input, cx| input.focus(window, cx));
                #[cfg(target_os = "macos")]
                self.end_programmatic_focus();
            }
        }
    }

    /*
    CDXC:SessionChatPromptQueue 2026-08-21:
    The terminal view's "Queued: N" control is the leading item of the pane's
    own tab bar — existing chrome, a normal sibling frame, drawn beside the tab
    strip rather than over anything.

    It deliberately does NOT float over the terminal the way the web host's does:
    GPUI cannot paint above a mounted Ghostty/CEF body, and solving that with a
    transparent overlay or hit-test routing is exactly what this repo's native
    layout discipline forbids. A chrome ROW between the tab bar and the body (the
    search bar's slot) was the other candidate and was rejected: it would resize
    the Ghostty surface — reflowing the user's scrollback — every time a queue
    filled or drained. The tab bar has a fixed height, so appearing and
    disappearing here cannot touch terminal geometry at all.
    */
    pub(crate) fn render_agents_terminal_queued_prompts_chip(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        // Chat and Find own this pane's body when they are on, and the chat view
        // renders the queue rows themselves, so the chip has nothing to add.
        if self.agents_chat_mode_sessions.contains(&session_id)
            || self.agents_find_mode_sessions.contains(&session_id)
        {
            return None;
        }
        let counts = self
            .session_chat_queued_counts
            .get(&session_id)
            .copied()
            .unwrap_or_default();
        if counts.total == 0 {
            return None;
        }
        let count = counts.total;
        /*
        CDXC:SessionChatPromptQueue 2026-08-21-b:
        A `failed` row holds the whole queue until the user retries or deletes
        it, so the chip's dot turns the sidebar's error red instead of the
        waiting yellow. Only the dot's colour changes — every box property below
        is identical either way, so the chip cannot resize the tab bar and
        therefore cannot touch the Ghostty surface's geometry.
        */
        let dot_color = if counts.failed > 0 {
            terminal_queued_prompts_failed_dot_color()
        } else {
            terminal_queued_prompts_dot_color()
        };
        let element_id_suffix = format!("agents-{}-{}", leaf.pane_id.0, session_id.0);
        Some(
            h_flex()
                .id(format!(
                    "ghostex-gpui-terminal-queued-prompts-{element_id_suffix}"
                ))
                .flex_shrink_0()
                .items_center()
                .gap(px(5.0))
                .ml(px(6.0))
                .mr(px(2.0))
                .h(px(TERMINAL_QUEUED_PROMPTS_CHIP_HEIGHT))
                .px(px(7.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(terminal_queued_prompts_border_color())
                .bg(terminal_queued_prompts_background_color())
                .text_size(px(11.0))
                .text_color(terminal_queued_prompts_text_color())
                .cursor_default()
                .hover(|this| this.bg(terminal_queued_prompts_hover_color()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.handoff_agents_session_chat_mode(session_id, cx);
                    }),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .size(px(6.0))
                        .rounded_full()
                        .bg(dot_color),
                )
                .child(div().child(format!("Queued: {count}")))
                .into_any_element(),
        )
    }

    pub(crate) fn render_agents_terminal_search_bar(
        &self,
        leaf: &WorkspaceLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let slot_id = self
            .agents_workspace
            .terminal_body_mount_candidate(leaf)
            .mount_slot_id()?;
        let runtime_session_id = self.agents_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .agents_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("agents-{}-{}", slot_id.pane_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn agents_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self.agents_terminal_ghostty_surfaces.get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    pub(crate) fn render_command_terminal_search_bar(
        &self,
        leaf: &CommandPaneLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = leaf.tab_group.active_session_id()?;
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id: leaf.group_id,
            session_id,
        };
        let runtime_session_id = self.command_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .command_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!("command-{}-{}", slot_id.group_id.0, slot_id.session_id.0),
            cx,
        ))
    }

    pub(crate) fn command_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: CommandTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.command_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self.command_terminal_ghostty_surfaces.get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    pub(crate) fn render_project_editor_companion_terminal_search_bar(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let runtime_session_id =
            self.project_editor_companion_terminal_search_bar_runtime_session_id(slot_id)?;
        let search = self
            .agents_terminal_runtime_osc_states
            .get(&runtime_session_id)?
            .search
            .clone()?;
        Some(self.render_terminal_search_bar(
            runtime_session_id,
            &search,
            format!(
                "companion-{}-{}",
                slot_id.mode.element_slug(),
                slot_id.session_id.0
            ),
            cx,
        ))
    }

    pub(crate) fn project_editor_companion_terminal_search_bar_runtime_session_id(
        &self,
        slot_id: ProjectEditorCompanionTerminalBodyMountSlotId,
    ) -> Option<AgentsTerminalRuntimeSessionId> {
        if let Some(record) = self.agents_gpui_engine_terminals.get(&slot_id.session_id) {
            return Some(record.runtime_session_id);
        }
        #[cfg(target_os = "macos")]
        {
            let surface = self
                .project_editor_companion_terminal_ghostty_surfaces
                .get(&slot_id)?;
            return (surface.mount_slot_id() == slot_id).then(|| surface.runtime_session_id());
        }
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// The GPUI search bar is a normal-layout chrome row above the terminal
    /// body (same discipline as the close-confirm banner) because GPUI cannot
    /// float elements over the mounted AppKit Ghostty view. Contents are
    /// right-aligned to echo the macOS floating bar's top-right placement.
    pub(crate) fn render_terminal_search_bar(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        search: &GpuiTerminalSearchState,
        element_id_suffix: String,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let count_label = terminal_search_count_label(search);
        let input = self
            .terminal_search_inputs
            .get(&runtime_session_id)
            .cloned();

        h_flex()
            .id(format!(
                "ghostex-gpui-terminal-search-row-{element_id_suffix}"
            ))
            .flex_shrink_0()
            .w_full()
            .h(px(FIND_BAR_HEIGHT))
            .items_center()
            .justify_end()
            .pl(px(8.0))
            .bg(terminal_search_bar_row_color())
            .border_b_1()
            .border_color(terminal_search_bar_divider_color())
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                #[cfg(target_os = "macos")]
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        this.close_terminal_search(runtime_session_id, window, cx);
                    }
                    "up" => {
                        cx.stop_propagation();
                        let _ = this.perform_terminal_search_binding_action(
                            runtime_session_id,
                            "navigate_search:previous",
                            cx,
                        );
                    }
                    "down" => {
                        cx.stop_propagation();
                        let _ = this.perform_terminal_search_binding_action(
                            runtime_session_id,
                            "navigate_search:next",
                            cx,
                        );
                    }
                    _ => {}
                }
                #[cfg(not(target_os = "macos"))]
                let _ = (this, event, window, cx);
            }))
            .child(
                h_flex()
                    .id(format!(
                        "ghostex-gpui-terminal-search-bar-{element_id_suffix}"
                    ))
                    .w(px(300.0))
                    .max_w_full()
                    .h_full()
                    .items_center()
                    .gap(px(4.0))
                    .border_l_1()
                    .border_color(terminal_search_bar_border_color())
                    .bg(terminal_search_bar_background_color())
                    .pl(px(9.0))
                    .when_some(input, |this, input| {
                        this.child(
                            div().flex_1().min_w_0().overflow_hidden().child(
                                Input::new(&input)
                                    .with_size(ComponentSize::XSmall)
                                    .appearance(false)
                                    .bordered(false)
                                    .focus_bordered(false)
                                    .w_full()
                                    .px(px(0.0))
                                    .py(px(0.0))
                                    .text_size(px(13.0))
                                    .text_color(terminal_search_bar_text_color()),
                            ),
                        )
                    })
                    .when(!count_label.is_empty(), |this| {
                        this.child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(11.0))
                                .text_color(terminal_search_bar_count_color())
                                .child(count_label),
                        )
                    })
                    .child(
                        h_flex()
                            .h_full()
                            .flex_shrink_0()
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-prev-{element_id_suffix}"),
                                "↑",
                                FIND_BAR_NAV_BUTTON_WIDTH,
                                move |this, _window, cx| {
                                    #[cfg(target_os = "macos")]
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:previous",
                                        cx,
                                    );
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, cx);
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-next-{element_id_suffix}"),
                                "↓",
                                FIND_BAR_NAV_BUTTON_WIDTH,
                                move |this, _window, cx| {
                                    #[cfg(target_os = "macos")]
                                    let _ = this.perform_terminal_search_binding_action(
                                        runtime_session_id,
                                        "navigate_search:next",
                                        cx,
                                    );
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, cx);
                                },
                                cx,
                            ))
                            .child(self.render_terminal_search_button(
                                format!("ghostex-gpui-terminal-search-close-{element_id_suffix}"),
                                "✕",
                                FIND_BAR_CLOSE_BUTTON_WIDTH,
                                move |this, window, cx| {
                                    #[cfg(target_os = "macos")]
                                    this.close_terminal_search(runtime_session_id, window, cx);
                                    #[cfg(not(target_os = "macos"))]
                                    let _ = (this, window, cx);
                                },
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_terminal_search_button(
        &self,
        id: String,
        glyph: &'static str,
        width: f32,
        on_click: impl Fn(&mut Self, &mut Window, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .h_full()
            .w(px(width))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(terminal_search_bar_border_color())
            .text_size(px(12.0))
            .line_height(px(20.0))
            .text_color(terminal_search_bar_button_color())
            .bg(terminal_search_bar_button_background_color())
            .cursor_default()
            .hover(|this| this.bg(terminal_search_bar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_click(this, window, cx);
                }),
            )
            .child(div().child(glyph))
            .into_any_element()
    }

    /// Fork/Reload for Agents terminals follow macOS ownership: Rust resolves
    /// the mapped gxserver identity and the sidebar runtime commits the
    /// gxserver mutation (`/api/forkSession` or the sleep→wake full reload).
    /// Unmapped local placeholder tabs have no gxserver identity and no-op.
    pub(crate) fn dispatch_gpui_workspace_terminal_runtime_action(
        &mut self,
        action: &str,
        shell_session_id: TerminalSessionId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(key) = self
            .local_workspace_session_mappings
            .iter()
            .find_map(|(key, mapped)| (*mapped == shell_session_id).then(|| key.clone()))
        else {
            return false;
        };
        self.dispatch_gpui_workspace_session_key_runtime_action(action, &key, cx)
    }

    /// Same sidebar-runtime lifecycle route addressed by gxserver identity, for
    /// sessions that exist in the daemon presentation without a mounted pane in
    /// this window.
    pub(crate) fn dispatch_gpui_workspace_session_key_runtime_action(
        &mut self,
        action: &str,
        key: &GpuiLocalWorkspaceSessionKey,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": action,
            "projectId": key.project_id,
            "sessionId": key.session_id,
            "type": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_WORKSPACE_TERMINAL_RUNTIME_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_workspace_terminal_runtime_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn focused_agents_workspace_shell_session_id(&self) -> Option<TerminalSessionId> {
        if self.active_mode != TitlebarMode::Agents {
            return None;
        }
        let ShellFocusTarget::AgentsPane(pane_id) = self.shell_focus else {
            return None;
        };
        self.agents_workspace
            .find_leaf(pane_id)
            .map(|leaf| leaf.tab_group.active_tab)
    }

    /// The Agents workspace session behind the focused terminal, whether it is
    /// focused in the Agents view or in a project-editor companion pane (top
    /// or bottom split). Companion panes display Agents workspace sessions, so
    /// both resolve into the same shell-session id space; session-scoped
    /// agent actions (Rename, Delayed Send, Close After Done, Prompts, the
    /// overlay cluster) treat the two focus surfaces identically.
    pub(crate) fn focused_agents_or_companion_shell_session_id(&self) -> Option<TerminalSessionId> {
        if let Some(session_id) = self.focused_agents_workspace_shell_session_id() {
            return Some(session_id);
        }
        match self.focused_terminal_text_mount_target() {
            Some(FocusedTerminalTextMountTarget::ProjectEditorCompanion(slot_id)) => {
                Some(slot_id.session_id)
            }
            _ => None,
        }
    }

    /// The sidebar projection is the title authority for Agents tabs. It owns
    /// placeholder, generated, user, and terminal-title trust rules; using a
    /// second live OSC source here lets WSL's `user@host: /path` shell title
    /// hide title generation that the sidebar has already projected.
    pub(crate) fn agents_workspace_tab_display_title(&self, session_id: TerminalSessionId) -> String {
        self.agents_workspace
            .session(session_id)
            .map(|session| session.title.clone())
            .unwrap_or_else(|| "Terminal Session".to_string())
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_ghostty_surface_focus(&mut self) {
        self.sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(false);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_agents_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        /*
        CDXC:GPUTerminalGhosttySurfaceFocus 2026-06-22-22:59:
        Apply focus only to already-mounted real Agents Ghostty surfaces. The focused surface is the current shell-focused visible Running Agents mount slot; all other mounted slots receive unfocused state. The shared Ghostty app focus means only "GPUI currently has a focused terminal surface" and deliberately does not infer NSApp activation or route keyboard/mouse input.

        CDXC:GPUTerminalAppKitFocus 2026-06-22-23:11:
        The same focused mounted Agents slot must also hand first responder to the exact App-owned host NSView that backs its real Ghostty surface. Store only runtime slot plus host identity to avoid repeated `makeFirstResponder` calls during render sync, but allow terminal-body clicks to force one handoff even when shell focus already points at the same slot.
        */
        let mounted_slot_ids = self
            .agents_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focus_states = agents_terminal_surface_focus_states_for_slots(
            self.active_mode,
            self.shell_focus,
            &self.agents_workspace,
            &mounted_slot_ids,
        );
        let app_has_focused_terminal_surface =
            focus_states.iter().any(|(_slot_id, focused)| *focused);
        let focused_mounted_slot_id = focus_states
            .iter()
            .find_map(|(slot_id, focused)| (*focused).then_some(*slot_id));

        for (slot_id, focused) in focus_states {
            if let Some(surface) = self.agents_terminal_ghostty_surfaces.get_mut(&slot_id) {
                surface.set_focus(focused);
            }
        }

        if let Some(app) = self.agents_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_mounted_slot_id.and_then(|slot_id| {
            if !self.agents_terminal_ghostty_surfaces.contains_key(&slot_id) {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.agents_terminal_host_native_views.get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.agents_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.agents_terminal_appkit_focused_host =
                next_appkit_focus_identity.and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.agents_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.agents_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drop_command_terminal_ghostty_surface_before_host_detach(
        &mut self,
        commands: &[terminal_surface_host::NativeTerminalSurfaceHostCommand<
            CommandTerminalBodyMountSlotId,
        >],
    ) {
        for command in commands {
            let terminal_surface_host::NativeTerminalSurfaceHostCommand::HideAndDetach { plan } =
                *command
            else {
                continue;
            };

            if park_command_terminal_runtime_owner_before_host_detach(
                &self.command_pane,
                &mut self.command_terminal_parked_runtime_owners,
                &mut self.command_terminal_host_native_views,
                &mut self.command_terminal_ghostty_surfaces,
                plan,
            ) {
                continue;
            }
            self.command_terminal_ghostty_surfaces.remove(&plan.slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&plan.slot_id),
                false,
            );
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surfaces(&mut self, cx: &mut gpui::Context<Self>) {
        let host_slot_ids = self
            .command_terminal_host_native_views
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let request_slot_ids = self
            .command_terminal_ghostty_surface_config_requests
            .keys()
            .copied()
            .collect::<HashSet<_>>();
        let stale_surface_slot_ids = self
            .command_terminal_ghostty_surfaces
            .keys()
            .copied()
            .filter(|slot_id| {
                !host_slot_ids.contains(slot_id) || !request_slot_ids.contains(slot_id)
            })
            .collect::<Vec<_>>();

        for slot_id in stale_surface_slot_ids {
            self.command_terminal_ghostty_surfaces.remove(&slot_id);
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&slot_id),
                false,
            );
        }

        self.sync_command_terminal_ghostty_surface_focus();

        if self.command_terminal_host_native_views.is_empty() {
            return;
        }

        if self.command_terminal_ghostty_app.is_none() {
            let Ok(app) = terminal_ghostty_surface::GhosttyAppOwner::new() else {
                self.command_terminal_ghostty_surfaces.clear();
                for host_view in self.command_terminal_host_native_views.values() {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        Some(host_view),
                        false,
                    );
                }
                return;
            };
            self.command_terminal_ghostty_app = Some(app);
        }

        let host_plans = self
            .command_terminal_host_native_views
            .iter()
            .map(|(slot_id, host_view)| (*slot_id, host_view.attachment_plan()))
            .collect::<Vec<_>>();

        for (slot_id, plan) in host_plans {
            let runtime_session_id = command_terminal_runtime_session_id(slot_id);
            let Some(request) = self
                .command_terminal_ghostty_surface_config_requests
                .get(&slot_id)
                .cloned()
            else {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.command_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            };

            if self
                .command_terminal_ghostty_surfaces
                .get(&slot_id)
                .is_some_and(|surface| {
                    surface.mount_slot_id() != slot_id
                        || surface.runtime_session_id() != runtime_session_id
                })
            {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
            }

            if !self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                let Some(app) = self.command_terminal_ghostty_app.as_ref() else {
                    return;
                };
                let Ok(surface) = terminal_ghostty_surface::GhosttySurfaceOwner::new(
                    app,
                    slot_id,
                    runtime_session_id,
                    &request,
                ) else {
                    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                        self.command_terminal_host_native_views.get(&slot_id),
                        false,
                    );
                    continue;
                };
                self.command_terminal_ghostty_surfaces
                    .insert(slot_id, surface);
            }

            let update_failed = self
                .command_terminal_ghostty_surfaces
                .get_mut(&slot_id)
                .is_some_and(|surface| {
                    surface
                        .update_content_scale_and_size(plan.bounds, request.scale_factor())
                        .is_err()
                });
            if update_failed {
                self.command_terminal_ghostty_surfaces.remove(&slot_id);
                terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                    self.command_terminal_host_native_views.get(&slot_id),
                    false,
                );
                continue;
            }

            let surface_mounted = self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id);
            if surface_mounted {
                if let (Some(host_view), Some(surface)) = (
                    self.command_terminal_host_native_views.get(&slot_id),
                    self.command_terminal_ghostty_surfaces.get(&slot_id),
                ) {
                    /*
                    CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
                    Command-pane terminals use the same exact host-view key bridge as Agents terminals, but registration is scoped to command mount slots and their own Ghostty app/surface map so command keys cannot fall through to Agents surfaces or shell placeholders.
                    */
                    terminal_ghostty_surface::register_native_key_target(
                        host_view.native_view_handle(),
                        surface,
                    );
                }
            }
            terminal_native_view::set_app_owned_terminal_host_native_view_visible(
                self.command_terminal_host_native_views.get(&slot_id),
                surface_mounted,
            );
        }

        self.sync_command_terminal_ghostty_surface_focus();

        if let Some(app) = self.command_terminal_ghostty_app.as_ref() {
            if !self.command_terminal_ghostty_surfaces.is_empty() {
                app.tick_if_woken();
                app.tick();
                self.drain_command_terminal_runtime_clipboard_requests(cx);
            };
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn drain_command_terminal_runtime_clipboard_requests(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUITerminalClipboard 2026-06-23-19:07:
        Command runtime clipboard handoff mirrors Agents ownership rules: only exact command mount keys from the current surface map can authorize app-thread standard clipboard access for queued owner-local Ghostty requests. Focus is never requester identity, reads stay explicit-string-only, writes forward only runtime-provided text, and stale/missing surfaces naturally keep their queued operations unreachable.

        CDXC:GPUITerminalImagePaste 2026-06-27-10:28:
        Command-pane runtime clipboard reads use the same runtime previewable-image setting and normalization as Agents terminals, keeping image Markdown parity per mounted owner without focused-surface requester fallback.
        */
        let paste_previewable_images_enabled =
            shared_settings::shared_sidebar_settings_snapshot().terminal_paste_previewable_images();
        let slot_ids = terminal_runtime_clipboard_authorized_mounted_slot_ids(
            self.command_terminal_ghostty_surfaces
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            &self.command_terminal_ghostty_surfaces,
        );

        let mut runtime_osc_state_changed = false;
        let mut terminal_link_requests = Vec::new();
        for slot_id in slot_ids {
            let Some(surface) = self.command_terminal_ghostty_surfaces.get(&slot_id) else {
                continue;
            };
            surface.drain_runtime_clipboard_requests(
                true,
                || {
                    terminal_runtime_clipboard_read_text(
                        || cx.read_from_clipboard(),
                        paste_previewable_images_enabled,
                    )
                },
                |text| {
                    terminal_runtime_clipboard_write_standard_text(text, |item| {
                        cx.write_to_clipboard(item);
                    });
                },
            );
            let action_events = surface.drain_runtime_action_events();
            if !action_events.is_empty() {
                let runtime_session_id = surface.runtime_session_id();
                terminal_link_requests.extend(action_events.iter().filter_map(|event| {
                    let terminal_ghostty_surface::GhosttyRuntimeActionEvent::OpenUrl { url } =
                        event
                    else {
                        return None;
                    };
                    Some((runtime_session_id, url.clone()))
                }));
                if action_events.iter().any(|event| {
                    matches!(
                        event,
                        terminal_ghostty_surface::GhosttyRuntimeActionEvent::StartSearch { .. }
                    )
                }) {
                    self.terminal_search_focus_pending = Some(surface.runtime_session_id());
                }
                runtime_osc_state_changed |= apply_gpui_terminal_runtime_action_events(
                    &mut self.command_terminal_runtime_osc_states,
                    runtime_session_id,
                    action_events,
                );
            }
        }
        for (runtime_session_id, url) in terminal_link_requests {
            let working_directory = self
                .command_terminal_runtime_osc_states
                .get(&runtime_session_id)
                .and_then(|state| state.pwd.clone());
            self.open_gpui_engine_terminal_action_url(&url, working_directory.as_deref(), cx);
        }
        if !self.command_terminal_runtime_osc_states.is_empty() {
            let live_runtime_session_ids = self
                .command_terminal_ghostty_surfaces
                .values()
                .map(|surface| surface.runtime_session_id())
                .chain(
                    self.command_gpui_engine_terminals
                        .values()
                        .map(|record| record.runtime_session_id),
                )
                .collect::<HashSet<_>>();
            self.command_terminal_runtime_osc_states
                .retain(|runtime_session_id, _| {
                    live_runtime_session_ids.contains(runtime_session_id)
                });
        }
        if runtime_osc_state_changed {
            cx.notify();
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surface_focus(&mut self) {
        self.sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(false);
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn sync_command_terminal_ghostty_surface_focus_with_appkit_handoff(
        &mut self,
        force_terminal_appkit_focus_handoff: bool,
    ) {
        let mounted_slot_ids = self
            .command_terminal_ghostty_surfaces
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let focus_states = command_terminal_surface_focus_states_for_slots(
            self.shell_focus,
            &self.command_pane,
            &mounted_slot_ids,
        );
        let app_has_focused_terminal_surface =
            focus_states.iter().any(|(_slot_id, focused)| *focused);
        let focused_mounted_slot_id = focus_states
            .iter()
            .find_map(|(slot_id, focused)| (*focused).then_some(*slot_id));

        for (slot_id, focused) in focus_states {
            if let Some(surface) = self.command_terminal_ghostty_surfaces.get_mut(&slot_id) {
                surface.set_focus(focused);
            }
        }

        if let Some(app) = self.command_terminal_ghostty_app.as_mut() {
            app.set_focus(app_has_focused_terminal_surface);
        }

        let next_appkit_focus_identity = focused_mounted_slot_id.and_then(|slot_id| {
            if !self
                .command_terminal_ghostty_surfaces
                .contains_key(&slot_id)
            {
                return None;
            }
            terminal_native_view::app_owned_terminal_host_focus_identity(
                self.command_terminal_host_native_views.get(&slot_id),
            )
        });
        if terminal_native_view::app_owned_terminal_host_focus_should_execute(
            self.command_terminal_appkit_focused_host,
            next_appkit_focus_identity,
            force_terminal_appkit_focus_handoff,
        ) {
            self.command_terminal_appkit_focused_host =
                next_appkit_focus_identity.and_then(|focus_identity| {
                    terminal_native_view::focus_app_owned_terminal_host_native_view(
                        self.command_terminal_host_native_views
                            .get(&focus_identity.slot_id()),
                    )
                });
        } else {
            self.command_terminal_appkit_focused_host = next_appkit_focus_identity;
        }
    }

    pub(crate) fn record_command_group_layout_bounds(
        &mut self,
        group_id: CommandPaneGroupId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.command_group_layout_bounds.insert(group_id, bounds);
        }
    }

    pub(crate) fn record_command_pane_layout_bounds(&mut self, child_bounds: &[Bounds<Pixels>]) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.command_pane_layout_bounds = Some(bounds);
        }
    }

    pub(crate) fn record_browser_leaf_layout_bounds(
        &mut self,
        pane_id: BrowserPaneId,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.browser_leaf_layout_bounds.insert(pane_id, bounds);
        }
    }

    pub(crate) fn record_project_editor_surface_layout_bounds(
        &mut self,
        mode: TitlebarMode,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.project_editor_surface_layout_bounds =
                Some(ProjectEditorFocusBounds { mode, bounds });
        }
    }

    pub(crate) fn record_project_editor_companion_layout_bounds(
        &mut self,
        mode: TitlebarMode,
        child_bounds: &[Bounds<Pixels>],
    ) {
        if let Some(bounds) = pane_focus_bounds_from_child_bounds(child_bounds) {
            self.project_editor_companion_layout_bounds =
                Some(ProjectEditorFocusBounds { mode, bounds });
        }
    }

    pub(crate) fn initialize_cef(&mut self, cx: &mut gpui::Context<Self>) {
        if self.sidebar.is_some() {
            return;
        }

        cef::initialize(cx).expect("failed to initialize CEF");
        if !cef::context_initialized() {
            if self.cef_context_initialization_waiting {
                return;
            }
            self.cef_context_initialization_waiting = true;
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(25))
                        .await;
                    if cef::context_initialized() {
                        let _ = this.update(cx, |this, cx| {
                            this.cef_context_initialization_waiting = false;
                            this.initialize_cef(cx);
                        });
                        break;
                    }
                }
            })
            .detach();
            return;
        }
        let parent_ns_view = self.parent_ns_view;
        let sidebar_url = self.sidebar_url.clone();
        let sidebar_bridge_event_handler = self.sidebar_bridge_event_handler(cx);
        let app_modal_host_bridge_event_handler = self.app_modal_host_bridge_event_handler(cx);
        let sidebar_runtime_settings = self.sidebar_runtime_settings_snapshot.clone();
        let sidebar_gxserver_bootstrap = self.sidebar_gxserver_bootstrap.clone();
        let sidebar_visible = gpui_sidebar_chrome_visible(self.sidebar_collapsed);
        match CefSurface::try_new(
            "gpui-sidebar".to_string(),
            parent_ns_view,
            sidebar_url,
            "gpui-sidebar".to_string(),
            sidebar_cef_prepaint_background_color(),
            false,
            titlebar_background(),
            None,
            sidebar_visible,
            None,
            None,
            None,
            Some(sidebar_runtime_settings),
            sidebar_gxserver_bootstrap,
            Some(sidebar_bridge_event_handler),
            None,
            None,
            Some(cef::AppModalHostBridgeSurface::Sidebar),
            Some(app_modal_host_bridge_event_handler),
            None,
            cx,
        ) {
            Ok(sidebar) => {
                /*
                CDXC:GPUISidebarPointerTracking 2026-08-02:
                Hand the sidebar's CEF child view to the AppKit sendEvent
                observer so pointer crossings of its frame, and mouse-downs
                outside it, become the page's hover-suppression and
                context-menu-dismissal signals.
                */
                #[cfg(target_os = "macos")]
                if let Some(native_view) =
                    sidebar.read(cx).native_view_for_sidebar_pointer_tracking()
                {
                    cef::set_sidebar_pointer_tracking_view(native_view);
                }
                self.sidebar = Some(sidebar);
            }
            Err(error) => {
                // The sidebar profile uses the pre-initialized global app-ui
                // context, so a creation failure here is unexpected. Retry
                // once after CEF has had time to settle; on a second failure
                // keep the app alive without the sidebar instead of the
                // previous process abort
                // (CDXC:GPUICefBrowserCreateFallible 2026-07-11).
                support_logs::append(
                    support_logs::GpuiSupportLog::CrashReports,
                    "gpui.cefSurface.createFailed",
                    serde_json::json!({
                        "surface": "sidebar",
                        "retryScheduled": !self.cef_sidebar_creation_retried,
                        "error": error,
                    }),
                );
                if !self.cef_sidebar_creation_retried {
                    self.cef_sidebar_creation_retried = true;
                    cx.spawn(async move |this, cx| {
                        cx.background_executor()
                            .timer(Duration::from_millis(750))
                            .await;
                        let _ = this.update(cx, |this, cx| {
                            if this.sidebar.is_none() {
                                this.initialize_cef(cx);
                            }
                        });
                    })
                    .detach();
                }
                return;
            }
        }
        self.ensure_active_browser_surface(cx);
        self.ensure_project_workarea_runtime_cef_surfaces_for_current_context(cx);
        self.update_active_mode_cef_child_visibility(cx);
        // First-run onboarding may open the CEF app-modal host. Start it only
        // after the required runtime and initial sidebar surface are ready;
        // macOS release first launch can spend time in the native component
        // window before CEF is available.
        self.start_gpui_first_run_onboarding(cx);
        cx.notify();
    }

    pub(crate) fn update_active_mode_cef_child_visibility(&mut self, cx: &mut gpui::Context<Self>) {
        /*
        CDXC:GPUIProjectEditor 2026-06-22-05:49:
        Browser tab CEF surfaces may show only in Browser mode. Kanban, Automate, and Manage use the separate project-workarea CEF surface map and visibility gate, so Browser CEF child views must still hide under Source, Kanban, Automate, Manage, and Agents instead of sitting underneath non-browser editor panes.

        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Per-tab Browser CEF entities must never visually overlap each other or other project-editor modes. Visibility is derived from Browser mode plus rendered Browser leaves' active loaded tab ids, so inactive tab views and all Browser views outside Browser mode are hidden at the native child-view boundary.

        CDXC:GPUIProjectEditorLifecycle 2026-06-22-07:30:
        Browser project-editor sleep is shell-level for this slice: sleeping hides all Browser CEF child views but does not delete Browser tab metadata or CEF surface entities. Waking Browser mode uses the existing selected-tab sync path plus rendered-leaf visibility so existing active loaded surfaces can show again.

        CDXC:GPUIBrowserDragDrop 2026-06-22-07:41:
        Browser tab drags must hide native CEF child surfaces for the whole drag, not only while hovering a valid drop target. GPUI keeps the drag feedback in normal tab-strip layout, while eligible rendered Browser leaf surfaces are restored through this same visibility gate when the drag drops or is canceled.

        CDXC:GPUICommandPaneDrag 2026-06-22-08:01:
        Command-pane tab drags can run while Browser mode remains active underneath the command panel, so any active command tab drag must hide every Browser CEF child surface through the same visibility gate used by Browser tab drags. Drop and root mouse-up cancellation restore eligible rendered Browser leaf surfaces by clearing the command drag flag, without overlays, hit-test rerouting, or changing command-only drag/drop semantics.

        CDXC:GPUIBrowserSplits 2026-06-22-09:55:
        Browser split parity requires all rendered Browser leaves to show their own active loaded CEF surface when that surface already exists. Visibility is a set of BrowserTabIds derived from rendered leaves; this keeps Browser sleep, non-Browser modes, Browser tab drags, and command-tab drags hiding every surface while avoiding CEF creation for restored or inactive tabs that have not been materialized yet.

        CDXC:GPUIBrowserLifecycle 2026-06-23-11:32:
        Runtime visibility now routes through BrowserRuntimeSurfacePolicy so the hide/hold/restored-placeholder decision is centralized and reviewable. The sync loop only toggles existing tab-owned CEF entities; it never creates restored loaded tab surfaces or tears down hidden ones.

        CDXC:GPUIBrowserLifecycle 2026-06-23-14:30:
        Keep this loop limited to set_visible on existing Browser surfaces. Browser sleep, non-Browser mode, Browser tab drags, and command-tab drags hide-and-hold; deeper CEF suspend/teardown and restored-surface recreation remain deferred decisions, not fallback behavior in the visibility path.

        CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-24-10:12:
        The same active-mode visibility pass also hides or shows already-owned Source/Kanban/Automate/Manage runtime CEF child views. It does not create project-workarea surfaces, issue URLs, use temporary pages, use WKWebView/WebKit, or allow hidden CEF views to sit under placeholders.
        */
        let visible_tab_ids = browser_runtime_visible_surface_tab_ids(
            self.browser_runtime_lifecycle_input(),
            &self.browser_tabs,
            self.browser_surfaces.keys().copied().collect::<Vec<_>>(),
        );

        for (tab_id, surface) in &self.browser_surfaces {
            let surface_visible = visible_tab_ids.contains(tab_id);
            surface.update(cx, |surface, _| {
                surface.set_visible(surface_visible);
            });
        }
        self.update_project_workarea_runtime_cef_surface_visibility(cx);
        /*
        CDXC:GPUISessionChatSurface 2026-08-19:
        Session Chat is a CEF child view gated on exactly the same inputs as
        the Browser and project-workarea surfaces: active mode, mode
        wakefulness, companion visibility, and the tab-drag flags. Every mode
        switch, drag, sleep, and companion mutation already re-runs this pass,
        so the chat gate belongs here rather than being re-added by hand at
        each call site. Sites that set `active_mode` and then only synced
        Browser surfaces (terminal link opens through
        `open_browser_url_from_renderer_command`, for example) used to leave
        the Agents chat child painted over the new workarea.
        */
        self.reconcile_agents_pane_surfaces(cx);
    }
}
