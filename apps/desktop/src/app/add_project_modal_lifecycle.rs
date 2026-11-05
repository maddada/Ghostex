//! Open and host plumbing for the native Add Project dialog.
//! SEE-ALSO: apps/desktop/src/app/window/add_project_modal/ (the window entity and its decision record), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/remote_conn/project_browse_and_add.rs (`run_gpui_add_project_dialog_operation`, which answers every round trip).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the native dialog for the `addProject` open message. Every opener (the sidebar's
    /// empty state and menu, a remote machine's header, Quick Access, the menu bar) sends the
    /// same message; a `machineId` preselects that machine and skips the machine step.
    pub(crate) fn open_gpui_add_project_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let initial_machine_id = message
            .get("machineId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|machine_id| !machine_id.is_empty())
            .map(str::to_string);
        let machines =
            parse_add_project_machine_options(&self.gpui_add_project_dialog_machine_options());
        let config = AddProjectModalConfig {
            machines: machines.clone(),
            initial_machine_id,
            active_project_cwd: None,
            client_platform: gpui_add_project_dialog_local_platform().to_string(),
            palette: self.gpui_native_dialog_palette(),
            clone_job_poll_interval: AddProjectModalConfig::CLONE_JOB_POLL_INTERVAL,
            slow_operation_notice: AddProjectModalConfig::SLOW_OPERATION_NOTICE,
            // A handoff's clone of the session's repository (gx_store/git/export_handoff_remote.rs).
            initial_clone_url: message
                .get("cloneUrl")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|url| !url.is_empty())
                .map(str::to_string),
            new_project: self.gpui_add_project_new_project_config(&machines),
        };
        let host = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_add_project_modal_command(command, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::AddProject,
            ADD_PROJECT_MODAL_WIDTH,
            ADD_PROJECT_MODAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiAddProjectModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// What New project folder's form starts from: each machine's open Space and the selected
    /// project's group and folder (gx_store/added_project.rs), the agents Start offers, and the last
    /// Start picked, else the default agent.
    fn gpui_add_project_new_project_config(
        &mut self,
        machines: &[AddProjectMachineOption],
    ) -> AddProjectNewProjectConfig {
        let placements = machines
            .iter()
            .map(|machine| {
                let remote = (machine.machine_id != GPUI_ADD_PROJECT_DIALOG_LOCAL_MACHINE_ID)
                    .then_some(machine.machine_id.as_str());
                let (placement, parent) = self.gx_store_new_project_placement(remote);
                let choice =
                    |choice: ghostex_gx_core::NewProjectPlaceChoice| AddProjectPlaceChoice {
                        id: choice.id,
                        label: choice.label,
                        space_ids: choice.space_ids,
                    };
                (
                    machine.machine_id.clone(),
                    AddProjectPlacementOptions {
                        spaces: placement
                            .spaces
                            .map(|spaces| spaces.into_iter().map(choice).collect()),
                        groups: placement.groups.into_iter().map(choice).collect(),
                        space_id: placement.space_id,
                        group_id: placement.group_id,
                        selected_project_parent: parent,
                    },
                )
            })
            .collect();
        let agents: Vec<AddProjectStartAgent> = self
            .gx_store_launch_hud()
            .and_then(|hud| hud["agents"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|agent| {
                let agent_id = agent["agentId"].as_str()?.trim();
                let name = agent["name"].as_str().map(str::trim).unwrap_or(agent_id);
                (!agent_id.is_empty()).then(|| AddProjectStartAgent {
                    agent_id: agent_id.to_string(),
                    name: if name.is_empty() { agent_id } else { name }.to_string(),
                })
            })
            .collect();
        let remembered = crate::app::gx_store::read_new_project_start()
            .as_deref()
            .and_then(AddProjectStart::from_wire)
            .filter(|start| match start {
                AddProjectStart::Agent(agent_id) => {
                    agents.iter().any(|agent| &agent.agent_id == agent_id)
                }
                _ => true,
            });
        let default_agent = self.git_default_prompt_agent_id(None);
        let start = remembered.or_else(|| {
            agents
                .iter()
                .any(|agent| agent.agent_id == default_agent)
                .then(|| AddProjectStart::Agent(default_agent))
        });
        AddProjectNewProjectConfig {
            placements,
            agents,
            start,
        }
    }

    fn handle_gpui_add_project_modal_command(
        &mut self,
        command: AddProjectModalCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        let kind = GpuiAppModalKind::AddProject;
        match command {
            AddProjectModalCommand::Request {
                request_id,
                operation,
                machine_id,
                params,
            } => {
                let Some(operation) = GpuiAddProjectDialogOperation::from_wire(operation) else {
                    return;
                };
                let empty = serde_json::Map::new();
                let params = params.as_object().unwrap_or(&empty);
                let added_on =
                    (operation == GpuiAddProjectDialogOperation::Add).then(|| machine_id.clone());
                self.run_gpui_add_project_dialog_operation(
                    operation,
                    Some(machine_id.as_str()),
                    params,
                    move |app, result, cx| {
                        if let (Some(machine_id), Ok(answer)) = (&added_on, &result) {
                            app.git_export_note_added_project(machine_id, answer);
                        }
                        app.update_native_app_modal(
                            kind,
                            cx,
                            |modal: &mut GpuiAddProjectModalWindow, window, cx| {
                                modal.receive_response(request_id, result, window, cx);
                            },
                        );
                    },
                    cx,
                );
            }
            AddProjectModalCommand::Close => {
                self.release_native_app_modal_window(kind, cx);
                // Opened from a handoff's destination: Handoff / Export comes back now.
                self.git_export_add_project_closed(cx);
            }
        }
    }
}
