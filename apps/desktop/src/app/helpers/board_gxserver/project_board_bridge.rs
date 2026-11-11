// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the Kanban project board bridge: workarea URL derivation,
// bridge runtime context.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

#[derive(Clone)]
pub(crate) struct ProjectBoardBridgeRuntimeContext {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) remote_target: Option<GpuiRemoteGxserverRequestTarget>,
}

pub(crate) fn project_board_bridge_runtime_context_from_snapshot(
    snapshot: Option<&GpuiProjectSnapshot>,
) -> Option<ProjectBoardBridgeRuntimeContext> {
    /*
    CDXC:CefRuntime 2026-06-24-11:03:
    Kanban and Automate CEF bridge execution may carry only explicit active-project identity from the live sidebar snapshot. The bridge sends gxserver the current project id/path pair for scoped Beads and automation operations, never derives projects from .git/folders/URLs, never launches bd directly, and never logs or persists the private path.
    */
    let snapshot = snapshot?;
    if snapshot.is_quick_projectless
        || (!snapshot.feature_availability.kanban && !snapshot.feature_availability.automate)
    {
        return None;
    }
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .trim()
        .to_string();
    if project_path.is_empty() {
        return None;
    }
    let active_project_id = snapshot
        .active_project_id
        .as_ref()
        .map(|project_id| project_id.0.clone());
    let remote_reference = active_project_id
        .as_deref()
        .and_then(gpui_remote_project_reference_from_project_id);
    Some(ProjectBoardBridgeRuntimeContext {
        project_id: remote_reference
            .as_ref()
            .map(|reference| reference.project_id.clone())
            .or(active_project_id),
        project_path,
        remote_machine_id: remote_reference.map(|reference| reference.remote_machine_id),
        remote_target: None,
    })
}
