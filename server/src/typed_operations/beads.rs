use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::{json, Map, Value};

use crate::toolchain::require_system_bd;

use super::values::{
    chmod_executable_if_supported, command_summary_json, display_unknown_value, normalize_issue_id,
    normalize_nullish_required_text, normalize_required_text, nullish_value_to_string,
    resolve_path_against, run_process_command, typed_result, value_to_string, ProcessCommand,
    StringOrElse, TypedOperationContext, TypedOperationError,
};

const BEADS_BOARD_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const BEADS_BOARD_ROW_LIMIT: usize = 5_000;
const BEADS_GIT_HOOK_NAMES: [&str; 3] = ["pre-commit", "post-merge", "post-checkout"];

#[derive(Clone, Copy, Debug)]
pub(crate) struct BeadsBoardLimits {
    pub(crate) response_limit_bytes: usize,
    pub(crate) row_limit: usize,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct BeadsLabelCount {
    count: usize,
    label: String,
}

pub(crate) async fn run_beads_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_beads_action(params.get("action"))?;
    if action == "storageExists" {
        let exists = Path::new(&resolve_beads_cwd(context))
            .join(".beads")
            .is_dir();
        return Ok(json!({
            "action": action,
            "exitCode": if exists { 0 } else { 1 },
            "stderr": "",
            "stdout": if exists { "true" } else { "false" },
        }));
    }
    /*
    CDXC:ProjectBoard 2026-06-22-01:09:
    server must match the TypeScript Beads execution contract: every subprocess-backed Project Board action runs with BD_JSON_ENVELOPE=1, failed board reads still return an empty `issues` array for UI consumers, and board parsing uses the same bounded JSON response rules.
    */
    let command = build_beads_command(&action, params, context)?.with_env("BD_JSON_ENVELOPE", "1");
    let output = run_process_command(&command, context).await?;
    if action == "show" && output.exit_code == 0 {
        let (issue, stdout) = parse_beads_show_output(&output.stdout)?;
        return Ok(json!({
            "action": action,
            "command": command_summary_json(&command.summary()),
            "exitCode": 0,
            "issue": issue,
            "stderr": output.stderr,
            "stdout": stdout,
        }));
    }
    if matches!(action.as_str(), "board" | "listAllLabels") {
        if output.exit_code != 0 {
            let mut result = typed_result(&action, &command, output);
            if action == "board" {
                result
                    .as_object_mut()
                    .expect("typed result object")
                    .insert("issues".to_string(), Value::Array(Vec::new()));
            }
            return Ok(result);
        }
        let (issues, stdout) = parse_beads_board_output(&output.stdout)?;
        if action == "listAllLabels" {
            let labels_stdout = serde_json::to_string(&derive_beads_label_counts(&issues))
                .map_err(|_| {
                    TypedOperationError::bad_request("Could not serialize Beads label counts.")
                })?;
            return Ok(json!({
                "action": action,
                "command": command_summary_json(&command.summary()),
                "exitCode": 0,
                "stderr": output.stderr,
                "stdout": labels_stdout,
            }));
        }
        return Ok(json!({
            "action": action,
            "command": command_summary_json(&command.summary()),
            "exitCode": 0,
            "issues": issues,
            "stderr": output.stderr,
            "stdout": stdout,
        }));
    }
    Ok(typed_result(&action, &command, output))
}

