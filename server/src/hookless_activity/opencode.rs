//! OpenCode 2's own service event stream (`/api/event`), turned into the events Ghostex's OpenCode
//! TUI plugin sends.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! OpenCode 2 runs every conversation in its shared background service, which streams `session.execution.started` and `.succeeded` / `.failed` / `.interrupted` for each run, `permission.asked` / `.replied` for approvals and `form.created` / `.replied` / `.cancelled` for questions, all carrying the conversation's `sessionID`. That is the busy / idle / needs-input state the TUI plugin polls from the same service. Which conversation a TUI shows is the TUI's own route and never reaches the service, so the plugin's SessionStart (the TUI opening another conversation) has no counterpart here. Events sent while the stream is down are lost by OpenCode's contract.
//! SEE-ALSO: server/src/session_chat_opencode/events.rs (the one stream reader, which hands every event here), server/src/agent_hooks/install/opencode_tui.js (the plugin being compared).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::comparison::ActivityEvent;
use crate::server::{read_runtime_text, read_session_text};

pub(crate) const OBSERVABLE_EVENTS: &[&str] = &[
    "SessionBusy",
    "Stop",
    "PermissionRequest",
    "AskUserQuestion",
];

static ENABLED: AtomicBool = AtomicBool::new(false);
/// OpenCode conversation id → the Ghostex session bound to it.
static BOUND: LazyLock<Mutex<HashMap<String, (String, String)>>> = LazyLock::new(Default::default);
static QUEUE: LazyLock<Mutex<Vec<ActivityEvent>>> = LazyLock::new(Default::default);
/// Approvals asked within the last `ASK_HOLD`, by request id, with whether one was released.
static ASKS: LazyLock<Mutex<HashMap<String, (ActivityEvent, bool)>>> =
    LazyLock::new(Default::default);
/// An OpenCode TUI started with `--auto` (Ghostex's accept-all launch flag) answers each approval
/// itself within milliseconds; one still open after this long waits on the user.
const ASK_HOLD: Duration = Duration::from_secs(1);
/// Stream events the comparison dropped, by why: `unbound:<type>` for a conversation no running
/// Ghostex session is bound to (child sessions, other clients), `autoAnswered` for an approval
/// answered before it could be seen.
static DROPPED: LazyLock<Mutex<HashMap<String, u64>>> = LazyLock::new(Default::default);

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

/// Called by the OpenCode stream reader for every event it receives.
pub(crate) fn observe_stream_event(kind: &str, data: &Value) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let name = match kind {
        "session.execution.started" | "permission.replied" | "form.replied" | "form.cancelled" => {
            "SessionBusy"
        }
        "session.execution.succeeded"
        | "session.execution.failed"
        | "session.execution.interrupted" => "Stop",
        "permission.asked" => "PermissionRequest",
        "form.created" => "AskUserQuestion",
        _ => return,
    };
    // `form.created` carries the whole form, its conversation inside it.
    let Some(conversation) = data["sessionID"]
        .as_str()
        .or_else(|| data["form"]["sessionID"].as_str())
    else {
        return;
    };
    let Some((project_id, session_id)) = lock(&BOUND).get(conversation).cloned() else {
        *lock(&DROPPED).entry(format!("unbound:{kind}")).or_default() += 1;
        return;
    };
    let event = ActivityEvent::new(
        &project_id,
        &session_id,
        "opencode",
        name,
        Some(conversation.to_string()),
    )
    .with("streamEvent", json!(kind));
    match kind {
        "permission.asked" => {
            if let Some(id) = data["id"].as_str() {
                lock(&ASKS).insert(id.to_string(), (event, false));
                return;
            }
        }
        "permission.replied" => {
            if let Some((_, released)) = data["requestID"]
                .as_str()
                .and_then(|id| lock(&ASKS).remove(id))
            {
                if !released {
                    // Answered before anyone could see it: neither the ask nor its answer happened
                    // for the user.
                    *lock(&DROPPED)
                        .entry("autoAnswered".to_string())
                        .or_default() += 1;
                    return;
                }
            }
        }
        _ => {}
    }
    lock(&QUEUE).push(event);
}

/// Releases approvals that stayed open past `ASK_HOLD`.
fn release_asks() {
    let now = Instant::now();
    let mut asks = lock(&ASKS);
    let mut queue = lock(&QUEUE);
    for (event, released) in asks.values_mut() {
        if !*released && now.saturating_duration_since(event.at) >= ASK_HOLD {
            *released = true;
            queue.push(event.clone());
        }
    }
    // An answer the stream lost leaves its ask behind; forget it after a while.
    asks.retain(|_, (event, _)| {
        now.saturating_duration_since(event.at) < Duration::from_secs(3600)
    });
}

/// Binds the running OpenCode sessions' conversations and starts the stream when one runs.
pub(super) fn refresh(sessions: &[Value], enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    let mut bound = HashMap::new();
    if enabled {
        for session in sessions {
            if read_session_text(session, "agentId").as_deref() != Some("opencode") {
                continue;
            }
            if let (Some(project_id), Some(session_id), Some(conversation)) = (
                read_session_text(session, "projectId"),
                read_session_text(session, "sessionId"),
                read_runtime_text(session, "agentSessionId"),
            ) {
                bound.insert(conversation, (project_id, session_id));
            }
        }
    }
    let any = !bound.is_empty();
    *lock(&BOUND) = bound;
    if any {
        crate::session_chat_opencode::events::ensure_started();
    }
}

pub(super) fn drain() -> Vec<ActivityEvent> {
    release_asks();
    std::mem::take(&mut *lock(&QUEUE))
}

pub(super) fn dropped_counts() -> Value {
    json!(*lock(&DROPPED))
}

pub(super) fn reset() {
    lock(&DROPPED).clear();
    lock(&QUEUE).clear();
    lock(&ASKS).clear();
}
