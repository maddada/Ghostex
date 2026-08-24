// C1 wave-3 re-cluster: parked Agents/Command terminal runtime owner reattach plans and the park/transfer functions that drive them, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;

/*
CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
Sleeping wake and popped-out reattach are runtime-owner moves, not startup attempts. Parked owner geometry and reattach plans stay process-local, require the same durable shell session, process-local runtime id, pane/session slot, and current body bounds, and must not create launch payloads, startup hosts, fallback surfaces, logs, shell-state fields, or fake Running state.
*/
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalParkedOwnerBodyGeometry {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct AgentsTerminalParkedOwnerReattachPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) parked_mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) current_mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) scale_factor: f32,
}

#[cfg(target_os = "macos")]
impl AgentsTerminalParkedOwnerReattachPlan {
    pub(crate) fn attachment_plan(
        self,
    ) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
        terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
            host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
                self.current_mount_slot_id,
            ),
            slot_id: self.current_mount_slot_id,
            bounds: self.bounds,
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct AgentsTerminalParkedRuntimeOwner {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) mount_slot_id: AgentsTerminalBodyMountSlotId,
    pub(crate) host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView,
    pub(crate) surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner,
}

#[cfg(target_os = "macos")]
impl AgentsTerminalParkedRuntimeOwner {
    pub(crate) fn new(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        mount_slot_id: AgentsTerminalBodyMountSlotId,
        host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView,
        surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner,
    ) -> Self {
        Self {
            runtime_session_id,
            shell_session_id,
            mount_slot_id,
            host_native_view,
            surface_owner,
        }
    }

    pub(crate) fn matches_identity(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        shell_session_id: TerminalSessionId,
        mount_slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.runtime_session_id == runtime_session_id
            && self.shell_session_id == shell_session_id
            && self.mount_slot_id == mount_slot_id
            && self.surface_owner.mount_slot_id() == mount_slot_id
            && self.surface_owner.runtime_session_id() == runtime_session_id
    }

    pub(crate) fn can_reattach_with_plan(
        &self,
        plan: AgentsTerminalParkedOwnerReattachPlan,
    ) -> bool {
        self.matches_identity(
            plan.runtime_session_id,
            plan.shell_session_id,
            plan.parked_mount_slot_id,
        ) && self
            .host_native_view
            .can_move_to_running_attachment_plan(plan.attachment_plan())
            && self
                .surface_owner
                .can_move_to_mount_slot(plan.runtime_session_id)
    }

    pub(crate) fn into_running_owners(
        self,
        plan: AgentsTerminalParkedOwnerReattachPlan,
    ) -> (
        terminal_native_view::AppOwnedTerminalHostNativeView,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    ) {
        (
            self.host_native_view
                .into_rekeyed_running_host_native_view(plan.attachment_plan()),
            self.surface_owner
                .into_rekeyed_surface_owner(plan.current_mount_slot_id, plan.runtime_session_id),
        )
    }
}

/*
CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
Sleeping command terminals should park the existing AppKit host and Ghostty surface owner, not free and recreate them on wake. The parked owner is process-local and exact command group/session keyed; it must never infer ownership from titles, command text, cwd/env, terminal output, focus fallback, shell-state JSON, logs, or Agents runtime maps.
*/
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct CommandTerminalParkedOwnerReattachPlan {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) parked_mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) current_mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) bounds: Bounds<Pixels>,
}

