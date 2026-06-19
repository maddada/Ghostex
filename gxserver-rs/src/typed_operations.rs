use std::{
    cmp::Ordering,
    collections::HashMap,
    env, fmt, fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

use serde_json::{json, Map, Value};
use tokio::{
    io::AsyncWriteExt,
    process::Command,
    time::{timeout, Instant},
};

use crate::toolchain::require_bundled_bd;

const TYPED_OPERATION_TIMEOUT_MS: u64 = 120_000;
const TYPED_OPERATION_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const TYPED_OPERATION_STDERR_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const BEADS_BOARD_RESPONSE_LIMIT_BYTES: usize = TYPED_OPERATION_STDOUT_LIMIT_BYTES;
const BEADS_BOARD_ROW_LIMIT: usize = 5_000;
const COMMIT_MESSAGE_LIMIT_BYTES: usize = 64 * 1024;
const SETUP_COMMAND_LIMIT_BYTES: usize = 16 * 1024;
const FILE_PATH_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct TypedOperationError {
    pub code: &'static str,
    pub message: String,
}

impl TypedOperationError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            message: message.into(),
        }
    }

    fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "dependencyUnavailable",
            message: message.into(),
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            code: "forbidden",
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            message: message.into(),
        }
    }
}

impl fmt::Display for TypedOperationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(formatter)
    }
}

impl std::error::Error for TypedOperationError {}

#[derive(Clone, Debug)]
pub struct TypedOperationContext {
    beads_cwd: Option<String>,
    cwd: String,
    env_path: Option<String>,
    projects: Vec<Value>,
}

#[derive(Clone, Debug)]
struct ProcessCommand {
    args: Vec<String>,
    cwd: String,
    executable: String,
    result_command: Option<CommandSummary>,
    stdin: Option<String>,
}

#[derive(Clone, Debug)]
struct CommandSummary {
    args: Vec<String>,
    cwd: String,
    executable: String,
}

#[derive(Debug)]
struct CommandOutput {
    error: Option<Value>,
    exit_code: i32,
    stderr: String,
    stdout: String,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct BeadsLabelCount {
    count: usize,
    label: String,
}

/*
CDXC:GxserverRustPort 2026-06-16-00:49:
Phase 7 typed operations are shared backend command surfaces, not arbitrary remote shell access. Rust mirrors the TypeScript allowlists, registered-project scope checks, redacted command metadata, bounded subprocess output, and pinned Beads resolution so clients can opt into Rust without receiving broader filesystem or command execution power.
*/
pub async fn dispatch_typed_operation_endpoint(
    endpoint_path: &str,
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<Value, TypedOperationError> {
    let context = resolve_project_operation_context(endpoint_path, params, projects)?;
    match endpoint_path {
        "/api/runGitAction" => run_git_action(params, &context).await,
        "/api/runGitHubAction" => run_github_action(params, &context).await,
        "/api/runWorktreeAction" => run_worktree_action(params, &context).await,
        "/api/runProjectSetupCommand" => run_project_setup_command(params, &context).await,
        "/api/runBeadsAction" => run_beads_action(params, &context).await,
        _ => Err(TypedOperationError::not_found(format!(
            "{endpoint_path} is not a gxserver typed operation endpoint."
        ))),
    }
}

pub fn is_expected_non_zero_typed_operation(action: &str, exit_code: i32, has_error: bool) -> bool {
    exit_code != 0
        && !has_error
        && matches!(
            action,
            "diffNoIndexAgainstNull"
                | "getOriginRemoteUrl"
                | "isInsideWorkTree"
                | "prView"
                | "remoteBranchExists"
                | "storageExists"
                | "upstreamCounts"
                | "verifyRef"
        )
}

pub fn typed_operation_log_level(action: &str, exit_code: i32, has_error: bool) -> &'static str {
    if exit_code == 0 || is_expected_non_zero_typed_operation(action, exit_code, has_error) {
        "info"
    } else {
        "warn"
    }
}

pub fn typed_operation_log_details(result: &Value) -> Value {
    let action = result
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let command = result.get("command").and_then(Value::as_object);
    let args_len = command
        .and_then(|command| command.get("args"))
        .and_then(Value::as_array)
        .map(Vec::len);
    let executable = command
        .and_then(|command| command.get("executable"))
        .and_then(Value::as_str);
    let exit_code = result.get("exitCode").and_then(Value::as_i64).unwrap_or(1);
    let has_error = result.get("error").is_some();
    json!({
        "action": action,
        "argumentCount": args_len,
        "commandBuilt": command.is_some(),
        "executable": executable,
        "expectedNonZero": is_expected_non_zero_typed_operation(action, exit_code as i32, has_error),
        "exitCode": exit_code,
        "operationError": result.get("error").cloned(),
    })
}

fn resolve_project_operation_context(
    endpoint_path: &str,
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<TypedOperationContext, TypedOperationError> {
    let project = resolve_scoped_project(&projects, params)?;
    let project_id = read_project_id_value(&project)?;
    let project_path = project.get("path").and_then(Value::as_str).ok_or_else(|| {
        TypedOperationError::bad_request(format!("Project {project_id} has no filesystem path."))
    })?;
    let cwd = normalize_existing_directory_path_value(
        Some(&Value::String(project_path.to_string())),
        "project.path",
    )?;
    let beads_cwd = if endpoint_path == "/api/runBeadsAction"
        && params.get("projectBoardScope").and_then(Value::as_bool) == Some(true)
    {
        project
            .get("projectBoardConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("beadsDirectory"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|path| {
                normalize_existing_directory_path_value(
                    Some(&Value::String(path.to_string())),
                    "beadsDirectory",
                )
            })
            .transpose()?
    } else {
        None
    };
    Ok(TypedOperationContext {
        beads_cwd,
        cwd,
        env_path: env::var("PATH").ok(),
        projects,
    })
}

fn resolve_scoped_project(
    projects: &[Value],
    scope: &Map<String, Value>,
) -> Result<Value, TypedOperationError> {
    if let Some(project_id) = scope
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let project = projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .cloned()
            .ok_or_else(|| {
                TypedOperationError::not_found(format!("Project {project_id} does not exist."))
            })?;
        if scope.contains_key("projectPath") {
            let scoped_path =
                normalize_existing_directory_path_value(scope.get("projectPath"), "projectPath")?;
            let project_path = project
                .get("path")
                .and_then(Value::as_str)
                .map(|path| {
                    normalize_absolute_path_value(
                        Some(&Value::String(path.to_string())),
                        "project.path",
                    )
                })
                .transpose()?;
            if project_path.as_deref() != Some(scoped_path.as_str()) {
                return Err(TypedOperationError::forbidden(
                    "projectPath does not match the requested projectId.",
                ));
            }
        }
        return Ok(project);
    }

