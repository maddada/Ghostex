//! Which session is Autopilot's supervisor and which project is its "Autopilot" project, for the
//! readers that hold only a project row or a session's ids (the sidebar presentation, the chat
//! frame, the title rules). The judge (server/src/server/autopilot_judge.rs) starts the supervisor
//! and writes its record here.
//!
//! CDXC:Autopilot 2026-11-24 DECISION:
//! User: "move those to be in main hub instead please". Autopilot's system project (`autopilotSupervisorProject` on its presentation row, by its folder or supervisor record) is never drawn as a normal project in the sidebar (even with Show hidden on); while Main Hub is on, every connected computer's Autopilot supervisor session is lifted into Main Hub's Autopilot section (with its computer badge for remote computers). While Main Hub is turned off, the system project stays hidden from the projects list and the supervisor stays reachable from Settings > Autopilot ("Open supervisor"), the chat Autopilot pill ("Open supervisor"), and notifications. Supersedes the 2026-11-18 `hide_once` rule that kept Autopilot as a hidden project group.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/chief.rs (`lift_into_home`), packages/gx-core/src/sidebar_view/merged.rs, packages/gx-core/src/sidebar_view/assemble.rs, packages/gx-chat-core/src/extras/autopilot.rs (the pill's row), apps/desktop/src/app/window/settings_modal/tabs/autopilot.rs (Settings' button).

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::Value;

use crate::paths::GxserverPaths;

const SUPERVISOR_FILE: &str = "autopilot-supervisor.json";
/// The supervisor's session title. Ghostex's own, so the agent's auto-title never replaces it
/// (`keeps_ghostex_title`).
pub const SUPERVISOR_TITLE: &str = "Autopilot supervisor";

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorRecord {
    pub project_id: String,
    pub session_id: String,
    pub agent: String,
}

/// The record as last read or written by this process (`None` inside: there is none).
fn cache() -> &'static Mutex<Option<Option<SupervisorRecord>>> {
    static CACHE: OnceLock<Mutex<Option<Option<SupervisorRecord>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The supervisor's record. gxserver alone writes it, so it is read from disk once.
pub fn read_record(paths: &GxserverPaths) -> Option<SupervisorRecord> {
    let mut cached = cache().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(record) = cached.as_ref() {
        return record.clone();
    }
    let record = std::fs::read_to_string(paths.app_state_dir.join(SUPERVISOR_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok());
    *cached = Some(record.clone());
    record
}

pub fn write_record(paths: &GxserverPaths, record: &SupervisorRecord) {
    let _ = std::fs::write(
        paths.app_state_dir.join(SUPERVISOR_FILE),
        serde_json::to_string_pretty(record).unwrap_or_default(),
    );
    *cache().lock().unwrap_or_else(PoisonError::into_inner) = Some(Some(record.clone()));
}

#[cfg(not(test))]
fn own_paths() -> &'static GxserverPaths {
    static PATHS: OnceLock<GxserverPaths> = OnceLock::new();
    PATHS.get_or_init(|| crate::paths::get_gxserver_paths(None))
}

/// The supervisor's session on this gxserver, when it has one.
#[cfg(not(test))]
pub fn supervisor_session() -> Option<(String, String)> {
    read_record(own_paths()).map(|record| (record.project_id, record.session_id))
}

/// Tests never read this computer's own supervisor.
#[cfg(test)]
pub fn supervisor_session() -> Option<(String, String)> {
    None
}

/// The notes folder as written and as the file system resolves it.
#[cfg(not(test))]
fn notes_dirs() -> &'static (PathBuf, Option<PathBuf>) {
    static DIRS: OnceLock<(PathBuf, Option<PathBuf>)> = OnceLock::new();
    DIRS.get_or_init(|| {
        let dir = super::notes_dir(own_paths());
        let canonical = std::fs::canonicalize(&dir).ok();
        (dir, canonical)
    })
}

#[cfg(test)]
fn notes_dirs() -> &'static (PathBuf, Option<PathBuf>) {
    static DIRS: OnceLock<(PathBuf, Option<PathBuf>)> = OnceLock::new();
    DIRS.get_or_init(|| (PathBuf::from("/nonexistent/ghostex-autopilot-test"), None))
}

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

/// Whether this project row is the Autopilot project: the one whose folder holds the supervisor's
/// notes (or the one the record names).
pub fn is_supervisor_project(project: &Value) -> bool {
    let (dir, canonical) = notes_dirs();
    if let Some(path) = project.get("path").and_then(Value::as_str) {
        let key = folder_key(path);
        if key == folder_key(&dir.to_string_lossy())
            || canonical
                .as_ref()
                .is_some_and(|resolved| key == folder_key(&resolved.to_string_lossy()))
        {
            return true;
        }
    }
    project
        .get("projectId")
        .and_then(Value::as_str)
        .zip(supervisor_session())
        .is_some_and(|(project_id, (supervisor_project, _))| project_id == supervisor_project)
}

/// True for the supervisor's session: its title is Ghostex's, never the agent's own auto-title.
///
/// CDXC:Autopilot 2026-11-18 WHY:
/// Claude named the supervisor "Ghostex Autopilot wake 1" from its first message, so the "Autopilot supervisor" it was created with was lost. Like a coordinator's given title (`keeps_its_given_title`), the agent's terminal title and metadata title no longer replace it; a rename the user asks for still applies.
pub fn keeps_ghostex_title(session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    let (Some(project_id), Some(session_id)) = (text("projectId"), text("sessionId")) else {
        return false;
    };
    supervisor_session().is_some_and(|(p, s)| p == project_id && s == session_id)
}
