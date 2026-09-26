//! The Automate view's calls into the automation bridge logic the CEF page used
//! (`gpui_project_board_automation_result` in app/helpers/board_gxserver/automation.rs): the same
//! request objects the React page posted, run on the background executor.

use crate::app::helpers::{
    GpuiAutomationBoardNavigation, ProjectBoardBridgeRuntimeContext,
    gpui_project_board_automation_result,
};
use serde_json::{Value, json};

/// Which automations the view shows, from the sidebar's active project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutomateScope {
    /// The All Automations overview (`scope=all` on the React page).
    pub(crate) all_projects: bool,
    pub(crate) project_id: String,
    pub(crate) project_path: String,
    pub(crate) project_name: String,
    /// The project editor id the board requests carry.
    pub(crate) editor_id: String,
    /// All Automations without Show Beta Features: the "coming very soon" notice instead of data.
    pub(crate) coming_soon: bool,
}

pub(crate) enum AutomateRequest {
    Load,
    Save {
        definition: Value,
        project_id: String,
    },
    Delete {
        automation_id: String,
        project_id: String,
    },
    SetEnabled {
        automation_id: String,
        project_id: String,
        enabled: bool,
    },
    RunNow {
        automation_id: String,
        project_id: String,
    },
    ArchiveRun {
        run_id: String,
        project_id: String,
    },
    MarkRunRead {
        run_id: String,
        project_id: String,
    },
    OpenRunSession {
        run_id: String,
        project_id: String,
    },
    OpenRunWorktree {
        run_id: String,
        project_id: String,
    },
}

impl AutomateRequest {
    /// The message the React page would have failed with.
    pub(crate) fn failure_message(&self) -> &'static str {
        match self {
            Self::Load => "Could not load automations.",
            Self::Save { .. } => "Could not save automation.",
            Self::Delete { .. } => "Could not delete automation.",
            Self::SetEnabled { .. } => "Could not update automation.",
            Self::RunNow { .. } => "Could not run automation.",
            Self::ArchiveRun { .. } => "Could not archive automation run.",
            Self::MarkRunRead { .. } => "Could not mark automation run read.",
            Self::OpenRunSession { .. } => "Could not open automation session.",
            Self::OpenRunWorktree { .. } => "Could not open automation worktree.",
        }
    }

    /// The row or automation this request keeps busy (`automationActionId`).
    pub(crate) fn busy_id(&self) -> Option<String> {
        match self {
            Self::Load => None,
            Self::Save { definition, .. } => definition["id"].as_str().map(str::to_string),
            Self::Delete { automation_id, .. }
            | Self::SetEnabled { automation_id, .. }
            | Self::RunNow { automation_id, .. } => Some(automation_id.clone()),
            Self::ArchiveRun { run_id, .. }
            | Self::MarkRunRead { run_id, .. }
            | Self::OpenRunSession { run_id, .. }
            | Self::OpenRunWorktree { run_id, .. } => Some(run_id.clone()),
        }
    }

    /// The bridge request object. `project_path` resolves a target project id to its path
    /// (`automationProjectPathForId`).
    pub(crate) fn to_bridge_request(
        &self,
        scope: &AutomateScope,
        project_path: impl Fn(&str) -> Option<String>,
    ) -> Value {
        let target = |action: &str, project_id: &str, extra: Value| -> Value {
            let mut request = json!({
                "action": action,
                "projectEditorId": scope.editor_id,
                "projectId": project_id,
                "requestId": "native-automate",
            });
            if let Some(path) = project_path(project_id).filter(|path| !path.is_empty()) {
                request["projectPath"] = Value::String(path);
            }
            if let (Some(request), Some(extra)) = (request.as_object_mut(), extra.as_object()) {
                request.extend(extra.clone());
            }
            request
        };
        match self {
            Self::Load if scope.all_projects => json!({
                "action": "automationGetAllState",
                "projectEditorId": scope.editor_id,
                "projectId": scope.project_id,
                "requestId": "native-automate",
            }),
            Self::Load => target("automationGetState", &scope.project_id, Value::Null),
            Self::Save {
                definition,
                project_id,
            } => target(
                "automationSave",
                project_id,
                json!({ "payloadJson": definition.to_string() }),
            ),
            Self::Delete {
                automation_id,
                project_id,
            } => target(
                "automationDelete",
                project_id,
                json!({ "sessionId": automation_id }),
            ),
            Self::SetEnabled {
                automation_id,
                project_id,
                enabled,
            } => target(
                "automationSetEnabled",
                project_id,
                json!({
                    "payloadJson": json!({ "enabled": enabled }).to_string(),
                    "sessionId": automation_id,
                }),
            ),
            Self::RunNow {
                automation_id,
                project_id,
            } => target(
                "automationRunNow",
                project_id,
                json!({ "sessionId": automation_id }),
            ),
            // Archiving keeps the run's worktree: the React page's typed-path removal is not ported.
            Self::ArchiveRun { run_id, project_id } => target(
                "automationArchiveRun",
                project_id,
                json!({
                    "payloadJson": json!({ "removeWorktree": false }).to_string(),
                    "sessionId": run_id,
                }),
            ),
            Self::MarkRunRead { run_id, project_id } => target(
                "automationMarkRunRead",
                project_id,
                json!({ "sessionId": run_id }),
            ),
            Self::OpenRunSession { run_id, project_id } => target(
                "automationOpenRunSession",
                project_id,
                json!({ "sessionId": run_id }),
            ),
            Self::OpenRunWorktree { run_id, project_id } => target(
                "automationOpenWorktree",
                project_id,
                json!({ "sessionId": run_id }),
            ),
        }
    }
}

pub(crate) type AutomateResult = Result<(Value, Option<GpuiAutomationBoardNavigation>), String>;

/// Blocking: gxserver round trips. Call from the background executor.
pub(crate) fn run_automate_request(
    request: &Value,
    context: Option<&ProjectBoardBridgeRuntimeContext>,
) -> AutomateResult {
    gpui_project_board_automation_result(request, context)
}