    let scoped_path =
        normalize_existing_directory_path_value(scope.get("projectPath"), "projectPath")?;
    projects
        .iter()
        .find(|candidate| {
            candidate
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| {
                    normalize_absolute_path_value(
                        Some(&Value::String(path.to_string())),
                        "project.path",
                    )
                    .ok()
                })
                .as_deref()
                == Some(scoped_path.as_str())
        })
        .cloned()
        .ok_or_else(|| {
            TypedOperationError::forbidden(
                "projectPath must be a registered gxserver project path.",
            )
        })
}

async fn run_git_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_git_action(params.get("action"))?;
    if action == "countFileLines" {
        let file_paths = optional_relative_file_paths(params.get("filePaths"))?;
        let line_count = count_project_file_lines(&context.cwd, &file_paths)?;
        return Ok(json!({
            "action": action,
            "exitCode": 0,
            "stderr": "",
            "stdout": line_count.to_string(),
        }));
    }
    let command = build_git_command(&action, params, &context.cwd)?;
    let output = run_process_command(&command, context).await?;
    Ok(typed_result(&action, &command, output))
}

async fn run_github_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_github_action(params.get("action"))?;
    let command = build_github_command(&action, &context.cwd);
    match run_process_command(&command, context).await {
        Ok(output) => Ok(typed_result(&action, &command, output)),
        Err(error) => Ok(json!({
            "action": action,
            "command": command_summary_json(&command.summary()),
            "exitCode": 1,
            "stderr": error.message,
            "stdout": "",
        })),
    }
}

async fn run_worktree_action(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = normalize_worktree_action(params.get("action"))?;
    if action == "pathExists" {
        let worktree_path = normalize_worktree_target_path(params.get("worktreePath"), context)?;
        let exists = Path::new(&worktree_path).exists();
        return Ok(json!({
            "action": action,
            "exitCode": if exists { 0 } else { 1 },
            "stderr": "",
            "stdout": if exists { "true" } else { "false" },
        }));
    }
    if action == "ensureBeadsHooks" {
        return ensure_beads_git_hooks(context).await;
    }
    let command = build_worktree_command(&action, params, context)?;
    let output = run_process_command(&command, context).await?;
    let mut result = typed_result(&action, &command, output);
    if action == "list" && result.get("exitCode").and_then(Value::as_i64) == Some(0) {
        let stdout = result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        result.as_object_mut().expect("result object").insert(
            "worktrees".to_string(),
            Value::Array(parse_git_worktree_list_porcelain(&stdout)),
        );
    }
    Ok(result)
}

async fn run_project_setup_command(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let action = params
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            TypedOperationError::bad_request("Unsupported project setup action: undefined")
        })?;
    if action != "worktreeSetupCommand" {
        return Err(TypedOperationError::bad_request(format!(
            "Unsupported project setup action: {}",
            params
                .get("action")
                .map(value_to_string)
                .unwrap_or_else(|| "undefined".to_string())
        )));
    }
    let setup_project = resolve_project_setup_command_project(params, context)?;
    let command_text = normalize_project_setup_command(
        setup_project
            .get("gitConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("worktreeCommand")),
    )?;
    if command_text.is_empty() {
        return Ok(json!({
            "action": action,
            "exitCode": 0,
            "stderr": "",
            "stdout": "",
        }));
    }
    let command = ProcessCommand {
        args: vec!["-lc".to_string(), command_text],
        cwd: context.cwd.clone(),
        executable: "/bin/zsh".to_string(),
        result_command: Some(CommandSummary {
            args: vec!["-lc".to_string(), "<worktree setup command>".to_string()],
            cwd: context.cwd.clone(),
            executable: "/bin/zsh".to_string(),
        }),
        stdin: None,
    };
    let output = run_process_command(&command, context).await?;
    Ok(typed_result(action, &command, output))
}

