//! Per event socket: leaves the heavy side fields a chat state frame repeats unchanged out of it.
//!
//! CDXC:SessionChat 2026-11-10 WHY:
//! Snapshot, replaced and state frames all carry the session's side state (CDXC:SessionChat
//! 2026-08-21 in session_chat_follower/registry.rs), so every state frame repeated it in full. On
//! the user's iPhone a coordinator chat sent a 164 KB state frame about every 6 s while nothing
//! visible changed (a 124 KB thread list, a 37 KB draft), about 1.3 MB a minute, all parsed by the
//! phone's chat core. A state frame now leaves out each of these fields whose value is the one this
//! socket last sent for that chat. Every chat core since 2026-10-01 (11.2.1 included) reads a field
//! missing from a state frame as unchanged (`gx-chat-core` session/apply.rs), so nothing changes
//! for any client; snapshot and replaced frames always carry them in full and reset the memory.
//! SEE-ALSO: packages/gx-chat-core/src/session/apply.rs (`apply_state_frame`), server/src/server/ws.rs.
//!
//! CDXC:SessionChat 2026-11-10 WHY (thread list patches):
//! With the repeats gone, a coordinator chat's state frame was still 155 KB on the user's iPhone,
//! 152 KB of it the thread list, which changes whenever any running thread writes (each thread's
//! `detail` is its latest message). A socket whose client subscribed with
//! `coordinatorThreadsPatch: true` gets `coordinatorThreadsPatch` instead: the changed values only,
//! by path, against the list this socket last carried for that chat, named by its `revision`. The
//! chat core rebuilds the whole list (packages/gx-chat-core/src/session/thread_patch.rs) and
//! subscribes again when a patch does not fit. Clients that do not ask (11.2.1 phones, the desktop)
//! get the whole list as before.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

use crate::events::EventPayload;

/// The side fields a state frame may leave out when unchanged: each is read with `if let Some` in
/// `gx-chat-core`'s state frame apply (checked against 11.2.1's core too).
const OMITTABLE_FIELDS: [&str; 5] = [
    "coordinatorThreads",
    "resourceIndex",
    "draft",
    "queue",
    "agentMessageDeliveries",
];

/// What one socket last sent of each omittable field, per chat.
#[derive(Default)]
pub(crate) struct ChatFrameMemory {
    sent: HashMap<(String, String), HashMap<&'static str, u64>>,
    /// Set once this socket's client subscribed with `coordinatorThreadsPatch: true`.
    thread_patches: Arc<AtomicBool>,
    /// The thread list this socket last carried, per chat, while `thread_patches` is set.
    threads_sent: HashMap<(String, String), Value>,
}

/// Whether a socket's client asked for thread list patches; shared by its read and send loops.
pub(crate) type ThreadPatchOptIn = Arc<AtomicBool>;

/// Notes a client's `subscribeSessionChat` that asks for thread list patches.
pub(crate) fn note_client_message(text: &str, opt_in: &ThreadPatchOptIn) {
    if opt_in.load(Ordering::Relaxed) || !text.contains("coordinatorThreadsPatch") {
        return;
    }
    let Ok(parsed) = serde_json::from_str::<Value>(text) else {
        return;
    };
    if parsed.get("type").and_then(Value::as_str) == Some("subscribeSessionChat")
        && parsed
            .get("coordinatorThreadsPatch")
            .and_then(Value::as_bool)
            == Some(true)
    {
        opt_in.store(true, Ordering::Relaxed);
    }
}

