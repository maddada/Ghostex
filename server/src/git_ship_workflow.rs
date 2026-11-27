//! `/api/runGitShipWorkflow`: commit, push, sync and open a pull request for one project.
//!
//! CDXC:Git 2026-09-25 WHY:
//! The desktop's QuickJS runtime ran these writes as a client-side sequence of typed operations
//! (stage, generate a message, cut a branch, probe Beads, commit, re-read, push or set upstream,
//! pull fast-forward, create the pull request), once for this computer and once, slightly
//! differently, down a remote machine's tunnel. The rules in that sequence (which push a branch
//! needs, when a behind branch refuses, when the Beads pre-commit hook is bypassed) are the same
//! for every client, so the daemon that owns the repository runs them. The client keeps what is
//! its own: the review dialog and its trusted file list, the started and finished toasts around
//! this call, opening the pull request, and deleting the worktree after a confirmed pull request.
//!
//! CDXC:Git 2026-06-24-16:28:
//! A pull request counts only when `gh` confirms an open one with a github.com pull URL; the
//! client deletes a worktree after a pull request only on that confirmation.
//!
//! SEE-ALSO: server/src/project_git_state.rs, apps/desktop/src/app/gx_store/git/ (the caller),
//! packages/gx-core/src/git_menu/ (the decisions around it).

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::agents::read_agent_settings;
use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::project_git_state::{exit_code, git, list_projects, run_project_operation, stdout};
use crate::protocol::rpc_success;
use crate::server::{
    build_commit_message_generation_shell_command, checkout_project_new_branch_for_commit,
    domain_error_response, generate_commit_message_for_project,
    resolve_commit_message_generation_agent, routed_json, AppState, ProjectWorktreeOperationError,
    RoutedResponse,
};
use crate::storage::open_gxserver_database;
use crate::typed_operations::{create_pull_request_for_project, TypedOperationError};

pub const RUN_GIT_SHIP_WORKFLOW_ENDPOINT: &str = "/api/runGitShipWorkflow";

pub(crate) async fn handle_run_git_ship_workflow_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    // CDXC:Git 2026-10-25 WHY: the workflow runs as its own task, not inside the request. A request the caller abandons (a page reloaded or closed, a dropped connection, the client's time limit) used to drop it half way: an empty message had already staged the files, the message agent was killed with it, and nothing was committed. It now always finishes; the caller that went away re-reads the Git state.
    let workflow_state = state.clone();
    let workflow =
        tokio::spawn(async move { run_git_ship_workflow(&workflow_state, &params).await });
    match workflow.await {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => domain_error_response(endpoint_path, request_id, error),
        Err(_) => domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "internalError",
                message: "The Git workflow stopped unexpectedly.".to_string(),
            },
        ),
    }
}

fn failure(message: impl Into<String>) -> DomainStateError {
    DomainStateError::bad_request(message)
}

fn typed(error: TypedOperationError) -> DomainStateError {
    DomainStateError {
        code: error.code,
        message: error.message,
    }
}

/// The commit a workflow starts with.
struct CommitRequest {
    message: String,
    file_paths: Option<Vec<String>>,
    agent_id: Option<String>,
    commit_on_new_ref: bool,
}

fn read_commit_request(value: Option<&Value>) -> Result<Option<CommitRequest>, DomainStateError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| failure("commit must be an object."))?;
    let message = object
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if message.chars().count() > 20_000 {
        return Err(failure("The commit message is too long."));
    }
    let file_paths = match object.get("filePaths") {
        None | Some(Value::Null) => None,
        Some(Value::Array(paths)) => Some(
            paths
                .iter()
                .map(|path| {
                    path.as_str()
                        .map(str::to_string)
                        .ok_or_else(|| failure("filePaths must contain relative paths."))
                })
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(_) => return Err(failure("filePaths must be an array.")),
    };
    Ok(Some(CommitRequest {
        message: message.to_string(),
        file_paths,
        agent_id: object
            .get("agentId")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        commit_on_new_ref: object.get("commitOnNewRef").and_then(Value::as_bool) == Some(true),
    }))
}

