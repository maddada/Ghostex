// C1 wave-1 deferred split: apps/desktop/src/app/helpers/board_gxserver.rs
// (~4.3k lines) further divided into responsibility-scoped submodules (pure
// move, no logic changes). This file holds the Kanban project board bridge: workarea URL derivation,
// bridge runtime context, board command intents/requests, and the project
// beads bridge response/error helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) fn kanban_workarea_runtime_url_from_project_snapshot(
    snapshot: &GpuiProjectSnapshot,
) -> Option<ProjectWorkareaRealRuntimeUrl> {
    /*
    CDXC:CefRuntime 2026-06-24-11:03:
    Kanban runtime URL authority is the bundled first-party CEF page plus the explicit live sidebar project identity. The URL is passed directly to CefSurface creation and is not stored in shell state, logged, derived from .git/folders, or backed by WKWebView/WebKit.
    */
    if !snapshot.feature_availability.kanban || snapshot.is_quick_projectless {
        return None;
    }
    let active_project_id = snapshot.active_project_id.as_ref()?.0.clone();
    let remote_reference = gpui_remote_project_reference_from_project_id(&active_project_id);
    let request_project_id = remote_reference
        .as_ref()
        .map(|reference| reference.project_id.clone())
        .unwrap_or(active_project_id);
    let project_path = snapshot
        .in_memory_project_path
        .as_ref()?
        .to_string_lossy()
        .to_string();
    let surface_id = snapshot.surface_ids.kanban_board_id.as_ref()?.clone();
    let base_url = gpui_cef_html_entry_url("GHOSTEX_GPUI_KANBAN_URL", "kanban.html").ok()?;
    let mut params = vec![
        ("projectName", snapshot.display_name.clone()),
        ("projectPath", project_path),
        ("projectId", request_project_id),
        ("projectEditorId", surface_id),
        ("beadsDisplayKey", snapshot.display_name.clone()),
    ];
    if let Some(reference) = remote_reference {
        params.push(("remoteMachineId", reference.remote_machine_id));
    }
    ProjectWorkareaRealRuntimeUrl::from_authorized_runtime_url(append_url_query_params(
        base_url, &params,
    ))
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectBoardCommandIntent {
    InitializeBeads,
    InstallOrUpdateBeads,
    MigrateBeads,
    AdoptBeads,
    AdoptBeadsFastForward,
    ReconcileBeadsFork,
}

impl GpuiProjectBoardCommandIntent {
    pub(crate) fn command_id(self) -> &'static str {
        match self {
            Self::InitializeBeads => GPUI_PROJECT_BOARD_INITIALIZE_BEADS_COMMAND_ID,
            Self::InstallOrUpdateBeads => GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_COMMAND_ID,
            Self::MigrateBeads => GPUI_PROJECT_BOARD_MIGRATE_BEADS_COMMAND_ID,
            Self::AdoptBeads => GPUI_PROJECT_BOARD_ADOPT_BEADS_COMMAND_ID,
            Self::AdoptBeadsFastForward => GPUI_PROJECT_BOARD_ADOPT_BEADS_FAST_FORWARD_COMMAND_ID,
            Self::ReconcileBeadsFork => GPUI_PROJECT_BOARD_RECONCILE_BEADS_FORK_COMMAND_ID,
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::InitializeBeads => "Initialize Beads",
            Self::InstallOrUpdateBeads => "Install or Update Beads",
            Self::MigrateBeads => "Migrate Beads",
            Self::AdoptBeads | Self::AdoptBeadsFastForward => "Adopt Beads Remote",
            Self::ReconcileBeadsFork => "Back Up and Adopt Beads Remote",
        }
    }

    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::InitializeBeads => "bd init",
            Self::InstallOrUpdateBeads => {
                "curl -fsSL https://raw.githubusercontent.com/gastownhall/beads/main/scripts/install.sh | bash"
            }
            Self::MigrateBeads => "bd migrate --force && bd dolt push",
            Self::AdoptBeads | Self::AdoptBeadsFastForward => "bd bootstrap",
            Self::ReconcileBeadsFork => "bd export --all -o backup.jsonl && bd bootstrap",
        }
    }
}

pub(crate) fn gpui_project_board_action_for_command_id(command_id: &str) -> Option<&'static str> {
    match command_id {
        GPUI_PROJECT_BOARD_INITIALIZE_BEADS_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION)
        }
        GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION)
        }
        GPUI_PROJECT_BOARD_MIGRATE_BEADS_COMMAND_ID
        | GPUI_PROJECT_BOARD_ADOPT_BEADS_COMMAND_ID
        | GPUI_PROJECT_BOARD_ADOPT_BEADS_FAST_FORWARD_COMMAND_ID
        | GPUI_PROJECT_BOARD_RECONCILE_BEADS_FORK_COMMAND_ID => {
            Some(GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION)
        }
        _ => None,
    }
}

