//! The three read lanes: the seed read, the resync read, and the history page.
//!
//! Ported from `seedRead`, `requestResync` and `loadEarlier` in
//! `packages/shared/session-chat-controller/controller.ts`, together with `withReadTimeout` and the
//! two retry schedulers that lived inside `requestResync`. The TypeScript kept each read's
//! identity in the closure that awaited it; the core has no closures, so the same facts ride on
//! `state.messages.reads` and a settled request id picks the row back out.

use ghostex_gx_protocol::{ChatStatus, ReadSessionChatResult};
use serde_json::{Map, Value};

use crate::effect::Effect;
use crate::session::constants::{
    not_found_retry_delay_ms, resync_retry_delay_ms, MAX_RESYNC_FOLLOW_UPS,
    NOT_FOUND_RETRY_WINDOW_MS, PAGE, READ_TIMEOUT_MS, RESYNC_FOLLOW_UP_DELAY_MS,
    TIMER_READ_DEADLINE, TIMER_RESYNC_FOLLOW_UP, TIMER_RESYNC_RETRY, TIMER_SEED_RETRY,
};
use crate::state::{ChatContext, ChatState, OutstandingRead, ReadKind};
use crate::wire::ChatRpcMethod;

/// The copy every failed read that reaches the view shows.
pub const READ_FAILED_MESSAGE: &str = "Conversation could not be loaded.";

/// Issues one read and records what it is for.
///
/// `before_offset` is set for a page read alone; the seed and resync lanes always ask for the
/// window `limit` currently names, which grows with what is on screen.
pub fn issue_read(
    state: &mut ChatState,
    context: &ChatContext,
    kind: ReadKind,
    before_offset: Option<u64>,
) -> Effect {
    let request_id = state.core.allocate_request_id();
    let limit = match kind {
        ReadKind::Page => PAGE,
        _ => state.messages.limit,
    };
    if matches!(kind, ReadKind::Resync) {
        state.messages.resync.in_flight = true;
        state.messages.resync.seen_in_flight = None;
    }
    state.messages.reads.push(OutstandingRead {
        request_id,
        kind,
        generation: state.messages.generation,
        before_offset,
        started_at_ms: context.now_ms,
    });
    arm_read_deadline(state, context);

    let mut params = Map::new();
    params.insert(
        "projectId".to_string(),
        Value::String(state.identity.project_id.clone()),
    );
    params.insert(
        "sessionId".to_string(),
        Value::String(state.identity.session_id.clone()),
    );
    params.insert("limit".to_string(), Value::from(limit));
    if let Some(offset) = before_offset {
        params.insert("beforeOffset".to_string(), Value::from(offset));
    }
    // `loadEarlier` read through `transport.readHistory` (`native-host.ts`), which was
    // `rpc('readSessionChat', { ...params, historyMode: params.detail ? 'detail' : 'turns' })`, with
    // `preserveNewest` while a working session has no user turn on screen yet. Without the two the
    // page came back as a plain window rather than as history.
    if matches!(kind, ReadKind::Page) {
        let preserve_newest = crate::session::working::is_working(state)
            && !state
                .messages
                .list
                .iter()
                .any(|message| matches!(message.role, ghostex_gx_protocol::ChatRole::User));
        params.insert("preserveNewest".to_string(), Value::Bool(preserve_newest));
        params.insert("historyMode".to_string(), Value::from("turns"));
    }
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionChat,
        params: Box::new(Value::Object(params)),
    }
}

/// Wakes the core when the oldest read in flight hits its deadline.
///
/// The deadline does not cancel the request, it settles the state machine: a read that never
/// resolves would otherwise pin the resync flight and freeze every later gap verdict.
pub fn arm_read_deadline(state: &mut ChatState, context: &ChatContext) {
    match state
        .messages
        .reads
        .iter()
        .map(|read| read.started_at_ms)
        .fold(None, |oldest: Option<f64>, at| {
            Some(match oldest {
                Some(current) if current <= at => current,
                _ => at,
            })
        }) {
        Some(oldest) => {
            let delay = (oldest + READ_TIMEOUT_MS as f64 - context.now_ms).max(0.0);
            state
                .core
                .timers
                .arm(TIMER_READ_DEADLINE, context.now_ms, delay);
        }
        None => {
            state.core.timers.cancel(TIMER_READ_DEADLINE);
        }
    }
}

/// Takes the outstanding read a settled request belongs to.
///
/// By id first, which is what a real host answers with. The replay that checked the port fed back
/// the ids of the other brain's own rpc sequence, so an id this core never issued falls through to
/// the oldest read in flight; the caller only reaches here for a payload that already parsed as a
/// chat read, so nothing else can be consumed by the fallback.
pub fn take_read(state: &mut ChatState, request_id: u64) -> Option<OutstandingRead> {
    let at = state
        .messages
        .reads
        .iter()
        .position(|read| read.request_id == request_id)
        .or(if state.messages.reads.is_empty() {
            None
        } else {
            Some(0)
        })?;
    Some(state.messages.reads.remove(at))
}

