use super::*;

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
`createWorktreeSession` is one atomic server operation: optional `git fetch
origin`, `git worktree add -b ghostex/<8hex>`, the project's own worktree setup
command, then an ORDINARY gxserver session created through the same
create/identity/start machinery every other session uses, with the worktree as
its cwd. Anything that fails after the checkout exists rolls the checkout back,
so a failed attempt never leaves a stray directory or branch behind.

The worktree is deliberately NOT registered as a project (no
`registerProjectPath`): in Sidebar V2 a worktree is an attribute of a session,
and the branch on its card comes from the per-session git probe reading that cwd.
*/
pub(crate) const GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_MAX_BYTES: usize = 16_384;
/*
The same settle window GPUI uses between starting an agent session's provider
and submitting its first prompt: the agent CLI has to draw its composer before
typed text means anything.
*/
pub(crate) const GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_READY_DELAY_MS: u64 = 4_000;
pub(crate) const GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS: usize = 8;
pub(crate) const GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE: &str = "Terminal";
/*
Warnings are user-facing strings by contract (`warnings?: readonly string[]`),
so they stay fixed, bounded sentences: raw git stdout/stderr never reaches a
client through this endpoint.
*/
pub(crate) const WORKTREE_SESSION_DIRTY_WARNING: &str = "This worktree has uncommitted changes.";

pub(crate) struct WorktreeSessionCreateRequest {
    agent_id: Option<String>,
    base_branch: Option<String>,
    existing_worktree_path: Option<String>,
    first_prompt: Option<String>,
    start_from_origin: bool,
}

pub(crate) struct PreparedWorktreeCheckout {
    pub(crate) branch: String,
    /// False when an existing worktree was adopted: rollback must never remove a
    /// checkout this request did not create.
    pub(crate) created: bool,
    pub(crate) path: String,
}

/*
`git worktree list` prints the REAL path (symlinks resolved), while registered
project paths and client-supplied paths keep whatever form the user typed. Both
are compared through their resolved form so a repository reached through a
symlink still matches its own worktree list, while every git command keeps
running against the path form the registered project family is expressed in.
*/
pub(crate) fn canonical_worktree_path_key(path: &str) -> String {
    fs::canonicalize(path)
        .map(|resolved| normalize_project_path_for_comparison(&path_to_string(&resolved)))
        .unwrap_or_else(|_| normalize_project_path_for_comparison(path))
}

pub(crate) async fn handle_worktree_session_http(
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
        "/api/createWorktreeSession" => create_worktree_session(state, &params).await,
        "/api/removeSessionWorktree" => remove_session_worktree(state, &params).await,
        _ => Err(DomainStateError::not_found(format!(
            "{endpoint_path} is not a gxserver worktree session endpoint."
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

pub(crate) async fn create_worktree_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let request = normalize_worktree_session_create_request(params)?;
    let prepared = prepare_worktree_session_checkout(state, &context, &request).await?;
    match start_worktree_session(state, &context, &request, &prepared).await {
        Ok(session_id) => Ok(json!({
            "branch": prepared.branch,
            "sessionId": session_id,
            "worktreePath": prepared.path,
        })),
        Err(error) => {
            if prepared.created {
                rollback_worktree_session_checkout(&context, &prepared).await;
            }
            Err(error)
        }
    }
}

pub(crate) fn normalize_worktree_session_create_request(
    params: &Map<String, Value>,
) -> std::result::Result<WorktreeSessionCreateRequest, DomainStateError> {
    let agent_id = params
        .get("agentId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(agent_id) = agent_id.as_deref() {
        if agent_id.len() > 64
            || !agent_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            return Err(DomainStateError::bad_request(
                "agentId is not an allowed agent id.",
            ));
        }
    }
    let base_branch = match params.get("baseBranch") {
        None | Some(Value::Null) => None,
        Some(value) if value.as_str().map(str::trim) == Some("") => None,
        Some(value) => Some(normalize_project_worktree_git_ref(
            Some(value),
            "baseBranch",
        )?),
    };
    let first_prompt = params
        .get("firstPrompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(prompt) = first_prompt.as_deref() {
        if prompt.len() > GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_MAX_BYTES {
            return Err(DomainStateError::bad_request(
                "firstPrompt exceeds the 16384-byte limit.",
            ));
        }
    }
    let existing_worktree_path = match params.get("existingWorktree") {
        None | Some(Value::Null) => None,
        Some(Value::Object(existing)) => {
            let path = existing
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    DomainStateError::bad_request("existingWorktree.path must be a non-empty path.")
                })?;
            Some(path.to_string())
        }
        Some(_) => {
            return Err(DomainStateError::bad_request(
                "existingWorktree must be an object with a path.",
            ))
        }
    };
    Ok(WorktreeSessionCreateRequest {
        agent_id,
        base_branch,
        existing_worktree_path,
        first_prompt,
        start_from_origin: params.get("startFromOrigin").and_then(Value::as_bool) == Some(true),
    })
}

