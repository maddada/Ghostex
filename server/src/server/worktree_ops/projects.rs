use super::*;

pub(crate) fn resolve_project_worktree_operation_context(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<ProjectWorktreeOperationContext, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let source_project = repository.get_project(&project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    if source_project
        .get("isRecentProject")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(DomainStateError::bad_request(
            "Restore the project before using worktree actions.",
        )
        .into());
    }
    let source_path = project_required_path(&source_project, "Project")?;
    let source_path = normalize_existing_directory_path(
        Some(&Value::String(source_path)),
        "project.path",
        &state.paths.home_dir,
    )?;
    let source_project_id = value_text(&source_project, "projectId")?;
    let (parent_project, parent_project_id) =
        if let Some(worktree) = normalize_worktree_metadata(source_project.get("worktree")) {
            let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
            let parent_project_id = value_text(&parent_project, "projectId")?;
            (parent_project, parent_project_id)
        } else {
            (source_project.clone(), source_project_id.clone())
        };
    let parent_path = project_required_path(&parent_project, "Parent project")?;
    let parent_path = normalize_existing_directory_path(
        Some(&Value::String(parent_path)),
        "parentProject.path",
        &state.paths.home_dir,
    )?;
    Ok(ProjectWorktreeOperationContext {
        parent_path,
        parent_project,
        parent_project_id,
        projects,
        source_path,
        source_project_id,
    })
}

pub(crate) async fn project_worktree_list_payload(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let worktrees = project_worktree_options(context).await?;
    let branches = project_worktree_base_branches(context).await?;
    Ok(json!({
        "branches": branches,
        "parentProjectId": context.parent_project_id,
        "sourceProjectId": context.source_project_id,
        "worktrees": worktrees.into_iter().map(project_worktree_option_json).collect::<Vec<_>>(),
    }))
}

pub(crate) async fn project_worktree_options(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Vec<ProjectWorktreeOptionRow>, ProjectWorktreeOperationError> {
    let result =
        run_project_worktree_action(&context.projects, "list", &context.parent_path, Map::new())
            .await?;
    if exit_code(&result) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&result, "Could not list worktrees."),
            scope_rejection: false,
        }
        .into());
    }
    let entries = result
        .get("worktrees")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let main_path = entries
        .iter()
        .find(|entry| entry.get("bare").and_then(Value::as_bool) != Some(true))
        .and_then(|entry| entry.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .filter(|path| !path.is_empty())
        .unwrap_or_else(|| normalize_project_path_for_comparison(&context.parent_path));
    let source_path = normalize_project_path_for_comparison(&context.source_path);
    let registered_paths = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .collect::<HashSet<_>>();

    let mut options = Vec::new();
    for entry in entries {
        if entry.get("bare").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let path = entry
            .get("path")
            .and_then(Value::as_str)
            .map(normalize_project_path_for_comparison)
            .unwrap_or_default();
        if path.is_empty() || path == main_path {
            continue;
        }
        let branch = entry
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_string();
        options.push(ProjectWorktreeOptionRow {
            branch: branch.clone(),
            is_current_project: path == source_path,
            is_registered: registered_paths.contains(&path),
            name: path_file_name_for_project(&path),
            worktree_key: project_worktree_selection_key(&path, &branch),
            path,
        });
    }
    Ok(options)
}

pub(crate) async fn project_worktree_base_branches(
    context: &ProjectWorktreeOperationContext,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let result = run_project_git_action(
        &context.projects,
        "listBranches",
        &context.parent_path,
        Map::new(),
    )
    .await?;
    if exit_code(&result) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&result, "Could not list branches."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(result
        .get("branches")
        .cloned()
        .unwrap_or_else(|| Value::Array(Vec::new())))
}