async fn run_beads_action(
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
    let command = build_beads_command(&action, params, context)?;
    let output = run_process_command(&command, context).await?;
    if matches!(action.as_str(), "board" | "listAllLabels") && output.exit_code == 0 {
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

fn build_git_command(
    action: &str,
    params: &Map<String, Value>,
    cwd: &str,
) -> Result<ProcessCommand, TypedOperationError> {
    let command = match action {
        "addAll" => {
            let files = optional_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec!["add".to_string(), "-A".to_string()];
            let result_command = if files.is_empty() {
                None
            } else {
                args.push("--".to_string());
                args.extend(files.clone());
                Some(CommandSummary {
                    args: vec![
                        "add".to_string(),
                        "-A".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                })
            };
            ProcessCommand::new("git", args, cwd).with_result_command(result_command)
        }
        "branch" => ProcessCommand::new("git", vec!["branch", "--show-current"], cwd),
        "checkout" => ProcessCommand::new(
            "git",
            vec![
                "checkout".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "checkoutNewBranch" => ProcessCommand::new(
            "git",
            vec![
                "checkout".to_string(),
                "-b".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "deleteLocalBranch" => {
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "branch".to_string(),
                    "-d".to_string(),
                    "--".to_string(),
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec!["branch", "-d", "--", "<branch>"])
        }
        "deleteRemoteBranch" => {
            let remote = normalize_git_remote_name(params.get("remoteName"))?;
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec!["push".to_string(), remote, "--delete".to_string(), branch],
                cwd,
            )
            .with_result_args(vec!["push", "<remote>", "--delete", "<branch>"])
        }
        "diff" => {
            let mut args = vec!["diff".to_string(), "--".to_string()];
            args.extend(optional_relative_file_path(params.get("filePath"))?);
            ProcessCommand::new("git", args, cwd)
        }
        "diffCached" => ProcessCommand::new("git", vec!["diff", "--cached"], cwd),
        "diffCachedNoExt" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffCachedStat" => ProcessCommand::new("git", vec!["diff", "--cached", "--stat"], cwd),
        "diffNoExt" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffNoIndexAgainstNull" => ProcessCommand::new(
            "git",
            vec![
                "diff".to_string(),
                "--no-index".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
                "/dev/null".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "diffNumstat" => ProcessCommand::new("git", vec!["diff", "--numstat", "HEAD"], cwd),
        "getOriginRemoteUrl" => {
            ProcessCommand::new("git", vec!["remote", "get-url", "origin"], cwd)
        }
        "isInsideWorkTree" => {
            ProcessCommand::new("git", vec!["rev-parse", "--is-inside-work-tree"], cwd)
        }
        "isUntrackedFile" => ProcessCommand::new(
            "git",
            vec![
                "ls-files".to_string(),
                "--others".to_string(),
                "--exclude-standard".to_string(),
                "--".to_string(),
                normalize_relative_file_path(params.get("filePath"))?,
            ],
            cwd,
        ),
        "list" => ProcessCommand::new(
            "git",
            vec![
                "ls-files",
                "--cached",
                "--modified",
                "--others",
                "--exclude-standard",
            ],
            cwd,
        ),
        "listRemotes" => ProcessCommand::new("git", vec!["remote"], cwd),
        "listUntracked" => ProcessCommand::new(
            "git",
            vec!["ls-files", "--others", "--exclude-standard", "-z"],
            cwd,
        ),
        "status" => ProcessCommand::new("git", vec!["status", "--short", "--branch"], cwd),
        "statusPorcelain" => ProcessCommand::new("git", vec!["status", "--porcelain"], cwd),
        "upstreamCounts" => ProcessCommand::new(
            "git",
            vec!["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
            cwd,
        ),
        "merge" => ProcessCommand::new(
            "git",
            vec![
                "merge".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "commit" => {
            let message = normalize_git_commit_message(
                params.get("messageSubject"),
                params.get("messageBody"),
            )?;
            let mut args = vec!["commit".to_string()];
            if params.get("noVerify").and_then(Value::as_bool) == Some(true) {
                args.push("--no-verify".to_string());
            }
            args.extend(["-F".to_string(), "-".to_string()]);
            let result_args = args
                .iter()
                .map(|arg| {
                    if arg == "-" {
                        "<stdin>".to_string()
                    } else {
                        arg.clone()
                    }
                })
                .collect();
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: result_args,
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_stdin(message)
        }
        "pullFastForward" => {
            /*
            CDXC:GitOperations 2026-06-19-14:38:
            The titlebar remote-sync workflow must update the current branch only through Git's fast-forward pull contract. Rust keeps the typed operation to `git pull --ff-only` so merge, rebase, dirty-worktree, and divergent-history failures remain visible to callers instead of being hidden by fallback behavior.
            */
            ProcessCommand::new("git", vec!["pull", "--ff-only"], cwd)
        }
        "push" => ProcessCommand::new("git", vec!["push"], cwd),
        "pushSetUpstream" => ProcessCommand::new(
            "git",
            vec![
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        "remoteBranchExists" => {
            let remote = normalize_git_remote_name(params.get("remoteName"))?;
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "ls-remote".to_string(),
                    "--exit-code".to_string(),
                    "--heads".to_string(),
                    remote,
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec![
                "ls-remote",
                "--exit-code",
                "--heads",
                "<remote>",
                "<branch>",
            ])
        }
        "verifyRef" => ProcessCommand::new(
            "git",
            vec![
                "rev-parse".to_string(),
                "--verify".to_string(),
                normalize_git_ref(params.get("ref"), "ref")?,
            ],
            cwd,
        ),
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported Git action: {action}"
            )))
        }
    };
    Ok(command)
}

fn build_github_command(action: &str, cwd: &str) -> ProcessCommand {
    match action {
        "prCreateFill" => ProcessCommand::new("gh", vec!["pr", "create", "--fill"], cwd),
        "prView" => ProcessCommand::new(
            "gh",
            vec!["pr", "view", "--json", "number,state,title,url"],
            cwd,
        ),
        "version" => ProcessCommand::new("gh", vec!["--version"], cwd),
        _ => unreachable!("validated GitHub action"),
    }
}

fn build_worktree_command(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<ProcessCommand, TypedOperationError> {
    let cwd = context.cwd.as_str();
    Ok(match action {
        "create" => {
            let worktree_path =
                normalize_worktree_target_path(params.get("worktreePath"), context)?;
            let branch = normalize_optional_git_ref(params.get("branch"), "branch")?;
            let base_ref = normalize_optional_git_ref(params.get("baseRef"), "baseRef")?;
            let mut args = vec!["worktree".to_string(), "add".to_string(), worktree_path];
            if let Some(branch) = branch {
                args.extend(["-b".to_string(), branch]);
            }
            if let Some(base_ref) = base_ref {
                args.push(base_ref);
            }
            ProcessCommand::new("git", args, cwd)
        }
        "list" => ProcessCommand::new("git", vec!["worktree", "list", "--porcelain"], cwd),
        "prune" => ProcessCommand::new("git", vec!["worktree", "prune"], cwd),
        "remove" => {
            let worktree_path =
                normalize_existing_worktree_path(params.get("worktreePath"), context)?;
            let mut args = vec!["worktree".to_string(), "remove".to_string()];
            if params.get("force").and_then(Value::as_bool) == Some(true) {
                args.push("--force".to_string());
            }
            args.extend(["--".to_string(), worktree_path]);
            ProcessCommand::new("git", args, cwd)
        }
        "switch" => ProcessCommand::new(
            "git",
            vec![
                "switch".to_string(),
                normalize_git_ref(params.get("branch"), "branch")?,
            ],
            cwd,
        ),
        _ => {
            return Err(TypedOperationError::bad_request(format!(
                "Unsupported worktree action: {action}"
            )))
        }
    })
}

fn build_beads_command(
    action: &str,
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<ProcessCommand, TypedOperationError> {
    let bd = require_bundled_bd().map_err(TypedOperationError::dependency_unavailable)?;
    build_beads_command_with_executable(action, params, context, &bd.executable_path)
}

fn build_beads_command_with_executable(
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
                params
                    .get("description")
                    .map(value_to_string)
                    .unwrap_or_default(),
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

async fn ensure_beads_git_hooks(
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    if !Path::new(&context.cwd).join(".beads").is_dir() {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": 0,
            "stderr": "",
            "stdout": "skipped: no Beads workspace",
        }));
    }
    let bd = require_bundled_bd().map_err(TypedOperationError::dependency_unavailable)?;
    let where_command =
        ProcessCommand::new(&bd.executable_path, vec!["where", "--json"], &context.cwd);
    let where_result = run_process_command(&where_command, context).await?;
    if where_result.exit_code != 0 {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": 0,
            "stderr": "",
            "stdout": "skipped: no Beads workspace",
        }));
    }
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
    let hooks_path = Path::new(&context.cwd)
        .join(common_git_dir.stdout.trim())
        .join("ghostex-hooks");
    fs::create_dir_all(&hooks_path).map_err(|error| {
        TypedOperationError::bad_request(format!("Could not create Beads hooks directory: {error}"))
    })?;
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

impl ProcessCommand {
    fn new<S, A, C>(executable: S, args: Vec<A>, cwd: C) -> Self
    where
        S: Into<String>,
        A: Into<String>,
        C: Into<String>,
    {
        Self {
            args: args.into_iter().map(Into::into).collect(),
            cwd: cwd.into(),
            executable: executable.into(),
            result_command: None,
            stdin: None,
        }
    }

    fn with_result_args(mut self, args: Vec<&str>) -> Self {
        self.result_command = Some(CommandSummary {
            args: args.into_iter().map(str::to_string).collect(),
            cwd: self.cwd.clone(),
            executable: self.executable.clone(),
        });
        self
    }

    fn with_result_command(mut self, result_command: Option<CommandSummary>) -> Self {
        self.result_command = result_command;
        self
    }

    fn with_stdin(mut self, stdin: String) -> Self {
        self.stdin = Some(stdin);
        self
    }

    fn summary(&self) -> CommandSummary {
        self.result_command
            .clone()
            .unwrap_or_else(|| CommandSummary {
                args: self.args.clone(),
                cwd: self.cwd.clone(),
                executable: self.executable.clone(),
            })
    }
}

trait StringOrElse {
    fn or_else(self, fallback: &str) -> String;
}

impl StringOrElse for String {
    fn or_else(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

async fn run_process_command(
    command: &ProcessCommand,
    context: &TypedOperationContext,
) -> Result<CommandOutput, TypedOperationError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.args)
        .current_dir(&command.cwd)
        .envs(typed_operation_environment(context.env_path.as_deref()))
        .kill_on_drop(true)
        .stdin(if command.stdin.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::null()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = process.spawn().map_err(|error| {
        TypedOperationError::dependency_unavailable(format!(
            "Could not start typed operation command: {error}"
        ))
    })?;

    if let Some(stdin) = command.stdin.as_deref() {
        if let Some(mut child_stdin) = child.stdin.take() {
            if child_stdin.write_all(stdin.as_bytes()).await.is_err() {
                let _ = child.start_kill();
                return Ok(CommandOutput {
                    error: Some(json!({
                        "code": "stdinFailed",
                        "message": "Typed operation stdin write failed.",
                    })),
                    exit_code: 1,
                    stderr: String::new(),
                    stdout: String::new(),
                });
            }
            let _ = child_stdin.shutdown().await;
        }
    }

    let started = Instant::now();
    let result = timeout(
        Duration::from_millis(TYPED_OPERATION_TIMEOUT_MS),
        child.wait_with_output(),
    )
    .await;
    let output = match result {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            return Err(TypedOperationError::dependency_unavailable(format!(
                "Typed operation command failed: {error}"
            )))
        }
        Err(_) => {
            return Ok(CommandOutput {
                error: Some(json!({
                    "code": "timeout",
                    "message": format!("Typed operation timed out after {TYPED_OPERATION_TIMEOUT_MS}ms."),
                    "timeoutMs": TYPED_OPERATION_TIMEOUT_MS,
                })),
                exit_code: 1,
                stderr: String::new(),
                stdout: String::new(),
            })
        }
    };
    let mut stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.stdout.len() > TYPED_OPERATION_STDOUT_LIMIT_BYTES {
        stdout = String::from_utf8_lossy(&output.stdout[..TYPED_OPERATION_STDOUT_LIMIT_BYTES])
            .trim()
            .to_string();
        return Ok(CommandOutput {
            error: Some(json!({
                "capturedBytes": TYPED_OPERATION_STDOUT_LIMIT_BYTES,
                "code": "stdoutLimitExceeded",
                "limitBytes": TYPED_OPERATION_STDOUT_LIMIT_BYTES,
                "message": format!("Typed operation stdout exceeded {TYPED_OPERATION_STDOUT_LIMIT_BYTES} bytes."),
                "stream": "stdout",
            })),
            exit_code: 1,
            stderr,
            stdout,
        });
    }
    if output.stderr.len() > TYPED_OPERATION_STDERR_LIMIT_BYTES {
        stderr = String::from_utf8_lossy(&output.stderr[..TYPED_OPERATION_STDERR_LIMIT_BYTES])
            .trim()
            .to_string();
        return Ok(CommandOutput {
            error: Some(json!({
                "capturedBytes": TYPED_OPERATION_STDERR_LIMIT_BYTES,
                "code": "stderrLimitExceeded",
                "limitBytes": TYPED_OPERATION_STDERR_LIMIT_BYTES,
                "message": format!("Typed operation stderr exceeded {TYPED_OPERATION_STDERR_LIMIT_BYTES} bytes."),
                "stream": "stderr",
            })),
            exit_code: 1,
            stderr,
            stdout,
        });
    }
    let _duration_ms = started.elapsed().as_millis();
    Ok(CommandOutput {
        error: None,
        exit_code: output.status.code().unwrap_or(1),
        stderr,
        stdout,
    })
}

fn typed_operation_environment(env_path: Option<&str>) -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = env::vars().collect();
    environment.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "ANSI_COLORS_DISABLED" | "NO_COLOR" | "NODE_DISABLE_COLORS"
        )
    });
    if let Some(path) = env_path {
        environment.retain(|(key, _)| key != "PATH");
        environment.push(("PATH".to_string(), path.to_string()));
    }
    environment
}