/*
Either adopts the caller's existing worktree or creates a fresh one. An adopted
path is never trusted as given: it has to appear in THIS project family's current
`git worktree list`, which is the same authority `openProjectWorktree` uses, so a
renderer cannot point a session at an arbitrary directory.
*/
pub(crate) async fn prepare_worktree_session_checkout(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
) -> std::result::Result<PreparedWorktreeCheckout, ProjectWorktreeOperationError> {
    if let Some(existing_path) = request.existing_worktree_path.as_deref() {
        let requested = normalize_existing_directory_path(
            Some(&Value::String(existing_path.to_string())),
            "existingWorktree.path",
            &state.paths.home_dir,
        )?;
        let requested = normalize_project_path_for_comparison(&requested);
        let requested_key = canonical_worktree_path_key(&requested);
        let selected = project_worktree_options(context)
            .await?
            .into_iter()
            .find(|option| canonical_worktree_path_key(&option.path) == requested_key)
            .ok_or_else(|| {
                DomainStateError::bad_request(
                    "existingWorktree.path is not a worktree of this project.",
                )
            })?;
        let branch = if selected.branch.is_empty() {
            worktree_session_branch_for_path(context, &requested)
                .await?
                .unwrap_or_default()
        } else {
            selected.branch.clone()
        };
        return Ok(PreparedWorktreeCheckout {
            branch,
            created: false,
            path: requested,
        });
    }

    let base_ref = resolve_worktree_session_base_ref(context, request).await?;
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let project_folder_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let mut target: Option<(String, String)> = None;
    for _ in 0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS {
        let suffix = worktree_sessions::create_temp_branch_suffix();
        let branch = worktree_sessions::temp_branch_name(&suffix);
        let path = path_to_string(&parent_directory.join(
            worktree_sessions::worktree_directory_name(project_folder_name, &suffix),
        ));
        let mut branch_params = Map::new();
        branch_params.insert(
            "ref".to_string(),
            Value::String(format!("refs/heads/{branch}")),
        );
        let branch_check = run_project_git_action(
            &context.projects,
            "verifyRef",
            &context.source_path,
            branch_params,
        )
        .await?;
        let mut path_params = Map::new();
        path_params.insert("worktreePath".to_string(), Value::String(path.clone()));
        let path_check = run_project_worktree_action(
            &context.projects,
            "pathExists",
            &context.source_path,
            path_params,
        )
        .await?;
        if exit_code(&branch_check) != 0 && exit_code(&path_check) != 0 {
            target = Some((branch, path));
            break;
        }
    }
    let Some((branch, path)) = target else {
        return Err(DomainStateError::bad_request(
            "Could not reserve a unique worktree branch and directory.",
        )
        .into());
    };

    let mut create_params = Map::new();
    create_params.insert("baseRef".to_string(), Value::String(base_ref));
    create_params.insert("branch".to_string(), Value::String(branch.clone()));
    create_params.insert("worktreePath".to_string(), Value::String(path.clone()));
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        /*
        A failed `git worktree add` is not always a no-op: it can leave a stale
        worktree registration behind, and `-b` may already have created the
        branch. The compensator therefore runs on this path too, so a refused
        request leaves the repository exactly as it found it.
        */
        rollback_worktree_session_checkout(
            context,
            &PreparedWorktreeCheckout {
                branch: branch.clone(),
                created: true,
                path: path.clone(),
            },
        )
        .await;
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }
    let prepared = PreparedWorktreeCheckout {
        branch,
        created: true,
        path,
    };

    if let Err(error) = run_worktree_session_setup_command(context, &prepared.path).await {
        rollback_worktree_session_checkout(context, &prepared).await;
        return Err(error);
    }
    Ok(prepared)
}