fn build_beads_command(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<ProcessCommand, TypedOperationError> {
    let bd = require_system_bd().map_err(TypedOperationError::dependency_unavailable)?;
    build_beads_command_with_executable(action, params, context, &bd.executable_path)
}

pub(crate) fn build_beads_command_with_executable(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
    bd_executable_path: &str,
) -> Result<ProcessCommand, TypedOperationError> {
    let cwd = resolve_beads_cwd(context);
    Ok(match action {
        "board" | "list" => {
            ProcessCommand::new(bd_executable_path, vec!["list", "--all", "--json"], &cwd)
        }
        "addLabel" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "label".to_string(),
                "add".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                normalize_required_text(params.get("label"), "label")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "close" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "close".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "comment" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "comments".to_string(),
                "add".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                normalize_required_text(params.get("comment"), "comment")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "configGet" => ProcessCommand::new(
            bd_executable_path,
            vec!["config", "get", "status.custom", "--json"],
            &cwd,
        ),
        "configGetIssuePrefix" => ProcessCommand::new(
            bd_executable_path,
            vec!["config", "get", "issue_prefix", "--json"],
            &cwd,
        ),
        "configSet" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "config".to_string(),
                "set".to_string(),
                "status.custom".to_string(),
                normalize_required_text(params.get("value"), "value")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "renamePrefix" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "rename-prefix".to_string(),
                normalize_beads_rename_prefix(params.get("value"))?,
                "--repair".to_string(),
                "--json".to_string(),
            ],
            &cwd,
        ),
        "create" => ProcessCommand::new(bd_executable_path, build_beads_create_args(params)?, &cwd),
        "delete" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "delete".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--force".to_string(),
                "--json".to_string(),
            ],
            &cwd,
        ),
        "depAdd" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "dep".to_string(),
                "add".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                normalize_issue_id(params.get("dependsOnId"))?,
                "--type".to_string(),
                normalize_beads_dependency_type(params.get("depType"))?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "depRemove" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "dep".to_string(),
                "remove".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                normalize_issue_id(params.get("dependsOnId"))?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "listAllLabels" => {
            /*
            CDXC:ProjectBoardLabels 2026-06-19-14:38:
            Beads label vocabulary requests must share the board list read path instead of calling `label list-all`, because the board list output already contains the labels needed by the UI and avoids the slower issue-by-issue inventory path.
            */
            ProcessCommand::new(bd_executable_path, vec!["list", "--all", "--json"], &cwd)
        }
        "removeLabel" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "label".to_string(),
                "remove".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                normalize_required_text(params.get("label"), "label")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "search" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "search".to_string(),
                normalize_required_text(params.get("query"), "query")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "setLabels" => {
            let mut args = vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
            ];
            args.extend(build_beads_set_label_args(params.get("labels"))?);
            args.push("--json".to_string());
            ProcessCommand::new(bd_executable_path, args, &cwd)
        }
        "show" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "show".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--include-comments".to_string(),
                "--json".to_string(),
            ],
            &cwd,
        ),
        "status" => ProcessCommand::new(bd_executable_path, vec!["status"], &cwd),
        "update" => {
            let mut args = vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
            ];
            args.extend(build_beads_update_args(params)?);
            args.push("--json".to_string());
            ProcessCommand::new(bd_executable_path, args, &cwd)
        }
        "updateDescription" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--description".to_string(),
                nullish_value_to_string(params.get("description"), ""),
                "--json".to_string(),
            ],
            &cwd,
        ),
        "updateEstimate" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--estimate".to_string(),
                normalize_beads_estimate(params.get("estimate"))?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "updatePriority" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--priority".to_string(),
                normalize_required_text(params.get("priority"), "priority")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "updateStatus" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--status".to_string(),
                normalize_beads_status(params.get("status"))?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        "updateTitle" => ProcessCommand::new(
            bd_executable_path,
            vec![
                "update".to_string(),
                normalize_issue_id(params.get("issueId"))?,
                "--title".to_string(),
                normalize_required_text(params.get("title"), "title")?,
                "--json".to_string(),
            ],
            &cwd,
        ),
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported Beads action: {action}"
            )))
        }
    })
}

pub(crate) async fn ensure_beads_git_hooks(
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    /*
    CDXC:WorktreeBeads 2026-06-22-01:09:
    The Rust port must install the same Ghostex-managed Beads hooks as the TypeScript server. Generated hooks live under the common Git directory, call the resolved machine-installed bd by absolute path, pin BEADS_DIR to `bd where --json`, and are written before core.hooksPath is pointed at them.
    */
    if !Path::new(&context.cwd).join(".beads").is_dir() {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": 0,
            "stderr": "",
            "stdout": "skipped: no Beads workspace",
        }));
    }
    let bd = require_system_bd().map_err(TypedOperationError::dependency_unavailable)?;
    ensure_beads_git_hooks_with_executable(context, &bd.executable_path).await
}

