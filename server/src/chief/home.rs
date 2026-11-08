//! The home: the project rooted at this computer's home folder (`~`, `C:\Users\<name>` on
//! Windows). Main Hub's row is its header on every client, its loose sessions (terminals,
//! agents, quick questions started there) are listed under it, and Chief's own chats run in it.
//!
//! CDXC:Chief 2026-11-08 DECISION:
//! User (Q1911, Q1912, Q1914, Q1916): "Chief is home": Main Hub's row becomes the header of the home project at ~ (the home folder itself, C:\Users\<name> on Windows), loose sessions live under it and Chief is its coordinator. The existing project at that folder ("madda", with its sessions, history and settings) is adopted as it is: a project is keyed by its folder, so the home is simply whichever project has the home folder's path, and one is added only when none has. Chief's own chat runs at ~ (against the recommendation to keep it in the config folder).
//! SEE-ALSO: server/src/presentation/session_projection.rs (`isHome`), packages/gx-core/src/sidebar_view/chief.rs (the home group under Chief's header), apps/mobile/app/src/components/sessions/ChiefRow.tsx.

use std::path::Path;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::{schedule_presentation_project_delta, AppState};

/// `path` with its trailing separators taken off, compared case-blind on Windows.
fn folder_key(path: &str) -> String {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    let trimmed = if trimmed.is_empty() {
        path.trim()
    } else {
        trimmed
    };
    if cfg!(windows) {
        trimmed.replace('\\', "/").to_lowercase()
    } else {
        trimmed.to_string()
    }
}

/// This computer's home folder as written in `HOME` and as its real path (`/home` may be a link),
/// read once: comparing every project on every snapshot must not touch the disk.
fn home_keys() -> &'static [String] {
    static KEYS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    KEYS.get_or_init(|| {
        let home = crate::resume_lookup::home_dir();
        let mut keys = vec![folder_key(&home.to_string_lossy())];
        if let Ok(real) = std::fs::canonicalize(&home) {
            let real = folder_key(&real.to_string_lossy());
            if !keys.contains(&real) {
                keys.push(real);
            }
        }
        keys
    })
}

/// Whether `path` is this computer's home folder.
pub(crate) fn is_home_folder(path: &str) -> bool {
    let key = folder_key(path);
    home_keys().contains(&key)
}

/// Whether `project` is the home project.
pub(crate) fn is_home_project(project: &Value) -> bool {
    project
        .get("path")
        .and_then(Value::as_str)
        .is_some_and(|path| Path::new(path).is_absolute() && is_home_folder(path))
}

/// The home project: the one already registered at the home folder (adopted as it is), or a new
/// one added there.
pub(super) fn home_project(
    state: &AppState,
    db: &Connection,
    repository: &DomainRepository<'_>,
) -> Result<Value, DomainStateError> {
    if let Some(project) = repository
        .list_projects()?
        .into_iter()
        .find(is_home_project)
    {
        return Ok(project);
    }
    let home = crate::resume_lookup::home_dir();
    let mut params = Map::new();
    params.insert("path".into(), json!(home.to_string_lossy().to_string()));
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
