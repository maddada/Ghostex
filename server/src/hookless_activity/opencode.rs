//! OpenCode 2's own service event stream (`/api/event`), turned into the events Ghostex's OpenCode
//! TUI plugin sends.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! OpenCode 2 runs every conversation in its shared background service, which streams `session.execution.started` and `.succeeded` / `.failed` / `.interrupted` for each run, `permission.asked` / `.replied` for approvals and `form.created` / `.replied` / `.cancelled` for questions, all carrying the conversation's `sessionID`. That is the busy / idle / needs-input state the TUI plugin polls from the same service. Events sent while the stream is down are lost by OpenCode's contract.
//!
//! CDXC:AgentHooks 2026-11-11 DECISION:
//! The user wants Ghostex to stop installing hooks and plugins into Claude, Codex and OpenCode and to track them from what each agent exposes itself, and picked "Claude, then OpenCode and Codex". Two things only the plugin gave: the cards and the conversation the TUI shows. `permission.asked` and `form.created` carry the whole request or form, so the observer builds the same card the chat builds from the service (session_chat_opencode/prompts.rs) and stores it through the hook path, and clears it when the request is answered. Which conversation a TUI shows is the TUI's own route and never reaches the service (`session.viewed` fires only for an unread conversation while the terminal has focus, and from any client), and OpenCode 2.2.7 keeps no per-TUI route on disk. So the conversation at TUI start is the one Ghostex pins with `opencode --session <id>` and creates before launch (agents/opencode_session_id.rs, session_chat_opencode/conversation.rs), and a switch inside the TUI is read from that terminal's own OSC 7501 records, which OpenCode writes for each root conversation it shows that works or waits on the user: when they name exactly one conversation the stream reported and not the bound one, the session is moved to it. A switch to a conversation that stays idle is learned once it runs. The plugin keeps deciding where it is installed: these events drive a session only where it is missing (agent_drive.rs).
//! SEE-ALSO: server/src/session_chat_opencode/events.rs (the one stream reader, which hands every event here), server/src/agent_hooks/install/opencode_tui.js (the plugin being compared), server/src/program_status/ (the OSC 7501 records), server/src/agents/activity.rs (`OBSERVED_PROMPT`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use super::agent_drive::HOOK_PARAMS;
use super::comparison::ActivityEvent;
use crate::server::{read_runtime_text, read_session_text};

pub(crate) const OBSERVABLE_EVENTS: &[&str] = &[
    "SessionBusy",
    "Stop",
    "PermissionRequest",
    "AskUserQuestion",
];

/// The hook param carrying the card the observer built (a stored prompt's JSON), or `null` once
/// the conversation has none open.
pub(crate) const OBSERVED_PROMPT: &str = "observedSessionChatPrompt";

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
/// What the stream said about every conversation it reported, bound or not, so a session moved to
/// another conversation starts from where that one stands.
static CONVERSATIONS: LazyLock<Mutex<HashMap<String, Conversation>>> =
    LazyLock::new(Default::default);
/// A Ghostex session → the conversation its terminal's records last moved it to, until its row
/// says so too (an ingest that refused the move is not proposed again every pass).
static MOVED: LazyLock<Mutex<HashMap<(String, String), String>>> = LazyLock::new(Default::default);

#[derive(Default)]
struct Conversation {
    running: bool,
    /// Open approvals and forms, oldest first.
    open: Vec<Open>,
    seen: Option<Instant>,
}

struct Open {
    /// The approval's request id or the form's id.
    id: String,
    approval: bool,
    /// The stored prompt's JSON; `None` for a form the chat cannot ask (a browser sign-in step).
    card: Option<String>,
    /// An approval shows once it outlasts `ASK_HOLD`; a form shows at once.
    shown: bool,
}

impl Conversation {
    /// The card the chat shows for it: its first approval, else its first form, as
    /// `session_chat_opencode::interactive_prompt` picks from the service.
    fn card(&self) -> Value {
        let first = self
            .open
            .iter()
            .find(|open| open.approval && open.shown)
            .or_else(|| self.open.iter().find(|open| !open.approval));
        first
            .and_then(|open| open.card.clone())
            .map_or(Value::Null, Value::String)
    }