/// The project's own `worktreeCommand`, run with the new (unregistered) worktree
/// as cwd. A project without one resolves to a no-op inside the typed operation.
pub(crate) async fn run_worktree_session_setup_command(
    context: &ProjectWorktreeOperationContext,
    worktree_path: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let mut setup_params = Map::new();
    setup_params.insert(
        "action".to_string(),
        Value::String("worktreeSetupCommand".to_string()),
    );
    setup_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    setup_params.insert(
        "setupCommandProjectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    setup_params.insert(
        "worktreePath".to_string(),
        Value::String(worktree_path.to_string()),
    );
    let setup = dispatch_worktree_path_operation(
        "/api/runProjectSetupCommand",
        &setup_params,
        context.projects.clone(),
    )
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

/*
`startFromOrigin` fetches the remote first and bases
the new branch on the REMOTE tip, not on whatever the local branch happens to
point at. Without it the base is the requested branch, or the repository's own
default branch resolved by the shared P3 rules.
*/
pub(crate) async fn resolve_worktree_session_base_ref(
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let base = match request.base_branch.clone() {
        Some(base) => base,
        None => {
            let repository_path = context.parent_path.clone();
            let default_branch = tokio::task::spawn_blocking(move || {
                worktree_sessions::resolve_repository_default_branch(&repository_path)
            })
            .await
            .ok()
            .flatten();
            default_branch.map(|branch| branch.git_ref).ok_or_else(|| {
                DomainStateError::bad_request(
                    "This repository has no default branch to base a worktree on. Choose a base branch.",
                )
            })?
        }
    };
    if !request.start_from_origin {
        return Ok(base);
    }
    let repository_path = context.parent_path.clone();
    let fetch_base = base.clone();
    let commit = tokio::task::spawn_blocking(move || {
        if !worktree_sessions::fetch_worktree_origin(&repository_path) {
            return None;
        }
        worktree_sessions::resolve_origin_base_commit(&repository_path, &fetch_base)
    })
    .await
    .ok()
    .flatten();
    commit.ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "Could not resolve origin/{} after fetching origin.",
            worktree_sessions::base_branch_short_name(&base)
        ))
        .into()
    })
}

pub(crate) async fn worktree_session_branch_for_path(
    context: &ProjectWorktreeOperationContext,
    worktree_path: &str,
) -> std::result::Result<Option<String>, ProjectWorktreeOperationError> {
    let mut params = Map::new();
    params.insert("action".to_string(), Value::String("branch".to_string()));
    params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    params.insert(
        "worktreePath".to_string(),
        Value::String(worktree_path.to_string()),
    );
    let branch =
        dispatch_worktree_path_operation("/api/runGitAction", &params, context.projects.clone())
            .await?;
    if exit_code(&branch) != 0 {
        return Ok(None);
    }
    Ok(branch
        .get("stdout")
        .and_then(Value::as_str)
        .and_then(normalize_branch_name))
}

/*
Compensation for a half-created worktree session. Every step is best effort and
independently useful: the checkout may exist without the branch being reachable,
`git worktree add` may have left a registration behind, and the caller has
already failed, so a cleanup failure must not replace the real error.
*/
pub(crate) async fn rollback_worktree_session_checkout(
    context: &ProjectWorktreeOperationContext,
    prepared: &PreparedWorktreeCheckout,
) {
    let mut remove_params = Map::new();
    remove_params.insert(
        "worktreePath".to_string(),
        Value::String(prepared.path.clone()),
    );
    remove_params.insert("force".to_string(), Value::Bool(true));
    let _ = run_project_worktree_action(
        &context.projects,
        "remove",
        &context.parent_path,
        remove_params,
    )
    .await;
    let _ =
        run_project_worktree_action(&context.projects, "prune", &context.parent_path, Map::new())
            .await;
    if worktree_sessions::is_managed_worktree_branch(&prepared.branch) {
        let mut branch_params = Map::new();
        branch_params.insert("branch".to_string(), Value::String(prepared.branch.clone()));
        let _ = run_project_git_action(
            &context.projects,
            "deleteLocalBranchForce",
            &context.parent_path,
            branch_params,
        )
        .await;
    }
}

