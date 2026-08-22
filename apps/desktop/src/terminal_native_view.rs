#![allow(dead_code)]

#[cfg(target_os = "macos")]
use std::{
    collections::HashMap,
    ffi::{c_double, c_void},
    ptr::NonNull,
};

#[cfg(target_os = "macos")]
use crate::terminal_ghostty_surface::{
    self, GhosttySurfaceConfigRequest, GhosttySurfaceConfigRequestError,
};
#[cfg(target_os = "macos")]
use crate::terminal_surface_host::{
    NativeTerminalSurfaceAttachmentPlan, NativeTerminalSurfaceHostCommand,
};
use crate::terminal_surface_host::{
    NativeTerminalSurfacePlatformBounds, NativeTerminalSurfacePlatformCommand,
};
#[cfg(target_os = "macos")]
use crate::terminal_surface_lifecycle::{
    NativeTerminalSurfaceLifecycleDecision, NativeTerminalSurfaceLifecycleState,
};
#[cfg(target_os = "macos")]
use crate::{
    AgentsTerminalBodyMountSlotId, AgentsTerminalStartupBodySlotId,
    AgentsTerminalStartupHostPreservationKey, AgentsTerminalStartupLaunchPlan,
    TerminalSurfaceMountSlotKey,
};

/*
CDXC:GPUTerminalNativeView 2026-06-22-21:04:
Slice 104 introduces only the Rust-side execution boundary for the GPUI terminal AppKit adapter. AppKit terminal commands require a real non-null native terminal view handle, map host reconciliation commands to frame/show/hide operations, and stay unwired from GhostexGpuiApp until future terminal code supplies an existing NSView; do not create fake views, dangling production handles, terminal processes, GhosttyKit calls, focus side effects, logging, persistence, overlays, hidden hit regions, hit-test routing, or app launch behavior here.

CDXC:GPUTerminalNativeView 2026-06-22-21:11:
Slice 105 allows only pure config-building code to read this handle as an opaque non-null NSView pointer. That accessor must not allocate, retain, synthesize fallback views, call AppKit, call GhosttyKit, launch terminals, log, persist, route hit tests, or change focus.

CDXC:GPUTerminalNativeView 2026-06-22-21:42:
Slice 108 adds the AppKit host-view ownership boundary but keeps it unwired from GhostexGpuiApp. Creating a terminal host view requires an explicit non-null parent NSView plus validated finite, non-negative GPUI body bounds; only the owned wrapper may destroy the retained child view it created, while borrowed/test handles remain non-owning and must not trigger AppKit destroy.

CDXC:GPUTerminalNativeView 2026-06-22-22:45:
App-owned runtime host-view ownership is per visible running Agents mount slot. Creation is driven exclusively by a `NeedsRealNativeView` lifecycle decision, uses the app's parent NSView plus the exact plan bounds, remains hidden until a real Ghostty surface exists, and may execute only the slice-115 first-responder focus helper after a real focused surface is mounted; it must not log, persist, or invent fallback views.

CDXC:GPUTerminalStartupNativeHost 2026-06-23-00:32:
Mounting startup preparation may create a separate hidden App-owned host NSView only from a current `AgentsTerminalStartupLaunchPlan`. This startup owner is keyed by `AgentsTerminalStartupBodySlotId`, never by the Running mount slot during preparation, and it must not show, focus, create Ghostty surfaces, launch processes, persist, log, or feed the Running host maps except through the exact ready handoff.

CDXC:GPUTerminalStartupHostLifetime 2026-06-23-03:23:
Startup host lifetime may bridge the render-start geometry gap only for an already-created host whose stored launch plan matches the current pending runtime id and startup body slot. This helper must not create from pending records without geometry, and invalid parent/bounds/config state must still prune before any future launch consumer can use it.

CDXC:GPUTerminalStartupGhosttySurface 2026-06-23-03:33:
Startup Ghostty surfaces borrow the hidden startup host NSView, so app sync must be able to prove whether a startup host will survive before host reconciliation drops or replaces it. This helper remains a normal ownership predicate only; it does not show, focus, route input, launch processes, log, persist, or promote Mounting sessions.

CDXC:GPUTerminalStartupHandoff 2026-06-23-04:25:
Ready Mounting startup hosts must move into the Running host-owner map as the same AppKit child view. The conversion is allowed only for the same pane/session body identity and exact launch bounds, and it must not create, destroy, show, focus, overlap, route hit tests, log, persist, or infer command/cwd/env state.

CDXC:GPUTerminalNativeView 2026-06-22-22:05:
Slice 110 frame reconciliation may execute only `SetFrame` for same-slot `MoveOrResize` lifecycle decisions whose real-view handle still matches the App-owned hidden host view. Generic attach/show and hide/detach command execution still do not focus; slice 115 uses a separate focused-slot helper so terminal focus handoff cannot leak into broad host reconciliation.

CDXC:GPUTerminalGhosttySurfaceConfig 2026-06-22-22:17:
Slice 111 may prepare only a runtime Ghostty surface config request from the App-owned hidden host NSView plus the current GPUI window scale. The helper must return no request without an owner, reject invalid scale, and avoid AppKit attach/show/hide/focus, GhosttyKit/libghostty calls, surface creation, terminal processes, logging, persistence, fallbacks, overlays, hidden hit regions, and hit-test routing.

CDXC:GPUTerminalGhosttySurface 2026-06-22-22:59:
After a real GhosttyKit surface is created for a visible running Agents mount slot, that App-owned host view may be shown as the normal child view inside the exact recorded terminal body bounds. Visibility remains bounded to owned children only, and first-responder focus is allowed only through the slice-115 focused-slot helper for the same owned host view; keyboard/mouse event translation, broad AppKit execution, fallback views, overlays, hidden hit regions, hit-test routing, command/cwd/env lifecycle, persistence, and logging remain out of scope.

CDXC:GPUTerminalNativeView 2026-06-22-22:45:
All visible running Agents leaves now own AppKit host views by stable pane/session mount slot. The helper must reconcile maps without recreating same-slot views on resize and must drop only stale slot owners after Ghostty surfaces have already been released by the app.

CDXC:GPUTerminalNativeViewFocus 2026-06-22-23:11:
Slice 115 wires AppKit first-responder handoff through only the App-owned terminal host NSView that already backs a mounted real Agents Ghostty surface. Focus uses `[window makeFirstResponder:view]` via the GPUI-local adapter, is tracked as runtime-only slot plus host identity for idempotence, and must not add hit-test overrides, window pre-dispatch routing, transparent overlays, synthetic event routing, IME/paste/selection behavior, persistent logs, or shell persistence.

CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
Runtime AppKit host ownership is shared across Agents and command-pane terminal bodies by a typed mount-slot key. Command hosts use command group/session ids and never reuse Agents pane/session keys, startup host keys, shell-state JSON, or launch payload sources; collapse and close detach through the normal host/surface cleanup order.

CDXC:GPUITerminalNativeKeyBridge 2026-06-24-20:58:
Real terminal host views now accept native key focus only as exact App-owned child views. Visibility false and Drop both unregister the host from the Ghostty key-target registry so a stale NSView pointer cannot keep forwarding Return, Backspace, or modifier events after the mounted surface pairing is gone.
*/
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RealTerminalNativeViewHandle {
    native_view: NonNull<c_void>,
}