/// `requestResync`: re-read authoritative state, unless one is already in flight.
///
/// Frames arriving from here on are recorded by the frame handler and covered by the follow-up read
/// this flight schedules.
pub fn request_resync(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    if state.messages.resync.in_flight {
        return Vec::new();
    }
    vec![issue_read(state, context, ReadKind::Resync, None)]
}

/// Backoff, never giving up while the chat is open: the next read is the only thing that can clear
/// the error state, which a successful read does on its way through `applyAuthoritative`.
pub fn schedule_resync_retry(state: &mut ChatState, context: &ChatContext) {
    let delay = resync_retry_delay_ms(state.messages.resync.failures);
    if state
        .core
        .timers
        .arm_once(TIMER_RESYNC_RETRY, context.now_ms, delay as f64)
    {
        state.messages.resync.failures += 1;
    }
}

/// One paced follow-up for the bytes a successful read outran, capped so a continuously streaming
/// turn cannot turn follow-ups into a read loop.
pub fn schedule_resync_follow_up(state: &mut ChatState, context: &ChatContext) {
    if state.messages.resync.follow_ups >= MAX_RESYNC_FOLLOW_UPS {
        return;
    }
    if state.core.timers.arm_once(
        TIMER_RESYNC_FOLLOW_UP,
        context.now_ms,
        RESYNC_FOLLOW_UP_DELAY_MS as f64,
    ) {
        state.messages.resync.follow_ups += 1;
    }
}

/// The seed read's own retry, on the not-found and starting patience ladder.
pub fn schedule_seed_retry(state: &mut ChatState, context: &ChatContext) {
    let delay = not_found_retry_delay_ms(state.messages.seed_attempt);
    state
        .core
        .timers
        .arm(TIMER_SEED_RETRY, context.now_ms, delay as f64);
    state.messages.seed_attempt += 1;
}

/// Whether the seed lane is still inside its 60 s patience window.
pub fn inside_seed_window(state: &ChatState, context: &ChatContext) -> bool {
    context.now_ms - state.messages.seed_started_at_ms < NOT_FOUND_RETRY_WINDOW_MS
}

/// A read that never landed. Settles the state machine exactly as a rejection would, so the lane
/// that was waiting for it can retry.
pub fn expire_overdue_reads(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let deadline = context.now_ms - READ_TIMEOUT_MS as f64;
    let (overdue, kept): (Vec<OutstandingRead>, Vec<OutstandingRead>) =
        std::mem::take(&mut state.messages.reads)
            .into_iter()
            .partition(|read| read.started_at_ms <= deadline);
    state.messages.reads = kept;
    let mut effects = Vec::new();
    for read in overdue {
        effects.extend(fail_read(state, context, &read));
    }
    arm_read_deadline(state, context);
    effects
}

/// What each lane does with a read that failed, including by timeout.
pub fn fail_read(
    state: &mut ChatState,
    context: &ChatContext,
    read: &OutstandingRead,
) -> Vec<Effect> {
    if read.generation != state.messages.generation {
        return Vec::new();
    }
    match read.kind {
        ReadKind::Resync => {
            state.messages.resync.in_flight = false;
            state.session.error = Some(READ_FAILED_MESSAGE.to_string());
            state.session.server_status = ChatStatus::Error;
            schedule_resync_retry(state, context);
        }
        ReadKind::Seed => {
            // A seed read that lost the race to a frame has nothing left to do: the transcript is
            // already on screen.
            if state.messages.position.frame_arrived {
                return Vec::new();
            }
            if inside_seed_window(state, context) {
                schedule_seed_retry(state, context);
                return Vec::new();
            }
            state.session.error = Some(READ_FAILED_MESSAGE.to_string());
            state.session.server_status = ChatStatus::Error;
        }
        ReadKind::Page => {
            // A failed page must not become the view's error state: the live tail is still valid.
            // `hasMore` stays set, so the control comes back and the user can ask again.
            if state
                .messages
                .load_earlier_request
                .as_ref()
                .is_some_and(|request| request.before_offset == read.before_offset.unwrap_or(0))
            {
                state.messages.load_earlier_request = None;
                state.messages.loading_earlier = false;
            }
        }
    }
    Vec::new()
}

/// Whether a settled payload is a chat read at all.
pub fn parse_read(result: &Value) -> Option<ReadSessionChatResult> {
    serde_json::from_value::<ReadSessionChatResult>(result.clone()).ok()
}
