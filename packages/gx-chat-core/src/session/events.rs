//! What family a does with each event: the start, the frames, the reads, and the clock.
//!
//! Ported from the subscribe effect of `packages/shared/session-chat-controller/controller.ts`
//! together with the ordering half of `apps/desktop/sidebar/session-chat-runtime/store.ts`, which
//! the Rust core folds into one place because the broker and the per-view runtime are both gone.

use crate::effect::Effect;
use crate::event::{ChatSettings, ComposerBootRead, ConnectionUpdate, Event, StartConfig};
use crate::session::apply::{
    apply_authoritative, apply_draft_agent_carriage, apply_selected_options,
};
use crate::session::constants::{
    INITIAL_LIMIT, INITIAL_STALL_THRESHOLD_MS, MAX_AUTOMATIC_RECONNECTS, MAX_LIMIT,
    STALL_CHECK_INTERVAL_MS, STALL_THRESHOLD_MS, TIMER_READ_DEADLINE, TIMER_RESYNC_FOLLOW_UP,
    TIMER_RESYNC_RETRY, TIMER_SEED_RETRY, TIMER_STALL, TIMER_TERMINAL_TOOL_HOLD,
};
use crate::session::fold::{fold_append, fold_state, FoldedSnapshot, StateCarrier};
use crate::session::frame_publish::{side_state_moves, FrameIdentity};
use crate::session::pagination::{page_has_more, PageBoundary};
use crate::session::reads::{
    arm_read_deadline, expire_overdue_reads, fail_read, inside_seed_window, issue_read, parse_read,
    request_resync, schedule_resync_follow_up, schedule_seed_retry, take_read,
};
use crate::session::stream::{
    accept_authoritative_frame, accept_sequenced_frame, snapshot_is_stale, Verdict,
};
use crate::session::view_state::select_view_state;
use crate::session::working::{is_working, publish_status, working_signal};
use crate::state::{ChatContext, ChatState, PublishAwait, ReadKind};
use crate::wire::{ChatFrame, RpcOutcome};

/// Handles one event family a owns.
pub fn handle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let mut effects = route(state, event, context);
    // The boundary fill is a `useEffect` on the transcript, so it re-evaluates after every event
    // rather than only after a read.
    effects.extend(crate::session::actions::fill_history_boundary(
        state, context,
    ));
    effects
}

/// Family a's uniform settle hook, the first of the six.
///
/// It owns the seam's own bookkeeping rather than a surface: the answer an in-flight action was
/// waiting on is what ends that action's turn, and `action` in `native-host.ts` published there
/// (`crate::dispatch::actions::dispatch`).
pub fn settle(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    let effects = settle_awaits(state, event);
    // `schedulePersistence()` is called from every place `store.ts` replaces `this.snapshot`, which
    // is every place the core bumps `authoritative_revision`. One check instead of six call sites,
    // and the same decision.
    if state.messages.persisted_revision != state.messages.authoritative_revision {
        state.messages.persisted_revision = state.messages.authoritative_revision;
        crate::session::persistence::schedule(state, context);
    }
    effects
        .into_iter()
        .chain(record_deliveries(state))
        .chain(crate::session::persistence::flush_due(state, context))
        .chain(crate::session::presentation::settle(state, context))
        .collect()
}

