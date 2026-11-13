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
//! CDXC:SessionChat 2026-11-10 WHY:
//! That pass was 4.6% of gxserver's busy time: every socket serialized each frame's heavy fields to
//! hash them and deep copied the whole frame to drop the repeats, and a full-mode socket gets every
//! chat's frames, so the work repeated per socket. The frame's digests are now worked out once per
//! event (written straight into the hasher, no text built) and kept on the shared event, and the
//! trimmed copy for each set of left-out fields is built once, without the fields it leaves out,
//! and shared by every socket that leaves out the same ones (its wire text is then written once
//! too). Each socket still keeps its own memory of what it sent, so what it receives is unchanged.

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
use std::hash::{DefaultHasher, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};

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

/// What every socket needs of one chat frame: its chat, whether it resets the memory, the digest
/// of each omittable field it carries, and the trimmed copies built so far, by left-out fields.
#[derive(Debug)]
pub(crate) struct ChatFrameShare {
    chat: (String, String),
    full: bool,
    digests: [Option<u64>; 5],
    trimmed: Mutex<Vec<(u8, EventPayload)>>,
}

/// Feeds serialized JSON to the hasher as it is written.
struct HashWriter<'a>(&'a mut DefaultHasher);

impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The digest the memory compares: of the value's serialized text, so equal JSON hashes equal
/// whatever its map order in memory. The coordinator panel works out its summary's digest with it
/// once per change (`coordinators::coordinator_threads_digest`).
pub(crate) fn digest(value: &Value) -> u64 {
    let mut hasher = DefaultHasher::new();
    let _ = serde_json::to_writer(HashWriter(&mut hasher), value);
    hasher.finish()
}

/// Counts serialized JSON without keeping it.
struct CountWriter(usize);

impl std::io::Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The length of `value`'s compact JSON text, without building the text.
pub(crate) fn json_len(value: &Value) -> usize {
    let mut counter = CountWriter(0);
    let _ = serde_json::to_writer(&mut counter, value);
    counter.0
}

/// CDXC:SessionChat 2026-11-12 WHY:
/// Digesting a coordinator chat's thread list (about 124 KB of board, work and threads) serialized it on every state frame, about 10% of gxserver's busy samples on the profiled Mac, though it only changes when the panel is rebuilt. The panel keeps its summary's digest, and a frame whose list equals the panel's current summary takes it after one comparison; any other list (the panel moved on since the frame was built) is digested as before, so a socket's memory never holds a digest of a list it did not send.
fn field_digest(field: &str, chat: &(String, String), value: &Value) -> u64 {
    if field == "coordinatorThreads" {
        if let Some(known) = crate::coordinators::coordinator_threads_digest(chat, value) {
            return known;
        }
    }
    digest(value)
}

fn share_of(event: &EventPayload) -> Option<&ChatFrameShare> {
    event
        .chat_frame_share()
        .get_or_init(|| build_share(event).map(Box::new))
        .as_deref()
}

fn build_share(event: &Value) -> Option<ChatFrameShare> {
    let object = event.as_object()?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let full = matches!(kind, "sessionChatSnapshot" | "sessionChatReplaced");
    if !full && kind != "sessionChatState" {
        return None;
    }
    let project_id = object.get("projectId").and_then(Value::as_str)?;
    let session_id = object.get("sessionId").and_then(Value::as_str)?;
    let chat = (project_id.to_string(), session_id.to_string());
    let digests = OMITTABLE_FIELDS.map(|field| {
        object
            .get(field)
            .map(|value| field_digest(field, &chat, value))
    });
    Some(ChatFrameShare {
        chat,
        full,
        digests,
        trimmed: Mutex::new(Vec::new()),
    })
}

impl ChatFrameShare {
    /// The frame without the fields `left_out` marks, built once per set and shared.
    fn trimmed(&self, event: &EventPayload, left_out: u8) -> EventPayload {
        let build = || {
            let entries: Vec<(&String, &Value)> = event.as_object().into_iter().flatten().collect();
            // The key order the frame had when each left-out field was removed in turn with
            // `Map::remove` (a swap remove: the last key takes the removed key's place), so the
            // frame's text stays byte for byte what it was.
            let mut order: Vec<usize> = (0..entries.len()).collect();
            for (index, field) in OMITTABLE_FIELDS.iter().enumerate() {
                if left_out & (1 << index) == 0 {
                    continue;
                }
                if let Some(position) = order.iter().position(|entry| entries[*entry].0 == field) {
                    order.swap_remove(position);
                }
            }
            let kept: Map<String, Value> = order
                .into_iter()
                .map(|entry| (entries[entry].0.clone(), entries[entry].1.clone()))
                .collect();
            EventPayload::from(Value::Object(kept))
        };
        let Ok(mut trimmed) = self.trimmed.lock() else {
            return build();
        };
        if let Some((_, payload)) = trimmed.iter().find(|(mask, _)| *mask == left_out) {
            return payload.clone();
        }
        let payload = build();
        trimmed.push((left_out, payload.clone()));
        payload
    }
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
        (json_len(&patch) * 2 < json_len(threads)).then_some(patch)
    }

    /// `event` as it goes out on this socket: a state frame without the fields it repeats.
    pub(crate) fn pass(&mut self, event: EventPayload) -> EventPayload {
        let Some(share) = share_of(&event) else {
            return event;
        };
        let memory = match self.sent.get_mut(&share.chat) {
            Some(memory) => memory,
            None => self.sent.entry(share.chat.clone()).or_default(),
        };
        if share.full {
            memory.clear();
        }
        let mut left_out = 0_u8;
        for (index, (field, hash)) in OMITTABLE_FIELDS.iter().zip(share.digests).enumerate() {
            let Some(hash) = hash else {
                continue;
            };
            if !share.full && memory.get(field) == Some(&hash) {
                left_out |= 1 << index;
            } else {
                memory.insert(field, hash);
            }
        }
        // The thread list as this socket sends it, when its client asked for patches and it is not
        // left out as a repeat (`thread_patch`).
        let threads_patch = if left_out & 1 == 0 {
            event
                .as_object()
                .and_then(|object| object.get("coordinatorThreads"))
                .and_then(|threads| self.thread_patch(&share.chat, threads, share.full))
        } else {
            None
        };
        let Some(patch) = threads_patch else {
            if left_out == 0 {
                return event;
            }
            return share.trimmed(&event, left_out);
        };
        // This socket's own copy: the shared trimmed frame, with the list swapped for its patch the
        // way the frame was trimmed (a swap remove, then the patch at the end).
        let shared = if left_out == 0 {
            event
        } else {
            share.trimmed(&event, left_out)
        };
        let mut patched = shared.as_object().cloned().unwrap_or_default();
        patched.remove("coordinatorThreads");
        patched.insert("coordinatorThreadsPatch".to_string(), patch);
        EventPayload::from(Value::Object(patched))
    }
}
