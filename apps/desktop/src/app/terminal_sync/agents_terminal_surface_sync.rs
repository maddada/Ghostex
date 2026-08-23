// C1 wave-4 re-cluster: further split out of app/terminal_sync.rs (~5,603
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: native Agents terminal surface host reconciliation: startup host config, ghostty surface spawn/promote/reattach, and applying startup results.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
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
    #[allow(dead_code)] // no live caller: the app-owned terminal startup-host reconcile pipeline is not driven any more (agents terminals mount through the surface-host path)
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
    #[allow(dead_code)] // no live caller: the app-owned terminal startup-host reconcile pipeline is not driven any more (agents terminals mount through the surface-host path)
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
    #[allow(dead_code)] // no live caller: the app-owned terminal startup-host reconcile pipeline is not driven any more (agents terminals mount through the surface-host path)
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

}
