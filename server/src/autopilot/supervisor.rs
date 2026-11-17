//! Which session is Autopilot's supervisor and which project is its "Autopilot" project, for the
//! readers that hold only a project row or a session's ids (the sidebar presentation, the chat
//! frame, the title rules). The judge (server/src/server/autopilot_judge.rs) starts the supervisor
//! and writes its record here.
//!
//! CDXC:Autopilot 2026-11-18 DECISION:
//! Q87, "A. Hidden, opened from Autopilot": the Autopilot project starts hidden, the same as a project the user hid (Show hidden in the sidebar still lists it), and Settings › Autopilot and the Autopilot pill's menu get "Open supervisor" once a supervisor session exists. gxserver marks the project (`autopilotSupervisorProject` on its presentation row, by its folder, so the very first row a client sees carries it) and every sidebar hides it once into its own hidden list; a project the user unhides afterwards stays shown. The chat frame's `autopilot.supervisor` names the session for the pill's row.
//! SEE-ALSO: packages/gx-core/src/sidebar_ui/store.rs (`hide_once`), packages/gx-chat-core/src/extras/autopilot.rs (the pill's row), apps/desktop/src/app/window/settings_modal/tabs/autopilot.rs (Settings' button).

use std::path::{Path, PathBuf};
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

/// Whether this project row is the Autopilot project: the one whose folder holds the supervisor's
/// notes (or the one the record names).
pub fn is_supervisor_project(project: &Value) -> bool {
    let (dir, canonical) = notes_dirs();
    if let Some(path) = project.get("path").and_then(Value::as_str) {
        let path = Path::new(path);
        if path == dir.as_path() || canonical.as_deref() == Some(path) {
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
