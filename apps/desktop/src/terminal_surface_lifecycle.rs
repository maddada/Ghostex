use std::collections::HashMap;

#[cfg(target_os = "macos")]
use crate::terminal_native_view::RealTerminalNativeViewHandle;
#[cfg(target_os = "macos")]
use crate::terminal_surface_host::NativeTerminalSurfacePlatformCommand;
use crate::terminal_surface_host::{
    NativeTerminalSurfaceAttachmentPlan, NativeTerminalSurfaceHostCommand,
};
use crate::{AgentsTerminalBodyMountSlotId, TerminalSurfaceMountSlotKey};

/*
CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-22:45:
The native-view lifecycle boundary is runtime-only for every current visible running Agents terminal mount slot. Host reconciliation commands may only become AppKit/Ghostty work after the slot has a supplied real native terminal view; awaiting state records only the current slot plan and must not manufacture handles, create views, call GhosttyKit/libghostty, build Ghostty surface configs, execute AppKit, log, persist, overlay, route hit tests, or launch/restart the app.

CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-21:27:
Slice 107 depends on empty host command batches being meaningful no-ops during visible Agents pre-layout bounds resets. The lifecycle must keep its awaiting real-view slot until host sync later reports same-bounds NoOp, move/resize, hidden workspace, or no-current-slot detach commands.

CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-22:45:
The App may own one runtime host NSView per visible running Agents mount slot. A lifecycle slot may move from awaiting to ready only after an owned host view is created for the exact same plan, and this state still must not execute AppKit commands, build Ghostty configs, call GhosttyKit/libghostty, log, persist, show, focus, or launch a terminal.

CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-22:45:
The all-visible-running-leaf slice keeps lifecycle state per pane/session mount slot. Adding, resizing, or detaching one visible Agents terminal must not clear unrelated visible running slots, and each slot still requires an explicit real App-owned host view before AppKit or Ghostty work can proceed.

CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
Command-pane runtime terminal bodies use the same lifecycle state machine with command group/session slot ids. The shared lifecycle never crosses into Agents startup/session registries or shell-state persistence; command collapse and close reconcile as ordinary detach commands before AppKit host views are released.
*/
pub(crate) struct NativeTerminalSurfaceLifecycleState<SlotId = AgentsTerminalBodyMountSlotId> {
    active_slots: HashMap<SlotId, NativeTerminalSurfaceLifecycleSlot<SlotId>>,
}

impl<SlotId> Default for NativeTerminalSurfaceLifecycleState<SlotId> {
    fn default() -> Self {
        Self {
            active_slots: HashMap::new(),
        }
    }
}

