//! Worktree sessions of a project that spans several repos (multi_repo/mod.rs): the tray's
//! answer with the repos to pick, the draft move into a new session folder, and
//! `createWorktreeSession`'s checkout (coordinator threads), all making one session folder with
//! a worktree of each picked repo on one branch.

use super::*;

use std::path::{Path, PathBuf};

use crate::multi_repo::session_folder;
use crate::multi_repo::{RepoKind, RepoSet};

/// The repo whose branches the tray's base-branch menu lists: the project folder's own, else the
/// first one inside it.
fn lead_repo(set: &RepoSet) -> Option<&crate::multi_repo::RepoEntry> {
    set.repos
        .iter()
        .find(|repo| repo.kind == RepoKind::Root)
        .or_else(|| set.repos.iter().find(|repo| !repo.rel.starts_with("..")))
        .or_else(|| set.repos.first())
}

/// The tray's answer for a project with several repos: the usual fields (the base branches are
/// the lead repo's; a repo without the one picked starts from its own default branch) plus
/// `multiRepo` and `repos` (`[{name, rel, kind, picked}]`, every repo picked by default up to
/// eight). Older clients ignore the two new keys and see a plain New worktree.
pub(crate) async fn multi_repo_tray(set: &RepoSet, base: Map<String, Value>) -> Value {
    let mut answer = base;
    let lead = lead_repo(set).map(|repo| repo.path.clone());
    let (branches, current_branch, default_branch) = match lead {
        Some(path) => {
            let lead_path = PathBuf::from(&path);
            tokio::task::spawn_blocking(move || {
                use crate::multi_repo::git;
                let listed = git::read(
                    &lead_path,
                    &[
                        "for-each-ref",
                        "--format=%(refname:short)%09%(refname)%09%(HEAD)",
                        "refs/heads",
                        "refs/remotes",
                    ],
                )
                .map(|stdout| crate::typed_operations::worktree::parse_git_branch_list(&stdout))
                .unwrap_or_default();
                (
                    Value::Array(listed),
                    git::current_branch(&lead_path),
                    git::default_branch(&lead_path),
                )
            })
            .await
            .unwrap_or((Value::Array(Vec::new()), None, None))
        }
        None => (Value::Array(Vec::new()), None, None),
    };
    let picked_by_default = set.repos.len() <= session_folder::DEFAULT_PICK_MAX;
    let repos: Vec<Value> = set
        .repos
        .iter()
        .map(|repo| {
            json!({
                "name": repo.name,
                "rel": repo.rel,
                "kind": repo.kind.as_str(),
                "picked": repo.kind == RepoKind::Root || picked_by_default,
                // The project folder's own repo is always in the worktree.
                "locked": repo.kind == RepoKind::Root,
            })
        })
        .collect();
    answer.insert("available".to_string(), json!(true));
    answer.insert("movable".to_string(), json!(true));
    answer.insert("multiRepo".to_string(), json!(true));
    answer.insert("repos".to_string(), Value::Array(repos));
    answer.insert("branches".to_string(), branches);
    answer.insert("currentBranch".to_string(), json!(current_branch));
    answer.insert("defaultBranch".to_string(), json!(default_branch));
    answer.insert("recentBranches".to_string(), Value::Array(Vec::new()));
    // Session folders are not listed as existing worktrees yet.
    answer.insert("worktrees".to_string(), Value::Array(Vec::new()));
    Value::Object(answer)
}

/// Picks a branch and folder free in every repo, from `names` (tried in order), and makes the
/// session folder. Blocking work runs on the blocking pool.
async fn make_session_folder(
    project_path: &str,
    set: &RepoSet,
    picked: Option<Vec<String>>,
    base: Option<String>,
    registered_paths: HashSet<String>,
    names: Vec<(String, String)>,
) -> std::result::Result<session_folder::Made, ProjectWorktreeOperationError> {
    let project_path = PathBuf::from(project_path);
    let set = set.clone();
    tokio::task::spawn_blocking(move || {
        let plan = session_folder::plan(&project_path, &set, picked.as_deref())
            .map_err(DomainStateError::bad_request)?;
        for (branch, suffix) in names {
            let folder = session_folder::folder_for(&project_path, &suffix);
            let project_dir = crate::multi_repo::join_rel(
                &folder,
                &crate::multi_repo::relative_path(&plan.anchor, &project_path)
                    .filter(|rel| rel != ".")
                    .unwrap_or_default(),
            );
            if registered_paths.contains(&normalize_project_path_for_comparison(&path_to_string(
                &project_dir,
            ))) {
                continue;
            }
            if !session_folder::is_free(&plan, &branch, &folder) {
                continue;
            }
            return session_folder::make(&plan, &folder, &branch, base.as_deref())
                .map_err(DomainStateError::bad_request);
        }
        Err(DomainStateError::bad_request(
            "Could not find a branch name free in every repo.",
        ))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Worktree task failed: {error}"),
    })?
    .map_err(Into::into)
}

