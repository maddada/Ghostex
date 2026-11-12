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
    not_found_retry_delay_ms, resync_retry_delay_ms, unreachable_retry_delay_ms,
    LIGHT_SEED_GRACE_MS, MAX_RESYNC_FOLLOW_UPS, NOT_FOUND_RETRY_WINDOW_MS, PAGE, READ_TIMEOUT_MS,
    RESYNC_FOLLOW_UP_DELAY_MS, TIMER_LIGHT_SEED_FALLBACK, TIMER_READ_DEADLINE,
    TIMER_RESYNC_FOLLOW_UP, TIMER_RESYNC_RETRY, TIMER_SEED_RETRY,
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
    issue_read_sized(state, context, kind, before_offset, false)
}

/// CDXC:SessionChat 2026-11-12 DECISION:
/// The user chose to open a chat once instead of twice (Q2720, "open a chat once, not twice"): every open sent the transcript twice, the socket's snapshot and this core's seed read of the same 300 rows (33 KiB after one turn, 60 KiB after two, measured on the phone's link), and the snapshot outranks the seed anyway. The seed stays, because it carries agent metadata older snapshots lack (CDXC:AgentProviders 2026-09-06 in session/events.rs), but asks for one row when the host says its socket is already up (`StartConfig::light_seed`) or the socket was just replaced: the snapshot then brings the rows. Its rows never paint; if no frame came within [`LIGHT_SEED_GRACE_MS`], the full seed goes out as before. A host that does not set the flag (the desktop, an older phone) keeps today's full seed, so a cold open never waits for the socket.
pub fn issue_light_seed(state: &mut ChatState, context: &ChatContext) -> Effect {
    state.core.timers.arm(
        TIMER_LIGHT_SEED_FALLBACK,
        context.now_ms,
        LIGHT_SEED_GRACE_MS as f64,
    );
    issue_read_sized(state, context, ReadKind::Seed, None, true)
}

fn issue_read_sized(
    state: &mut ChatState,
    context: &ChatContext,
    kind: ReadKind,
    before_offset: Option<u64>,
    light: bool,
) -> Effect {
    let request_id = state.core.allocate_request_id();
    let limit = match kind {
        ReadKind::Page => PAGE,
        _ if light => 1,
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
        light,
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
        // No answer in time is the service not answering, which a refusal never is.
        effects.extend(fail_read(state, context, &read, true));
    }
    arm_read_deadline(state, context);
    effects
}

/// Whether a refusal means Ghostex's service never answered: the desktop's transport failures
/// carry no code, and the phone's SSH bridge says `unreachable`. A refusal the service sent itself
/// always has a code.
pub fn is_unreachable(code: Option<&str>) -> bool {
    matches!(code, None | Some("unreachable"))
}

/// What every surface of the chat says when Ghostex's service did not answer a call, in place of
/// the transport's own words.
pub const UNREACHABLE_MESSAGE: &str = "Couldn't reach Ghostex. It may be restarting.";

/// CDXC:SessionChat 2026-11-05 WHY: User, on a red "Could not read gxserver response: the
/// connection to 127.0.0.1:58744 was closed before an answer arrived (Connection reset by peer (os
/// error 54)) ..." line under a finished turn, while gxserver was being rebuilt: "wtf this pls
/// fix". A call gxserver never answered is a restart or a dropped connection, never something the
/// person can act on from the socket's wording, so every such refusal reaches the families as
/// [`UNREACHABLE_MESSAGE`] under the `unreachable` code (the desktop's transport failures carried
/// no code), which is what draws the error line's Retry. The host keeps the transport's own text in
/// its log (`sessionChat.nativeRpcResult`, apps/desktop/src/app/native_chat/state.rs).
///
/// A session on another computer says that computer's name instead (`session/computer.rs`): the
/// restart words are for this computer's own Ghostex alone.
pub fn humanize_unreachable(outcome: &mut crate::wire::RpcOutcome, remote_message: Option<String>) {
    if let crate::wire::RpcOutcome::Err { code, message, .. } = outcome {
        if is_unreachable(code.as_deref()) {
            *code = Some("unreachable".to_string());
            *message = remote_message.unwrap_or_else(|| UNREACHABLE_MESSAGE.to_string());
        }
    }
}