pub(crate) async fn ensure_beads_git_hooks_with_executable(
    context: &TypedOperationContext,
    bd_executable_path: &str,
) -> Result<Value, TypedOperationError> {
    let where_command =
        ProcessCommand::new(bd_executable_path, vec!["where", "--json"], &context.cwd);
    let where_result = run_process_command(&where_command, context).await?;
    if where_result.exit_code != 0 {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": 0,
            "stderr": "",
            "stdout": "skipped: no Beads workspace",
        }));
    }
    let beads_path = normalize_beads_where_directory(&where_result.stdout)?;
    let common_git_dir = run_process_command(
        &ProcessCommand::new("git", vec!["rev-parse", "--git-common-dir"], &context.cwd),
        context,
    )
    .await?;
    if common_git_dir.exit_code != 0 || common_git_dir.stdout.trim().is_empty() {
        return Err(TypedOperationError::bad_request(
            common_git_dir
                .stderr
                .or_else("Could not resolve Git common directory."),
        ));
    }
    let hooks_path =
        resolve_path_against(&context.cwd, common_git_dir.stdout.trim()).join("ghostex-hooks");
    fs::create_dir_all(&hooks_path).map_err(|error| {
        TypedOperationError::bad_request(format!("Could not create Beads hooks directory: {error}"))
    })?;
    for hook_name in BEADS_GIT_HOOK_NAMES {
        let hook_path = hooks_path.join(hook_name);
        fs::write(
            &hook_path,
            build_ghostex_beads_git_hook_script(hook_name, bd_executable_path, &beads_path),
        )
        .map_err(|error| {
            TypedOperationError::bad_request(format!("Could not write Beads Git hook: {error}"))
        })?;
        chmod_executable_if_supported(&hook_path)?;
    }
    let config_output = run_process_command(
        &ProcessCommand::new(
            "git",
            vec![
                "config".to_string(),
                "core.hooksPath".to_string(),
                hooks_path.to_string_lossy().to_string(),
            ],
            &context.cwd,
        ),
        context,
    )
    .await?;
    if config_output.exit_code != 0 {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": config_output.exit_code,
            "stderr": config_output.stderr,
            "stdout": if config_output.stdout.is_empty() {
                "Could not configure Git hooks path for Beads worktree support.".to_string()
            } else {
                config_output.stdout
            },
        }));
    }
    Ok(json!({
        "action": "ensureBeadsHooks",
        "exitCode": 0,
        "stderr": "",
        "stdout": "installed",
    }))
}

pub(crate) fn normalize_beads_where_directory(stdout: &str) -> Result<String, TypedOperationError> {
    let parsed: Value = serde_json::from_str(stdout.trim())
        .map_err(|_| TypedOperationError::bad_request("Beads where output was not valid JSON."))?;
    let beads_path = parsed.get("path").and_then(Value::as_str).unwrap_or("");
    if !Path::new(beads_path).is_absolute() {
        return Err(TypedOperationError::bad_request(
            "Beads where output did not include an absolute storage path.",
        ));
    }
    let metadata = fs::metadata(beads_path)
        .map_err(|_| TypedOperationError::bad_request("Beads storage path is not a directory."))?;
    if !metadata.is_dir() {
        return Err(TypedOperationError::bad_request(
            "Beads storage path is not a directory.",
        ));
    }
    Ok(beads_path.to_string())
}

