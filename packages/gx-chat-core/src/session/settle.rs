//! The `useEffect`s the TypeScript ran between a state change and the composition that read it.
//!
//! The core has no render pass, so they run once per event, after the event's own handler and
//! before `crate::document::assemble`. All of them are from
//! `packages/shared/session-chat-controller/controller.ts`.

use crate::session::composition::{pending_transcript, refresh_boundaried};
use crate::session::constants::{
    DEFAULT_COMMAND_CATALOG, DELIVERY_UNCONFIRMED, LEFT_QUEUE_RETENTION_MS, TIMER_LEFT_QUEUE,
};
use crate::session::pending::prune_pending_sends;
use crate::session::startup_sends::{
    parse_iso_ms, pending_with_startup_sends, settle_rows_gone_from_queue,
};
use crate::session::working::{is_working, working_signal};
use crate::state::{ChatContext, ChatState};

/// Everything that must be true before the composed list is built.
///
/// Not one of the six per-family settle hooks: this runs inside `ChatCore::republish`, between
/// the event's handlers and `crate::session::composition::compose`, because the composition reads
/// what it prunes.
pub fn before_compose(state: &mut ChatState, context: &ChatContext) {
    let catalog: Vec<String> = DEFAULT_COMMAND_CATALOG
        .iter()
        .map(|name| (*name).to_string())
        .collect();
    refresh_boundaried(state, &catalog);
    // The list the echoes reconcile against is built only when something waits on it: it copies
    // the whole transcript, and doing that on every event made each keystroke on a long chat pay
    // for the full history.
    let mut transcript = None;

    if !state.pending.sends.is_empty() {
        let against = reconciled(state, &mut transcript);
        state.pending.sends = prune_pending_sends(&state.pending.sends, against);
    }
    // Keep hydrated sends until the transcript replaces them, including the gap between terminal
    // delivery and the agent flushing its transcript to disk, but no longer than that gap can last.
    if let Some(queue) = state.session.queue_prompts.clone() {
        let oldest_loaded = state
            .messages
            .has_more
            .then(|| {
                state
                    .messages
                    .list
                    .iter()
                    .find_map(|message| message.timestamp)
            })
            .flatten();
        let hydrated = settle_rows_gone_from_queue(
            pending_with_startup_sends(
                &state.pending.sends,
                &queue,
                state.session.agent.as_deref(),
            ),
            &queue,
            context.now_millis(),
            oldest_loaded,
        );
        state.pending.sends = if hydrated.is_empty() {
            hydrated
        } else {
            prune_pending_sends(&hydrated, reconciled(state, &mut transcript))
        };
        // An idle chat gets no other event to move the echo on.
        if let Some(left_at) = state
            .pending
            .sends
            .iter()
            .filter(|entry| {
                entry
                    .startup_delivery
                    .as_ref()
                    .is_some_and(|delivery| delivery.state != DELIVERY_UNCONFIRMED)
            })
            .filter_map(|entry| entry.left_queue_at_ms)
            .min()
        {
            let remaining = left_at + LEFT_QUEUE_RETENTION_MS - context.now_millis();
            state
                .core
                .timers
                .arm_once(TIMER_LEFT_QUEUE, context.now_ms, remaining.max(1) as f64);
        }
    }
    if crate::session::answer_echoes::waiting(state) {
        let against = reconciled(state, &mut transcript);
        crate::session::answer_echoes::settle(state, context, against);
    }

    // Clear the Stop suppression once the live signal settles.
    if state.session.interrupted && !working_signal(state) {
        state.session.interrupted = false;
    }

    confirm_fast_mode_marker(state);
    crate::session::terminal::settle(state, is_working(state));
    let _ = context;
}

/// The list pending echoes are reconciled against, built once per settle on first use.
fn reconciled<'a>(
    state: &ChatState,
    slot: &'a mut Option<Vec<ghostex_gx_protocol::ChatMessage>>,
) -> &'a [ghostex_gx_protocol::ChatMessage] {
    slot.get_or_insert_with(|| pending_transcript(state, &state.messages.boundaried))
}

/// CDXC:SessionChat 2026-09-04 DECISION:
/// User: Codex's terminal-only "Service tier set to priority/default" confirmation must appear in
/// chat as a Fast mode ON/OFF action pill. The rollout records neither message, so the pill is
/// completed only after gxserver's post-command footer probe confirms whether `fast` is present.
fn confirm_fast_mode_marker(state: &mut ChatState) {
    if state
        .session
        .agent
        .as_deref()
        .map(str::to_lowercase)
        .as_deref()
        != Some("codex")
    {
        return;
    }
    let options = state.session.selected_options.as_ref();
    let Some(detected_at) = options
        .and_then(|value| value.get("detectedAt"))
        .and_then(serde_json::Value::as_str)
        .and_then(|stamp| parse_iso_ms(Some(stamp)))
    else {
        return;
    };
    let fast = options
        .and_then(|value| value.get("fast"))
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    let Some(at) = state.pending.markers.iter().rposition(|marker| {
        marker.label.is_none()
            && marker.sent_at_ms <= detected_at
            && marker
                .command
                .trim_matches(crate::session::text::is_js_space)
                .to_lowercase()
                == "/fast"
    }) else {
        return;
    };
    state.pending.markers[at].label = Some(if fast {
        "Fast mode ON".to_string()
    } else {
        "Fast mode OFF".to_string()
    });
}