impl<SlotId> NativeTerminalSurfaceLifecycleState<SlotId>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[cfg(target_os = "macos")]
    #[allow(dead_code)] // no caller: the surface host owns native view creation now
    pub(crate) fn with_explicit_real_native_view(
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
        real_view: RealTerminalNativeViewHandle,
    ) -> Self {
        /*
        CDXC:GPUTerminalSurfaceLifecycle 2026-06-22-21:17:
        Ready lifecycle state can only be constructed around an explicit existing real terminal native view handle. This constructor does not create, retain, validate, log, persist, or operate on the view; the unsafe boundary remains the handle supplier.
        */
        Self {
            active_slots: HashMap::from([(
                plan.slot_id,
                NativeTerminalSurfaceLifecycleSlot {
                    plan,
                    native_view: NativeTerminalNativeViewState::Ready { real_view },
                },
            )]),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn mark_ready_with_real_native_view(
        &mut self,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
        real_view: RealTerminalNativeViewHandle,
    ) -> bool {
        let Some(slot) = self.active_slots.get_mut(&plan.slot_id) else {
            return false;
        };
        if slot.plan != plan {
            return false;
        }

        *slot = NativeTerminalSurfaceLifecycleSlot {
            plan,
            native_view: NativeTerminalNativeViewState::Ready { real_view },
        };
        true
    }

    pub(crate) fn reconcile_host_commands(
        &mut self,
        commands: &[NativeTerminalSurfaceHostCommand<SlotId>],
    ) -> Vec<NativeTerminalSurfaceLifecycleDecision<SlotId>> {
        let mut decisions = Vec::new();

        for command in commands {
            decisions.extend(self.reconcile_host_command(*command));
        }

        decisions
    }

    fn reconcile_host_command(
        &mut self,
        command: NativeTerminalSurfaceHostCommand<SlotId>,
    ) -> Vec<NativeTerminalSurfaceLifecycleDecision<SlotId>> {
        match command {
            NativeTerminalSurfaceHostCommand::AttachOrShow { plan }
            | NativeTerminalSurfaceHostCommand::MoveOrResize { plan } => {
                self.reconcile_visible_plan(command, plan)
            }
            NativeTerminalSurfaceHostCommand::NoOp { plan } => {
                self.reconcile_no_op_plan(command, plan)
            }
            NativeTerminalSurfaceHostCommand::HideAndDetach { plan } => {
                self.reconcile_detached_plan(command, plan)
            }
        }
    }

    fn reconcile_visible_plan(
        &mut self,
        command: NativeTerminalSurfaceHostCommand<SlotId>,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> Vec<NativeTerminalSurfaceLifecycleDecision<SlotId>> {
        let native_view = self
            .active_slots
            .get(&plan.slot_id)
            .map(|slot| slot.native_view)
            .unwrap_or(NativeTerminalNativeViewState::AwaitingRealNativeView);

        self.active_slots.insert(
            plan.slot_id,
            NativeTerminalSurfaceLifecycleSlot { plan, native_view },
        );
        vec![decision_for_visible_command(command, native_view)]
    }

    fn reconcile_no_op_plan(
        &mut self,
        command: NativeTerminalSurfaceHostCommand<SlotId>,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> Vec<NativeTerminalSurfaceLifecycleDecision<SlotId>> {
        let native_view = self
            .active_slots
            .get(&plan.slot_id)
            .map(|slot| slot.native_view)
            .unwrap_or(NativeTerminalNativeViewState::AwaitingRealNativeView);

        self.active_slots.insert(
            plan.slot_id,
            NativeTerminalSurfaceLifecycleSlot { plan, native_view },
        );
        let decision = match native_view {
            NativeTerminalNativeViewState::AwaitingRealNativeView => {
                NativeTerminalSurfaceLifecycleDecision::NeedsRealNativeView { plan }
            }
            #[cfg(target_os = "macos")]
            NativeTerminalNativeViewState::Ready { .. } => {
                NativeTerminalSurfaceLifecycleDecision::NoOp { plan }
            }
        };

        let _ = command;
        vec![decision]
    }

    fn reconcile_detached_plan(
        &mut self,
        command: NativeTerminalSurfaceHostCommand<SlotId>,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> Vec<NativeTerminalSurfaceLifecycleDecision<SlotId>> {
        let Some(slot) = self.active_slots.get(&plan.slot_id).copied() else {
            return Vec::new();
        };

        if !slot.plan.same_attachment_identity(plan) {
            return Vec::new();
        }

        self.active_slots.remove(&plan.slot_id);
        match slot.native_view {
            NativeTerminalNativeViewState::AwaitingRealNativeView => Vec::new(),
            #[cfg(target_os = "macos")]
            NativeTerminalNativeViewState::Ready { real_view } => {
                vec![NativeTerminalSurfaceLifecycleDecision::DetachStaleView {
                    command: command.to_platform_command(),
                    real_view,
                }]
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
struct NativeTerminalSurfaceLifecycleSlot<SlotId = AgentsTerminalBodyMountSlotId> {
    plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    native_view: NativeTerminalNativeViewState,
}

#[derive(Clone, Copy, PartialEq)]
enum NativeTerminalNativeViewState {
    AwaitingRealNativeView,
    #[cfg(target_os = "macos")]
    Ready {
        real_view: RealTerminalNativeViewHandle,
    },
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum NativeTerminalSurfaceLifecycleDecision<SlotId = AgentsTerminalBodyMountSlotId> {
    NeedsRealNativeView {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
    #[cfg(target_os = "macos")]
    CanExecuteWithRealView {
        command: NativeTerminalSurfacePlatformCommand<SlotId>,
        real_view: RealTerminalNativeViewHandle,
    },
    #[cfg(target_os = "macos")]
    DetachStaleView {
        command: NativeTerminalSurfacePlatformCommand<SlotId>,
        real_view: RealTerminalNativeViewHandle,
    },
    NoOp {
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    },
}

fn decision_for_visible_command<SlotId: Copy>(
    command: NativeTerminalSurfaceHostCommand<SlotId>,
    native_view: NativeTerminalNativeViewState,
) -> NativeTerminalSurfaceLifecycleDecision<SlotId> {
    let plan = match command {
        NativeTerminalSurfaceHostCommand::AttachOrShow { plan }
        | NativeTerminalSurfaceHostCommand::MoveOrResize { plan }
        | NativeTerminalSurfaceHostCommand::NoOp { plan }
        | NativeTerminalSurfaceHostCommand::HideAndDetach { plan } => plan,
    };

    match native_view {
        NativeTerminalNativeViewState::AwaitingRealNativeView => {
            NativeTerminalSurfaceLifecycleDecision::NeedsRealNativeView { plan }
        }
        #[cfg(target_os = "macos")]
        NativeTerminalNativeViewState::Ready { real_view } => {
            NativeTerminalSurfaceLifecycleDecision::CanExecuteWithRealView {
                command: command.to_platform_command(),
                real_view,
            }
        }
    }
}