pub(crate) fn build_ghostex_beads_git_hook_script(
    hook_name: &str,
    bd: &str,
    beads_path: &str,
) -> String {
    format!(
        "#!/usr/bin/env sh\n\
# Ghostex-managed Beads hook. This local file is generated under the common Git directory.\n\
BD_BIN={}\n\
BEADS_DIR_VALUE={}\n\
HOOK_NAME={}\n\
if [ ! -x \"$BD_BIN\" ]; then\n\
  echo \"Warning: the configured bd executable is missing; install or update Beads, then reinstall hooks\" >&2\n\
  exit 0\n\
fi\n\
export BEADS_DIR=\"$BEADS_DIR_VALUE\"\n\
export BD_GIT_HOOK=1\n\
export PATH=\"$(dirname \"$BD_BIN\"):$PATH\"\n\
run_bd_hook() {{\n\
  _bd_timeout=${{BEADS_HOOK_TIMEOUT:-300}}\n\
  _bd_used_perl=0\n\
  if command -v timeout >/dev/null 2>&1; then\n\
    timeout \"$_bd_timeout\" \"$BD_BIN\" hooks run \"$HOOK_NAME\" \"$@\"\n\
    _bd_exit=$?\n\
  elif command -v gtimeout >/dev/null 2>&1; then\n\
    gtimeout \"$_bd_timeout\" \"$BD_BIN\" hooks run \"$HOOK_NAME\" \"$@\"\n\
    _bd_exit=$?\n\
  elif command -v perl >/dev/null 2>&1; then\n\
    _bd_used_perl=1\n\
    perl -e 'alarm shift; exec @ARGV' \"$_bd_timeout\" \"$BD_BIN\" hooks run \"$HOOK_NAME\" \"$@\"\n\
    _bd_exit=$?\n\
  else\n\
    echo >&2 \"beads: hook '$HOOK_NAME' running without timeout; install coreutils or perl to enable BEADS_HOOK_TIMEOUT\"\n\
    \"$BD_BIN\" hooks run \"$HOOK_NAME\" \"$@\"\n\
    _bd_exit=$?\n\
  fi\n\
  if [ $_bd_exit -eq 124 ] || {{ [ $_bd_used_perl -eq 1 ] && [ $_bd_exit -eq 142 ]; }}; then\n\
    echo >&2 \"beads: hook '$HOOK_NAME' timed out after ${{_bd_timeout}}s; continuing without beads\"\n\
    _bd_exit=0\n\
  fi\n\
  if [ $_bd_exit -eq 3 ]; then\n\
    echo >&2 \"beads: database not initialized; skipping hook '$HOOK_NAME'\"\n\
    _bd_exit=0\n\
  fi\n\
  return $_bd_exit\n\
}}\n\
run_bd_hook \"$@\"\n\
exit $?\n",
        shell_single_quote(bd),
        shell_single_quote(beads_path),
        shell_single_quote(hook_name),
    )
}

pub(crate) fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn normalize_beads_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "addLabel"
        | "board"
        | "close"
        | "comment"
        | "configGet"
        | "configGetIssuePrefix"
        | "configSet"
        | "create"
        | "delete"
        | "depAdd"
        | "depRemove"
        | "list"
        | "listAllLabels"
        | "renamePrefix"
        | "removeLabel"
        | "search"
        | "setLabels"
        | "show"
        | "status"
        | "storageExists"
        | "update"
        | "updateDescription"
        | "updateEstimate"
        | "updatePriority"
        | "updateStatus"
        | "updateTitle" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported Beads action: {}",
            display_unknown_value(input)
        ))),
    }
}

/*
CDXC:ProjectBoardCustomColumns 2026-08-21:
Beads owns the set of valid statuses: a board can define its own through
`bd config set status.custom`, so a fixed list here was rejecting real statuses
before bd ever saw them and made custom board columns undraggable. Validate the
shape only and let bd be the authority on which statuses exist — it already
returns a clear error for a name it does not know, and args are passed to the
process without a shell, so this is a well-formedness check, not a safety gate.
*/
fn normalize_beads_status(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let status = normalize_required_text(input, "status")?;
    if status.len() <= 64
        && status.chars().enumerate().all(|(index, ch)| {
            (index == 0 && ch.is_ascii_alphabetic())
                || (index > 0 && (ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')))
        })
    {
        Ok(status)
    } else {
        Err(TypedOperationError::bad_request(format!(
            "Unsupported Beads status: {status}"
        )))
    }
}

fn normalize_beads_rename_prefix(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let value = normalize_required_text(input, "value")?.to_ascii_lowercase();
    let mut normalized = String::new();
    for ch in value.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' {
            normalized.push(ch);
        } else if !normalized.ends_with('-') {
            normalized.push('-');
        }
    }
    let trimmed = normalized.trim_matches('-').to_string();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_lowercase())
    {
        return Err(TypedOperationError::bad_request(
            "value must start with a letter after normalization.",
        ));
    }
    Ok(format!("{trimmed}-"))
}

fn normalize_beads_estimate(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let Some(number) = input.and_then(Value::as_i64) else {
        return Err(TypedOperationError::bad_request(
            "estimate must be a non-negative integer.",
        ));
    };
    if number < 0 {
        return Err(TypedOperationError::bad_request(
            "estimate must be a non-negative integer.",
        ));
    }
    Ok(number.to_string())
}