#[cfg(target_os = "macos")]
impl RealTerminalNativeViewHandle {
    /// # Safety
    ///
    /// `native_view` must be an existing real AppKit terminal `NSView` that remains valid for the
    /// duration of any executor call using this handle.
    pub(crate) unsafe fn from_existing_native_view(native_view: NonNull<c_void>) -> Self {
        Self { native_view }
    }

    pub(crate) fn as_non_null(self) -> NonNull<c_void> {
        self.native_view
    }

    pub(crate) fn as_ptr(self) -> *mut c_void {
        self.native_view.as_ptr()
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TerminalHostParentNativeViewHandle {
    parent_view: NonNull<c_void>,
}

#[cfg(target_os = "macos")]
impl TerminalHostParentNativeViewHandle {
    /// # Safety
    ///
    /// `parent_view` may be null only to return `NullParentNativeView`; a successful handle must
    /// wrap an existing real AppKit parent `NSView` that can own a terminal host child view.
    pub(crate) unsafe fn try_from_raw(
        parent_view: *mut c_void,
    ) -> Result<Self, TerminalHostNativeViewCreateError> {
        let parent_view = NonNull::new(parent_view)
            .ok_or(TerminalHostNativeViewCreateError::NullParentNativeView)?;
        Ok(Self { parent_view })
    }

    /// # Safety
    ///
    /// `parent_view` must be an existing real AppKit parent `NSView` that can own the terminal
    /// host child view for the lifetime of the returned owner.
    pub(crate) unsafe fn from_existing_parent_view(parent_view: NonNull<c_void>) -> Self {
        Self { parent_view }
    }

    fn as_ptr(self) -> *mut c_void {
        self.parent_view.as_ptr()
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerminalHostNativeViewCreateRequest {
    parent_view: TerminalHostParentNativeViewHandle,
    bounds: NativeTerminalSurfacePlatformBounds,
}

#[cfg(target_os = "macos")]
impl TerminalHostNativeViewCreateRequest {
    pub(crate) fn try_new(
        parent_view: TerminalHostParentNativeViewHandle,
        bounds: NativeTerminalSurfacePlatformBounds,
    ) -> Result<Self, TerminalHostNativeViewCreateError> {
        validate_terminal_host_bounds(bounds)?;
        Ok(Self {
            parent_view,
            bounds,
        })
    }

    fn parent_view(self) -> TerminalHostParentNativeViewHandle {
        self.parent_view
    }

    fn bounds(self) -> NativeTerminalSurfacePlatformBounds {
        self.bounds
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalHostNativeViewBoundsField {
    X,
    Y,
    Width,
    Height,
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum TerminalHostNativeViewCreateError {
    NullParentNativeView,
    InvalidBounds {
        field: TerminalHostNativeViewBoundsField,
        value: f64,
    },
    CreateReturnedNull,
}

#[cfg(target_os = "macos")]
fn validate_terminal_host_bounds(
    bounds: NativeTerminalSurfacePlatformBounds,
) -> Result<(), TerminalHostNativeViewCreateError> {
    validate_terminal_host_bound(TerminalHostNativeViewBoundsField::X, bounds.x)?;
    validate_terminal_host_bound(TerminalHostNativeViewBoundsField::Y, bounds.y)?;
    validate_terminal_host_bound(TerminalHostNativeViewBoundsField::Width, bounds.width)?;
    validate_terminal_host_bound(TerminalHostNativeViewBoundsField::Height, bounds.height)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn validate_terminal_host_bound(
    field: TerminalHostNativeViewBoundsField,
    value: f64,
) -> Result<(), TerminalHostNativeViewCreateError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(TerminalHostNativeViewCreateError::InvalidBounds { field, value })
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct TerminalHostNativeViewFactory;

#[cfg(target_os = "macos")]
impl TerminalHostNativeViewFactory {
    pub(crate) fn create(
        request: TerminalHostNativeViewCreateRequest,
    ) -> Result<OwnedTerminalHostNativeView, TerminalHostNativeViewCreateError> {
        let bounds = request.bounds();
        let native_view = unsafe {
            GhostexGpuiTerminalCreateHostNativeView(
                request.parent_view().as_ptr(),
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
            )
        };
        let native_view = NonNull::new(native_view)
            .ok_or(TerminalHostNativeViewCreateError::CreateReturnedNull)?;

        /*
        CDXC:GPUTerminalNativeView 2026-06-22-21:42:
        The factory owns only host views returned by the GPUI AppKit create shim. It must return an owned wrapper instead of a bare borrowed handle so Drop can remove and release exactly that retained child view later without touching borrowed/test handles.
        */
        Ok(unsafe { OwnedTerminalHostNativeView::from_created_native_view(native_view) })
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct OwnedTerminalHostNativeView {
    native_view: RealTerminalNativeViewHandle,
}

#[cfg(target_os = "macos")]
impl OwnedTerminalHostNativeView {
    /// # Safety
    ///
    /// `native_view` must be a retained NSView pointer returned by
    /// `GhostexGpuiTerminalCreateHostNativeView` and not an unrelated borrowed/test handle.
    unsafe fn from_created_native_view(native_view: NonNull<c_void>) -> Self {
        Self {
            native_view: RealTerminalNativeViewHandle { native_view },
        }
    }

    pub(crate) fn real_native_view_handle(&self) -> RealTerminalNativeViewHandle {
        self.native_view
    }
}

#[cfg(target_os = "macos")]
impl Drop for OwnedTerminalHostNativeView {
    fn drop(&mut self) {
        terminal_ghostty_surface::unregister_native_key_target(self.native_view);
        unsafe {
            GhostexGpuiTerminalDestroyHostNativeView(self.native_view.as_ptr());
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct AppOwnedTerminalHostNativeView<SlotId = AgentsTerminalBodyMountSlotId> {
    plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    native_view: OwnedTerminalHostNativeView,
}

#[cfg(target_os = "macos")]
impl<SlotId> AppOwnedTerminalHostNativeView<SlotId>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    fn new(
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
        native_view: OwnedTerminalHostNativeView,
    ) -> Self {
        Self { plan, native_view }
    }

    pub(crate) fn attachment_plan(&self) -> NativeTerminalSurfaceAttachmentPlan<SlotId> {
        self.plan
    }

    fn real_native_view_handle(&self) -> RealTerminalNativeViewHandle {
        self.native_view.real_native_view_handle()
    }

    pub(crate) fn native_view_handle(&self) -> RealTerminalNativeViewHandle {
        self.real_native_view_handle()
    }

    fn matches_plan_identity(&self, plan: NativeTerminalSurfaceAttachmentPlan<SlotId>) -> bool {
        self.plan.same_attachment_identity(plan)
    }

    fn matches_exact_plan(&self, plan: NativeTerminalSurfaceAttachmentPlan<SlotId>) -> bool {
        self.plan == plan
    }

    fn matches_platform_command(
        &self,
        command: NativeTerminalSurfacePlatformCommand<SlotId>,
    ) -> bool {
        let payload = match command {
            NativeTerminalSurfacePlatformCommand::AttachOrShow { payload }
            | NativeTerminalSurfacePlatformCommand::MoveOrResize { payload }
            | NativeTerminalSurfacePlatformCommand::HideAndDetach { payload }
            | NativeTerminalSurfacePlatformCommand::NoOp { payload } => payload,
        };

        payload.host_id == self.plan.host_id && payload.slot_id == self.plan.slot_id
    }

    pub(crate) fn can_rekey_to_running_attachment_plan(
        &self,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> bool {
        self.plan.slot_id == plan.slot_id
    }

    pub(crate) fn can_move_to_running_attachment_plan(
        &self,
        _plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> bool {
        /*
        CDXC:GPUISidebarGroupFocus 2026-07-10:
        A sidebar terminal selection may move the same Agents session into
        another group. The already-owned AppKit child
        can therefore accept a new typed Agents attachment slot; the caller
        still proves the same shell/runtime session and supplies the new exact
        normal-layout bounds before this owner is shown again.
        */
        true
    }

    pub(crate) fn into_rekeyed_running_host_native_view(
        self,
        plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    ) -> AppOwnedTerminalHostNativeView<SlotId> {
        /*
        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Sleeping wake and popped-out reattach move an existing Running AppKit host owner back to the current Agents body slot instead of creating a replacement view. Rekeying may update only the slot plan and current body bounds while preserving the retained child view and leaving show/focus to the normal Running sync path.

        CDXC:GPUICommandTerminalParkedOwnerReattach 2026-06-27-07:42:
        Command sleeping wake uses the same typed host-owner move for `CommandTerminalBodyMountSlotId`; the host child view is rekeyed only to the same command group/session slot and never recreated, retargeted to Agents, logged, persisted, or backed by fallback host state.
        */
        AppOwnedTerminalHostNativeView::new(plan, self.native_view)
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct AppOwnedTerminalStartupHostNativeView {
    plan: AgentsTerminalStartupLaunchPlan,
    native_view: OwnedTerminalHostNativeView,
}

#[cfg(target_os = "macos")]
impl AppOwnedTerminalStartupHostNativeView {
    fn new(
        plan: AgentsTerminalStartupLaunchPlan,
        native_view: OwnedTerminalHostNativeView,
    ) -> Self {
        Self { plan, native_view }
    }

    pub(crate) fn startup_launch_plan(&self) -> AgentsTerminalStartupLaunchPlan {
        self.plan
    }

    fn real_native_view_handle(&self) -> RealTerminalNativeViewHandle {
        self.native_view.real_native_view_handle()
    }

    fn matches_exact_launch_plan(&self, plan: AgentsTerminalStartupLaunchPlan) -> bool {
        self.plan == plan
    }

    fn matches_startup_host_preservation_key(
        &self,
        key: AgentsTerminalStartupHostPreservationKey,
    ) -> bool {
        self.plan.runtime_session_id == key.runtime_session_id
            && self.plan.startup_body_slot_id == key.startup_body_slot_id
    }

    pub(crate) fn can_transfer_to_running_attachment_plan(
        &self,
        plan: NativeTerminalSurfaceAttachmentPlan,
    ) -> bool {
        self.plan.pane_id == plan.slot_id.pane_id
            && self.plan.shell_session_id == plan.slot_id.session_id
            && self.plan.startup_body_slot_id.pane_id == plan.slot_id.pane_id
            && self.plan.startup_body_slot_id.session_id == plan.slot_id.session_id
            && self.plan.bounds == plan.bounds
    }

    pub(crate) fn into_running_host_native_view(
        self,
        plan: NativeTerminalSurfaceAttachmentPlan,
    ) -> AppOwnedTerminalHostNativeView {
        /*
        CDXC:GPUTerminalStartupHandoff 2026-06-23-04:25:
        The startup host owns the retained AppKit child view until a ready startup surface can become the same Running mount slot. Moving `native_view` into `AppOwnedTerminalHostNativeView` preserves the child view and leaves visibility/focus to the existing Running sync path.
        */
        AppOwnedTerminalHostNativeView::new(plan, self.native_view)
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct AppOwnedTerminalHostFocusIdentity<SlotId = AgentsTerminalBodyMountSlotId> {
    slot_id: SlotId,
    real_view: RealTerminalNativeViewHandle,
}

#[cfg(target_os = "macos")]
impl<SlotId> AppOwnedTerminalHostFocusIdentity<SlotId>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    fn from_app_owned_host_view(owned_host_view: &AppOwnedTerminalHostNativeView<SlotId>) -> Self {
        Self {
            slot_id: owned_host_view.plan.slot_id,
            real_view: owned_host_view.real_native_view_handle(),
        }
    }

    pub(crate) fn slot_id(self) -> SlotId {
        self.slot_id
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn reconcile_app_owned_terminal_host_native_view<SlotId, F>(
    owned_host_views: &mut HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    lifecycle: &mut NativeTerminalSurfaceLifecycleState<SlotId>,
    parent_ns_view: *mut c_void,
    host_commands: &[NativeTerminalSurfaceHostCommand<SlotId>],
    lifecycle_decisions: &[NativeTerminalSurfaceLifecycleDecision<SlotId>],
    mut create_host_view: F,
) -> Vec<AppOwnedTerminalHostFrameOperation>
where
    SlotId: TerminalSurfaceMountSlotKey,
    F: FnMut(
        TerminalHostNativeViewCreateRequest,
    ) -> Result<OwnedTerminalHostNativeView, TerminalHostNativeViewCreateError>,
{
    for command in host_commands {
        match *command {
            NativeTerminalSurfaceHostCommand::AttachOrShow { plan }
            | NativeTerminalSurfaceHostCommand::MoveOrResize { plan }
            | NativeTerminalSurfaceHostCommand::NoOp { plan } => {
                refresh_owned_host_view_plan(owned_host_views, plan);
            }
            NativeTerminalSurfaceHostCommand::HideAndDetach { plan } => {
                drop_owned_host_view_for_plan(owned_host_views, plan);
            }
        }
    }

    for decision in lifecycle_decisions {
        match *decision {
            NativeTerminalSurfaceLifecycleDecision::NeedsRealNativeView { plan } => {
                create_owned_host_view_for_plan(
                    owned_host_views,
                    lifecycle,
                    parent_ns_view,
                    plan,
                    &mut create_host_view,
                );
            }
            NativeTerminalSurfaceLifecycleDecision::DetachStaleView { command, real_view } => {
                drop_owned_host_view_for_lifecycle_detach(owned_host_views, command, real_view);
            }
            NativeTerminalSurfaceLifecycleDecision::CanExecuteWithRealView { .. }
            | NativeTerminalSurfaceLifecycleDecision::NoOp { .. } => {}
        }
    }

    frame_only_operations_for_app_owned_terminal_host_native_view(
        owned_host_views,
        lifecycle_decisions,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn reconcile_app_owned_terminal_startup_host_native_view<F>(
    owned_startup_host_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AppOwnedTerminalStartupHostNativeView,
    >,
    startup_launch_plans: &[AgentsTerminalStartupLaunchPlan],
    startup_host_preservation_keys: &[AgentsTerminalStartupHostPreservationKey],
    parent_ns_view: *mut c_void,
    mut create_host_view: F,
) where
    F: FnMut(
        TerminalHostNativeViewCreateRequest,
    ) -> Result<OwnedTerminalHostNativeView, TerminalHostNativeViewCreateError>,
{
    let current_plans_by_slot = startup_launch_plans
        .iter()
        .copied()
        .map(|plan| (plan.startup_body_slot_id, plan))
        .collect::<HashMap<_, _>>();
    let current_preservation_keys_by_slot = startup_host_preservation_keys
        .iter()
        .copied()
        .map(|key| (key.startup_body_slot_id, key))
        .collect::<HashMap<_, _>>();
    if (unsafe { TerminalHostParentNativeViewHandle::try_from_raw(parent_ns_view) }).is_err() {
        owned_startup_host_views.clear();
        return;
    }

    owned_startup_host_views.retain(|slot_id, owned_host_view| {
        app_owned_terminal_startup_host_native_view_will_survive_reconcile(
            owned_host_view,
            current_plans_by_slot.get(slot_id).copied(),
            current_preservation_keys_by_slot.get(slot_id).copied(),
            parent_ns_view,
        )
    });

    for plan in startup_launch_plans {
        let plan = *plan;
        if !current_plans_by_slot
            .get(&plan.startup_body_slot_id)
            .is_some_and(|current_plan| *current_plan == plan)
            || owned_startup_host_views.contains_key(&plan.startup_body_slot_id)
        {
            continue;
        }

        create_owned_startup_host_view_for_plan(
            owned_startup_host_views,
            parent_ns_view,
            plan,
            &mut create_host_view,
        );
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn app_owned_terminal_startup_host_native_view_will_survive_reconcile(
    owned_host_view: &AppOwnedTerminalStartupHostNativeView,
    startup_launch_plan: Option<AgentsTerminalStartupLaunchPlan>,
    startup_host_preservation_key: Option<AgentsTerminalStartupHostPreservationKey>,
    parent_ns_view: *mut c_void,
) -> bool {
    let Ok(parent_view) =
        (unsafe { TerminalHostParentNativeViewHandle::try_from_raw(parent_ns_view) })
    else {
        return false;
    };
    let bounds = NativeTerminalSurfacePlatformBounds::from_gpui_bounds(owned_host_view.plan.bounds);
    if TerminalHostNativeViewCreateRequest::try_new(parent_view, bounds).is_err() {
        return false;
    }

    if let Some(plan) = startup_launch_plan {
        return owned_host_view.matches_exact_launch_plan(plan);
    }

    startup_host_preservation_key
        .is_some_and(|key| owned_host_view.matches_startup_host_preservation_key(key))
}

#[cfg(target_os = "macos")]
fn create_owned_startup_host_view_for_plan<F>(
    owned_startup_host_views: &mut HashMap<
        AgentsTerminalStartupBodySlotId,
        AppOwnedTerminalStartupHostNativeView,
    >,
    parent_ns_view: *mut c_void,
    plan: AgentsTerminalStartupLaunchPlan,
    create_host_view: &mut F,
) where
    F: FnMut(
        TerminalHostNativeViewCreateRequest,
    ) -> Result<OwnedTerminalHostNativeView, TerminalHostNativeViewCreateError>,
{
    let Ok(parent_view) =
        (unsafe { TerminalHostParentNativeViewHandle::try_from_raw(parent_ns_view) })
    else {
        return;
    };
    let bounds = NativeTerminalSurfacePlatformBounds::from_gpui_bounds(plan.bounds);
    let Ok(request) = TerminalHostNativeViewCreateRequest::try_new(parent_view, bounds) else {
        return;
    };
    let Ok(native_view) = create_host_view(request) else {
        return;
    };

    owned_startup_host_views.insert(
        plan.startup_body_slot_id,
        AppOwnedTerminalStartupHostNativeView::new(plan, native_view),
    );
}

#[cfg(target_os = "macos")]
fn create_owned_host_view_for_plan<SlotId, F>(
    owned_host_views: &mut HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    lifecycle: &mut NativeTerminalSurfaceLifecycleState<SlotId>,
    parent_ns_view: *mut c_void,
    plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
    create_host_view: &mut F,
) where
    SlotId: TerminalSurfaceMountSlotKey,
    F: FnMut(
        TerminalHostNativeViewCreateRequest,
    ) -> Result<OwnedTerminalHostNativeView, TerminalHostNativeViewCreateError>,
{
    if let Some(existing) = owned_host_views.get(&plan.slot_id) {
        if existing.matches_exact_plan(plan) {
            let real_view = existing.real_native_view_handle();
            lifecycle.mark_ready_with_real_native_view(plan, real_view);
            return;
        }
    }

    if owned_host_views
        .get(&plan.slot_id)
        .is_some_and(|existing| !existing.matches_exact_plan(plan))
    {
        owned_host_views.remove(&plan.slot_id);
    }

    let Ok(parent_view) =
        (unsafe { TerminalHostParentNativeViewHandle::try_from_raw(parent_ns_view) })
    else {
        return;
    };
    let bounds = NativeTerminalSurfacePlatformBounds::from_gpui_bounds(plan.bounds);
    let Ok(request) = TerminalHostNativeViewCreateRequest::try_new(parent_view, bounds) else {
        return;
    };
    let Ok(native_view) = create_host_view(request) else {
        return;
    };
    let real_view = native_view.real_native_view_handle();
    if lifecycle.mark_ready_with_real_native_view(plan, real_view) {
        owned_host_views.insert(
            plan.slot_id,
            AppOwnedTerminalHostNativeView::new(plan, native_view),
        );
    }
}

#[cfg(target_os = "macos")]
fn refresh_owned_host_view_plan<SlotId>(
    owned_host_views: &mut HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    if let Some(owned) = owned_host_views.get_mut(&plan.slot_id) {
        if owned.matches_plan_identity(plan) {
            owned.plan = plan;
        }
    }
}

#[cfg(target_os = "macos")]
fn drop_owned_host_view_for_plan<SlotId>(
    owned_host_views: &mut HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    plan: NativeTerminalSurfaceAttachmentPlan<SlotId>,
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    if owned_host_views
        .get(&plan.slot_id)
        .is_some_and(|owned| owned.matches_plan_identity(plan))
    {
        owned_host_views.remove(&plan.slot_id);
    }
}

#[cfg(target_os = "macos")]
fn drop_owned_host_view_for_lifecycle_detach<SlotId>(
    owned_host_views: &mut HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    command: NativeTerminalSurfacePlatformCommand<SlotId>,
    real_view: RealTerminalNativeViewHandle,
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let stale_slot_id = owned_host_views.iter().find_map(|(slot_id, owned)| {
        (owned.matches_platform_command(command) || owned.real_native_view_handle() == real_view)
            .then_some(*slot_id)
    });
    if let Some(stale_slot_id) = stale_slot_id {
        owned_host_views.remove(&stale_slot_id);
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AppOwnedTerminalHostFrameOperation {
    real_view: RealTerminalNativeViewHandle,
    bounds: NativeTerminalSurfacePlatformBounds,
}

#[cfg(target_os = "macos")]
impl AppOwnedTerminalHostFrameOperation {
    fn set_frame(
        real_view: RealTerminalNativeViewHandle,
        bounds: NativeTerminalSurfacePlatformBounds,
    ) -> Self {
        Self { real_view, bounds }
    }

    fn platform_operation(self) -> NativeTerminalSurfacePlatformOperation {
        NativeTerminalSurfacePlatformOperation::SetFrame {
            bounds: self.bounds,
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn frame_only_operations_for_app_owned_terminal_host_native_view<SlotId>(
    owned_host_views: &HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    lifecycle_decisions: &[NativeTerminalSurfaceLifecycleDecision<SlotId>],
) -> Vec<AppOwnedTerminalHostFrameOperation>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    lifecycle_decisions
        .iter()
        .filter_map(|decision| match *decision {
            NativeTerminalSurfaceLifecycleDecision::CanExecuteWithRealView {
                command: NativeTerminalSurfacePlatformCommand::MoveOrResize { payload },
                real_view,
            } => owned_host_views
                .get(&payload.slot_id)
                .filter(|owned_host_view| {
                    real_view == owned_host_view.real_native_view_handle()
                        && owned_host_view.matches_platform_command(
                            NativeTerminalSurfacePlatformCommand::MoveOrResize { payload },
                        )
                })
                .map(|_| AppOwnedTerminalHostFrameOperation::set_frame(real_view, payload.bounds)),
            _ => None,
        })
        .collect()
}

#[cfg(target_os = "macos")]
pub(crate) fn execute_app_owned_terminal_host_frame_operations<SlotId>(
    owned_host_views: &HashMap<SlotId, AppOwnedTerminalHostNativeView<SlotId>>,
    operations: &[AppOwnedTerminalHostFrameOperation],
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    for operation in operations {
        if owned_host_views
            .values()
            .any(|owned| owned.real_native_view_handle() == operation.real_view)
        {
            let executor = NativeTerminalSurfaceAppKitExecutor::new(operation.real_view);
            executor.apply_operation(operation.platform_operation());
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn set_app_owned_terminal_host_native_view_visible<SlotId>(
    owned_host_view: Option<&AppOwnedTerminalHostNativeView<SlotId>>,
    visible: bool,
) where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let Some(owned_host_view) = owned_host_view else {
        return;
    };
    if !visible {
        terminal_ghostty_surface::unregister_native_key_target(
            owned_host_view.real_native_view_handle(),
        );
    }
    let executor =
        NativeTerminalSurfaceAppKitExecutor::new(owned_host_view.real_native_view_handle());
    let operation = if visible {
        NativeTerminalSurfacePlatformOperation::Show
    } else {
        NativeTerminalSurfacePlatformOperation::Hide
    };
    executor.apply_operation(operation);
}

#[cfg(target_os = "macos")]
pub(crate) fn app_owned_terminal_host_focus_identity<SlotId>(
    owned_host_view: Option<&AppOwnedTerminalHostNativeView<SlotId>>,
) -> Option<AppOwnedTerminalHostFocusIdentity<SlotId>>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    owned_host_view.map(AppOwnedTerminalHostFocusIdentity::from_app_owned_host_view)
}

#[cfg(target_os = "macos")]
pub(crate) fn app_owned_terminal_host_contains_responder<SlotId>(
    owned_host_view: &AppOwnedTerminalHostNativeView<SlotId>,
    responder: *mut c_void,
) -> bool
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let native_view = owned_host_view.real_native_view_handle().as_ptr();
    unsafe { GhostexGpuiNativeViewContainsResponder(native_view, responder) }
}

#[cfg(target_os = "macos")]
pub(crate) fn app_owned_terminal_host_focus_should_execute<SlotId>(
    latest_focus_identity: Option<AppOwnedTerminalHostFocusIdentity<SlotId>>,
    next_focus_identity: Option<AppOwnedTerminalHostFocusIdentity<SlotId>>,
    force_focus_handoff: bool,
) -> bool
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    next_focus_identity.is_some()
        && (force_focus_handoff || latest_focus_identity != next_focus_identity)
}

#[cfg(target_os = "macos")]
pub(crate) fn focus_app_owned_terminal_host_native_view<SlotId>(
    owned_host_view: Option<&AppOwnedTerminalHostNativeView<SlotId>>,
) -> Option<AppOwnedTerminalHostFocusIdentity<SlotId>>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let focus_identity = app_owned_terminal_host_focus_identity(owned_host_view)?;
    let executor = NativeTerminalSurfaceAppKitExecutor::new(focus_identity.real_view);
    executor.apply_operation(NativeTerminalSurfacePlatformOperation::Focus);
    Some(focus_identity)
}

#[cfg(target_os = "macos")]
pub(crate) fn ghostty_surface_config_request_for_app_owned_terminal_startup_host_native_view(
    owned_host_view: Option<&AppOwnedTerminalStartupHostNativeView>,
) -> Result<Option<GhosttySurfaceConfigRequest>, GhosttySurfaceConfigRequestError> {
    let Some(owned_host_view) = owned_host_view else {
        return Ok(None);
    };

    GhosttySurfaceConfigRequest::try_from_terminal_native_view(
        owned_host_view.real_native_view_handle(),
        f64::from(owned_host_view.plan.scale_factor),
    )
    .map(Some)
}

#[cfg(target_os = "macos")]
pub(crate) fn ghostty_surface_config_request_for_app_owned_terminal_host_native_view<SlotId>(
    owned_host_view: Option<&AppOwnedTerminalHostNativeView<SlotId>>,
    scale_factor: f64,
) -> Result<Option<GhosttySurfaceConfigRequest>, GhosttySurfaceConfigRequestError>
where
    SlotId: TerminalSurfaceMountSlotKey,
{
    let Some(owned_host_view) = owned_host_view else {
        return Ok(None);
    };

    GhosttySurfaceConfigRequest::try_from_terminal_native_view(
        owned_host_view.real_native_view_handle(),
        scale_factor,
    )
    .map(Some)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NativeTerminalSurfacePlatformOperation {
    SetFrame {
        bounds: NativeTerminalSurfacePlatformBounds,
    },
    Show,
    Hide,
    Focus,
}

pub(crate) fn operations_for_native_terminal_platform_command<SlotId>(
    command: NativeTerminalSurfacePlatformCommand<SlotId>,
) -> Vec<NativeTerminalSurfacePlatformOperation> {
    match command {
        NativeTerminalSurfacePlatformCommand::AttachOrShow { payload } => vec![
            NativeTerminalSurfacePlatformOperation::SetFrame {
                bounds: payload.bounds,
            },
            NativeTerminalSurfacePlatformOperation::Show,
        ],
        NativeTerminalSurfacePlatformCommand::MoveOrResize { payload } => {
            vec![NativeTerminalSurfacePlatformOperation::SetFrame {
                bounds: payload.bounds,
            }]
        }
        NativeTerminalSurfacePlatformCommand::HideAndDetach { .. } => {
            vec![NativeTerminalSurfacePlatformOperation::Hide]
        }
        NativeTerminalSurfacePlatformCommand::NoOp { .. } => Vec::new(),
    }
}

#[cfg(target_os = "macos")]
pub(crate) struct NativeTerminalSurfaceAppKitExecutor {
    native_view: RealTerminalNativeViewHandle,
}

#[cfg(target_os = "macos")]
impl NativeTerminalSurfaceAppKitExecutor {
    pub(crate) fn new(native_view: RealTerminalNativeViewHandle) -> Self {
        Self { native_view }
    }

    pub(crate) fn execute<SlotId>(&self, command: NativeTerminalSurfacePlatformCommand<SlotId>) {
        for operation in operations_for_native_terminal_platform_command(command) {
            self.apply_operation(operation);
        }
    }

    fn apply_operation(&self, operation: NativeTerminalSurfacePlatformOperation) {
        let native_view = self.native_view.as_ptr();

        unsafe {
            match operation {
                NativeTerminalSurfacePlatformOperation::SetFrame { bounds } => {
                    GhostexGpuiTerminalSetNativeViewFrame(
                        native_view,
                        bounds.x,
                        bounds.y,
                        bounds.width,
                        bounds.height,
                    );
                }
                NativeTerminalSurfacePlatformOperation::Show => {
                    GhostexGpuiTerminalShowNativeView(native_view);
                }
                NativeTerminalSurfacePlatformOperation::Hide => {
                    GhostexGpuiTerminalHideNativeView(native_view);
                }
                NativeTerminalSurfacePlatformOperation::Focus => {
                    GhostexGpuiTerminalFocusNativeView(native_view);
                }
            }
        }
    }
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn GhostexGpuiTerminalCreateHostNativeView(
        parent_view: *mut c_void,
        x: c_double,
        y: c_double,
        width: c_double,
        height: c_double,
    ) -> *mut c_void;
    fn GhostexGpuiTerminalDestroyHostNativeView(native_view: *mut c_void);
    fn GhostexGpuiTerminalSetNativeViewFrame(
        native_view: *mut c_void,
        x: c_double,
        y: c_double,
        width: c_double,
        height: c_double,
    );
    fn GhostexGpuiTerminalShowNativeView(native_view: *mut c_void);
    fn GhostexGpuiTerminalHideNativeView(native_view: *mut c_void);
    fn GhostexGpuiTerminalFocusNativeView(native_view: *mut c_void);
    fn GhostexGpuiNativeViewContainsResponder(
        root_native_view: *mut c_void,
        responder: *mut c_void,
    ) -> bool;
}