pub(crate) async fn run_git_ship_workflow(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| failure("projectId is required."))?;
    let flag = |key: &str| params.get(key).and_then(Value::as_bool) == Some(true);
    let commit = read_commit_request(params.get("commit"))?;
    let projects = list_projects(state)?;
    // CDXC:MultiRepo 2026-11-27: a project with several repos commits, pushes and opens a pull
    // request in each repo on its own (Q5269, Q5276).
    if let Some((project, set)) = multi_repo_project(&projects, &project_id).await {
        return run_multi_repo_ship(
            state,
            &projects,
            &project,
            &set,
            commit.as_ref(),
            MultiShipFlags {
                push: flag("push"),
                sync: flag("sync"),
                pull_request: flag("pullRequest"),
            },
        )
        .await;
    }

    let committed = match &commit {
        Some(request) => {
            commit_changes(state, &projects, &project_id, request).await?;
            true
        }
        None => false,
    };
    let mut pushed = false;
    if flag("sync") {
        sync_current_branch(&projects, &project_id).await?;
        pushed = true;
    } else if flag("push") || flag("pullRequest") {
        push_current_branch(&projects, &project_id).await?;
        pushed = true;
    }
    let mut pull_request = Value::Null;
    if flag("pullRequest") {
        let mut pr_params = Map::new();
        pr_params.insert("projectId".to_string(), json!(project_id));
        let result = create_pull_request_for_project(&pr_params, projects.clone())
            .await
            .map_err(typed)?;
        if !is_confirmed_open_pull_request(&result) {
            return Err(failure(
                "GitHub CLI could not create or find an open pull request.",
            ));
        }
        pull_request = result.get("pr").cloned().unwrap_or(Value::Null);
    }
    Ok(json!({
        "committed": committed,
        "pullRequest": pull_request,
        "pushed": pushed,
    }))
}

/// `commitWithMessage`: stage the reviewed files (or have the prompt agent write the message,
/// which stages them itself), optionally move onto a new branch named after the subject, and
/// commit.
async fn commit_changes(
    state: &AppState,
    projects: &[Value],
    project_id: &str,
    request: &CommitRequest,
) -> Result<(), DomainStateError> {
    let (mut subject, mut body) = parse_commit_message(&request.message);
    if !subject.is_empty() {
        let mut extra = Map::new();
        if let Some(paths) = &request.file_paths {
            extra.insert("filePaths".to_string(), json!(paths));
        }
        let add = run_project_operation("/api/runGitAction", projects, project_id, "addAll", extra)
            .await
            .map_err(typed)?;
        if exit_code(&add) != 0 {
            return Err(failure("Could not stage changes."));
        }
    } else {
        let paths = request
            .file_paths
            .as_ref()
            .filter(|paths| !paths.is_empty())
            .ok_or_else(|| {
                failure("Select at least one changed file before generating a commit message.")
            })?;
        let mut generate = Map::new();
        generate.insert("projectId".to_string(), json!(project_id));
        generate.insert("filePaths".to_string(), json!(paths));
        if let Some(agent_id) = &request.agent_id {
            generate.insert("agentId".to_string(), json!(agent_id));
        }
        refuse_unsupported_generation_agent(state, project_id, &generate)?;
        let generated = generate_commit_message_for_project(state, &generate).await?;
        subject = generated
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        body = generated
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
    }
    if request.commit_on_new_ref {
        let mut branch = Map::new();
        branch.insert("projectId".to_string(), json!(project_id));
        branch.insert("branchLabel".to_string(), json!(subject));
        checkout_project_new_branch_for_commit(state, &branch)
            .await
            .map_err(worktree_error)?;
    }
    let no_verify = should_bypass_missing_beads_database_hook(projects, project_id).await?;
    let mut extra = Map::new();
    extra.insert("messageBody".to_string(), json!(body));
    extra.insert("messageSubject".to_string(), json!(subject));
    extra.insert("noVerify".to_string(), json!(no_verify));
    let commit = run_project_operation("/api/runGitAction", projects, project_id, "commit", extra)
        .await
        .map_err(typed)?;
    if exit_code(&commit) != 0 {
        return Err(failure("Could not commit changes."));
    }
    Ok(())
}

