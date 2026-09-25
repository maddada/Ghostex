//! The draft-save glue: the outbox rows a send owes, the receipts it clears, and the retry ladder.
//!
//! CDXC:Drafts 2026-09-10 DECISION:
//! User: unsaved edits must survive unavailable connections and retry across restarts, with visible
//! save failures, and saving must work quietly in the background without a routine saving indicator
//! while typing. These records belong to the HOST, not the brain: the core reads none of them and a
//! platform-neutral crate cannot own a disk-backed retry worker. The outbox row is written BEFORE
//! the call goes out, so a crash between the two leaves the save to be retried rather than lost.
//!
//! `outbox.rs` owns the queue itself (the rows, the ladder's arithmetic, the acknowledgement rule);
//! this file is what connects it to a chat: which request was a save, whose answer it was, and when
//! the next attempt goes out.

use serde_json::Value;
use std::time::Duration;
use web_time::Instant;

use ghostex_gx_chat_core::HostRequest;

use super::host_records::{self, DraftVersion, PendingDraft, RecoveryCheckpoint};
use super::outbox::{self, DraftWorker};
use super::world::{World, now_millis, publish};

/// How many saves may be in flight before the oldest is forgotten.
///
/// One entry is added per `setSessionChatDraft` the core issues and removed when the view answers
/// it. A view that goes away mid-flight never answers, so the map needs a bound: the rows are still
/// in the outbox and the ladder still drains them, and all a forgotten entry costs is that the
/// acknowledgement for that one revision comes from the retry worker instead.
const MAX_IN_FLIGHT_SAVES: usize = 256;

/// How many delivery ids one chat remembers in memory.
///
/// The STORED receipt set is the real de-duplication; this one only keeps a chat that re-reads the
/// same snapshot from touching disk again. So it is cleared wholesale rather than trimmed: the cost
/// of clearing it is one extra read of a record that already says the same thing.
const MAX_REMEMBERED_DELIVERIES: usize = 256;

/// Writes the sent history for every draft gxserver reports it delivered, once each.
///
/// `onDeliveredDrafts` in `native-host.ts` handed the receipts to `composer('deliveries')`, which
/// was `recordDeliveredSessionChatDrafts`. The core folds them onto `session.synced_draft`
/// (`merge_draft_state`), so the host reads them there rather than needing a callback into the
/// core.
pub(super) fn record_deliveries(world: &mut World, key: &str) {
    let Some(retained) = world.store.get(key) else {
        return;
    };
    let Some(deliveries) = retained
        .core
        .state()
        .session
        .synced_draft
        .as_ref()
        .and_then(|draft| draft.get("deliveredDrafts"))
        .and_then(Value::as_array)
        .filter(|deliveries| !deliveries.is_empty())
        .cloned()
    else {
        return;
    };
    let now_ms = now_millis();
    let seen = world.delivered.entry(key.to_string()).or_default();
    if seen.len() >= MAX_REMEMBERED_DELIVERIES {
        seen.clear();
    }
    let mut refusals = 0u64;
    for delivery in deliveries {
        let (Some(id), Some(project_id), Some(session_id)) = (
            delivery.get("id").and_then(Value::as_str),
            delivery.get("projectId").and_then(Value::as_str),
            delivery.get("sessionId").and_then(Value::as_str),
        ) else {
            continue;
        };
        if !seen.insert(id.to_string()) {
            continue;
        }
        if host_records::record_delivered_draft(
            id,
            project_id,
            session_id,
            delivery
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            delivery
                .get("deliveredAt")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            now_ms,
        )
        .is_err()
        {
            // A refused write must be retried rather than remembered as done.
            seen.remove(id);
            refusals += 1;
        }
    }
    world.counters.storage_refused += refusals;
}

