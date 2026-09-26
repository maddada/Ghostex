//! The gxserver calls the Git, worktree and export flows make, and nothing else: each one is
//! `gx_rpc` plus the parse of its answer. No CEF, AppKit or window API, so the web build can take
//! this file unchanged (docs/2026-09-25/app-runtime-port/PLAN.md, step 4).

use std::time::Duration;

use ghostex_gx_core::git_menu::{GitHubState, ProjectGitRead, READ_PROJECT_GIT_STATE_PATH};
use serde_json::{Value, json};

use crate::app::gx_store::{GxRpcError, gx_rpc, gx_rpc_with_timeout};
use crate::app::model::GpuiRemoteGxserverRequestTarget;

/// The machine a call goes to: `None` is this computer.
pub(crate) type Remote = Option<GpuiRemoteGxserverRequestTarget>;

pub(crate) const RUN_GIT_SHIP_WORKFLOW_PATH: &str = "/api/runGitShipWorkflow";

/// A commit message an agent writes can take the daemon's whole generation budget (120 s); the
/// old runtime gave the remote call 125 s.
pub(crate) const GIT_SHIP_WORKFLOW_TIMEOUT: Duration = Duration::from_secs(125);

fn parse<T: serde::de::DeserializeOwned>(path: &str, value: Value) -> Result<T, GxRpcError> {
    serde_json::from_value(value)
        .map_err(|_| GxRpcError::transport(path, "gxserver returned an unexpected answer."))
}

/// A project's full Git state; `git_hub` also runs the two `gh` probes.
pub(crate) async fn read_git_state(
    remote: Remote,
    project_id: String,
    git_hub: bool,
) -> Result<ProjectGitRead, GxRpcError> {
    let params = json!({ "gitHub": git_hub, "projectId": project_id });
    let result = gx_rpc(remote, READ_PROJECT_GIT_STATE_PATH, params).await?;
    parse(READ_PROJECT_GIT_STATE_PATH, result)
}

/// Only the GitHub CLI answer.
pub(crate) async fn read_git_hub(
    remote: Remote,
    project_id: String,
) -> Result<GitHubState, GxRpcError> {
    let params = json!({ "gitHubOnly": true, "projectId": project_id });
    let result = gx_rpc(remote, READ_PROJECT_GIT_STATE_PATH, params).await?;
    let read: ProjectGitRead = parse(READ_PROJECT_GIT_STATE_PATH, result)?;
    Ok(read.git_hub.unwrap_or_default())
}

/// A project header's numbers.
pub(crate) async fn read_diff_stats(
    remote: Remote,
    project_id: String,
    count_untracked_when_clean: bool,
) -> Result<ProjectGitRead, GxRpcError> {
    let params = json!({
        "countUntrackedWhenClean": count_untracked_when_clean,
        "diffStatsOnly": true,
        "projectId": project_id,
    });
    let result = gx_rpc(remote, READ_PROJECT_GIT_STATE_PATH, params).await?;
    parse(READ_PROJECT_GIT_STATE_PATH, result)
}

/// Commit, push, sync and pull request (server/src/git_ship_workflow.rs).
pub(crate) async fn run_ship_workflow(remote: Remote, params: Value) -> Result<Value, GxRpcError> {
    gx_rpc_with_timeout(
        remote,
        RUN_GIT_SHIP_WORKFLOW_PATH,
        params,
        GIT_SHIP_WORKFLOW_TIMEOUT,
    )
    .await
}

/// Any other call, with the default time limit or a longer one.
pub(crate) async fn call(
    remote: Remote,
    path: &'static str,
    params: Value,
    timeout: Option<Duration>,
) -> Result<Value, GxRpcError> {
    match timeout {
        Some(timeout) => gx_rpc_with_timeout(remote, path, params, timeout).await,
        None => gx_rpc(remote, path, params).await,
    }
}

/// `/api/runGitAction` with one action, for the dialogs that still read one fact at a time.
pub(crate) async fn git_action(
    remote: Remote,
    project_id: String,
    action: &'static str,
    mut extra: Value,
) -> Result<Value, GxRpcError> {
    extra["action"] = json!(action);
    extra["projectId"] = json!(project_id);
    gx_rpc(remote, "/api/runGitAction", extra).await
}

/// The exit code of a typed operation's answer (`1` when it has none).
pub(crate) fn exit_code(result: &Value) -> i64 {
    result.get("exitCode").and_then(Value::as_i64).unwrap_or(1)
}

pub(crate) fn stdout(result: &Value) -> &str {
    result.get("stdout").and_then(Value::as_str).unwrap_or("")
}