fn normalize_beads_dependency_type(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let dep_type = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("blocks");
    if dep_type.len() <= 32
        && dep_type
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
    {
        Ok(dep_type.to_string())
    } else {
        Err(TypedOperationError::bad_request(
            "depType contains unsupported characters.",
        ))
    }
}

fn build_beads_update_args(
    params: &Map<String, Value>,
) -> Result<Vec<String>, TypedOperationError> {
    let mut args = Vec::new();
    if params.contains_key("status") {
        args.extend([
            "--status".to_string(),
            normalize_beads_status(params.get("status"))?,
        ]);
    }
    if params.contains_key("title") {
        args.extend([
            "--title".to_string(),
            normalize_required_text(params.get("title"), "title")?,
        ]);
    }
    if params.contains_key("description") {
        args.extend([
            "--description".to_string(),
            params
                .get("description")
                .map(value_to_string)
                .unwrap_or_default(),
        ]);
    }
    if params.contains_key("priority") {
        args.extend([
            "--priority".to_string(),
            normalize_required_text(params.get("priority"), "priority")?,
        ]);
    }
    if params.contains_key("estimate") {
        args.extend([
            "--estimate".to_string(),
            normalize_beads_estimate(params.get("estimate"))?,
        ]);
    }
    if params.contains_key("labels") {
        args.extend(build_beads_set_label_args(params.get("labels"))?);
    }
    if args.is_empty() {
        return Err(TypedOperationError::bad_request(
            "Beads update requires at least one typed field.",
        ));
    }
    Ok(args)
}

fn build_beads_create_args(
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
        "task".to_string(),
    ];
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

fn build_beads_set_label_args(input: Option<&Value>) -> Result<Vec<String>, TypedOperationError> {
    let labels = match input {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(labels)) => labels.clone(),
        Some(_) => {
            return Err(TypedOperationError::bad_request(
                "labels must be an array of non-empty strings.",
            ))
        }
    };
    let mut args = Vec::new();
    for label in labels {
        args.extend([
            "--set-labels".to_string(),
            normalize_required_text(Some(&label), "label")?,
        ]);
    }
    Ok(args)
}

fn resolve_beads_cwd(context: &TypedOperationContext) -> String {
    context
        .beads_cwd
        .clone()
        .unwrap_or_else(|| context.cwd.clone())
}

pub(crate) fn parse_beads_board_output(
    stdout: &str,
) -> Result<(Vec<Value>, String), TypedOperationError> {
    parse_beads_board_output_with_limits(stdout, default_beads_board_limits())
}

pub(crate) fn parse_beads_show_output(
    stdout: &str,
) -> Result<(Value, String), TypedOperationError> {
    let parsed: Value = serde_json::from_str(stdout.trim()).map_err(|_| {
        TypedOperationError::bad_request("Beads issue detail output was not valid JSON.")
    })?;
    let payload = if let Some(data) = parsed.get("data") {
        data
    } else {
        &parsed
    };
    let issue = match payload {
        Value::Array(items) => {
            if items.len() != 1 {
                return Err(TypedOperationError::bad_request(
                    "Beads issue detail output must contain exactly one issue object.",
                ));
            }
            let issue = items[0].clone();
            if !issue.is_object() {
                return Err(TypedOperationError::bad_request(
                    "Beads issue detail output must contain exactly one issue object.",
                ));
            }
            issue
        }
        Value::Object(_) => payload.clone(),
        _ => {
            return Err(TypedOperationError::bad_request(
                "Beads issue detail output must contain exactly one issue object.",
            ))
        }
    };
    let stdout = serde_json::to_string(&issue)
        .map_err(|_| TypedOperationError::bad_request("Could not serialize Beads issue detail."))?;
    Ok((issue, stdout))
}

fn default_beads_board_limits() -> BeadsBoardLimits {
    BeadsBoardLimits {
        response_limit_bytes: BEADS_BOARD_RESPONSE_LIMIT_BYTES,
        row_limit: BEADS_BOARD_ROW_LIMIT,
    }
}

