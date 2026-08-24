use super::*;

/*
CDXC:WorktreeRename 2026-08-09-18:40:
Renaming a worktree is a multi-step git + database mutation, so it lives behind
ONE endpoint for exactly the reason `deleteWorktreeProject` does: the rollback
must not depend on a renderer that may not survive the operation. The order is
pre-flight, branch, folder, database, delta — every step checked, and everything
reversible until the folder actually moves. If the branch rename lands and the
move then fails, this rolls the branch back before returning, so a failed rename
leaves nothing half-applied.

The caller sends a NAME, never a path. The daemon slugs it into
`<ParentFolder>-<slug>` next to the parent checkout itself, which is what keeps a
renderer from pointing the move at a directory the user never named, and what
keeps the typed operation's family-root guard meaningful.
*/
#[derive(Clone)]
pub(crate) struct RenameWorktreeProjectParams {
    pub(crate) name: String,
    pub(crate) project_id: String,
    pub(crate) rename_branch: bool,
}

pub(crate) struct RenameWorktreeProjectPlan {
    pub(crate) destination_path: String,
    pub(crate) moves_folder: bool,
    pub(crate) params: RenameWorktreeProjectParams,
    pub(crate) parent_path: String,
    pub(crate) parent_project_name: String,
    pub(crate) projects: Vec<Value>,
    pub(crate) worktree_branch: Option<String>,
    pub(crate) worktree_path: String,
    /*
    CDXC:WorktreeRename 2026-08-10:
    The same folder can be spelled two ways on macOS — `/tmp/rt-old` and
    `/private/tmp/rt-old` — and nothing forces a session's stored `cwd` to use
    the spelling the project row happens to carry. Rebasing compares strings, so
    a session recorded through the other spelling matched no prefix, kept an
    absolute path into a folder that had just moved, and broke on its next cold
    start (`start_session_provider` bakes the cwd into the run script).

    Resolve the alias here, at plan time, because it is the last moment the old
    folder still exists to resolve. `None` when it cannot be resolved or adds
    nothing, so the lexical comparison stays the only one that runs.
    */
    pub(crate) worktree_path_resolved: Option<String>,
}

pub(crate) enum RenameWorktreeProjectError {
    Domain(DomainStateError),
    ProjectPath(ProjectPathHttpError),
    Typed(TypedOperationError),
}

impl From<DomainStateError> for RenameWorktreeProjectError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ProjectPathHttpError> for RenameWorktreeProjectError {
    fn from(error: ProjectPathHttpError) -> Self {
        Self::ProjectPath(error)
    }
}

impl From<TypedOperationError> for RenameWorktreeProjectError {
    fn from(error: TypedOperationError) -> Self {
        Self::Typed(error)
    }
}

pub(crate) async fn handle_rename_worktree_project_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match prepare_rename_worktree_project_plan(state, &params) {
        Ok(plan) => rename_worktree_project_from_plan(state, plan).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => rename_worktree_project_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn normalize_rename_worktree_project_params(
    params: &Map<String, Value>,
) -> std::result::Result<RenameWorktreeProjectParams, DomainStateError> {
    let project_id = read_project_id(params)?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if let Some(error) = worktree_rename_name_error(&name) {
        return Err(DomainStateError::bad_request(error));
    }
    Ok(RenameWorktreeProjectParams {
        name,
        project_id,
        rename_branch: params.get("renameBranch").and_then(Value::as_bool) == Some(true),
    })
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
The daemon re-validates the typed name with the same nine rules the sidebar field
enforces (`packages/shared/worktree-rename-name.ts`), because the field is a courtesy and
this is the boundary. Rules 1-6 are gxserver's existing ref policy; 7-9 are the
shapes git refuses that the character allowlist alone would let through.
*/
pub(crate) fn worktree_rename_name_error(name: &str) -> Option<&'static str> {
    const CHARACTER_ERROR: &str =
        "Use letters, numbers, and . _ / - only, starting with a letter or number.";
    const SEPARATOR_ERROR: &str = "Names cannot contain \"..\", \"//\", or end with \"/\".";
    if name.chars().count() > 200 {
        return Some("Name is too long (200 characters max).");
    }
    if !name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphanumeric())
        || !name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        || name.ends_with('.')
        || name
            .split('/')
            .any(|component| component.starts_with('.') || component.ends_with(".lock"))
    {
        return Some(CHARACTER_ERROR);
    }
    if name.contains("..") || name.contains("//") || name.ends_with('/') {
        return Some(SEPARATOR_ERROR);
    }
    None
}

