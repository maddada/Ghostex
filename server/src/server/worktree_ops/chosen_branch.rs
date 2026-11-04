use super::*;

/// Where a chosen branch comes from when the new worktree is cut.
enum ChosenBranchSource {
    /// It exists in this repository: check it out as it is.
    Local,
    /// Only origin has it: create it locally from `origin/<branch>`, tracking it.
    Origin,
    /// Nobody has it yet: create it from the default branch (or the requested base).
    New,
}

/*
CDXC:WorkMode 2026-10-09 DECISION:
User: a ticket's branch is Linear's own `branchName` for the issue
(`yahia/spx-1245-copy-link`), or `<user>/<number>-<slug>` for a GitHub issue,
and in work mode every piece of work gets its own worktree. So a worktree
session can be started on a branch the caller names: the branch is reused when
it already exists here or on origin, and created from the default branch
otherwise. A branch already checked out in one of this project's worktrees is
not checked out twice (git refuses that): the session joins that worktree.
*/
pub(crate) async fn prepare_chosen_branch_checkout(
    context: &ProjectWorktreeOperationContext,
    request: &WorktreeSessionCreateRequest,
    branch: &str,
) -> std::result::Result<PreparedWorktreeCheckout, ProjectWorktreeOperationError> {
    if let Some(existing) = project_worktree_options(context)
        .await?
        .into_iter()
        .find(|option| option.branch == branch)
    {
        return Ok(PreparedWorktreeCheckout {
            branch: branch.to_string(),
            created: false,
            path: existing.path,
        });
    }

    let repository_path = context.parent_path.clone();
    let probe_branch = branch.to_string();
    let source =
        tokio::task::spawn_blocking(move || chosen_branch_source(&repository_path, &probe_branch))
            .await
            .unwrap_or(ChosenBranchSource::New);

    let path = reserve_chosen_branch_worktree_path(context, branch).await?;
    let mut create_params = Map::new();
    create_params.insert("worktreePath".to_string(), Value::String(path.clone()));
    match source {
        ChosenBranchSource::Local => {
            // `git worktree add <path> <branch>` checks the existing branch out.
            create_params.insert("baseRef".to_string(), Value::String(branch.to_string()));
        }
        ChosenBranchSource::Origin => {
            create_params.insert("branch".to_string(), Value::String(branch.to_string()));
            create_params.insert(
                "baseRef".to_string(),
                Value::String(format!("origin/{branch}")),
            );
        }
        ChosenBranchSource::New => {
            let base_ref = resolve_worktree_session_base_ref(context, request).await?;
            create_params.insert("branch".to_string(), Value::String(branch.to_string()));
            create_params.insert("baseRef".to_string(), Value::String(base_ref));
            create_params.insert("noTrack".to_string(), Value::Bool(true));
        }
    }
    let prepared = PreparedWorktreeCheckout {
        branch: branch.to_string(),
        created: true,
        path,
    };
    let create = run_project_worktree_action(
        &context.projects,
        "create",
        &context.source_path,
        create_params,
    )
    .await?;
    if exit_code(&create) != 0 {
        // Removes a half-registered checkout; a ticket branch is never deleted by it.
        rollback_worktree_session_checkout(context, &prepared).await;
        return Err(TypedOperationError {
            code: "badRequest",
            details: None,
            message: operation_failure_message(&create, "git worktree add failed."),
            scope_rejection: false,
        }
        .into());
    }
    if let Err(error) = run_worktree_session_setup_command(context, &prepared.path).await {
        rollback_worktree_session_checkout(context, &prepared).await;
        return Err(error);
    }
    Ok(prepared)
}

fn chosen_branch_source(repository_path: &str, branch: &str) -> ChosenBranchSource {
    if worktree_sessions::worktree_branch_exists(repository_path, branch) {
        return ChosenBranchSource::Local;
    }
    // Fetching just this branch answers "is it on origin?" and makes `origin/<branch>` current;
    // a branch origin does not have and an unreachable origin both read as "new".
    let refspec = format!("+refs/heads/{branch}:refs/remotes/origin/{branch}");
    if worktree_sessions::worktree_git_succeeds(
        repository_path,
        &["fetch", "origin", &refspec],
        worktree_sessions::WORKTREE_FETCH_COMMAND_TIMEOUT,
    ) {
        ChosenBranchSource::Origin
    } else {
        ChosenBranchSource::New
    }
}

/// `<project folder>-<last branch segment>` next to the project (`shortpoint-spx-1245-copy-link`),
/// with `-2`, `-3`… when that folder is taken.
async fn reserve_chosen_branch_worktree_path(
    context: &ProjectWorktreeOperationContext,
    branch: &str,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let project_folder_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let segment = branch.rsplit('/').next().unwrap_or(branch);
    let slug = worktree_sessions::worktree_rename_folder_slug(segment);
    let slug = if slug.is_empty() {
        worktree_sessions::create_temp_branch_suffix()
    } else {
        slug
    };
    for attempt in 0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS {
        let suffix = if attempt == 0 {
            slug.clone()
        } else {
            format!("{slug}-{}", attempt + 1)
        };
        let path = path_to_string(&parent_directory.join(
            worktree_sessions::worktree_directory_name(project_folder_name, &suffix),
        ));
        let mut path_params = Map::new();
        path_params.insert("worktreePath".to_string(), Value::String(path.clone()));
        let path_check = run_project_worktree_action(
            &context.projects,
            "pathExists",
            &context.source_path,
            path_params,
        )
        .await?;
        if exit_code(&path_check) != 0 {
            return Ok(path);
        }
    }
    Err(
        DomainStateError::bad_request("Could not find a free folder for this branch's worktree.")
            .into(),
    )
}
