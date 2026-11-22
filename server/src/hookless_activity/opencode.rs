//! OpenCode 2's own service event stream (`/api/event`), turned into the events Ghostex's OpenCode
//! TUI plugin sends.
//!
//! CDXC:AgentHooks 2026-11-01 WHY:
//! OpenCode 2 runs every conversation in its shared background service, which streams `session.execution.started` and `.succeeded` / `.failed` / `.interrupted` for each run, `permission.asked` / `.replied` for approvals and `form.created` / `.replied` / `.cancelled` for questions, all carrying the conversation's `sessionID`. That is the busy / idle / needs-input state the TUI plugin polls from the same service. Events sent while the stream is down are lost by OpenCode's contract.
//!
//! CDXC:AgentHooks 2026-11-11 DECISION:
//! The user wants Ghostex to stop installing hooks and plugins into Claude, Codex and OpenCode and to track them from what each agent exposes itself, and picked "Claude, then OpenCode and Codex". Two things only the plugin gave: the cards and the conversation the TUI shows. `permission.asked` and `form.created` carry the whole request or form, so the observer builds the same card the chat builds from the service (session_chat_opencode/prompts.rs) and stores it through the hook path, and clears it when the request is answered. Which conversation a TUI shows is the TUI's own route and never reaches the service (`session.viewed` fires only for an unread conversation while the terminal has focus, and from any client), and OpenCode 2.2.7 keeps no per-TUI route on disk. So the conversation at TUI start is the one Ghostex pins with `opencode --session <id>` and creates before launch (agents/opencode_session_id.rs, session_chat_opencode/conversation.rs), and a switch inside the TUI is read from that terminal's own OSC 7501 records, which OpenCode writes for each root conversation it shows that works or waits on the user: when they name exactly one conversation the stream reported and not the bound one, the session is moved to it. A switch to a conversation that stays idle is read from the terminal title OpenCode sets (`OC | <title>`, clipped past 40 characters): when exactly one top-level conversation in the session's folder carries that title, the session moves to it with the plugin's own SessionStart, and the records still decide whenever they name a conversation. The plugin keeps deciding where it is installed: these events drive a session only where it is missing (agent_drive.rs).
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

/// A Ghostex session → the conversation title its OpenCode TUI last put in the terminal title
/// (`OC | <title>`; empty for `OpenCode`, a conversation with no title yet or the home screen).
static TITLES: LazyLock<Mutex<HashMap<(String, String), String>>> = LazyLock::new(Default::default);
/// A Ghostex session → the terminal title last matched against the service, so each title is
/// looked up once.
static TITLES_MATCHED: LazyLock<Mutex<HashMap<(String, String), String>>> =
    LazyLock::new(Default::default);
/// OpenCode 2.2.7 clips a terminal title's conversation title past this many characters to 37 and
/// an ellipsis.
const TITLE_CLIP: usize = 40;

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

/// Called with every terminal title a session's terminal sets.
pub(crate) fn observe_terminal_title(project_id: &str, session_id: &str, title: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let title = title.trim();
    let shown = match title.strip_prefix("OC | ") {
        Some(shown) => shown.trim().to_string(),
        None if title == "OpenCode" => String::new(),
        None => return,
    };
    lock(&TITLES).insert((project_id.to_string(), session_id.to_string()), shown);
}

/// Whether `title` (a conversation's) is what a terminal title shows as `shown`.
fn title_shows(title: &str, shown: &str) -> bool {
    match shown.strip_suffix('\u{2026}') {
        Some(prefix) if title.chars().count() > TITLE_CLIP => title.starts_with(prefix),
        Some(_) => false,
        None => title == shown,
    }
}

/// Two spellings of one folder: the same path, or on macOS and Windows the same path in another
/// case (the TUI reports its working directory as the system spells it).
fn same_folder(a: &str, b: &str) -> bool {
    let canonical = |path: &str| {
        std::fs::canonicalize(path)
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|_| path.trim_end_matches('/').to_string())
    };
    let (a, b) = (canonical(a), canonical(b));
    a == b || (cfg!(any(target_os = "macos", windows)) && a.eq_ignore_ascii_case(&b))
}

