//! The retained snapshot record, as it is written to and read from client storage.
//!
//! Ported from `apps/desktop/sidebar/session-chat-runtime/persistence.ts`. The record must stay
//! byte compatible with what the TypeScript wrote, because an installed Ghostex reads its own old
//! records after the switch: every number is an integer here for the same reason
//! `packages/client-storage-native/src/storage_records.rs` had to be fixed, since JavaScript writes `1`
//! where an `f64` would write `1.0`.

use serde::{Deserialize, Serialize};

use crate::effect::Effect;
use crate::session::apply::{apply_authoritative, apply_draft_agent_carriage};
use crate::session::constants::{
    INITIAL_LIMIT, MAX_LIMIT, PERSISTED_MAX_AGE_MS, PERSISTED_MAX_RECORD_BYTES,
    STORE_PERSISTENCE_DEBOUNCE_MS,
};
use crate::session::fold::FoldedSnapshot;
use crate::state::{ChatContext, ChatState};

/// One stored conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSnapshot {
    /// `JSON.stringify([machineId, projectId, sessionId])`, the store's own key.
    pub key: String,
    /// Epoch milliseconds, an integer.
    pub saved_at: i64,
    /// The window the next read should ask for, so a restored tail is not re-read smaller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requested_window: Option<u32>,
    pub snapshot: FoldedSnapshot,
}

/// The managed store the record lives in (`managedStore('chatSnapshots')`).
///
/// The host reads and writes it through [`crate::Effect::ReadRetainedSnapshot`] and
/// [`crate::Effect::WriteRetainedSnapshot`] rather than through [`crate::Effect::ReadStorage`],
/// for two reasons: the key is the host's to build (only it knows the machine id), and these round
/// trips are the STORE's, not an action's, so they must not join
/// [`crate::state::CoreState::publish_awaits`] and move a publish.
pub const SNAPSHOTS_STORE: &str = "chatSnapshots";

/// The store key for one session on one machine.
///
/// `JSON.stringify` of a three-string array, which is what the TypeScript wrote and what an
/// installed Ghostex already has on disk. The HOST calls this: it is the half of the identity that
/// knows the machine, the same way it builds [`crate::state::SessionIdentity::session_key`], and
/// it passes the answer back as [`crate::event::StartConfig::retained_key`] so the record's own
/// `key` field is written with the same bytes.
pub fn storage_key(machine_id: &str, project_id: &str, session_id: &str) -> String {
    serde_json::to_string(&[machine_id, project_id, session_id]).unwrap_or_default()
}

/// `readPersistedSessionChat`: the stored record, if there is one and it is still fresh.
///
/// A record that cannot be parsed is dropped rather than raised, which is the TypeScript's
/// `catch { return undefined }` around the whole read.
pub fn decode(value: Option<&str>, now_ms: f64) -> Option<StoredSnapshot> {
    let record: StoredSnapshot = serde_json::from_str(value?).ok()?;
    is_fresh(&record, now_ms).then_some(record)
}

/// `persistSessionChat`'s write, or `None` when the snapshot is over the record bound.
///
/// `None` DELETES the record, exactly as the TypeScript's `update` callback returning `undefined`
/// did: an oversized snapshot is disposable, and slicing it would leave an invalid pagination
/// cursor behind.
pub fn encode(record: &StoredSnapshot) -> Option<String> {
    if !fits_record_bound(&record.snapshot) {
        return None;
    }
    serde_json::to_string(record).ok()
}

/// The hydration read the store starts as soon as it is created.
///
/// `store.ts:113`, which runs in the `RetainedSession` constructor, before anything is subscribed
/// and beside the `composer('read')` the controller waits on.
pub fn read_at_boot() -> Effect {
    Effect::ReadRetainedSnapshot
}