/// Writes the durable save outbox row and the recovery checkpoint a draft save owes.
pub(super) fn note_draft_save(world: &mut World, session_key: &str, request: &HostRequest) {
    if request.kind != ghostex_gx_chat_core::RequestKind::Rpc
        || request.method != ghostex_gx_chat_core::ChatRpcMethod::SetSessionChatDraft.as_str()
    {
        return;
    }
    let Some(request_id) = request.id else {
        return;
    };
    let version = request
        .params
        .get("version")
        .or_else(|| request.params.get("draftVersion"));
    let Some(version) = version.and_then(|value| {
        Some(DraftVersion {
            draft_id: value.get("draftId")?.as_str()?.to_string(),
            revision: value.get("revision")?.as_i64()?,
        })
    }) else {
        return;
    };
    let content = request
        .params
        .get("content")
        .or_else(|| request.params.get("text"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let now_ms = now_millis();
    let draft = PendingDraft {
        client_id: request
            .params
            .get("clientId")
            .and_then(Value::as_str)
            .map(str::to_string),
        session_key: session_key.to_string(),
        content: content.clone(),
        version: version.clone(),
        updated_at: now_ms,
    };
    if host_records::queue_draft_save(&draft, now_ms).is_err() {
        world.counters.storage_refused += 1;
    }
    if host_records::preserve_draft_revision(
        &RecoveryCheckpoint {
            session_key: session_key.to_string(),
            text: content,
            updated_at: now_ms,
            version: Some(version),
            dismissed: None,
        },
        now_ms,
    )
    .is_err()
    {
        world.counters.storage_refused += 1;
    }
    // Ids rise, so the lowest is the oldest.
    while world.draft_saves.len() >= MAX_IN_FLIGHT_SAVES {
        let Some(oldest) = world.draft_saves.keys().next().copied() else {
            break;
        };
        world.draft_saves.remove(&oldest);
        world.counters.saves_forgotten += 1;
    }
    world.draft_saves.insert(request_id, draft);
}

/// Clears the outbox row once gxserver acknowledged the save, and arms the ladder when it did not.
///
/// A refusal leaves the row where it is, which is what makes the save retry at all: the stored
/// record IS the queue, so a save is not lost when the app quits between two attempts.
pub(super) fn settle_draft_save(world: &mut World, key: &str, arguments: &[Value]) {
    let Some(request_id) = arguments.first().and_then(Value::as_u64) else {
        return;
    };
    let Some(draft) = world.draft_saves.remove(&request_id) else {
        return;
    };
    if arguments.get(2).is_some_and(|error| !error.is_null()) {
        // The row it queued is still in the outbox, and that row is the queue, so the in-flight
        // entry is dropped rather than kept: keeping it would grow a map nothing ever reads again.
        arm_retry(world, key, true);
        return;
    }
    if outbox::acknowledge(&draft.session_key, &draft.version, now_millis()).is_err() {
        world.counters.storage_refused += 1;
    }
    // `get_mut`, not `entry().or_default()`: the answer to a save can arrive after its chat was
    // pruned or disabled, and creating the worker here put an entry back into a map `purge` had
    // just emptied, one per chat the user ever opened, for the life of the process.
    if let Some(worker) = world.draft_workers.get_mut(key) {
        worker.failures = 0;
    }
}

/// The answer to a retry write of this host's own, which the core must never see.
///
/// A retry is numbered from [`outbox::HOST_REQUEST_ID_BASE`], above every id
/// `ChatCore::allocate_request_id` can reach, so one glance at the id says whose answer this is.
pub(super) fn settle_retry_write(world: &mut World, key: &str, arguments: &[Value]) -> bool {
    let Some(request_id) = arguments.first().and_then(Value::as_u64) else {
        return false;
    };
    if request_id < outbox::HOST_REQUEST_ID_BASE {
        return false;
    }
    let failed = arguments.get(2).is_some_and(|error| !error.is_null());
    let now_ms = now_millis();
    let Some(worker) = world.draft_workers.get_mut(key) else {
        return true;
    };
    let Some(cleared) = outbox::settle(worker, request_id, failed, now_ms) else {
        return true;
    };
    if cleared {
        // The queue is drained one row at a time, in the order the revisions were typed.
        drain_outbox(world, key);
    } else {
        arm_retry(world, key, false);
    }
    true
}

/// Sends the next pending save of one chat.
///
/// With no view attached the request is HELD rather than sent (`publish`), which is the closest
/// this host can come to `store.ts`'s rule that "draft outbox writers survive their mounted chat":
/// there the broker owned one mutable RPC endpoint per machine and could deliver with no view at
/// all, and here the transport is the view's. Nothing is lost either way, because the stored row is
/// the queue; the delivery waits for the chat to be opened.
pub(super) fn drain_outbox(world: &mut World, key: &str) {
    let Some(session_key) = world
        .store
        .get(key)
        .map(|retained| retained.session_key.clone())
    else {
        return;
    };
    let now_ms = now_millis();
    let Some(request) = next_retry_write(world, key, &session_key, now_ms) else {
        return;
    };
    publish(world, key, vec![request]);
}

/// The next retry write, or `None` when one is in flight or nothing is pending.
pub(super) fn next_retry_write(
    world: &mut World,
    key: &str,
    session_key: &str,
    now_ms: i64,
) -> Option<HostRequest> {
    let worker = world.draft_workers.entry(key.to_string()).or_default();
    outbox::next_write(worker, session_key, now_ms)
}

/// Arms the retry ladder after a refused save.
///
/// `count` is whether this refusal is the worker's own to count: a save the CORE issued is not one
/// of the worker's attempts, but it is the event that starts the ladder, which is exactly what
/// `queueDraftSave`'s `void flushDraftSaves(...)` did on the TypeScript side.
///
/// A chat that is no longer retained arms nothing: there is no transport to retry down (the request
/// is the view's) and nothing to hold the ladder's position, and the row is still in the outbox, so
/// opening the chat again is what sends it. Arming here instead put a worker and a wake back into
/// maps `purge` had just emptied.
pub(super) fn arm_retry(world: &mut World, key: &str, count: bool) {
    if world.store.get(key).is_none() {
        return;
    }
    let worker: &mut DraftWorker = world.draft_workers.entry(key.to_string()).or_default();
    if count {
        worker.failures = worker.failures.saturating_add(1);
    }
    let delay = outbox::retry_delay_ms(worker.failures);
    world.retry_wakes.insert(
        key.to_string(),
        Instant::now() + Duration::from_millis(delay),
    );
}