pub(crate) async fn resolve_unique_project_worktree_target(
    context: &ProjectWorktreeOperationContext,
    name_hint: &str,
) -> std::result::Result<ProjectWorktreeTarget, ProjectWorktreeOperationError> {
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let project_folder_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("project");
    let registered_paths = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .map(normalize_project_path_for_comparison)
        .collect::<HashSet<_>>();
    for index in 0..50 {
        let name = if index == 0 {
            name_hint.to_string()
        } else {
            format!("{name_hint}-{}", index + 1)
        };
        let branch = name.clone();
        let path = path_to_string(&parent_directory.join(format!("{project_folder_name}-{name}")));
        let normalized_path = normalize_project_path_for_comparison(&path);
        let mut branch_params = Map::new();
        branch_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let mut path_params = Map::new();
        path_params.insert("worktreePath".to_string(), Value::String(path.clone()));
        let branch_check = run_project_git_action(
            &context.projects,
            "verifyRef",
            &context.source_path,
            branch_params,
        )
        .await?;
        let path_check = run_project_worktree_action(
            &context.projects,
            "pathExists",
            &context.source_path,
            path_params,
        )
        .await?;
        if exit_code(&branch_check) != 0
            && exit_code(&path_check) != 0
            && !registered_paths.contains(&normalized_path)
        {
            return Ok(ProjectWorktreeTarget {
                branch,
                name,
                path: normalized_path,
            });
        }
    }
    Err(DomainStateError::bad_request("Could not create a unique worktree name.").into())
}

pub(crate) fn register_project_worktree_path(
    state: &AppState,
    path: &str,
    name: &str,
    delta_type: &str,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut params = Map::new();
    params.insert("name".to_string(), Value::String(name.to_string()));
    params.insert("path".to_string(), Value::String(path.to_string()));
    let project = repository.add_project_path(&params)?;
    let project_id = value_text(&project, "projectId")?;
    schedule_presentation_project_delta(state, &db, &repository, &project_id, delta_type)?;
    Ok(project)
}

pub(crate) async fn prepare_registered_worktree_project(
    state: &AppState,
    project: &Value,
    setup_project_id: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let project_id = value_text(project, "projectId")?;
    let projects = list_domain_projects(state)?;
    let hooks = run_project_worktree_action_by_project_id(
        &projects,
        "ensureBeadsHooks",
        &project_id,
        Map::new(),
    )
    .await?;
    if exit_code(&hooks) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&hooks, "Could not prepare Beads hooks."),
            scope_rejection: false,
        }
        .into());
    }
    let setup_project = projects
        .iter()
        .find(|candidate| {
            candidate.get("projectId").and_then(Value::as_str) == Some(setup_project_id)
        })
        .cloned();
    /*
    CDXC:Projects 2026-08-02:
    This gate skips the setup call entirely when nothing is configured, so it has
    to consult the Global Default too. Otherwise a project relying on the global
    would return here and never reach the executor that would have run it.
    */
    if crate::global_project_defaults::resolve_with_global_default(
        setup_project
            .as_ref()
            .and_then(|project| project.get("gitConfig"))
            .and_then(Value::as_object)
            .and_then(|config| config.get("worktreeCommand"))
            .and_then(Value::as_str),
        &crate::global_project_defaults::read_global_project_defaults().worktree_command,
    )
    .is_none()
    {
        return Ok(());
    }
    let mut setup_params = Map::new();
    setup_params.insert(
        "action".to_string(),
        Value::String("worktreeSetupCommand".to_string()),
    );
    setup_params.insert("projectId".to_string(), Value::String(project_id));
    setup_params.insert(
        "setupCommandProjectId".to_string(),
        Value::String(setup_project_id.to_string()),
    );
    let setup =
        dispatch_typed_operation_endpoint("/api/runProjectSetupCommand", &setup_params, projects)
            .await?;
    if exit_code(&setup) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&setup, "Worktree setup command failed."),
            scope_rejection: false,
        }
        .into());
    }
    Ok(())
}

pub(crate) struct ProjectWorktreeMergePlan {
    pub(crate) parent_path: String,
    pub(crate) parent_project_id: String,
    pub(crate) projects: Vec<Value>,
    pub(crate) worktree_branch: Option<String>,
    pub(crate) worktree_path: String,
}