#[cfg(target_os = "macos")]
impl CommandTerminalParkedOwnerReattachPlan {
    pub(crate) fn attachment_plan(
        self,
    ) -> terminal_surface_host::NativeTerminalSurfaceAttachmentPlan<CommandTerminalBodyMountSlotId>
    {
        terminal_surface_host::NativeTerminalSurfaceAttachmentPlan {
            host_id: terminal_surface_host::NativeTerminalSurfaceHostId::from_slot_id(
                self.current_mount_slot_id,
            ),
            slot_id: self.current_mount_slot_id,
            bounds: self.bounds,
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct CommandTerminalParkedRuntimeOwner {
    pub(crate) runtime_session_id: AgentsTerminalRuntimeSessionId,
    pub(crate) mount_slot_id: CommandTerminalBodyMountSlotId,
    pub(crate) host_native_view:
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    pub(crate) surface_owner:
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
}

#[cfg(target_os = "macos")]
impl CommandTerminalParkedRuntimeOwner {
    pub(crate) fn new(
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        mount_slot_id: CommandTerminalBodyMountSlotId,
        host_native_view: terminal_native_view::AppOwnedTerminalHostNativeView<
            CommandTerminalBodyMountSlotId,
        >,
        surface_owner: terminal_ghostty_surface::GhosttySurfaceOwner<
            CommandTerminalBodyMountSlotId,
        >,
    ) -> Self {
        Self {
            runtime_session_id,
            mount_slot_id,
            host_native_view,
            surface_owner,
        }
    }

    pub(crate) fn matches_identity(
        &self,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
        mount_slot_id: CommandTerminalBodyMountSlotId,
    ) -> bool {
        self.runtime_session_id == runtime_session_id
            && self.mount_slot_id == mount_slot_id
            && self.surface_owner.mount_slot_id() == mount_slot_id
            && self.surface_owner.runtime_session_id() == runtime_session_id
    }

    pub(crate) fn can_reattach_with_plan(
        &self,
        plan: CommandTerminalParkedOwnerReattachPlan,
    ) -> bool {
        self.matches_identity(plan.runtime_session_id, plan.parked_mount_slot_id)
            && plan.parked_mount_slot_id == plan.current_mount_slot_id
            && self
                .host_native_view
                .can_rekey_to_running_attachment_plan(plan.attachment_plan())
            && self
                .surface_owner
                .can_rekey_to_mount_slot(plan.current_mount_slot_id, plan.runtime_session_id)
    }

    pub(crate) fn into_running_owners(
        self,
        plan: CommandTerminalParkedOwnerReattachPlan,
    ) -> (
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    ) {
        (
            self.host_native_view
                .into_rekeyed_running_host_native_view(plan.attachment_plan()),
            self.surface_owner
                .into_rekeyed_surface_owner(plan.current_mount_slot_id, plan.runtime_session_id),
        )
    }
}

pub(crate) fn prune_agents_terminal_parked_owner_body_slot_geometries(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
) {
    let current_slot_ids = if agents_workspace_visible {
        agents_workspace.rendered_terminal_parked_owner_body_slots()
    } else {
        Vec::new()
    };
    parked_owner_body_geometries.retain(|slot_id, _| current_slot_ids.contains(slot_id));
}

pub(crate) fn record_agents_terminal_parked_owner_body_slot_geometry(
    agents_workspace_visible: bool,
    agents_workspace: &WorkspaceModel,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
    bounds: Bounds<Pixels>,
    scale_factor: f32,
) {
    prune_agents_terminal_parked_owner_body_slot_geometries(
        agents_workspace_visible,
        agents_workspace,
        parked_owner_body_geometries,
    );

    if agents_workspace_visible
        && agents_workspace.is_current_terminal_parked_owner_body_slot(slot_id)
    {
        parked_owner_body_geometries.insert(
            slot_id,
            AgentsTerminalParkedOwnerBodyGeometry {
                bounds,
                scale_factor,
            },
        );
    } else {
        parked_owner_body_geometries.remove(&slot_id);
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn terminal_presentation_state_can_hold_parked_runtime_owner(
    session: &TerminalSession,
) -> bool {
    matches!(
        session.presentation_state,
        TerminalSessionPresentationState::Running
            | TerminalSessionPresentationState::Sleeping
            | TerminalSessionPresentationState::PoppedOutPlaceholder
    ) || (session.presentation_state == TerminalSessionPresentationState::Mounting
        && !session.can_enter_startup_pipeline())
}

#[cfg(target_os = "macos")]
pub(crate) fn prune_agents_terminal_parked_runtime_owners(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
) {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Parked Agents owners survive only while the same shell session and process-local runtime id remain current and absent from Running owner maps. The remembered slot stays the proof of where the owner was parked. Running tabs may otherwise park while inactive so tab switches preserve their attached terminal like macOS; stale entries are pruned instead of relaunched, inferred from titles/paths/commands, or promoted to fake Running state.
    */
    parked_runtime_owners.retain(|runtime_session_id, owner| {
        *runtime_session_id == owner.runtime_session_id
            && runtime_sessions.runtime_session_id_for_shell_session(owner.shell_session_id)
                == Some(*runtime_session_id)
            && workspace.has_session(owner.shell_session_id)
            && workspace
                .session(owner.shell_session_id)
                .is_some_and(terminal_presentation_state_can_hold_parked_runtime_owner)
            && !running_host_native_views.contains_key(&owner.mount_slot_id)
            && !running_surface_owners.contains_key(&owner.mount_slot_id)
            && owner.matches_identity(
                *runtime_session_id,
                owner.shell_session_id,
                owner.mount_slot_id,
            )
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_parked_owner_reattach_plan_for_slot(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<AgentsTerminalParkedOwnerReattachPlan> {
    if !agents_workspace_visible || !workspace.is_current_terminal_parked_owner_body_slot(slot_id) {
        return None;
    }
    let session = workspace.session(slot_id.session_id)?;
    if session.presentation_state != TerminalSessionPresentationState::Mounting
        || session.can_enter_startup_pipeline()
    {
        return None;
    }
    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let geometry = parked_owner_body_geometries.get(&slot_id).copied()?;
    let plan = AgentsTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        shell_session_id: slot_id.session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds: geometry.bounds,
        scale_factor: geometry.scale_factor,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}

#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_running_parked_owner_reattach_plan_for_slot(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_mount_slot_bounds: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: AgentsTerminalBodyMountSlotId,
) -> Option<AgentsTerminalParkedOwnerReattachPlan> {
    /*
    CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
    Inactive Running Agents tabs keep their AppKit/Ghostty owner parked so switching back to a sidebar-attached session shows the existing terminal immediately instead of creating a blank replacement shell. Reattach only when the same Running slot is current and its body bounds were recorded by the normal mount-slot canvas.
    */
    if !agents_workspace_visible || !workspace.is_current_terminal_body_mount_slot(slot_id) {
        return None;
    }
    if !workspace
        .session(slot_id.session_id)
        .is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Running
        })
    {
        return None;
    }
    let runtime_session_id =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)?;
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let bounds = *running_mount_slot_bounds.get(&slot_id)?;
    let plan = AgentsTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        shell_session_id: slot_id.session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds,
        scale_factor: 1.0,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}

#[cfg(target_os = "macos")]
pub(crate) fn agents_terminal_parked_owner_reattach_plans(
    agents_workspace_visible: bool,
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    running_mount_slot_bounds: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
) -> Vec<AgentsTerminalParkedOwnerReattachPlan> {
    let mut plans = workspace
        .rendered_terminal_parked_owner_body_slots()
        .into_iter()
        .filter_map(|slot_id| {
            agents_terminal_parked_owner_reattach_plan_for_slot(
                agents_workspace_visible,
                workspace,
                runtime_sessions,
                parked_runtime_owners,
                parked_owner_body_geometries,
                slot_id,
            )
        })
        .collect::<Vec<_>>();
    plans.extend(
        workspace
            .rendered_terminal_body_mount_slots()
            .into_iter()
            .filter_map(|slot_id| {
                agents_terminal_running_parked_owner_reattach_plan_for_slot(
                    agents_workspace_visible,
                    workspace,
                    runtime_sessions,
                    parked_runtime_owners,
                    running_mount_slot_bounds,
                    slot_id,
                )
            }),
    );
    plans.sort_by_key(|plan| {
        (
            plan.current_mount_slot_id.pane_id.0,
            plan.current_mount_slot_id.session_id.0,
            plan.runtime_session_id.0,
        )
    });
    plans
}

#[cfg(target_os = "macos")]
pub(crate) fn park_agents_terminal_runtime_owner_before_host_detach(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    detach_plan: terminal_surface_host::NativeTerminalSurfaceAttachmentPlan,
) -> bool {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Detaching a Running slot parks ownership when the same shell tab remains a valid Running, Sleeping, popped-out, or non-startup Mounting owner. This preserves inactive running tabs across ordinary tab switches while still requiring the exact AppKit host and Ghostty surface to match the runtime id; otherwise the normal detach/drop path remains honest instead of creating a fallback parked owner.
    */
    let slot_id = detach_plan.slot_id;
    let Some(session) = workspace.session(slot_id.session_id) else {
        return false;
    };
    if !terminal_presentation_state_can_hold_parked_runtime_owner(session)
        || !workspace.session_belongs_to_pane(slot_id.pane_id, slot_id.session_id)
    {
        return false;
    }
    let Some(runtime_session_id) =
        runtime_sessions.runtime_session_id_for_shell_session(slot_id.session_id)
    else {
        return false;
    };
    if parked_runtime_owners.contains_key(&runtime_session_id)
        || parked_runtime_owners.values().any(|owner| {
            owner.mount_slot_id == slot_id || owner.shell_session_id == slot_id.session_id
        })
    {
        return false;
    }
    if !running_host_native_views
        .get(&slot_id)
        .is_some_and(|host| host.attachment_plan().same_attachment_identity(detach_plan))
        || !running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id && surface.runtime_session_id() == runtime_session_id
        })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&slot_id) else {
        running_host_native_views.insert(slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        AgentsTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            slot_id.session_id,
            slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}

#[cfg(target_os = "macos")]
#[allow(dead_code)] // no caller: group moves park runtime owners through the pane-level parking path
pub(crate) fn park_agents_terminal_runtime_owner_for_group_move(
    workspace: &WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    source_slot_id: AgentsTerminalBodyMountSlotId,
) -> bool {
    /*
    CDXC:GPUISidebarGroupFocus 2026-07-10:
    Before a sidebar-selected Running terminal moves from its old Agents group
    into the currently focused group, park its exact AppKit/Ghostty owners.
    The normal render pass will provide the destination bounds and reattach the
    same process under the new slot; no shell replay, fallback surface, hidden
    overlap, or synthetic input routing participates.
    */
    let Some(session) = workspace.session(source_slot_id.session_id) else {
        return false;
    };
    if session.presentation_state != TerminalSessionPresentationState::Running
        || !workspace.session_belongs_to_pane(source_slot_id.pane_id, source_slot_id.session_id)
    {
        return false;
    }
    let Some(runtime_session_id) =
        runtime_sessions.runtime_session_id_for_shell_session(source_slot_id.session_id)
    else {
        return false;
    };
    if parked_runtime_owners.contains_key(&runtime_session_id) {
        return false;
    }
    if !running_host_native_views
        .get(&source_slot_id)
        .is_some_and(|host| host.attachment_plan().slot_id == source_slot_id)
        || !running_surface_owners
            .get(&source_slot_id)
            .is_some_and(|surface| {
                surface.mount_slot_id() == source_slot_id
                    && surface.runtime_session_id() == runtime_session_id
            })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&source_slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&source_slot_id) else {
        running_host_native_views.insert(source_slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        AgentsTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            source_slot_id.session_id,
            source_slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn transfer_agents_terminal_parked_runtime_owner_reattach(
    workspace: &mut WorkspaceModel,
    runtime_sessions: &AgentsTerminalRuntimeSessionRegistry,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        AgentsTerminalParkedRuntimeOwner,
    >,
    parked_owner_body_geometries: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        AgentsTerminalParkedOwnerBodyGeometry,
    >,
    running_mount_slot_bounds: &mut HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView,
    >,
    running_surface_owners: &mut HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner,
    >,
    running_config_requests: &HashMap<
        AgentsTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    plan: AgentsTerminalParkedOwnerReattachPlan,
) -> bool {
    /*
    CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
    Reattach is transactional: require exact current body geometry, exact parked runtime ownership, empty Running owner maps, and the same pane/session slot before moving ownership back. Mounting wake/reattach placeholders transition to Running; already-Running inactive tabs keep their shell state and reclaim the parked host/surface without relaunching or showing a blank replacement.
    */
    let Some(session) = workspace.session(plan.shell_session_id) else {
        return false;
    };
    let presentation_state = session.presentation_state;
    let can_enter_startup_pipeline = session.can_enter_startup_pipeline();
    let current_body_matches = if presentation_state == TerminalSessionPresentationState::Running {
        workspace.is_current_terminal_body_mount_slot(plan.current_mount_slot_id)
            && running_mount_slot_bounds
                .get(&plan.current_mount_slot_id)
                .is_some_and(|bounds| *bounds == plan.bounds)
    } else if presentation_state == TerminalSessionPresentationState::Mounting
        && !can_enter_startup_pipeline
    {
        workspace.is_current_terminal_parked_owner_body_slot(plan.current_mount_slot_id)
            && parked_owner_body_geometries
                .get(&plan.current_mount_slot_id)
                .is_some_and(|geometry| {
                    geometry.bounds == plan.bounds && geometry.scale_factor == plan.scale_factor
                })
    } else {
        false
    };
    if runtime_sessions.runtime_session_id_for_shell_session(plan.shell_session_id)
        != Some(plan.runtime_session_id)
        || !current_body_matches
        || running_host_native_views.contains_key(&plan.current_mount_slot_id)
        || running_surface_owners.contains_key(&plan.current_mount_slot_id)
        || running_config_requests.contains_key(&plan.current_mount_slot_id)
    {
        return false;
    }
    let Some(parked_owner) = parked_runtime_owners.get(&plan.runtime_session_id) else {
        return false;
    };
    if !parked_owner.can_reattach_with_plan(plan) {
        return false;
    }

    let Some(parked_owner) = parked_runtime_owners.remove(&plan.runtime_session_id) else {
        return false;
    };
    if presentation_state == TerminalSessionPresentationState::Mounting {
        let changed = workspace.transition_terminal_session_presentation_state(
            plan.shell_session_id,
            TerminalSessionPresentationState::Mounting,
            TerminalSessionPresentationState::Running,
        );
        if !changed {
            parked_runtime_owners.insert(plan.runtime_session_id, parked_owner);
            return false;
        }
    }

    let (host_native_view, surface_owner) = parked_owner.into_running_owners(plan);
    running_mount_slot_bounds.insert(plan.current_mount_slot_id, plan.bounds);
    running_host_native_views.insert(plan.current_mount_slot_id, host_native_view);
    running_surface_owners.insert(plan.current_mount_slot_id, surface_owner);
    parked_owner_body_geometries.remove(&plan.current_mount_slot_id);
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_session_can_hold_parked_runtime_owner(
    command_pane: &CommandPaneModel,
    slot_id: CommandTerminalBodyMountSlotId,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Native command-panel owner selection parks renderer ownership whenever the command session still belongs to its command group, not only when Sleep hides it. Inactive tabs, collapsed command panels, and Focus-hidden groups may reattach the same host/surface later; removed or stale command sessions still prune.
    */
    command_pane_group_for_session(command_pane, slot_id.session_id) == Some(slot_id.group_id)
        && command_pane.session(slot_id.session_id).is_some()
}

#[cfg(target_os = "macos")]
pub(crate) fn prune_command_terminal_parked_runtime_owners(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
) {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Parked command owners survive only for the same command group/session slot while the tab is still part of command-panel state. Stale session membership, close/removal, and Running owner collisions prune the parked process instead of relaunching, retargeting, logging, persisting, or fabricating fallback surfaces.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Owner-selection parity requires inactive, collapsed, and Focus-hidden command tabs to keep their parked runtime owners. Prune only when the command session no longer belongs to that exact command group or a live Running owner already exists for the slot.
    */
    parked_runtime_owners.retain(|runtime_session_id, owner| {
        *runtime_session_id == owner.runtime_session_id
            && command_terminal_runtime_session_id(owner.mount_slot_id) == *runtime_session_id
            && command_terminal_session_can_hold_parked_runtime_owner(
                command_pane,
                owner.mount_slot_id,
            )
            && !running_host_native_views.contains_key(&owner.mount_slot_id)
            && !running_surface_owners.contains_key(&owner.mount_slot_id)
            && owner.matches_identity(*runtime_session_id, owner.mount_slot_id)
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn park_command_terminal_runtime_owner_before_host_detach(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_host_native_views: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    detach_plan: terminal_surface_host::NativeTerminalSurfaceAttachmentPlan<
        CommandTerminalBodyMountSlotId,
    >,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Command `HideAndDetach` parks ownership only when the command tab still belongs to its exact group. The AppKit host and Ghostty surface must already exist with the exact command slot/runtime identity; close/removal, stale groups, collisions, and missing owners continue through the honest detach/drop path.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Native owner-selection, collapse, and Focus hiding detach visible command panes without freeing their terminal owners. Do not require `isSleeping`; a live inactive command tab should park so reselecting or reopening can reattach the same runtime owner instead of creating a replacement process.
    */
    let slot_id = detach_plan.slot_id;
    if command_pane.session(slot_id.session_id).is_none()
        || command_pane_group_for_session(command_pane, slot_id.session_id)
            != Some(slot_id.group_id)
    {
        return false;
    }
    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    if parked_runtime_owners.contains_key(&runtime_session_id)
        || parked_runtime_owners.values().any(|owner| {
            owner.mount_slot_id == slot_id || owner.mount_slot_id.session_id == slot_id.session_id
        })
    {
        return false;
    }
    if !running_host_native_views
        .get(&slot_id)
        .is_some_and(|host| host.attachment_plan().same_attachment_identity(detach_plan))
        || !running_surface_owners.get(&slot_id).is_some_and(|surface| {
            surface.mount_slot_id() == slot_id && surface.runtime_session_id() == runtime_session_id
        })
    {
        return false;
    }

    let Some(host_native_view) = running_host_native_views.remove(&slot_id) else {
        return false;
    };
    let Some(mut surface_owner) = running_surface_owners.remove(&slot_id) else {
        running_host_native_views.insert(slot_id, host_native_view);
        return false;
    };
    surface_owner.set_focus(false);
    terminal_native_view::set_app_owned_terminal_host_native_view_visible(
        Some(&host_native_view),
        false,
    );
    parked_runtime_owners.insert(
        runtime_session_id,
        CommandTerminalParkedRuntimeOwner::new(
            runtime_session_id,
            slot_id,
            host_native_view,
            surface_owner,
        ),
    );
    true
}

#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_parked_owner_reattach_plan_for_slot(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    current_body_bounds: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    slot_id: CommandTerminalBodyMountSlotId,
) -> Option<CommandTerminalParkedOwnerReattachPlan> {
    if !command_pane.is_current_terminal_body_mount_slot(slot_id)
        || command_pane_group_for_session(command_pane, slot_id.session_id)
            != Some(slot_id.group_id)
        || command_pane
            .session(slot_id.session_id)
            .is_some_and(|session| session.is_sleeping)
    {
        return None;
    }
    let runtime_session_id = command_terminal_runtime_session_id(slot_id);
    let parked_owner = parked_runtime_owners.get(&runtime_session_id)?;
    let bounds = current_body_bounds.get(&slot_id).copied()?;
    let plan = CommandTerminalParkedOwnerReattachPlan {
        runtime_session_id,
        parked_mount_slot_id: parked_owner.mount_slot_id,
        current_mount_slot_id: slot_id,
        bounds,
    };
    parked_owner.can_reattach_with_plan(plan).then_some(plan)
}

#[cfg(target_os = "macos")]
pub(crate) fn command_terminal_parked_owner_reattach_plans(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    current_body_bounds: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
) -> Vec<CommandTerminalParkedOwnerReattachPlan> {
    let mut plans = command_pane
        .rendered_terminal_body_mount_slots()
        .into_iter()
        .filter_map(|slot_id| {
            command_terminal_parked_owner_reattach_plan_for_slot(
                command_pane,
                parked_runtime_owners,
                current_body_bounds,
                slot_id,
            )
        })
        .collect::<Vec<_>>();
    plans.sort_by_key(|plan| {
        (
            plan.current_mount_slot_id.group_id.0,
            plan.current_mount_slot_id.session_id.0,
            plan.runtime_session_id.0,
        )
    });
    plans
}

#[cfg(target_os = "macos")]
pub(crate) fn transfer_command_terminal_parked_runtime_owner_reattach(
    command_pane: &CommandPaneModel,
    parked_runtime_owners: &mut HashMap<
        AgentsTerminalRuntimeSessionId,
        CommandTerminalParkedRuntimeOwner,
    >,
    running_mount_slot_bounds: &mut HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    running_host_native_views: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_native_view::AppOwnedTerminalHostNativeView<CommandTerminalBodyMountSlotId>,
    >,
    running_surface_owners: &mut HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceOwner<CommandTerminalBodyMountSlotId>,
    >,
    running_config_requests: &HashMap<
        CommandTerminalBodyMountSlotId,
        terminal_ghostty_surface::GhosttySurfaceConfigRequest,
    >,
    plan: CommandTerminalParkedOwnerReattachPlan,
) -> bool {
    /*
    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
    Command reattach is transactional around exact current command body geometry and empty Running owner maps. A sleeping tab waking to the same group/session slot may receive the parked host/surface owner; mismatches leave normal command mount reconciliation responsible and never create launch payloads, new Ghostty surfaces, logs, persisted runtime ids, or fallback command processes.

    CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-08:59:
    Reattach also serves native command-panel owner selection: an inactive or collapsed command tab that becomes the current visible owner may receive its parked host/surface if the same group/session slot and current body bounds match.
    */
    if plan.parked_mount_slot_id != plan.current_mount_slot_id
        || command_terminal_runtime_session_id(plan.current_mount_slot_id)
            != plan.runtime_session_id
        || !command_pane.is_current_terminal_body_mount_slot(plan.current_mount_slot_id)
        || command_pane_group_for_session(command_pane, plan.current_mount_slot_id.session_id)
            != Some(plan.current_mount_slot_id.group_id)
        || command_pane
            .session(plan.current_mount_slot_id.session_id)
            .is_some_and(|session| session.is_sleeping)
        || !running_mount_slot_bounds
            .get(&plan.current_mount_slot_id)
            .is_some_and(|bounds| *bounds == plan.bounds)
        || running_host_native_views.contains_key(&plan.current_mount_slot_id)
        || running_surface_owners.contains_key(&plan.current_mount_slot_id)
        || running_config_requests.contains_key(&plan.current_mount_slot_id)
    {
        return false;
    }
    let Some(parked_owner) = parked_runtime_owners.get(&plan.runtime_session_id) else {
        return false;
    };
    if !parked_owner.can_reattach_with_plan(plan) {
        return false;
    }

    let Some(parked_owner) = parked_runtime_owners.remove(&plan.runtime_session_id) else {
        return false;
    };
    let (host_native_view, surface_owner) = parked_owner.into_running_owners(plan);
    running_mount_slot_bounds.insert(plan.current_mount_slot_id, plan.bounds);
    running_host_native_views.insert(plan.current_mount_slot_id, host_native_view);
    running_surface_owners.insert(plan.current_mount_slot_id, surface_owner);
    true
}
