//! A terminal create: the empty-sidebar double-click, a project header's Create a Terminal and New
//! Session row, the New Thread picker's Terminal, the Quick Terminal and the second step of the
//! onboarding's first session, performed in Rust (`createSession`, `createProjectTerminal`,
//! `createQuickTerminal` in the old runtime).
//!
//! The calls are the runtime's: `/api/createSession` on this computer or down a remote machine's
//! tunnel, a created session placed into the user-made group the create named, then the session
//! opened. A project whose folder is gone opens the Missing Project Folder dialog instead, before
//! the call and when the daemon refuses with `projectPathUnavailable`.
//!
//! SEE-ALSO: packages/gx-core/src/session_create/ (target and params),
//! apps/desktop/src/app/gx_store/create/focus_created.rs (the open).

use std::time::Duration;

use ghostex_gx_core::{
    ActiveGroup, CreateTarget, MachineId, ProjectKey, SessionKey, created_session,
    open_remote_session_terminal, terminal_create_params, terminal_create_target,
};
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;
use crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode;

/// `requestRemoteGxserver`'s default timeout.
pub(super) const REMOTE_TIMEOUT: Duration = Duration::from_secs(20);

impl GhostexGpuiApp {
    /// The group a create names when the message names none: the runtime's `activeGroupId`, which
    /// the store's focus mirrors.
    pub(super) fn gx_store_active_group_id(&self) -> Option<String> {
        self.gx_store
            .core
            .focus()
            .active_group
            .as_ref()
            .map(ActiveGroup::to_sidebar_group_id)
    }

    /// `this.activeProjectId`: this computer's active project, never a remote one.
    pub(super) fn gx_store_active_local_project_id(&self) -> Option<String> {
        self.gx_store
            .core
            .focus()
            .active_project
            .as_ref()
            .filter(|project| project.machine.is_local())
            .map(|project| project.project_id.clone())
    }

    /// `createSession(groupId = this.activeGroupId)`. The task resolves when the create has come
    /// back, with the failure a caller that awaited it (the onboarding's first session) reports.
    pub(crate) fn gx_store_create_terminal(
        &mut self,
        group_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        let group_id = group_id
            .map(str::to_string)
            .or_else(|| self.gx_store_active_group_id());
        let active_project = self.gx_store_active_local_project_id();
        self.gx_store.create.counters.terminal_creates += 1;
        match terminal_create_target(group_id.as_deref(), active_project.as_deref()) {
            CreateTarget::Remote { project, subgroup } => {
                self.gx_store_create_remote_terminal(project, subgroup, cx);
                gpui::Task::ready(Ok(()))
            }
            CreateTarget::Local {
                project_id,
                subgroup,
            } => self.gx_store_create_local_terminal(project_id, subgroup, cx),
        }
    }