fn typed_result(action: &str, command: &ProcessCommand, output: CommandOutput) -> Value {
    let mut result = Map::new();
    result.insert("action".to_string(), json!(action));
    result.insert(
        "command".to_string(),
        command_summary_json(&command.summary()),
    );
    if let Some(error) = output.error {
        result.insert("error".to_string(), error);
    }
    result.insert("exitCode".to_string(), json!(output.exit_code));
    result.insert("stderr".to_string(), json!(output.stderr));
    result.insert("stdout".to_string(), json!(output.stdout));
    Value::Object(result)
}

fn command_summary_json(command: &CommandSummary) -> Value {
    json!({
        "args": command.args,
        "cwd": command.cwd,
        "executable": command.executable,
    })
}

fn read_project_id_value(project: &Value) -> Result<String, TypedOperationError> {
    project
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TypedOperationError::bad_request("Project is missing projectId."))
}

fn normalize_absolute_path_value(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be an absolute path or start with ~/"
        )));
    }
    Ok(normalize_path_string(expanded))
}

fn normalize_existing_directory_path_value(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let path = normalize_absolute_path_value(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| TypedOperationError::not_found(format!("{field} does not exist: {path}")))?;
    if !metadata.is_dir() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not a directory: {path}"
        )));
    }
    Ok(path)
}

