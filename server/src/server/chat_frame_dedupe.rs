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

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

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
        let memory = self
            .sent
            .entry((project_id.to_string(), session_id.to_string()))
            .or_default();
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
        if repeated.is_empty() {
            return event;
        }
        let mut trimmed = object.clone();
        for field in repeated {
            trimmed.remove(field);
        }
        EventPayload::from(Value::Object(trimmed))
    }
}
