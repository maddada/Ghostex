//! Which workspace a project belongs to (`launchSettings.workspaceId`) and moving a project between
//! workspaces.

use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};

use super::store::*;

/// The workspace id stored on a project; `None` means the default workspace.
pub(crate) fn stored_project_workspace_id(project: &Value) -> Option<&str> {
    project
        .get("launchSettings")
        .and_then(|settings| settings.get("workspaceId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// Whether a project shows in every workspace: the Ghostex config folder's project, where every
/// Help chat (and the Cloud Boxes setup chat) lives, until the user moves it to a workspace.
///
/// CDXC:Workspaces 2026-11-20 DECISION:
/// User: "it doesn't matter if I move it to Personal or Work, it's always shown in the Work workspace" (the config folder's project, holding Chief of Staff and a Grok Bot chat); Move to workspace must stick for it. Supersedes "always in every workspace" (2026-10-09, kept below): it shows in every workspace only while it has no stored `workspaceId`, and Move to workspace stores one even for the default workspace (`place_project_in_workspace`). Help opened from another window still never moves it; once it has a workspace, a Help chat opens in the window showing that workspace (workspace_windows/session_routing.rs). It was shown in Work as a project of its own because Main Hub lifts its chats into the home project only in a window that shows the home (gx-core `lift_into_home`).
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: the Help-chat project must not flip workspaces each time Help is opened from a different
/// window; make it visible in every workspace. It is recognised by its folder (the Ghostex config
/// folder the desktop roots it at), so no client has to mark it and installs that already have it
/// need no migration; its own `workspaceId` still decides its Linear key and Claude account.
pub(crate) fn project_in_every_workspace(project: &Value) -> bool {
    is_config_folder_project(project) && stored_project_workspace_id(project).is_none()
}

/// Whether `project` is rooted at the Ghostex config folder (Ask Ghostex AI's project), wherever
/// it is placed.
pub(crate) fn is_config_folder_project(project: &Value) -> bool {
    static CONFIG_DIR: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let config_dir = CONFIG_DIR.get_or_init(|| {
        comparable_path(
            &ghostex_paths::GhostexPaths::resolve()
                .config_dir
                .to_string_lossy(),
        )
    });
    !config_dir.is_empty()
        && project
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|path| comparable_path(path) == *config_dir)
}

fn comparable_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    let path = path.trim_end_matches('/');
    if cfg!(windows) || cfg!(target_os = "macos") {
        path.to_lowercase()
    } else {
        path.to_string()
    }
}

/// The workspace a project belongs to: a worktree project follows its parent checkout, and an
/// id the workspaces document no longer has reads as the default workspace.
pub(crate) fn project_workspace_id(
    workspaces: &Value,
    project: &Value,
    projects: &[Value],
) -> String {
    let parent = project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
        .and_then(|parent_id| {
            projects.iter().find(|candidate| {
                candidate.get("projectId").and_then(Value::as_str) == Some(parent_id)
            })
        });
    let id = stored_project_workspace_id(parent.unwrap_or(project))
        .or_else(|| stored_project_workspace_id(project));
    match id {
        Some(id) if workspace_exists(workspaces, id) => id.to_string(),
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    }
}

/// Writes `workspaceId` (dropped for the default workspace) into the project's `launchSettings`.
/// Returns the updated project, or `None` when nothing changed. The Work-workspace default for
/// Work mode it used to write is gone with the switch (CDXC:WorkMode 2026-10-24 in
/// server/src/work_mode/project.rs); a stored `workMode` is left as it is.
pub(crate) fn place_project_in_workspace(
    repository: &DomainRepository<'_>,
    project: &Value,
    workspace_id: &str,
) -> Result<Option<Value>, DomainStateError> {
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut launch_settings = project
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let before = launch_settings.clone();
    // The config folder's project keeps an explicit default id: without one it shows in every
    // workspace (`project_in_every_workspace`), so a move to Personal would not stick.
    if workspace_id == DEFAULT_WORKSPACE_ID && !is_config_folder_project(project) {
        launch_settings.remove("workspaceId");
    } else {
        launch_settings.insert("workspaceId".into(), json!(workspace_id));
    }
    if launch_settings == before {
        return Ok(None);
    }
    let mut update = Map::new();
    update.insert("projectId".into(), json!(project_id));
    update.insert("launchSettings".into(), Value::Object(launch_settings));
    repository.update_project(&update).map(Some)
}

/// Moves a project and its worktree projects into a workspace. Returns the ids of every project
/// row that changed.
pub(crate) fn move_project_to_workspace(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
    project_id: &str,
    workspace_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    if !workspace_exists(workspaces, workspace_id) {
        return Err(DomainStateError::bad_request("No such workspace."));
    }
    let projects = repository.list_projects()?;
    let project = projects
        .iter()
        .find(|project| project.get("projectId").and_then(Value::as_str) == Some(project_id))
        .ok_or_else(|| {
            DomainStateError::bad_request(format!("No project matched \"{project_id}\"."))
        })?;
    let mut changed = Vec::new();
    let worktrees = projects.iter().filter(|candidate| {
        candidate
            .get("worktree")
            .and_then(|worktree| worktree.get("parentProjectId"))
            .and_then(Value::as_str)
            == Some(project_id)
    });
    for moving in std::iter::once(project).chain(worktrees) {
        if place_project_in_workspace(repository, moving, workspace_id)?.is_some() {
            if let Some(id) = moving.get("projectId").and_then(Value::as_str) {
                changed.push(id.to_string());
            }
        }
    }
    Ok(changed)
}

/// Re-places every project in a workspace (after it was deleted and its projects fell back to the
/// default workspace), which drops their stale workspace ids. Returns the ids of the project rows
/// that changed.
pub(crate) fn reapply_workspace_defaults(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
    workspace_id: &str,
) -> Result<Vec<String>, DomainStateError> {
    let projects = repository.list_projects()?;
    let mut changed = Vec::new();
    for project in &projects {
        // A project whose stored workspace was deleted resolves to the default workspace here,
        // and placing it there drops the stale id.
        if project_workspace_id(workspaces, project, &projects) != workspace_id {
            continue;
        }
        if place_project_in_workspace(repository, project, workspace_id)?.is_some() {
            if let Some(id) = project.get("projectId").and_then(Value::as_str) {
                changed.push(id.to_string());
            }
        }
    }
    Ok(changed)
}

/// Puts a project a client just added into the workspace it names (`params.workspaceId`, the
/// window's workspace, or a workspace name from the CLI), so it shows in the window it was added
/// from; with none it stays in the default workspace. A worktree project of a registered checkout
/// takes its parent's workspace instead. Returns the project as it is now.
///
/// CDXC:Workspaces 2026-10-09 DECISION:
/// User: new projects land in the window's workspace from every path (Add Project, a clone from
/// Add Project, worktree projects, and `ghostex` CLI project creation, which uses Personal when it
/// has no window to go by).
pub(crate) fn place_added_project(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    if project_parent_id(&project).is_some() {
        return place_worktree_project(repository, db, project);
    }
    // With Workspaces off a window has no workspace to add to, so the project stays in the
    // default one.
    if !super::workspaces_feature_enabled() {
        return Ok(project);
    }
    let Some(reference) = params
        .get("workspaceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
    else {
        return Ok(project);
    };
    let workspaces = read_sidebar_workspaces(db)?;
    let Some(workspace_id) = find_workspace_id(&workspaces, reference) else {
        return Ok(project);
    };
    Ok(place_project_in_workspace(repository, &project, &workspace_id)?.unwrap_or(project))
}

fn project_parent_id(project: &Value) -> Option<&str> {
    project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
}

/// A worktree project takes its parent checkout's workspace. Returns the project as it is now.
pub(crate) fn place_worktree_project(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
) -> Result<Value, DomainStateError> {
    let Some(parent) = project_parent_id(&project)
        .map(|parent_id| repository.get_project(parent_id))
        .transpose()?
        .flatten()
    else {
        return Ok(project);
    };
    let workspaces = read_sidebar_workspaces(db)?;
    let workspace_id = match stored_project_workspace_id(&parent) {
        Some(id) if workspace_exists(&workspaces, id) => id.to_string(),
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    };
    let mut launch_settings = project
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let before = launch_settings.clone();
    if workspace_id == DEFAULT_WORKSPACE_ID {
        launch_settings.remove("workspaceId");
    } else {
        launch_settings.insert("workspaceId".into(), json!(workspace_id));
    }
    if launch_settings == before {
        return Ok(project);
    }
    let mut update = Map::new();
    update.insert(
        "projectId".into(),
        project.get("projectId").cloned().unwrap_or(Value::Null),
    );
    update.insert("launchSettings".into(), Value::Object(launch_settings));
    repository.update_project(&update)
}

/// The project `reference` names: a project id, else a project name (any case, when only one
/// project has it), else a folder path inside a project (the deepest project wins).
pub(crate) fn resolve_project_reference(
    repository: &DomainRepository<'_>,
    reference: &str,
) -> Result<String, DomainStateError> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err(DomainStateError::bad_request("Pass the project."));
    }
    let projects = repository.list_projects()?;
    let id_of = |project: &Value| {
        project
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    if let Some(id) = projects.iter().filter_map(id_of).find(|id| id == reference) {
        return Ok(id);
    }
    let named: Vec<&Value> = projects
        .iter()
        .filter(|project| {
            project
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.trim().eq_ignore_ascii_case(reference))
        })
        .collect();
    match named.as_slice() {
        [only] => {
            return id_of(only).ok_or_else(|| DomainStateError::bad_request("No project id."))
        }
        [_, _, ..] => {
            return Err(DomainStateError::bad_request(format!(
                "Several projects are named \"{reference}\"; pass its id or folder."
            )))
        }
        [] => {}
    }
    let mut params = Map::new();
    params.insert("path".into(), json!(reference));
    let project =
        crate::work_mode::resolve_work_mode_project(repository, &params).map_err(|_| {
            DomainStateError::bad_request(format!("No project matched \"{reference}\"."))
        })?;
    id_of(&project).ok_or_else(|| DomainStateError::bad_request("No project id."))
}