fn normalize_absolute_path(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty absolute path or ~/ path."
        )));
    }
    let expanded = expand_user_path(text);
    if !expanded.is_absolute() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be absolute or start with ~/."
        )));
    }
    Ok(normalize_path_string(expanded))
}

fn expand_user_path(input: &str) -> PathBuf {
    if input == "~" {
        return home_dir();
    }
    if let Some(rest) = input.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(input)
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn normalize_path_string(path: PathBuf) -> String {
    path.components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .to_string()
}

fn is_path_inside(parent_path: &str, candidate_path: &str) -> bool {
    let parent = Path::new(parent_path);
    let candidate = Path::new(candidate_path);
    if parent == candidate {
        return true;
    }
    candidate.starts_with(parent)
}

fn optional_relative_file_path(input: Option<&Value>) -> Result<Vec<String>, TypedOperationError> {
    if input.is_none() || input == Some(&Value::Null) || input.and_then(Value::as_str) == Some("") {
        return Ok(Vec::new());
    }
    Ok(vec![normalize_relative_file_path(input)?])
}

fn optional_relative_file_paths(input: Option<&Value>) -> Result<Vec<String>, TypedOperationError> {
    let Some(value) = input else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }
    let array = value.as_array().ok_or_else(|| {
        TypedOperationError::bad_request("filePaths must be an array of relative paths.")
    })?;
    if array.len() > FILE_PATH_LIMIT {
        return Err(TypedOperationError::bad_request(
            "filePaths exceeds the 500-file limit.",
        ));
    }
    array
        .iter()
        .map(|value| normalize_relative_file_path(Some(value)))
        .collect()
}

fn normalize_relative_file_path(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).unwrap_or("");
    if text.is_empty()
        || Path::new(text).is_absolute()
        || text.split(['/', '\\']).any(|part| part == "..")
    {
        return Err(TypedOperationError::bad_request(
            "filePath must be a relative path inside the project.",
        ));
    }
    let normalized = text
        .replace('\\', "/")
        .trim_start_matches('/')
        .trim()
        .to_string();
    if normalized.is_empty() {
        return Err(TypedOperationError::bad_request(
            "filePath must be a relative path inside the project.",
        ));
    }
    Ok(normalized)
}

fn normalize_git_commit_message(
    subject_input: Option<&Value>,
    body_input: Option<&Value>,
) -> Result<String, TypedOperationError> {
    let subject = normalize_required_text(subject_input, "messageSubject")?;
    let body = body_input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let message = if let Some(body) = body {
        format!("{subject}\n\n{body}")
    } else {
        subject
    };
    if message.len() > COMMIT_MESSAGE_LIMIT_BYTES {
        return Err(TypedOperationError::bad_request(
            "Commit message exceeds the 65536-byte limit.",
        ));
    }
    Ok(format!("{message}\n"))
}

fn normalize_git_ref(input: Option<&Value>, field: &str) -> Result<String, TypedOperationError> {
    let value = normalize_required_text(input, field)?;
    if !is_allowed_git_ref(&value) {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not an allowed Git ref."
        )));
    }
    Ok(value)
}

