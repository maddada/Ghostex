//! CDXC:SessionChat 2026-10-17 WHY:
//! OpenCode's message API returns a reply's text and reasoning only once each part ends, so a chat built on it alone shows a long reply all at once (GitHub #239). Its `/api/event` stream carries the deltas as they are written, and it says when a followed conversation changed, so chats refetch on change instead of reading every message twice a second. Events sent while the stream is down are lost by contract, so every (re)connect marks all chats stale.

use super::{error, Client};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        LazyLock, Mutex, Once,
    },
    time::Duration,
};

static CONNECTED: AtomicBool = AtomicBool::new(false);
static START: Once = Once::new();
/// Bumped whenever the stream (re)connects or drops, which makes every chat refetch.
static GENERATION: AtomicU64 = AtomicU64::new(0);
static FOLLOWED: LazyLock<Mutex<HashMap<String, Followed>>> = LazyLock::new(Default::default);

/// Kept apart from the conversation cache so the stream reader never waits on a refetch:
/// OpenCode fails a stream whose reader falls behind.
#[derive(Default)]
struct Followed {
    changes: u64,
    live: LiveText,
    live_changed: bool,
}

fn followed() -> std::sync::MutexGuard<'static, HashMap<String, Followed>> {
    FOLLOWED.lock().unwrap_or_else(|e| e.into_inner())
}

/// Starts reporting events for `id`, and returns a counter that changes whenever OpenCode
/// reports a change to it.
pub(crate) fn follow(id: &str) -> u64 {
    let changes = followed().entry(id.to_string()).or_default().changes;
    GENERATION.load(Ordering::Relaxed).wrapping_add(changes)
}

pub(crate) fn forget(id: &str) {
    followed().remove(id);
}

/// The streamed text to show, and whether it changed since the last call.
pub(crate) fn live(id: &str, completed: Option<&[Value]>) -> (LiveText, bool) {
    let mut map = followed();
    let Some(entry) = map.get_mut(id) else {
        return (LiveText::default(), false);
    };
    if let Some(messages) = completed {
        entry.live.prune(messages);
    }
    let changed = std::mem::take(&mut entry.live_changed);
    (entry.live.clone(), changed)
}

/// Streamed text of parts the message API still returns empty, keyed by the assistant
/// message and the part's position among that message's text (or reasoning) parts.
#[derive(Default, Clone)]
pub(crate) struct LiveText {
    parts: HashMap<(String, bool, u64), String>,
}

impl LiveText {
    pub(crate) fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }

    fn append(&mut self, message: &str, reasoning: bool, ordinal: u64, delta: &str) {
        self.parts
            .entry((message.to_string(), reasoning, ordinal))
            .or_default()
            .push_str(delta);
    }

    /// Drops the text of messages the API now returns complete.
    fn prune(&mut self, messages: &[Value]) {
        self.parts.retain(|(message, _, _), _| {
            !messages.iter().any(|row| {
                row["id"].as_str() == Some(message.as_str()) && !row["time"]["completed"].is_null()
            })
        });
    }

    /// Fills the empty text and reasoning parts of in-progress replies with what has streamed.
    pub(crate) fn apply(&self, messages: &[Value]) -> Vec<Value> {
        messages
            .iter()
            .map(|row| {
                let Some(id) = row["id"].as_str().filter(|_| row["type"] == "assistant") else {
                    return row.clone();
                };
                if !self.parts.keys().any(|(message, _, _)| message == id) {
                    return row.clone();
                }
                let mut row = row.clone();
                let mut ordinals = [0u64; 2];
                for part in row["content"].as_array_mut().into_iter().flatten() {
                    let reasoning = match part["type"].as_str() {
                        Some("text") => false,
                        Some("reasoning") => true,
                        _ => continue,
                    };
                    let ordinal = ordinals[reasoning as usize];
                    ordinals[reasoning as usize] += 1;
                    if part["text"].as_str().is_some_and(str::is_empty) {
                        if let Some(text) = self.parts.get(&(id.to_string(), reasoning, ordinal)) {
                            part["text"] = Value::String(text.clone());
                        }
                    }
                }
                row
            })
            .collect()
    }
}

pub(crate) fn connected() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

pub(crate) fn ensure_started() {
    START.call_once(|| {
        let _ = std::thread::Builder::new()
            .name("opencode-events".into())
            .spawn(run);
    });
}

fn run() {
    let mut delay = Duration::from_millis(250);
    loop {
        let result = Client::discover().and_then(|client| read_stream(&client));
        if CONNECTED.swap(false, Ordering::Relaxed) {
            delay = Duration::from_millis(250);
            GENERATION.fetch_add(1, Ordering::Relaxed);
        }
        drop(result);
        // The service restarts on upgrade and may not be running yet; keep trying quietly.
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_secs(5));
    }
}

fn read_stream(client: &Client) -> Result<(), crate::domain::DomainStateError> {
    let response = client.event_stream()?;
    CONNECTED.store(true, Ordering::Relaxed);
    GENERATION.fetch_add(1, Ordering::Relaxed);
    let mut reader = BufReader::new(response.into_body().into_reader());
    let mut line = String::new();
    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|_| error("The OpenCode event stream closed."))?
            == 0
        {
            return Err(error("The OpenCode event stream closed."));
        }
        if let Some(data) = line.strip_prefix("data:") {
            if let Ok(event) = serde_json::from_str::<Value>(data.trim()) {
                handle(&event);
            }
        }
    }
}

fn handle(event: &Value) {
    let Some(kind) = event["type"].as_str() else {
        return;
    };
    let data = &event["data"];
    crate::hookless_activity::observe_opencode_stream_event(kind, data);
    let Some(session) = data["sessionID"].as_str() else {
        return;
    };
    let delta = match kind {
        "session.text.delta" => Some(false),
        "session.reasoning.delta" => Some(true),
        _ => None,
    };
    let mut map = followed();
    let Some(entry) = map.get_mut(session) else {
        return;
    };
    match delta {
        Some(reasoning) => {
            if let (Some(message), Some(ordinal), Some(text)) = (
                data["assistantMessageID"].as_str(),
                data["ordinal"].as_u64(),
                data["delta"].as_str(),
            ) {
                entry.live.append(message, reasoning, ordinal, text);
                entry.live_changed = true;
            }
        }
        None => entry.changes = entry.changes.wrapping_add(1),
    }
}
