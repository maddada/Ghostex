//! `/api/readProjectGitState`: a project's Git state in one call.
//!
//! CDXC:Git 2026-09-25 WHY:
//! The desktop's QuickJS runtime used to assemble a project's Git state from about ten
//! `/api/runGitAction` and `/api/runGitHubAction` calls (branch, status, numstat, untracked,
//! upstream counts, remotes, origin URL, `gh --version`, `gh pr view`) and, for the sidebar's
//! background poll, from three more per project and poll. Every client that shows Git needs the
//! same answer (the desktop, the web build, a remote machine's tunnel), so the daemon that owns the
//! repository reads it once and answers with the parsed facts. The subprocesses are the same
//! allowlisted typed operations; only their parsing moved here. Preferences are not read: the
//! client lays the project's `gitConfig` over the answer.
//!
//! Three shapes. A full read (`gitHub` asks for the GitHub CLI probe as well) answers what the
//! titlebar Git menu and the Git actions need. A diff-stats read (`diffStatsOnly`) answers only the
//! project header's numbers, cheaply: `diffNumstat` first, and a repository check only when that
//! fails, so a poll of a healthy repository spawns one process. A GitHub read (`gitHubOnly`) runs
//! only the two `gh` probes, for a client that published local facts first.
//!
//! SEE-ALSO: packages/gx-core/src/git_menu/read.rs (the client's reader),
//! apps/desktop/src/app/gx_store/git/ (the callers), server/src/git_ship_workflow.rs.

use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use crate::domain::{DomainRepository, DomainStateError, read_domain_rpc_params};
use crate::protocol::rpc_success;
use crate::server::{
    AppState, RoutedResponse, domain_error_response, routed_json, typed_operation_error_response,
};
use crate::storage::open_gxserver_database;
use crate::typed_operations::{TypedOperationError, dispatch_typed_operation_endpoint};

pub const READ_PROJECT_GIT_STATE_ENDPOINT: &str = "/api/readProjectGitState";

/// How many untracked files one `countFileLines` call sums (`CDXC:Git 2026-08-16`).
const UNTRACKED_LINE_COUNT_BATCH_SIZE: usize = 200;

pub(crate) async fn handle_read_project_git_state_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let projects = match list_projects(state) {
        Ok(projects) => projects,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match read_project_git_state(&params, projects).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => typed_operation_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn list_projects(state: &AppState) -> Result<Vec<Value>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    DomainRepository::new(&db, state.metadata.server_id.as_str()).list_projects()
}

/// One typed operation against `project_id`.
pub(crate) async fn run_project_operation(
    endpoint: &str,
    projects: &[Value],
    project_id: &str,
    action: &str,
    extra: Map<String, Value>,
) -> Result<Value, TypedOperationError> {
    let mut params = extra;
    params.insert("action".to_string(), json!(action));
    params.insert("projectId".to_string(), json!(project_id));
    dispatch_typed_operation_endpoint(endpoint, &params, projects.to_vec()).await
}

pub(crate) async fn git(
    projects: &[Value],
    project_id: &str,
    action: &str,
) -> Result<Value, TypedOperationError> {
    run_project_operation(
        "/api/runGitAction",
        projects,
        project_id,
        action,
        Map::new(),
    )
    .await
}

pub(crate) fn exit_code(result: &Value) -> i64 {
    result.get("exitCode").and_then(Value::as_i64).unwrap_or(1)
}

pub(crate) fn stdout(result: &Value) -> &str {
    result.get("stdout").and_then(Value::as_str).unwrap_or("")
}

fn read_project_id(params: &Map<String, Value>) -> Result<String, TypedOperationError> {
    params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| TypedOperationError::bad_request("projectId is required."))
}

pub async fn read_project_git_state(
    params: &Map<String, Value>,
    projects: Vec<Value>,
) -> Result<Value, TypedOperationError> {
    let project_id = read_project_id(params)?;
    let flag = |key: &str| params.get(key).and_then(Value::as_bool) == Some(true);
    // The GitHub CLI probe alone: a client that published local facts without it runs this a
    // moment later (`CDXC:Git 2026-07-29`).
    if flag("gitHubOnly") {
        return Ok(json!({ "gitHub": read_git_hub_state(&projects, &project_id).await? }));
    }
    if flag("diffStatsOnly") {
        return read_diff_stats(&projects, &project_id, flag("countUntrackedWhenClean")).await;
    }
    let state = read_git_state(&projects, &project_id).await?;
    let Some(mut state) = state else {
        return Ok(json!({ "isRepo": false }));
    };
    if flag("gitHub") {
        state["gitHub"] = read_git_hub_state(&projects, &project_id).await?;
    }
    Ok(state)
}

