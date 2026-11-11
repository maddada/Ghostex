//! Beads calls for the native Kanban. Each one is gxserver's `/api/runBeadsAction` reached through
//! `gx_rpc`, so bd command construction stays in gxserver and the same file runs in the GPUI web
//! build (where `gx_rpc` is `fetch`). A multi-step flow is one async sequence, in bd's order.
//!
//! CDXC:ProjectBoard 2026-11-10 WHY:
//! These calls used to block a background thread on the desktop's own gxserver request helpers,
//! which a browser page cannot do, so the board could not be compiled into the web build. They now
//! await the shared `gx_rpc` seam with the same parameters the old bridge sent.

use serde_json::{Value, json};

use super::host::ProjectBoardBridgeRuntimeContext;
use super::model::{
    BeadsIssue, is_bootstrap_issue_prefix, normalize_issue_prefix, reconciled_workflow_statuses,
};
use super::text::beads_error_message;
use crate::app::gx_store::gx_rpc;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

const RUN_BEADS_ACTION_PATH: &str = "/api/runBeadsAction";
const GENERATE_TITLE_PATH: &str = "/api/beadsGenerateTitle";

/// The board's action names and gxserver's `runBeadsAction` names for them.
fn project_beads_gxserver_action_for_board_action(action: &str) -> Result<&'static str, String> {
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

/// The gxserver a call goes to: the remote machine's tunnel for a remote project (refused while it
/// is not connected), the local one otherwise.
fn gxserver_target(
    context: &ProjectBoardBridgeRuntimeContext,
) -> Result<Option<GpuiRemoteGxserverRequestTarget>, String> {
    if context.remote_machine_id.is_none() {
        return Ok(None);
    }
    context
        .remote_target
        .clone()
        .map(Some)
        .ok_or_else(|| "The remote gxserver is not connected.".to_string())
}

/// `runBeadsAction`'s parameters for one board request, exactly as the old bridge built them.
fn run_beads_action_params(
    context: &ProjectBoardBridgeRuntimeContext,
    request: &Value,
) -> Result<Value, String> {
    let string = |key: &str| request.get(key).and_then(Value::as_str).map(str::to_string);
    let action = string("action").unwrap_or_default();
    let mut params = serde_json::Map::new();
    params.insert(
        "action".into(),
        json!(project_beads_gxserver_action_for_board_action(&action)?),
    );
    params.insert("projectBoardScope".into(), json!(true));
    params.insert("projectPath".into(), json!(context.project_path));
    if let Some(project_id) = context.project_id.as_ref() {
        params.insert("projectId".into(), json!(project_id));
    }
    for key in [
        "comment",
        "dependsOnId",
        "depType",
        "description",
        "issueId",
        "label",
        "priority",
        "query",
        "status",
        "title",
        "value",
    ] {
        if let Some(value) = string(key) {
            params.insert(key.to_string(), json!(value));
        }
    }
    if let Some(estimate) = request.get("estimate").filter(|value| value.is_number()) {
        params.insert("estimate".into(), estimate.clone());
    }
    if let Some(labels) = request.get("labels").and_then(Value::as_array) {
        let labels = labels
            .iter()
            .filter_map(Value::as_str)
            .map(|label| json!(label))
            .collect::<Vec<_>>();
        params.insert("labels".into(), Value::Array(labels));
    }
    Ok(Value::Object(params))
}

/// One Beads action. `Ok` carries bd's parsed JSON stdout (`Null` when it printed nothing).
pub(crate) async fn beads_call(
    context: &ProjectBoardBridgeRuntimeContext,
    request: Value,
) -> Result<Value, String> {
    let answer = match (
        gxserver_target(context),
        run_beads_action_params(context, &request),
    ) {
        (Ok(remote), Ok(params)) => gx_rpc(remote, RUN_BEADS_ACTION_PATH, params)
            .await
            .map_err(|error| error.message),
        (Err(error), _) | (_, Err(error)) => Err(error),
    };
    let result = answer.map_err(|error| beads_error_message(&error))?;
    let text = |key: &str| result[key].as_str().unwrap_or_default().to_string();
    if result["exitCode"].as_i64().unwrap_or(1) != 0 {
        let stderr = text("stderr");
        let output = if stderr.trim().is_empty() {
            text("stdout")
        } else {
            stderr
        };
        return Err(beads_error_message(&output));
    }
    let stdout = text("stdout");
    let stdout = stdout.trim();
    if stdout.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(stdout)
        .map_err(|_| "Beads returned output the board could not read.".into())
}

