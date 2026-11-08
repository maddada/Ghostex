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
#[cfg(not(test))]
pub fn current() -> AutopilotConfig {
    read_config(&crate::paths::get_gxserver_paths(None))
}

/// Tests keep to the choices this process wrote, else the defaults: the default paths are this
/// Mac's own gxserver, and its saved Autopilot (on, answering approvals) wrote every approval
/// row already read and sent no push, so the OMP wait and push delivery tests failed on it.
#[cfg(test)]
pub fn current() -> AutopilotConfig {
    config::cached_or_default()
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

/// `autopilot` on a chat frame: `{ on, covered, offline }`. A coordinator's chat draws the switch
/// from `on`; every other chat draws its own pill while `on`, from `covered`; `offline` while this
/// computer is offline, when Autopilot waits (CDXC:OfflineHold in network_status).
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
            "offline": crate::network_status::is_offline(),
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

fn left_for_user_marks() -> &'static Mutex<HashSet<(String, String)>> {
    static LEFT: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
    LEFT.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Autopilot's rules gave up on a session (`Act::Park`): until it moves again, its red dot is the
/// user's and alerts (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs).
pub fn note_left_for_user(project_id: &str, session_id: &str) {
    left_for_user_marks()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert((project_id.to_string(), session_id.to_string()));
}

/// The session moved again (its stuck stretch ended): Autopilot heals its next one first.
pub fn clear_left_for_user(project_id: &str, session_id: &str) {
    left_for_user_marks()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&(project_id.to_string(), session_id.to_string()));
}

pub fn left_for_user(project_id: &str, session_id: &str) -> bool {
    left_for_user_marks()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .contains(&(project_id.to_string(), session_id.to_string()))
}

/// Whether Autopilot is healing this session's not-moving red dot, so it does not alert yet: the
/// switch covers it, its rules have not left it for the user, and the user did not take it back.
pub fn heals_stuck(project_id: &str, session_id: &str) -> bool {
    current().covers(
        project_id,
        remembered_main_project(project_id).as_deref(),
        session_id,
    ) && !left_for_user(project_id, session_id)
        && !user_has_it(
            project_id,
            session_id,
            chrono::Utc::now().timestamp_millis(),
        )
}

/// Whether Autopilot answers this session's approval cards itself: it covers the session, may
/// answer approvals, and on Everything settles every one (approves it, or declines a hard limit),
/// where Heal and decide leaves some for the user. Not while the user took the session back.
pub fn answers_approvals(project_id: &str, session_id: &str) -> bool {
    let config = current();
    config.answer_approvals
        && config.tier == Tier::Everything
        && config.covers(
            project_id,
            remembered_main_project(project_id).as_deref(),
            session_id,
        )
        && !user_has_it(
            project_id,
            session_id,
            chrono::Utc::now().timestamp_millis(),
        )
}
