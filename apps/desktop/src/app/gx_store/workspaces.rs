//! Workspaces in the sidebar: the workspace this window shows (a sidebar input, so gx-core filters
//! the projects and Spaces by it), the workspace tile's menu commands, and Move to workspace.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_menu/workspace.rs (the menus),
//! packages/gx-core/src/sidebar_view/workspaces.rs (the filter), server/src/workspaces/ and
//! server/src/server/route_http/workspaces.rs (the document and its routes),
//! apps/desktop/src/app/workspace_windows/window_workspace.rs (saving it with the window).

use ghostex_gx_core::MachineId;
use ghostex_gx_core::protocol::{SidebarWorkspace, SidebarWorkspacesState};
use serde_json::{Value, json};

use super::rpc::gxserver_rpc_result_task;
use crate::GhostexGpuiApp;

/// How many workspaces a window remembers having shown, newest first.
const MAX_RECENT_WINDOW_WORKSPACES: usize = 8;

/// What the workspace tile draws.
pub(crate) struct WorkspaceTile {
    pub(crate) name: String,
    pub(crate) letter: String,
    pub(crate) color: String,
    /// The workspace a click on the letter switches to; `None` with a single workspace.
    pub(crate) switch_to: Option<(String, String)>,
}

impl GhostexGpuiApp {
    /// This computer's workspaces; `None` until its daemon published them (or an older daemon).
    pub(crate) fn gx_store_workspaces_state(&self) -> Option<&SidebarWorkspacesState> {
        self.gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)?
            .side_state()
            .workspaces
            .as_ref()
    }

    /// The workspace id this window was set to, as saved (`None` = the default workspace).
    pub(crate) fn gx_store_window_workspace_id(&self) -> Option<String> {
        self.gx_store
            .sidebar_list
            .last_inputs
            .host
            .window_workspace_id
            .clone()
    }

    /// The workspace this window shows, resolved against the document (a deleted one reads as the
    /// default); `None` before the document arrived.
    pub(crate) fn gx_store_resolved_window_workspace_id(&self) -> Option<String> {
        let window_workspace_id = self.gx_store_window_workspace_id();
        self.gx_store_workspaces_state()
            .map(|state| state.resolve(window_workspace_id.as_deref()).to_string())
    }

    /// The window's workspace unless it is the default one: what a new Space and the Browser's
    /// profile take, where the default workspace is written as no id.
    pub(crate) fn gx_store_window_non_default_workspace_id(&self) -> Option<String> {
        let state = self.gx_store_workspaces_state()?;
        let window_workspace_id = self.gx_store_window_workspace_id();
        let resolved = state.resolve(window_workspace_id.as_deref());
        (resolved != state.default_id()).then(|| resolved.to_string())
    }

    /// `params` for a route that adds a project, with this window's workspace on it, so the
    /// project lands in the window it was added from (gxserver `place_added_project`).
    pub(crate) fn gx_store_with_window_workspace(&self, mut params: Value) -> Value {
        if let (Some(workspace_id), Some(object)) = (
            self.gx_store_window_non_default_workspace_id(),
            params.as_object_mut(),
        ) {
            object.insert("workspaceId".to_string(), json!(workspace_id));
        }
        params
    }

    /// Whether this computer's project `project_id` belongs to a workspace other than the one this
    /// window shows. `false` for a project the store does not hold, and while the daemon has no
    /// workspaces.
    pub(crate) fn gx_store_project_outside_window_workspace(&self, project_id: &str) -> bool {
        let store = self.gx_store.core.presentation();
        let Some(workspace) = ghostex_gx_core::window_workspace(
            store,
            &self.gx_store.sidebar_list.last_inputs,
            &MachineId::Local,
        ) else {
            return false;
        };
        store
            .loaded(&MachineId::Local)
            .filter(|loaded| loaded.project(project_id).is_some())
            .is_some_and(|loaded| !workspace.shows_project(loaded, project_id))
    }

    /// The workspace this computer's project `project_id` belongs to; `None` while the daemon has
    /// no workspaces, for a project the store does not hold, and for one shown in every workspace.
    pub(crate) fn gx_store_local_project_workspace_id(&self, project_id: &str) -> Option<String> {
        let state = self.gx_store_workspaces_state()?;
        let loaded = self
            .gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)?;
        let project = loaded.project(project_id)?;
        (!project.every_workspace)
            .then(|| ghostex_gx_core::project_workspace_id(state, loaded, project).to_string())
    }

    /// The window that shows an existing project opened from outside (a folder or terminal from
    /// the OS): this one when it shows the project's workspace, else another window that shows it
    /// (brought forward), else this one switched to that workspace.
    ///
    /// CDXC:Workspaces 2026-10-09 DECISION:
    /// User: do not move an existing project between workspaces when it is opened from the OS
    /// (folder or terminal) or by Help; Add Project from a window may keep moving it (that is an
    /// explicit "add here"). For an OS open of a folder that is already a project, keep it in its
    /// own workspace and show it there (switch the window that receives it to that workspace, or
    /// use a window already showing that workspace if there is one). A brand-new folder still lands
    /// in the receiving window's workspace.
    pub(crate) fn gx_store_window_for_existing_project(
        &mut self,
        project_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> gpui::WeakEntity<Self> {
        let own = cx.entity().downgrade();
        let Some(workspace_id) = self.gx_store_local_project_workspace_id(project_id) else {
            return own;
        };
        if self.gx_store_resolved_window_workspace_id().as_deref() == Some(workspace_id.as_str()) {
            return own;
        }
        if let Some(other) = self.other_window_showing_workspace(&workspace_id, cx) {
            return other;
        }
        self.switch_window_workspace(workspace_id, cx);
        own
    }

    pub(crate) fn gx_store_window_workspace(&self) -> Option<&SidebarWorkspace> {
        let state = self.gx_store_workspaces_state()?;
        let window_workspace_id = self.gx_store_window_workspace_id();
        state
            .workspaces
            .get(state.resolve(window_workspace_id.as_deref()))
    }

    /// Sets the window's workspace, and the ones it showed before (newest first), before its
    /// first list is built.
    pub(crate) fn gx_store_init_window_workspace(
        &mut self,
        workspace_id: Option<String>,
        recent_workspace_ids: Vec<String>,
    ) {
        let host = &mut self.gx_store.sidebar_list.last_inputs.host;
        host.window_workspace_id = workspace_id;
        host.window_recent_workspace_ids = recent_workspace_ids;
    }

    /// The workspaces this window showed before the current one, newest first.
    pub(crate) fn gx_store_window_recent_workspace_ids(&self) -> &[String] {
        &self
            .gx_store
            .sidebar_list
            .last_inputs
            .host
            .window_recent_workspace_ids
    }

    /// Switches this window to another workspace: the list is rebuilt on it and the choice is
    /// saved with the window.
    pub(crate) fn gx_store_set_window_workspace(
        &mut self,
        workspace_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store_window_workspace_id() == workspace_id {
            return;
        }
        let outgoing = self.gx_store_resolved_window_workspace_id();
        let incoming = self
            .gx_store_workspaces_state()
            .map(|state| state.resolve(workspace_id.as_deref()).to_string());
        let host = &mut self.gx_store.sidebar_list.last_inputs.host;
        if let Some(outgoing) = outgoing.filter(|outgoing| Some(outgoing) != incoming.as_ref()) {
            let recent = &mut host.window_recent_workspace_ids;
            recent.retain(|id| *id != outgoing && Some(id) != incoming.as_ref());
            recent.insert(0, outgoing);
            recent.truncate(MAX_RECENT_WINDOW_WORKSPACES);
        }
        host.window_workspace_id = workspace_id;
        self.persist_window_workspace_id();
        self.gx_store_sidebar_state_changed(cx);
        cx.notify();
    }

    /// The tile left of the Spaces row, when this computer's daemon has workspaces.
    pub(crate) fn gx_store_workspace_tile(&self) -> Option<WorkspaceTile> {
        let state = self.gx_store_workspaces_state()?;
        let workspace = self.gx_store_window_workspace()?;
        let switch_to = ghostex_gx_core::workspace_switch_target(
            state,
            &workspace.workspace_id,
            self.gx_store_window_recent_workspace_ids(),
        )
        .map(|target| (target.workspace_id.clone(), target.name.clone()));
        Some(WorkspaceTile {
            name: workspace.name.clone(),
            letter: workspace.letter.clone(),
            color: workspace.color.clone(),
            switch_to,
        })
    }

    /// Shows `workspace_id` from this window: another window that already shows it comes forward,
    /// else this window switches to it.
    pub(crate) fn gx_store_select_workspace(
        &mut self,
        workspace_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.gx_store_resolved_window_workspace_id().as_deref() == Some(&workspace_id) {
            return;
        }
        if !self.focus_window_showing_workspace(&workspace_id, cx) {
            self.switch_window_workspace(workspace_id, cx);
        }
    }

    /// The tile's menu, as the sidebar menu JSON. Built when it opens, because its new-window
    /// buttons depend on which workspaces the other windows show at that moment.
    pub(crate) fn gx_store_workspace_menu(&self, cx: &gpui::Context<Self>) -> Option<Value> {
        let state = self.gx_store_workspaces_state()?;
        let workspace = self.gx_store_window_workspace()?;
        let in_windows = self.workspaces_shown_in_other_windows(cx);
        Some(ghostex_gx_core::menu_to_json(
            &ghostex_gx_core::workspace_menu(state, &workspace.workspace_id, &in_windows),
        ))
    }

    /// The workspace tile's menu commands and a project's Move to workspace rows.
    pub(crate) fn gx_store_run_workspaces(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let text = |key: &str| {
            command
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        match command.get("type").and_then(Value::as_str) {
            Some("selectWorkspace") => {
                if let Some(workspace_id) = text("workspaceId") {
                    self.gx_store_select_workspace(workspace_id, cx);
                }
                true
            }
            Some("moveMachineToWorkspace") => {
                if let (Some(machine_id), Some(workspace_id)) =
                    (text("machineId"), text("workspaceId"))
                {
                    self.gx_store_move_machine_to_workspace(machine_id, workspace_id, cx);
                }
                true
            }
            Some("openWorkspaceWindow") => {
                // The row's new-window button does not close the menu by itself.
                self.dismiss_native_sidebar_menu(cx);
                if let Some(workspace_id) = text("workspaceId") {
                    self.open_workspace_in_new_window(&workspace_id, cx);
                }
                true
            }
            Some("openWorkspaceSettings") => {
                self.open_workspace_settings_page(cx);
                true
            }
            Some("newWorkspace") => {
                let move_project_id = text("moveProjectId");
                self.gx_store_create_workspace(move_project_id, cx);
                true
            }
            Some("command")
                if command["message"].get("type").and_then(Value::as_str)
                    == Some("moveProjectToWorkspace") =>
            {
                let message = &command["message"];
                let (Some(project_id), Some(workspace_id)) = (
                    message.get("projectId").and_then(Value::as_str),
                    message.get("workspaceId").and_then(Value::as_str),
                ) else {
                    return true;
                };
                self.gx_store_move_project_to_workspace(
                    project_id.to_string(),
                    workspace_id.to_string(),
                    cx,
                );
                true
            }
            _ => false,
        }
    }

    fn gx_store_move_project_to_workspace(
        &mut self,
        project_id: String,
        workspace_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/moveProjectToWorkspace",
                json!({ "projectId": project_id, "workspaceId": workspace_id }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't move the project",
                        &message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// Move to workspace on a remote machine's tab: this computer's daemon keeps the assignment
    /// (`sidebarWorkspaces.machineWorkspaces`) and every window follows it when it arrives
    /// (gx-core `window_machine_tabs`).
    fn gx_store_move_machine_to_workspace(
        &mut self,
        machine_id: String,
        workspace_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/moveMachineToWorkspace",
                json!({ "machineId": machine_id, "workspaceId": workspace_id }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            if let Err(message) = result {
                let _ = this.update(cx, |this, cx| {
                    this.dispatch_gpui_workspace_action_toast(
                        "error",
                        "Couldn't move the machine",
                        &message,
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// The tab menu's "Move to workspace ▸" for a remote machine, as sidebar menu JSON; `None`
    /// while this computer's daemon has no workspaces or has only one.
    pub(crate) fn gx_store_machine_workspace_menu(&self, machine_id: &str) -> Option<Value> {
        let state = self.gx_store_workspaces_state()?;
        if state.workspaces.len() < 2 {
            return None;
        }
        let menu = ghostex_gx_core::menu_to_json(&[ghostex_gx_core::machine_workspace_menu(
            state, machine_id,
        )]);
        menu.as_array()?.first().cloned()
    }

    /// "New workspace…": creates a Work workspace, moves the project the menu was opened on into
    /// it, and opens the Workspaces settings page to name it and set it up.
    fn gx_store_create_workspace(
        &mut self,
        move_project_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let created = gxserver_rpc_result_task(
                &background,
                "/api/createWorkspace",
                json!({ "name": "New workspace", "kind": "work" }),
                super::sidebar_lifecycle::rpc_timeout(),
            )
            .await;
            let workspace_id = match created {
                Ok(result) => result
                    .get("workspaceId")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                Err(message) => {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't create the workspace",
                            &message,
                            cx,
                        );
                    });
                    return;
                }
            };
            if let (Some(project_id), Some(workspace_id)) = (move_project_id, workspace_id) {
                let moved = gxserver_rpc_result_task(
                    &background,
                    "/api/moveProjectToWorkspace",
                    json!({ "projectId": project_id, "workspaceId": workspace_id }),
                    super::sidebar_lifecycle::rpc_timeout(),
                )
                .await;
                if let Err(message) = moved {
                    let _ = this.update(cx, |this, cx| {
                        this.dispatch_gpui_workspace_action_toast(
                            "error",
                            "Couldn't move the project",
                            &message,
                            cx,
                        );
                    });
                }
            }
            let _ = this.update(cx, |this, cx| this.open_workspace_settings_page(cx));
        })
        .detach();
    }
}