pub(crate) fn parse_beads_board_output_with_limits(
    stdout: &str,
    limits: BeadsBoardLimits,
) -> Result<(Vec<Value>, String), TypedOperationError> {
    let parsed: Value = serde_json::from_str(if stdout.trim().is_empty() {
        "[]"
    } else {
        stdout.trim()
    })
    .map_err(|_| TypedOperationError::bad_request("Beads board output was not valid JSON."))?;
    let payload = parsed
        .get("data")
        .and_then(Value::as_array)
        .cloned()
        .or_else(|| parsed.as_array().cloned())
        .ok_or_else(|| {
            TypedOperationError::bad_request("Beads board output must be a JSON array.")
        })?;
    let mut issues = Vec::new();
    let mut serialized = Vec::new();
    let mut response_bytes = 2usize;
    let mut row_count = 0usize;
    for item in payload {
        if !item.is_object() {
            continue;
        }
        row_count += 1;
        if row_count > limits.row_limit {
            return Err(TypedOperationError::bad_request(format!(
                "Beads board state exceeds the {}-row limit; refusing to return oversized board data.",
                limits.row_limit
            ))
            .with_details(json!({
                "rowCount": row_count,
                "rowLimit": limits.row_limit,
            })));
        }
        let text = serde_json::to_string(&item).unwrap_or_else(|_| "{}".to_string());
        let next_response_bytes =
            response_bytes + text.len() + if serialized.is_empty() { 0 } else { 1 };
        if next_response_bytes > limits.response_limit_bytes {
            return Err(TypedOperationError::bad_request(format!(
                "Beads board response exceeds the {}-byte serialized JSON limit; refusing to return oversized board data.",
                limits.response_limit_bytes
            ))
            .with_details(json!({
                "capturedBytes": response_bytes,
                "responseLimitBytes": limits.response_limit_bytes,
                "rowCount": row_count,
            })));
        }
        response_bytes = next_response_bytes;
        issues.push(item);
        serialized.push(text);
    }
    Ok((issues, format!("[{}]", serialized.join(","))))
}

pub(crate) fn derive_beads_label_counts(issues: &[Value]) -> Vec<BeadsLabelCount> {
    let mut counts = HashMap::<String, usize>::new();
    for issue in issues {
        let labels = issue
            .get("labels")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for label in labels {
            let normalized = label.as_str().map(str::trim).unwrap_or("");
            if normalized.is_empty() {
                continue;
            }
            *counts.entry(normalized.to_string()).or_insert(0) += 1;
        }
    }
    let mut labels = counts
        .into_iter()
        .map(|(label, count)| BeadsLabelCount { count, label })
        .collect::<Vec<_>>();
    labels.sort_by(|left, right| compare_beads_label_locale_order(&left.label, &right.label));
    labels
}

fn compare_beads_label_locale_order(left: &str, right: &str) -> Ordering {
    /*
    CDXC:ProjectBoardLabels 2026-06-19-14:44:
    TypeScript sorts derived Beads labels with `localeCompare`. Keep Rust's common ASCII label ordering aligned without adding an ICU dependency to this typed-operation path; uncommon Unicode labels still fall back to deterministic string ordering after the ASCII-compatible pass.
    */
    let primary = compare_label_primary_order(left, right);
    if primary != Ordering::Equal {
        return primary;
    }
    compare_label_case_order(left, right).then_with(|| left.cmp(right))
}

fn compare_label_primary_order(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.chars();
    let mut right_chars = right.chars();
    loop {
        match (left_chars.next(), right_chars.next()) {
            (Some(left_char), Some(right_char)) => {
                let ordering =
                    label_char_primary_key(left_char).cmp(&label_char_primary_key(right_char));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn label_char_primary_key(ch: char) -> (u8, char) {
    match ch {
        '_' => (0, '\0'),
        '-' => (1, '\0'),
        ':' => (2, '\0'),
        '.' => (3, '\0'),
        '/' => (4, '\0'),
        ch if ch.is_ascii_digit() => (5, ch),
        ch if ch.is_ascii_alphabetic() => (6, ch.to_ascii_lowercase()),
        ch => (7, ch.to_lowercase().next().unwrap_or(ch)),
    }
}

fn compare_label_case_order(left: &str, right: &str) -> Ordering {
    let mut left_chars = left.chars();
    let mut right_chars = right.chars();
    loop {
        match (left_chars.next(), right_chars.next()) {
            (Some(left_char), Some(right_char)) => {
                let ordering = label_char_case_key(left_char).cmp(&label_char_case_key(right_char));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn label_char_case_key(ch: char) -> u8 {
    if ch.is_uppercase() {
        1
    } else {
        0
    }
}
