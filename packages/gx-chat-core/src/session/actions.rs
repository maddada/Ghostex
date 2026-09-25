//! Family a's user actions: the three that drive the connection and the history window.
//!
//! `retry` rebuilds the subscription, `refresh` re-reads authoritative state, and `loadEarlier`
//! pages backwards. Ported from `reconnect`, `requestResync` and `loadEarlier` in
//! `packages/shared/session-chat-controller/controller.ts`.

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::session::events::resubscribe;
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
    });
    vec![issue_read(
        state,
        context,
        ReadKind::Page,
        Some(before_offset),
    )]
}

/// The automatic boundary fill: a transcript whose first row is not a user turn starts
/// mid-conversation, so one page is fetched without the user asking.
///
/// It runs once per cursor, so a boundary that stays unresolved cannot become a read loop.
pub fn fill_history_boundary(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    let starts_mid_turn = state
        .messages
        .list
        .first()
        .is_some_and(|first| !matches!(first.role, ghostex_gx_protocol::ChatRole::User));
    if !starts_mid_turn || !state.messages.has_more || state.messages.loading_earlier {
        return Vec::new();
    }
    let cursor = state.messages.before_offset;
    if state.messages.boundary_attempt == Some(cursor) {
        return Vec::new();
    }
    state.messages.boundary_attempt = Some(cursor);
    load_earlier(state, context)
}
