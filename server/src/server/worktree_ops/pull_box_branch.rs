use super::*;

use ghostex_gx_protocol::cloud_box::{CloudBox, PullBranchResult};

/*
CDXC:CloudBoxes 2026-11-02 DECISION:
User (Q1014 "push branch"): work leaves a box as a branch the agent pushes; the box chip's Pull
brings that branch to this computer. It lands the way Ghostex keeps any other branch of a
project: as a worktree project beside the project (`<project>-<branch slug>`), cut from
`origin/<branch>` like Work Mode's chosen branch (chosen_branch.rs), or, when a worktree already
has the branch, that worktree fast-forwarded. The project's own checkout never switches branch.
A branch not pushed yet is refused with a reason instead of making an empty worktree.
SEE-ALSO: apps/desktop/src/app/render/workarea_header/box_chip.rs (Pull),
server/src/cloud_boxes/box_mode/prepare_session.rs (the branch and the push setup).
*/

fn failure(error: ProjectWorktreeOperationError) -> DomainStateError {
    DomainStateError::bad_request(super::draft_worktree::user_message(error))
}

/// The session's branch on the box: the one named (by its id on the box or its home draft's id),
/// else the first session with a branch.
fn box_branch(row: &CloudBox, session_id: Option<&str>) -> Option<String> {
    let named = session_id.and_then(|id| {
        row.sessions.iter().find(|session| {
            session.session_id == id || session.home_session_id.as_deref() == Some(id)
        })
    });
    named
        .and_then(|session| session.branch.clone())
        .or_else(|| {
            row.sessions
                .iter()
                .find_map(|session| session.branch.clone())
        })
        .filter(|branch| !branch.trim().is_empty())
}

/// `<project folder>-<branch slug>` beside the project, with `-2`, `-3`… when taken.
async fn free_worktree_path(
    context: &ProjectWorktreeOperationContext,
    branch: &str,
) -> std::result::Result<String, ProjectWorktreeOperationError> {
    let source_path = Path::new(&context.source_path);
    let parent_directory = source_path.parent().unwrap_or_else(|| Path::new("/"));
    let folder = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let segment = branch.rsplit('/').next().unwrap_or(branch);
    let slug = match worktree_sessions::worktree_rename_folder_slug(segment) {
        slug if slug.is_empty() => worktree_sessions::create_temp_branch_suffix(),
        slug => slug,
    };
    for attempt in 0..GXSERVER_WORKTREE_SESSION_UNIQUE_TARGET_ATTEMPTS {
        let suffix = if attempt == 0 {
            slug.clone()
        } else {
            format!("{slug}-{}", attempt + 1)
        };
        let path = path_to_string(
            &parent_directory.join(worktree_sessions::worktree_directory_name(folder, &suffix)),
        );
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

/// `pullBranch`.
pub(crate) async fn pull_box_branch(
    state: &AppState,
    row: &CloudBox,
    session_id: Option<&str>,
) -> std::result::Result<PullBranchResult, DomainStateError> {
    let branch = box_branch(row, session_id).ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{} has no session on its own branch yet, so there is nothing to pull.",
            row.name
        ))
    })?;
    let project_id = row.home_project_id.clone().ok_or_else(|| {
        DomainStateError::bad_request(format!(
            "{} has no project on this computer to pull into.",
            row.name
        ))
    })?;
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(project_id));
    let context = resolve_project_worktree_operation_context(state, &params).map_err(failure)?;

    // Only what the agent pushed can come over: fetch the branch from origin first.
    let repository = context.parent_path.clone();
    let fetch_branch = branch.clone();
    let fetched = tokio::task::spawn_blocking(move || {
        let refspec = format!("+refs/heads/{fetch_branch}:refs/remotes/origin/{fetch_branch}");
        worktree_sessions::worktree_git_succeeds(
            &repository,
            &["fetch", "origin", &refspec],
            worktree_sessions::WORKTREE_FETCH_COMMAND_TIMEOUT,
        )
    })
    .await
    .unwrap_or(false);
    if !fetched {
        return Err(DomainStateError::bad_request(format!(
            "{branch} isn't on the project's remote yet. Ask the agent on {} to push it, then Pull again.",
            row.name
        )));
    }

    let existing = project_worktree_options(&context)
        .await
        .map_err(failure)?
        .into_iter()
        .find(|option| option.branch == branch);
    let (path, created) = match existing {
        Some(option) => (option.path, false),
        None => {
            let path = free_worktree_path(&context, &branch)
                .await
                .map_err(failure)?;
            let repository = context.parent_path.clone();
            let probe = branch.clone();
            let local = tokio::task::spawn_blocking(move || {
                worktree_sessions::worktree_branch_exists(&repository, &probe)
            })
            .await
            .unwrap_or(false);
            let mut create_params = Map::new();
            create_params.insert("worktreePath".to_string(), Value::String(path.clone()));
            if local {
                // `git worktree add <path> <branch>` checks the existing branch out.
                create_params.insert("baseRef".to_string(), Value::String(branch.clone()));
            } else {
                create_params.insert("branch".to_string(), Value::String(branch.clone()));
                create_params.insert(
                    "baseRef".to_string(),
                    Value::String(format!("origin/{branch}")),
                );
            }
            let create = run_project_worktree_action(
                &context.projects,
                "create",
                &context.source_path,
                create_params,
            )
            .await
            .map_err(failure)?;
            if exit_code(&create) != 0 {
                return Err(DomainStateError::bad_request(operation_failure_message(
                    &create,
                    "git worktree add failed.",
                )));
            }
            (path, true)
        }
    };

    // Up to date with what the box pushed; a local branch with its own commits is left alone.
    let merge_path = path.clone();
    let remote = format!("origin/{branch}");
    let fast_forwarded = tokio::task::spawn_blocking(move || {
        worktree_sessions::worktree_git_succeeds(
            &merge_path,
            &["merge", "--ff-only", &remote],
            worktree_sessions::WORKTREE_FETCH_COMMAND_TIMEOUT,
        )
    })
    .await
    .unwrap_or(false);
    let note = (!fast_forwarded).then(|| {
        format!("{branch} here has commits the box's branch doesn't, so it was not updated.")
    });

    let parent_name = context
        .parent_project
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("Project");
    let slug = worktree_sessions::worktree_rename_folder_slug(
        branch.rsplit('/').next().unwrap_or(&branch),
    );
    let project = register_project_worktree_path(
        state,
        &path,
        &format!("{parent_name}-{slug}"),
        "projectAdded",
    )
    .map_err(failure)?;
    if created {
        prepare_registered_worktree_project(state, &project, Some(&context.source_project_id))
            .await
            .map_err(failure)?;
    }
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Ok(PullBranchResult {
        branch,
        project_id,
        path,
        created,
        note,
    })
}
