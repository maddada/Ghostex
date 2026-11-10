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
    write_record(
        &record.key,
        record.saved_at,
        record.requested_window,
        &record.snapshot,
        false,
    )
}

/// The head of a [`StoredSnapshot`]: every field before `snapshot`, in the same order and names.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordHead<'a> {
    key: &'a str,
    saved_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    requested_window: Option<u32>,
}

/// [`encode`] without a [`StoredSnapshot`] to hold the snapshot: the same bytes
/// `serde_json::to_string` writes for the record, or `None` when the snapshot is over the bound.
///
/// CDXC:SessionChat 2026-11-10 WHY: a flush cloned the whole snapshot and wrote it out three times (measured by `fits_record_bound`, measured again by `encode`, then serialized), about 16% of a hidden chat's event in the memharness soak with nine agents working. The snapshot is now written once, counted as it is written and given up at the bound, straight after the record's head; the result is byte for byte what serializing the record wrote. A snapshot whose rows' own text already passes the bound is judged too large before anything is written ([`text_units`]), and that count sizes the buffer for the rest.
fn write_record(
    key: &str,
    saved_at: i64,
    requested_window: Option<u32>,
    snapshot: &FoldedSnapshot,
    count_first: bool,
) -> Option<String> {
    let limit = PERSISTED_MAX_RECORD_BYTES / 2;
    let text = text_units(snapshot, limit);
    if text > limit {
        return None;
    }
    // A chat that was over the bound last time likely still is: it is counted first without being
    // kept, so a chat just over the bound does not copy a megabyte only to drop it.
    if count_first && !fits_record_bound(snapshot) {
        return None;
    }
    let mut head = Vec::with_capacity(text * 2);
    serde_json::to_writer(
        &mut head,
        &RecordHead {
            key,
            saved_at,
            requested_window,
        },
    )
    .ok()?;
    // `{"key":…,"savedAt":…}` loses its closing brace and takes the snapshot as its last field.
    let mut writer = Utf16Counter {
        units: 0,
        limit,
        out: Some(head),
    };
    let out = writer.out.as_mut()?;
    out.pop();
    out.extend_from_slice(b",\"snapshot\":");
    serde_json::to_writer(&mut writer, snapshot).ok()?;
    let mut out = writer.out?;
    out.push(b'}');
    String::from_utf8(out).ok()
}

/// The UTF-16 units of the rows' own text (ids, words, tool output), counted until they pass
/// `limit`.
///
/// Each of those strings is written into the record at least once and never shorter than it is
/// (an escape only lengthens it, and anything else is written as it stands), so a count over
/// `limit` means the record is over the bound too; one under it decides nothing, and the snapshot
/// is measured.
fn text_units(snapshot: &FoldedSnapshot, limit: usize) -> usize {
    let mut units = 0usize;
    for message in &snapshot.rows {
        units += utf16_units(message.id.as_bytes());
        for block in &message.blocks {
            units += match block {
                ghostex_gx_protocol::chat::ChatBlock::Text { text } => utf16_units(text.as_bytes()),
                ghostex_gx_protocol::chat::ChatBlock::ToolResult { output, .. } => {
                    utf16_units(output.as_bytes())
                }
                _ => 0,
            };
        }
        if units > limit {
            break;
        }
    }
    units
}

/// The UTF-16 code units of a UTF-8 string ([`Utf16Counter`]'s rule).
fn utf16_units(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .map(|byte| match byte {
            0x80..=0xBF => 0,
            0xF0..=0xFF => 2,
            _ => 1,
        })
        .sum()
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
        .unwrap_or(stored.snapshot.rows.len() as u32)
        .max(INITIAL_LIMIT);
    state.messages.limit = state.messages.limit.max(window).min(MAX_LIMIT);
    state.messages.position.epoch = Some(stored.snapshot.result.epoch);
    state.messages.position.seq = stored.snapshot.result.seq;
    apply_draft_agent_carriage(state, &stored.snapshot.result);
    apply_authoritative(state, &stored.snapshot.read_result(), true, false, context);
    let mut snapshot = stored.snapshot;
    crate::session::merge::share_with_list(&mut snapshot.rows, &state.messages);
    state.messages.snapshot = Some(snapshot);
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
    let Some(snapshot) = state.messages.snapshot.as_ref() else {
        return Vec::new();
    };
    // An oversized snapshot deletes the record (`None`), as `encode` does.
    let value = write_record(
        &state.messages.retained_key,
        // `Date.now()` is an integer, and the record is read back by JavaScript.
        state.messages.retained_saved_at_ms as i64,
        Some(state.messages.limit),
        snapshot,
        state.messages.persisted_over_bound,
    );
    state.messages.persisted_over_bound = value.is_none();
    vec![Effect::WriteRetainedSnapshot { value }]
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
///
/// CDXC:SessionChat 2026-11-08 WHY:
/// The check serialized the whole snapshot to measure it, once a second while rows arrived. A chat scrolled far up holds tens of megabytes of rows, far over the bound, so every flush wrote out the whole transcript only to throw it away, which was a fifth of the chat worker's time per new row on a 19,000-row chat (the user's "gets slower with time until it becomes super slow"). The writer below counts as it goes and stops at the bound, so an oversized snapshot costs what the bound costs, and the answer is the same.
pub fn fits_record_bound(snapshot: &FoldedSnapshot) -> bool {
    let limit = PERSISTED_MAX_RECORD_BYTES / 2;
    let mut counter = Utf16Counter {
        units: 0,
        limit,
        out: None,
    };
    text_units(snapshot, limit) <= limit && serde_json::to_writer(&mut counter, snapshot).is_ok()
}

/// Counts the UTF-16 code units of the UTF-8 it is written, keeps it in `out` when given one,
/// and fails once the units pass `limit`.
struct Utf16Counter {
    units: usize,
    limit: usize,
    out: Option<Vec<u8>>,
}

impl std::io::Write for Utf16Counter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        // Each UTF-8 lead byte starts one character: a four-byte one (0xF0 and up) is two UTF-16
        // units, and continuation bytes (0b10xx_xxxx) add nothing, however the writes are split.
        self.units += utf16_units(bytes);
        if self.units > self.limit {
            return Err(std::io::Error::other("over the record bound"));
        }
        if let Some(out) = self.out.as_mut() {
            out.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
