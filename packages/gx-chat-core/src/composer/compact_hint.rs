//! The "Option + Enter to compact then send" hint, pinned at the left of the chat box's pill strip
//! on a Codex chat whose newest turn is more than a day old.
//!
//! CDXC:SessionChat 2026-11-14 DECISION:
//! User, for Codex: "put a pill saying "Option + Enter to compact then send" show this on codex when it's an idle thread for more than 24h (that's when the cache gets busted for codex)". The hint shows on a Codex chat whose newest transcript row is more than 24 hours old, while the agent is not working and the chat can queue (Compact & Send queues the message behind `/compact`), and goes once a new turn lands. It names Option on a Mac keyboard and Alt elsewhere. Option+Enter (Alt+Enter) was already Compact & Send (`SubmissionMode::Compact`: `/compact`, then the message queued until the compaction's turn ends), so the key is unchanged; plain Enter still sends as before. The phone has no Option key and shows no hint.

use crate::document::CompactHint;
use crate::state::{ChatContext, ChatState};
use ghostex_gx_protocol::ChatSource;

/// How long a Codex chat sits idle before the hint shows: when its prompt cache is gone.
pub const CODEX_IDLE_CACHE_MS: i64 = 24 * 60 * 60 * 1000;

/// The timer that redraws the chat when its newest turn turns a day old with the chat open.
pub const COMPACT_HINT_TIMER: &str = "composer.compactHint";

/// When the newest transcript row of a Codex chat was recorded, or `None` for any other agent or a
/// chat with no stamped row yet.
fn codex_newest_row_at(state: &ChatState) -> Option<i64> {
    if crate::extras::agents::transcript_agent([state.session.agent.as_deref(), None])
        != Some("codex")
    {
        return None;
    }
    state
        .messages
        .composed
        .iter()
        .filter(|row| row.source == ChatSource::Transcript)
        .filter_map(|row| row.timestamp)
        .max()
}

/// The hint, or `None` when it does not apply.
pub fn compact_hint(state: &ChatState, context: &ChatContext) -> Option<CompactHint> {
    let newest = codex_newest_row_at(state)?;
    if (context.now_ms as i64).saturating_sub(newest) <= CODEX_IDLE_CACHE_MS
        || crate::session::working::is_working(state)
        || !crate::composer::document::queue(state)
            .capabilities
            .can_queue
    {
        return None;
    }
    let words = "to compact then send";
    Some(CompactHint {
        mac_label: format!("Option + Enter {words}"),
        label: format!("Alt + Enter {words}"),
        aria_label: "This chat has been idle for over a day. Press Option or Alt with Enter to compact it, then send your message".to_string(),
    })
}

/// Arms the redraw for the moment the newest turn of an open Codex chat turns a day old.
pub fn settle(state: &mut ChatState, context: &ChatContext) {
    if state.core.timer_fired(COMPACT_HINT_TIMER) {
        state.core.request_publish();
    }
    let due = codex_newest_row_at(state)
        .map(|newest| (newest.saturating_add(CODEX_IDLE_CACHE_MS) + 1) as f64)
        .filter(|due| *due > context.now_ms);
    match due {
        Some(due) => {
            state
                .core
                .timers
                .arm(COMPACT_HINT_TIMER, context.now_ms, due - context.now_ms)
        }
        None => {
            state.core.timers.cancel(COMPACT_HINT_TIMER);
        }
    }
}