/*
The session half. This is the ordinary gxserver create path — the same
`createAgentSession` parameter builder, the same `create_session` +
`apply_created_session_identity` pair, the same `startSessionProvider` — with
`cwd` pointed at the worktree. Nothing here is a worktree-specific session
concept; the only extra state is the marker that lets the branch auto-rename
recognise its own work later.
*/
pub(crate) async fn start_worktree_session(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    prepared: &PreparedWorktreeCheckout,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let (project_id, session_id) =
        create_and_start_worktree_session(state, context, request, prepared)?;
    if let Some(prompt) = request.first_prompt.as_deref() {
        /*
        Text and Enter are two separate zmx sends with a settle window between
        them (`sendSessionMessage` owns that split): bracketed-paste composers
        treat a carriage return inside the same burst as a newline and leave the
        prompt staged instead of submitted. A prompt that fails to land is not
        worth discarding a working session and its worktree over, so it is
        logged rather than rolled back.
        */
        tokio::time::sleep(Duration::from_millis(
            GXSERVER_WORKTREE_SESSION_FIRST_PROMPT_READY_DELAY_MS,
        ))
        .await;
        send_worktree_session_first_prompt(state, &project_id, &session_id, prompt);
    }
    Ok(session_id)
}

pub(crate) fn create_and_start_worktree_session(
    state: &AppState,
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    prepared: &PreparedWorktreeCheckout,
) -> std::result::Result<(String, String), ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project = repository
        .get_project(&context.source_project_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Project {} does not exist.",
                context.source_project_id
            ))
        })?;
    let agent_settings = read_agent_settings(&db)?;

    let mut create_params = Map::new();
    create_params.insert("cwd".to_string(), Value::String(prepared.path.clone()));
    create_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    create_params.insert(
        "surface".to_string(),
        Value::String("workspace".to_string()),
    );
    let mut create_params = if let Some(agent_id) = request.agent_id.as_deref() {
        create_params.insert("agentId".to_string(), Value::String(agent_id.to_string()));
        create_params.insert("requireLaunchCommand".to_string(), Value::Bool(true));
        if let Some(prompt) = request.first_prompt.as_deref() {
            create_params.insert(
                "runtimeSettings".to_string(),
                json!({ "firstUserMessage": prompt }),
            );
        }
        create_agent_session_params_for_project(&db, &project, &create_params)?
    } else {
        create_params.insert("kind".to_string(), Value::String("terminal".to_string()));
        create_params.insert(
            "title".to_string(),
            Value::String(GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE.to_string()),
        );
        create_params
    };

    let initial_title = create_params
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(GXSERVER_WORKTREE_SESSION_DEFAULT_TITLE)
        .to_string();
    let mut runtime_settings = create_params
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_settings.insert(
        worktree_sessions::WORKTREE_SESSION_RUNTIME_KEY.to_string(),
        worktree_sessions::worktree_session_marker_value(
            &prepared.branch,
            &prepared.path,
            &initial_title,
            &now_iso(),
        ),
    );
    create_params.insert(
        "runtimeSettings".to_string(),
        Value::Object(runtime_settings),
    );

    let created = repository.create_session(&create_params, false)?;
    /*
    The durable row exists the moment `create_session` returns. If the identity
    pass (or reading the identity back) fails, the caller's rollback only removes
    the checkout — the row would survive pointing at a directory that no longer
    exists. Drop it here so the failure leaves nothing behind.
    */
    let identity =
        apply_created_session_identity(&repository, &created, &create_params).and_then(|session| {
            let project_id = value_text(&session, "projectId")?;
            let session_id = value_text(&session, "sessionId")?;
            Ok((project_id, session_id))
        });
    let (project_id, session_id) = match identity {
        Ok(identity) => identity,
        Err(error) => {
            remove_created_worktree_session_row(&repository, &created);
            return Err(error.into());
        }
    };

    let zmx_context = ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().to_string(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    let mut lifecycle_params = Map::new();
    lifecycle_params.insert("projectId".to_string(), Value::String(project_id.clone()));
    lifecycle_params.insert("sessionId".to_string(), Value::String(session_id.clone()));
    if let Err(error) = dispatch_zmx_lifecycle_endpoint(
        &repository,
        "/api/startSessionProvider",
        &lifecycle_params,
        &zmx_context,
        &agent_settings,
    ) {
        /*
        The row exists but has no live terminal, so it must not survive as a
        ghost in the sidebar. Kill first in case a detached provider did come up
        after the failure, then drop the durable row.
        */
        let _ = dispatch_zmx_lifecycle_endpoint(
            &repository,
            "/api/killSession",
            &lifecycle_params,
            &zmx_context,
            &agent_settings,
        );
        let _ = repository.remove_session(&lifecycle_params);
        return Err(worktree_session_zmx_error(error).into());
    }
    /*
    Past this line the session is LIVE: a provider is running in the checkout.
    Failing the request now would roll the worktree back out from under it, so
    a delta that cannot be published is logged instead — the next presentation
    snapshot carries the row anyway.
    */
    if let Err(error) =
        schedule_presentation_session_delta(state, &db, &repository, &project_id, &session_id)
    {
        log_worktree_session_failure(
            state,
            "worktreeSessionDeltaFailed",
            &project_id,
            &session_id,
            &error.message,
        );
    }
    Ok((project_id, session_id))
}