/// Moves a session to the conversation its TUI's terminal title names, when the TUI shows one no
/// OSC 7501 record names (an idle conversation): exactly one top-level conversation in the
/// session's folder must carry that title, and no other session may hold it.
fn follow_title_switches(sessions: &[Value], bound: &mut HashMap<String, (String, String)>) {
    if cfg!(test) {
        return;
    }
    let mut client = None;
    for session in sessions {
        let (Some(project_id), Some(session_id), Some(conversation), Some(folder)) = (
            read_session_text(session, "projectId"),
            read_session_text(session, "sessionId"),
            read_runtime_text(session, "agentSessionId"),
            read_session_text(session, "cwd"),
        ) else {
            continue;
        };
        let key = (project_id.clone(), session_id.clone());
        let Some(shown) = lock(&TITLES)
            .get(&key)
            .cloned()
            .filter(|shown| !shown.is_empty())
        else {
            continue;
        };
        if lock(&TITLES_MATCHED).get(&key) == Some(&shown) {
            continue;
        }
        // The terminal's records name the conversation whenever one works or waits; they decide.
        if crate::program_status::program_status_entry(&project_id, &session_id).is_some_and(
            |entry| {
                entry.records.iter().any(|record| {
                    !record.id.is_empty() && record.app.as_deref() == Some("opencode")
                })
            },
        ) {
            continue;
        }
        lock(&TITLES_MATCHED).insert(key.clone(), shown.clone());
        if client.is_none() {
            client = crate::session_chat_opencode::Client::discover().ok();
        }
        let Some(client) = client.as_ref() else {
            return;
        };
        let search = shown.trim_end_matches('\u{2026}');
        let encoded: String = url::form_urlencoded::byte_serialize(search.as_bytes()).collect();
        let Ok(response) = client.request(
            "GET",
            &format!(
                "/api/session?limit=50&order=desc&parentID=null&archived=false&search={encoded}"
            ),
            None,
        ) else {
            continue;
        };
        let matches: Vec<&str> = response["data"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|row| {
                row["title"]
                    .as_str()
                    .is_some_and(|title| title_shows(title, &shown))
                    && row["location"]["directory"]
                        .as_str()
                        .is_some_and(|directory| same_folder(directory, &folder))
            })
            .filter_map(|row| row["id"].as_str())
            .filter(|id| crate::session_chat_opencode::safe_id(id))
            .collect();
        if matches.contains(&conversation.as_str()) {
            continue;
        }
        let [next] = matches[..] else {
            continue;
        };
        if bound.contains_key(next) {
            continue;
        }
        bound.remove(&conversation);
        bound.insert(next.to_string(), (project_id.clone(), session_id.clone()));
        // The TUI only opened it: the plugin's SessionStart, which moves the session and sets no
        // activity.
        let mut params = Map::new();
        params.insert("agentSessionId".to_string(), json!(next));
        lock(&QUEUE).push(
            ActivityEvent::new(
                &project_id,
                &session_id,
                "opencode",
                "SessionStart",
                Some(next.to_string()),
            )
            .with("streamEvent", json!("terminalTitle"))
            .with(HOOK_PARAMS, Value::Object(params)),
        );
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
        follow_title_switches(sessions, &mut bound);
    } else {
        lock(&CONVERSATIONS).clear();
        lock(&MOVED).clear();
        lock(&TITLES).clear();
        lock(&TITLES_MATCHED).clear();
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

/// The sessions whose conversation the observer follows while OpenCode's stream is connected.
pub(super) fn followed() -> Vec<(String, String)> {
    if !crate::session_chat_opencode::events::connected() {
        return Vec::new();
    }
    lock(&BOUND).values().cloned().collect()
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