pub(crate) fn prepare_rename_worktree_project_plan(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<RenameWorktreeProjectPlan, RenameWorktreeProjectError> {
    let params = normalize_rename_worktree_project_params(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let worktree_project = repository.get_project(&params.project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {} does not exist.", params.project_id))
    })?;
    let worktree_project_path = worktree_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Worktree project has no filesystem path."))?;
    let worktree = normalize_worktree_metadata(worktree_project.get("worktree"))
        .ok_or_else(|| DomainStateError::bad_request("Only worktree projects can be renamed."))?;
    let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
    let parent_project_path = parent_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    let parent_project_name = parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let worktree_path_value = Value::String(worktree_project_path.to_string());
    let parent_path_value = Value::String(parent_project_path.to_string());
    let worktree_path = normalize_existing_directory_path(
        Some(&worktree_path_value),
        "project.path",
        &state.paths.home_dir,
    )?;
    let parent_path = normalize_existing_directory_path(
        Some(&parent_path_value),
        "parentProject.path",
        &state.paths.home_dir,
    )?;

    /*
    CDXC:WorktreeRename 2026-08-09-18:40:
    The typed operation scopes every worktree command to "inside the parent's
    family directory" and compares paths LEXICALLY, by design — it collapses
    `.`/`..` but deliberately does not resolve symlinks. So two rows that
    describe the same folder through different symlink forms fail that guard
    with a sentence about `worktreePath`, which means nothing to whoever just
    clicked Rename. It happens for real on macOS: register a project as
    `/tmp/rt` and its worktree resolves to `/private/tmp/rt-old`, because
    `git worktree list` reports the resolved path while the project kept the
    typed one.

    Catch it here, where both paths are known, and say which two disagree. The
    typed-operation guard stays the backstop; this only replaces the message.
    */
    let family_root = Path::new(&parent_path)
        .parent()
        .map(path_to_string)
        .unwrap_or_else(|| parent_path.clone());
    if !worktree_path.starts_with(&format!("{family_root}/")) {
        return Err(DomainStateError::bad_request(
            "This worktree and its project are registered under different paths, so Ghostex cannot tell they are siblings. This usually means one path goes through a symlink. Re-add the project using the same path form as the worktree.",
        )
        .into());
    }

    let destination_path = worktree_rename_destination_path(&parent_path, &params.name)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Use letters, numbers, and . _ / - only, starting with a letter or number.",
            )
        })?;
    let moves_folder = destination_path != worktree_path;
    if !moves_folder && !params.rename_branch {
        return Err(DomainStateError::bad_request("Nothing to rename.").into());
    }
    if moves_folder {
        if destination_path == parent_path {
            return Err(DomainStateError::bad_request(
                "That name would collide with the main checkout.",
            )
            .into());
        }
        if Path::new(&destination_path).exists() {
            return Err(DomainStateError::bad_request(format!(
                "A folder named \"{}\" already exists next to the project.",
                path_file_name_for_rename(&destination_path)
            ))
            .into());
        }
        if projects.iter().any(|project| {
            project.get("projectId").and_then(Value::as_str) != Some(params.project_id.as_str())
                && project
                    .get("path")
                    .and_then(Value::as_str)
                    .map(|path| {
                        path_to_string(&resolve_path_syntax(PathBuf::from(path)))
                            == destination_path
                    })
                    .unwrap_or(false)
        }) {
            return Err(DomainStateError::bad_request(
                "Another project is already registered at that folder.",
            )
            .into());
        }
        /*
        CDXC:WorktreeRename 2026-08-10:
        These two probes exist to refuse early, with a sentence the user can act
        on, and they are not authoritative. `worktree_is_locked` matches
        `worktree_path` against what `git worktree list --porcelain` prints, and
        git prints the symlink-resolved path while `normalize_existing_directory_path`
        deliberately does not resolve one — a worktree registered through an
        alias that still sits lexically inside the family root passes the guard
        above and then reads as "not locked" here. A submodule can also be
        initialised between this check and the move. `move_worktree_project_checkout`
        re-classifies both refusals off git's own output and produces the same
        sentences, so it, not this, is what actually enforces them.
        */
        if worktree_sessions::worktree_has_populated_submodules(&worktree_path) {
            return Err(DomainStateError::bad_request(
                "This worktree has initialised submodules, and git cannot move those. Remove them (git submodule deinit --all) or move the folder yourself.",
            )
            .into());
        }
        if worktree_sessions::worktree_is_locked(&parent_path, &worktree_path) {
            return Err(DomainStateError::bad_request(
                "This worktree is locked. Unlock it before renaming.",
            )
            .into());
        }
    }

    let worktree_path_resolved = std::fs::canonicalize(&worktree_path)
        .ok()
        .map(|path| path_to_string(&path))
        .filter(|resolved| resolved != &worktree_path);

    Ok(RenameWorktreeProjectPlan {
        destination_path,
        moves_folder,
        params,
        parent_path,
        parent_project_name,
        projects,
        worktree_branch: worktree.branch,
        worktree_path,
        worktree_path_resolved,
    })
}