/// The generation agent must exist and support writing a message in the background, checked
/// BEFORE the generation stages anything (`generateCommitMessage`'s checks in the old runtime).
///
/// CDXC:Git 2026-06-24-16:11:
/// Blank commit-message generation mirrors the native background prompt support set. Built-in
/// agents that do not expose a safe headless prompt mode fail explicitly, while configured
/// non-default custom agents may use their stored command.
fn refuse_unsupported_generation_agent(
    state: &AppState,
    project_id: &str,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let project = DomainRepository::new(&db, state.metadata.server_id.as_str())
        .get_project(project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
    let settings = read_agent_settings(&db)?;
    let agent = resolve_commit_message_generation_agent(&project, params, &settings)?;
    build_commit_message_generation_shell_command(&agent, "")?;
    Ok(())
}

fn worktree_error(error: ProjectWorktreeOperationError) -> DomainStateError {
    match error {
        ProjectWorktreeOperationError::Domain(error) => error,
        ProjectWorktreeOperationError::Typed(error) => typed(error),
        ProjectWorktreeOperationError::ProjectPath(_) => failure("Could not create a new branch."),
    }
}

/// `parseGpuiSidebarGitCommitMessage`.
fn parse_commit_message(message: &str) -> (String, String) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return (String::new(), String::new());
    }
    let mut lines = trimmed
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line));
    let subject = lines.next().unwrap_or_default().trim().to_string();
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    (subject, body)
}

/// A repository whose Beads store exists but whose database is missing would fail the Beads
/// pre-commit hook on every commit, so that hook is skipped for exactly that case
/// (`shouldBypassMissingBeadsDatabasePreCommitHook`).
async fn should_bypass_missing_beads_database_hook(
    projects: &[Value],
    project_id: &str,
) -> Result<bool, DomainStateError> {
    let storage = run_project_operation(
        "/api/runBeadsAction",
        projects,
        project_id,
        "storageExists",
        Map::new(),
    )
    .await
    .map_err(typed)?;
    if exit_code(&storage) != 0 || stdout(&storage).trim() != "true" {
        return Ok(false);
    }
    let Ok(status) = run_project_operation(
        "/api/runBeadsAction",
        projects,
        project_id,
        "status",
        Map::new(),
    )
    .await
    else {
        return Ok(false);
    };
    let stderr = status.get("stderr").and_then(Value::as_str).unwrap_or("");
    Ok(exit_code(&status) != 0
        && is_missing_beads_database_error(&format!("{stderr}\n{}", stdout(&status))))
}