pub(crate) fn prepare_project_worktree_merge_plan(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<ProjectWorktreeMergePlan, ProjectWorktreeOperationError> {
    let project_id = read_project_id(params)?;
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let projects = repository.list_projects()?;
    let worktree_project = repository.get_project(&project_id)?.ok_or_else(|| {
        DomainStateError::not_found(format!("Project {project_id} does not exist."))
    })?;
    let worktree = normalize_worktree_metadata(worktree_project.get("worktree"))
        .ok_or_else(|| DomainStateError::bad_request("Project is not a worktree."))?;
    let parent_project = resolve_worktree_parent_project(&projects, &worktree)?;
    let worktree_path = project_required_path(&worktree_project, "Worktree project")?;
    let parent_path = project_required_path(&parent_project, "Parent project")?;
    Ok(ProjectWorktreeMergePlan {
        parent_path: normalize_existing_directory_path(
            Some(&Value::String(parent_path)),
            "parentProject.path",
            &state.paths.home_dir,
        )?,
        parent_project_id: value_text(&parent_project, "projectId")?,
        projects,
        worktree_branch: worktree.branch,
        worktree_path: normalize_existing_directory_path(
            Some(&Value::String(worktree_path)),
            "project.path",
            &state.paths.home_dir,
        )?,
    })
}

pub(crate) async fn resolve_current_project_worktree_branch_name(
    projects: &[Value],
    worktree_path: &str,
) -> std::result::Result<Option<String>, ProjectWorktreeOperationError> {
    let branch = run_project_git_action(projects, "branch", worktree_path, Map::new()).await?;
    if exit_code(&branch) == 0 {
        if let Some(branch_name) = branch
            .get("stdout")
            .and_then(Value::as_str)
            .and_then(normalize_branch_name)
        {
            return Ok(Some(branch_name));
        }
    }
    Ok(None)
}

pub(crate) fn resolve_registered_project_path(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(Vec<Value>, String), ProjectWorktreeOperationError> {
    let projects = list_domain_projects(state)?;
    let project = projects
        .iter()
        .find(|candidate| candidate.get("projectId").and_then(Value::as_str) == Some(project_id))
        .ok_or_else(|| {
            DomainStateError::not_found(format!("Project {project_id} does not exist."))
        })?;
    let path = project_required_path(project, "Project")?;
    let path = normalize_existing_directory_path(
        Some(&Value::String(path)),
        "project.path",
        &state.paths.home_dir,
    )?;
    Ok((projects, path))
}

pub(crate) fn list_domain_projects(
    state: &AppState,
) -> std::result::Result<Vec<Value>, ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    repository.list_projects().map_err(Into::into)
}

pub(crate) async fn run_project_git_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runGitAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) async fn run_project_worktree_action(
    projects: &[Value],
    action: &str,
    project_path: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectPath".to_string(),
        Value::String(project_path.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) async fn run_project_worktree_action_by_project_id(
    projects: &[Value],
    action: &str,
    project_id: &str,
    mut extra_params: Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    extra_params.insert("action".to_string(), Value::String(action.to_string()));
    extra_params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    dispatch_typed_operation_endpoint("/api/runWorktreeAction", &extra_params, projects.to_vec())
        .await
        .map_err(Into::into)
}

pub(crate) fn project_worktree_operation_error_response(
    endpoint_path: String,
    request_id: String,
    error: ProjectWorktreeOperationError,
) -> RoutedResponse {
    match error {
        ProjectWorktreeOperationError::Domain(error) => {
            domain_error_response(endpoint_path, request_id, error)
        }
        ProjectWorktreeOperationError::ProjectPath(error) => {
            project_path_error_response(endpoint_path, request_id, error)
        }
        ProjectWorktreeOperationError::Typed(error) => {
            typed_operation_error_response(endpoint_path, request_id, error)
        }
    }
}

pub(crate) fn project_worktree_option_json(option: ProjectWorktreeOptionRow) -> Value {
    json!({
        "branch": option.branch,
        "isCurrentProject": option.is_current_project,
        "isRegistered": option.is_registered,
        "name": option.name,
        "path": option.path,
        "worktreeKey": option.worktree_key,
    })
}

pub(crate) fn project_worktree_selection_key(path: &str, branch: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"gxserver-worktree-selection-v1\0");
    hasher.update(path.as_bytes());
    hasher.update(b"\0");
    hasher.update(branch.as_bytes());
    let digest = hasher.finalize();
    format!("W{}", hex_prefix(&digest, 32))
}