/// Records that the service answered, or that it did not. A change publishes, because the loading
/// hold's skeleton follows it.
pub fn note_reachable(state: &mut ChatState, context: &ChatContext, reachable: bool) {
    if reachable {
        state.messages.unreachable_failures = 0;
    }
    let since = &mut state.messages.unreachable_since_ms;
    match (reachable, since.is_some()) {
        (true, true) => *since = None,
        (false, false) => *since = Some(context.now_ms),
        _ => return,
    }
    state.core.request_publish();
}

/// What each lane does with a read that failed, including by timeout. `unreachable` is a failure
/// the service never answered (see [`is_unreachable`]): those are retried without limit on the
/// [`unreachable_retry_delay_ms`] ladder and never turn the view into an error.
pub fn fail_read(
    state: &mut ChatState,
    context: &ChatContext,
    read: &OutstandingRead,
    unreachable: bool,
) -> Vec<Effect> {
    if read.generation != state.messages.generation {
        return Vec::new();
    }
    if !matches!(read.kind, ReadKind::Page) {
        note_reachable(state, context, !unreachable);
    }
    let unreachable_delay = || unreachable_retry_delay_ms(state.messages.unreachable_failures);
    match read.kind {
        ReadKind::Resync if unreachable => {
            let delay = unreachable_delay();
            state.messages.unreachable_failures += 1;
            state.messages.resync.in_flight = false;
            state
                .core
                .timers
                .arm(TIMER_RESYNC_RETRY, context.now_ms, delay as f64);
        }
        ReadKind::Seed if unreachable && !state.messages.position.frame_arrived => {
            let delay = unreachable_delay();
            state.messages.unreachable_failures += 1;
            state
                .core
                .timers
                .arm(TIMER_SEED_RETRY, context.now_ms, delay as f64);
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::constants::UNREACHABLE_RETRY_DELAYS_MS;

    fn context(now_ms: f64) -> ChatContext {
        ChatContext {
            now_ms,
            ..Default::default()
        }
    }

    fn read(kind: ReadKind, request_id: u64) -> OutstandingRead {
        OutstandingRead {
            request_id,
            kind,
            generation: 0,
            before_offset: None,
            started_at_ms: 0.0,
            light: false,
        }
    }

    /// Fails one unanswered read of `kind` at `now` and returns the delay its retry was armed for.
    fn fail_unanswered(state: &mut ChatState, kind: ReadKind, now: f64) -> u64 {
        fail_read(state, &context(now), &read(kind, 1), true);
        state
            .core
            .timers
            .next_wake_ms(now)
            .expect("a retry is armed")
    }

    #[test]
    fn unanswered_reads_retry_every_two_seconds_then_back_off() {
        let mut state = ChatState::default();
        let mut now = 1_000_000.0;
        let mut delays = Vec::new();
        for _ in 0..10 {
            let delay = fail_unanswered(&mut state, ReadKind::Resync, now);
            delays.push(delay);
            now += delay as f64;
        }
        assert_eq!(
            delays,
            [2_000, 2_000, 2_000, 2_000, 2_000, 4_000, 8_000, 15_000, 15_000, 15_000]
        );
        // An hour of a dead tunnel is now a few hundred reads per chat, not 1,800.
        let per_hour =
            3_600_000 / UNREACHABLE_RETRY_DELAYS_MS[UNREACHABLE_RETRY_DELAYS_MS.len() - 1];
        assert!(per_hour <= 240);
        // The view never turns into an error for a service that is not answering.
        assert_eq!(state.session.error, None);
    }

    #[test]
    fn any_answer_starts_the_ladder_over() {
        let mut state = ChatState::default();
        let mut now = 1_000_000.0;
        for _ in 0..8 {
            now += fail_unanswered(&mut state, ReadKind::Seed, now) as f64;
        }
        assert_eq!(fail_unanswered(&mut state, ReadKind::Seed, now), 15_000);
        assert!(state.messages.unreachable_since_ms.is_some());
        note_reachable(&mut state, &context(now), true);
        assert_eq!(state.messages.unreachable_since_ms, None);
        assert_eq!(fail_unanswered(&mut state, ReadKind::Seed, now), 2_000);
    }

    #[test]
    fn a_refusal_keeps_its_own_resync_backoff() {
        let mut state = ChatState::default();
        // A refusal is an answer: it does not climb the unanswered ladder.
        fail_read(&mut state, &context(0.0), &read(ReadKind::Resync, 1), false);
        assert_eq!(state.messages.unreachable_failures, 0);
        assert_eq!(state.core.timers.next_wake_ms(0.0), Some(1_000));
    }
}