fn is_missing_beads_database_error(message: &str) -> bool {
    let lowered = message.to_lowercase();
    [
        "no beads database found",
        "run bd init",
        "run 'bd init",
        "run \"bd init",
        "not initialized",
        "no storage",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

/// What a push decides from.
struct PushFacts {
    branch: Option<String>,
    ahead: i64,
    behind: i64,
    has_upstream: bool,
    has_origin_remote: bool,
}

async fn read_push_facts(
    projects: &[Value],
    project_id: &str,
) -> Result<PushFacts, DomainStateError> {
    let (branch, upstream, remotes) = tokio::try_join!(
        git(projects, project_id, "branch"),
        git(projects, project_id, "upstreamCounts"),
        git(projects, project_id, "listRemotes"),
    )
    .map_err(typed)?;
    let has_upstream = exit_code(&upstream) == 0;
    let counts: Vec<i64> = if has_upstream {
        stdout(&upstream)
            .split_whitespace()
            .map(|part| part.parse::<i64>().unwrap_or(0))
            .collect()
    } else {
        Vec::new()
    };
    let branch = stdout(&branch).trim();
    Ok(PushFacts {
        branch: (!branch.is_empty()).then(|| branch.to_string()),
        ahead: counts.first().copied().unwrap_or(0),
        behind: counts.get(1).copied().unwrap_or(0),
        has_upstream,
        has_origin_remote: stdout(&remotes)
            .split_whitespace()
            .any(|name| name == "origin"),
    })
}

/// `pushCurrentBranch`.
async fn push_current_branch(projects: &[Value], project_id: &str) -> Result<(), DomainStateError> {
    let facts = read_push_facts(projects, project_id).await?;
    push_with_facts(projects, project_id, &facts).await
}

async fn push_with_facts(
    projects: &[Value],
    project_id: &str,
    facts: &PushFacts,
) -> Result<(), DomainStateError> {
    if facts.branch.is_none() {
        return Err(failure("Create and checkout a branch before pushing."));
    }
    if facts.behind > 0 {
        return Err(failure("Branch is behind upstream."));
    }
    let action = if facts.has_upstream {
        "push"
    } else if facts.has_origin_remote {
        // CDXC:Git 2026-06-24-17:47: push the current HEAD so Git derives the branch from the
        // checked-out repository state rather than from a name a client sent.
        "pushSetUpstreamCurrent"
    } else {
        return Err(failure("Add an \"origin\" remote before pushing."));
    };
    let push = git(projects, project_id, action).await.map_err(typed)?;
    if exit_code(&push) != 0 {
        return Err(failure("Could not push branch."));
    }
    Ok(())
}

/// `syncCurrentBranchWithRemote`: fast-forward from the upstream, then push what is ahead; a
/// branch with no upstream is pushed.
async fn sync_current_branch(projects: &[Value], project_id: &str) -> Result<(), DomainStateError> {
    let facts = read_push_facts(projects, project_id).await?;
    if facts.branch.is_none() {
        return Err(failure("Create and checkout a branch before syncing."));
    }
    if !facts.has_upstream {
        return push_with_facts(projects, project_id, &facts).await;
    }
    let pull = git(projects, project_id, "pullFastForward")
        .await
        .map_err(typed)?;
    if exit_code(&pull) != 0 {
        return Err(failure("Could not pull branch."));
    }
    let next = read_push_facts(projects, project_id).await?;
    if next.ahead > 0 {
        push_with_facts(projects, project_id, &next).await?;
    }
    Ok(())
}

/// `isGpuiConfirmedOpenPullRequest`.
fn is_confirmed_open_pull_request(result: &Value) -> bool {
    let Some(pr) = result.get("pr") else {
        return false;
    };
    result.get("ok").and_then(Value::as_bool) == Some(true)
        && pr.get("state").and_then(Value::as_str) == Some("open")
        && pr
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(is_git_hub_pull_url)
}

/// `^https://github.com/[^/\s]+/[^/\s]+/pull/\d+$`.
fn is_git_hub_pull_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    let segment = |part: &str| !part.is_empty() && !part.chars().any(char::is_whitespace);
    parts.len() == 4
        && segment(parts[0])
        && segment(parts[1])
        && parts[2] == "pull"
        && !parts[3].is_empty()
        && parts[3].chars().all(|ch| ch.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// several repos
// ---------------------------------------------------------------------------

struct MultiShipFlags {
    push: bool,
    sync: bool,
    pull_request: bool,
}

pub(crate) async fn multi_repo_project(
    projects: &[Value],
    project_id: &str,
) -> Option<(Value, std::sync::Arc<crate::multi_repo::RepoSet>)> {
    let project = projects
        .iter()
        .find(|project| project.get("projectId").and_then(Value::as_str) == Some(project_id))?
        .clone();
    let path = project.get("path").and_then(Value::as_str)?.to_string();
    let set = tokio::task::spawn_blocking(move || crate::multi_repo::multi_repo_set(&path))
        .await
        .ok()
        .flatten()?;
    Some((project, set))
}

/// A project row for one repo of the project, so the typed operations (which run only in a
/// registered folder) run in that repo; it carries the project's Git settings.
pub(crate) fn repo_project_row(
    project: &Value,
    repo: &crate::multi_repo::RepoEntry,
) -> (String, Value) {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let repo_id = format!("{project_id}::repo::{}", repo.rel);
    let mut row = project.as_object().cloned().unwrap_or_default();
    row.insert("projectId".to_string(), json!(repo_id));
    row.insert("path".to_string(), json!(repo.path));
    row.remove("worktree");
    (repo_id, Value::Object(row))
}

/// Git's own first line from a failed typed operation, or `fallback`.
fn git_reason(result: &Value, fallback: &str) -> String {
    ["stderr", "stdout"]
        .iter()
        .filter_map(|key| result.get(*key).and_then(Value::as_str))
        .flat_map(str::lines)
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("hint:"))
        .fold(None::<&str>, |found, line| match found {
            Some(found) if found.starts_with("fatal: ") || found.starts_with("error: ") => {
                Some(found)
            }
            Some(found) if !(line.starts_with("fatal: ") || line.starts_with("error: ")) => {
                Some(found)
            }
            _ => Some(line),
        })
        .map(|line| {
            line.trim_start_matches("fatal: ")
                .trim_start_matches("error: ")
                .to_string()
        })
        .unwrap_or_else(|| fallback.to_string())
}

/// Commits `paths` (inside the repo) in one repo: the given message, or one the prompt agent
/// writes for this repo's changes.
async fn commit_repo(
    state: &AppState,
    projects: &[Value],
    repo_id: &str,
    repo_path: &str,
    project: &Value,
    request: &CommitRequest,
    paths: &[String],
) -> Result<(), String> {
    let (mut subject, mut body) = parse_commit_message(&request.message);
    if subject.is_empty() {
        let mut generate = Map::new();
        generate.insert("projectId".to_string(), json!(repo_id));
        generate.insert("filePaths".to_string(), json!(paths));
        if let Some(agent_id) = &request.agent_id {
            generate.insert("agentId".to_string(), json!(agent_id));
        }
        let settings = {
            let db = open_gxserver_database(&state.paths).map_err(|error| error.to_string())?;
            read_agent_settings(&db).map_err(|error| error.message)?
        };
        let generated = crate::server::generate_commit_message_in(
            state,
            &generate,
            repo_id,
            project,
            repo_path,
            projects,
            &settings,
            paths.to_vec(),
        )
        .await
        .map_err(|error| error.message)?;
        subject = generated
            .get("subject")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        body = generated
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if subject.is_empty() {
            return Err("The prompt agent wrote no commit message.".to_string());
        }
    } else {
        let mut extra = Map::new();
        extra.insert("filePaths".to_string(), json!(paths));
        let add = run_project_operation("/api/runGitAction", projects, repo_id, "addAll", extra)
            .await
            .map_err(|error| error.message)?;
        if exit_code(&add) != 0 {
            return Err(git_reason(&add, "Could not stage changes."));
        }
    }
    if request.commit_on_new_ref {
        let slug = crate::worktree_sessions::slugify_branch_title(&subject)
            .unwrap_or_else(|| "changes".to_string());
        let repo = std::path::PathBuf::from(repo_path);
        let made = tokio::task::spawn_blocking(move || {
            use crate::multi_repo::git;
            for index in 0..20 {
                let branch = if index == 0 {
                    slug.clone()
                } else {
                    format!("{slug}-{}", index + 1)
                };
                if git::branch_taken(&repo, &branch) {
                    continue;
                }
                let run = git::run(&repo, &["checkout", "-b", &branch], git::GIT_TIMEOUT);
                return if run.ok {
                    Ok(())
                } else {
                    Err(run.reason("Could not create a new branch."))
                };
            }
            Err("Could not find a free branch name.".to_string())
        })
        .await
        .map_err(|error| error.to_string())?;
        made?;
    }
    let mut extra = Map::new();
    extra.insert("messageBody".to_string(), json!(body));
    extra.insert("messageSubject".to_string(), json!(subject));
    extra.insert("noVerify".to_string(), json!(false));
    let commit = run_project_operation("/api/runGitAction", projects, repo_id, "commit", extra)
        .await
        .map_err(|error| error.message)?;
    if exit_code(&commit) != 0 {
        return Err(git_reason(&commit, "Could not commit changes."));
    }
    Ok(())
}

/// After a submodule commits, its new pointer goes into the commit of the repo holding it.
fn add_pointer_to_parent(
    set: &crate::multi_repo::RepoSet,
    submodule: &crate::multi_repo::RepoEntry,
    picked: &mut Vec<(String, Vec<String>)>,
) {
    let Some(parent) = set
        .repos
        .iter()
        .filter(|candidate| candidate.rel != submodule.rel)
        .filter(|candidate| {
            candidate.rel.is_empty() || submodule.rel.starts_with(&format!("{}/", candidate.rel))
        })
        .max_by_key(|candidate| candidate.rel.len())
    else {
        return;
    };
    let inside = if parent.rel.is_empty() {
        submodule.rel.clone()
    } else {
        submodule.rel[parent.rel.len() + 1..].to_string()
    };
    match picked.iter_mut().find(|(rel, _)| rel == &parent.rel) {
        Some((_, paths)) => {
            if !paths.contains(&inside) {
                paths.push(inside);
            }
        }
        None => picked.push((parent.rel.clone(), vec![inside])),
    }
}

/// Every changed path in a repo (`git status -z`; a rename gives its new path).
fn changed_paths(repo: &std::path::Path) -> Vec<String> {
    let run = crate::multi_repo::git::run(
        repo,
        &[
            "status",
            "--porcelain",
            "-z",
            "-uall",
            "--ignore-submodules=dirty",
        ],
        crate::multi_repo::git::GIT_TIMEOUT,
    );
    if !run.ok {
        return Vec::new();
    }
    let mut paths = Vec::new();
    let mut entries = run.stdout.split('\0');
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (status, path) = entry.split_at(3);
        paths.push(path.to_string());
        if status.starts_with('R') || status.starts_with('C') {
            entries.next();
        }
    }
    paths
}

/// Whether a repo has commits to push: ahead of its upstream, or, with none, commits its default
/// branch lacks.
fn repo_has_unpushed_work(repo: &std::path::Path) -> bool {
    use crate::multi_repo::git;
    if let Some(counts) = git::read(
        repo,
        &["rev-list", "--count", "--left-right", "@{upstream}...HEAD"],
    ) {
        return counts
            .split_whitespace()
            .nth(1)
            .and_then(|ahead| ahead.parse::<u64>().ok())
            .unwrap_or(0)
            > 0;
    }
    let Some(default) = git::default_branch(repo) else {
        return false;
    };
    if git::current_branch(repo).as_deref() == Some(default.as_str()) {
        return false;
    }
    git::read(repo, &["rev-list", "--count", &format!("{default}..HEAD")])
        .and_then(|count| count.parse::<u64>().ok())
        .unwrap_or(0)
        > 0
}

/// `runGitShipWorkflow` for a project with several repos. Each repo with picked files (every
/// repo with changes when no files are named) gets its own commit; push, sync and the pull
/// request then run in each repo with commits to send. A repo that fails says git's reason and
/// the others carry on, and the answer lists every repo (even when all of them failed).
async fn run_multi_repo_ship(
    state: &AppState,
    projects: &[Value],
    project: &Value,
    set: &crate::multi_repo::RepoSet,
    commit: Option<&CommitRequest>,
    flags: MultiShipFlags,
) -> Result<Value, DomainStateError> {
    let mut all_projects = projects.to_vec();
    let mut rows: Vec<(String, crate::multi_repo::RepoEntry)> = Vec::new();
    for repo in &set.repos {
        let (repo_id, row) = repo_project_row(project, repo);
        all_projects.push(row);
        rows.push((repo_id, repo.clone()));
    }
    // Deepest first: a submodule commits before the repo that points at it, so that repo's commit
    // can carry the new pointer.
    rows.sort_by_key(|(_, repo)| {
        std::cmp::Reverse(std::path::Path::new(&repo.path).components().count())
    });
    // The files to commit in each repo.
    let mut picked: Vec<(String, Vec<String>)> = Vec::new();
    if let Some(request) = commit {
        match &request.file_paths {
            Some(paths) => {
                for (repo, inside) in crate::multi_repo::changes::group_paths(set, paths) {
                    picked.push((repo.rel.clone(), inside));
                }
            }
            None => {
                let repos = set.repos.clone();
                let changed = tokio::task::spawn_blocking(move || {
                    repos
                        .iter()
                        .filter_map(|repo| {
                            let paths = changed_paths(std::path::Path::new(&repo.path));
                            (!paths.is_empty()).then(|| (repo.rel.clone(), paths))
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .unwrap_or_default();
                picked.extend(changed);
            }
        }
        if picked.is_empty() {
            return Err(failure("Select at least one changed file to commit."));
        }
    }
    let sends = flags.push || flags.sync || flags.pull_request;
    let mut results: Vec<Value> = Vec::new();
    let (mut tried, mut failed) = (0usize, 0usize);
    let mut any_committed = false;
    let mut any_pushed = false;
    let mut first_pull_request = Value::Null;
    for (repo_id, repo) in &rows {
        let paths = picked
            .iter()
            .find(|(rel, _)| rel == &repo.rel)
            .map(|(_, paths)| paths.clone())
            .filter(|paths| !paths.is_empty());
        let mut row = json!({
            "name": repo.name,
            "rel": repo.rel,
            "committed": false,
            "pushed": false,
            "pullRequest": Value::Null,
            "error": Value::Null,
        });
        let mut error: Option<String> = None;
        if let (Some(request), Some(paths)) = (commit, paths.as_ref()) {
            tried += 1;
            match commit_repo(
                state,
                &all_projects,
                repo_id,
                &repo.path,
                project,
                request,
                paths,
            )
            .await
            {
                Ok(()) => {
                    row["committed"] = json!(true);
                    any_committed = true;
                    if repo.kind == crate::multi_repo::RepoKind::Submodule {
                        add_pointer_to_parent(set, repo, &mut picked);
                    }
                }
                Err(message) => error = Some(message),
            }
        }
        if sends && error.is_none() {
            let path = std::path::PathBuf::from(&repo.path);
            let has_work = tokio::task::spawn_blocking(move || repo_has_unpushed_work(&path))
                .await
                .unwrap_or(false);
            if has_work {
                if paths.is_none() {
                    tried += 1;
                }
                let pushed = if flags.sync {
                    sync_current_branch(&all_projects, repo_id).await
                } else {
                    push_current_branch(&all_projects, repo_id).await
                };
                match pushed {
                    Ok(()) => {
                        row["pushed"] = json!(true);
                        any_pushed = true;
                    }
                    Err(push_error) => error = Some(push_error.message),
                }
                if flags.pull_request && error.is_none() {
                    let mut pr_params = Map::new();
                    pr_params.insert("projectId".to_string(), json!(repo_id));
                    match create_pull_request_for_project(&pr_params, all_projects.clone()).await {
                        Ok(result) if is_confirmed_open_pull_request(&result) => {
                            let pr = result.get("pr").cloned().unwrap_or(Value::Null);
                            if first_pull_request.is_null() {
                                first_pull_request = pr.clone();
                            }
                            row["pullRequest"] = pr;
                        }
                        Ok(_) => {
                            error = Some(
                                "GitHub CLI could not create or find an open pull request."
                                    .to_string(),
                            )
                        }
                        Err(pr_error) => error = Some(pr_error.message),
                    }
                }
            }
        }
        if let Some(message) = error {
            failed += 1;
            row["error"] = json!(message);
        }
        if row["committed"] == json!(true)
            || row["pushed"] == json!(true)
            || !row["error"].is_null()
        {
            results.push(row);
        }
    }
    // Back in the project's order for the list the client shows.
    results.sort_by_key(|row| {
        set.repos
            .iter()
            .position(|repo| Some(repo.rel.as_str()) == row.get("rel").and_then(Value::as_str))
            .unwrap_or(usize::MAX)
    });
    // Even when every repo failed the answer is the list, so the client shows each repo's
    // reason with Try again (Q5276); `failed` says how many.
    if commit.is_none() && sends && tried == 0 {
        return Err(failure("No repo has commits to push."));
    }
    Ok(json!({
        "committed": any_committed,
        "pushed": any_pushed,
        "pullRequest": first_pull_request,
        "multiRepo": true,
        "failed": failed,
        "repos": results,
    }))
}