fn normalize_optional_git_ref(
    input: Option<&Value>,
    field: &str,
) -> Result<Option<String>, TypedOperationError> {
    if input.is_none() || input == Some(&Value::Null) || input.and_then(Value::as_str) == Some("") {
        return Ok(None);
    }
    normalize_git_ref(input, field).map(Some)
}

fn is_allowed_git_ref(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}

fn normalize_git_remote_name(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let value = if input.is_none() || input == Some(&Value::Null) {
        "origin".to_string()
    } else {
        normalize_required_text(input, "remoteName")?
    };
    if value.chars().enumerate().all(|(index, ch)| {
        (index == 0 && ch.is_ascii_alphanumeric())
            || (index > 0 && (ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')))
    }) {
        return Ok(value);
    }
    Err(TypedOperationError::bad_request(
        "remoteName is not an allowed Git remote name.",
    ))
}

fn normalize_issue_id(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let issue_id = normalize_required_text(input, "issueId")?;
    let mut chars = issue_id.chars();
    if matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && issue_id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        return Ok(issue_id);
    }
    Err(TypedOperationError::bad_request(
        "issueId contains unsupported characters.",
    ))
}

fn normalize_beads_status(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let status = normalize_required_text(input, "status")?;
    if matches!(
        status.as_str(),
        "backlog" | "closed" | "in_progress" | "open" | "review" | "test"
    ) {
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

fn normalize_required_text(
    input: Option<&Value>,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.and_then(Value::as_str).map(str::trim).unwrap_or("");
    if text.is_empty() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} must be a non-empty string."
        )));
    }
    Ok(text.to_string())
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

fn normalize_worktree_target_path(
    input: Option<&Value>,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    let worktree_path = normalize_absolute_path(input, "worktreePath")?;
    let family_root = Path::new(&context.cwd)
        .parent()
        .unwrap_or_else(|| Path::new(&context.cwd))
        .to_string_lossy()
        .to_string();
    if !is_path_inside(&family_root, &worktree_path) {
        return Err(TypedOperationError::forbidden(
            "worktreePath must stay inside the source project worktree family directory.",
        ));
    }
    if normalize_path_string(PathBuf::from(&worktree_path))
        == normalize_path_string(PathBuf::from(&context.cwd))
    {
        return Err(TypedOperationError::forbidden(
            "worktreePath cannot be the source project directory.",
        ));
    }
    Ok(worktree_path)
}

fn normalize_existing_worktree_path(
    input: Option<&Value>,
    context: &TypedOperationContext,
) -> Result<String, TypedOperationError> {
    let worktree_path = normalize_worktree_target_path(input, context)?;
    let metadata = fs::metadata(&worktree_path).map_err(|_| {
        TypedOperationError::not_found(format!("worktreePath does not exist: {worktree_path}"))
    })?;
    if metadata.is_dir() {
        Ok(worktree_path)
    } else {
        Err(TypedOperationError::bad_request(
            "worktreePath is not a directory.",
        ))
    }
}

fn normalize_project_setup_command(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let Some(text) = input.and_then(Value::as_str) else {
        return Ok(String::new());
    };
    if text.len() > SETUP_COMMAND_LIMIT_BYTES {
        return Err(TypedOperationError::bad_request(
            "worktreeCommand exceeds the 16384-byte limit.",
        ));
    }
    Ok(text.trim().to_string())
}

fn resolve_project_setup_command_project(
    params: &Map<String, Value>,
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    if let Some(project_id) = params
        .get("setupCommandProjectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return context
            .projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .cloned()
            .ok_or_else(|| {
                TypedOperationError::not_found(format!("Project {project_id} does not exist."))
            });
    }
    if let Some(path) = params
        .get("setupCommandProjectPath")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let setup_path = normalize_existing_directory_path_value(
            Some(&Value::String(path.to_string())),
            "setupCommandProjectPath",
        )?;
        return context
            .projects
            .iter()
            .find(|candidate| {
                candidate
                    .get("path")
                    .and_then(Value::as_str)
                    .and_then(|path| {
                        normalize_existing_directory_path_value(
                            Some(&Value::String(path.to_string())),
                            "project.path",
                        )
                        .ok()
                    })
                    .as_deref()
                    == Some(setup_path.as_str())
            })
            .cloned()
            .ok_or_else(|| {
                TypedOperationError::forbidden(
                    "setupCommandProjectPath must be a registered gxserver project path.",
                )
            });
    }
    context
        .projects
        .iter()
        .find(|candidate| {
            candidate
                .get("path")
                .and_then(Value::as_str)
                .and_then(|path| {
                    normalize_existing_directory_path_value(
                        Some(&Value::String(path.to_string())),
                        "project.path",
                    )
                    .ok()
                })
                .as_deref()
                == Some(context.cwd.as_str())
        })
        .cloned()
        .ok_or_else(|| {
            TypedOperationError::forbidden(
                "Project setup command target must be a registered gxserver project.",
            )
        })
}

fn normalize_git_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "addAll"
        | "branch"
        | "commit"
        | "countFileLines"
        | "checkout"
        | "checkoutNewBranch"
        | "deleteLocalBranch"
        | "deleteRemoteBranch"
        | "diff"
        | "diffCached"
        | "diffCachedNoExt"
        | "diffCachedStat"
        | "diffNoExt"
        | "diffNoIndexAgainstNull"
        | "diffNumstat"
        | "getOriginRemoteUrl"
        | "isInsideWorkTree"
        | "isUntrackedFile"
        | "list"
        | "listRemotes"
        | "listUntracked"
        | "merge"
        | "pullFastForward"
        | "push"
        | "pushSetUpstream"
        | "remoteBranchExists"
        | "status"
        | "statusPorcelain"
        | "upstreamCounts"
        | "verifyRef" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported Git action: {}",
            display_unknown_value(input)
        ))),
    }
}

