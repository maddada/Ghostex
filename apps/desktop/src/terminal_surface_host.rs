use std::collections::{HashMap, HashSet};

use gpui::{Bounds, Pixels};

use crate::{
    AgentsTerminalBodyMountSlotId, CommandTerminalBodyMountSlotId, TerminalSurfaceMountSlotKey,
};

/*
CDXC:Terminal 2026-06-22-22:45:
Phase 2 native terminal parity needs App-owned runtime host boundaries before GPUI can safely create real libghostty AppKit child views. This synchronizer plans normal-layout attachments for current visible terminal mount slots and their recorded body bounds, but it must stay inert: no fake terminal surface, process, command text, output, fallback mount, logging, persistence, overlay, hidden hit region, AppKit hook, or libghostty call is allowed here.

CDXC:Terminal 2026-06-22-22:45:
The platform adapter needs typed runtime commands derived by reconciling previous validated host plans against the latest all-visible slot plans. Commands may only carry host id, slot id, and exact body bounds so stale cleanup, attach/show, and move/resize stay executable without exposing command text, terminal content, output, paths, URLs, titles, or user text.

CDXC:Terminal 2026-06-22-20:58:
Platform-command conversion stays pure and runtime-only until a future slice supplies a real terminal NSView pointer. The converted AppKit payload preserves GPUI body bounds without CEF-style integer rounding so the native terminal view can stay inside the measured body rectangle instead of overlapping tab bars, split handles, sidebars, CEF, or command-pane chrome.

CDXC:Terminal 2026-06-22-22:45:
Per-render GPUI bounds resets are not terminal removals. When Agents mode is visible and current running slots exist but body canvases have not recorded this frame's bounds yet, preserve their runtime host/lifecycle identities and wait for recorded bounds; clear stale state only when Agents is hidden or slots are no longer current.

CDXC:Terminal 2026-06-22-22:45:
The Phase 2 all-visible-leaf expansion plans one runtime host per rendered Agents leaf whose selected session is Running. Reconcile by stable pane/session slot id so visible non-focused leaves can mount real surfaces while hidden, sleeping, missing, inactive-tab, and non-Agents slots detach without fallback views or overlap.

CDXC:Terminal 2026-06-23-05:03:
The host reconciler is shared by Agents and command-pane terminal bodies through a typed mount-slot key. Command panes instantiate it with command group/session ids only, so command hosts remain isolated from Agents workspace maps, startup maps, shell-state JSON, and launch payload sources while still using the same normal AppKit child-view pipeline.
*/
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct NativeTerminalSurfaceHostId<SlotId = AgentsTerminalBodyMountSlotId> {
    pub(crate) slot_id: SlotId,
}