/// The Claude account the workspace of `project` picked for its agents, if any. None while
/// Workspaces is off: launches use the account they would without workspaces.
pub(crate) fn project_claude_account_id(
    db: &rusqlite::Connection,
    project: &Value,
) -> Result<Option<String>, DomainStateError> {
    if !super::workspaces_feature_enabled() {
        return Ok(None);
    }
    let workspaces = read_sidebar_workspaces(db)?;
    let project_workspace = match project
        .get("worktree")
        .and_then(|worktree| worktree.get("parentProjectId"))
        .and_then(Value::as_str)
    {
        // A worktree project follows its parent checkout's workspace.
        Some(parent_id) => {
            let parent = db
                .query_row(
                    "SELECT launchSettingsJson FROM projects WHERE projectId = ?1",
                    [parent_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .ok()
                .flatten()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            parent
                .as_ref()
                .and_then(|settings| settings.get("workspaceId"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| stored_project_workspace_id(project).map(str::to_string))
        }
        None => stored_project_workspace_id(project).map(str::to_string),
    };
    let workspace_id = match project_workspace {
        Some(id) if workspace_exists(&workspaces, &id) => id,
        _ => DEFAULT_WORKSPACE_ID.to_string(),
    };
    Ok(workspace_claude_account_id(&workspaces, &workspace_id))
}