/// Removes a session folder a failed step made. Best effort: the step has already failed.
pub(crate) async fn discard_session_folder(root: PathBuf) {
    let _ = tokio::task::spawn_blocking(move || session_folder::remove(&root, true, true)).await;
}

fn registered_paths(context: &ProjectWorktreeOperationContext) -> HashSet<String> {
    context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .collect()
}

/// The worktree metadata a session folder's project carries: the usual keys, so it folds under
/// its parent in the sidebar, plus `multiRepo`.
fn session_folder_metadata(
    context: &ProjectWorktreeOperationContext,
    branch: &str,
    name: &str,
) -> Map<String, Value> {
    let mut metadata = Map::new();
    metadata.insert("branch".to_string(), json!(branch));
    metadata.insert("createdAt".to_string(), json!(crate::domain::now_iso()));
    metadata.insert("name".to_string(), json!(name));
    metadata.insert(
        "parentProjectId".to_string(),
        json!(context.parent_project_id),
    );
    metadata.insert(
        "parentProjectName".to_string(),
        context
            .parent_project
            .get("name")
            .cloned()
            .unwrap_or(Value::Null),
    );
    metadata.insert("parentProjectPath".to_string(), json!(context.parent_path));
    metadata.insert("multiRepo".to_string(), json!(true));
    metadata
}

/// Registers the session folder's project folder as a worktree project of the source project.
fn register_session_folder_project(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    project_dir: &Path,
    branch: &str,
    name: &str,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let parent_name = context
        .parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let project = register_project_worktree_path(
        state,
        &path_to_string(project_dir),
        &format!("{parent_name}-{name}"),
        "projectAdded",
    )?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project_id = value_text(&project, "projectId")?;
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert(
        "worktree".to_string(),
        Value::Object(session_folder_metadata(context, branch, name)),
    );
    let project = repository.update_project(&update)?;
    schedule_presentation_project_delta(state, &db, &repository, &project_id, "projectUpdated")?;
    Ok(project)
}

/// The draft move's New worktree for a project with several repos: a session folder named from
/// the first message, registered as a worktree project, the setup command run in it.
pub(crate) async fn create_multi_repo_for_move(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &DraftMoveRequest,
    made: &mut MadeForMove,
    set: &RepoSet,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let hint = crate::worktree_branch_name::worktree_name_hint(
        &request.text,
        &crate::worktree_branch_name::ReferenceScope {
            project_id: &context.source_project_id,
            cwd: &context.source_path,
        },
    );
    let hint = normalize_project_slug_label(&hint, "nameHint")?;
    let names: Vec<(String, String)> = (0..50)
        .map(|index| {
            let name = if index == 0 {
                hint.clone()
            } else {
                format!("{hint}-{}", index + 1)
            };
            (name.clone(), name)
        })
        .collect();
    let session = make_session_folder(
        &context.source_path,
        set,
        request.repos.clone(),
        request.base_branch.clone(),
        registered_paths(context),
        names,
    )
    .await?;
    made.session_folder = Some(session.root.clone());
    let name = session.branch.clone();
    let project = register_session_folder_project(
        state,
        context,
        &session.project_dir,
        &session.branch,
        &name,
    )?;
    made.registered_project_id = Some(value_text(&project, "projectId")?);
    let projects = list_domain_projects(state)?;
    run_registered_worktree_setup_command(
        &projects,
        &value_text(&project, "projectId")?,
        &context.source_project_id,
    )
    .await?;
    Ok(project)
}

