//! The No project project and how a create names it.
//!
//! CDXC:Projects 2026-11-27 DECISION:
//! User (Q5260 C, Q5261 B, reversing the Oct 23 "no more chats" removal on purpose): every chat without a project lives in one "No project" project, one more project in the list, sorted and moved like the rest, and its chats start in the home folder (~, with the secret folders kept out, as Main Hub's chats did). Main Hub keeps Ask Ghostex AI, Autopilot, terminals and coordinators.
//! CDXC:Projects 2026-11-27 WHY:
//! A project is keyed by its folder and Main Hub is "the project at ~" (chief/home.rs), so a second project at ~ would make every lookup by folder ambiguous. No project is a Ghostex-made project at its own empty folder in gxserver's state (`<state>/no-project`), found by that folder like Autopilot's (autopilot/supervisor.rs), and a session created in it without a folder of its own starts in the home folder (domain/repository/session.rs), where the secret-folder guard applies as it does for every launch at ~ (agents/home_secret_folders.rs). The user may rename it; it is still found by its folder.
//! SEE-ALSO: server/src/presentation/session_projection.rs (`isNoProject`), ghostex_cli/sessions/mobile_summary.rs (the phone), packages/gx-core/src/sidebar_view/ (the sidebar), apps/mobile/app (the phone's New Chat).

use std::path::PathBuf;
use std::sync::OnceLock;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::server::{schedule_presentation_project_delta, AppState};

/// The name the project is created with.
pub(crate) const NO_PROJECT_NAME: &str = "No project";

/// What a create may pass as `projectId` (or `noProject: true`) to start in No project without
/// knowing its id: `ghostex create-agent --no-project`, a coordinator's `start-thread
/// --no-project`, the phone's New Chat. Not a gxserver project id (those are `P` and four
/// characters, ids.rs), so it can never name a real project.
pub(crate) const NO_PROJECT_ALIAS: &str = "no-project";

/// The project's folder: empty, made by Ghostex, never a repository.
pub(crate) fn no_project_folder(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("no-project")
}

fn folder_key(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    if cfg!(windows) {
        trimmed.replace('\\', "/").to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// This gxserver's No project folder as written and as the file system resolves it.
#[cfg(not(test))]
fn folder_keys() -> &'static Vec<String> {
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let folder = no_project_folder(&crate::paths::get_gxserver_paths(None));
        let mut keys = vec![folder_key(&folder.to_string_lossy())];
        if let Ok(real) = std::fs::canonicalize(&folder) {
            let real = folder_key(&real.to_string_lossy());
            if !keys.contains(&real) {
                keys.push(real);
            }
        }
        keys
    })
}

#[cfg(test)]
fn folder_keys() -> &'static Vec<String> {
    static KEYS: OnceLock<Vec<String>> = OnceLock::new();
    KEYS.get_or_init(|| vec!["/nonexistent/ghostex-no-project-test".to_string()])
}

/// Whether `project` is the No project project.
pub(crate) fn is_no_project_project(project: &Value) -> bool {
    project
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| folder_keys().contains(&folder_key(path)))
}

/// The No project project: the one already registered at its folder, or a new one added there.
pub(crate) fn ensure_no_project_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    if let Some(project) = repository
        .list_projects()?
        .into_iter()
        .find(is_no_project_project)
    {
        return Ok(project);
    }
    let folder = no_project_folder(&state.paths);
    std::fs::create_dir_all(&folder).map_err(|error| {
        DomainStateError::corrupt_state(format!(
            "Could not make the No project folder {}: {error}",
            folder.display()
        ))
    })?;
    let mut params = Map::new();
    params.insert("path".into(), json!(folder.to_string_lossy().to_string()));
    params.insert("name".into(), json!(NO_PROJECT_NAME));
    let project = repository.add_project_path(&params)?;
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::corrupt_state("Missing projectId."))?;
    let project = crate::workspaces::place_added_project(repository, db, project, &params)?;
    schedule_presentation_project_delta(state, db, repository, &project_id, "projectAdded")?;
    Ok(project)
}

/// A create that names No project by its alias (`projectId: "no-project"` or `noProject: true`)
/// gets its real id, the project made first when it is missing.
pub(crate) fn place_create_alias(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
    params: &mut Map<String, Value>,
) -> Result<(), DomainStateError> {
    let flagged = params.remove("noProject").and_then(|value| value.as_bool()) == Some(true);
    let aliased = params
        .get("projectId")
        .and_then(Value::as_str)
        .is_some_and(|id| id.trim() == NO_PROJECT_ALIAS);
    if !flagged && !aliased {
        return Ok(());
    }
    let project = ensure_no_project_project(state, db, repository)?;
    let project_id = project
        .get("projectId")
        .cloned()
        .ok_or_else(|| DomainStateError::corrupt_state("Missing projectId."))?;
    params.insert("projectId".to_string(), project_id);
    params.remove("projectPath");
    Ok(())
}

/// A create in No project without a folder of its own starts in the home folder. Set on the create
/// itself (not only on the stored row) so the launch plan's secret-folder guard sees ~
/// (agents/launch_plan.rs reads the create's `cwd` before the project's folder).
pub(crate) fn place_home_cwd(project: &Value, params: &mut Map<String, Value>) {
    let has_cwd = params
        .get("cwd")
        .and_then(Value::as_str)
        .is_some_and(|cwd| !cwd.trim().is_empty());
    if !has_cwd && is_no_project_project(project) {
        params.insert(
            "cwd".to_string(),
            json!(crate::resume_lookup::home_dir()
                .to_string_lossy()
                .to_string()),
        );
    }
}