/// `<dirname(parent)>/<basename(parent)>-<slug(name)>`. Worktrees stay siblings
/// of the parent checkout because the typed operation's family-root guard is
/// written in those terms; a destination anywhere else could not pass it.
pub(crate) fn worktree_rename_destination_path(parent_path: &str, name: &str) -> Option<String> {
    let slug = worktree_sessions::worktree_rename_folder_slug(name);
    if slug.is_empty() {
        return None;
    }
    let parent = Path::new(parent_path);
    let folder = parent.file_name()?.to_string_lossy().to_string();
    let family_root = parent.parent()?;
    Some(path_to_string(
        &family_root.join(format!("{folder}-{slug}")),
    ))
}

pub(crate) fn path_file_name_for_rename(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

pub(crate) async fn rename_worktree_project_from_plan(
    state: &AppState,
    plan: RenameWorktreeProjectPlan,
) -> std::result::Result<Value, RenameWorktreeProjectError> {
    let current_branch = resolve_renamed_worktree_branch_name(&plan).await?;
    /*
    CDXC:WorktreeRename 2026-08-10:
    "Nothing to rename" cannot be decided from the checkbox alone. Asking to
    rename the branch to the name it already has, on a folder that is already
    correct, is a request to change nothing — and reporting success for that
    tells the user something happened when it did not. The plan-time check
    catches the checkbox-off case; this catches the one that needs git.
    */
    if !plan.moves_folder
        && current_branch
            .as_deref()
            .is_some_and(|branch| branch == plan.params.name)
    {
        return Err(DomainStateError::bad_request("Nothing to rename.").into());
    }
    let renamed_branch = rename_worktree_project_branch(&plan, current_branch.as_deref()).await?;
    if plan.moves_folder {
        if let Err(error) = move_worktree_project_checkout(&plan).await {
            /*
            CDXC:WorktreeRename 2026-08-09-18:40:
            The branch rename is the only step that already landed, and it is
            trivially reversible, so undo it before reporting the move failure.
            A user who sees "could not rename" must not then discover their
            branch was renamed anyway.
            */
            if let (Some(from), Some(to)) = (current_branch.as_deref(), renamed_branch.as_deref()) {
                /*
                CDXC:WorktreeRename 2026-08-10:
                The undo is the only thing standing between the user and the
                state the comment above forbids, so a failed undo is exactly the
                case worth recording. Dropping it left the branch renamed, the
                request reporting the move error, and nothing anywhere saying
                the two had diverged. The user still gets the move error — the
                rename genuinely did not happen — but support can now see it.
                */
                let rollback = run_rename_worktree_action(
                    &plan.projects,
                    "renameBranch",
                    &plan.parent_path,
                    rename_branch_params(to, from),
                )
                .await;
                let rollback_failure = match &rollback {
                    Err(_) => Some("branch rollback dispatch failed".to_string()),
                    Ok(result) if exit_code(result) != 0 => {
                        Some(format!("git exited {}", exit_code(result)))
                    }
                    Ok(_) => None,
                };
                if let Some(reason) = rollback_failure {
                    let _ = state.logger.log(GxserverLogInput {
                        level: LogLevel::Warn,
                        event: "worktreeRenameBranchRollbackFailed".to_string(),
                        server_id: Some(state.metadata.server_id.clone()),
                        request_id: None,
                        client: None,
                        duration_ms: None,
                        error: Some(reason),
                        details: Some(json!({ "projectId": plan.params.project_id })),
                    });
                }
            }
            return Err(error);
        }
    }
    let project = apply_renamed_worktree_project_state(state, &plan, renamed_branch.as_deref())?;
    Ok(json!({
        "movedFolder": plan.moves_folder,
        "project": project,
        "renamedBranch": renamed_branch,
    }))
}

pub(crate) async fn resolve_renamed_worktree_branch_name(
    plan: &RenameWorktreeProjectPlan,
) -> std::result::Result<Option<String>, RenameWorktreeProjectError> {
    let mut extra = Map::new();
    extra.insert("action".to_string(), Value::String("branch".to_string()));
    extra.insert(
        "projectPath".to_string(),
        Value::String(plan.worktree_path.clone()),
    );
    let branch =
        dispatch_typed_operation_endpoint("/api/runGitAction", &extra, plan.projects.clone())
            .await?;
    if exit_code(&branch) == 0 {
        if let Some(branch_name) = branch
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name)
        {
            return Ok(Some(branch_name));
        }
    }
    Ok(plan.worktree_branch.clone())
}

