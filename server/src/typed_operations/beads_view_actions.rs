//! The Beads actions the Beads view added: claim, reopen, defer and bring back, answering or
//! dismissing a decision, and the Pull and Push commands, plus `create`'s optional type, epic,
//! label inheritance and blockers. Every existing action keeps its builder in `beads.rs`.

use serde_json::{Map, Value};

use super::beads::{normalize_beads_dependency_type, normalize_beads_estimate};
use super::values::{
    normalize_issue_id, normalize_nullish_required_text, normalize_required_text,
    nullish_value_to_string, ProcessCommand, TypedOperationError,
};

/// The bd config key the Board's hidden built-in columns live in: in the workspace database, next
/// to `status.custom`, so every computer, clone and agent sees the same Board. `custom.*` is bd's
/// namespace for keys it does not own.
const BOARD_HIDDEN_COLUMNS_KEY: &str = "custom.ghostex.board_hidden_columns";

const BEADS_VIEW_ACTIONS: [&str; 10] = [
    "claim",
    "reopen",
    "defer",
    "undefer",
    "humanRespond",
    "humanDismiss",
    "doltPull",
    "doltPush",
    "boardHiddenColumnsGet",
    "boardHiddenColumnsSet",
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
        // CDXC:ProjectBoard 2026-11-12 WHY: a push with no Dolt remote can quietly adopt the git origin as one, and Ghostex never makes that choice for the user. Push names the workspace's own Dolt remote with `--remote` (sync.rs reads it first and does not push without one). It replaces `--no-adopt`, which bd 1.2.2 rejects as an unknown flag, so every Push failed there; `--remote` is in every bd that has `dolt push`.
        "doltPush" => vec![
            "dolt".into(),
            "push".into(),
            "--remote".into(),
            normalize_required_text(params.get("remote"), "remote")?,
        ],
        // Internal: the remote list Push reads before it pushes. Not a client action.
        "doltRemoteList" => vec![
            "dolt".into(),
            "remote".into(),
            "list".into(),
            "--json".into(),
        ],
        "boardHiddenColumnsGet" => vec![
            "config".into(),
            "get".into(),
            BOARD_HIDDEN_COLUMNS_KEY.into(),
            "--json".into(),
        ],
        // Internal: the write `run_board_hidden_columns_set` makes once it has merged the change
        // into the current value. Not a client action (`normalize_beads_action` refuses it).
        "boardHiddenColumnsWrite" => {
            let hidden = params
                .get("hidden")
                .and_then(Value::as_array)
                .map(|names| {
                    names
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|name| normalize_hideable_board_column(Some(name)))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            if hidden.is_empty() {
                vec![
                    "config".into(),
                    "unset".into(),
                    BOARD_HIDDEN_COLUMNS_KEY.into(),
                    "--json".into(),
                ]
            } else {
                vec![
                    "config".into(),
                    "set".into(),
                    BOARD_HIDDEN_COLUMNS_KEY.into(),
                    hidden.join(","),
                    "--json".into(),
                ]
            }
        }
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported Beads action: {action}"
            )))
        }
    };
    Ok(ProcessCommand::new(bd_executable_path, args, cwd))
}

/// Only Backlog, Test and Review can be hidden.
fn normalize_hideable_board_column(name: Option<&str>) -> Result<String, TypedOperationError> {
    let name = name.map(str::trim).unwrap_or_default();
    if ghostex_gx_protocol::beads_view::HIDEABLE_BOARD_COLUMNS.contains(&name) {
        Ok(name.to_string())
    } else {
        Err(TypedOperationError::bad_request(format!(
            "The Board can't hide the {name} column."
        )))
    }
}

/// `boardHiddenColumnsSet`: shows or hides one column (`column`, `visible`).
///
/// CDXC:ProjectBoard 2026-11-11 WHY: a client that sent its whole hidden list overwrote what another computer or clone had hidden since it last read. The client sends only the column it changed, and gxserver reads the current value and writes the merged one; inside a workspace both bd calls run under the workspace's lock (`run_beads_action_in_workspace` calls this through `run_beads_action_direct`), so two changes never lose each other.
pub(crate) async fn run_board_hidden_columns_set(
    params: &Map<String, Value>,
    context: &super::TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let column = normalize_hideable_board_column(params.get("column").and_then(Value::as_str))?;
    let visible = params
        .get("visible")
        .and_then(Value::as_bool)
        .ok_or_else(|| TypedOperationError::bad_request("visible must be true or false."))?;
    let read = super::beads_action_command("boardHiddenColumnsGet", &Map::new(), context)?;
    let output = super::run_process_command(&read, context).await?;
    if output.exit_code != 0 || output.error.is_some() {
        return super::finish_beads_action("boardHiddenColumnsSet", &read, output);
    }
    let current = serde_json::from_str::<Value>(output.stdout.trim()).unwrap_or(Value::Null);
    let current = current.get("data").unwrap_or(&current);
    let mut hidden = current
        .get("value")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|name| ghostex_gx_protocol::beads_view::HIDEABLE_BOARD_COLUMNS.contains(name))
        .map(str::to_string)
        .collect::<Vec<_>>();
    hidden.dedup();
    hidden.retain(|name| *name != column);
    if !visible {
        hidden.push(column);
    }
    let mut write_params = Map::new();
    write_params.insert("hidden".to_string(), Value::from(hidden));
    let write = super::beads_action_command("boardHiddenColumnsWrite", &write_params, context)?;
    let output = super::run_process_command(&write, context).await?;
    super::finish_beads_action("boardHiddenColumnsSet", &write, output)
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
            .map(|id| {
                // CDXC:ProjectBoard 2026-11-11 WHY: a bare id means "this bead depends on id" in every bd release, while older bd (0.51) reads `blocked-by:<id>` as a custom, non-blocking dependency and `blocks:<id>` points the other way in current bd. The `blocked-by:` prefix is used only for an id that itself contains `:`, which a bare id would split at.
                let id = normalize_issue_id(Some(id))?;
                Ok(if id.contains(':') {
                    format!("blocked-by:{id}")
                } else {
                    id
                })
            })
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
