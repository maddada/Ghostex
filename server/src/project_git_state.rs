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

pub mod checkout;
mod diff_stats_cache;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, routed_json, typed_operation_error_response, AppState, RoutedResponse,
};
use crate::storage::open_gxserver_database;
use crate::typed_operations::{dispatch_typed_operation_endpoint, TypedOperationError};

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
    let projects = match crate::server::blocking_section(|| requested_project(state, &params)) {
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

/// The requested project as the one-row project list the typed operations resolve their scope from: every operation this endpoint runs is scoped to `projectId`, so reading the whole registry for each call (the sidebar's poll makes one a second) only to pick one row out of it is skipped. An unknown or missing id yields an empty list, which the operations refuse exactly as before.
fn requested_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Vec<Value>, DomainStateError> {
    let Ok(project_id) = read_project_id(params) else {
        return Ok(Vec::new());
    };
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    Ok(
        DomainRepository::new(&db, state.metadata.server_id.as_str())
            .get_project(&project_id)?
            .into_iter()
            .collect(),
    )
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

/// `result`, unless its git ran out of time: that answers nothing, so it is an error and never
/// kept (diff_stats_cache.rs) as "not a repository" or as numbers (review of 23fa0ff90).
fn answered(result: Value) -> Result<Value, TypedOperationError> {
    if result.pointer("/error/code").and_then(Value::as_str) == Some("timeout") {
        return Err(git_timed_out());
    }
    Ok(result)
}

/// The error a git call that ran out of time answers with.
pub(super) fn git_timed_out() -> TypedOperationError {
    TypedOperationError::dependency_unavailable(GIT_TIMED_OUT)
}

const GIT_TIMED_OUT: &str = "Git did not answer in time.";

/// Whether `error` is a git call that ran out of time (`answered`).
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub(super) fn is_git_timeout(error: &TypedOperationError) -> bool {
    error.message == GIT_TIMED_OUT
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

/// `checkout: true`: `{"checkout": …}` for the project's folder, read off the async workers.
async fn read_checkout(
    params: &Map<String, Value>,
    projects: &[Value],
) -> Result<Value, TypedOperationError> {
    let path = projects
        .first()
        .and_then(|project| project.get("path"))
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(std::path::PathBuf::from)
        .ok_or_else(|| TypedOperationError::bad_request("No project has that projectId."))?;
    let if_origin = params
        .get("ifOrigin")
        .and_then(Value::as_str)
        .map(str::to_string);
    let checkout = tokio::task::spawn_blocking(move || {
        checkout::read_checkout_state(&path, if_origin.as_deref())
    })
    .await
    .map_err(|error| {
        TypedOperationError::bad_request(format!("Reading the checkout failed: {error}"))
    })?;
    Ok(json!({ "checkout": checkout }))
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
    // The checkout's work not on origin yet: its submodules, worktrees and stashes too (checkout.rs).
    // `ifOrigin` answers a project of another repository with its origin alone.
    if flag("checkout") {
        return read_checkout(params, &projects).await;
    }
    if flag("diffStatsOnly") {
        let count_untracked = flag("countUntrackedWhenClean");
        return match diff_stats_cache::read_diff_stats_cached(&projects, count_untracked, || {
            read_diff_stats(&projects, &project_id, count_untracked)
        })
        .await
        {
            Err(error) if error.is_command_not_found() => Ok(not_a_repo_diff_stats()),
            result => result,
        };
    }
    // A project that spans several repos reads each repo (not cached: the cache keys on one
    // checkout).
    if let Some(state) = read_multi_repo_git_state(&projects, &project_id).await? {
        let mut state = state;
        if flag("gitHub") {
            state["gitHub"] = read_git_hub_state(&projects, &project_id).await?;
        }
        return Ok(state);
    }
    let state = match diff_stats_cache::read_git_state_cached(&projects, || {
        read_git_state(&projects, &project_id)
    })
    .await
    {
        Err(error) if error.is_command_not_found() => None,
        result => result?,
    };
    let Some(mut state) = state else {
        return Ok(json!({ "isRepo": false }));
    };
    if flag("gitHub") {
        state["gitHub"] = read_git_hub_state(&projects, &project_id).await?;
    }
    Ok(state)
}

/// The local facts of a project that spans several repos, or `None` for any other project: each
/// repo read in that repo, its files under the repo's place in the project (the paths Changes
/// shows and a Commit takes), the numbers summed, and the branch of the project's own repo (the
/// first one when the project folder is not a repo).
///
/// CDXC:Changes 2026-11-27 WHY: a Commit first reads this to check the picked files; read in the
/// project folder alone, a folder of repos was "not a repository" and a submodule's files were
/// unknown, so the Commit refused files Changes had offered.
async fn read_multi_repo_git_state(
    projects: &[Value],
    project_id: &str,
) -> Result<Option<Value>, TypedOperationError> {
    let Some((project, set)) =
        crate::git_ship_workflow::multi_repo_project(projects, project_id).await
    else {
        return Ok(None);
    };
    let lead = set
        .repos
        .iter()
        .position(|repo| repo.kind == crate::multi_repo::RepoKind::Root)
        .unwrap_or(0);
    let mut files: Vec<Value> = Vec::new();
    let (mut additions, mut deletions, mut ahead, mut behind) = (0i64, 0i64, 0i64, 0i64);
    let (mut github, mut origin, mut upstream, mut changes) = (false, false, false, false);
    let mut branch = Value::Null;
    let mut git_provider = Value::Null;
    for (index, repo) in set.repos.iter().enumerate() {
        let (repo_id, row) = crate::git_ship_workflow::repo_project_row(&project, repo);
        let Some(state) = read_git_state(&[row], &repo_id).await? else {
            continue;
        };
        let number = |key: &str| state.get(key).and_then(Value::as_i64).unwrap_or(0);
        let truth = |key: &str| state.get(key).and_then(Value::as_bool) == Some(true);
        additions += number("additions");
        deletions += number("deletions");
        ahead += number("aheadCount");
        behind += number("behindCount");
        github |= truth("hasGitHubRemote");
        origin |= truth("hasOriginRemote");
        upstream |= truth("hasUpstream");
        changes |= truth("hasWorkingTreeChanges");
        if index == lead || branch.is_null() {
            branch = state.get("branch").cloned().unwrap_or(Value::Null);
        }
        if index == lead || git_provider.is_null() {
            if let Some(provider) = state.get("gitProvider") {
                git_provider = provider.clone();
            }
        }
        for file in state
            .get("files")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let mut file = file.clone();
            if let Some(path) = file.get("path").and_then(Value::as_str) {
                if !repo.rel.is_empty() {
                    file["path"] = json!(format!("{}/{path}", repo.rel));
                }
            }
            files.push(file);
        }
    }
    Ok(Some(json!({
        "additions": additions,
        "aheadCount": ahead,
        "behindCount": behind,
        "branch": branch,
        "deletions": deletions,
        "files": files,
        "gitProvider": if git_provider.is_null() { json!("github") } else { git_provider },
        "hasGitHubRemote": github,
        "hasOriginRemote": origin,
        "hasUpstream": upstream,
        "hasWorkingTreeChanges": changes,
        "isRepo": true,
        "multiRepo": true,
    })))
}

/// The local facts, or `None` when the project is not inside a work tree.
pub(crate) async fn read_git_state(
    projects: &[Value],
    project_id: &str,
) -> Result<Option<Value>, TypedOperationError> {
    let repo = answered(git(projects, project_id, "isInsideWorkTree").await?)?;
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
    let [branch, status, diff, untracked, upstream, remotes, origin] =
        [branch, status, diff, untracked, upstream, remotes, origin].map(answered);
    let (branch, status, diff, untracked, upstream, remotes, origin) = (
        branch?, status?, diff?, untracked?, upstream?, remotes?, origin?,
    );
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
    let origin_url = (exit_code(&origin) == 0).then(|| stdout(&origin));
    let resolved_provider =
        crate::source_control::pull_request::resolve_repo_provider_from_origin(origin_url);
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
        "gitProvider": resolved_provider.kind.wire_name(),
        "hasGitHubRemote": origin_url.is_some_and(|url| normalize_git_hub_remote_url(url).is_some()),
        "hasOriginRemote": stdout(&remotes).split_whitespace().any(|name| name == "origin"),
        "hasUpstream": exit_code(&upstream) == 0,
        "hasWorkingTreeChanges": !stdout(&status).trim().is_empty(),
        "isRepo": true,
    })))
}