fn normalize_github_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "prCreateFill" | "prView" | "version" => Ok(action.to_string()),
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported GitHub action: {}",
            display_unknown_value(input)
        ))),
    }
}

fn normalize_worktree_action(input: Option<&Value>) -> Result<String, TypedOperationError> {
    let action = input.and_then(Value::as_str).unwrap_or("undefined");
    match action {
        "create" | "ensureBeadsHooks" | "list" | "pathExists" | "prune" | "remove" | "switch" => {
            Ok(action.to_string())
        }
        _ => Err(TypedOperationError::bad_request(format!(
            "Unsupported worktree action: {}",
            display_unknown_value(input)
        ))),
    }
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
        params
            .get("description")
            .map(value_to_string)
            .unwrap_or_default(),
        "--priority".to_string(),
        params
            .get("priority")
            .map(|_| normalize_required_text(params.get("priority"), "priority"))
            .transpose()?
            .unwrap_or_else(|| "2".to_string()),
        "--type".to_string(),
        "task".to_string(),
    ];
    if params.contains_key("estimate") {
        args.extend([
            "--estimate".to_string(),
            normalize_beads_estimate(params.get("estimate"))?,
        ]);
    }
    if let Some(labels) = params
        .get("labels")
        .and_then(Value::as_array)
        .filter(|labels| !labels.is_empty())
    {
        let labels = labels
            .iter()
            .map(|label| normalize_required_text(Some(label), "label"))
            .collect::<Result<Vec<_>, _>>()?
            .join(",");
        args.extend(["--labels".to_string(), labels]);
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
    let labels = input.and_then(Value::as_array).cloned().unwrap_or_default();
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

fn parse_beads_board_output(stdout: &str) -> Result<(Vec<Value>, String), TypedOperationError> {
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
        if row_count > BEADS_BOARD_ROW_LIMIT {
            return Err(TypedOperationError::bad_request(format!(
                "Beads board state exceeds the {BEADS_BOARD_ROW_LIMIT}-row limit; refusing to return oversized board data."
            )));
        }
        let text = serde_json::to_string(&item).unwrap_or_else(|_| "{}".to_string());
        let next_response_bytes =
            response_bytes + text.len() + if serialized.is_empty() { 0 } else { 1 };
        if next_response_bytes > BEADS_BOARD_RESPONSE_LIMIT_BYTES {
            return Err(TypedOperationError::bad_request(format!(
                "Beads board response exceeds the {BEADS_BOARD_RESPONSE_LIMIT_BYTES}-byte serialized JSON limit; refusing to return oversized board data."
            )));
        }
        response_bytes = next_response_bytes;
        issues.push(item);
        serialized.push(text);
    }
    Ok((issues, format!("[{}]", serialized.join(","))))
}

fn derive_beads_label_counts(issues: &[Value]) -> Vec<BeadsLabelCount> {
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

fn count_project_file_lines(
    cwd: &str,
    file_paths: &[String],
) -> Result<usize, TypedOperationError> {
    let mut total = 0;
    for file_path in file_paths {
        let absolute = Path::new(cwd).join(file_path);
        if !is_path_inside(cwd, &absolute.to_string_lossy()) {
            return Err(TypedOperationError::forbidden(
                "filePath must stay inside the project.",
            ));
        }
        let mut file = fs::File::open(&absolute)
            .map_err(|error| TypedOperationError::not_found(error.to_string()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| TypedOperationError::bad_request(error.to_string()))?;
        total += bytes.iter().filter(|byte| **byte == b'\n').count();
    }
    Ok(total)
}

fn parse_git_worktree_list_porcelain(stdout: &str) -> Vec<Value> {
    let mut entries = Vec::new();
    let mut current: Option<Map<String, Value>> = None;
    for line in stdout.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                if entry
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|path| !path.is_empty())
                {
                    entries.push(Value::Object(entry));
                }
            }
            let mut entry = Map::new();
            entry.insert("bare".to_string(), json!(false));
            entry.insert("detached".to_string(), json!(false));
            entry.insert("path".to_string(), json!(path.trim()));
            current = Some(entry);
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if line == "bare" {
            entry.insert("bare".to_string(), json!(true));
        } else if line == "detached" {
            entry.insert("detached".to_string(), json!(true));
        } else if let Some(branch) = line.strip_prefix("branch ") {
            entry.insert(
                "branch".to_string(),
                json!(normalize_worktree_branch(Some(branch.trim()))),
            );
        }
    }
    if let Some(entry) = current {
        if entry
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| !path.is_empty())
        {
            entries.push(Value::Object(entry));
        }
    }
    for entry in &mut entries {
        if entry.get("branch").is_none() {
            if let Some(object) = entry.as_object_mut() {
                object.insert("branch".to_string(), json!("detached"));
            }
        }
    }
    entries
}

