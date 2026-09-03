use super::*;

#[derive(Clone)]
pub(crate) struct DeleteWorktreeProjectParams {
    delete_local_branch: bool,
    delete_remote_branch: bool,
    project_id: String,
    remote_name: String,
}

#[derive(Clone)]
pub(crate) struct NormalizedWorktreeMetadata {
    pub(crate) branch: Option<String>,
    pub(crate) parent_project_id: String,
    pub(crate) parent_project_path: Option<String>,
}

pub(crate) struct DeleteWorktreeProjectPlan {
    params: DeleteWorktreeProjectParams,
    parent_path: String,
    projects: Vec<Value>,
    worktree_branch: Option<String>,
    worktree_path: String,
}

pub(crate) enum DeleteWorktreeProjectError {
    Domain(DomainStateError),
    ProjectPath(ProjectPathHttpError),
    Typed(TypedOperationError),
}

impl From<DomainStateError> for DeleteWorktreeProjectError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ProjectPathHttpError> for DeleteWorktreeProjectError {
    fn from(error: ProjectPathHttpError) -> Self {
        Self::ProjectPath(error)
    }
}

impl From<TypedOperationError> for DeleteWorktreeProjectError {
    fn from(error: TypedOperationError) -> Self {
        Self::Typed(error)
    }
}

/*
CDXC:Worktrees 2026-06-22-08:47:
Rust gxserver must own the same shared Delete Worktree workflow as TypeScript: validate the selected worktree project, remove the Git checkout from the registered parent, delete the durable project row before optional branch cleanup, return cleanup failures as warnings, and publish the presentation delta after the canonical row is gone.
*/
pub(crate) async fn handle_delete_worktree_project_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match prepare_delete_worktree_project_plan(state, &params) {
        Ok(plan) => delete_worktree_project_from_plan(state, plan).await,
        Err(error) => Err(error),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => delete_worktree_project_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) fn prepare_delete_worktree_project_plan(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<DeleteWorktreeProjectPlan, DeleteWorktreeProjectError> {
    let params = normalize_delete_worktree_project_params(params)?;
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
        .ok_or_else(|| DomainStateError::bad_request("Project is not a worktree."))?;
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
    Ok(DeleteWorktreeProjectPlan {
        params,
        parent_path,
        projects,
        worktree_branch: worktree.branch,
        worktree_path,
    })
}

pub(crate) async fn delete_worktree_project_from_plan(
    state: &AppState,
    plan: DeleteWorktreeProjectPlan,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let branch_name = if plan.params.delete_local_branch || plan.params.delete_remote_branch {
        resolve_current_worktree_branch_name(&plan).await?
    } else {
        None
    };
    let checkout_removal = remove_worktree_checkout(&plan).await?;
    let project = remove_worktree_project_row(state, &plan.params.project_id)?;
    let mut warnings = delete_selected_worktree_branches(&plan, branch_name).await?;
    let prune =
        run_delete_worktree_action(&plan.projects, "prune", &plan.parent_path, Map::new()).await?;
    if exit_code(&prune) != 0 {
        warnings.push(json!({
            "kind": "pruneFailed",
            "message": operation_failure_message(&prune, "git worktree prune failed."),
        }));
    }
    schedule_deleted_worktree_project_delta(state, &plan.params.project_id)?;
    Ok(json!({
        "checkoutRemoval": checkout_removal,
        "project": project,
        "warnings": warnings,
    }))
}

pub(crate) fn normalize_delete_worktree_project_params(
    params: &Map<String, Value>,
) -> std::result::Result<DeleteWorktreeProjectParams, DomainStateError> {
    let project_id = read_project_id(params)?;
    let remote_name = params
        .get("remoteName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("origin")
        .to_string();
    if !is_allowed_git_remote_name(&remote_name) {
        return Err(DomainStateError::bad_request(
            "remoteName is not an allowed Git remote name.",
        ));
    }
    Ok(DeleteWorktreeProjectParams {
        delete_local_branch: params.get("deleteLocalBranch").and_then(Value::as_bool) == Some(true),
        delete_remote_branch: params.get("deleteRemoteBranch").and_then(Value::as_bool)
            == Some(true),
        project_id,
        remote_name,
    })
}

pub(crate) fn normalize_worktree_metadata(
    candidate: Option<&Value>,
) -> Option<NormalizedWorktreeMetadata> {
    let worktree = candidate.and_then(Value::as_object)?;
    let parent_project_id = worktree.get("parentProjectId").and_then(Value::as_str)?;
    if !is_gxserver_project_id(parent_project_id) {
        return None;
    }
    Some(NormalizedWorktreeMetadata {
        branch: worktree
            .get("branch")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name),
        parent_project_id: parent_project_id.to_string(),
        parent_project_path: worktree
            .get("parentProjectPath")
            .and_then(Value::as_str)
            .map(str::trim)
            .map(str::to_string),
    })
}

pub(crate) fn resolve_worktree_parent_project(
    projects: &[Value],
    worktree: &NormalizedWorktreeMetadata,
) -> std::result::Result<Value, DomainStateError> {
    let parent_project = projects
        .iter()
        .find(|project| {
            project.get("projectId").and_then(Value::as_str)
                == Some(worktree.parent_project_id.as_str())
        })
        .cloned()
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    let parent_path = parent_project
        .get("path")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Parent project {} does not exist.",
                worktree.parent_project_id
            ))
        })?;
    if let Some(expected_path) = worktree
        .parent_project_path
        .as_deref()
        .filter(|path| !path.is_empty())
    {
        let expected = path_to_string(&resolve_path_syntax(PathBuf::from(expected_path)));
        let actual = path_to_string(&resolve_path_syntax(PathBuf::from(parent_path)));
        if expected != actual {
            return Err(DomainStateError::bad_request(
                "Worktree parent project path does not match the registered parent project.",
            ));
        }
    }
    Ok(parent_project)
}