/// `useEffect(() => { options.onDeliveredDrafts(syncedDraft?.deliveredDrafts ?? []) },
/// [syncedDraft])`, whose host arm writes only when the list is not empty.
fn record_deliveries(state: &mut ChatState) -> Vec<Effect> {
    if state.session.deliveries_recorded_revision == state.session.synced_draft_revision {
        return Vec::new();
    }
    state.session.deliveries_recorded_revision = state.session.synced_draft_revision;
    let deliveries: Vec<serde_json::Value> = state
        .session
        .synced_draft
        .as_ref()
        .and_then(|draft| draft.get("deliveredDrafts"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    if deliveries.is_empty() {
        return Vec::new();
    }
    vec![Effect::RecordDeliveries { deliveries }]
}

/// The publish chain's own bookkeeping, which is what the settle above is mostly for.
fn settle_awaits(state: &mut ChatState, event: &Event) -> Vec<Effect> {
    match event {
        Event::RpcSettled {
            request_id,
            outcome,
        } => {
            let awaited = state.core.awaits(&PublishAwait::Rpc(*request_id));
            state
                .core
                .settle_publish_await(&PublishAwait::Rpc(*request_id));
            // The arm is suspended on this call, so a refusal is a throw out of it. Remembered
            // rather than applied here, because the family that owns the request settles later
            // and may claim the refusal for a surface of its own.
            if awaited {
                if let crate::wire::RpcOutcome::Err { message, code, .. } = outcome.as_ref() {
                    state.core.awaited_refusal = Some((message.clone(), code.clone()));
                }
            }
        }
        Event::StorageLoaded { key, .. } | Event::StorageWritten { key, .. } => {
            state
                .core
                .settle_publish_await(&PublishAwait::Storage(key.clone()));
        }
        _ => {}
    }
    Vec::new()
}

/// The event's own handler.
fn route(state: &mut ChatState, event: &Event, context: &ChatContext) -> Vec<Effect> {
    match event {
        Event::Start(config) => start(state, config, context),
        Event::ComposerBootRead(read) => boot_read(state, read, context),
        Event::ComposerBootFailed { error } => boot_failed(state, error),
        Event::RetainedSnapshotLoaded { value } => {
            crate::session::persistence::adopt(state, value.as_deref(), context);
            Vec::new()
        }
        Event::Frame(frame) => frame_arrived(state, frame, context),
        Event::Connection(update) => connection(state, *update, context),
        Event::RpcSettled {
            request_id,
            outcome,
        } => rpc_settled(state, *request_id, outcome, context),
        Event::Tick => tick(state, context),
        Event::SettingsChanged(settings) => {
            state.core.hide_account_emails = settings.hide_account_emails;
            state.core.title = settings.title.clone();
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The host is opening this chat.
///
/// Nothing the document can see moves here, and nothing is subscribed: like `start` in
/// `native-host.ts`, the core asks for `composer('read')` and waits, so the first frame the host
/// drains carries no snapshot at all. The identity, the cached transcript and the first two calls
/// all land in [`boot_read`]. The one read that does go out here is the store's own hydration
/// (`apps/desktop/sidebar/session-chat-runtime/store.ts:113`, in the `RetainedSession`
/// constructor), which runs beside the boot read rather than after it.
fn start(state: &mut ChatState, config: &StartConfig, context: &ChatContext) -> Vec<Effect> {
    state.identity.project_id = config.project_id.clone();
    state.identity.session_id = config.session_id.clone();
    state.messages.retained_key = config.retained_key.clone();
    state.messages.generation += 1;
    state.messages.limit = state.messages.limit.max(INITIAL_LIMIT);
    state.messages.last_frame_at_ms = context.now_ms;
    state.messages.seed_started_at_ms = context.now_ms;
    state.messages.seed_attempt = 0;
    state.session.boot_config = Some(config.clone());
    // `bootConfig = config; if (booting) return;` (`native-host.ts:640`): a second `start` while a
    // boot read is still in flight replaces the config and asks for nothing.
    if state.session.boot_read_request.is_some() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.session.boot_read_request = Some(request_id);
    vec![
        Effect::ReadComposerBoot { request_id },
        crate::session::persistence::read_at_boot(),
    ]
}

/// `start(bootConfig)` from a `retry` that arrives before the controller exists: a boot read that
/// failed is asked for again, one still in flight is left alone (`if (booting) return`).
pub fn restart_boot(state: &mut ChatState) -> Vec<Effect> {
    if state.session.boot_config.is_none() || state.session.boot_read_request.is_some() {
        return Vec::new();
    }
    let request_id = state.core.allocate_request_id();
    state.session.boot_read_request = Some(request_id);
    vec![Effect::ReadComposerBoot { request_id }]
}

/// `start`'s own `.catch`: the transcript is emptied and the document becomes the error state.
///
/// `booting` is cleared in the `.finally`, so a refused read leaves the chat retryable rather than
/// wedged. Until 2026-09-22 the core cleared `boot_read_request` only on success and had no
/// failure arm at all, so a refused `composer('read')` never started a controller and the core
/// published nothing for the rest of the session.
fn boot_failed(state: &mut ChatState, error: &str) -> Vec<Effect> {
    state.session.boot_read_request = None;
    state.core.boot_error = Some(error.to_string());
    Vec::new()
}

/// The boot read answered: adopt the client id and the cached transcript, then subscribe and seed.
///
/// The port of `start`'s `.then(...)` in `native-host.ts`: it adopts the catalog, the preferences
/// and the settings, pushes `composerInit`, and only then builds the controller, whose subscribe
/// effect opens the stream and whose seed read fills the transcript.
pub fn boot_read(
    state: &mut ChatState,
    read: &ComposerBootRead,
    context: &ChatContext,
) -> Vec<Effect> {
    state.session.boot_read_request = None;
    state.core.controller_started = true;
    state.identity.client_id = read.client_id.clone();
    state.identity.session_key = read.session_key.clone();
    // `adoptNativeChatSettings(result.chatSettings)` (`native-host.ts:649`): the boot read is
    // where the two settings FIRST arrive, before any push. Without it the core masked no account
    // text until a `chatSettings` broker message came, which on a real session never does.
    if let Ok(settings) = serde_json::from_value::<ChatSettings>(read.chat_settings.clone()) {
        state.core.hide_account_emails = settings.hide_account_emails;
        state.core.title = settings.title;
    }
    let config = state.session.boot_config.clone().unwrap_or_default();
    state.core.preview_settings = config.preview.clone();
    // The `sessionChanged` branch of the subscribe effect (`controller.ts:853`): the agent
    // identity, the status line's own options and the working directory come back from the shared
    // cache before anything is read, which is what makes a return to a chat immediate.
    // `setWorkingDirectory` was read again on every publish in `native-host.ts:415`, and it is what
    // turns a diff card's absolute path into one relative to the project.
    crate::session::presentation::seed(state, config.initial_presentation.as_ref());

    // Retained data is folded before the first paint, so a session switch never shows an empty
    // transcript between mount and the subscription's first frame. `getCachedSnapshot` reads the
    // store's own `this.snapshot`, so a hydration read that has already landed owns the tail and
    // this must not roll it back.
    if let Some(cached) = config
        .initial_snapshot
        .as_ref()
        .filter(|_| state.messages.snapshot.is_none())
        .and_then(|value| serde_json::from_value::<FoldedSnapshot>(value.clone()).ok())
    {
        state.messages.position.epoch = Some(cached.result.epoch);
        state.messages.position.seq = cached.result.seq;
        state.messages.limit = state
            .messages
            .limit
            .max(cached.result.messages.len() as u32)
            .min(MAX_LIMIT);
        // A retained fold whose identity contradicts the cache belongs to a conversation this one
        // has moved on from, so its read-only draft-agent fields stay out.
        let matching = crate::session::presentation::identity_matches(state, &cached.result);
        if matching {
            apply_draft_agent_carriage(state, &cached.result);
        }
        apply_authoritative(state, &cached.result, matching, context);
        state.messages.snapshot = Some(cached);
        state.messages.authoritative_revision += 1;
        // The host handed this fold over; writing it straight back would only restamp it.
        state.messages.persisted_revision = state.messages.authoritative_revision;
    }

    // The stall watchdog runs for as long as the chat is open. Nothing below the socket reports a
    // follower that stopped delivering: without a frame there is no gap, and without a gap the fold
    // rules never ask for a resync.
    state
        .core
        .timers
        .arm_interval(TIMER_STALL, context.now_ms, STALL_CHECK_INTERVAL_MS as f64);

    vec![
        Effect::Subscribe {
            limit: state.messages.limit,
            catalog: true,
        },
        issue_read(state, context, ReadKind::Seed, None),
    ]
}

/// One accepted frame.
fn frame_arrived(state: &mut ChatState, frame: &ChatFrame, context: &ChatContext) -> Vec<Effect> {
    // Liveness is "a frame reached us", not "a frame changed something": a dropped or duplicate
    // frame still proves the stream is alive.
    state.messages.last_frame_at_ms = context.now_ms;
    // CDXC:SessionChat 2026-09-22 WHY:
    // An authoritative frame behind the one already accepted, on the same daemon generation, is a
    // late duplicate and rolls the whole transcript backwards for one frame. `snapshot_is_stale`
    // was written for this and had NO CALLER, so every out-of-order snapshot folded. The store
    // drops it before its listeners ever see it (`store.ts:200`), which is why the check belongs
    // ahead of the in-flight bookkeeping below rather than inside the fold.
    if let ChatFrame::Snapshot(snapshot) | ChatFrame::Replaced(snapshot) = frame {
        if snapshot_is_stale(
            &state.messages.position,
            state.messages.position.frame_arrived,
            &snapshot.base.server_id,
            snapshot.base.epoch,
            snapshot.base.seq,
        ) {
            return Vec::new();
        }
    }
    let position = frame.position();
    if state.messages.resync.in_flight {
        // Remember how far the live stream ran while the read was in flight; the read answers from
        // a position captured before it.
        let ahead = state
            .messages
            .resync
            .seen_in_flight
            .as_ref()
            .is_none_or(|seen| position.is_ahead_of(seen));
        if ahead {
            state.messages.resync.seen_in_flight = Some(position.clone());
        }
    }

    match frame {
        ChatFrame::Snapshot(snapshot) | ChatFrame::Replaced(snapshot) => {
            // `const previous = this.serverId && this.serverId !== event.serverId ? undefined :
            // this.snapshot` (`store.ts:208`): a different daemon restarts the numbering, so
            // nothing of the old one's state may carry into the new one's fold.
            let carried = if state.messages.position.server_id.is_empty()
                || state.messages.position.server_id == snapshot.base.server_id
            {
                state.messages.snapshot.as_ref()
            } else {
                None
            };
            let folded = fold_state(carried, StateCarrier::Snapshot(snapshot));
            // The view takes the frame's own fields (`applyAuthoritative(event, …)`); the fold
            // below is what is retained.
            let own = crate::session::fold::snapshot_frame_result(snapshot);
            accept_authoritative_frame(
                &mut state.messages.position,
                &snapshot.base.server_id,
                snapshot.base.epoch,
                snapshot.base.seq,
            );
            // Ordinary daemon frames do not own the three read-only draft-agent fields; a host
            // that synthesizes snapshots from reads does, including a cleared one on promotion, so
            // a frame that carries any of them carries all three (`controller.ts`, the
            // `'sessionAgentId' in event` test).
            let before = FrameIdentity::capture(state);
            let mut moved =
                side_state_moves(state, snapshot.lifecycle.as_ref(), true, &snapshot.state);
            if crate::session::fold::snapshot_owns_draft_agents(snapshot) {
                // The read path's `carries_new_agents`, plus the clear an owned `null` makes: the
                // switcher leaving is a change the document must publish too.
                let agents_before = (
                    state.session.session_agent_id.clone(),
                    state.session.available_agents.clone(),
                    state.session.switchable_agents.clone(),
                );
                apply_draft_agent_carriage(state, &own);
                moved |= carries_new_agents(&own)
                    || agents_before
                        != (
                            state.session.session_agent_id.clone(),
                            state.session.available_agents.clone(),
                            state.session.switchable_agents.clone(),
                        );
            }
            apply_authoritative(state, &own, true, context);
            state.messages.snapshot = Some(folded);
            state.messages.authoritative_revision += 1;
            if moved || before != FrameIdentity::capture(state) {
                state.core.request_render();
            }
            Vec::new()
        }
        ChatFrame::Appended(appended) => {
            match accept_sequenced_frame(
                &mut state.messages.position,
                appended.base.epoch,
                appended.base.seq,
            ) {
                Verdict::Drop => Vec::new(),
                Verdict::Resync => request_resync(state, context),
                Verdict::Apply => {
                    let Some(previous) = state.messages.snapshot.clone() else {
                        return request_resync(state, context);
                    };
                    // Retract first: the rows that replace an abandoned prompt can ride the very
                    // same frame.
                    if state.messages.remove_ids(&appended.superseded_message_ids) {
                        state.messages.snapshot = Some(previous.clone());
                        state.messages.authoritative_revision += 1;
                        // `setTranscript(mergerRef.current.list)` on a retraction.
                        state.messages.new_composition_identity();
                    }
                    let folded = fold_append(&previous, appended);
                    if !appended.messages.is_empty() {
                        state.messages.apply_append(&appended.messages);
                        // `setTranscript(mergerRef.current.list)`: `applySessionChatMergerAppend`
                        // copies the list, and a row it replaced or added is a new object, so the
                        // assembler never sees a suffix extension by identity.
                        state.messages.new_composition_identity();
                        // Keep the read window at least as large as what is on screen, so a later
                        // resync or pagination read cannot answer with less than the live list
                        // already holds.
                        let on_screen = state
                            .messages
                            .list
                            .len()
                            .saturating_sub(state.messages.history_prefix_count)
                            as u32;
                        state.messages.limit = state.messages.limit.max(on_screen).min(MAX_LIMIT);
                        state.session.server_status = ghostex_gx_protocol::ChatStatus::Ready;
                    }
                    if let Some(lifecycle) = appended.lifecycle.clone() {
                        state.session.lifecycle = Some(lifecycle);
                        // `setLifecycle(event.lifecycle)`: a new object, so a publish.
                        state.core.request_render();
                    }
                    state.messages.snapshot = Some(folded);
                    state.messages.authoritative_revision += 1;
                    Vec::new()
                }
            }
        }
        ChatFrame::State(frame) => {
            match accept_sequenced_frame(
                &mut state.messages.position,
                frame.base.epoch,
                frame.base.seq,
            ) {
                Verdict::Drop => Vec::new(),
                Verdict::Resync => request_resync(state, context),
                Verdict::Apply => {
                    // The retained fold still absorbs the frame (that is what the host persists
                    // and what a reopen seeds from); the VIEW takes the frame's own fields.
                    let folded =
                        fold_state(state.messages.snapshot.as_ref(), StateCarrier::State(frame));
                    state.messages.snapshot = Some(folded);
                    let before = FrameIdentity::capture(state);
                    let moved =
                        side_state_moves(state, frame.lifecycle.as_ref(), false, &frame.state);
                    crate::session::apply::apply_state_frame(state, frame, context);
                    if moved || before != FrameIdentity::capture(state) {
                        state.core.request_render();
                    }
                    Vec::new()
                }
            }
        }
    }
}

/// The socket's own state changed.
fn connection(
    state: &mut ChatState,
    update: ConnectionUpdate,
    context: &ChatContext,
) -> Vec<Effect> {
    state.messages.last_frame_at_ms = context.now_ms;
    match update {
        ConnectionUpdate::Subscribed => Vec::new(),
        ConnectionUpdate::Lost => Vec::new(),
        ConnectionUpdate::Resubscribed => {
            // A fresh socket restarts the stream position: nothing applies until its snapshot
            // re-seeds the epoch, and a snapshot replaces the content wholesale.
            resubscribe(state, context)
        }
    }
}

/// The subscribe effect re-running: a fresh socket, a fresh seed read, and everything a read in
/// flight across the swap must not be allowed to finish.
///
/// The VIEW state is deliberately left alone. A same-session recycle is a reconnect of the same
/// conversation, and wiping there destroyed a perfectly good view: status fell back to `loading`,
/// whose early return unmounts the whole pane, composer included, mid-typing. The sequencing reset
/// here already guarantees no stale frame can fold in.
pub fn resubscribe(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    state.messages.position.frame_arrived = false;
    state.messages.generation += 1;
    state.messages.reads.clear();
    state.messages.load_earlier_request = None;
    state.messages.history_epoch = None;
    state.messages.history_prefix_count = 0;
    state.messages.boundary_attempt = None;
    state.messages.loading_earlier = false;
    state.messages.resync = Default::default();
    state.messages.last_frame_at_ms = context.now_ms;
    state.messages.last_watchdog_resync_at_ms = 0.0;
    state.messages.seed_started_at_ms = context.now_ms;
    state.messages.seed_attempt = 0;
    state.session.working_started_at_ms = None;
    for key in [
        TIMER_SEED_RETRY,
        TIMER_RESYNC_RETRY,
        TIMER_RESYNC_FOLLOW_UP,
        TIMER_READ_DEADLINE,
    ] {
        state.core.timers.cancel(key);
    }
    state
        .core
        .timers
        .arm_interval(TIMER_STALL, context.now_ms, STALL_CHECK_INTERVAL_MS as f64);
    vec![issue_read(state, context, ReadKind::Seed, None)]
}

/// A read the core asked for has settled.
fn rpc_settled(
    state: &mut ChatState,
    request_id: u64,
    outcome: &RpcOutcome,
    context: &ChatContext,
) -> Vec<Effect> {
    match outcome {
        RpcOutcome::Ok { result } => {
            let Some(read) = parse_read(result) else {
                return Vec::new();
            };
            let Some(request) = take_read(state, request_id) else {
                return Vec::new();
            };
            arm_read_deadline(state, context);
            if request.generation != state.messages.generation {
                // The answer belongs to the previous conversation.
                return Vec::new();
            }
            match request.kind {
                ReadKind::Seed => seed_read_settled(state, &read, context),
                ReadKind::Resync => resync_read_settled(state, &read, context),
                ReadKind::Page => page_read_settled(state, &read, &request, context),
            }
        }
        RpcOutcome::Err { .. } => {
            let Some(request) = state
                .messages
                .reads
                .iter()
                .position(|read| read.request_id == request_id)
                .map(|at| state.messages.reads.remove(at))
            else {
                return Vec::new();
            };
            let effects = fail_read(state, context, &request);
            arm_read_deadline(state, context);
            effects
        }
    }
}

/// The seed read's answer. The seed transcript is outranked by the first snapshot or replacement
/// frame; its read-only agent metadata still needs to be applied.
fn seed_read_settled(
    state: &mut ChatState,
    read: &ghostex_gx_protocol::ReadSessionChatResult,
    context: &ChatContext,
) -> Vec<Effect> {
    // CDXC:AgentProviders 2026-09-06 WHY:
    // Older live snapshots omit the agent family and account-menu metadata. Dropping the entire
    // seed read when a snapshot won the race made Switch Account disappear from otherwise identical
    // Claude sessions. Keep the newer transcript while accepting the read's identity; a read from
    // an older stream generation must resync instead of restoring an obsolete agent.
    if state.messages.position.frame_arrived {
        if state
            .messages
            .position
            .epoch
            .is_some_and(|epoch| read.epoch < epoch)
        {
            return request_resync(state, context);
        }
        // The carriage's `setAvailableAgents(result.availableAgents ?? null)` and the options'
        // setter take new objects, which re-renders and publishes even when nothing moved.
        let before = FrameIdentity::capture(state);
        let moved = carries_new_agents(read);
        apply_draft_agent_carriage(state, read);
        apply_selected_options(state, read.state.selected_options.as_ref());
        if read.state.screen_probed == Some(true) {
            state.session.screen_probed = true;
        }
        if moved || before != FrameIdentity::capture(state) {
            state.core.request_render();
        }
        return Vec::new();
    }
    state.messages.last_frame_at_ms = context.now_ms;
    state.messages.position.epoch = Some(read.epoch);
    state.messages.position.seq = read.seq;
    apply_read_result(state, read, context);
    if matches!(read.status, ghostex_gx_protocol::ChatStatus::Starting)
        && inside_seed_window(state, context)
    {
        schedule_seed_retry(state, context);
    }
    Vec::new()
}

/// `applyDraftAgentCarriage(result); applyAuthoritative(result, …)` for a seed or resync read,
/// and the render the TypeScript brain ran when that handed a setter a new object.
///
/// A read result is parsed fresh, so every object it carries is new to the setters: the
/// frame's rule ([`side_state_moves`]) plus `setAvailableAgents(result.availableAgents ?? null)`
/// and `setSwitchableAgents(...)`, which a read carries and an ordinary frame does not. The core
/// published a read only when the document changed, so a resync that confirmed what was on
/// screen shipped nothing where the TypeScript brain re-rendered and published.
fn apply_read_result(
    state: &mut ChatState,
    read: &ghostex_gx_protocol::ReadSessionChatResult,
    context: &ChatContext,
) {
    let before = FrameIdentity::capture(state);
    let moved = side_state_moves(state, read.lifecycle.as_ref(), true, &read.state)
        || carries_new_agents(read);
    apply_draft_agent_carriage(state, read);
    apply_authoritative(state, read, true, context);
    if moved || before != FrameIdentity::capture(state) {
        state.core.request_render();
    }
}

/// `setAvailableAgents(result.availableAgents ?? null)` and `setSwitchableAgents(...)` handed a
/// freshly parsed array.
fn carries_new_agents(read: &ghostex_gx_protocol::ReadSessionChatResult) -> bool {
    read.available_agents.is_some()
        || read
            .switchable_agents
            .as_ref()
            .and_then(serde_json::Value::as_array)
            .is_some_and(|rows| !rows.is_empty())
}

/// The resync read's answer.
fn resync_read_settled(
    state: &mut ChatState,
    read: &ghostex_gx_protocol::ReadSessionChatResult,
    context: &ChatContext,
) -> Vec<Effect> {
    state.messages.resync.in_flight = false;
    // The read landed, so the stream is answering again: drop the failure backoff and count this as
    // liveness for the watchdog.
    state.messages.resync.failures = 0;
    state.messages.last_frame_at_ms = context.now_ms;
    state.core.timers.cancel(TIMER_RESYNC_RETRY);

    let observed = state.messages.resync.seen_in_flight.take();
    let read_position = crate::wire::StreamPosition {
        server_id: state.messages.position.server_id.clone(),
        epoch: read.epoch,
        seq: read.seq,
    };
    let outrun = observed
        .as_ref()
        .is_some_and(|seen| seen.is_ahead_of(&read_position));
    if outrun
        && observed
            .as_ref()
            .is_some_and(|seen| seen.epoch > read.epoch)
    {
        // A newer generation already replaced the tail; this result is from the previous one and
        // must not clobber it.
        schedule_resync_follow_up(state, context);
        return Vec::new();
    }
    state.messages.position.epoch = Some(read.epoch);
    // Frames seen during the flight were already accounted for; keeping the cursor at the read's
    // older seq would make every following append look like a gap and resync forever.
    state.messages.position.seq = match observed.as_ref().filter(|_| outrun) {
        Some(seen) => seen.seq,
        None => read.seq,
    };
    apply_read_result(state, read, context);
    if outrun {
        schedule_resync_follow_up(state, context);
    } else {
        state.messages.resync.follow_ups = 0;
    }
    Vec::new()
}

/// One history page's answer, from `loadEarlier`.
fn page_read_settled(
    state: &mut ChatState,
    read: &ghostex_gx_protocol::ReadSessionChatResult,
    request: &crate::state::OutstandingRead,
    context: &ChatContext,
) -> Vec<Effect> {
    let Some(pending) = state.messages.load_earlier_request.clone() else {
        return Vec::new();
    };
    if pending.before_offset != request.before_offset.unwrap_or(0)
        || pending.generation != request.generation
    {
        return Vec::new();
    }
    state.messages.load_earlier_request = None;
    state.messages.loading_earlier = false;
    if matches!(read.status, ghostex_gx_protocol::ChatStatus::Error) {
        // Same lane as a rejection: the live tail is still valid and `hasMore` stays set.
        return Vec::new();
    }
    if state.messages.position.epoch != pending.epoch {
        // A replacement rebuilt the tail while this page was in flight.
        return Vec::new();
    }
    let older: Vec<ghostex_gx_protocol::ChatMessage> = read
        .messages
        .iter()
        .filter(
            |message| match state.messages.index_by_id.get(&message.id) {
                None => true,
                // The same id on a different row (a shared response id) is real history, not a
                // duplicate: the merger re-keys it on the way in.
                Some(at) => state.messages.list.get(*at).is_some_and(|existing| {
                    crate::session::assembler::id_collides(existing, message)
                }),
            },
        )
        .cloned()
        .collect();
    for message in &older {
        state.messages.note_arrival(message, false);
    }
    let mut rows = older.clone();
    rows.extend(state.messages.list.iter().cloned());
    state.messages.replace_list(&rows);
    // `setTranscript(merger.list)`: the same objects shifted right, so the assembler resets and
    // the memo re-runs only when the page actually prepended rows.
    if !older.is_empty() {
        state.messages.new_composition_identity();
    }
    // Grow the read window so a later resync answers with at least the history already on screen.
    state.messages.history_epoch = pending.epoch;
    state.messages.history_prefix_count += older.len();
    state.messages.has_more = page_has_more(
        PageBoundary {
            message_count: read.messages.len(),
            has_more: read.has_more,
            has_more_exact: read.has_more_exact,
            before_offset: read.before_offset,
        },
        Some(pending.before_offset),
    );
    state.messages.before_offset = read.before_offset;
    // Older pages never rewind the live lifecycle or status.
    let _ = context;
    Vec::new()
}

/// The clock: every deadline family a armed, answered in the order the host's own `tick` would.
fn tick(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let mut effects = Vec::new();
    if state.core.timer_fired(TIMER_READ_DEADLINE) {
        effects.extend(expire_overdue_reads(state, context));
    }
    if state.core.timer_fired(TIMER_SEED_RETRY) {
        effects.push(issue_read(state, context, ReadKind::Seed, None));
    }
    if state.core.timer_fired(TIMER_RESYNC_RETRY) {
        effects.extend(request_resync(state, context));
    }
    if state.core.timer_fired(TIMER_RESYNC_FOLLOW_UP) {
        effects.extend(request_resync(state, context));
    }
    if state.core.timer_fired(TIMER_TERMINAL_TOOL_HOLD) {
        state.pending.terminal_tool = None;
        state.pending.terminal_tool_hold_until_ms = None;
    }
    if state.core.timer_fired(TIMER_STALL) {
        effects.extend(stall_watchdog(state, context));
    }
    effects
}

/// The stall watchdog, from the `setInterval` at the end of the subscribe effect.
fn stall_watchdog(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    // `const now = Date.now()` at the top of the callback: a read of its own, one past the tick's.
    // A resync read that answered inside the same millisecond as the previous check makes the
    // 20 s threshold a matter of that millisecond, and on a real chat it is, every fourth tick.
    let now = state.core.timer_now(TIMER_STALL, context);
    // Initial window: the working gate below cannot protect it, because a session that never
    // resolved its transcript may never report work. A silent socket here leaves the view in its
    // blank loading hold forever, and only a new subscription can re-request the snapshot that was
    // lost. Rebuilding restamps `last_frame_at_ms`, so the next tick starts a fresh window rather
    // than firing again immediately.
    if !state.messages.position.frame_arrived
        && loading_hold(state)
        && state.messages.auto_reconnects < MAX_AUTOMATIC_RECONNECTS
        && now - state.messages.last_frame_at_ms > INITIAL_STALL_THRESHOLD_MS
    {
        state.messages.auto_reconnects += 1;
        let mut effects = vec![Effect::Reconnect];
        effects.extend(resubscribe(state, context));
        return effects;
    }
    if !working_signal(state) || state.session.interrupted {
        return Vec::new();
    }
    if now - state.messages.last_frame_at_ms <= STALL_THRESHOLD_MS
        || now - state.messages.last_watchdog_resync_at_ms <= STALL_THRESHOLD_MS
    {
        return Vec::new();
    }
    // Stamped before the call: a read that succeeds but answers with the same stale tail leaves
    // `last_frame_at_ms` fresh, but one that is dropped by the in-flight guard must not let the
    // watchdog fire again on the next tick.
    state.messages.last_watchdog_resync_at_ms = now;
    request_resync(state, context)
}

/// Whether the pane is still holding blank, which is the initial watchdog's own gate.
fn loading_hold(state: &ChatState) -> bool {
    let working = is_working(state);
    let status = publish_status(
        &state.session.server_status,
        working,
        state.session.error.is_some(),
    );
    let view = select_view_state(
        &status,
        state.messages.composed.len(),
        state.session.error.as_deref(),
    );
    view.kind == "loading" || view.kind == "starting"
}