fn normalize_worktree_branch(branch: Option<&str>) -> String {
    branch
        .map(|branch| {
            branch
                .trim()
                .strip_prefix("refs/heads/")
                .unwrap_or(branch.trim())
        })
        .filter(|branch| !branch.is_empty())
        .unwrap_or("detached")
        .to_string()
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

fn display_unknown_value(value: Option<&Value>) -> String {
    value
        .map(value_to_string)
        .unwrap_or_else(|| "undefined".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command as StdCommand;
    use tempfile::tempdir;

    fn context(cwd: &Path) -> TypedOperationContext {
        let project = json!({
            "projectId": "P-test",
            "path": cwd.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        TypedOperationContext {
            beads_cwd: None,
            cwd: cwd.to_string_lossy().to_string(),
            env_path: None,
            projects: vec![project],
        }
    }

    #[test]
    fn git_commit_redacts_stdin_command_metadata() {
        let dir = tempdir().unwrap();
        let mut params = Map::new();
        params.insert("messageSubject".to_string(), json!("secret subject"));
        params.insert("messageBody".to_string(), json!("secret body"));
        params.insert("noVerify".to_string(), json!(true));
        let command = build_git_command("commit", &params, &dir.path().to_string_lossy()).unwrap();
        assert_eq!(
            command.summary().args,
            vec!["commit", "--no-verify", "-F", "<stdin>"]
        );
        assert!(command.stdin.unwrap().contains("secret subject"));
    }

    #[test]
    fn git_pull_fast_forward_plans_ff_only_command() {
        let dir = tempdir().unwrap();
        let command = build_git_command(
            "pullFastForward",
            &Map::new(),
            &dir.path().to_string_lossy(),
        )
        .unwrap();
        assert_eq!(command.args, vec!["pull", "--ff-only"]);
        assert_eq!(command.summary().args, vec!["pull", "--ff-only"]);
        assert_eq!(
            normalize_git_action(Some(&json!("pullFastForward"))).unwrap(),
            "pullFastForward"
        );
    }

    #[tokio::test]
    async fn git_pull_fast_forward_executes_local_remote_update() {
        let root = tempdir().unwrap();
        let remote = root.path().join("remote.git");
        let seed = root.path().join("seed");
        let repo = root.path().join("repo");
        fs::create_dir(&seed).unwrap();

        run_git(["init", "--bare", remote.to_str().unwrap()], root.path());
        run_git(["init"], &seed);
        run_git(["checkout", "-b", "main"], &seed);
        run_git(["config", "user.email", "typed@example.invalid"], &seed);
        run_git(["config", "user.name", "Typed Operation Test"], &seed);
        fs::write(seed.join("file.txt"), "one\n").unwrap();
        run_git(["add", "file.txt"], &seed);
        run_git(["commit", "-m", "initial"], &seed);
        run_git(["remote", "add", "origin", remote.to_str().unwrap()], &seed);
        run_git(["push", "-u", "origin", "main"], &seed);
        run_git(
            [
                "--git-dir",
                remote.to_str().unwrap(),
                "symbolic-ref",
                "HEAD",
                "refs/heads/main",
            ],
            root.path(),
        );
        run_git(
            ["clone", remote.to_str().unwrap(), repo.to_str().unwrap()],
            root.path(),
        );

        fs::write(seed.join("file.txt"), "one\ntwo\n").unwrap();
        run_git(["add", "file.txt"], &seed);
        run_git(["commit", "-m", "second"], &seed);
        run_git(["push"], &seed);

        let ctx = context(&repo);
        let mut params = Map::new();
        params.insert("action".to_string(), json!("pullFastForward"));
        let result = run_git_action(&params, &ctx).await.unwrap();
        assert_eq!(
            result.get("action").and_then(Value::as_str),
            Some("pullFastForward")
        );
        assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
        assert_eq!(
            result
                .get("command")
                .and_then(|command| command.get("args"))
                .cloned()
                .unwrap(),
            json!(["pull", "--ff-only"])
        );
        assert_eq!(
            fs::read_to_string(repo.join("file.txt")).unwrap(),
            "one\ntwo\n"
        );
    }

    #[test]
    fn worktree_target_stays_inside_family_root() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("repo");
        fs::create_dir(&source).unwrap();
        let ctx = context(&source);
        let mut params = Map::new();
        params.insert(
            "worktreePath".to_string(),
            json!(dir.path().join("repo-two").to_string_lossy()),
        );
        assert!(normalize_worktree_target_path(params.get("worktreePath"), &ctx).is_ok());
        params.insert(
            "worktreePath".to_string(),
            json!("/tmp/outside-worktree-family"),
        );
        assert_eq!(
            normalize_worktree_target_path(params.get("worktreePath"), &ctx)
                .unwrap_err()
                .code,
            "forbidden"
        );
    }

    #[test]
    fn beads_board_filters_non_object_rows() {
        let (issues, stdout) =
            parse_beads_board_output(r#"{"data":[{"id":"A"},1,{"id":"B"}]}"#).unwrap();
        assert_eq!(issues.len(), 2);
        assert_eq!(stdout, r#"[{"id":"A"},{"id":"B"}]"#);
    }

    #[test]
    fn beads_list_all_labels_plans_board_list_command() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let command =
            build_beads_command_with_executable("listAllLabels", &Map::new(), &ctx, "/tmp/bd")
                .unwrap();
        assert_eq!(command.args, vec!["list", "--all", "--json"]);
    }

    #[test]
    fn beads_label_counts_derive_sorted_counts_from_board_output() {
        let (issues, _) = parse_beads_board_output(
            r#"{"data":[{"id":"gxserver-1","labels":["ui"," mac ","",null,"ui"]},{"id":"gxserver-2","labels":["backend","Backend"]},{"id":"gxserver-3","labels":"ignored"},{"id":"gxserver-4","labels":["z","Z","a","A","aa"]},1]}"#,
        )
        .unwrap();
        let labels = derive_beads_label_counts(&issues);
        assert_eq!(
            serde_json::to_string(&labels).unwrap(),
            r#"[{"count":1,"label":"a"},{"count":1,"label":"A"},{"count":1,"label":"aa"},{"count":1,"label":"backend"},{"count":1,"label":"Backend"},{"count":1,"label":"mac"},{"count":2,"label":"ui"},{"count":1,"label":"z"},{"count":1,"label":"Z"}]"#
        );

        let (empty_issues, _) = parse_beads_board_output("").unwrap();
        assert_eq!(
            serde_json::to_string(&derive_beads_label_counts(&empty_issues)).unwrap(),
            "[]"
        );
        let error = parse_beads_board_output("{").unwrap_err();
        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "Beads board output was not valid JSON.");
    }

    fn run_git<I, S>(args: I, cwd: &Path)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_os_string())
            .collect::<Vec<_>>();
        let output = StdCommand::new("git")
            .args(&args)
            .current_dir(cwd)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
