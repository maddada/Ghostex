// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the Automations board navigation, scope, and gxserver
// endpoint/response helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::time::Duration;

use crate::app::helpers::*;
use crate::*;

pub(crate) enum GpuiAutomationBoardNavigation {
    FocusSession(String),
    FocusProject(String),
    RevealWorktreePath(String),
}

pub(crate) const GPUI_QUICK_AUTOMATIONS_PROJECT_ID: &str = "quick-automations";
pub(crate) const GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE: &str = "Automations Overview";

pub(crate) fn gpui_automation_gxserver_endpoint_for_board_action(
    action: &str,
) -> Option<&'static str> {
    // macOS `gxserverAutomationEndpointForProjectBoardAction` parity.
    match action {
        "automationGetState" => Some("/api/readAutomationState"),
        "automationSave" => Some("/api/saveAutomation"),
        "automationDelete" => Some("/api/deleteAutomation"),
        "automationRunNow" => Some("/api/runAutomationNow"),
        "automationSetEnabled" => Some("/api/setAutomationEnabled"),
        "automationArchiveRun" => Some("/api/archiveAutomationRun"),
        "automationMarkRunRead" => Some("/api/markAutomationRunRead"),
        _ => None,
    }
}

pub(crate) fn gpui_automation_board_ok_response(
    request_id: &str,
    automation_state: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "payload": automation_state,
        "requestId": request_id,
    })
}

pub(crate) fn gpui_project_board_error_response(
    request_id: &str,
    error: &str,
) -> serde_json::Value {
    serde_json::json!({
        "error": error,
        "ok": false,
        "requestId": request_id,
    })
}

pub(crate) fn gpui_project_board_conversation_action_forwarded(action: &str) -> bool {
    // The board conversation surface owned by the sidebar runtime — the same
    // action set macOS `handleProjectBoardRequest` serves in native-sidebar.tsx
    // (minus the automation family, which Rust forwards to gxserver directly,
    // and `projectEditorFocusOwnerChanged`, which stays Rust-answered).
    matches!(
        action,
        "getState"
            | "startWork"
            | "associateFocusedSession"
            | "jumpToConversation"
            | "unlinkConversation"
            | "showToast"
            | "appendDebugLog"
    )
}

pub(crate) fn gpui_automation_rpc_automation_state(
    endpoint: &str,
    params: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (status_code, body) =
        gxserver_post_typed_operation(endpoint, params, Duration::from_secs(60))?;
    if !(200..300).contains(&status_code) {
        return Err(gpui_automation_gxserver_error_message(&body, status_code));
    }
    parse_gpui_gxserver_rpc_result(&body)?
        .get("automationState")
        .cloned()
        .ok_or_else(|| "gxserver did not return an automation state.".to_string())
}

pub(crate) fn gpui_automation_gxserver_error_message(body: &str, status_code: u16) -> String {
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .or_else(|| value.get("message").and_then(serde_json::Value::as_str))
        {
            return message.to_string();
        }
    }
    format!("gxserver automation request failed with HTTP {status_code}.")
}

pub(crate) fn gpui_list_gxserver_domain_projects() -> Result<Vec<serde_json::Value>, String> {
    let result = gpui_gxserver_rpc_result(
        "/api/listProjects",
        &serde_json::json!({}),
        Duration::from_secs(30),
    )?;
    Ok(result
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default())
}

pub(crate) fn gpui_find_gxserver_project_id_by_path(path: &str) -> Result<Option<String>, String> {
    let normalized = gpui_normalized_project_path_for_comparison(path);
    Ok(gpui_list_gxserver_domain_projects()?
        .iter()
        .find(|project| {
            project
                .get("path")
                .and_then(serde_json::Value::as_str)
                .map(gpui_normalized_project_path_for_comparison)
                .is_some_and(|candidate| candidate == normalized)
        })
        .and_then(|project| project.get("projectId").and_then(serde_json::Value::as_str))
        .map(str::to_string))
}

pub(crate) struct GpuiAutomationBoardScope {
    pub(crate) project_id: Option<String>,
    pub(crate) project_path: Option<String>,
}

