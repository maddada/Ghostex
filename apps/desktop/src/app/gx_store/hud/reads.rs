//! The HUD's gxserver reads, as the runtime made them (`fetchSidebarHud`, `fetchRecentProjects`,
//! `refreshRemoteSidebarHudFromGxserver`). Uses only `gx_rpc` and gx-core, so the GPUI web build
//! can compile this file unchanged.

use serde_json::{Value, json};

use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// `/api/readSidebarHud` of this computer. The active project's own Actions ride along
/// (`commands`), so the read names it.
///
/// CDXC:Projects 2026-08-01:
/// The GPUI sidebar renders showOnProjectRow quick actions on every project row, so the HUD read
/// always asks for the per-project command block.
pub(super) async fn read_local_sidebar_hud(active_project_id: Option<String>) -> Option<Value> {
    let mut params = json!({ "includeAllProjectCommands": true });
    if let Some(project_id) = active_project_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        params["activeProjectId"] = Value::from(project_id);
    }
    gx_rpc(None, "/api/readSidebarHud", params).await.ok()
}

/// A remote machine's `/api/readSidebarHud`, kept only when it carries a command list.
///
/// CDXC:RemoteMachines 2026-08-29:
/// A remote project's Actions are stored by the daemon that owns the project, so the only place
/// they can come from is that machine's own HUD projection. Ask for the per-project command block,
/// exactly like the local HUD read, so every remote project row can render its own quick actions.
pub(super) async fn read_remote_sidebar_hud(
    remote: GpuiRemoteGxserverRequestTarget,
) -> Option<Value> {
    let hud = gx_rpc(
        Some(remote),
        "/api/readSidebarHud",
        json!({ "includeAllProjectCommands": true }),
    )
    .await
    .ok()?;
    hud.get("commands")?.as_array()?;
    Some(hud)
}

/// `/api/listRecentProjects`: the rows, or `None` when the read or its shape failed, which keeps
/// the previous list.
pub(super) async fn read_recent_projects() -> Option<Vec<Value>> {
    gx_rpc(None, "/api/listRecentProjects", json!({}))
        .await
        .ok()?
        .get("recentProjects")?
        .as_array()
        .cloned()
}
