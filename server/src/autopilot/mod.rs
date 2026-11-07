//! Autopilot: the "keep going no matter what" mode. gxserver keeps its choices (config.rs) and
//! its decision log (log.rs); the watcher that acts on stuck sessions is
//! server/src/server/autopilot_runtime.rs and its calls are server/src/server/autopilot_http.rs.

mod config;
mod log;
mod notes;

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::{json, Map, Value};

pub use config::{
    apply_patch, read_config, update_config, AutopilotConfig, HardLimits, Membership, Scope,
    SupervisorModel, Tier,
};
pub use log::{append as append_log, read_recent as read_log, summary, LogEntry, Outcome};
pub use notes::{ensure_notes, next_wake_number, notes_dir, write_wake};

/// The choices as last read or written; the paths of this gxserver once the runtime started.
pub fn current() -> AutopilotConfig {
    read_config(&crate::paths::get_gxserver_paths(None))
}

/// The project a worktree project was made from, as its row says (`worktree.parentProjectId`).
pub fn main_project_of(project: &Value) -> Option<String> {
    project
        .pointer("/worktree/parentProjectId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn parents() -> &'static Mutex<HashMap<String, String>> {
    static PARENTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    PARENTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Remembers a worktree project's main project, for readers that hold only a session's ids.
pub fn remember_project(project: &Value) {
    let Some(project_id) = project.get("projectId").and_then(Value::as_str) else {
        return;
    };
    let mut parents = parents().lock().unwrap_or_else(PoisonError::into_inner);
    match main_project_of(project) {
        Some(main) => {
            parents.insert(project_id.to_string(), main);
        }
        None => {
            parents.remove(project_id);
        }
    }
}

pub fn remembered_main_project(project_id: &str) -> Option<String> {
    parents()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(project_id)
        .cloned()
}

/// `autopilot` on a project's presentation row: whether Autopilot covers it (and its worktrees),
/// sent only while the switch is on and the scope goes by project, so the sidebar's
/// Advanced > Autopilot row shows only then.
pub fn project_presentation(project: &Value) -> Option<bool> {
    remember_project(project);
    let config = current();
    if !config.enabled || config.scope == Scope::Sessions || main_project_of(project).is_some() {
        return None;
    }
    let project_id = project.get("projectId").and_then(Value::as_str)?;
    Some(config.project_in(project_id, None))
}

/// `autopilot` on a chat frame: `{ on, covered }`. A coordinator's chat draws the switch from
/// `on`; every other chat draws its own pill while `on`, from `covered`.
pub fn insert_chat_autopilot(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let config = current();
    frame.insert(
        "autopilot".to_string(),
        json!({
            "on": config.enabled,
            "covered": config.covers(
                project_id,
                remembered_main_project(project_id).as_deref(),
                session_id,
            ),
        }),
    );
}

/// How long a session the user stopped by hand is left alone.
pub const HANDS_OFF_MS: i64 = 30 * 60_000;

fn hands_off() -> &'static Mutex<HashMap<(String, String), i64>> {
    static HANDS_OFF: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();
    HANDS_OFF.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A chat Stop or `ghostex interrupt-session-chat`: someone took the session back, so Autopilot
/// leaves it for `HANDS_OFF_MS`. Autopilot's own interrupts ride its send (`interrupt: true`) and
/// never come through here.
///
/// CDXC:Autopilot 2026-11-07 WHY:
/// A session the user stopped on purpose ends its turn mid-task, which is exactly what Autopilot carries on; without this it would start the agent again right after the user stopped it.
pub fn note_user_interrupt(project_id: &str, session_id: &str) {
    let key = (project_id.to_string(), session_id.to_string());
    hands_off()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(key, chrono::Utc::now().timestamp_millis());
}

/// Whether the user stopped this session by hand in the last `HANDS_OFF_MS`.
pub fn user_has_it(project_id: &str, session_id: &str, now: i64) -> bool {
    let mut map = hands_off().lock().unwrap_or_else(PoisonError::into_inner);
    map.retain(|_, at| now - *at < HANDS_OFF_MS);
    map.contains_key(&(project_id.to_string(), session_id.to_string()))
}

fn signals() -> &'static (Mutex<HashSet<(String, String)>>, tokio::sync::Notify) {
    static SIGNALS: OnceLock<(Mutex<HashSet<(String, String)>>, tokio::sync::Notify)> =
        OnceLock::new();
    SIGNALS.get_or_init(|| (Mutex::new(HashSet::new()), tokio::sync::Notify::new()))
}

/// An agent hook said a session's turn ended: the watcher looks at it soon, without waiting for
/// its sweep.
pub fn signal_turn_ended(project_id: &str, session_id: &str) {
    let (pending, notify) = signals();
    pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((project_id.to_string(), session_id.to_string()));
    notify.notify_one();
}

/// The sessions signalled since the last call.
pub fn take_signalled() -> HashSet<(String, String)> {
    std::mem::take(&mut *signals().0.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Waits for the next signal.
pub async fn signalled() {
    signals().1.notified().await;
}
