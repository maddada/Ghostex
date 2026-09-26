//! A picked folder: the Add Project folder pick, the Missing Project Folder dialog's Locate, and the
//! onboarding's Finish, performed in Rust (`handleGpuiWorkspaceFolderPicked` and
//! `relocateProjectFolder` in the old runtime).
//!
//! The calls are the runtime's: `/api/relocateProject` then a fresh snapshot, the dialog closed and
//! a toast; `/api/addProjectPath` then the project opened the way the menu bar opens one (its last
//! session, or the default agent when it has none); and for the onboarding, the project made active
//! and its first session created with the agent the Get Started page chose, the dialog told how it
//! went.
//!
//! CDXC:Onboarding 2026-08-24:
//! Onboarding Finish lands the user in a working workspace: the project it just registered gets its
//! first session immediately, using the default agent chosen on the Get Started page ('terminal'
//! means a plain shell).
//!
//! SEE-ALSO: apps/desktop/src/app/sidebar_dispatch.rs (`dispatch_gpui_workspace_folder_picked_message`,
//! `handle_gpui_first_launch_create_project_session_message`).

use ghostex_gx_core::ProjectKey;
use serde_json::{Value, json};

use super::super::gx_rpc;
use crate::GhostexGpuiApp;

/// The two picked-folder messages, read the way `normalizeGpuiWorkspaceFolderPick` and
/// `normalizeGpuiReplacementProjectFolderPick` read them.
enum FolderPick {
    Replacement {
        path: String,
        project_id: String,
    },
    Workspace {
        first_launch_agent_id: Option<String>,
        name: Option<String>,
        path: String,
        request_id: Option<String>,
    },
}

