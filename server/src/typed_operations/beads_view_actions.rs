//! The Beads actions the Beads view added: claim, reopen, defer and bring back, answering or
//! dismissing a decision, and the Pull and Push commands, plus `create`'s optional type, epic,
//! label inheritance and blockers. Every existing action keeps its builder in `beads.rs`.

use serde_json::{Map, Value};

use super::beads::{normalize_beads_dependency_type, normalize_beads_estimate};
use super::values::{
    normalize_issue_id, normalize_nullish_required_text, normalize_required_text,
    nullish_value_to_string, ProcessCommand, TypedOperationError,
};

const BEADS_VIEW_ACTIONS: [&str; 8] = [
    "claim",
    "reopen",
    "defer",
    "undefer",
    "humanRespond",
    "humanDismiss",
    "doltPull",
    "doltPush",
];

pub(crate) fn is_beads_view_action(action: &str) -> bool {
    BEADS_VIEW_ACTIONS.contains(&action)
}

pub(crate) fn build_beads_view_command(
    action: &str,
    params: &Map<String, Value>,
    cwd: &str,
    bd_executable_path: &str,
) -> Result<ProcessCommand, TypedOperationError> {
    let id = || normalize_issue_id(params.get("issueId"));
    let args: Vec<String> = match action {
        "claim" => vec!["update".into(), id()?, "--claim".into(), "--json".into()],
        "reopen" => vec!["reopen".into(), id()?, "--json".into()],
        "defer" => {
            let mut args = vec!["defer".to_string(), id()?];
            if let Some(until) = optional_text(params.get("until")) {
                args.extend(["--until".to_string(), normalize_beads_defer_until(&until)?]);
            }
            args.push("--json".to_string());
            args
        }
        "undefer" => vec!["undefer".into(), id()?, "--json".into()],
        "humanRespond" => vec![
            "human".into(),
            "respond".into(),
            id()?,
            "-r".into(),
            normalize_required_text(params.get("response"), "response")?,
        ],
        "humanDismiss" => vec!["human".into(), "dismiss".into(), id()?],
        "doltPull" => vec!["dolt".into(), "pull".into()],
        // CDXC:ProjectBoard 2026-11-11 WHY: without --no-adopt a push with no Dolt remote can quietly adopt the git origin as one; Ghostex never makes that choice for the user. `bd dolt pull` has no such flag (it rejects it).
        "doltPush" => vec!["dolt".into(), "push".into(), "--no-adopt".into()],
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported Beads action: {action}"
            )))
        }
    };
    Ok(ProcessCommand::new(bd_executable_path, args, cwd))
}

fn optional_text(input: Option<&Value>) -> Option<String> {
    input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// bd decides which types exist (custom ones included); this is a shape check only, like
/// statuses. Absent keeps the create that predates the Beads view: a task.
fn normalize_beads_issue_type(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let Some(issue_type) = optional_text(input) else {
        return Ok("task".to_string());
    };
    if issue_type.len() <= 32
        && issue_type
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        Ok(issue_type)
    } else {
        Err(TypedOperationError::bad_request(
            "issueType must be a Beads type name.",
        ))
    }
}

/// `bd defer --until` takes bd's own date words (`tomorrow`, `+1w`, `next monday`) or a date.
fn normalize_beads_defer_until(until: &str) -> Result<String, TypedOperationError> {
    if until.len() <= 64 && !until.starts_with('-') && !until.contains('\n') {
        Ok(until.to_string())
    } else {
        Err(TypedOperationError::bad_request(
            "until must be a date or a bd date word.",
        ))
    }
}

fn normalize_beads_blocked_by(input: Option<&Value>) -> Result<Vec<String>, TypedOperationError> {
    match input {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(ids)) => ids
            .iter()
            .map(|id| Ok(format!("blocked-by:{}", normalize_issue_id(Some(id))?)))
            .collect(),
        Some(_) => Err(TypedOperationError::bad_request(
            "blockedBy must be an array of bead ids.",
        )),
    }
}

pub(crate) fn build_beads_create_args(
    params: &Map<String, Value>,
) -> Result<Vec<String>, TypedOperationError> {
    let mut args = vec![
        "create".to_string(),
        "--title".to_string(),
        normalize_required_text(params.get("title"), "title")?,
        "--description".to_string(),
        nullish_value_to_string(params.get("description"), ""),
        "--priority".to_string(),
        normalize_nullish_required_text(params.get("priority"), "priority", "2")?,
        "--type".to_string(),
        normalize_beads_issue_type(params.get("issueType"))?,
    ];
    if let Some(parent) = optional_text(params.get("parent")) {
        args.extend([
            "--parent".to_string(),
            normalize_issue_id(Some(&Value::String(parent)))?,
        ]);
    }
    if params.get("noInheritLabels").and_then(Value::as_bool) == Some(true) {
        args.push("--no-inherit-labels".to_string());
    }
    if params.contains_key("estimate") {
        args.extend([
            "--estimate".to_string(),
            normalize_beads_estimate(params.get("estimate"))?,
        ]);
    }
    if let Some(labels_value) = params.get("labels") {
        if !labels_value.is_null() {
            let labels = labels_value.as_array().ok_or_else(|| {
                TypedOperationError::bad_request("labels must be an array of non-empty strings.")
            })?;
            if !labels.is_empty() {
                let labels = labels
                    .iter()
                    .map(|label| normalize_required_text(Some(label), "label"))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",");
                args.extend(["--labels".to_string(), labels]);
            }
        }
    }
    let blocked_by = normalize_beads_blocked_by(params.get("blockedBy"))?;
    if !blocked_by.is_empty() {
        if optional_text(params.get("dependsOnId")).is_some() {
            return Err(TypedOperationError::bad_request(
                "blockedBy and dependsOnId cannot be combined.",
            ));
        }
        // CDXC:ProjectBoard 2026-11-11 WHY: every blocker goes into the one create, so bd writes the bead and all its dependencies together and a bead is never left half blocked.
        args.extend(["--deps".to_string(), blocked_by.join(",")]);
    }
    if let Some(depends_on) = params
        .get("dependsOnId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.extend([
            "--deps".to_string(),
            format!(
                "{}:{}",
                normalize_beads_dependency_type(params.get("depType"))?,
                normalize_issue_id(Some(&Value::String(depends_on.to_string())))?
            ),
        ]);
    }
    args.push("--json".to_string());
    Ok(args)
}
