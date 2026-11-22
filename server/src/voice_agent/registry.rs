//! The open voice conversations, by id, so a client can reattach to one and announcements reach
//! all of them.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::conversation::Input;
use super::provider::Provider;

struct Entry {
    tx: mpsc::UnboundedSender<Input>,
    provider: Provider,
    attached: bool,
    /// The call's scope as clients see it (`focus.rs`): `null` for all sessions.
    focus: Value,
}

fn conversations() -> &'static Mutex<HashMap<String, Entry>> {
    static CONVERSATIONS: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    CONVERSATIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn insert(id: &str, tx: mpsc::UnboundedSender<Input>, provider: Provider) {
    if let Ok(mut map) = conversations().lock() {
        map.insert(
            id.to_string(),
            Entry {
                tx,
                provider,
                attached: false,
                focus: Value::Null,
            },
        );
    }
}

pub(super) fn remove(id: &str) {
    if let Ok(mut map) = conversations().lock() {
        map.remove(id);
    }
}

pub(super) fn sender(id: &str) -> Option<mpsc::UnboundedSender<Input>> {
    conversations()
        .lock()
        .ok()?
        .get(id)
        .map(|entry| entry.tx.clone())
}

pub(super) fn set_attached(id: &str, attached: bool) {
    if let Ok(mut map) = conversations().lock() {
        if let Some(entry) = map.get_mut(id) {
            entry.attached = attached;
        }
    }
}

pub(super) fn set_focus(id: &str, focus: Value) {
    if let Ok(mut map) = conversations().lock() {
        if let Some(entry) = map.get_mut(id) {
            entry.focus = focus;
        }
    }
}

pub(super) fn count() -> usize {
    conversations().lock().map(|map| map.len()).unwrap_or(0)
}

pub(super) fn senders() -> Vec<mpsc::UnboundedSender<Input>> {
    conversations()
        .lock()
        .map(|map| map.values().map(|entry| entry.tx.clone()).collect())
        .unwrap_or_default()
}

/// The open conversations for `readVoiceAgent`.
pub(super) fn summary() -> Value {
    let list: Vec<Value> = conversations()
        .lock()
        .map(|map| {
            map.iter()
                .map(|(id, entry)| {
                    json!({
                        "conversationId": id,
                        "provider": entry.provider.id(),
                        "attached": entry.attached,
                        "focus": entry.focus,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(list)
}
