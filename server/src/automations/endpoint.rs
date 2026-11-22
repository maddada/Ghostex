use rusqlite::Connection;
use serde_json::{Value, json};

use crate::{
    agents::read_agent_settings,
    domain::{DomainRepository, DomainStateError, read_domain_rpc_params},
    sidebar_hud::read_sidebar_hud,
    storage::open_gxserver_database,
};

use super::*;

pub async fn handle_automation_endpoint(
    runtime: &AutomationRuntime,
    endpoint_path: &str,
    body: &Value,
) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let db = open_gxserver_database(&runtime.paths).map_err(internal_error)?;
    let repository = DomainRepository::new(&db, runtime.server_id.as_str());
    match endpoint_path {
        "/api/readAutomationState" => {
            let project = resolve_automation_project(&repository, &params)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/saveAutomation" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation = normalize_definition_payload(&project, &params)?;
            if automation.model.is_some() || automation.effort.is_some() {
                let project_value = repository
                    .get_project(&automation.project_id)?
                    .ok_or_else(|| DomainStateError::not_found("Automation project not found."))?;
                crate::agents::check_requested_agent_model(
                    &db,
                    &project_value,
                    &automation.agent_id,
                    automation.model.as_deref(),
                    automation.effort.as_deref(),
                )?;
            }
            upsert_automation(&db, &automation)?;
            let stored = resolve_project_by_id(&repository, &automation.project_id)?;
            read_project_automation_state(&repository, &db, &stored)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/deleteAutomation" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            delete_automation(&db, &project, &automation_id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/setAutomationEnabled" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("enabled is required."))?;
            set_automation_enabled(&db, &project, &automation_id, enabled)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/runAutomationNow" => {
            let project = resolve_automation_project(&repository, &params)?;
            let automation_id = read_param_text(&params, "automationId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("automationId is required."))?;
            let automation = read_automation(&db, &project, &automation_id)?;
            let _ = queue_automation_run(runtime, &repository, &db, &automation)?;
            update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/archiveAutomationRun" => {
            let project = resolve_automation_project(&repository, &params)?;
            let run_id = read_param_text(&params, "runId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("runId is required."))?;
            archive_run(
                &db,
                &repository,
                &project,
                &run_id,
                params.get("removeWorktree").and_then(Value::as_bool) == Some(true),
            )?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        "/api/markAutomationRunRead" => {
            let project = resolve_automation_project(&repository, &params)?;
            let run_id = read_param_text(&params, "runId")
                .or_else(|| read_param_text(&params, "sessionId"))
                .ok_or_else(|| DomainStateError::bad_request("runId is required."))?;
            patch_run_read(&db, &project.project_id, &run_id)?;
            read_project_automation_state(&repository, &db, &project)
                .map(|state| json!({ "automationState": state }))
        }
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver automation endpoint."
        ))),
    }
}

#[derive(Clone)]
pub(super) struct ProjectRecord {
    pub(super) name: String,
    pub(super) path: String,
    pub(super) project_id: String,
}

fn read_project_automation_state(
    repository: &DomainRepository<'_>,
    db: &Connection,
    project: &ProjectRecord,
) -> Result<Value, DomainStateError> {
    let projects = repository.list_projects()?;
    let hud = read_sidebar_hud(&projects, Some(project.project_id.as_str()));
    let agents = hud_agents_to_automation_agents(&hud);
    let agent_settings = read_agent_settings(db)?;
    let target_projects = read_automation_target_projects(repository)?;
    let worktree = worktree_availability(project);
    Ok(json!({
        "agents": agents,
        "automations": automations_to_value(read_automations_for_project(db, &project.project_id)?),
        "defaultAgentId": agent_settings.get("defaultPromptAgentId").and_then(Value::as_str).unwrap_or("codex"),
        "projectCanUseWorktrees": worktree.0,
        "projectId": project.project_id,
        "projectName": project.name,
        "projectPath": project.path,
        "projects": target_projects,
        "runs": runs_to_value(read_runs_for_project(db, &project.project_id)?),
        "worktreeUnavailableReason": worktree.1,
    }))
}
