//! The `useEffect`s the TypeScript ran between a state change and the composition that read it.
//!
//! The core has no render pass, so they run once per event, after the event's own handler and
//! before `crate::document::assemble`. All of them are from
//! `packages/shared/session-chat-controller/controller.ts`.

use crate::session::composition::{pending_transcript, refresh_boundaried};
use crate::session::constants::DEFAULT_COMMAND_CATALOG;
use crate::session::pending::prune_pending_sends;
use crate::session::startup_sends::{parse_iso_ms, pending_with_startup_sends};
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
    let boundaried = std::mem::take(&mut state.messages.boundaried);
    let transcript = pending_transcript(state, &boundaried);
    state.messages.boundaried = boundaried;

    if !state.pending.sends.is_empty() {
        state.pending.sends = prune_pending_sends(&state.pending.sends, &transcript);
    }
    // Keep hydrated sends until the transcript replaces them, including the gap between terminal
    // delivery and the agent flushing its transcript to disk.
    if let Some(queue) = state.session.queue_prompts.clone() {
        let hydrated = pending_with_startup_sends(&state.pending.sends, &queue);
        state.pending.sends = prune_pending_sends(&hydrated, &transcript);
    }

    // Clear the Stop suppression once the live signal settles.
    if state.session.interrupted && !working_signal(state) {
        state.session.interrupted = false;
    }

    confirm_fast_mode_marker(state);
    crate::session::terminal::settle(state, is_working(state));
    let _ = context;
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