pub(crate) fn gpui_project_board_command_request(
    request: &serde_json::Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> Result<GpuiProjectBoardCommandIntent, String> {
    let object = request
        .as_object()
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    if object
        .keys()
        .any(|key| !["action", "migrationOption", "projectId", "requestId"].contains(&key.as_str()))
    {
        return Err("The Beads command request is invalid.".to_string());
    }
    let _request_id = object
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| {
            !value.is_empty()
                && value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    let action = object
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "The Beads command request is invalid.".to_string())?;
    let migration_option = object
        .get("migrationOption")
        .and_then(serde_json::Value::as_str);
    let intent = match (action, migration_option) {
        (GPUI_PROJECT_BOARD_INITIALIZE_BEADS_ACTION, None) => {
            GpuiProjectBoardCommandIntent::InitializeBeads
        }
        (GPUI_PROJECT_BOARD_INSTALL_OR_UPDATE_BEADS_ACTION, None) => {
            GpuiProjectBoardCommandIntent::InstallOrUpdateBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("migrate")) => {
            GpuiProjectBoardCommandIntent::MigrateBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("adopt")) => {
            GpuiProjectBoardCommandIntent::AdoptBeads
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("adopt-fast-forward")) => {
            GpuiProjectBoardCommandIntent::AdoptBeadsFastForward
        }
        (GPUI_PROJECT_BOARD_RUN_BEADS_MIGRATION_ACTION, Some("reconcile-fork")) => {
            GpuiProjectBoardCommandIntent::ReconcileBeadsFork
        }
        _ => return Err("The Beads command request is invalid.".to_string()),
    };
    let context = context.ok_or_else(|| "No active Kanban project is available.".to_string())?;
    if context.remote_machine_id.is_some() {
        return Err(
            "Run this Beads command from a terminal on the remote machine, then refresh the board."
                .to_string(),
        );
    }
    let request_project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if request_project_id.is_some_and(|project_id| {
        project_id.chars().count() > GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
            || project_id.chars().any(char::is_control)
            || context.project_id.as_deref() != Some(project_id)
    }) {
        return Err("The Beads command request is not for this active project.".to_string());
    }
    Ok(intent)
}

pub(crate) fn project_board_bridge_response_for_request_payload(
    payload: &str,
    snapshot: Option<&GpuiProjectSnapshot>,
) -> serde_json::Value {
    let request = serde_json::from_str::<serde_json::Value>(payload).unwrap_or_default();
    let request_id = manage_request_string(&request, "requestId").unwrap_or_default();
    let action = manage_request_string(&request, "action").unwrap_or_default();
    if action == "getState" || action == "projectEditorFocusOwnerChanged" {
        let project_id = snapshot
            .and_then(|snapshot| snapshot.active_project_id.as_ref())
            .map(|id| id.0.clone());
        return serde_json::json!({
            "ok": true,
            "payload": {
                "agents": [],
                "links": [],
                "projectId": project_id,
                "sessions": [],
            },
            "requestId": request_id,
        });
    }
    serde_json::json!({
        "error": "Project board conversation action is not handled by this GPUI runtime surface.",
        "ok": false,
        "requestId": request_id,
    })
}

pub(crate) fn project_beads_gxserver_action_for_board_action(
    action: &str,
) -> Result<&'static str, String> {
    match action {
        "addComment" => Ok("comment"),
        "addLabel" => Ok("addLabel"),
        "configGet" => Ok("configGet"),
        "configGetIssuePrefix" => Ok("configGetIssuePrefix"),
        "configSet" => Ok("configSet"),
        "create" => Ok("create"),
        "delete" => Ok("delete"),
        "depAdd" => Ok("depAdd"),
        "depRemove" => Ok("depRemove"),
        "list" => Ok("list"),
        "listIssues" => Ok("board"),
        "listAllLabels" => Ok("listAllLabels"),
        "renamePrefix" => Ok("renamePrefix"),
        "removeLabel" => Ok("removeLabel"),
        "search" => Ok("search"),
        "setLabels" => Ok("setLabels"),
        "show" => Ok("show"),
        "updateDescription" => Ok("updateDescription"),
        "updateEstimate" => Ok("updateEstimate"),
        "updatePriority" => Ok("updatePriority"),
        "updateStatus" => Ok("updateStatus"),
        "updateTitle" => Ok("updateTitle"),
        _ => Err(format!("Unsupported Beads action: {action}")),
    }
}

pub(crate) fn project_beads_bridge_response_from_gxserver(
    status_code: u16,
    body: &str,
    request_id: &str,
) -> Result<serde_json::Value, String> {
    if !(200..300).contains(&status_code) {
        let error = project_beads_gxserver_error_message(body, Some(status_code));
        return Ok(project_beads_bridge_error_response(request_id, &error));
    }
    let response = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| "gxserver returned an invalid Beads response.".to_string())?;
    let result = response
        .get("result")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "gxserver did not return a Beads result.".to_string())?;
    Ok(project_beads_bridge_response_from_result(
        &serde_json::Value::Object(result.clone()),
        request_id,
    ))
}

pub(crate) fn project_beads_gxserver_error_message(body: &str, status_code: Option<u16>) -> String {
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
    match status_code {
        Some(status_code) => format!("gxserver Beads request failed with HTTP {status_code}."),
        None => "gxserver Beads request failed.".to_string(),
    }
}
