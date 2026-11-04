//! Open the native Create Linear Ticket dialog, and open the session it started.
//! SEE-ALSO: apps/desktop/src/app/window/create_linear_ticket_modal.rs (the window and its gxserver
//! calls), packages/gx-core/src/sidebar_actions/open.rs (the `createLinearTicket` project action).
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    /// Opens the dialog for the `createLinearTicket` modal: `projectId` and `projectName`, plus
    /// `projects` (`[{ projectId, name }]`) when the Work page offers several to pick from. An open
    /// without a project is dropped.
    pub(crate) fn open_gpui_create_linear_ticket_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let projects: Vec<(String, String)> = message
            .get("projects")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|project| {
                let project_id = project.get("projectId")?.as_str()?.to_string();
                let name = project
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(&project_id)
                    .to_string();
                Some((project_id, name))
            })
            .collect();
        let Some(project_id) =
            text("projectId").or_else(|| projects.first().map(|(id, _)| id.clone()))
        else {
            return;
        };
        let project_name = text("projectName")
            .or_else(|| {
                projects
                    .iter()
                    .find(|(id, _)| *id == project_id)
                    .map(|(_, name)| name.clone())
            })
            .unwrap_or_else(|| "this project".to_string());
        let (agents, selected_agent) = self.create_linear_ticket_agents(&project_id);
        let config = CreateLinearTicketModalConfig {
            project_id,
            project_name,
            projects,
            agents,
            selected_agent,
            palette: self.gpui_native_dialog_palette(),
        };
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if !matches!(command, CreateLinearTicketModalCommand::Cancel) {
                app.work_view_ticket_created(cx);
            }
            match command {
                CreateLinearTicketModalCommand::Started {
                    project_id,
                    session_id,
                } => {
                    app.gx_store_focus_created_session(&project_id, &session_id, false, None, cx);
                }
                CreateLinearTicketModalCommand::Created { identifier } => {
                    // A GitHub issue is named `#218`, a Linear ticket by its ID.
                    let where_it_is = if identifier.starts_with('#') {
                        "The issue is on GitHub."
                    } else {
                        "The ticket is in Linear."
                    };
                    app.dispatch_gpui_workspace_action_toast(
                        "success",
                        &format!("Created {identifier}"),
                        where_it_is,
                        cx,
                    );
                }
                CreateLinearTicketModalCommand::Cancel => {}
            }
            app.release_native_app_modal_window(GpuiAppModalKind::CreateLinearTicket, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::CreateLinearTicket,
            CREATE_LINEAR_TICKET_MODAL_WIDTH,
            CREATE_LINEAR_TICKET_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| GpuiCreateLinearTicketModalWindow::new(config, host, window, cx))
            },
            cx,
        );
    }

    /// The New session launcher's agents as `(agentId, name)`, and which one it starts by default
    /// (the agent the project's header button starts).
    fn create_linear_ticket_agents(&self, project_id: &str) -> (Vec<(String, String)>, usize) {
        let agents: Vec<(String, String)> = self
            .gx_store
            .runtime_facts
            .hud()
            .and_then(|hud| hud.get("agents"))
            .and_then(serde_json::Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|row| {
                        let agent_id = row.get("agentId")?.as_str()?.to_string();
                        let name = row
                            .get("name")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or(&agent_id)
                            .to_string();
                        Some((agent_id, name))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let place = self
            .gx_store_project_agent_choice_place(&ghostex_gx_core::ProjectKey::local(project_id));
        let selected = crate::app::gx_store::read_agent_choices()
            .candidates(&place)
            .into_iter()
            .find_map(|primary| agents.iter().position(|(id, _)| id == primary))
            .unwrap_or(0);
        (agents, selected)
    }
}
