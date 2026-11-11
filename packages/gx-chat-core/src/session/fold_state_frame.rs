//! [`fold_state`](crate::session::fold::fold_state) for a state frame, in place.
//!
//! CDXC:SessionChat 2026-11-10 WHY: a state frame folded by copying the whole previous fold (every side value: the coordinator threads, the resource index, the queue, the fleet, the draft) and then overwriting nearly all of it with the frame's, on every streamed sample; on a coordinator chat that was most of an event's cost (memharness, coordinator chat shown). The previous fold is now taken by value and only what the frame carries replaces what it held, with each rule exactly as `fold_state` applies it to a state frame. The two must change together.
//! SEE-ALSO: `session/fold.rs` (`fold_state`).

use ghostex_gx_protocol::{ChatStateFrame, Tri};

use crate::session::fold::{carried, merge_draft_state, merge_options, FoldedSnapshot};
use crate::session::view_state::transcript_status_after_state;

/// `previous` with the state frame folded in: what `fold_state(Some(&previous), State(frame))`
/// returns, without copying `previous` first.
pub fn fold_state_frame(mut folded: FoldedSnapshot, frame: &ChatStateFrame) -> FoldedSnapshot {
    let side = &frame.state;
    let result = &mut folded.result;
    result.epoch = frame.base.epoch;
    result.seq = frame.base.seq;
    result.status = transcript_status_after_state(result.status.clone(), frame.status.clone());
    // A state frame names no agent, so the agent and the session agent stay what they were.
    if let Some(lifecycle) = frame.lifecycle.as_ref() {
        result.lifecycle = Some(lifecycle.clone());
    }
    if let Some(working) = frame.working {
        result.working = Some(working);
    }

    let state = &mut result.state;
    // Prompt semantics: a carrier that can hold it and does not has CLEARED it.
    state.prompt = side.prompt.clone();
    state.terminal_notice = side.terminal_notice.clone();
    state.agent_gone = side.agent_gone;
    state.terminal_activity = side.terminal_activity.clone();
    state.agent_fleet = side.agent_fleet.clone();
    state.agent_tasks = side.agent_tasks.clone();
    state.workflows = side.workflows.clone();

    // `incoming ?? previous`: a value the frame does not carry is the one already held.
    macro_rules! carry {
        ($($field:ident),* $(,)?) => {
            $(
                if let Some(value) = side.$field.as_ref() {
                    state.$field = Some(value.clone());
                }
            )*
        };
    }
    carry!(
        retired_async_question_ids,
        later_question_ids,
        removed_question_ids,
        unavailable_modes,
        agent_session_id,
        app_commands,
        returned_prompt,
        side_chats,
        secret_requests,
        side_chat_of,
        agent_message_deliveries,
        queue,
    );
    state.resource_index = carried(side.resource_index.as_ref(), state.resource_index.as_ref());
    state.coordinator_threads = carried(
        side.coordinator_threads.as_ref(),
        state.coordinator_threads.as_ref(),
    );

    // `Tri::Absent` keeps what was held; null and a value replace it.
    macro_rules! carry_tri {
        ($($field:ident),* $(,)?) => {
            $(
                if !matches!(side.$field, Tri::Absent) {
                    state.$field = side.$field.clone();
                }
            )*
        };
    }
    carry_tri!(
        async_questions_since,
        forked_at,
        account_switch,
        api_retry,
        pending_model_selection,
    );

    state.selected_options = merge_options(
        state.selected_options.as_ref(),
        side.selected_options.as_ref(),
    );
    state.screen_probed = match (side.screen_probed, state.screen_probed) {
        (Some(true), _) => Some(true),
        (_, Some(true)) => Some(true),
        (Some(false), previous) => previous.or(Some(false)),
        (None, previous) => previous,
    };

    if let Some(draft) = side.draft.as_ref() {
        let merged = merge_draft_state(state.draft.as_deref(), draft);
        // An equal merge keeps the draft already held, which the session shares.
        if state.draft.as_deref() != Some(&merged) {
            state.draft = Some(merged.into());
        }
    }
    folded
}
