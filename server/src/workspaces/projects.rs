//! Which workspace a project belongs to (`launchSettings.workspaceId`) and moving a project between
//! workspaces.

use serde_json::{Map, Value, json};

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

/// Set in the config folder's project's `launchSettings` once the user moves it to a workspace:
/// from then on it stays there instead of following the home project.
const WORKSPACE_MOVED_KEY: &str = "workspaceMoved";

/// Whether `project` is rooted at the Ghostex config folder: Ask Ghostex AI's project, where every
/// Help chat (and the Cloud Boxes setup chat) lives.
///
/// CDXC:Workspaces 2026-11-20 DECISION:
/// User: "it doesn't matter if I move it to Personal or Work, it's always shown in the Work workspace" (this project, holding Chief of Staff and Help chats), then: it follows the home project's workspace unless the user moved it, so a Work window never draws it as "ghostex" and its chats stay under Main Hub where the home project lives. Supersedes "visible in every workspace" (2026-10-09): that drew it as a project of its own in every window whose workspace does not hold the home, because Main Hub lifts its chats only into a home it shows (gx-core `lift_into_home`). It is stored, not derived, like a worktree that follows its parent: a move of the home project moves it too (`move_project_to_workspace`), adding it places it with the home (`place_added_project`), and gxserver lines it up at start (`config_project_follows_home_at_startup`), so every client and older phones place it from its `workspaceId` alone. A Move to workspace of this project itself marks it (`workspaceMoved`) and it stays where it was put. Help opened from another window never moves it; its chat opens in the window showing its workspace (workspace_windows/session_routing.rs). It is recognised by its folder, so no client has to mark it.
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
/// `moved_by_user` marks the config folder's project as placed by the user (`workspaceMoved`).
pub(crate) fn place_project_in_workspace(
    repository: &DomainRepository<'_>,
    project: &Value,
    workspace_id: &str,
    moved_by_user: bool,
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
    if workspace_id == DEFAULT_WORKSPACE_ID {
        launch_settings.remove("workspaceId");
    } else {
        launch_settings.insert("workspaceId".into(), json!(workspace_id));
    }
    if moved_by_user && is_config_folder_project(project) {
        launch_settings.insert(WORKSPACE_MOVED_KEY.into(), json!(true));
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
        if place_project_in_workspace(repository, moving, workspace_id, true)?.is_some() {
            if let Some(id) = moving.get("projectId").and_then(Value::as_str) {
                changed.push(id.to_string());
            }
        }
    }
    // The config folder's project follows the home project it was not moved away from.
    if crate::chief::is_home_project(project) {
        for id in config_project_follow_home(repository, workspaces)? {
            if !changed.contains(&id) {
                changed.push(id);
            }
        }
    }
    Ok(changed)
}

/// Places the config folder's project (`is_config_folder_project`) in the home project's workspace
/// (the default one while there is no home project), unless the user moved it there themselves
/// (`workspaceMoved`). Returns its id when its row changed.
pub(crate) fn config_project_follow_home(
    repository: &DomainRepository<'_>,
    workspaces: &Value,
) -> Result<Vec<String>, DomainStateError> {
    let projects = repository.list_projects_without_session_history()?;
    let Some(config) = projects
        .iter()
        .find(|project| is_config_folder_project(project))
    else {
        return Ok(Vec::new());
    };
    let moved_by_user = config
        .get("launchSettings")
        .and_then(|settings| settings.get(WORKSPACE_MOVED_KEY))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if moved_by_user {
        return Ok(Vec::new());
    }
    let workspace_id = projects
        .iter()
        .find(|project| crate::chief::is_home_project(project))
        .map(|home| project_workspace_id(workspaces, home, &projects))
        .unwrap_or_else(|| DEFAULT_WORKSPACE_ID.to_string());
    if place_project_in_workspace(repository, config, &workspace_id, false)?.is_none() {
        return Ok(Vec::new());
    }
    Ok(config
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .into_iter()
        .collect())
}

/// Lines the config folder's project up with the home project once at start, for a home that was
/// added or moved while it could not follow (CDXC:Workspaces 2026-11-20 on
/// `is_config_folder_project`). Runs before the first snapshot, so nothing is published.
pub(crate) fn config_project_follows_home_at_startup(
    paths: &crate::paths::GxserverPaths,
    server_id: &str,
) {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, server_id);
    if let Ok(workspaces) = read_sidebar_workspaces(&db) {
        let _ = config_project_follow_home(&repository, &workspaces);
    }
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
        if place_project_in_workspace(repository, project, workspace_id, false)?.is_some() {
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
    // The config folder's project goes where the home project is, and follows a home added now.
    let is_config = is_config_folder_project(&project);
    if is_config || crate::chief::is_home_project(&project) {
        let project = place_added_project_in_window(repository, db, project, params, is_config)?;
        let workspaces = read_sidebar_workspaces(db)?;
        let project_id = project.get("projectId").and_then(Value::as_str);
        let changed = config_project_follow_home(repository, &workspaces)?;
        return match project_id {
            Some(id) if changed.iter().any(|changed| changed == id) => {
                Ok(repository.get_project(id)?.unwrap_or(project))
            }
            _ => Ok(project),
        };
    }
    place_added_project_in_window(repository, db, project, params, false)
}

/// The window's workspace part of `place_added_project`; `skip` leaves the project where it is.
fn place_added_project_in_window(
    repository: &DomainRepository<'_>,
    db: &rusqlite::Connection,
    project: Value,
    params: &Map<String, Value>,
    skip: bool,
) -> Result<Value, DomainStateError> {
    if skip {
        return Ok(project);
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
    Ok(place_project_in_workspace(repository, &project, &workspace_id, false)?.unwrap_or(project))
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
            return id_of(only).ok_or_else(|| DomainStateError::bad_request("No project id."));
        }
        [_, _, ..] => {
            return Err(DomainStateError::bad_request(format!(
                "Several projects are named \"{reference}\"; pass its id or folder."
            )));
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