pub(crate) async fn resolve_current_worktree_branch_name(
    plan: &DeleteWorktreeProjectPlan,
) -> std::result::Result<Option<String>, DeleteWorktreeProjectError> {
    let branch =
        run_delete_git_action(&plan.projects, "branch", &plan.worktree_path, Map::new()).await?;
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

pub(crate) async fn remove_worktree_checkout(
    plan: &DeleteWorktreeProjectPlan,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let status = run_delete_git_action(
        &plan.projects,
        "statusPorcelain",
        &plan.worktree_path,
        Map::new(),
    )
    .await?;
    if exit_code(&status) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&status, "Could not read worktree status."),
            scope_rejection: false,
        }
        .into());
    }
    let force_initial_remove = has_porcelain_status_changes(
        status
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let mut extra = Map::new();
    extra.insert(
        "worktreePath".to_string(),
        Value::String(plan.worktree_path.clone()),
    );
    if force_initial_remove {
        extra.insert("force".to_string(), Value::Bool(true));
    }
    let mut remove =
        run_delete_worktree_action(&plan.projects, "remove", &plan.parent_path, extra).await?;
    let mut retried_for_submodules = false;
    if exit_code(&remove) != 0 && !force_initial_remove && is_submodule_worktree_refusal(&remove) {
        retried_for_submodules = true;
        let mut retry_extra = Map::new();
        retry_extra.insert(
            "worktreePath".to_string(),
            Value::String(plan.worktree_path.clone()),
        );
        retry_extra.insert("force".to_string(), Value::Bool(true));
        remove =
            run_delete_worktree_action(&plan.projects, "remove", &plan.parent_path, retry_extra)
                .await?;
    }
    if exit_code(&remove) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&remove, "git worktree remove failed."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(json!({
        "forced": force_initial_remove || retried_for_submodules,
        "retriedForSubmodules": retried_for_submodules,
    }))
}

pub(crate) async fn delete_selected_worktree_branches(
    plan: &DeleteWorktreeProjectPlan,
    branch_name: Option<String>,
) -> std::result::Result<Vec<Value>, DeleteWorktreeProjectError> {
    let mut warnings = Vec::new();
    if plan.params.delete_local_branch {
        if let Some(branch) = branch_name.as_deref() {
            let mut extra = Map::new();
            extra.insert("branch".to_string(), Value::String(branch.to_string()));
            let local_delete = run_delete_git_action(
                &plan.projects,
                "deleteLocalBranch",
                &plan.parent_path,
                extra,
            )
            .await?;
            if exit_code(&local_delete) != 0 {
                warnings.push(json!({
                    "kind": "localBranchDeleteFailed",
                    "message": operation_failure_message(&local_delete, "git branch -d failed."),
                }));
            }
        } else {
            warnings.push(json!({
                "kind": "localBranchNotResolved",
                "message": "No local branch could be resolved.",
            }));
        }
    }
    if plan.params.delete_remote_branch {
        if let Some(branch) = branch_name.as_deref() {
            let mut extra = Map::new();
            extra.insert("branch".to_string(), Value::String(branch.to_string()));
            extra.insert(
                "remoteName".to_string(),
                Value::String(plan.params.remote_name.clone()),
            );
            let remote_delete = run_delete_git_action(
                &plan.projects,
                "deleteRemoteBranch",
                &plan.parent_path,
                extra,
            )
            .await?;
            if exit_code(&remote_delete) != 0 {
                warnings.push(json!({
                    "kind": "remoteBranchDeleteFailed",
                    "message": operation_failure_message(&remote_delete, "git push origin --delete failed."),
                }));
            }
        } else {
            warnings.push(json!({
                "kind": "remoteBranchNotResolved",
                "message": "No branch name could be resolved.",
            }));
        }
    }
    Ok(warnings)
}

pub(crate) async fn run_delete_git_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runGitAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) async fn run_delete_worktree_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) fn remove_worktree_project_row(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<Value, DeleteWorktreeProjectError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository.remove_project(project_id).map_err(Into::into)
}

pub(crate) fn schedule_deleted_worktree_project_delta(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(), DeleteWorktreeProjectError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")
        .map_err(Into::into)
}

pub(crate) fn delete_worktree_project_error_response(
    endpoint_path: String,
    request_id: String,
    error: DeleteWorktreeProjectError,
) -> RoutedResponse {
    match error {
        DeleteWorktreeProjectError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        DeleteWorktreeProjectError::ProjectPath(error) => {
            project_path_error_response(endpoint_path, request_id, error)
        }
        DeleteWorktreeProjectError::Typed(error) => {
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}
