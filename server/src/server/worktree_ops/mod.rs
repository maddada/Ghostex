use super::*;

pub mod common;
pub mod delete;
pub mod projects;
pub mod rename;
pub mod sessions;

pub(crate) use common::*;
pub(crate) use delete::*;
pub(crate) use projects::*;
pub(crate) use rename::*;
pub(crate) use sessions::*;

#[derive(Clone)]
pub(crate) struct ProjectWorktreeOperationContext {
    parent_path: String,
    parent_project: Value,
    parent_project_id: String,
    projects: Vec<Value>,
    source_path: String,
    source_project_id: String,
}

#[derive(Clone)]
pub(crate) struct ProjectWorktreeOptionRow {
    branch: String,
    is_current_project: bool,
    is_registered: bool,
    name: String,
    path: String,
    worktree_key: String,
}

pub(crate) struct ProjectWorktreeTarget {
    branch: String,
    name: String,
    path: String,
}

pub(crate) enum ProjectWorktreeOperationError {
    Domain(DomainStateError),
    ProjectPath(ProjectPathHttpError),
    Typed(TypedOperationError),
}

impl From<DomainStateError> for ProjectWorktreeOperationError {
    fn from(error: DomainStateError) -> Self {
        Self::Domain(error)
    }
}

impl From<ProjectPathHttpError> for ProjectWorktreeOperationError {
    fn from(error: ProjectPathHttpError) -> Self {
        Self::ProjectPath(error)
    }
}

impl From<TypedOperationError> for ProjectWorktreeOperationError {
    fn from(error: TypedOperationError) -> Self {
        Self::Typed(error)
    }
}

/*
CDXC:RemoteWorktrees 2026-06-24-18:40:
GPUI remote Add Worktree, Open Existing, direct merge, and commit-on-new-branch
must be id-scoped gxserver operations. The daemon resolves registered
project/worktree ids to paths, derives target branch/path names from bounded
labels, re-lists worktrees before opening by opaque key, and never accepts a
renderer-provided absolute path as remote mutation authority.
*/
pub(crate) async fn handle_project_worktree_operation_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = match endpoint_path.as_str() {
        "/api/listProjectWorktrees" => list_project_worktrees_for_project(state, &params).await,
        "/api/createProjectWorktree" => create_project_worktree_for_project(state, &params).await,
        "/api/openProjectWorktree" => open_project_worktree_for_project(state, &params).await,
        "/api/mergeWorktreeIntoMain" => merge_worktree_into_main_for_project(state, &params).await,
        "/api/checkoutProjectNewBranch" => {
            checkout_project_new_branch_for_commit(state, &params).await
        }
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver worktree endpoint."
        ))
        .into()),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => project_worktree_operation_error_response(endpoint_path, request_id, error),
    }
}

pub(crate) async fn list_project_worktrees_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    project_worktree_list_payload(&context).await
}

pub(crate) async fn create_project_worktree_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let base_ref = normalize_project_worktree_git_ref(params.get("baseRef"), "baseRef")?;
    let name_hint = normalize_project_worktree_name_hint(params.get("nameHint"))?;
    let target = resolve_unique_project_worktree_target(&context, &name_hint).await?;
    let mut create_params = Map::new();
    create_params.insert("baseRef".to_string(), Value::String(base_ref));
    create_params.insert("branch".to_string(), Value::String(target.branch.clone()));
    create_params.insert(
        "worktreePath".to_string(),
        Value::String(target.path.clone()),
    );
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }

    let parent_name = context
        .parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let project = register_project_worktree_path(
        state,
        &target.path,
        &format!("{parent_name}-{}", target.name),
        "projectAdded",
    )?;
    prepare_registered_worktree_project(state, &project, &context.source_project_id).await?;
    Ok(json!({ "project": project }))
}

pub(crate) async fn open_project_worktree_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let worktree_key = normalize_project_worktree_key(params.get("worktreeKey"))?;
    let options = project_worktree_options(&context).await?;
    let selected = options
        .into_iter()
        .find(|option| option.worktree_key == worktree_key)
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "Selected worktree is no longer in the current gxserver worktree list.",
            )
        })?;
    let project =
        register_project_worktree_path(state, &selected.path, &selected.name, "projectAdded")?;
    prepare_registered_worktree_project(state, &project, &context.source_project_id).await?;
    Ok(json!({ "project": project }))
}

pub(crate) async fn merge_worktree_into_main_for_project(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let plan = prepare_project_worktree_merge_plan(state, params)?;
    let branch = resolve_current_project_worktree_branch_name(&plan.projects, &plan.worktree_path)
        .await?
        .or(plan.worktree_branch)
        .ok_or_else(|| {
            DomainStateError::bad_request("Create and checkout a branch before merging.")
        })?;

    let mut main_params = Map::new();
    main_params.insert("ref".to_string(), Value::String("main".to_string()));
    let main_check =
        run_project_git_action(&plan.projects, "verifyRef", &plan.parent_path, main_params).await?;
    if exit_code(&main_check) != 0 {
        return Err(DomainStateError::bad_request(
            "The parent project does not have a local main branch.",
        )
        .into());
    }

    let status =
        run_project_git_action(&plan.projects, "status", &plan.parent_path, Map::new()).await?;
    if exit_code(&status) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&status, "Could not read parent project status."),
            scope_rejection: false,
        }
        .into());
    }
    if has_porcelain_status_changes(status.get("stdout").and_then(Value::as_str).unwrap_or("")) {
        return Err(DomainStateError::bad_request(
            "Commit or stash changes in the main project before merging this worktree.",
        )
        .into());
    }

    let mut checkout_params = Map::new();
    checkout_params.insert("branch".to_string(), Value::String("main".to_string()));
    let checkout = run_project_git_action(
        &plan.projects,
        "checkout",
        &plan.parent_path,
        checkout_params,
    )
    .await?;
    if exit_code(&checkout) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&checkout, "Could not checkout main."),
            scope_rejection: false,
        }
        .into());
    }

    let mut merge_params = Map::new();
    merge_params.insert("branch".to_string(), Value::String(branch));
    let merge =
        run_project_git_action(&plan.projects, "merge", &plan.parent_path, merge_params).await?;
    let status = if exit_code(&merge) == 0 {
        "merged"
    } else {
        "conflicts"
    };
    Ok(json!({
        "parentProjectId": plan.parent_project_id,
        "status": status,
    }))
}

pub(crate) async fn checkout_project_new_branch_for_commit(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let branch_label = normalize_project_branch_label(params.get("branchLabel"))?;
    let (projects, project_path) = resolve_registered_project_path(state, &project_id)?;
    for index in 0..20 {
        let branch = if index == 0 {
            branch_label.clone()
        } else {
            format!("{branch_label}-{}", index + 1)
        };
        let mut verify_params = Map::new();
        verify_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let exists =
            run_project_git_action(&projects, "verifyRef", &project_path, verify_params).await?;
        if exit_code(&exists) == 0 {
            continue;
        }
        let mut checkout_params = Map::new();
        checkout_params.insert("branch".to_string(), Value::String(branch));
        let checkout = run_project_git_action(
            &projects,
            "checkoutNewBranch",
            &project_path,
            checkout_params,
        )
        .await?;
        if exit_code(&checkout) == 0 {
            return Ok(json!({ "checkedOut": true }));
        }
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&checkout, "Could not create a new branch."),
            scope_rejection: false,
        }
        .into());
    }
    Err(DomainStateError::bad_request("Could not create a unique branch.").into())
}

// ---------------------------------------------------------------------------
// Sidebar V2 worktree sessions
// ---------------------------------------------------------------------------
