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
use url::Url;

use crate::{platform::shell::command_shell, toolchain::require_bundled_bd};

const TYPED_OPERATION_TIMEOUT_MS: u64 = 120_000;
const TYPED_OPERATION_STDOUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const TYPED_OPERATION_STDERR_LIMIT_BYTES: usize = 4 * 1024 * 1024;
const BEADS_BOARD_RESPONSE_LIMIT_BYTES: usize = TYPED_OPERATION_STDOUT_LIMIT_BYTES;
const BEADS_BOARD_ROW_LIMIT: usize = 5_000;
const BEADS_GIT_HOOK_NAMES: [&str; 3] = ["pre-commit", "post-merge", "post-checkout"];
const COMMIT_MESSAGE_LIMIT_BYTES: usize = 64 * 1024;
const SETUP_COMMAND_LIMIT_BYTES: usize = 16 * 1024;
const FILE_PATH_LIMIT: usize = 500;

#[derive(Debug, Clone)]
pub struct TypedOperationError {
    pub code: &'static str,
    pub details: Option<Value>,
    pub message: String,
    pub scope_rejection: bool,
}

impl TypedOperationError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "badRequest",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    fn dependency_unavailable(message: impl Into<String>) -> Self {
        Self {
            code: "dependencyUnavailable",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            code: "forbidden",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "notFound",
            details: None,
            message: message.into(),
            scope_rejection: false,
        }
    }

    fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    fn with_scope_rejection(mut self) -> Self {
        self.scope_rejection = true;
        self
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
    env: Vec<(String, String)>,
    executable: String,
    preserve_stdout_whitespace: bool,
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

#[derive(Clone, Copy, Debug)]
struct BeadsBoardLimits {
    response_limit_bytes: usize,
    row_limit: usize,
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

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
Sidebar V2 worktree sessions live in a checkout that is deliberately NOT a
registered project, so the ordinary scope resolution ("cwd is a registered
project path") cannot reach them. Scope still comes from a registered project:
the caller names the source project, and the requested `worktreePath` is
accepted only after `normalize_existing_worktree_path` proves it is an existing
directory inside that project's worktree family. The command then runs with the
worktree as cwd through the same allowlisted builders, so an unregistered
worktree never widens what a typed operation can execute.
*/
pub async fn dispatch_worktree_path_operation(
    endpoint_path: &str,
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<Value, TypedOperationError> {
    let scope = resolve_project_operation_context(endpoint_path, params, projects)?;
    let worktree_path = normalize_existing_worktree_path(params.get("worktreePath"), &scope)?;
    let context = TypedOperationContext {
        beads_cwd: None,
        cwd: worktree_path,
        env_path: scope.env_path.clone(),
        projects: scope.projects.clone(),
    };
    match endpoint_path {
        "/api/runGitAction" => run_git_action(params, &context).await,
        "/api/runProjectSetupCommand" => run_project_setup_command(params, &context).await,
        _ => Err(TypedOperationError::not_found(format!(
            "{endpoint_path} is not a gxserver worktree-path operation endpoint."
        ))),
    }
}

/*
CDXC:GPUISidebarGit 2026-06-24-16:28:
GPUI direct PR creation needs gxserver to own `gh pr create --fill` completion
and return only a verified open PR record. Do not expose command output, branch
names, titles, commit messages, argv text, or daemon stderr/stdout through this
result; callers only need a success boolean and trusted PR state/URL metadata
before opening a browser or deleting a completed worktree.
*/
pub async fn create_pull_request_for_project(
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<Value, TypedOperationError> {
    let context = resolve_project_operation_context("/api/createPullRequest", params, projects)?;

    if let PullRequestProbe::Open(pr) = probe_open_github_pull_request(&context).await? {
        return Ok(github_pull_request_success(false, pr));
    }

    let create_command = build_github_command("prCreateFill", &context.cwd);
    let create_output = match run_process_command(&create_command, &context).await {
        Ok(output) => output,
        Err(_) => return Ok(github_pull_request_failure("githubCliUnavailable")),
    };

    let created = create_output.exit_code == 0 && create_output.error.is_none();
    match probe_open_github_pull_request(&context).await? {
        PullRequestProbe::Open(pr) => Ok(github_pull_request_success(created, pr)),
        PullRequestProbe::Failed if created => Ok(github_pull_request_failure("viewFailed")),
        PullRequestProbe::Invalid | PullRequestProbe::NotOpen if created => {
            Ok(github_pull_request_failure("invalidResult"))
        }
        _ => Ok(github_pull_request_failure("createFailed")),
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

/*
CDXC:GxserverCommandRedaction 2026-06-22-09:38:
Typed operation persistent logs must expose enum and shape facts only: action, command presence, argument count, executable, expected-nonzero state, exit code, and structured operation error. Do not copy argv, cwd, stdout, stderr, branch names, paths, Beads text fields, or setup command text into log details; the RPC result remains TypeScript-compatible for the caller.
*/
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
            .with_scope_rejection()
    })?;
    let cwd = normalize_existing_directory_path_value(
        Some(&Value::String(project_path.to_string())),
        "project.path",
    )
    .map_err(TypedOperationError::with_scope_rejection)?;
    let beads_cwd = if endpoint_path == "/api/runBeadsAction"
        && params.get("projectBoardScope").and_then(Value::as_bool) == Some(true)
    {
        /*
        CDXC:GlobalProjectDefaults 2026-08-02:
        The project's own Beads directory still wins. Only an unset one consults
        the Global Default, and an unset global still yields None so the board
        keeps launching from the project root exactly as it did before.
        */
        let project_directory = project
            .get("projectBoardConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("beadsDirectory"))
            .and_then(Value::as_str);
        crate::global_project_defaults::resolve_with_global_default(
            project_directory,
            &crate::global_project_defaults::read_global_project_defaults().beads_directory,
        )
        .map(|path| {
            normalize_existing_directory_path_value(
                Some(&Value::String(path)),
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
        .filter(|value| !value.trim().is_empty())
    {
        let project = projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .cloned()
            .ok_or_else(|| {
                TypedOperationError::not_found(format!("Project {project_id} does not exist."))
                    .with_scope_rejection()
            })?;
        if scope.contains_key("projectPath") {
            let scoped_path =
                normalize_existing_directory_path_value(scope.get("projectPath"), "projectPath")
                    .map_err(TypedOperationError::with_scope_rejection)?;
            let project_path = project
                .get("path")
                .and_then(Value::as_str)
                .map(|path| {
                    normalize_absolute_path_value(
                        Some(&Value::String(path.to_string())),
                        "project.path",
                    )
                })
                .transpose()
                .map_err(TypedOperationError::with_scope_rejection)?;
            if project_path.as_deref() != Some(scoped_path.as_str()) {
                return Err(TypedOperationError::forbidden(
                    "projectPath does not match the requested projectId.",
                )
                .with_scope_rejection());
            }
        }
        return Ok(project);
    }

    let scoped_path =
        normalize_existing_directory_path_value(scope.get("projectPath"), "projectPath")
            .map_err(TypedOperationError::with_scope_rejection)?;
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
            .with_scope_rejection()
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
    let mut result = typed_result(&action, &command, output);
    if action == "listBranches" && result.get("exitCode").and_then(Value::as_i64) == Some(0) {
        let stdout = result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        result.as_object_mut().expect("result object").insert(
            "branches".to_string(),
            Value::Array(parse_git_branch_list(&stdout)),
        );
    }
    Ok(result)
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
    let action = params.get("action").and_then(Value::as_str);
    if action != Some("worktreeSetupCommand") {
        return Err(TypedOperationError::bad_request(format!(
            "Unsupported project setup action: {}",
            setup_action_value_to_string(params.get("action"))
        )));
    }
    let action = action.expect("validated setup action");
    let setup_project = resolve_project_setup_command_project(params, context)?;
    /*
    CDXC:GlobalProjectDefaults 2026-08-02:
    A project without its own worktree command now runs the Global Default. With
    no global configured this resolves to None and the operation still no-ops
    below, which is the pre-feature behavior.
    */
    let resolved_setup_command = crate::global_project_defaults::resolve_with_global_default(
        setup_project
            .get("gitConfig")
            .and_then(Value::as_object)
            .and_then(|config| config.get("worktreeCommand"))
            .and_then(Value::as_str),
        &crate::global_project_defaults::read_global_project_defaults().worktree_command,
    );
    let command_text = normalize_project_setup_command(
        resolved_setup_command.map(Value::String).as_ref(),
    )?;
    if command_text.is_empty() {
        return Ok(json!({
            "action": action,
            "exitCode": 0,
            "stderr": "",
            "stdout": "",
        }));
    }
    let shell = command_shell();
    let command = ProcessCommand {
        args: vec![shell.command_flag(false).to_string(), command_text],
        cwd: context.cwd.clone(),
        env: Vec::new(),
        executable: shell.executable.clone(),
        preserve_stdout_whitespace: false,
        result_command: Some(CommandSummary {
            args: vec![
                shell.command_flag(false).to_string(),
                "<worktree setup command>".to_string(),
            ],
            cwd: context.cwd.clone(),
            executable: shell.executable.clone(),
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
    /*
    CDXC:ProjectBoard 2026-06-22-01:09:
    gxserver-rs must match the TypeScript Beads execution contract: every subprocess-backed Project Board action runs with BD_JSON_ENVELOPE=1, failed board reads still return an empty `issues` array for UI consumers, and board parsing uses the same bounded JSON response rules.
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
            let command = ProcessCommand::new("git", args, cwd).with_result_command(result_command);
            if files.is_empty() {
                command
            } else {
                command.with_env("GIT_LITERAL_PATHSPECS", "1")
            }
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
        /*
        CDXC:SidebarV2Worktrees 2026-07-29-00:00:
        Rolling back a half-created worktree session, and force-removing a
        worktree the user chose to discard, both have to delete a branch that
        `git branch -d` refuses because it is unmerged — which is exactly the
        state a discarded attempt is in. Kept as its own allowlisted action so
        the safe delete stays the default everywhere else.
        */
        "deleteLocalBranchForce" => {
            let branch = normalize_git_ref(params.get("branch"), "branch")?;
            ProcessCommand::new(
                "git",
                vec![
                    "branch".to_string(),
                    "-D".to_string(),
                    "--".to_string(),
                    branch,
                ],
                cwd,
            )
            .with_result_args(vec!["branch", "-D", "--", "<branch>"])
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
        /*
        CDXC:GPUISidebarGit 2026-06-24-16:11:
        GPUI blank commit-message generation needs staged diff text for exactly
        the review-approved file set. Keep this as an allowlisted cached-diff
        action with path validation and redacted file-count command metadata
        instead of exposing a free-form git command or logging file paths.
        */
        "diffCachedFiles" => {
            let files = required_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--no-ext-diff".to_string(),
                "--".to_string(),
            ];
            args.extend(files.clone());
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: vec![
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--no-ext-diff".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_env("GIT_LITERAL_PATHSPECS", "1")
        }
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
        "diffCachedStatFiles" => {
            let files = required_relative_file_paths(params.get("filePaths"))?;
            let mut args = vec![
                "diff".to_string(),
                "--cached".to_string(),
                "--stat".to_string(),
                "--".to_string(),
            ];
            args.extend(files.clone());
            ProcessCommand::new("git", args, cwd)
                .with_result_command(Some(CommandSummary {
                    args: vec![
                        "diff".to_string(),
                        "--cached".to_string(),
                        "--stat".to_string(),
                        "--".to_string(),
                        format!("<{} files>", files.len()),
                    ],
                    cwd: cwd.to_string(),
                    executable: "git".to_string(),
                }))
                .with_env("GIT_LITERAL_PATHSPECS", "1")
        }
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
        "listBranches" => {
            /*
            CDXC:WorktreeBaseBranch 2026-06-24-11:32:
            Add Worktree needs an explicit base-branch picker. Keep branch
            discovery inside the gxserver typed Git boundary, include local and
            remote-tracking refs, and parse structured metadata server-side so
            UI clients do not shell out or parse raw Git output.
            */
            ProcessCommand::new(
                "git",
                vec![
                    "for-each-ref",
                    "--format=%(refname:short)%09%(refname)%09%(HEAD)",
                    "refs/heads",
                    "refs/remotes",
                ],
                cwd,
            )
        }
        "listRemotes" => ProcessCommand::new("git", vec!["remote"], cwd),
        "listUntracked" => ProcessCommand::new(
            "git",
            vec!["ls-files", "--others", "--exclude-standard", "-z"],
            cwd,
        ),
        "status" => ProcessCommand::new("git", vec!["status", "--short", "--branch"], cwd),
        "statusPorcelain" => ProcessCommand::new("git", vec!["status", "--porcelain"], cwd)
            .with_preserved_stdout_whitespace(),
        "statusPorcelainZ" => ProcessCommand::new("git", vec!["status", "--porcelain", "-z"], cwd)
            .with_preserved_stdout_whitespace(),
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
        /*
        CDXC:GPUIRemoteGit 2026-06-24-17:47:
        Remote GPUI push parity must not send renderer-observed branch names as mutation authority. Push the current HEAD to origin with upstream tracking so gxserver/Git derive the branch from the checked-out repository state.
        */
        "pushSetUpstreamCurrent" => ProcessCommand::new(
            "git",
            vec![
                "push".to_string(),
                "-u".to_string(),
                "origin".to_string(),
                "HEAD".to_string(),
            ],
            cwd,
        ),
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

enum PullRequestProbe {
    Failed,
    Invalid,
    NotOpen,
    Open(Value),
}

async fn probe_open_github_pull_request(
    context: &TypedOperationContext,
) -> Result<PullRequestProbe, TypedOperationError> {
    let command = ProcessCommand::new(
        "gh",
        vec!["pr", "view", "--json", "number,state,url"],
        context.cwd.as_str(),
    );
    let output = match run_process_command(&command, context).await {
        Ok(output) => output,
        Err(_) => return Ok(PullRequestProbe::Failed),
    };
    if output.exit_code != 0 || output.error.is_some() || output.stdout.trim().is_empty() {
        return Ok(PullRequestProbe::Failed);
    }
    let Some(pr) = parse_github_pull_request_summary(&output.stdout) else {
        return Ok(PullRequestProbe::Invalid);
    };
    if pr.get("state").and_then(Value::as_str) == Some("open") {
        Ok(PullRequestProbe::Open(pr))
    } else {
        Ok(PullRequestProbe::NotOpen)
    }
}

fn parse_github_pull_request_summary(stdout: &str) -> Option<Value> {
    let value = serde_json::from_str::<Value>(stdout.trim()).ok()?;
    let object = value.as_object()?;
    let state_value = object.get("state")?.as_str()?.to_ascii_lowercase();
    let state = match state_value.as_str() {
        "open" => "open",
        "closed" => "closed",
        "merged" => "merged",
        _ => return None,
    };
    let url = validate_github_pull_request_url(object.get("url")?.as_str()?)?;
    let number = object
        .get("number")
        .and_then(Value::as_u64)
        .or_else(|| github_pull_request_number_from_url(&url));
    let mut summary = Map::new();
    if let Some(number) = number {
        summary.insert("number".to_string(), json!(number));
    }
    summary.insert("state".to_string(), json!(state));
    summary.insert("url".to_string(), json!(url));
    Some(Value::Object(summary))
}

fn validate_github_pull_request_url(candidate: &str) -> Option<String> {
    let trimmed = candidate.trim();
    if trimmed.is_empty()
        || trimmed.chars().count() > 2048
        || trimmed.contains('\\')
        || trimmed
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return None;
    }
    let parsed = Url::parse(trimmed).ok()?;
    if parsed.scheme() != "https"
        || !parsed
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("github.com"))
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    let segments = parsed.path_segments()?.collect::<Vec<_>>();
    if segments.len() != 4
        || segments[0].is_empty()
        || segments[1].is_empty()
        || segments[2] != "pull"
        || segments[3].is_empty()
        || !segments[3].bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(parsed.as_str().to_string())
}

fn github_pull_request_number_from_url(url: &str) -> Option<u64> {
    Url::parse(url)
        .ok()?
        .path_segments()?
        .nth(3)?
        .parse::<u64>()
        .ok()
}

fn github_pull_request_success(created: bool, pr: Value) -> Value {
    json!({
        "created": created,
        "ok": true,
        "pr": pr,
    })
}

fn github_pull_request_failure(reason: &str) -> Value {
    json!({
        "created": false,
        "ok": false,
        "reason": reason,
    })
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

async fn ensure_beads_git_hooks(
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    /*
    CDXC:WorktreeBeads 2026-06-22-01:09:
    The Rust port must install the same Ghostex-managed Beads hooks as the TypeScript server. Generated hooks live under the common Git directory, call the bundled bd by absolute path, pin BEADS_DIR to `bd where --json`, and are written before core.hooksPath is pointed at them.
    */
    if !Path::new(&context.cwd).join(".beads").is_dir() {
        return Ok(json!({
            "action": "ensureBeadsHooks",
            "exitCode": 0,
            "stderr": "",
            "stdout": "skipped: no Beads workspace",
        }));
    }
    let bd = require_bundled_bd().map_err(TypedOperationError::dependency_unavailable)?;
    ensure_beads_git_hooks_with_executable(context, &bd.executable_path).await
}

async fn ensure_beads_git_hooks_with_executable(
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

fn normalize_beads_where_directory(stdout: &str) -> Result<String, TypedOperationError> {
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

fn build_ghostex_beads_git_hook_script(hook_name: &str, bd: &str, beads_path: &str) -> String {
    format!(
        "#!/usr/bin/env sh\n\
# Ghostex-managed Beads hook. This local file is generated under the common Git directory.\n\
BD_BIN={}\n\
BEADS_DIR_VALUE={}\n\
HOOK_NAME={}\n\
if [ ! -x \"$BD_BIN\" ]; then\n\
  echo \"Warning: Ghostex bundled bd is missing; skipping Beads hook\" >&2\n\
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

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn resolve_path_against(cwd: &str, path_text: &str) -> PathBuf {
    let path = Path::new(path_text.trim());
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(cwd).join(path)
    };
    resolved.components().collect()
}

#[cfg(unix)]
fn chmod_executable_if_supported(path: &Path) -> Result<(), TypedOperationError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|error| {
        TypedOperationError::bad_request(format!("Could not chmod Beads Git hook: {error}"))
    })
}

#[cfg(not(unix))]
fn chmod_executable_if_supported(_path: &Path) -> Result<(), TypedOperationError> {
    Ok(())
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
            env: Vec::new(),
            executable: executable.into(),
            preserve_stdout_whitespace: false,
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

    fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    fn with_stdin(mut self, stdin: String) -> Self {
        self.stdin = Some(stdin);
        self
    }

    fn with_preserved_stdout_whitespace(mut self) -> Self {
        self.preserve_stdout_whitespace = true;
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
        .envs(typed_operation_environment(
            context.env_path.as_deref(),
            &command.env,
        ))
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
    /*
    CDXC:GPUISidebarGit 2026-07-24-17:38:
    Git porcelain status uses the first two columns as data, including a
    leading space for an unstaged change. Preserve stdout exactly for those
    commands so the first path is not shifted left and truncated before the
    reviewed-file staging boundary.
    */
    let stdout_text = String::from_utf8_lossy(&output.stdout);
    let mut stdout = if command.preserve_stdout_whitespace {
        stdout_text.to_string()
    } else {
        stdout_text.trim().to_string()
    };
    let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.stdout.len() > TYPED_OPERATION_STDOUT_LIMIT_BYTES {
        let truncated_stdout =
            String::from_utf8_lossy(&output.stdout[..TYPED_OPERATION_STDOUT_LIMIT_BYTES]);
        stdout = if command.preserve_stdout_whitespace {
            truncated_stdout.to_string()
        } else {
            truncated_stdout.trim().to_string()
        };
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

fn typed_operation_environment(
    env_path: Option<&str>,
    extra_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = env::vars().collect();
    environment.retain(|(key, _)| !is_typed_operation_color_env_key(key));
    if let Some(path) = env_path {
        environment.retain(|(key, _)| key != "PATH");
        environment.push(("PATH".to_string(), path.to_string()));
    }
    for (key, value) in extra_env {
        if is_typed_operation_color_env_key(key) {
            continue;
        }
        environment.retain(|(existing_key, _)| existing_key != key);
        environment.push((key.clone(), value.clone()));
    }
    environment
}

fn is_typed_operation_color_env_key(key: &str) -> bool {
    matches!(
        key,
        "ANSI_COLORS_DISABLED" | "NO_COLOR" | "NODE_DISABLE_COLORS"
    )
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
    /*
    CDXC:GxserverProjectPaths 2026-06-22-09:35:
    Typed-operation projectPath, worktreePath, setupCommandProjectPath, and Beads directory checks must match TypeScript `path.resolve` syntax normalization. Collapse `.` and `..` without canonicalizing symlinks so registered-root equality and containment stay lexical while selected paths still get filesystem validation at the operation boundary.
    */
    let path = if path.is_absolute() {
        path
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Normal(segment) => normalized.push(segment),
        }
    }
    if normalized.as_os_str().is_empty() {
        ".".to_string()
    } else {
        normalized.to_string_lossy().to_string()
    }
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

fn required_relative_file_paths(input: Option<&Value>) -> Result<Vec<String>, TypedOperationError> {
    let paths = optional_relative_file_paths(input)?;
    if paths.is_empty() {
        return Err(TypedOperationError::bad_request(
            "filePaths must include at least one relative path.",
        ));
    }
    Ok(paths)
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
        .filter(|value| !value.trim().is_empty())
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
        .filter(|value| !value.trim().is_empty())
    {
        let setup_path =
            normalize_project_setup_existing_directory_path(path, "setupCommandProjectPath")?;
        for candidate in &context.projects {
            if let Some(candidate_path) = candidate.get("path").and_then(Value::as_str) {
                if normalize_project_setup_existing_directory_path(candidate_path, "project.path")?
                    == setup_path
                {
                    return Ok(candidate.clone());
                }
            }
        }
        return Err(TypedOperationError::forbidden(
            "setupCommandProjectPath must be a registered gxserver project path.",
        ));
    }
    resolve_project_setup_command_for_context_cwd(context)
}

fn resolve_project_setup_command_for_context_cwd(
    context: &TypedOperationContext,
) -> Result<Value, TypedOperationError> {
    let context_cwd =
        normalize_project_setup_existing_directory_path(&context.cwd, "project.path")?;
    for candidate in &context.projects {
        if let Some(candidate_path) = candidate.get("path").and_then(Value::as_str) {
            if normalize_project_setup_existing_directory_path(candidate_path, "project.path")?
                == context_cwd
            {
                return Ok(candidate.clone());
            }
        }
    }
    Err(TypedOperationError::forbidden(
        "Project setup command target must be a registered gxserver project.",
    ))
}

fn normalize_project_setup_existing_directory_path(
    input: &str,
    field: &str,
) -> Result<String, TypedOperationError> {
    let path = normalize_project_setup_absolute_path(input, field)?;
    let metadata = fs::metadata(&path)
        .map_err(|_| TypedOperationError::not_found(format!("{field} does not exist: {path}")))?;
    if !metadata.is_dir() {
        return Err(TypedOperationError::bad_request(format!(
            "{field} is not a directory."
        )));
    }
    Ok(path)
}

fn normalize_project_setup_absolute_path(
    input: &str,
    field: &str,
) -> Result<String, TypedOperationError> {
    let text = input.trim();
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

fn setup_action_value_to_string(input: Option<&Value>) -> String {
    match input {
        None => "undefined".to_string(),
        Some(value) => json_value_to_javascript_string(value),
    }
}

fn json_value_to_javascript_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| match value {
                Value::Null => String::new(),
                Value::Array(_) => json_value_to_javascript_string(value),
                _ => json_value_to_javascript_string(value),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
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
        | "deleteLocalBranchForce"
        | "deleteRemoteBranch"
        | "diff"
        | "diffCached"
        | "diffCachedFiles"
        | "diffCachedStatFiles"
        | "diffCachedNoExt"
        | "diffCachedStat"
        | "diffNoExt"
        | "diffNoIndexAgainstNull"
        | "diffNumstat"
        | "getOriginRemoteUrl"
        | "isInsideWorkTree"
        | "isUntrackedFile"
        | "list"
        | "listBranches"
        | "listRemotes"
        | "listUntracked"
        | "merge"
        | "pullFastForward"
        | "push"
        | "pushSetUpstreamCurrent"
        | "pushSetUpstream"
        | "remoteBranchExists"
        | "status"
        | "statusPorcelain"
        | "statusPorcelainZ"
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

fn parse_beads_board_output(stdout: &str) -> Result<(Vec<Value>, String), TypedOperationError> {
    parse_beads_board_output_with_limits(stdout, default_beads_board_limits())
}

fn parse_beads_show_output(stdout: &str) -> Result<(Value, String), TypedOperationError> {
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

fn parse_beads_board_output_with_limits(
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

fn parse_git_branch_list(stdout: &str) -> Vec<Value> {
    let mut branches = Vec::new();
    let mut seen: HashMap<String, bool> = HashMap::new();
    for line in stdout.lines() {
        let mut columns = line.split('\t');
        let name = columns.next().unwrap_or_default().trim();
        let ref_name = columns.next().unwrap_or_default().trim();
        let head_marker = columns.next().unwrap_or_default().trim();
        if name.is_empty()
            || ref_name.is_empty()
            || name.ends_with("/HEAD")
            || ref_name.ends_with("/HEAD")
            || !is_allowed_git_ref(name)
            || seen.contains_key(name)
        {
            continue;
        }
        seen.insert(name.to_string(), true);
        branches.push(json!({
            "current": head_marker == "*",
            "name": name,
            "remote": ref_name.starts_with("refs/remotes/"),
        }));
    }
    branches
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

fn nullish_value_to_string(input: Option<&Value>, default_value: &str) -> String {
    match input {
        None | Some(Value::Null) => default_value.to_string(),
        Some(value) => value_to_string(value),
    }
}

fn normalize_nullish_required_text(
    input: Option<&Value>,
    field: &str,
    default_value: &str,
) -> Result<String, TypedOperationError> {
    match input {
        None | Some(Value::Null) => Ok(default_value.to_string()),
        Some(value) => normalize_required_text(Some(value), field),
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

    #[tokio::test]
    async fn project_setup_action_errors_use_javascript_string_conversion() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let cases = [
            (
                None,
                "Unsupported project setup action: undefined".to_string(),
            ),
            (
                Some(Value::Null),
                "Unsupported project setup action: null".to_string(),
            ),
            (
                Some(json!(false)),
                "Unsupported project setup action: false".to_string(),
            ),
            (
                Some(json!({"action": "worktreeSetupCommand"})),
                "Unsupported project setup action: [object Object]".to_string(),
            ),
            (
                Some(json!(["worktreeSetupCommand", null, {"x": 1}])),
                "Unsupported project setup action: worktreeSetupCommand,,[object Object]"
                    .to_string(),
            ),
        ];

        for (action, expected_message) in cases {
            let mut params = Map::new();
            if let Some(action) = action {
                params.insert("action".to_string(), action);
            }
            let error = run_project_setup_command(&params, &ctx).await.unwrap_err();
            assert_eq!(error.code, "badRequest");
            assert_eq!(error.message, expected_message);
        }
    }

    #[tokio::test]
    async fn project_setup_source_project_id_lookup_does_not_trim() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));
        params.insert("setupCommandProjectId".to_string(), json!(" P-test "));

        let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

        assert_eq!(error.code, "notFound");
        assert_eq!(error.message, "Project  P-test  does not exist.");
    }

    #[tokio::test]
    async fn project_setup_blank_source_project_id_still_allows_source_project_path() {
        let dir = tempdir().unwrap();
        let source = dir.path().join("source");
        let target = dir.path().join("target");
        fs::create_dir(&source).unwrap();
        fs::create_dir(&target).unwrap();
        let source_project = json!({
            "projectId": "P-source",
            "path": source.to_string_lossy(),
            "gitConfig": {
                "worktreeCommand": "printf source-selected"
            },
            "projectBoardConfig": {},
        });
        let target_project = json!({
            "projectId": "P-target",
            "path": target.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let ctx = TypedOperationContext {
            beads_cwd: None,
            cwd: target.to_string_lossy().to_string(),
            env_path: None,
            projects: vec![source_project, target_project],
        };
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));
        params.insert("setupCommandProjectId".to_string(), json!("   "));
        params.insert(
            "setupCommandProjectPath".to_string(),
            json!(source.to_string_lossy()),
        );

        let result = run_project_setup_command(&params, &ctx).await.unwrap();

        assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
        assert_eq!(
            result.get("stdout").and_then(Value::as_str),
            Some("source-selected")
        );
        assert_eq!(
            result
                .get("command")
                .and_then(|command| command.get("args"))
                .cloned(),
            Some(json!(["-lc", "<worktree setup command>"]))
        );
    }

    #[tokio::test]
    async fn project_setup_endpoint_scope_project_id_lookup_does_not_trim() {
        let dir = tempdir().unwrap();
        let project = json!({
            "projectId": "P-test",
            "path": dir.path().to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));
        params.insert("projectId".to_string(), json!(" P-test "));

        let error = dispatch_typed_operation_endpoint(
            "/api/runProjectSetupCommand",
            &params,
            vec![project],
        )
        .await
        .unwrap_err();

        assert_eq!(error.code, "notFound");
        assert_eq!(error.message, "Project  P-test  does not exist.");
        assert!(error.scope_rejection);
    }

    #[tokio::test]
    async fn typed_operation_scope_matches_path_resolve_for_project_path_lookup() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        let project = json!({
            "projectId": "P-test",
            "path": repo.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let unresolved_intermediate_path = dir
            .path()
            .join("missing-intermediate")
            .join("..")
            .join("repo");
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));
        params.insert(
            "projectPath".to_string(),
            json!(unresolved_intermediate_path.to_string_lossy()),
        );

        let result = dispatch_typed_operation_endpoint(
            "/api/runProjectSetupCommand",
            &params,
            vec![project],
        )
        .await
        .unwrap();

        assert_eq!(
            result.get("action").and_then(Value::as_str),
            Some("worktreeSetupCommand")
        );
        assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
    }

    #[tokio::test]
    async fn project_setup_command_path_reports_file_without_echoing_path() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("not-a-directory");
        fs::write(&file_path, "x").unwrap();
        let ctx = context(dir.path());
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));
        params.insert(
            "setupCommandProjectPath".to_string(),
            json!(file_path.to_string_lossy()),
        );

        let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

        assert_eq!(error.code, "badRequest");
        assert_eq!(error.message, "setupCommandProjectPath is not a directory.");
    }

    #[tokio::test]
    async fn project_setup_default_lookup_normalizes_context_cwd() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        let project = json!({
            "projectId": "P-test",
            "path": repo.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let ctx = TypedOperationContext {
            beads_cwd: None,
            cwd: repo.join(".").to_string_lossy().to_string(),
            env_path: None,
            projects: vec![project],
        };
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));

        let result = run_project_setup_command(&params, &ctx).await.unwrap();

        assert_eq!(
            result.get("action").and_then(Value::as_str),
            Some("worktreeSetupCommand")
        );
        assert_eq!(result.get("command"), None);
        assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
    }

    #[tokio::test]
    async fn project_setup_default_lookup_validates_candidate_project_paths() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let stale = dir.path().join("stale");
        fs::create_dir(&repo).unwrap();
        let stale_project = json!({
            "projectId": "P-stale",
            "path": stale.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let project = json!({
            "projectId": "P-test",
            "path": repo.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {},
        });
        let ctx = TypedOperationContext {
            beads_cwd: None,
            cwd: repo.to_string_lossy().to_string(),
            env_path: None,
            projects: vec![stale_project, project],
        };
        let mut params = Map::new();
        params.insert("action".to_string(), json!("worktreeSetupCommand"));

        let error = run_project_setup_command(&params, &ctx).await.unwrap_err();

        assert_eq!(error.code, "notFound");
        assert_eq!(
            error.message,
            format!("project.path does not exist: {}", stale.to_string_lossy())
        );
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
    fn typed_operation_log_details_omit_command_args_and_output() {
        let result = json!({
            "action": "commit",
            "command": {
                "args": ["commit", "-F", "<stdin>"],
                "cwd": "/Users/person/dev/private-project",
                "executable": "git",
            },
            "exitCode": 1,
            "stderr": "private stderr containing command text",
            "stdout": "private stdout containing command text",
        });

        let details = typed_operation_log_details(&result);

        assert_eq!(details.get("action"), Some(&json!("commit")));
        assert_eq!(details.get("argumentCount"), Some(&json!(3)));
        assert_eq!(details.get("commandBuilt"), Some(&json!(true)));
        assert_eq!(details.get("executable"), Some(&json!("git")));
        assert_eq!(details.get("exitCode"), Some(&json!(1)));
        assert_eq!(details.get("operationError"), Some(&Value::Null));
        assert!(!details.to_string().contains("private-project"));
        assert!(!details.to_string().contains("<stdin>"));
        assert!(!details.to_string().contains("private stdout"));
        assert!(!details.to_string().contains("private stderr"));
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

    #[test]
    fn git_list_branches_plans_structured_branch_command() {
        /*
        CDXC:WorktreeBaseBranch 2026-06-24-11:32:
        Add Worktree branch selection must use a typed Git action that lists
        local and remote-tracking branch refs without exposing a generic shell.
        */
        let dir = tempdir().unwrap();
        let command =
            build_git_command("listBranches", &Map::new(), &dir.path().to_string_lossy()).unwrap();

        assert_eq!(
            command.args,
            vec![
                "for-each-ref",
                "--format=%(refname:short)%09%(refname)%09%(HEAD)",
                "refs/heads",
                "refs/remotes",
            ]
        );
        assert_eq!(
            normalize_git_action(Some(&json!("listBranches"))).unwrap(),
            "listBranches"
        );
    }

    #[test]
    fn git_branch_list_parser_filters_symbolic_and_invalid_refs() {
        let branches = parse_git_branch_list(
            "main\trefs/heads/main\t*\n\
feature/worktree-base\trefs/heads/feature/worktree-base\t\n\
origin/main\trefs/remotes/origin/main\t\n\
origin/HEAD\trefs/remotes/origin/HEAD\t\n\
bad branch\trefs/heads/bad branch\t\n",
        );

        assert_eq!(
            branches,
            vec![
                json!({"current": true, "name": "main", "remote": false}),
                json!({"current": false, "name": "feature/worktree-base", "remote": false}),
                json!({"current": false, "name": "origin/main", "remote": true}),
            ]
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
    fn beads_board_rejects_explicit_row_and_response_limits() {
        let row_error = parse_beads_board_output_with_limits(
            r#"[{"id":"A"},{"id":"B"},{"id":"C"}]"#,
            BeadsBoardLimits {
                response_limit_bytes: 1024,
                row_limit: 2,
            },
        )
        .unwrap_err();
        assert_eq!(row_error.code, "badRequest");
        assert!(row_error.message.contains("2-row limit"));
        assert_eq!(
            row_error
                .details
                .as_ref()
                .and_then(|details| details.get("rowLimit")),
            Some(&json!(2))
        );

        let response_error = parse_beads_board_output_with_limits(
            r#"[{"id":"A","title":"response body is too large"}]"#,
            BeadsBoardLimits {
                response_limit_bytes: 16,
                row_limit: 10,
            },
        )
        .unwrap_err();
        assert_eq!(response_error.code, "badRequest");
        assert!(response_error
            .message
            .contains("16-byte serialized JSON limit"));
        assert_eq!(
            response_error
                .details
                .as_ref()
                .and_then(|details| details.get("responseLimitBytes")),
            Some(&json!(16))
        );
    }

    #[test]
    fn beads_hook_script_matches_typescript_contract() {
        let script = build_ghostex_beads_git_hook_script(
            "pre-commit",
            "/tmp/Ghostex Resources/bin/bd",
            "/tmp/project/.beads",
        );
        assert!(script.contains("BD_BIN='/tmp/Ghostex Resources/bin/bd'"));
        assert!(script.contains("BEADS_DIR_VALUE='/tmp/project/.beads'"));
        assert!(script.contains("HOOK_NAME='pre-commit'"));
        assert!(script.contains("export BEADS_DIR=\"$BEADS_DIR_VALUE\""));
        assert!(script.contains("export BD_GIT_HOOK=1"));
        assert!(script.contains("hooks run \"$HOOK_NAME\""));
        assert!(!script.contains("bd sync"));
        assert!(!script.contains("issues.jsonl"));

        let quoted = shell_single_quote("/tmp/it's/bd");
        assert_eq!(quoted, "'/tmp/it'\\''s/bd'");
    }

    #[tokio::test]
    async fn ensure_beads_hooks_installs_scripts_in_common_git_directory() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let app_bin = dir.path().join("app-web").join("bin");
        let bd = app_bin.join("bd");
        let beads_dir = repo.join(".beads");
        let hook_log = dir.path().join("hook.log");
        fs::create_dir_all(&repo).unwrap();
        fs::create_dir_all(&beads_dir).unwrap();
        fs::create_dir_all(&app_bin).unwrap();
        run_git(["init"], &repo);
        fs::write(
            &bd,
            format!(
                "#!/bin/sh\n\
if [ \"$1\" = \"where\" ]; then\n\
  printf '%s\\n' '{{\"path\":\"{}\"}}'\n\
  exit 0\n\
fi\n\
if [ \"$1\" = \"hooks\" ] && [ \"$2\" = \"run\" ]; then\n\
  printf '%s\\n' \"$BEADS_DIR|$BD_GIT_HOOK|$3\" >> {}\n\
  exit 0\n\
fi\n\
exit 0\n",
                beads_dir.to_string_lossy(),
                shell_single_quote(&hook_log.to_string_lossy()),
            ),
        )
        .unwrap();
        chmod_executable_if_supported(&bd).unwrap();

        let ctx = context(&repo);
        let result = ensure_beads_git_hooks_with_executable(&ctx, &bd.to_string_lossy())
            .await
            .unwrap();
        assert_eq!(result.get("exitCode").and_then(Value::as_i64), Some(0));
        assert_eq!(
            result.get("stdout").and_then(Value::as_str),
            Some("installed")
        );

        let hooks_path = repo.join(".git").join("ghostex-hooks");
        assert_eq!(
            run_git_output(["config", "--get", "core.hooksPath"], &repo).trim(),
            hooks_path.to_string_lossy()
        );
        let pre_commit = fs::read_to_string(hooks_path.join("pre-commit")).unwrap();
        assert!(pre_commit.contains("BD_BIN="));
        assert!(pre_commit.contains("BEADS_DIR_VALUE="));
        assert!(pre_commit.contains("HOOK_NAME='pre-commit'"));
        assert!(pre_commit.contains("hooks run \"$HOOK_NAME\""));
        assert!(!pre_commit.contains("issues.jsonl"));

        let hook_output = StdCommand::new(hooks_path.join("pre-commit"))
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            hook_output.status.success(),
            "hook failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&hook_output.stdout),
            String::from_utf8_lossy(&hook_output.stderr)
        );
        assert_eq!(
            fs::read_to_string(&hook_log).unwrap().trim(),
            format!("{}|1|pre-commit", beads_dir.to_string_lossy())
        );
    }

    #[test]
    fn beads_where_output_requires_absolute_directory() {
        let dir = tempdir().unwrap();
        let beads_dir = dir.path().join(".beads");
        fs::create_dir(&beads_dir).unwrap();
        let stdout = format!(r#"{{"path":"{}"}}"#, beads_dir.to_string_lossy());
        assert_eq!(
            normalize_beads_where_directory(&stdout).unwrap(),
            beads_dir.to_string_lossy()
        );

        let relative = normalize_beads_where_directory(r#"{"path":".beads"}"#).unwrap_err();
        assert_eq!(relative.code, "badRequest");
        assert!(relative.message.contains("absolute storage path"));

        let file_path = dir.path().join("not-a-dir");
        fs::write(&file_path, "x").unwrap();
        let file_stdout = format!(r#"{{"path":"{}"}}"#, file_path.to_string_lossy());
        let file_error = normalize_beads_where_directory(&file_stdout).unwrap_err();
        assert_eq!(file_error.code, "badRequest");
        assert!(file_error.message.contains("not a directory"));
    }

    #[test]
    fn beads_commands_match_typescript_nullish_argument_behavior() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());

        let mut create_params = Map::new();
        create_params.insert("title".to_string(), json!("Create from board"));
        create_params.insert("description".to_string(), Value::Null);
        create_params.insert("priority".to_string(), Value::Null);
        let create =
            build_beads_command_with_executable("create", &create_params, &ctx, "/tmp/bd").unwrap();
        assert_eq!(
            create.args,
            vec![
                "create",
                "--title",
                "Create from board",
                "--description",
                "",
                "--priority",
                "2",
                "--type",
                "task",
                "--json",
            ]
        );

        let mut update_description_params = Map::new();
        update_description_params.insert("issueId".to_string(), json!("gxserver-15"));
        update_description_params.insert("description".to_string(), Value::Null);
        let update_description = build_beads_command_with_executable(
            "updateDescription",
            &update_description_params,
            &ctx,
            "/tmp/bd",
        )
        .unwrap();
        assert_eq!(
            update_description.args,
            vec!["update", "gxserver-15", "--description", "", "--json"]
        );

        let mut update_params = Map::new();
        update_params.insert("issueId".to_string(), json!("gxserver-15"));
        update_params.insert("description".to_string(), Value::Null);
        let update =
            build_beads_command_with_executable("update", &update_params, &ctx, "/tmp/bd").unwrap();
        assert_eq!(
            update.args,
            vec!["update", "gxserver-15", "--description", "null", "--json"]
        );
    }

    #[test]
    fn beads_board_scope_controls_command_cwd_only_for_board_calls() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let board_dir = dir.path().join("board");
        fs::create_dir(&repo).unwrap();
        fs::create_dir(&board_dir).unwrap();
        let project = json!({
            "projectId": "P-test",
            "path": repo.to_string_lossy(),
            "gitConfig": {},
            "projectBoardConfig": {
                "beadsDirectory": board_dir.to_string_lossy()
            },
        });

        let mut board_params = Map::new();
        board_params.insert("action".to_string(), json!("list"));
        board_params.insert("projectBoardScope".to_string(), json!(true));
        board_params.insert("projectId".to_string(), json!("P-test"));
        let board_context = resolve_project_operation_context(
            "/api/runBeadsAction",
            &board_params,
            vec![project.clone()],
        )
        .unwrap();
        let board_command =
            build_beads_command_with_executable("list", &board_params, &board_context, "/tmp/bd")
                .unwrap();
        assert_eq!(board_command.cwd, board_dir.to_string_lossy());

        let mut probe_params = Map::new();
        probe_params.insert("action".to_string(), json!("storageExists"));
        probe_params.insert("projectId".to_string(), json!("P-test"));
        let probe_context = resolve_project_operation_context(
            "/api/runBeadsAction",
            &probe_params,
            vec![project.clone()],
        )
        .unwrap();
        assert_eq!(probe_context.beads_cwd, None);

        let worktree_context = resolve_project_operation_context(
            "/api/runWorktreeAction",
            &board_params,
            vec![project],
        )
        .unwrap();
        assert_eq!(worktree_context.beads_cwd, None);
    }

    #[test]
    fn beads_label_arguments_reject_non_arrays() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());

        let mut create_params = Map::new();
        create_params.insert("title".to_string(), json!("Create from board"));
        create_params.insert("labels".to_string(), json!("ui"));
        let create_error =
            build_beads_command_with_executable("create", &create_params, &ctx, "/tmp/bd")
                .unwrap_err();
        assert_eq!(create_error.code, "badRequest");
        assert!(create_error.message.contains("labels must be an array"));

        let mut set_params = Map::new();
        set_params.insert("issueId".to_string(), json!("gxserver-15"));
        set_params.insert("labels".to_string(), json!("ui"));
        let set_error =
            build_beads_command_with_executable("setLabels", &set_params, &ctx, "/tmp/bd")
                .unwrap_err();
        assert_eq!(set_error.code, "badRequest");
        assert!(set_error.message.contains("labels must be an array"));
    }

    #[test]
    fn beads_show_requests_comment_bodies() {
        let dir = tempdir().unwrap();
        let ctx = context(dir.path());
        let mut params = Map::new();
        params.insert("issueId".to_string(), json!("gxserver-57"));

        let command =
            build_beads_command_with_executable("show", &params, &ctx, "/tmp/bd").unwrap();

        assert_eq!(
            command.args,
            vec!["show", "gxserver-57", "--include-comments", "--json"]
        );
    }

    #[test]
    fn beads_show_output_normalizes_enveloped_single_issue_with_comments() {
        let (issue, stdout) = parse_beads_show_output(
            r#"{"data":[{"id":"gxserver-57","comment_count":1,"comments":[{"id":1,"text":"hello"}]}]}"#,
        )
        .unwrap();

        assert_eq!(issue.get("id"), Some(&json!("gxserver-57")));
        assert_eq!(
            issue
                .get("comments")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1)
        );
        assert_eq!(
            serde_json::from_str::<Value>(&stdout).unwrap(),
            json!({"id":"gxserver-57","comment_count":1,"comments":[{"id":1,"text":"hello"}]})
        );
    }

    #[test]
    fn typed_operation_environment_applies_beads_json_envelope() {
        let env = typed_operation_environment(
            Some("/tmp/bin"),
            &[
                ("BD_JSON_ENVELOPE".to_string(), "1".to_string()),
                ("NO_COLOR".to_string(), "1".to_string()),
            ],
        );
        assert!(env.contains(&("PATH".to_string(), "/tmp/bin".to_string())));
        assert!(env.contains(&("BD_JSON_ENVELOPE".to_string(), "1".to_string())));
        assert!(!env.iter().any(|(key, _)| key == "NO_COLOR"));
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
        let output = run_git_raw(args, cwd);
        assert!(
            output.status.success(),
            "git failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn run_git_output<I, S>(args: I, cwd: &Path) -> String
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        let output = run_git_raw(args, cwd);
        assert!(
            output.status.success(),
            "git failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    fn run_git_raw<I, S>(args: I, cwd: &Path) -> std::process::Output
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
        if !output.status.success() {
            eprintln!(
                "git {:?} failed\nstdout:\n{}\nstderr:\n{}",
                args,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        output
    }
}