pub(crate) async fn rename_worktree_project_branch(
    plan: &RenameWorktreeProjectPlan,
    current_branch: Option<&str>,
) -> std::result::Result<Option<String>, RenameWorktreeProjectError> {
    if !plan.params.rename_branch {
        return Ok(None);
    }
    let Some(current_branch) = current_branch else {
        return Err(DomainStateError::bad_request(
            "This worktree has no branch checked out, so there is no branch to rename.",
        )
        .into());
    };
    if current_branch == plan.params.name {
        return Ok(None);
    }
    if worktree_sessions::worktree_branch_exists(&plan.parent_path, &plan.params.name) {
        return Err(DomainStateError::bad_request(format!(
            "Branch \"{}\" already exists.",
            plan.params.name
        ))
        .into());
    }
    let result = run_rename_worktree_action(
        &plan.projects,
        "renameBranch",
        &plan.parent_path,
        rename_branch_params(current_branch, &plan.params.name),
    )
    .await?;
    if exit_code(&result) != 0 {
        return Err(DomainStateError::bad_request("Could not rename the branch.").into());
    }
    Ok(Some(plan.params.name.clone()))
}

pub(crate) fn rename_branch_params(branch: &str, new_branch: &str) -> Map<String, Value> {
    let mut params = Map::new();
    params.insert("branch".to_string(), Value::String(branch.to_string()));
    params.insert(
        "newBranch".to_string(),
        Value::String(new_branch.to_string()),
    );
    params
}

