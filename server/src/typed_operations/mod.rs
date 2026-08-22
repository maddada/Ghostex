use std::env;

use serde_json::{json, Map, Value};

pub mod beads;
pub mod git;
pub mod github;
pub mod project_setup;
pub mod values;
pub mod worktree;
#[cfg(test)]
mod tests;

pub use values::TypedOperationError;

pub(crate) use beads::*;
pub(crate) use git::*;
pub(crate) use github::*;
pub(crate) use project_setup::*;
pub(crate) use values::*;
pub(crate) use worktree::*;

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

pub(crate) fn resolve_project_operation_context(
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
            normalize_existing_directory_path_value(Some(&Value::String(path)), "beadsDirectory")
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