    /// The hook event standing for where it is now, for a session just moved to it.
    fn event_name(&self) -> Option<&'static str> {
        let first = self
            .open
            .iter()
            .find(|open| open.approval && open.shown)
            .or_else(|| self.open.iter().find(|open| !open.approval));
        match first {
            Some(open) if open.approval => Some("PermissionRequest"),
            Some(_) => Some("AskUserQuestion"),
            None if self.running => Some("SessionBusy"),
            None => None,
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn card_json(prompt: Option<crate::session_chat::SessionChatInteractivePrompt>) -> Option<String> {
    prompt.and_then(|prompt| serde_json::to_string(&prompt).ok())
}

/// Records what `kind` says about `conversation`, and returns the card it now shows.
fn note(conversation: &str, kind: &str, data: &Value) -> Value {
    let mut conversations = lock(&CONVERSATIONS);
    if conversations.len() > 512 {
        let now = Instant::now();
        conversations.retain(|_, state| {
            state
                .seen
                .is_some_and(|seen| now.saturating_duration_since(seen) < Duration::from_secs(3600))
        });
    }
    let state = conversations.entry(conversation.to_string()).or_default();
    state.seen = Some(Instant::now());
    match kind {
        "session.execution.started" => state.running = true,
        "session.execution.succeeded"
        | "session.execution.failed"
        | "session.execution.interrupted" => {
            state.running = false;
            state.open.clear();
        }
        "permission.asked" => {
            if let Some(id) = data["id"].as_str() {
                state.open.push(Open {
                    id: id.to_string(),
                    approval: true,
                    card: card_json(Some(crate::session_chat_opencode::approval_prompt(data))),
                    shown: false,
                });
            }
        }
        "form.created" => {
            let form = &data["form"];
            if let Some(id) = form["id"].as_str() {
                state.open.push(Open {
                    id: id.to_string(),
                    approval: false,
                    card: card_json(crate::session_chat_opencode::form_prompt(form)),
                    shown: true,
                });
            }
        }
        "permission.replied" | "form.replied" | "form.cancelled" => {
            if let Some(id) = data["requestID"].as_str().or_else(|| data["id"].as_str()) {
                state.open.retain(|open| open.id != id);
            }
        }
        _ => {}
    }
    state.card()
}

fn with_card(event: ActivityEvent, card: Value) -> ActivityEvent {
    let mut params = Map::new();
    params.insert(OBSERVED_PROMPT.to_string(), card);
    event.with(HOOK_PARAMS, Value::Object(params))
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
    let card = note(conversation, kind, data);
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
                // Its card goes with it when it is released.
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
    lock(&QUEUE).push(with_card(event, card));
}

/// Releases approvals that stayed open past `ASK_HOLD`, each with the card its conversation now
/// shows.
fn release_asks() {
    let now = Instant::now();
    let mut released = Vec::new();
    {
        let mut asks = lock(&ASKS);
        for (id, (event, done)) in asks.iter_mut() {
            if !*done && now.saturating_duration_since(event.at) >= ASK_HOLD {
                *done = true;
                released.push((id.clone(), event.clone()));
            }
        }
        // An answer the stream lost leaves its ask behind; forget it after a while.
        asks.retain(|_, (event, _)| {
            now.saturating_duration_since(event.at) < Duration::from_secs(3600)
        });
    }
    for (id, event) in released {
        let card = event
            .agent_session_id
            .as_deref()
            .and_then(|conversation| {
                let mut conversations = lock(&CONVERSATIONS);
                let state = conversations.get_mut(conversation)?;
                for open in state.open.iter_mut().filter(|open| open.id == id) {
                    open.shown = true;
                }
                Some(state.card())
            })
            .unwrap_or(Value::Null);
        lock(&QUEUE).push(with_card(event, card));
    }
}

/// Moves a session to the conversation its terminal shows now, read from the OSC 7501 records
/// OpenCode's TUI writes for each root conversation it shows that works or waits: when they name
/// exactly one conversation the stream reported, none of them the bound one, and no other session
/// holds it.
fn follow_terminal_switches() {
    let bound = lock(&BOUND).clone();
    for (conversation, (project_id, session_id)) in &bound {
        let key = (project_id.clone(), session_id.clone());
        let Some(entry) = crate::program_status::program_status_entry(project_id, session_id)
        else {
            continue;
        };
        let shown: Vec<&str> = entry
            .records
            .iter()
            .filter(|record| !record.id.is_empty() && record.app.as_deref() == Some("opencode"))
            .map(|record| record.id.as_str())
            .collect();
        if shown.contains(&conversation.as_str()) {
            lock(&MOVED).remove(&key);
            continue;
        }
        let candidates: Vec<&str> = {
            let conversations = lock(&CONVERSATIONS);
            shown
                .iter()
                .copied()
                .filter(|id| conversations.contains_key(*id) && !bound.contains_key(*id))
                .collect()
        };
        let [next] = candidates[..] else {
            continue;
        };
        if lock(&MOVED).get(&key).map(String::as_str) == Some(next) {
            continue;
        }
        let (name, card) = {
            let conversations = lock(&CONVERSATIONS);
            let state = &conversations[next];
            let record = entry.records.iter().find(|record| record.id == next);
            let from_record = match record.map(|record| record.state.as_str()) {
                Some("blocked") if record.and_then(|r| r.kind.as_deref()) == Some("permission") => {
                    "PermissionRequest"
                }
                Some("blocked") => "AskUserQuestion",
                _ => "SessionBusy",
            };
            (state.event_name().unwrap_or(from_record), state.card())
        };
        lock(&MOVED).insert(key, next.to_string());
        {
            let mut bound = lock(&BOUND);
            bound.remove(conversation);
            bound.insert(next.to_string(), (project_id.clone(), session_id.clone()));
        }
        let mut event = with_card(
            ActivityEvent::new(
                project_id,
                session_id,
                "opencode",
                name,
                Some(next.to_string()),
            )
            .with("streamEvent", json!("terminalSwitch")),
            card,
        );
        if let Some(Value::Object(params)) = event.payload.get_mut(HOOK_PARAMS) {
            params.insert("agentSessionId".to_string(), json!(next));
        }
        lock(&QUEUE).push(event);
    }
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
    } else {
        lock(&CONVERSATIONS).clear();
        lock(&MOVED).clear();
    }
    let any = !bound.is_empty();
    *lock(&BOUND) = bound;
    if any {
        crate::session_chat_opencode::events::ensure_started();
    }
}

/// Binds one conversation without starting the stream reader, as `refresh` would.
#[cfg(test)]
pub(super) fn bind_for_test(conversation: &str, project_id: &str, session_id: &str) {
    ENABLED.store(true, Ordering::Relaxed);
    lock(&BOUND).insert(
        conversation.to_string(),
        (project_id.to_string(), session_id.to_string()),
    );
}

pub(super) fn drain() -> Vec<ActivityEvent> {
    release_asks();
    follow_terminal_switches();
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
