//! A project's Remove (Settings > Projects, the missing-folder dialog, a worktree project's menu)
//! and Close (a project's menu), performed in Rust.
//!
//! CDXC:Projects 2026-09-25 WHY:
//! The old runtime resolved the ids and made one call: `/api/removeProject` or
//! `/api/closeProjectToRecent` on this computer, `/api/removeProject` down a remote machine's
//! tunnel. The daemon's presentation drops a removed or parked project for every client
//! (`should_include_presentation_project`), so nothing local has to be edited by hand here: the
//! store and every other client learn it from the stream.
//!
//! CDXC:RemoteMachines 2026-09-25 WHY:
//! A remote project's Close now parks it on the REMOTE daemon (`/api/closeProjectToRecent`), the
//! same call Quick Access's Close already made. It used to park a machine-scoped row in this
//! app's client storage (`ghostex-gpui-remote-recent-projects`, the 2026-06-27 "client-local
//! parking" note), which only the runtime's HUD read and nothing could restore once the runtime's
//! Recent Projects arm was gone, while Quick Access showed and restored the machine's own list.
//! One model, the daemon's, is what every client and the web build can share.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_close_project.rs (the successor Close names),
//! packages/gx-core/src/sidebar_actions/resolve.rs (`local_project_group_project_id`).

use std::time::Duration;

use ghostex_gx_core::{ProjectKey, local_project_group_project_id};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// `requestRemoteGxserver`'s default timeout.
const REMOTE_TIMEOUT: Duration = Duration::from_secs(20);

impl GhostexGpuiApp {
    /// `removeProject(projectId)`: a machine-scoped id goes down that machine's tunnel.
    pub(super) fn gx_store_remove_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project) = ProjectKey::parse_workspace_project_id(project_id) else {
            return;
        };
        self.gx_store.create.counters.project_removes += 1;
        if let Some(machine_id) = project.machine.remote_id() {
            let machine_id = machine_id.to_string();
            self.gx_store_remove_remote_project(&machine_id, &project.project_id, cx);
            return;
        }
        // A failure was an unhandled rejection in the runtime: no toast, and the row stays.
        cx.spawn(async move |_, _| {
            let _ = gx_rpc(
                None,
                "/api/removeProject",
                json!({ "projectId": project.project_id }),
            )
            .await;
        })
        .detach();
    }

    /// `removeProjectForGroup(groupId)`: a project's own group, on this computer or a machine.
    pub(super) fn gx_store_remove_project_for_group(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            return;
        };
        let Some(machine_id) = project.machine.remote_id().map(str::to_string) else {
            self.gx_store_remove_project(&project.project_id, cx);
            return;
        };
        // `resolveRemotePresentationProjectScope`: the machine must be streaming this project.
        if self
            .gx_store
            .core
            .presentation()
            .project(&project)
            .is_none()
        {
            self.gx_store_create_toast(
                "warning",
                "Remote project removal unavailable",
                Some("Reconnect the remote machine before removing the project."),
                cx,
            );
            return;
        }
        self.gx_store.create.counters.project_removes += 1;
        self.gx_store_remove_remote_project(&machine_id, &project.project_id, cx);
    }

    fn gx_store_remove_remote_project(
        &mut self,
        machine_id: &str,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let task = self.start_gpui_remote_sidebar_rpc(
            machine_id,
            "/api/removeProject",
            Some(json!({ "projectId": project_id })),
            REMOTE_TIMEOUT,
            GpuiRemoteSidebarRpcMode::Awaited,
            cx,
        );
        cx.spawn(async move |this, cx| {
            if task.await.is_err() {
                let _ = this.update(cx, |this, cx| {
                    this.gx_store_create_toast(
                        "warning",
                        "Remote project removal failed",
                        Some("The remote gxserver could not remove that project."),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// `closeProjectForGroup(groupId, successorSessionId)`. The successor, when the store named one
    /// (sidebar_close_project.rs), is focused first, so the active project moves straight to it.
    ///
    /// CDXC:Projects 2026-09-16 DECISION:
    /// User: closing a project in a Space stays in that Space and selects a non-sleeping session from the next project in the list.
    /// The successor is picked from the Space the user is in; it is focused before the park so the active project moves straight to it and never passes through the "no active project" state, which is what used to let the host land on a project outside the Space.
    pub(super) fn gx_store_close_project_for_group(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(group_id) = message.get("groupId").and_then(Value::as_str) else {
            return;
        };
        if let Some(successor) = message
            .get("successorSessionId")
            .and_then(Value::as_str)
            .filter(|successor| !successor.is_empty())
        {
            self.dispatch_native_sidebar_command(
                json!({ "type": "focusSession", "sessionId": successor }),
                cx,
            );
        }
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            return;
        };
        if let Some(machine_id) = project.machine.remote_id().map(str::to_string) {
            if self
                .gx_store
                .core
                .presentation()
                .project(&project)
                .is_none()
            {
                self.gx_store_create_toast(
                    "warning",
                    "Remote project close unavailable",
                    Some("Reconnect the remote machine before closing the project."),
                    cx,
                );
                return;
            }
            self.gx_store.create.counters.project_closes += 1;
            self.start_gpui_remote_sidebar_rpc(
                &machine_id,
                "/api/closeProjectToRecent",
                Some(json!({ "projectId": project.project_id })),
                REMOTE_TIMEOUT,
                GpuiRemoteSidebarRpcMode::FireAndForget,
                cx,
            )
            .detach();
            return;
        }
        // `resolveProjectIdForGroup`: a project the sidebar has a group for.
        let Some(project_id) = local_project_group_project_id(
            &self.gx_store.core,
            &self.gx_store.sidebar_list.last_inputs,
            group_id,
        ) else {
            return;
        };
        self.gx_store.create.counters.project_closes += 1;
        // CDXC:Projects 2026-06-24-12:38:
        // GPUI reuses SidebarApp's macOS close/remove split. Close must call the gxserver park endpoint with the project id resolved from the live presentation group, then consume gxserver's authoritative parked row; never synthesize a Recent Project row or map Close to hard delete when resolution or the daemon mutation fails.
        cx.spawn(async move |_, _| {
            let _ = gx_rpc(
                None,
                "/api/closeProjectToRecent",
                json!({ "projectId": project_id }),
            )
            .await;
        })
        .detach();
    }

    /// `createAppToastRequest(level, title, description)` on the app modal host.
    pub(super) fn gx_store_create_toast(
        &mut self,
        level: &str,
        title: &str,
        description: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut request = json!({ "level": level, "title": title, "type": "toast" });
        if let Some(description) = description {
            request["description"] = json!(description);
        }
        self.receive_gpui_app_toast_bridge_message(&request, cx);
    }
}