pub(crate) async fn move_worktree_project_checkout(
    plan: &RenameWorktreeProjectPlan,
) -> std::result::Result<(), RenameWorktreeProjectError> {
    let mut extra = Map::new();
    extra.insert(
        "worktreePath".to_string(),
        Value::String(plan.worktree_path.clone()),
    );
    extra.insert(
        "destinationPath".to_string(),
        Value::String(plan.destination_path.clone()),
    );
    let result =
        run_rename_worktree_action(&plan.projects, "move", &plan.parent_path, extra).await?;
    if exit_code(&result) == 0 {
        return Ok(());
    }
    /*
    CDXC:WorktreeRename 2026-08-09-18:40:
    git's stderr never reaches the user (typed-operation results are bounded by
    contract), so translate the two refusals that are actionable and fall back to
    a plain sentence for everything else. Submodules are re-checked here as well
    as in the pre-flight because one can be initialised between the two.
    */
    if is_submodule_worktree_refusal(&result) {
        return Err(DomainStateError::bad_request(
            "This worktree has initialised submodules, and git cannot move those. Remove them (git submodule deinit --all) or move the folder yourself.",
        )
        .into());
    }
    if is_locked_worktree_refusal(&result) {
        return Err(DomainStateError::bad_request(
            "This worktree is locked. Unlock it before renaming.",
        )
        .into());
    }
    Err(DomainStateError::bad_request("Could not move the worktree folder.").into())
}

pub(crate) async fn run_rename_worktree_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, RenameWorktreeProjectError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) fn is_locked_worktree_refusal(result: &Value) -> bool {
    let text = format!(
        "{}\n{}",
        result
            .get("stderr")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        result
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default()
    );
    text.to_lowercase()
        .contains("cannot move a locked working tree")
}

/*
CDXC:WorktreeRename 2026-08-09-18:40:
Everything that mirrors the worktree's location has to move with it in one pass,
because the alternative is a sidebar row pointing at a folder that no longer
exists — worse than not having the feature. The full list, each traced rather
than guessed:

- `projects.path`, through `relocate_project` so the destination is validated and
  a collision with another registered project is still refused;
- `projects.name`, the derived `<ParentProjectName>-<slug>` label;
- `projects.worktreeJson`, re-detected from git at the new path (its `name` and
  `branch` are stamped once at registration and never recomputed otherwise);
- every `sessions.cwd` under the old folder — V2 worktree sessions store an
  absolute cwd, and `start_session_provider` bakes that cwd into the generated
  run script, so a stale one breaks the next cold start;
- the V2 worktree session marker's `path`, which is the only authority the
  branch auto-rename trusts and silently no-ops when it points nowhere;
- `projectBoardConfig.beadsDirectory`, but only when it is an absolute path
  inside the worktree; it defaults to the project path, which the relocate fixes.

Caches are deliberately absent: the git-status cache, the worktree-topology probe
(60s TTL), and the project-icon cache are all keyed by path and self-heal.
*/
pub(crate) fn apply_renamed_worktree_project_state(
    state: &AppState,
    plan: &RenameWorktreeProjectPlan,
    renamed_branch: Option<&str>,
) -> std::result::Result<Value, RenameWorktreeProjectError> {
    /*
    The busy handler is the point of this entry point: the transaction below
    reserves the writer, and the daemon has other writers (lifecycle sweeps,
    session updates). Without lock waiting, a concurrent write would fail this
    one outright — after the folder has already moved, which is the one moment
    there is nothing to retry.
    */
    let db = open_gxserver_database_with_busy_timeout(&state.paths, Duration::from_secs(10))
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
    let project_id = plan.params.project_id.as_str();
    /*
    CDXC:WorktreeRename 2026-08-10:
    The folder has already moved by the time any of this runs, so these rows are
    the only remaining description of where it went — and the list above only has
    value whole. Written one statement at a time, a session write that failed
    halfway left the project row pointing at the new folder while the sessions
    behind it still named the old one: a state no later request can tell apart
    from a rename that worked. One transaction makes the failure mean "the
    database still describes the old folder", which the returned error then says.
    Following `create_session_transactional`: IMMEDIATE so the writer reservation
    is held before the first read the writes depend on.
    */
    let transaction =
        rusqlite::Transaction::new_unchecked(&db, rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            })?;
    let project = {
        let repository = DomainRepository::new(&transaction, state.metadata.server_id.as_str());
        write_renamed_worktree_project_state(&repository, plan, renamed_branch)
            .map_err(|error| unrecorded_worktree_rename_error(plan, error))?
    };
    transaction
        .commit()
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })
        .map_err(|error| unrecorded_worktree_rename_error(plan, error.into()))?;

    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")?;
    Ok(project)
}