pub(crate) fn hex_prefix(bytes: &[u8], max_chars: usize) -> String {
    let mut output = String::new();
    for byte in bytes {
        if output.len() >= max_chars {
            break;
        }
        output.push_str(&format!("{byte:02x}"));
    }
    output.truncate(max_chars);
    output
}

pub(crate) fn normalize_project_path_for_comparison(path: &str) -> String {
    let normalized = path_to_string(&resolve_path_syntax(PathBuf::from(path.trim())));
    if normalized == "/" {
        normalized
    } else {
        normalized.trim_end_matches('/').to_string()
    }
}

pub(crate) fn resolve_sidebar_collection_project_id(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<String, DomainStateError> {
    if let Some(project_id) = params
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|project_id| !project_id.is_empty())
    {
        return repository
            .get_project(project_id)?
            .map(|_| project_id.to_string())
            .ok_or_else(|| {
                DomainStateError::not_found(format!("Project {project_id} does not exist."))
            });
    }
    let projects = repository.list_projects()?;
    let matches: Vec<&Value> = if let Some(path) = params
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let requested = normalize_project_path_for_comparison(path);
        projects
            .iter()
            .filter(|project| {
                project
                    .get("path")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| {
                        normalize_project_path_for_comparison(candidate) == requested
                    })
            })
            .collect()
    } else if let Some(name) = params
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        projects
            .iter()
            .filter(|project| {
                project
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
            .collect()
    } else {
        return Err(DomainStateError::bad_request(
            "group-project requires --project-id, --path, or --name.",
        ));
    };
    if matches.len() > 1 {
        return Err(DomainStateError::bad_request(
            "The project selector matched more than one project; pass --project-id.",
        ));
    }
    matches
        .first()
        .and_then(|project| project.get("projectId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::not_found("No Ghostex project matched that selector."))
}

pub(crate) fn path_file_name_for_project(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Worktree")
        .to_string()
}

pub(crate) fn project_required_path(
    project: &Value,
    label: &str,
) -> std::result::Result<String, DomainStateError> {
    project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{label} has no filesystem path.")))
}

pub(crate) fn normalize_project_worktree_key(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("worktreeKey must be a non-empty string."))?;
    if value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(value.to_string())
    } else {
        Err(DomainStateError::bad_request(
            "worktreeKey contains unsupported characters.",
        ))
    }
}

pub(crate) fn normalize_project_worktree_git_ref(
    input: Option<&Value>,
    field: &str,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request(format!("{field} must be a non-empty string."))
        })?;
    if value.len() <= 200 && is_allowed_project_git_ref(value) {
        Ok(value.to_string())
    } else {
        Err(DomainStateError::bad_request(format!(
            "{field} is not an allowed Git ref."
        )))
    }
}

pub(crate) fn normalize_project_worktree_name_hint(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("nameHint must be a non-empty string."))?;
    normalize_project_slug_label(value, "nameHint")
}

pub(crate) fn normalize_project_branch_label(
    input: Option<&Value>,
) -> std::result::Result<String, DomainStateError> {
    let value = input
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("branchLabel must be a non-empty string."))?;
    normalize_project_slug_label(value, "branchLabel")
}

pub(crate) fn normalize_project_slug_label(
    value: &str,
    field: &str,
) -> std::result::Result<String, DomainStateError> {
    if value.chars().count() > 160 || value.contains('\0') {
        return Err(DomainStateError::bad_request(format!(
            "{field} exceeds the allowed label size."
        )));
    }
    let mut output = String::new();
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            output.push('-');
            last_was_dash = true;
        }
        if output.len() >= 48 {
            break;
        }
    }
    let normalized = output.trim_matches('-').to_string();
    if normalized.is_empty()
        || !normalized
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(DomainStateError::bad_request(format!(
            "{field} must contain at least one ASCII letter or number."
        )));
    }
    Ok(normalized)
}

pub(crate) fn is_allowed_project_git_ref(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphanumeric())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '/' | '-'))
        && !value.contains("..")
        && !value.contains("//")
        && !value.ends_with('/')
}