/// The local facts, or `None` when the project is not inside a work tree.
pub(crate) async fn read_git_state(
    projects: &[Value],
    project_id: &str,
) -> Result<Option<Value>, TypedOperationError> {
    let repo = git(projects, project_id, "isInsideWorkTree").await?;
    if exit_code(&repo) != 0 || stdout(&repo).trim() != "true" {
        return Ok(None);
    }
    let (branch, status, diff, untracked, upstream, remotes, origin) = tokio::try_join!(
        git(projects, project_id, "branch"),
        git(projects, project_id, "statusPorcelain"),
        git(projects, project_id, "diffNumstat"),
        git(projects, project_id, "listUntracked"),
        git(projects, project_id, "upstreamCounts"),
        git(projects, project_id, "listRemotes"),
        git(projects, project_id, "getOriginRemoteUrl"),
    )?;
    let mut files = parse_numstat_files(stdout(&diff));
    files.extend(parse_status_porcelain_files(stdout(&status)));
    files.extend(
        stdout(&untracked)
            .split('\0')
            .filter(|path| !path.is_empty())
            .filter_map(normalize_relative_git_file_path)
            .map(|path| ChangedFile {
                additions: 0,
                deletions: 0,
                path,
            }),
    );
    let files = merge_changed_files(files);
    let additions: i64 = files.iter().map(|file| file.additions).sum();
    let deletions: i64 = files.iter().map(|file| file.deletions).sum();
    let upstream_parts: Vec<&str> = if exit_code(&upstream) == 0 {
        stdout(&upstream).split_whitespace().collect()
    } else {
        Vec::new()
    };
    let count = |index: usize| {
        upstream_parts
            .get(index)
            .and_then(|part| part.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map(|value| value as i64)
            .unwrap_or(0)
    };
    let branch_name = stdout(&branch).trim();
    Ok(Some(json!({
        "additions": additions,
        "aheadCount": count(0),
        "behindCount": count(1),
        "branch": (!branch_name.is_empty()).then_some(branch_name),
        "deletions": deletions,
        "files": files
            .iter()
            .map(|file| json!({ "additions": file.additions, "deletions": file.deletions, "path": file.path }))
            .collect::<Vec<_>>(),
        "hasGitHubRemote": exit_code(&origin) == 0
            && normalize_git_hub_remote_url(stdout(&origin)).is_some(),
        "hasOriginRemote": stdout(&remotes).split_whitespace().any(|name| name == "origin"),
        "hasUpstream": exit_code(&upstream) == 0,
        "hasWorkingTreeChanges": !stdout(&status).trim().is_empty(),
        "isRepo": true,
    })))
}

/// `gh --version` and `gh pr view`, parsed.
pub(crate) async fn read_git_hub_state(
    projects: &[Value],
    project_id: &str,
) -> Result<Value, TypedOperationError> {
    let (version, pr) = tokio::try_join!(
        run_project_operation(
            "/api/runGitHubAction",
            projects,
            project_id,
            "version",
            Map::new()
        ),
        run_project_operation(
            "/api/runGitHubAction",
            projects,
            project_id,
            "prView",
            Map::new()
        ),
    )?;
    Ok(json!({
        "hasGitHubCli": exit_code(&version) == 0,
        "pr": parse_git_hub_pull_request(stdout(&pr), exit_code(&pr) == 0),
    }))
}

async fn read_diff_stats(
    projects: &[Value],
    project_id: &str,
    count_untracked_when_clean: bool,
) -> Result<Value, TypedOperationError> {
    let diff = git(projects, project_id, "diffNumstat").await?;
    if exit_code(&diff) != 0 {
        let repo = git(projects, project_id, "isInsideWorkTree").await?;
        if exit_code(&repo) != 0 || stdout(&repo).trim() != "true" {
            return Ok(json!({
                "diffStats": { "additions": 0, "deletions": 0, "files": 0, "isRepo": false },
                "isRepo": false,
            }));
        }
        // A repository whose numbers cannot be read now (no commit yet, a lock): the client
        // keeps the numbers it has.
        return Ok(json!({ "diffStats": null, "isRepo": true }));
    }
    let (mut additions, mut deletions, mut file_count) = (0_i64, 0_i64, 0_i64);
    for line in stdout(&diff)
        .trim()
        .split('\n')
        .filter(|line| !line.is_empty())
    {
        let mut parts = line.split_whitespace();
        additions += numstat_number(parts.next());
        deletions += numstat_number(parts.next());
        file_count += 1;
    }
    /*
    CDXC:Git 2026-05-27-09:25:
    Project-header git stats match Starship-style tracked diffs (`git diff --numstat HEAD`) by
    default. Only fold untracked `wc -l` totals in when the user enables the opt-in setting and
    there are no tracked line changes.
    */
    if count_untracked_when_clean && additions == 0 && deletions == 0 {
        let untracked = git(projects, project_id, "listUntracked").await?;
        let paths: Vec<String> = stdout(&untracked)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        let mut lines = 0_i64;
        for chunk in paths.chunks(UNTRACKED_LINE_COUNT_BATCH_SIZE) {
            let mut extra = Map::new();
            extra.insert("filePaths".to_string(), json!(chunk));
            let result = run_project_operation(
                "/api/runGitAction",
                projects,
                project_id,
                "countFileLines",
                extra,
            )
            .await?;
            if exit_code(&result) != 0 {
                return Ok(json!({ "diffStats": null, "isRepo": true }));
            }
            lines += stdout(&result).trim().parse::<i64>().unwrap_or(0);
        }
        additions += lines;
        file_count += paths.len() as i64;
    }
    Ok(json!({
        "diffStats": {
            "additions": additions,
            "deletions": deletions,
            "files": file_count,
            "isRepo": true,
        },
        "isRepo": true,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChangedFile {
    additions: i64,
    deletions: i64,
    path: String,
}

fn numstat_number(value: Option<&str>) -> i64 {
    match value {
        None | Some("-") | Some("") => 0,
        Some(value) => value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| value as i64)
            .unwrap_or(0),
    }
}

/// `parseGpuiGitNumstatFiles`.
fn parse_numstat_files(stdout: &str) -> Vec<ChangedFile> {
    stdout
        .trim()
        .split('\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let additions = parts.next();
            let deletions = parts.next();
            let path = normalize_relative_git_file_path(&parts.collect::<Vec<_>>().join(" "))?;
            Some(ChangedFile {
                additions: numstat_number(additions),
                deletions: numstat_number(deletions),
                path,
            })
        })
        .collect()
}

/// `parseGpuiGitStatusPorcelainFiles`.
fn parse_status_porcelain_files(stdout: &str) -> Vec<ChangedFile> {
    stdout
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .filter(|line| line.chars().count() >= 4)
        .filter_map(|line| {
            let raw: String = line.chars().skip(3).collect();
            let raw = raw.trim();
            let candidate = if raw.contains(" -> ") {
                raw.rsplit(" -> ").next().unwrap_or("")
            } else {
                raw
            };
            normalize_relative_git_file_path(candidate).map(|path| ChangedFile {
                additions: 0,
                deletions: 0,
                path,
            })
        })
        .collect()
}

/// `mergeGpuiGitChangedFiles`: one row per path, in first-seen order, with the larger counts.
fn merge_changed_files(files: Vec<ChangedFile>) -> Vec<ChangedFile> {
    let mut merged: Vec<ChangedFile> = Vec::new();
    for file in files {
        if let Some(existing) = merged.iter_mut().find(|known| known.path == file.path) {
            existing.additions = existing.additions.max(file.additions);
            existing.deletions = existing.deletions.max(file.deletions);
        } else {
            merged.push(ChangedFile {
                additions: file.additions.max(0),
                deletions: file.deletions.max(0),
                path: file.path,
            });
        }
    }
    merged
}

/// `normalizeGpuiRelativeGitFilePath`.
pub(crate) fn normalize_relative_git_file_path(file_path: &str) -> Option<String> {
    let replaced = file_path.replace('\\', "/");
    let normalized = replaced.trim_start_matches('/').trim();
    if normalized.is_empty() || normalized.contains('\0') {
        return None;
    }
    if normalized
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return None;
    }
    Some(normalized.to_string())
}

/// `normalizeGpuiGitHubRemoteUrl`: the `https://github.com/<owner>/<repo>` an origin URL points
/// at, or `None` for any other host.
fn normalize_git_hub_remote_url(remote_url: &str) -> Option<String> {
    let first = remote_url.split_whitespace().next().unwrap_or("");
    let trimmed = first.strip_suffix(".git").unwrap_or(first);
    if trimmed.is_empty() {
        return None;
    }
    if let Some(path) = trimmed.strip_prefix("git@github.com:") {
        if path.is_empty() || path.contains('#') || path.contains('?') {
            return None;
        }
        let path = path.trim_start_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        return Some(format!("https://github.com/{path}"));
    }
    let (_, rest) = trimmed.split_once("://")?;
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host).to_ascii_lowercase();
    if host != "github.com" {
        return None;
    }
    let path = path.split(['?', '#']).next().unwrap_or("");
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    (!path.is_empty()).then(|| format!("https://github.com/{path}"))
}

/// `parseGpuiGitHubPullRequest`: the `gh pr view --json` answer, when it names an open, closed or
/// merged pull request with a title and a URL.
fn parse_git_hub_pull_request(stdout: &str, success: bool) -> Value {
    if !success || stdout.trim().is_empty() {
        return Value::Null;
    }
    let Ok(candidate) = serde_json::from_str::<Value>(stdout) else {
        return Value::Null;
    };
    let text = |key: &str| {
        candidate
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
    };
    let state = candidate
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    let (Some(url), Some(title)) = (text("url"), text("title")) else {
        return Value::Null;
    };
    if !matches!(state.as_str(), "open" | "closed" | "merged") {
        return Value::Null;
    }
    let mut pr = json!({ "state": state, "title": title, "url": url });
    if let Some(number) = candidate.get("number").filter(|number| number.is_number()) {
        pr["number"] = number.clone();
    }
    pr
}