/// The transaction rolled back, so the database still describes the old folder
/// while the filesystem no longer has one there. Say both halves: without the
/// paths, "SQLite error" gives nobody enough to put the two back in step.
pub(crate) fn unrecorded_worktree_rename_error(
    plan: &RenameWorktreeProjectPlan,
    error: RenameWorktreeProjectError,
) -> RenameWorktreeProjectError {
    if !plan.moves_folder {
        return error;
    }
    let RenameWorktreeProjectError::Domain(domain) = error else {
        return error;
    };
    DomainStateError {
        code: domain.code,
        message: format!(
            "The worktree folder moved to \"{}\", but Ghostex could not record it and still points at \"{}\". Nothing else was changed. ({})",
            plan.destination_path, plan.worktree_path, domain.message
        ),
    }
    .into()
}

pub(crate) fn write_renamed_worktree_project_state(
    repository: &DomainRepository<'_>,
    plan: &RenameWorktreeProjectPlan,
    renamed_branch: Option<&str>,
) -> std::result::Result<Value, RenameWorktreeProjectError> {
    let project_id = plan.params.project_id.as_str();

    if plan.moves_folder {
        let mut relocate = Map::new();
        relocate.insert("projectId".to_string(), json!(project_id));
        relocate.insert("path".to_string(), json!(plan.destination_path.clone()));
        repository.relocate_project(&relocate)?;
    }

    let projects = repository.list_projects()?;
    let project_name = renamed_worktree_project_name(plan);
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("name".to_string(), json!(project_name.clone()));
    if let Some(worktree) = crate::domain::detect_registered_git_worktree_metadata(
        &projects,
        &plan.destination_path,
        &project_name,
    ) {
        let mut worktree = worktree;
        /*
        The worktree's `createdAt` records when the checkout was made, so keep
        the registered value instead of stamping the rename as a creation.
        */
        if let Some(created_at) = projects
            .iter()
            .find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(project_id)
            })
            .and_then(|candidate| candidate.get("worktree"))
            .and_then(Value::as_object)
            .and_then(|current| current.get("createdAt"))
            .cloned()
        {
            worktree.insert("createdAt".to_string(), created_at);
        }
        update.insert("worktree".to_string(), Value::Object(worktree));
    }
    if let Some(board_config) = renamed_worktree_board_config(&projects, plan) {
        update.insert(
            "projectBoardConfig".to_string(),
            Value::Object(board_config),
        );
    }
    let project = repository.update_project(&update)?;

    if plan.moves_folder {
        for session in repository.list_sessions(Some(project_id))? {
            let Some(session_id) = session.get("sessionId").and_then(Value::as_str) else {
                continue;
            };
            let mut session_update = Map::new();
            let moved_cwd = session
                .get("cwd")
                .and_then(Value::as_str)
                .and_then(|cwd| rebase_renamed_worktree_path(cwd, plan));
            if let Some(cwd) = moved_cwd {
                session_update.insert("cwd".to_string(), json!(cwd));
            }
            if let Some(runtime_settings) =
                worktree_sessions::runtime_settings_with_moved_worktree_path(
                    &session,
                    &plan.destination_path,
                    renamed_branch,
                )
            {
                session_update.insert(
                    "runtimeSettings".to_string(),
                    Value::Object(runtime_settings),
                );
            }
            if session_update.is_empty() {
                continue;
            }
            session_update.insert("projectId".to_string(), json!(project_id));
            session_update.insert("sessionId".to_string(), json!(session_id));
            repository.update_session(&session_update)?;
        }
    } else if let Some(branch) = renamed_branch {
        for session in repository.list_sessions(Some(project_id))? {
            let Some(session_id) = session.get("sessionId").and_then(Value::as_str) else {
                continue;
            };
            let Some(runtime_settings) =
                worktree_sessions::runtime_settings_with_moved_worktree_path(
                    &session,
                    &plan.worktree_path,
                    Some(branch),
                )
            else {
                continue;
            };
            let mut session_update = Map::new();
            session_update.insert("projectId".to_string(), json!(project_id));
            session_update.insert("sessionId".to_string(), json!(session_id));
            session_update.insert(
                "runtimeSettings".to_string(),
                Value::Object(runtime_settings),
            );
            repository.update_session(&session_update)?;
        }
    }

    Ok(project)
}