fn text(message: &Value, key: &str) -> Option<String> {
    message
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

fn read_pick(message: &Value) -> Option<FolderPick> {
    match message.get("type").and_then(Value::as_str)? {
        "replacementProjectFolderPicked" => Some(FolderPick::Replacement {
            path: text(message, "path")?,
            project_id: text(message, "projectId")?,
        }),
        "workspaceFolderPicked" => Some(FolderPick::Workspace {
            first_launch_agent_id: text(message, "firstLaunchAgentId"),
            name: text(message, "name"),
            path: text(message, "path")?,
            request_id: text(message, "requestId"),
        }),
        _ => None,
    }
}

impl GhostexGpuiApp {
    /// A picked folder. Returns whether it was one of the two messages.
    pub(crate) fn gx_store_workspace_folder_picked(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(pick) = read_pick(message) else {
            return false;
        };
        self.gx_store.create.counters.folder_picks += 1;
        match pick {
            FolderPick::Replacement { path, project_id } => {
                self.gx_store_relocate_project_folder(project_id, path, cx);
            }
            FolderPick::Workspace {
                first_launch_agent_id,
                name,
                path,
                request_id,
            } => {
                self.gx_store_add_picked_project(first_launch_agent_id, name, path, request_id, cx)
            }
        }
        true
    }

    /// `relocateProjectFolder(projectId, path)`.
    fn gx_store_relocate_project_folder(
        &mut self,
        project_id: String,
        path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(
                None,
                "/api/relocateProject",
                json!({ "path": path, "projectId": project_id }),
            )
            .await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(_) => {
                    // `refreshDomainPresentationSnapshotFromClient('patch')`: the row shows the
                    // folder it has now.
                    if let Some(client) = this.gx_store.client.as_ref() {
                        client.request_resubscribe();
                    }
                    this.close_app_modal_from_bridge(cx);
                    this.gx_store_create_toast("info", "Project folder updated", None, cx);
                }
                Err(error) => this.gx_store_create_toast(
                    "error",
                    "Could not update project folder",
                    Some(&error.message),
                    cx,
                ),
            });
        })
        .detach();
    }

    /// The Add Project pick and the onboarding's Finish.
    fn gx_store_add_picked_project(
        &mut self,
        first_launch_agent_id: Option<String>,
        name: Option<String>,
        path: String,
        request_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = match &name {
            Some(name) => json!({ "name": name, "path": path }),
            None => json!({ "path": path }),
        };
        cx.spawn(async move |this, cx| {
            let result = gx_rpc(None, "/api/addProjectPath", params).await;
            let project_id = result.as_ref().ok().and_then(|response| {
                response
                    .get("project")
                    .and_then(|project| project.get("projectId"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
            let error = match (&result, &project_id) {
                (Err(error), _) => Some(error.message.clone()),
                (Ok(_), None) => Some("gxserver did not return the added project.".to_string()),
                _ => None,
            };
            let created = this.update(cx, |this, cx| {
                if let Some(error) = error {
                    this.gx_store_add_project_failed(request_id.as_deref(), &error, cx);
                    return None;
                }
                let project_id = project_id?;
                let Some(agent_id) = first_launch_agent_id else {
                    // `activateGpuiProject(project)`, which is what the menu bar's project row
                    // runs.
                    this.dispatch_gpui_menu_bar_project_activation(&project_id, cx);
                    return None;
                };
                this.gx_store_first_launch_session(&project_id, &agent_id, request_id.clone(), cx)
            });
            let Ok(Some(task)) = created else {
                return;
            };
            let outcome = task.await;
            let _ = this.update(cx, |this, cx| match outcome {
                Ok(()) => {
                    if let Some(request_id) = request_id.as_deref() {
                        this.dispatch_gpui_first_launch_create_project_session_result(
                            request_id, true, None, cx,
                        );
                    }
                }
                Err(error) => this.gx_store_add_project_failed(request_id.as_deref(), &error, cx),
            });
        })
        .detach();
    }

    /// The onboarding's first session: the project made active, then its first session. `None`
    /// when the Windows host answers the dialog itself.
    fn gx_store_first_launch_session(
        &mut self,
        project_id: &str,
        agent_id: &str,
        request_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<Result<(), String>>> {
        let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
        // `focusProjectId(project)` and the snapshot refresh that follows it.
        self.dispatch_native_sidebar_command(
            json!({ "type": "focusGroup", "groupId": group_id }),
            cx,
        );
        if let Some(client) = self.gx_store.client.as_ref() {
            client.request_resubscribe();
        }
        if cfg!(target_os = "windows")
            && let Some(request_id) = request_id
        {
            let payload = if agent_id == "terminal" {
                json!({
                    "projectId": project_id,
                    "requestId": request_id,
                    "type": "ghostex.gpui.sidebar.createProjectTerminal",
                    "version": 1,
                })
            } else {
                json!({
                    "agentId": agent_id,
                    "preferredInterface": self.gx_store_preferred_interface(agent_id),
                    "projectId": project_id,
                    "requestId": request_id,
                    "type": "ghostex.gpui.sidebar.createProjectAgent",
                    "version": 1,
                })
            };
            if agent_id == "terminal" {
                self.receive_sidebar_create_project_terminal_payload(&payload.to_string(), cx);
            } else {
                self.receive_sidebar_create_project_agent_payload(&payload.to_string(), cx);
            }
            return None;
        }
        Some(match agent_id {
            "terminal" => self.gx_store_create_terminal(Some(&group_id), cx),
            agent_id => self.gx_store_create_agent_session(agent_id, Some(&group_id), None, cx),
        })
    }

    /// The pick failed: the onboarding dialog is told why, and the toast says the add failed.
    fn gx_store_add_project_failed(
        &mut self,
        request_id: Option<&str>,
        error: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(request_id) = request_id {
            self.dispatch_gpui_first_launch_create_project_session_result(
                request_id,
                false,
                Some(error),
                cx,
            );
        }
        self.gx_store_create_toast(
            "error",
            "Add Project failed",
            Some("Ghostex could not add the selected folder."),
            cx,
        );
    }
}
