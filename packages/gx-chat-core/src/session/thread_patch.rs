//! Rebuilds a coordinator's thread list from a `coordinatorThreadsPatch` before a frame is applied.
//!
//! CDXC:SessionChat 2026-11-10 WHY:
//! On the user's iPhone the "Main Coordinator" chat got a 155 KB state frame every 4 to 12 s, and
//! 152 KB of it was the thread list: each thread's `detail` is its latest message, so any running
//! thread writing anything resent the whole list (which holds every thread three times, as
//! threads, board and work). A client that subscribes with `coordinatorThreadsPatch: true` now gets
//! only what changed since the list this socket last carried (server/src/server/chat_frame_dedupe.rs),
//! and this module turns it back into the whole list, so the fold, the cache and every view still see
//! an ordinary `coordinatorThreads`. Older phones and every other client never ask and get the whole
//! list as before.
//!
//! The base is `SessionState::coordinator_threads_wire`, the list exactly as the socket last carried
//! it, recorded here for every socket frame (accepted or not), because that is what gxserver
//! remembers sending. A patch whose `base` revision does not match, or that does not fit, loses the
//! list: the chat subscribes again, and the fresh snapshot carries it whole.
//! SEE-ALSO: packages/gx-protocol/src/chat.rs (`coordinator_threads_patch`).

use serde_json::Value;

use crate::state::ChatState;
use crate::wire::ChatFrame;

/// What a frame's thread list came to.
pub enum Resolved {
    /// Nothing to rebuild; apply the frame as it arrived.
    Unchanged,
    /// The frame with its whole thread list rebuilt from the patch.
    Patched(ChatFrame),
    /// The patch does not apply to what this socket last carried: subscribe again.
    Lost,
}

/// Records the list a socket frame carries, and rebuilds one a patch describes.
pub fn resolve(state: &mut ChatState, frame: &ChatFrame) -> Resolved {
    match frame {
        ChatFrame::Snapshot(snapshot) | ChatFrame::Replaced(snapshot) => {
            if let Some(threads) = &snapshot.state.coordinator_threads {
                state.session.coordinator_threads_wire = Some(threads.clone());
            }
            Resolved::Unchanged
        }
        ChatFrame::State(state_frame) => {
            if let Some(threads) = &state_frame.state.coordinator_threads {
                state.session.coordinator_threads_wire = Some(threads.clone());
                return Resolved::Unchanged;
            }
            let Some(patch) = &state_frame.state.coordinator_threads_patch else {
                return Resolved::Unchanged;
            };
            let rebuilt = state
                .session
                .coordinator_threads_wire
                .as_ref()
                .and_then(|base| apply_patch(base, patch));
            let Some(threads) = rebuilt else {
                state.session.coordinator_threads_wire = None;
                return Resolved::Lost;
            };
            // Held once, by the wire base and the frame alike (`document::SharedValue`).
            let threads = crate::document::SharedValue::from(threads);
            state.session.coordinator_threads_wire = Some(threads.clone());
            let mut whole = state_frame.clone();
            whole.state.coordinator_threads = Some(threads);
            whole.state.coordinator_threads_patch = None;
            Resolved::Patched(ChatFrame::State(whole))
        }
        ChatFrame::Appended(_) => Resolved::Unchanged,
    }
}

/// `base` with `patch`'s operations applied, or `None` when the patch was made from another list.
pub fn apply_patch(base: &Value, patch: &Value) -> Option<Value> {
    let expected = patch.get("base").cloned().unwrap_or(Value::Null);
    if base.get("revision").cloned().unwrap_or(Value::Null) != expected {
        return None;
    }
    let mut list = base.clone();
    for op in patch.get("ops")?.as_array()? {
        let path = op.get("path")?.as_array()?;
        let (last, parents) = path.split_last()?;
        let mut target = &mut list;
        for step in parents {
            target = match step {
                Value::String(key) => target.as_object_mut()?.get_mut(key)?,
                Value::Number(index) => target.as_array_mut()?.get_mut(index.as_u64()? as usize)?,
                _ => return None,
            };
        }
        let remove = op.get("remove").and_then(Value::as_bool) == Some(true);
        match last {
            Value::String(key) => {
                let object = target.as_object_mut()?;
                if remove {
                    object.remove(key)?;
                } else {
                    object.insert(key.clone(), op.get("value")?.clone());
                }
            }
            Value::Number(index) if !remove => {
                *target.as_array_mut()?.get_mut(index.as_u64()? as usize)? =
                    op.get("value")?.clone();
            }
            _ => return None,
        }
    }
    Some(list)
}