    fn gx_store_create_local_terminal(
        &mut self,
        project_id: Option<String>,
        subgroup: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<Result<(), String>> {
        if let Some(project_id) = project_id.as_deref()
            && !self.gx_store_ensure_local_project_path_available(project_id, cx)
        {
            return gpui::Task::ready(Ok(()));
        }
        let params = terminal_create_params(project_id.as_deref(), None);
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/createSession", params).await;
            let failure = result.as_ref().err().and_then(|error| {
                (error.code.as_deref() != Some("projectPathUnavailable") || project_id.is_none())
                    .then(|| error.message.clone())
            });
            let _ = this.update(cx, |this, cx| match result {
                Ok(response) => {
                    let Some((created_project, session_id)) =
                        created_session(&response, project_id.as_deref())
                    else {
                        return;
                    };
                    if let (Some(subgroup), Some(created_project)) = (&subgroup, &created_project)
                        && Some(created_project) == project_id.as_ref()
                    {
                        this.gx_store_place_created_session_in_subgroup(
                            &ProjectKey::local(created_project.as_str()),
                            &session_id,
                            subgroup,
                            cx,
                        );
                    }
                    if let Some(created_project) = created_project {
                        this.gx_store_focus_created_session(
                            &created_project,
                            &session_id,
                            false,
                            None,
                            cx,
                        );
                    }
                }
                Err(error) => {
                    // Any other failure was an unhandled rejection: no toast.
                    if let Some(project_id) = project_id.as_deref()
                        && error.code.as_deref() == Some("projectPathUnavailable")
                    {
                        this.gx_store_answer_project_path_unavailable(project_id, cx);
                    }
                }
            });
            failure.map_or(Ok(()), Err)
        })
    }

    fn gx_store_create_remote_terminal(
        &mut self,
        project: ProjectKey,
        subgroup: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(machine_id) = project.machine.remote_id().map(str::to_string) else {
            return;
        };
        let params = terminal_create_params(Some(&project.project_id), None);
        let task = self.start_gpui_remote_sidebar_rpc(
            &machine_id,
            "/api/createSession",
            Some(params),
            REMOTE_TIMEOUT,
            GpuiRemoteSidebarRpcMode::Awaited,
            cx,
        );
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(response) => {
                    let Some((created_project, session_id)) =
                        created_session(&response, Some(&project.project_id))
                    else {
                        return;
                    };
                    let created_project = created_project.unwrap_or(project.project_id.clone());
                    if let Some(subgroup) = &subgroup {
                        this.gx_store_place_created_session_in_subgroup(
                            &project,
                            &session_id,
                            subgroup,
                            cx,
                        );
                    }
                    // `setRemotePresentationSessionFocus` plus `openRemoteSessionTerminal`: the
                    // open's own tab selection moves the remote focus marks (sidebar_remote_focus.rs).
                    let session =
                        SessionKey::remote(machine_id.as_str(), created_project, session_id);
                    let payload = open_remote_session_terminal(
                        &session.to_sidebar_session_id(),
                        false,
                        None,
                        false,
                    );
                    this.receive_sidebar_native_project_path_action_payload(
                        &payload.to_string(),
                        cx,
                    );
                }
                Err(_) => this.gx_store_create_toast(
                    "warning",
                    "Remote session failed",
                    Some("The remote gxserver could not create that session."),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// `moveGpuiWorkspaceSessionToSubgroup` then `persistWorkspaceGroups`, which wrote even when the
    /// group was gone.
    pub(super) fn gx_store_place_created_session_in_subgroup(
        &mut self,
        project: &ProjectKey,
        session_id: &str,
        subgroup: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.gx_store_restore_workspace_groups(cx) {
            self.gx_store.create.counters.placements_unread += 1;
            return;
        }
        let document = self.gx_store.workspace_groups.sync.document().clone();
        let next = document
            .move_session_to_subgroup(
                &project.to_workspace_project_id(),
                session_id,
                Some(subgroup),
                None,
            )
            .unwrap_or(document);
        self.gx_store_edit_workspace_groups(next, cx);
    }

    /// `createQuickTerminal()`: a new projectless chat workspace, then its first running terminal.
    ///
    /// CDXC:AgentLauncher 2026-07-11:
    /// Match macOS createNativeChat: create and focus a new projectless chat workspace first, then
    /// create its initial running terminal through the ordinary gxserver session path.
    pub(super) fn gx_store_create_quick_terminal(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_create_quick_project("terminal", cx, |this, project_id, cx| {
            let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
            this.gx_store_create_terminal(Some(&group_id), cx).detach();
        });
    }

    /// `createProjectTerminal(message)`: a remote project's heading asks the machine for the
    /// terminal in one host operation, a Windows project's goes through the WSL create-and-attach,
    /// and every other one is an ordinary create.
    ///
    /// CDXC:PlatformSupport 2026-07-26:
    /// The project-heading terminal button is an explicit project-scoped create request. On
    /// Windows, keep the WSL gxserver create and attach sequence in the Rust host by handing it
    /// only the clicked local project id; it reuses the same atomic path as GPUI New Terminal.
    /// Remote project headings also stay host-owned: the bounded project reference lets Rust use
    /// one create/start/attach operation instead of creating a row and then serially waking it
    /// before the native tab can appear. Local macOS/Linux projects and subgroup creation keep
    /// their existing flows.
    pub(super) fn gx_store_create_project_terminal(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let group_id = message.get("groupId").and_then(Value::as_str);
        let project = group_id.and_then(ProjectKey::parse_sidebar_group_id);
        if let Some(project) = project
            .as_ref()
            .filter(|project| !project.machine.is_local())
        {
            self.gx_store.create.counters.terminal_creates += 1;
            // `postRemoteProjectNativeAction('openRemoteProjectTerminal', ...)`.
            self.receive_sidebar_native_project_path_action_payload(
                &json!({
                    "action": "openRemoteProjectTerminal",
                    "projectId": project.to_workspace_project_id(),
                    "type": ghostex_gx_core::NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
                    "version": ghostex_gx_core::NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
                })
                .to_string(),
                cx,
            );
            return;
        }
        if !cfg!(target_os = "windows") {
            self.gx_store_create_terminal(group_id, cx).detach();
            return;
        }
        let Some(project) = project.filter(|project| project.machine == MachineId::Local) else {
            self.gx_store_create_toast("warning", "Terminal unavailable", None, cx);
            return;
        };
        self.gx_store.create.counters.terminal_creates += 1;
        self.receive_sidebar_create_project_terminal_payload(
            &json!({
                "projectId": project.project_id,
                "type": "ghostex.gpui.sidebar.createProjectTerminal",
                "version": 1,
            })
            .to_string(),
            cx,
        );
    }

    /// `ensureLocalProjectPathAvailable(projectId)`: a folder the daemon reports as gone opens the
    /// Missing Project Folder dialog instead of a create that would fail.
    pub(super) fn gx_store_ensure_local_project_path_available(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let state = self
            .gx_store
            .core
            .presentation()
            .project(&ProjectKey::local(project_id))
            .and_then(|project| project.path_state.clone());
        match state {
            None | Some(ghostex_gx_core::protocol::PathState::Available) => true,
            Some(_) => {
                self.gx_store_present_missing_project_folder(project_id, cx);
                false
            }
        }
    }

    /// `presentMissingProjectFolder(projectId)`: the dialog, or a toast when the project has no
    /// saved folder to name.
    pub(super) fn gx_store_present_missing_project_folder(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let key = ProjectKey::local(project_id);
        let project = self.gx_store.core.presentation().project(&key).cloned();
        let path = project
            .as_ref()
            .and_then(|project| project.path.clone())
            .filter(|path| !path.trim().is_empty());
        let (Some(project), Some(path)) = (project, path) else {
            self.gx_store_create_toast(
                "warning",
                "Project folder unavailable",
                Some("Ghostex could not resolve this project's saved folder."),
                cx,
            );
            return false;
        };
        let title = self
            .gx_store
            .sidebar_list
            .view()
            .group(&key.to_sidebar_group_id())
            .map(|group| group.core.title.clone())
            .unwrap_or(project.title);
        self.open_app_modal_from_bridge(
            json!({
                "modal": "missingProjectFolder",
                "projectId": project_id,
                "projectName": title,
                "projectPath": path,
                "type": "open",
            }),
            cx,
        );
        true
    }

    /// A create the daemon refused with `projectPathUnavailable`: the dialog, and a fresh snapshot so
    /// the row shows the missing folder (the runtime's `refreshDomainPresentationSnapshotFromClient`).
    pub(super) fn gx_store_answer_project_path_unavailable(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store_present_missing_project_folder(project_id, cx)
            && let Some(client) = self.gx_store.client.as_ref()
        {
            client.request_resubscribe();
        }
    }
}
