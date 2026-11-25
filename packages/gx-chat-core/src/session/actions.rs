//! Family a's user actions: the three that drive the connection and the history window.
//!
//! `retry` rebuilds the subscription, `refresh` re-reads authoritative state, and `loadEarlier`
//! pages backwards. Ported from `reconnect`, `requestResync` and `loadEarlier` in
//! `packages/shared/session-chat-controller/controller.ts`.

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::session::constants::RELEASE_HISTORY_ABOVE;
use crate::session::events::resubscribe;
use crate::session::pagination::{page_has_more, PageBoundary};
use crate::session::reads::{issue_read, request_resync};
use crate::state::{ChatContext, ChatState, LoadEarlierRequest, ReadKind};

/// Handles one action family a owns. An action it does not own returns nothing, which is how the
/// dispatcher's default arm reads "not mine".
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    if matches!(
        action.kind,
        ActionKind::Retry | ActionKind::Refresh | ActionKind::LoadEarlier
    ) {
        // All three reached the end of the TypeScript's `action`, which published whatever
        // happened.
        state.core.request_publish();
    }
    match action.kind {
        // Tear the subscription down and rebuild it: a fresh socket plus a fresh seed read. The
        // only recovery for a socket that came up but never delivered its subscribe snapshot,
        // which no re-read can repair.
        ActionKind::Retry => {
            let mut effects = vec![Effect::Reconnect];
            effects.extend(resubscribe(state, context));
            effects
        }
        // Re-read authoritative state now. Callers use it after an action whose result lives only
        // in a read result, because no frame carries those fields.
        ActionKind::Refresh => request_resync(state, context),
        ActionKind::LoadEarlier => load_earlier(state, context),
        ActionKind::ReleaseHistory => {
            if release_history(state) {
                state.core.request_publish();
            }
            Vec::new()
        }
        ActionKind::ComputerRetry => crate::session::computer::retry(state),
        _ => Vec::new(),
    }
}

/// One page backwards.
///
/// The request's identity is `(epoch, beforeOffset, generation)`: a cancelled response must not be
/// allowed to complete a newer request in the same epoch, and a page that crosses a resubscribe
/// belongs to the previous conversation.
pub fn load_earlier(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    if state.messages.loading_earlier
        || state.messages.load_earlier_request.is_some()
        || !state.messages.has_more
    {
        return Vec::new();
    }
    state.messages.loading_earlier = true;
    let before_offset = state.messages.before_offset;
    state.messages.load_earlier_request = Some(LoadEarlierRequest {
        epoch: state.messages.position.epoch,
        before_offset,
        generation: state.messages.generation,
        island: false,
    });
    vec![issue_read(
        state,
        context,
        ReadKind::Page,
        Some(before_offset),
    )]
}

/// Drops the older history "Load earlier" (or a jump read) paged in above the live window, for a
/// reader back at the newest rows; the "Load earlier" cursor goes back to where the live window
/// starts, so scrolling up pages the same history in again. Says whether anything was dropped.
///
/// CDXC:SessionChat 2026-11-26 WHY: paged-in history was kept for as long as the chat was, and
/// nothing gave it back: a coordinator chat scrolled to its top once held 8,307 messages and
/// 852 MB and rebuilt all of them on the chat engine's one thread whenever its rows moved, which
/// is what made Enter take 170 ms to register in every chat. The renderer asks for this when its
/// list starts following the tail again (desktop `native_chat/pagination.rs`). Only a prefix of
/// more than [`RELEASE_HISTORY_ABOVE`] rows goes, and never while a page read, Find in chat or a
/// "Go to chat" is using it. The rows kept are the live window as the fold holds it, which is
/// exactly what a chat opened afresh holds, and its cursor is the fold's, as a re-read that does
/// not overlap the prefix leaves it (`apply_authoritative`).
pub fn release_history(state: &mut ChatState) -> bool {
    let messages = &state.messages;
    if messages.loading_earlier
        || messages.load_earlier_request.is_some()
        || state.extras.search.open
        || state.questions.resource_go.is_some()
    {
        return false;
    }
    let Some(folded) = messages.snapshot.as_ref() else {
        return false;
    };
    let Some(at) = folded
        .rows
        .first()
        .and_then(|first| messages.index_by_id.get(&first.id).copied())
    else {
        return false;
    };
    if at <= RELEASE_HISTORY_ABOVE {
        return false;
    }
    let window = PageBoundary {
        message_count: folded.rows.len(),
        has_more: folded.result.has_more,
        has_more_exact: folded.result.has_more_exact,
        before_offset: folded.result.before_offset,
    };
    let kept = messages.list[at..].to_vec();
    let released: Vec<String> = messages.list[..at]
        .iter()
        .map(|row| row.id.clone())
        .collect();
    let messages = &mut state.messages;
    messages.replace_list(&kept);
    messages.new_composition_identity();
    messages.has_more = page_has_more(window, None);
    messages.before_offset = window.before_offset;
    messages.history_epoch = None;
    messages.history_prefix_count = 0;
    messages.island_loaded = false;
    // The window may start inside a turn; its boundary fill already ran for this cursor when the
    // chat opened, and running it again would page in what was just let go.
    messages.boundary_attempt = Some(window.before_offset);
    // The projection's per-message models are the bulk of what those rows held.
    let projected = &mut state.transcript_view.projected;
    for id in &released {
        projected.remove(id);
    }
    true
}

/// A jump read: the window of history that ends just before `before_offset`, merged into the rows
/// without touching the "Load earlier" cursor or its `has_more`. "Go to chat" on a Resources item
/// older than the loaded rows reads the window that ends at the item's reply this way, so the chat
/// scrolls to it without paging in every message between (a 626 MB transcript cannot afford that).
/// The rows between the window and the loaded tail stay unloaded until "Load earlier" reaches them.
pub fn load_window_ending_at(
    state: &mut ChatState,
    context: &ChatContext,
    before_offset: u64,
) -> Vec<Effect> {
    if state.messages.loading_earlier || state.messages.load_earlier_request.is_some() {
        return Vec::new();
    }
    state.messages.loading_earlier = true;
    state.messages.load_earlier_request = Some(LoadEarlierRequest {
        epoch: state.messages.position.epoch,
        before_offset,
        generation: state.messages.generation,
        island: true,
    });
    vec![issue_read(
        state,
        context,
        ReadKind::Page,
        Some(before_offset),
    )]
}

/// The automatic boundary fill: a transcript whose first row does not open a turn starts
/// mid-conversation, so one page is fetched without the user asking.
///
/// It runs once per cursor, so a boundary that stays unresolved cannot become a read loop.
///
/// CDXC:SessionChat 2026-09-27 WHY: a background-task notification, a held prompt and a hidden slash command are user-role rows too, so testing the role stopped the fill on them. A long turn whose window began at a task notification then held no prompt at all, and summary mode drew an empty chat with only "Load earlier turns". The fill uses the summary's own turn test instead.
pub fn fill_history_boundary(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let starts_mid_turn = state
        .messages
        .list
        .first()
        .is_some_and(|first| !crate::transcript::turns::opens_turn(first));
    if !starts_mid_turn
        || !state.messages.has_more
        || state.messages.loading_earlier
        || state.messages.island_loaded
    {
        return Vec::new();
    }
    let cursor = state.messages.before_offset;
    if state.messages.boundary_attempt == Some(cursor) {
        return Vec::new();
    }
    state.messages.boundary_attempt = Some(cursor);
    load_earlier(state, context)
}