/// The changes from `old` to `new` as `{ path, value }` and `{ path, remove: true }` operations:
/// objects by key, arrays of the same length by index, anything else replaced whole.
fn diff(old: &Value, new: &Value, path: &mut Vec<Value>, ops: &mut Vec<Value>) {
    if old == new {
        return;
    }
    match (old, new) {
        (Value::Object(before), Value::Object(after)) => {
            for (key, value) in after {
                path.push(Value::String(key.clone()));
                match before.get(key) {
                    Some(previous) => diff(previous, value, path, ops),
                    None => ops.push(serde_json::json!({ "path": path.clone(), "value": value })),
                }
                path.pop();
            }
            for key in before.keys().filter(|key| !after.contains_key(*key)) {
                path.push(Value::String(key.clone()));
                ops.push(serde_json::json!({ "path": path.clone(), "remove": true }));
                path.pop();
            }
        }
        (Value::Array(before), Value::Array(after))
            if before.len() == after.len() && !path.is_empty() =>
        {
            for (index, (previous, value)) in before.iter().zip(after).enumerate() {
                path.push(Value::from(index));
                diff(previous, value, path, ops);
                path.pop();
            }
        }
        _ => ops.push(serde_json::json!({ "path": path.clone(), "value": new })),
    }
}

fn digest(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Serialized, so equal JSON hashes equal whatever its map order in memory.
    serde_json::to_string(value)
        .unwrap_or_default()
        .hash(&mut hasher);
    hasher.finish()
}

impl ChatFrameMemory {
    pub(crate) fn new(thread_patches: ThreadPatchOptIn) -> Self {
        Self {
            thread_patches,
            ..Self::default()
        }
    }

    /// `threads` as this socket sends it: a patch against the list it last carried when the client
    /// asked for patches and the patch is less than half the list's size, else the list itself.
    fn thread_patch(
        &mut self,
        chat: &(String, String),
        threads: &Value,
        full: bool,
    ) -> Option<Value> {
        if !self.thread_patches.load(Ordering::Relaxed) {
            return None;
        }
        let base = self.threads_sent.insert(chat.clone(), threads.clone());
        let base = base.filter(|_| !full)?;
        let mut ops = Vec::new();
        diff(&base, threads, &mut Vec::new(), &mut ops);
        let patch = serde_json::json!({
            "base": base.get("revision").cloned().unwrap_or(Value::Null),
            "ops": ops,
        });
        let patch_size = serde_json::to_string(&patch).map(|text| text.len()).ok()?;
        let list_size = serde_json::to_string(threads).map(|text| text.len()).ok()?;
        (patch_size * 2 < list_size).then_some(patch)
    }

    /// `event` as it goes out on this socket: a state frame without the fields it repeats.
    pub(crate) fn pass(&mut self, event: EventPayload) -> EventPayload {
        let Some(object) = event.as_object() else {
            return event;
        };
        let kind = object
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let full = matches!(kind, "sessionChatSnapshot" | "sessionChatReplaced");
        if !full && kind != "sessionChatState" {
            return event;
        }
        let (Some(project_id), Some(session_id)) = (
            object.get("projectId").and_then(Value::as_str),
            object.get("sessionId").and_then(Value::as_str),
        ) else {
            return event;
        };
        let chat = (project_id.to_string(), session_id.to_string());
        let memory = self.sent.entry(chat.clone()).or_default();
        if full {
            memory.clear();
        }
        let mut repeated = Vec::new();
        for field in OMITTABLE_FIELDS {
            let Some(value) = object.get(field) else {
                continue;
            };
            let hash = digest(value);
            if !full && memory.get(field) == Some(&hash) {
                repeated.push(field);
            } else {
                memory.insert(field, hash);
            }
        }
        let threads_patch = match object.get("coordinatorThreads") {
            Some(threads) if !repeated.contains(&"coordinatorThreads") => {
                self.thread_patch(&chat, threads, full)
            }
            _ => None,
        };
        if repeated.is_empty() && threads_patch.is_none() {
            return event;
        }
        let mut trimmed = object.clone();
        for field in repeated {
            trimmed.remove(field);
        }
        if let Some(patch) = threads_patch {
            trimmed.remove("coordinatorThreads");
            trimmed.insert("coordinatorThreadsPatch".to_string(), patch);
        }
        EventPayload::from(Value::Object(trimmed))
    }
}