/*
Best-effort compensation for a session row whose identity pass failed: without
ids there is nothing to delete, and a delete that fails leaves the same orphan
the caller is already reporting, so neither case replaces the real error.
*/
pub(crate) fn remove_created_worktree_session_row(repository: &DomainRepository<'_>, created: &Value) {
    let (Ok(project_id), Ok(session_id)) = (
        value_text(created, "projectId"),
        value_text(created, "sessionId"),
    ) else {
        return;
    };
    let mut params = Map::new();
    params.insert("projectId".to_string(), Value::String(project_id));
    params.insert("sessionId".to_string(), Value::String(session_id));
    let _ = repository.remove_session(&params);
}

pub(crate) fn send_worktree_session_first_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    prompt: &str,
) {
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            log_worktree_session_failure(
                state,
                "worktreeSessionFirstPromptFailed",
                project_id,
                session_id,
                &format!("SQLite gxserver state error: {error}"),
            );
            return;
        }
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let mut prompt_params = Map::new();
    prompt_params.insert(
        "projectId".to_string(),
        Value::String(project_id.to_string()),
    );
    prompt_params.insert(
        "sessionId".to_string(),
        Value::String(session_id.to_string()),
    );
    prompt_params.insert("submit".to_string(), Value::Bool(true));
    prompt_params.insert("text".to_string(), Value::String(prompt.to_string()));
    if let Err(error) = dispatch_zmx_session_interaction_endpoint(
        &repository,
        "/api/sendSessionMessage",
        &prompt_params,
    ) {
        log_worktree_session_failure(
            state,
            "worktreeSessionFirstPromptFailed",
            project_id,
            session_id,
            &worktree_session_zmx_error(error).message,
        );
    }
}

pub(crate) fn worktree_session_zmx_error(error: ZmxEndpointError) -> DomainStateError {
    match error {
        ZmxEndpointError::Domain(error) => error,
        ZmxEndpointError::DependencyUnavailable(message) => DomainStateError {
            code: "dependencyUnavailable",
            message,
        },
    }
}

pub(crate) fn log_worktree_session_failure(
    state: &AppState,
    event: &str,
    project_id: &str,
    session_id: &str,
    message: &str,
) {
    let _ = state.logger.log(GxserverLogInput {
        level: LogLevel::Warn,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(message.to_string()),
        details: Some(json!({
            "projectId": project_id,
            "sessionId": session_id,
        })),
    });
}