pub(crate) fn gpui_automation_board_scope(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> GpuiAutomationBoardScope {
    // macOS `createGxserverAutomationParams` scope resolution: the request's
    // ids win (the overview page targets other projects), the active board
    // project is the fallback.
    let request_project_id = manage_request_string(request, "projectId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let request_project_path = manage_request_string(request, "projectPath")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    GpuiAutomationBoardScope {
        project_id: request_project_id
            .or_else(|| context.and_then(|context| context.project_id.clone())),
        project_path: request_project_path
            .or_else(|| context.map(|context| context.project_path.clone())),
    }
}

pub(crate) fn gpui_automation_scope_params(
    scope: &GpuiAutomationBoardScope,
) -> serde_json::Map<String, serde_json::Value> {
    let mut params = serde_json::Map::new();
    if let Some(project_id) = &scope.project_id {
        params.insert(
            "projectId".to_string(),
            serde_json::Value::String(project_id.clone()),
        );
    }
    if let Some(project_path) = &scope.project_path {
        params.insert(
            "projectPath".to_string(),
            serde_json::Value::String(project_path.clone()),
        );
    }
    params
}

pub(crate) fn gpui_automation_payload_json(
    request: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let payload_json = manage_request_string(request, "payloadJson")
        .ok_or_else(|| "No automation payload was supplied.".to_string())?;
    serde_json::from_str::<serde_json::Value>(&payload_json)
        .map_err(|_| "Automation payload is not valid JSON.".to_string())
}

pub(crate) fn gpui_automation_enabled_from_payload(
    request: &serde_json::Value,
) -> Result<bool, String> {
    let payload = gpui_automation_payload_json(request)
        .map_err(|_| "No automation enabled payload was supplied.".to_string())?;
    payload
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| "Automation enabled payload is incomplete.".to_string())
}

pub(crate) fn gpui_automation_remove_worktree_from_payload(request: &serde_json::Value) -> bool {
    gpui_automation_payload_json(request)
        .ok()
        .and_then(|payload| {
            payload
                .get("removeWorktree")
                .and_then(serde_json::Value::as_bool)
        })
        == Some(true)
}

pub(crate) fn run_gpui_project_board_automation_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> (serde_json::Value, Option<GpuiAutomationBoardNavigation>) {
    let request_id = manage_request_string(request, "requestId").unwrap_or_default();
    match gpui_project_board_automation_result(request, context) {
        Ok((automation_state, navigation)) => (
            gpui_automation_board_ok_response(&request_id, automation_state),
            navigation,
        ),
        Err(error) => (gpui_project_board_error_response(&request_id, &error), None),
    }
}

pub(crate) fn gpui_project_board_automation_result(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<(serde_json::Value, Option<GpuiAutomationBoardNavigation>), String> {
    let action = manage_request_string(request, "action").unwrap_or_default();
    let scope = gpui_automation_board_scope(request, context);
    if action == "automationGetAllState" {
        return Ok((gpui_automation_all_projects_state(&scope)?, None));
    }
    if action == "automationOpenRunSession" || action == "automationOpenWorktree" {
        return gpui_automation_open_run_target(&action, request, &scope);
    }
    let endpoint = gpui_automation_gxserver_endpoint_for_board_action(&action)
        .ok_or_else(|| format!("Unsupported automation action: {action}"))?;
    let mut params = gpui_automation_scope_params(&scope);
    match action.as_str() {
        "automationSave" => {
            params.insert(
                "definition".to_string(),
                gpui_automation_payload_json(request)?,
            );
        }
        "automationDelete" | "automationRunNow" | "automationSetEnabled" => {
            let automation_id = manage_request_string(request, "sessionId")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "No automation id was supplied.".to_string())?;
            params.insert(
                "automationId".to_string(),
                serde_json::Value::String(automation_id),
            );
        }
        "automationArchiveRun" | "automationMarkRunRead" => {
            let run_id = manage_request_string(request, "sessionId")
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| "No automation run id was supplied.".to_string())?;
            params.insert("runId".to_string(), serde_json::Value::String(run_id));
        }
        _ => {}
    }
    if action == "automationSetEnabled" {
        params.insert(
            "enabled".to_string(),
            serde_json::Value::Bool(gpui_automation_enabled_from_payload(request)?),
        );
    }
    if action == "automationArchiveRun" {
        params.insert(
            "removeWorktree".to_string(),
            serde_json::Value::Bool(gpui_automation_remove_worktree_from_payload(request)),
        );
    }
    let state = gpui_automation_rpc_automation_state(endpoint, &serde_json::Value::Object(params))?;
    Ok((state, None))
}