/// `createWorktreeSession`'s new checkout for a project with several repos: a session folder on
/// `ghostex/<8 hex>`, today's temporary name, which the rename pass later renames in every repo.
pub(crate) async fn prepare_multi_repo_session_checkout(
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    set: &RepoSet,
) -> std::result::Result<PreparedWorktreeCheckout, ProjectWorktreeOperationError> {
    let names: Vec<(String, String)> = match request.branch.as_deref() {
        // A branch the caller chose (a coordinator thread's `--branch`), suffixed when taken.
        Some(branch) => {
            let slug: String = branch
                .rsplit('/')
                .next()
                .unwrap_or(branch)
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' {
                        ch
                    } else {
                        '-'
                    }
                })
                .collect();
            (0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS)
                .map(|index| {
                    if index == 0 {
                        (branch.to_string(), slug.clone())
                    } else {
                        (
                            format!("{branch}-{}", index + 1),
                            format!("{slug}-{}", index + 1),
                        )
                    }
                })
                .collect()
        }
        None => (0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS)
            .map(|_| {
                let suffix = worktree_sessions::create_temp_branch_suffix();
                (worktree_sessions::temp_branch_name(&suffix), suffix)
            })
            .collect(),
    };
    let session = make_session_folder(
        &context.source_path,
        set,
        request.repos.clone(),
        request.base_branch.clone(),
        registered_paths(context),
        names,
    )
    .await?;
    let prepared = PreparedWorktreeCheckout {
        branch: session.branch.clone(),
        created: true,
        path: path_to_string(&session.project_dir),
    };
    if let Err(error) = run_worktree_session_setup_command(context, &prepared.path).await {
        discard_session_folder(session.root.clone()).await;
        return Err(error);
    }
    Ok(prepared)
}

/// `createWorktreeSession`'s session folder registered as a worktree project of the source
/// project, as the draft move's is, so the session's header, Changes and git menu read the session
/// folder's repos and not the parent's. `None` when the checkout is not a session folder this call
/// made. The project is named after the folder (`shop-1b54dbd1`); the branch rename pass later
/// keeps its recorded branch in step (`sync_session_folder_project_branch`).
///
/// CDXC:MultiRepo 2026-11-27 WHY: a coordinator thread's session in a session folder was
/// unregistered, like a one-repo worktree session, so the desktop read the parent project's git
/// state: its Commit saw only the parent's checkout and could not reach the session's repos.
pub(crate) fn register_worktree_session_folder(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    prepared: &PreparedWorktreeCheckout,
) -> std::result::Result<Option<String>, ProjectWorktreeOperationError> {
    if !prepared.created || session_folder_root(&prepared.path).is_none() {
        return Ok(None);
    }
    let name = prepared
        .branch
        .rsplit('/')
        .next()
        .unwrap_or(prepared.branch.as_str())
        .to_string();
    let project = register_session_folder_project(
        state,
        context,
        Path::new(&prepared.path),
        &prepared.branch,
        &name,
    )?;
    Ok(Some(value_text(&project, "projectId")?))
}

/// Takes back a registration `register_worktree_session_folder` made, when the session it was
/// for failed to start. Best effort: the create has already failed.
pub(crate) fn unregister_worktree_session_folder(state: &AppState, project_id: &str) {
    if remove_worktree_project_row(state, project_id).is_ok() {
        let _ = schedule_deleted_worktree_project_delta(state, project_id);
    }
}

/// After the rename pass gave a session folder's branch a real name: the registered project of
/// that folder (if any) records the new branch, so its card and Delete dialog name it.
pub(crate) fn sync_session_folder_project_branch(
    repository: &DomainRepository<'_>,
    project_dir: &str,
    branch: &str,
) -> std::result::Result<Option<String>, DomainStateError> {
    let wanted = normalize_project_path_for_comparison(project_dir);
    for project in repository.list_projects()? {
        let same = project
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| normalize_project_path_for_comparison(path) == wanted);
        let Some(worktree) = project.get("worktree").and_then(Value::as_object) else {
            continue;
        };
        if !same || worktree.get("multiRepo") != Some(&json!(true)) {
            continue;
        }
        let project_id = value_text(&project, "projectId")?;
        let mut worktree = worktree.clone();
        worktree.insert("branch".to_string(), json!(branch));
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(project_id));
        update.insert("worktree".to_string(), Value::Object(worktree));
        repository.update_project(&update)?;
        return Ok(Some(project_id));
    }
    Ok(None)
}

/// The session folder a checkout path sits in, for a rollback.
pub(crate) fn session_folder_root(path: &str) -> Option<PathBuf> {
    crate::multi_repo::record::find(Path::new(path)).map(|(root, _)| root)
}