/*
CDXC:SidebarV2Worktrees 2026-07-29-00:00:
`removeSessionWorktree` backs the client's "last session in this worktree closed
— remove the worktree?" prompt. It answers the dirty question BEFORE destroying
anything, so the client can re-ask with `force`, and it only ever deletes a
branch gxserver itself minted (`ghostex/<8hex>` or the `ghostex/<slug>` it was
renamed to). Unlike `deleteWorktreeProject` it does not require the worktree to
be a registered project, because Sidebar V2 never registers one.
*/
pub(crate) async fn remove_session_worktree(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let context = resolve_project_worktree_operation_context(state, params)?;
    let force = params.get("force").and_then(Value::as_bool) == Some(true);
    let requested = normalize_existing_directory_path(
        params.get("worktreePath"),
        "worktreePath",
        &state.paths.home_dir,
    )?;
    let requested = normalize_project_path_for_comparison(&requested);
    let requested_key = canonical_worktree_path_key(&requested);
    let selected = project_worktree_options(&context)
        .await?
        .into_iter()
        .find(|option| canonical_worktree_path_key(&option.path) == requested_key)
        .ok_or_else(|| {
            DomainStateError::bad_request("worktreePath is not a worktree of this project.")
        })?;
    /*
    CDXC:SidebarV2Worktrees 2026-07-29:
    A checkout that is ALSO a registered project belongs to the V1 worktree
    project flow: deleting it here would remove the folder while the project row,
    its sessions and its own delete/merge affordances kept pointing at it. V2
    only ever owns worktrees it created as session attributes, so this is a
    refusal with a pointer at the flow that does own the checkout.

    The comparison runs through the same resolved-path key the worktree lookup
    above uses, not `selected.is_registered`: that flag compares the worktree
    list's REAL paths against registered paths as the user typed them, so a
    project registered through a symlink would read as unregistered here.
    */
    let is_registered_project = context
        .projects
        .iter()
        .filter_map(|project| project.get("path").and_then(Value::as_str))
        .any(|path| canonical_worktree_path_key(path) == requested_key);
    if is_registered_project {
        return Err(DomainStateError::bad_request(
            "This worktree is registered as its own project. Delete it from the project list instead.",
        )
        .into());
    }

    let mut status_params = Map::new();
    status_params.insert(
        "action".to_string(),
        Value::String("statusPorcelain".to_string()),
    );
    status_params.insert(
        "projectId".to_string(),
        Value::String(context.source_project_id.clone()),
    );
    status_params.insert("worktreePath".to_string(), Value::String(requested.clone()));
    let status = dispatch_worktree_path_operation(
        "/api/runGitAction",
        &status_params,
        context.projects.clone(),
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
    let dirty = has_porcelain_status_changes(
        status
            .get("stdout")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if dirty && !force {
        return Ok(json!({
            "dirty": true,
            "removed": false,
            "warnings": [WORKTREE_SESSION_DIRTY_WARNING],
        }));
    }

    let branch = worktree_session_branch_for_path(&context, &requested)
        .await?
        .or_else(|| normalize_branch_name(&selected.branch));
    let mut remove_params = Map::new();
    remove_params.insert("worktreePath".to_string(), Value::String(requested.clone()));
    if dirty || force {
        remove_params.insert("force".to_string(), Value::Bool(true));
    }
    let remove = run_project_worktree_action(
        &context.projects,
        "remove",
        &context.parent_path,
        remove_params,
    )
    .await?;
    if exit_code(&remove) != 0 {
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&remove, "git worktree remove failed."),
            scope_rejection: false,
        }
        .into());
    }

    let mut warnings = Vec::new();
    if let Some(branch) =
        branch.filter(|branch| worktree_sessions::is_managed_worktree_branch(branch))
    {
        let mut branch_params = Map::new();
        branch_params.insert("branch".to_string(), Value::String(branch));
        let action = if force {
            "deleteLocalBranchForce"
        } else {
            "deleteLocalBranch"
        };
        let deleted = run_project_git_action(
            &context.projects,
            action,
            &context.parent_path,
            branch_params,
        )
        .await?;
        if exit_code(&deleted) != 0 {
            warnings.push(json!(
                "The worktree was removed, but its branch could not be deleted."
            ));
        }
    }
    let prune =
        run_project_worktree_action(&context.projects, "prune", &context.parent_path, Map::new())
            .await?;
    if exit_code(&prune) != 0 {
        warnings.push(json!(
            "The worktree was removed, but stale worktree records could not be pruned."
        ));
    }
    schedule_worktree_session_project_delta(state, &context.source_project_id)?;
    Ok(json!({
        "dirty": dirty,
        "removed": true,
        "warnings": warnings,
    }))
}

pub(crate) fn schedule_worktree_session_project_delta(
    state: &AppState,
    project_id: &str,
) -> std::result::Result<(), ProjectWorktreeOperationError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    schedule_presentation_project_delta(state, &db, &repository, project_id, "projectUpdated")
        .map_err(Into::into)
}