pub(crate) fn gpui_automation_all_projects_state(
    scope: &GpuiAutomationBoardScope,
) -> Result<serde_json::Value, String> {
    /*
    macOS `createAllProjectGxserverAutomationsBridgeState` parity for a client
    with no native project registry: aggregate per-project gxserver automation
    states across the daemon's registered target projects. Agents come from the
    per-project states (the daemon builds them from the same HUD source macOS
    seeds from), and per-project read failures skip that project like the
    macOS `Promise.allSettled` walk.
    */
    let target_projects: Vec<(String, String)> = gpui_list_gxserver_domain_projects()?
        .iter()
        .filter(|project| {
            let project_id = project
                .get("projectId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let path = project
                .get("path")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            project_id != GPUI_QUICK_AUTOMATIONS_PROJECT_ID
                && !path.trim().is_empty()
                && project
                    .get("isRecentProject")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
                && project
                    .get("visibility")
                    .and_then(serde_json::Value::as_str)
                    != Some("hidden")
                && project
                    .get("systemKind")
                    .and_then(serde_json::Value::as_str)
                    != Some("remoteAttachCarrier")
        })
        .filter_map(|project| {
            let project_id = project
                .get("projectId")
                .and_then(serde_json::Value::as_str)?;
            let path = project.get("path").and_then(serde_json::Value::as_str)?;
            Some((project_id.to_string(), path.to_string()))
        })
        .collect();
    let states: Vec<serde_json::Value> = target_projects
        .iter()
        .filter_map(|(project_id, path)| {
            gpui_automation_rpc_automation_state(
                "/api/readAutomationState",
                &serde_json::json!({ "projectId": project_id, "projectPath": path }),
            )
            .ok()
        })
        .collect();
    let mut agents: Vec<serde_json::Value> = Vec::new();
    let mut seen_agent_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut automations: Vec<serde_json::Value> = Vec::new();
    let mut runs: Vec<serde_json::Value> = Vec::new();
    for state in &states {
        for agent in state
            .get("agents")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let agent_id = agent
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if !agent_id.is_empty() && seen_agent_ids.insert(agent_id) {
                agents.push(agent.clone());
            }
        }
        automations.extend(
            state
                .get("automations")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        runs.extend(
            state
                .get("runs")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
    }
    let first_state = states.first();
    let projects = first_state
        .and_then(|state| state.get("projects").cloned())
        .unwrap_or_else(|| {
            serde_json::Value::Array(
                target_projects
                    .iter()
                    .map(|(project_id, path)| {
                        serde_json::json!({
                            "canUseWorktrees": false,
                            "label": path.rsplit('/').next().unwrap_or(project_id),
                            "path": path,
                            "projectId": project_id,
                            "worktreeUnavailableReason":
                                "Open the project Automate view to use worktree mode.",
                        })
                    })
                    .collect(),
            )
        });
    Ok(serde_json::json!({
        "agents": agents,
        "automations": automations,
        "defaultAgentId": first_state
            .and_then(|state| state.get("defaultAgentId").cloned())
            .unwrap_or(serde_json::Value::String("codex".to_string())),
        "projectCanUseWorktrees": false,
        "projectId": scope
            .project_id
            .clone()
            .unwrap_or_else(|| GPUI_QUICK_AUTOMATIONS_PROJECT_ID.to_string()),
        "projectName": GPUI_QUICK_AUTOMATIONS_DISPLAY_TITLE,
        "projectPath": "",
        "projects": projects,
        "runs": runs,
        "worktreeUnavailableReason": "Choose a project before using worktree mode.",
    }))
}

pub(crate) fn gpui_automation_open_run_target(
    action: &str,
    request: &serde_json::Value,
    scope: &GpuiAutomationBoardScope,
) -> Result<(serde_json::Value, Option<GpuiAutomationBoardNavigation>), String> {
    let run_id = manage_request_string(request, "sessionId")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "No automation run id was supplied.".to_string())?;
    let state = gpui_automation_rpc_automation_state(
        "/api/readAutomationState",
        &serde_json::Value::Object(gpui_automation_scope_params(scope)),
    )?;
    let run = state
        .get("runs")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .find(|run| run.get("id").and_then(serde_json::Value::as_str) == Some(run_id.as_str()))
        .cloned()
        .ok_or_else(|| "Automation run not found.".to_string())?;
    let run_project_id = run
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .or_else(|| scope.project_id.clone())
        .ok_or_else(|| "Automation run has no project.".to_string())?;
    let worktree_path = run
        .get("worktree")
        .and_then(|worktree| worktree.get("path"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string);
    let navigation = if action == "automationOpenWorktree" {
        let worktree_path =
            worktree_path.ok_or_else(|| "Automation run has no linked worktree.".to_string())?;
        match gpui_find_gxserver_project_id_by_path(&worktree_path)? {
            // macOS parity: focus the registered worktree project when it
            // exists, otherwise reveal the checkout in Finder.
            Some(project_id) => Some(GpuiAutomationBoardNavigation::FocusProject(project_id)),
            None => Some(GpuiAutomationBoardNavigation::RevealWorktreePath(
                worktree_path,
            )),
        }
    } else {
        let session_id = run
            .get("sessionId")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|session_id| !session_id.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "Automation run has no linked session.".to_string())?;
        // Worktree/thread runs live in another project than the automation;
        // resolve the session's owning project like gxserver's own
        // `find_run_session_project_id` (worktree path match, else run project).
        let session_project_id = match &worktree_path {
            Some(path) => {
                gpui_find_gxserver_project_id_by_path(path)?.unwrap_or(run_project_id.clone())
            }
            None => run_project_id.clone(),
        };
        gpui_combined_presentation_session_focus_id(&session_project_id, &session_id)
            .map(GpuiAutomationBoardNavigation::FocusSession)
    };
    let mut mark_read_params = gpui_automation_scope_params(scope);
    mark_read_params.insert("runId".to_string(), serde_json::Value::String(run_id));
    let refreshed = gpui_automation_rpc_automation_state(
        "/api/markAutomationRunRead",
        &serde_json::Value::Object(mark_read_params),
    )?;
    Ok((refreshed, navigation))
}