/// Whether the computer that owns the project can write a title for a bead created without one
/// (its gxserver lists `generateTitle` in `/api/beadsView`). `Some(false)` only when that
/// gxserver answered and predates it; `None` when it could not be asked (not connected yet), so
/// the board asks again later instead of keeping a transient failure.
pub(crate) async fn title_generation_available(
    context: &ProjectBoardBridgeRuntimeContext,
) -> Option<bool> {
    let remote = gxserver_target(context).ok()?;
    let params = serde_json::json!({ "projectId": context.project_id });
    match gx_rpc(remote.clone(), "/api/beadsView", params).await {
        Ok(answer) => Some(
            answer
                .get("capabilities")
                .and_then(Value::as_array)
                .is_some_and(|capabilities| {
                    capabilities
                        .iter()
                        .any(|capability| capability.as_str() == Some("generateTitle"))
                }),
        ),
        Err(error) if error.code.as_deref() == Some("notFound") => Some(false),
        // A page's fetch of a path an older gxserver does not know fails before any answer; if a
        // call every gxserver answers works, that gxserver predates the Beads view.
        Err(error) if error.code.is_none() => {
            gx_rpc(remote, "/api/listProjects", serde_json::json!({}))
                .await
                .ok()
                .map(|_| false)
        }
        Err(_) => None,
    }
}