impl<SlotId> NativeTerminalSurfaceHostId<SlotId> {
    pub(crate) fn from_slot_id(slot_id: SlotId) -> Self {
        Self { slot_id }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct NativeTerminalSurfaceAttachmentPlan<SlotId = AgentsTerminalBodyMountSlotId> {
    pub(crate) host_id: NativeTerminalSurfaceHostId<SlotId>,
    pub(crate) slot_id: SlotId,
    pub(crate) bounds: Bounds<Pixels>,
}

impl<SlotId: Copy + PartialEq> NativeTerminalSurfaceAttachmentPlan<SlotId> {
    fn new(slot_id: SlotId, bounds: Bounds<Pixels>) -> Self {
        Self {
            host_id: NativeTerminalSurfaceHostId::from_slot_id(slot_id),
            slot_id,
            bounds,
        }
    }

    pub(crate) fn same_attachment_identity(self, other: Self) -> bool {
        self.host_id == other.host_id && self.slot_id == other.slot_id
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NativeTerminalSurfaceHostCommand<SlotId = AgentsTerminalBodyMountSlotId> {
    AttachOrShow {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
    MoveOrResize {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
    HideAndDetach {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
    NoOp {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeTerminalSurfacePlatformBounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[allow(dead_code)]
impl NativeTerminalSurfacePlatformBounds {
    pub(crate) fn from_gpui_bounds(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: bounds.origin.x.as_f32() as f64,
            y: bounds.origin.y.as_f32() as f64,
            width: bounds.size.width.as_f32().max(0.0) as f64,
            height: bounds.size.height.as_f32().max(0.0) as f64,
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct NativeTerminalSurfacePlatformCommandPayload<
    SlotId = AgentsTerminalBodyMountSlotId,
> {
    pub(crate) host_id: NativeTerminalSurfaceHostId<SlotId>,
    pub(crate) slot_id: SlotId,
    pub(crate) bounds: NativeTerminalSurfacePlatformBounds,
}

#[allow(dead_code)]
impl<SlotId: Copy> NativeTerminalSurfacePlatformCommandPayload<SlotId> {
    fn from_plan(plan: NativeTerminalSurfaceAttachmentPlan<SlotId>) -> Self {
        Self {
            host_id: plan.host_id,
            slot_id: plan.slot_id,
            bounds: NativeTerminalSurfacePlatformBounds::from_gpui_bounds(plan.bounds),
        }
    }
}

#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NativeTerminalSurfacePlatformCommand<SlotId = AgentsTerminalBodyMountSlotId> {
    AttachOrShow {
        payload: NativeTerminalSurfacePlatformCommandPayload<SlotId>,
    },
    MoveOrResize {
        payload: NativeTerminalSurfacePlatformCommandPayload<SlotId>,
    },
    HideAndDetach {
        payload: NativeTerminalSurfacePlatformCommandPayload<SlotId>,
    },
    NoOp {
        payload: NativeTerminalSurfacePlatformCommandPayload<SlotId>,
    },
}

impl<SlotId: Copy> NativeTerminalSurfaceHostCommand<SlotId> {
    #[allow(dead_code)]
    pub(crate) fn to_platform_command(self) -> NativeTerminalSurfacePlatformCommand<SlotId> {
        match self {
            Self::AttachOrShow { plan } => NativeTerminalSurfacePlatformCommand::AttachOrShow {
                payload: NativeTerminalSurfacePlatformCommandPayload::from_plan(plan),
            },
            Self::MoveOrResize { plan } => NativeTerminalSurfacePlatformCommand::MoveOrResize {
                payload: NativeTerminalSurfacePlatformCommandPayload::from_plan(plan),
            },
            Self::HideAndDetach { plan } => NativeTerminalSurfacePlatformCommand::HideAndDetach {
                payload: NativeTerminalSurfacePlatformCommandPayload::from_plan(plan),
            },
            Self::NoOp { plan } => NativeTerminalSurfacePlatformCommand::NoOp {
                payload: NativeTerminalSurfacePlatformCommandPayload::from_plan(plan),
            },
        }
    }
}

pub(super) struct NativeTerminalSurfaceHost<SlotId = AgentsTerminalBodyMountSlotId> {
    active_plans: HashMap<SlotId, NativeTerminalSurfaceAttachmentPlan<SlotId>>,
}

impl<SlotId> Default for NativeTerminalSurfaceHost<SlotId> {
    fn default() -> Self {
        Self {
            active_plans: HashMap::new(),
        }
    }
}

impl<SlotId> NativeTerminalSurfaceHost<SlotId>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn sync_visible_slots(
        &mut self,
        surface_visible: bool,
        current_slot_ids: &[SlotId],
        recorded_bounds: &HashMap<SlotId, Bounds<Pixels>>,
    ) -> Vec<NativeTerminalSurfaceHostCommand<SlotId>> {
        if !surface_visible {
            return self.reconcile_to_plans(&[], Vec::new());
        }

        let mut seen_slot_ids = HashSet::new();
        let mut current_visible_slot_ids = Vec::new();
        let mut next_plans = Vec::new();

        for slot_id in current_slot_ids.iter().copied() {
            if !seen_slot_ids.insert(slot_id) {
                continue;
            }
            current_visible_slot_ids.push(slot_id);

            if let Some(bounds) = recorded_bounds.get(&slot_id).copied() {
                next_plans.push(NativeTerminalSurfaceAttachmentPlan::new(slot_id, bounds));
            }
        }

        self.reconcile_to_plans(&current_visible_slot_ids, next_plans)
    }

    fn reconcile_to_plans(
        &mut self,
        current_slot_ids: &[SlotId],
        next_plans: Vec<NativeTerminalSurfaceAttachmentPlan<SlotId>>,
    ) -> Vec<NativeTerminalSurfaceHostCommand<SlotId>> {
        let mut commands = Vec::new();
        let current_slot_ids = current_slot_ids.iter().copied().collect::<HashSet<_>>();
        let stale_slot_ids = self
            .active_plans
            .keys()
            .copied()
            .filter(|slot_id| !current_slot_ids.contains(slot_id))
            .collect::<Vec<_>>();

        for stale_slot_id in stale_slot_ids {
            if let Some(stale_plan) = self.active_plans.remove(&stale_slot_id) {
                commands.push(NativeTerminalSurfaceHostCommand::HideAndDetach { plan: stale_plan });
            }
        }

        for next_plan in next_plans {
            let previous_plan = self.active_plans.get(&next_plan.slot_id).copied();
            commands.extend(Self::reconcile_plans(previous_plan, Some(next_plan)));
            self.active_plans.insert(next_plan.slot_id, next_plan);
        }

        commands
    }
    fn reconcile_plans(
        previous_plan: Option<NativeTerminalSurfaceAttachmentPlan<SlotId>>,
        next_plan: Option<NativeTerminalSurfaceAttachmentPlan<SlotId>>,
    ) -> Vec<NativeTerminalSurfaceHostCommand<SlotId>> {
        match (previous_plan, next_plan) {
            (None, None) => Vec::new(),
            (None, Some(plan)) => vec![NativeTerminalSurfaceHostCommand::AttachOrShow { plan }],
            (Some(plan), None) => vec![NativeTerminalSurfaceHostCommand::HideAndDetach { plan }],
            (Some(previous), Some(next)) if previous == next => {
                vec![NativeTerminalSurfaceHostCommand::NoOp { plan: next }]
            }
            (Some(previous), Some(next)) if previous.same_attachment_identity(next) => {
                vec![NativeTerminalSurfaceHostCommand::MoveOrResize { plan: next }]
            }
            (Some(previous), Some(next)) => vec![
                NativeTerminalSurfaceHostCommand::HideAndDetach { plan: previous },
                NativeTerminalSurfaceHostCommand::AttachOrShow { plan: next },
            ],
        }
    }
}

impl NativeTerminalSurfaceHost<AgentsTerminalBodyMountSlotId> {
    pub(crate) fn sync_visible_agents_slots(
        &mut self,
        agents_workspace_visible: bool,
        current_slot_ids: &[AgentsTerminalBodyMountSlotId],
        recorded_bounds: &HashMap<AgentsTerminalBodyMountSlotId, Bounds<Pixels>>,
    ) -> Vec<NativeTerminalSurfaceHostCommand> {
        self.sync_visible_slots(agents_workspace_visible, current_slot_ids, recorded_bounds)
    }
}

impl NativeTerminalSurfaceHost<CommandTerminalBodyMountSlotId> {
    pub(crate) fn sync_visible_command_slots(
        &mut self,
        command_pane_expanded: bool,
        current_slot_ids: &[CommandTerminalBodyMountSlotId],
        recorded_bounds: &HashMap<CommandTerminalBodyMountSlotId, Bounds<Pixels>>,
    ) -> Vec<NativeTerminalSurfaceHostCommand<CommandTerminalBodyMountSlotId>> {
        self.sync_visible_slots(command_pane_expanded, current_slot_ids, recorded_bounds)
    }
}