/// What Merge did in one repo of a session folder.
fn merge_row(name: &str, status: &str, into: Option<&str>, message: Option<String>) -> Value {
    json!({ "name": name, "status": status, "into": into, "message": message })
}

/// Merge for a session folder (Q5277): each repo's branch merges into that repo's own default
/// branch in the project's checkout, deepest repo first (a submodule before the repo that points
/// at it), a repo with no new commits is skipped, and the first conflict or refusal stops there,
/// so the answer lists what merged, what stopped and what was not reached.
pub(crate) async fn merge_session_folder(
    parent_project_id: String,
    record: crate::multi_repo::record::SessionRecord,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    use crate::multi_repo::git;
    tokio::task::spawn_blocking(move || {
        let mut repos = record.repos.clone();
        repos.sort_by_key(|repo| std::cmp::Reverse(repo.rel.split('/').count()));
        let branch = record.branch.clone();
        let has_submodules = record
            .repos
            .iter()
            .any(|repo| repo.kind == RepoKind::Submodule);
        let mut rows: Vec<Value> = Vec::new();
        let mut stopped: Option<(&'static str, String)> = None;
        for repo in &repos {
            if stopped.is_some() {
                rows.push(merge_row(&repo.name, "notReached", None, None));
                continue;
            }
            let source = Path::new(&repo.source);
            let Some(default) = git::default_branch(source) else {
                let message = format!("{}: no default branch found.", repo.name);
                rows.push(merge_row(&repo.name, "failed", None, Some(message.clone())));
                stopped = Some(("stopped", message));
                continue;
            };
            let ahead = git::read(
                source,
                &["rev-list", "--count", &format!("{default}..{branch}")],
            )
            .and_then(|count| count.parse::<u64>().ok())
            .unwrap_or(0);
            if ahead == 0 {
                rows.push(merge_row(&repo.name, "skipped", Some(&default), None));
                continue;
            }
            // A submodule merged a step earlier moved its pointer here; the merge brings the
            // same pointer, so only other changes stop it.
            let ignore = if has_submodules {
                "--ignore-submodules=all"
            } else {
                "--ignore-submodules=dirty"
            };
            let dirty = git::read(source, &["status", "--porcelain", ignore])
                .is_some_and(|status| status.lines().any(|line| !line.trim().is_empty()));
            if dirty {
                let message = format!(
                    "{}: commit or stash the changes in the project folder before merging.",
                    repo.name
                );
                rows.push(merge_row(
                    &repo.name,
                    "failed",
                    Some(&default),
                    Some(message.clone()),
                ));
                stopped = Some(("stopped", message));
                continue;
            }
            if git::current_branch(source).as_deref() != Some(default.as_str()) {
                let checkout = git::run(source, &["checkout", &default], git::GIT_TIMEOUT);
                if !checkout.ok {
                    let message = format!(
                        "{}: {}",
                        repo.name,
                        checkout.reason(&format!("Could not check out {default}."))
                    );
                    rows.push(merge_row(
                        &repo.name,
                        "failed",
                        Some(&default),
                        Some(message.clone()),
                    ));
                    stopped = Some(("stopped", message));
                    continue;
                }
            }
            let merged = git::run(
                source,
                &["merge", "--no-edit", &branch],
                git::COMMIT_TIMEOUT,
            );
            if merged.ok {
                rows.push(merge_row(&repo.name, "merged", Some(&default), None));
            } else {
                let message = format!(
                    "{}: merging into {default} has conflicts to resolve in the project folder.",
                    repo.name
                );
                rows.push(merge_row(
                    &repo.name,
                    "conflicts",
                    Some(&default),
                    Some(message.clone()),
                ));
                stopped = Some(("conflicts", message));
            }
        }
        let (status, message) = match stopped {
            Some((status, message)) => (status, Some(message)),
            None if rows.iter().all(|row| row["status"] == "skipped") => ("nothing", None),
            None => ("merged", None),
        };
        json!({
            "parentProjectId": parent_project_id,
            "status": status,
            "message": message,
            "multiRepo": true,
            "repos": rows,
        })
    })
    .await
    .map_err(|error| {
        DomainStateError {
            code: "internalError",
            message: format!("Merge task failed: {error}"),
        }
        .into()
    })
}