/// CDXC:Git 2026-09-28 WHY:
/// A computer without Git (a fresh Windows install) answered every background Git read with HTTP 503, so the desktop's 15 second poll logged a server error for each open project forever. Without the program the project simply has no Git state; an action the user asks for still says Git is not installed.
fn not_a_repo_diff_stats() -> Value {
    json!({
        "diffStats": { "additions": 0, "deletions": 0, "files": 0, "isRepo": false },
        "isRepo": false,
    })
}

/// Provider CLI version + current-branch PR/MR view (`gh` / `glab`), parsed.
/// Keeps the `hasGitHubCli` / `pr` wire keys so existing clients continue working unchanged, and
/// includes `provider` (`"github"` / `"gitlab"`) so newer clients show host-specific words.
pub(crate) async fn read_git_hub_state(
    projects: &[Value],
    project_id: &str,
) -> Result<Value, TypedOperationError> {
    match read_git_hub_cli(projects, project_id).await {
        Err(error) if error.is_command_not_found() => Ok(json!({
            "hasGitHubCli": false,
            "pr": Value::Null,
        })),
        result => result,
    }
}

async fn read_git_hub_cli(
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
    let provider_kind = version
        .get("provider")
        .or_else(|| pr.get("provider"))
        .and_then(Value::as_str)
        .and_then(ghostex_gx_protocol::git_provider::GitHostProvider::from_wire_name)
        .unwrap_or_default();
    let parsed_pr = parse_provider_pull_request(provider_kind, stdout(&pr), exit_code(&pr) == 0);
    Ok(json!({
        "hasGitHubCli": exit_code(&version) == 0,
        "pr": parsed_pr,
        "provider": provider_kind.wire_name(),
    }))
}