/// The hydration read answering.
///
/// `if (!this.disposed && !this.snapshot && stored)`: a live fold that beat the read owns the tail,
/// and the stored one is dropped rather than rolling it back. Everything after that is
/// `emitSnapshot()`, which is the same fold the host's own cached snapshot takes.
pub fn adopt(state: &mut ChatState, value: Option<&str>, context: &ChatContext) {
    if state.messages.snapshot.is_some() {
        return;
    }
    let Some(stored) = decode(value, context.now_ms) else {
        return;
    };
    state.messages.retained_saved_at_ms = stored.saved_at as f64;
    // `Math.max(SESSION_CHAT_INITIAL_LIMIT, stored.requestedWindow ?? stored.snapshot.messages.length)`.
    let window = stored
        .requested_window
        .unwrap_or(stored.snapshot.result.messages.len() as u32)
        .max(INITIAL_LIMIT);
    state.messages.limit = state.messages.limit.max(window).min(MAX_LIMIT);
    state.messages.position.epoch = Some(stored.snapshot.result.epoch);
    state.messages.position.seq = stored.snapshot.result.seq;
    apply_draft_agent_carriage(state, &stored.snapshot.result);
    apply_authoritative(state, &stored.snapshot.result, true, context);
    state.messages.snapshot = Some(stored.snapshot);
    state.messages.authoritative_revision += 1;
    // Hydration is not a fold of its own: `emitSnapshot()` does NOT call `schedulePersistence()`,
    // so the record the core has just read must not be rewritten with a fresh stamp.
    state.messages.persisted_revision = state.messages.authoritative_revision;
}

/// `schedulePersistence()`: stamp the fold, then debounce the write.
///
/// CDXC:SessionChat 2026-09-22 WHY:
/// The deadline is a plain field rather than a row in `state.core.timers`, because in the
/// TypeScript this `setTimeout` belonged to `store.ts`, which was the desktop runtime and not the
/// chat brain: `take`'s `nextWakeMs` reported the brain's timers alone. A row here made every
/// frame's wake disagree with the brain the replay graded it against, for a cache write whose only
/// cost of being late is one more tick.
pub fn schedule(state: &mut ChatState, context: &ChatContext) {
    state.messages.retained_saved_at_ms = context.now_ms;
    if state.messages.persist_due_ms.is_some() {
        return;
    }
    state.messages.persist_due_ms = Some(context.now_ms + STORE_PERSISTENCE_DEBOUNCE_MS as f64);
}

/// The debounce coming due, checked on every event the way the host's tick drives every other
/// deadline.
pub fn flush_due(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    match state.messages.persist_due_ms {
        Some(due) if context.now_ms >= due => {}
        _ => return Vec::new(),
    }
    state.messages.persist_due_ms = None;
    let Some(snapshot) = state.messages.snapshot.clone() else {
        return Vec::new();
    };
    let record = StoredSnapshot {
        key: state.messages.retained_key.clone(),
        // `Date.now()` is an integer, and the record is read back by JavaScript.
        saved_at: state.messages.retained_saved_at_ms as i64,
        requested_window: Some(state.messages.limit),
        snapshot,
    };
    vec![Effect::WriteRetainedSnapshot {
        value: encode(&record),
    }]
}

/// Whether a stored record is still usable, which is the read-side half of the bound.
pub fn is_fresh(record: &StoredSnapshot, now_ms: f64) -> bool {
    now_ms - (record.saved_at as f64) < PERSISTED_MAX_AGE_MS
}

/// Whether a record may be written at all.
///
/// The TypeScript measures `JSON.stringify(snapshot).length * 2`, which is the UTF-16 byte size of
/// the serialized value; an oversized snapshot is dropped rather than sliced, because slicing
/// would leave an invalid pagination cursor behind.
pub fn fits_record_bound(snapshot: &FoldedSnapshot) -> bool {
    let Ok(text) = serde_json::to_string(snapshot) else {
        return false;
    };
    text.encode_utf16().count() * 2 <= PERSISTED_MAX_RECORD_BYTES
}
