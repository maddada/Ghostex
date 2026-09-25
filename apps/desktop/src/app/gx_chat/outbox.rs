//! The durable draft save outbox and the retry ladder that drains it.
//!
//! CDXC:Drafts 2026-09-10 DECISION:
//! User: unsaved edits must survive unavailable connections and retry across restarts, with visible
//! save failures, and saving must work quietly in the background without a routine saving indicator
//! while typing. The outbox belongs to the HOST, not the brain: a platform-neutral core cannot own
//! a disk-backed retry worker, so `packages/core-ui/chat/session-chat-draft-outbox.ts` is ported
//! here rather than into `packages/gx-chat-core`.
//!
//! The shape is the TypeScript's `Worker`: one per session, at most one write in flight, and a
//! refusal arms `Math.min(30_000, 1_000 * 2 ** Math.min(failures, 5))` before the next attempt.
//! What differs is who writes the CURRENT revision: in QuickJS the worker was the only writer, and
//! here the core issues its own `setSessionChatDraft` and the host records the outbox row beside
//! it, so this worker only ever drains what a refusal left behind.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use ghostex_gx_chat_core::{ChatRpcMethod, HostRequest, RequestKind, StorageKey};

use super::host_records::{DraftVersion, PendingDraft};
use super::storage;

/// The first host-minted request id.
///
/// The core allocates its own ids from 1 upward (`ChatCore::allocate_request_id`), and the view
/// echoes whichever id it was given straight back, so the two counters must not meet. A retry is
/// numbered from here instead, which no chat's own counter can reach: the core would have to make
/// a trillion calls in one session first.
pub(super) const HOST_REQUEST_ID_BASE: u64 = 1 << 40;

/// One session's retry worker.
#[derive(Debug, Default)]
pub(super) struct DraftWorker {
    /// Consecutive refusals, which is what the ladder is measured from.
    pub(super) failures: u32,
    /// The retry write in flight, by the host id it was sent under.
    in_flight: Option<(u64, PendingDraft)>,
}

impl DraftWorker {
    /// Whether a write of this worker's is still waiting for its answer.
    pub(super) fn busy(&self) -> bool {
        self.in_flight.is_some()
    }
}

/// Every pending save of one session, oldest first.
///
/// `pendingDrafts(sessionKey)` sorts by `updatedAt` so an append-only run of keystrokes is
/// delivered in the order it was typed; a row whose payload no longer decodes is skipped the way
/// the TypeScript index's decoder returns `null`.
pub(super) fn pending(session_key: &str, now_ms: i64) -> Vec<PendingDraft> {
    let mut drafts: Vec<PendingDraft> =
        storage::scan("draftOutbox", &format!("{session_key}:"), now_ms)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(_, raw)| serde_json::from_str::<PendingDraft>(&raw).ok())
            .filter(|draft| draft.session_key == session_key && !draft.version.draft_id.is_empty())
            .collect();
    drafts.sort_by_key(|draft| draft.updated_at);
    drafts
}

/// The next write this worker owes, as the `rpc` request the view performs.
///
/// `None` when a write is already in flight or nothing is pending, which is what
/// `flushDraftSaves`'s `worker.running` guard and its empty-queue break do.
pub(super) fn next_write(
    worker: &mut DraftWorker,
    session_key: &str,
    now_ms: i64,
) -> Option<HostRequest> {
    if worker.busy() {
        return None;
    }
    let draft = pending(session_key, now_ms).into_iter().next()?;
    let request_id = HOST_REQUEST_ID_BASE + next_sequence();
    let mut params = Map::new();
    params.insert("content".into(), Value::String(draft.content.clone()));
    params.insert(
        "draftVersion".into(),
        json!({"draftId": draft.version.draft_id, "revision": draft.version.revision}),
    );
    if let Some(client_id) = draft.client_id.clone() {
        params.insert("clientId".into(), Value::String(client_id));
    }
    worker.in_flight = Some((request_id, draft));
    Some(HostRequest {
        id: Some(request_id),
        kind: RequestKind::Rpc,
        method: ChatRpcMethod::SetSessionChatDraft.as_str().to_string(),
        params,
    })
}

/// The answer to a retry write. `Some(true)` means the row is gone and the next one may go out.
///
/// A refusal leaves the row where it is, which is what makes the save retry at all: the stored
/// record IS the queue, so nothing is lost when the app quits between two attempts.
pub(super) fn settle(
    worker: &mut DraftWorker,
    request_id: u64,
    failed: bool,
    now_ms: i64,
) -> Option<bool> {
    let (pending_id, draft) = worker.in_flight.clone()?;
    if pending_id != request_id {
        return None;
    }
    worker.in_flight = None;
    if failed {
        worker.failures = worker.failures.saturating_add(1);
        return Some(false);
    }
    worker.failures = 0;
    let _ = acknowledge(&draft.session_key, &draft.version, now_ms);
    Some(true)
}

/// Clears every pending save of one draft at or below the acknowledged revision.
///
/// `acknowledgeDraftSave` removes the whole run rather than the one key, which is what makes an
/// append-only burst of keystrokes collapse to a single delivered write.
pub(super) fn acknowledge(
    session_key: &str,
    version: &DraftVersion,
    now_ms: i64,
) -> Result<(), &'static str> {
    let mut failure = None;
    for draft in pending(session_key, now_ms) {
        if draft.version.draft_id != version.draft_id || draft.version.revision > version.revision {
            continue;
        }
        let key = StorageKey {
            store: "draftOutbox".to_string(),
            suffix: format!(
                "{}:{}:{}",
                draft.session_key, draft.version.draft_id, draft.version.revision
            ),
        };
        if let Err(reason) = storage::write(&key, None, now_ms) {
            failure = Some(reason);
        }
    }
    match failure {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// The retry ladder a failed save waits out, in milliseconds.
///
/// `Math.min(30_000, 1_000 * 2 ** Math.min(failures, 5))` with `failures` already incremented, so
/// the first retry is 2 s: 2, 4, 8, 16, 30, 30, and so on.
pub(super) fn retry_delay_ms(failures: u32) -> u64 {
    30_000u64.min(1_000u64 << failures.min(5))
}

/// A number no other host request shares, so two sessions retrying at once cannot collide.
fn next_sequence() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    SEQUENCE.fetch_add(1, Ordering::Relaxed)
}

/// Every session's worker, keyed by the retention key its chat is held under.
///
/// A worker is created when a chat is attached and kept while it is retained, which is what
/// `replayDraftSaves` did on the TypeScript side: a row left by a previous run goes out when its
/// chat is opened again rather than being lost.
pub(super) type Workers = BTreeMap<String, DraftWorker>;