async fn read_diff_stats(
    projects: &[Value],
    project_id: &str,
    count_untracked_when_clean: bool,
) -> Result<Value, TypedOperationError> {
    let diff = answered(git(projects, project_id, "diffNumstat").await?)?;
    if exit_code(&diff) != 0 {
        let repo = answered(git(projects, project_id, "isInsideWorkTree").await?)?;
        if exit_code(&repo) != 0 || stdout(&repo).trim() != "true" {
            return Ok(not_a_repo_diff_stats());
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
        let untracked = answered(git(projects, project_id, "listUntracked").await?)?;
        // Only regular files have lines: a link to a folder (a shared `node_modules`, listed as
        // untracked because an ignore rule like `node_modules/` matches only a real folder), a
        // dangling link (that link in a fresh worktree) or a file deleted since the listing is
        // not counted. The checks stat each path, so they run off the async worker.
        let root = projects
            .first()
            .and_then(|project| project.get("path"))
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from);
        let listed: Vec<String> = stdout(&untracked)
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_string)
            .collect();
        let paths: Vec<String> = tokio::task::spawn_blocking(move || {
            listed
                .into_iter()
                .filter(|path| {
                    root.as_ref().is_none_or(|root| {
                        std::fs::metadata(root.join(path)).is_ok_and(|meta| meta.is_file())
                    })
                })
                .collect()
        })
        .await
        .map_err(|error| TypedOperationError::dependency_unavailable(error.to_string()))?;
        let mut lines = 0_i64;
        // A file the count could not open or read (no permission, locked on Windows) is not
        // counted either: `countedFiles` says how many were.
        let mut counted_files = 0_i64;
        for chunk in paths.chunks(UNTRACKED_LINE_COUNT_BATCH_SIZE) {
            let mut extra = Map::new();
            extra.insert("filePaths".to_string(), json!(chunk));
            let result = answered(
                run_project_operation(
                    "/api/runGitAction",
                    projects,
                    project_id,
                    "countFileLines",
                    extra,
                )
                .await?,
            )?;
            if exit_code(&result) != 0 {
                return Ok(json!({ "diffStats": null, "isRepo": true }));
            }
            lines += stdout(&result).trim().parse::<i64>().unwrap_or(0);
            counted_files += result
                .get("countedFiles")
                .and_then(Value::as_i64)
                .unwrap_or(chunk.len() as i64);
        }
        additions += lines;
        file_count += counted_files;
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

/// Parses `prView` stdout for the resolved hosting provider (`gh pr view --json` or
/// `glab mr view --output json`). GitHub keeps the exact previous parsing; GitLab parses through
/// the provider layer.
fn parse_provider_pull_request(
    provider: ghostex_gx_protocol::git_provider::GitHostProvider,
    stdout: &str,
    success: bool,
) -> Value {
    if !success || stdout.trim().is_empty() {
        return Value::Null;
    }
    match provider {
        ghostex_gx_protocol::git_provider::GitHostProvider::GitHub => {
            parse_git_hub_pull_request(stdout, success)
        }
        other => crate::source_control::pull_request::provider_for(other)
            .parse_view_output(stdout, None)
            .map(|view| view.to_project_git_json())
            .unwrap_or(Value::Null),
    }
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
    let mut pr = json!({ "provider": "github", "state": state, "title": title, "url": url });
    if let Some(number) = candidate.get("number").filter(|number| number.is_number()) {
        pr["number"] = number.clone();
    }
    pr
}