pub(crate) fn renamed_worktree_project_name(plan: &RenameWorktreeProjectPlan) -> String {
    let suffix = path_file_name_for_rename(&plan.destination_path);
    let parent_folder = path_file_name_for_rename(&plan.parent_path);
    let slug = suffix
        .strip_prefix(&format!("{parent_folder}-"))
        .unwrap_or(&suffix);
    let parent_label = if plan.parent_project_name.trim().is_empty() {
        parent_folder.as_str()
    } else {
        plan.parent_project_name.trim()
    };
    format!("{parent_label}-{slug}")
}

pub(crate) fn renamed_worktree_board_config(
    projects: &[Value],
    plan: &RenameWorktreeProjectPlan,
) -> Option<Map<String, Value>> {
    if !plan.moves_folder {
        return None;
    }
    let mut board_config = projects
        .iter()
        .find(|candidate| {
            candidate.get("projectId").and_then(Value::as_str)
                == Some(plan.params.project_id.as_str())
        })?
        .get("projectBoardConfig")
        .and_then(Value::as_object)
        .cloned()?;
    let directory = board_config.get("beadsDirectory").and_then(Value::as_str)?;
    let moved = rebase_renamed_worktree_path(directory, plan)?;
    board_config.insert("beadsDirectory".to_string(), json!(moved));
    Some(board_config)
}

/// `<old worktree>/x` becomes `<new worktree>/x`; anything outside the moved
/// folder is left exactly as it is. The old folder is recognised through either
/// spelling it had before the move — see `worktree_path_resolved`.
pub(crate) fn rebase_renamed_worktree_path(
    path: &str,
    plan: &RenameWorktreeProjectPlan,
) -> Option<String> {
    let normalized = path_to_string(&resolve_path_syntax(PathBuf::from(path)));
    let roots =
        std::iter::once(plan.worktree_path.as_str()).chain(plan.worktree_path_resolved.as_deref());
    for root in roots {
        if normalized == root {
            return Some(plan.destination_path.clone());
        }
        if let Some(rest) = normalized.strip_prefix(&format!("{root}/")) {
            return Some(format!("{}/{rest}", plan.destination_path));
        }
    }
    None
}

pub(crate) fn rename_worktree_project_error_response(
    endpoint_path: String,
    request_id: String,
    error: RenameWorktreeProjectError,
) -> RoutedResponse {
    match error {
        RenameWorktreeProjectError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        RenameWorktreeProjectError::ProjectPath(error) => {
            project_path_error_response(endpoint_path, request_id, error)
        }
        RenameWorktreeProjectError::Typed(error) => {
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}
