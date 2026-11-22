//! What the supervisor last decided about each open thread, for everything else that shows or acts
//! on a thread's state: the Workers rows and the board (panel.rs, board.rs), the sidebar's
//! `coordinatorThreadState` (presentation.rs), `ghostex coordinator status` and Auto Sleep.
//!
//! CDXC:Coordinators 2026-11-06 WHY:
//! User, of the board at 05:33: "why is board not up to date?? what can we do to avoid this please?". Two running threads sat under To do as "parked" with a stale last report, one slept mid-command sat there too, and a thread idle for two hours with nothing pending did as well. The supervisor (server/coordinator_runtime.rs) already read the thread's transcript when hooks said it stopped, so it nudged a thread as "working" that the board, which classified from hooks alone, showed as parked. Hooks miss turns: after a restart or a wake they are reset to idle, and a turn a Ghostex nudge or a queued task notification starts can run without one. The supervisor now publishes its own verdict here every tick, with the line the thread is on and what an idle or sleeping thread still owes, and every surface reads it through `classify_thread_live` instead of deciding again from hooks.
//! SEE-ALSO: server/src/coordinators/idle_wait.rs (what an idle thread owes), server/src/coordinators/board.rs (where a thread lands).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::idle_wait::IdleWaitReason;
use super::records::SessionKey;
use super::state::{classify_thread_session, ThreadProgress, ThreadState};

/// One open thread as the supervisor last saw it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadLive {
    /// Its transcript shows a turn running although its hooks say it stopped.
    pub working: bool,
    /// What it said last in the turn it is on (its newest assistant words), while it works.
    pub activity: Option<String>,
    /// When its newest turn started (ms), while it works.
    pub turn_started_ms: Option<i64>,
    /// Why an idle or sleeping thread still owes work (idle_wait.rs), and since when (ms).
    pub unfinished: Option<(IdleWaitReason, i64)>,
}

fn live() -> &'static Mutex<HashMap<SessionKey, ThreadLive>> {
    static LIVE: OnceLock<Mutex<HashMap<SessionKey, ThreadLive>>> = OnceLock::new();
    LIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Replaces every thread's entry with the supervisor's latest tick.
pub fn replace_thread_live(next: HashMap<SessionKey, ThreadLive>) {
    if let Ok(mut current) = live().lock() {
        *current = next;
    }
}

/// The supervisor's last view of a thread, `None` for a thread it does not watch (resolved, or its
/// coordinator closed) or before its first tick.
pub fn thread_live(project_id: &str, session_id: &str) -> Option<ThreadLive> {
    live()
        .lock()
        .ok()?
        .get(&(project_id.to_string(), session_id.to_string()))
        .cloned()
}

fn session_key(session: &Value) -> Option<(String, String)> {
    let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    Some((text("projectId")?, text("sessionId")?))
}

/// `classify_thread_session` with the supervisor's transcript reading: a thread its transcript
/// shows mid-turn is working even when its hooks say it stopped. Everything but the supervisor
/// itself classifies through this; the supervisor reads the transcript on its own.
pub fn classify_thread_live(
    session: Option<&Value>,
    progress: ThreadProgress,
    now_iso: &str,
) -> ThreadState {
    let state = classify_thread_session(session, progress, now_iso, false);
    if state != ThreadState::Finished {
        return state;
    }
    let working = session
        .and_then(session_key)
        .and_then(|(project_id, session_id)| thread_live(&project_id, &session_id))
        .is_some_and(|live| live.working);
    if working {
        ThreadState::Working
    } else {
        state
    }
}

/// Whether Auto Sleep must leave a thread awake: it works by its transcript, or its last turn
/// left work it has not done (idle_wait.rs: a job's result to read, a task it stopped in the
/// middle of, a turn that never ended).
pub fn thread_owes_work(project_id: &str, session_id: &str) -> bool {
    thread_live(project_id, session_id)
        .is_some_and(|live| live.working || live.unfinished.is_some())
}