/// `normalizeBeadsPayload`: bd's `{ data }` envelope, unwrapped.
pub(crate) fn beads_payload(payload: Value) -> Value {
    match payload {
        Value::Object(mut object) if object.contains_key("data") => {
            object.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

fn config_string(payload: Value) -> String {
    match beads_payload(payload) {
        Value::String(value) => value,
        Value::Object(object) => object
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        _ => String::new(),
    }
}

/// `ensureIssuePrefix`: replaces only a stale bootstrap prefix with the project's own.
pub(crate) async fn ensure_issue_prefix(
    context: &ProjectBoardBridgeRuntimeContext,
    desired: &str,
) -> Result<(), String> {
    let desired = normalize_issue_prefix(desired);
    let current = normalize_issue_prefix(&config_string(
        beads_call(context, json!({ "action": "configGetIssuePrefix" })).await?,
    ));
    if current != desired && is_bootstrap_issue_prefix(&current) {
        beads_call(
            context,
            json!({ "action": "renamePrefix", "value": format!("{desired}-") }),
        )
        .await?;
    }
    Ok(())
}

/// `readWorkflowStatuses`.
pub(crate) async fn read_workflow_statuses(
    context: &ProjectBoardBridgeRuntimeContext,
) -> Result<String, String> {
    Ok(config_string(
        beads_call(context, json!({ "action": "configGet" })).await?,
    ))
}

/// `ensureWorkflowStatuses`.
pub(crate) async fn ensure_workflow_statuses(
    context: &ProjectBoardBridgeRuntimeContext,
) -> Result<String, String> {
    let current = read_workflow_statuses(context).await?;
    match reconciled_workflow_statuses(&current) {
        Some(next) => {
            beads_call(context, json!({ "action": "configSet", "value": next })).await?;
            Ok(next)
        }
        None => Ok(current),
    }
}

pub(crate) async fn list_issues(
    context: &ProjectBoardBridgeRuntimeContext,
) -> Result<Vec<BeadsIssue>, String> {
    let payload = beads_payload(beads_call(context, json!({ "action": "listIssues" })).await?);
    Ok(payload
        .as_array()
        .map(|issues| issues.iter().filter_map(BeadsIssue::from_value).collect())
        .unwrap_or_default())
}

pub(crate) struct KanbanLoadResult {
    pub(crate) column_config: String,
    pub(crate) issues: Vec<BeadsIssue>,
}

pub(crate) async fn load_board(
    context: &ProjectBoardBridgeRuntimeContext,
    reconcile: bool,
    issue_prefix: &str,
) -> Result<KanbanLoadResult, String> {
    let column_config = if reconcile {
        ensure_issue_prefix(context, issue_prefix).await?;
        ensure_workflow_statuses(context).await?
    } else {
        read_workflow_statuses(context).await?
    };
    Ok(KanbanLoadResult {
        column_config,
        issues: list_issues(context).await?,
    })
}

pub(crate) async fn show_issue(
    context: &ProjectBoardBridgeRuntimeContext,
    issue_id: &str,
) -> Result<Option<BeadsIssue>, String> {
    let payload =
        beads_payload(beads_call(context, json!({ "action": "show", "issueId": issue_id })).await?);
    let issue = match &payload {
        Value::Array(issues) => issues.first().and_then(BeadsIssue::from_value),
        other => BeadsIssue::from_value(other),
    };
    Ok(issue)
}

/// Everything Edit ticket's Save writes, in the React board's order.
pub(crate) struct KanbanSaveDraft {
    pub(crate) issue_id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) priority: String,
    pub(crate) estimate: Option<i64>,
    pub(crate) labels: Vec<String>,
    pub(crate) status: Option<String>,
    pub(crate) comment: String,
}

pub(crate) async fn persist_ticket(
    context: &ProjectBoardBridgeRuntimeContext,
    draft: &KanbanSaveDraft,
) -> Result<(), String> {
    let id = &draft.issue_id;
    beads_call(
        context,
        json!({ "action": "updateTitle", "issueId": id, "title": draft.title }),
    )
    .await?;
    beads_call(
        context,
        json!({ "action": "updateDescription", "issueId": id, "description": draft.description }),
    )
    .await?;
    beads_call(
        context,
        json!({ "action": "updatePriority", "issueId": id, "priority": draft.priority }),
    )
    .await?;
    if let Some(estimate) = draft.estimate {
        beads_call(
            context,
            json!({ "action": "updateEstimate", "issueId": id, "estimate": estimate }),
        )
        .await?;
    }
    if !draft.labels.is_empty() {
        beads_call(
            context,
            json!({ "action": "setLabels", "issueId": id, "labels": draft.labels }),
        )
        .await?;
    }
    if let Some(status) = draft.status.as_ref() {
        beads_call(
            context,
            json!({ "action": "updateStatus", "issueId": id, "status": status }),
        )
        .await?;
    }
    if !draft.comment.trim().is_empty() {
        beads_call(
            context,
            json!({ "action": "addComment", "issueId": id, "comment": draft.comment.trim() }),
        )
        .await?;
    }
    Ok(())
}

pub(crate) struct KanbanCreateDraft {
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) priority: String,
    pub(crate) estimate: Option<i64>,
    pub(crate) labels: Vec<String>,
}

/// `create`, then the created bead: from bd's answer, or found in a fresh list when bd's answer
/// carried no id (`resolveCreatedIssueFromRefresh`).
pub(crate) async fn create_issue(
    context: &ProjectBoardBridgeRuntimeContext,
    issue_prefix: &str,
    draft: &KanbanCreateDraft,
    known_ids: &[String],
) -> Result<BeadsIssue, String> {
    ensure_issue_prefix(context, issue_prefix).await?;
    let mut request = json!({
        "action": "create",
        "description": draft.description,
        "labels": draft.labels,
        "priority": draft.priority,
        "title": draft.title,
    });
    if let Some(estimate) = draft.estimate {
        request["estimate"] = json!(estimate);
    }
    let created = beads_payload(beads_call(context, request).await?);
    let created = match &created {
        Value::Array(issues) => issues.first().and_then(BeadsIssue::from_value),
        other => BeadsIssue::from_value(other),
    };
    if let Some(issue) = created {
        return Ok(issue);
    }
    list_issues(context)
        .await?
        .into_iter()
        .filter(|issue| {
            !known_ids.contains(&issue.id)
                && issue.title == draft.title
                && issue.description == draft.description
        })
        .max_by_key(|issue| {
            super::model::parse_time_ms(issue.created_at.as_deref().or(issue.updated_at.as_deref()))
                .unwrap_or(0)
        })
        .ok_or_else(|| "Created ticket was not found after create.".to_string())
}

/// The title the project's own computer writes for an untitled ticket with its Title Generation
/// agent (`/api/beadsGenerateTitle`), written back to bd.
pub(crate) async fn generate_and_store_title(
    context: &ProjectBoardBridgeRuntimeContext,
    issue_id: &str,
    prompt: &str,
) -> Result<String, String> {
    let mut params = json!({
        "projectPath": context.project_path,
        "prompt": prompt,
    });
    if let Some(project_id) = context.project_id.as_ref() {
        params["projectId"] = json!(project_id);
    }
    let generated = gx_rpc(gxserver_target(context)?, GENERATE_TITLE_PATH, params)
        .await
        .map_err(|error| error.message)?;
    let title = generated["title"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .ok_or_else(|| "Title generation returned an empty title.".to_string())?
        .to_string();
    beads_call(
        context,
        json!({ "action": "updateTitle", "issueId": issue_id, "title": title }),
    )
    .await?;
    Ok(title)
}
