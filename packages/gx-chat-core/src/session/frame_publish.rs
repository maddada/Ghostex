//! Whether a chat frame moves a value the TypeScript brain's lifecycle republished on.
//!
//! The TypeScript's lifecycle (`session-chat-controller/lifecycle.ts`) published whenever a
//! `useState` setter was handed a value that was not `Object.is` the current one, whether or not
//! the document it then built changed. `onEvent` in `controller.ts` handed most of a frame's side
//! state straight to its setters as the objects the frame parsed into, so a frame that CARRIES a
//! lifecycle, a prompt, a notice, the fleet, the task list, the app commands, the retired question
//! ids, the account switch, the pending model selection or a draft is a publish, even when every
//! value equals the last one. The core publishes when the document changed, so it missed those:
//! about seven hundred documents on the real recordings, each the frame's turn.
//!
//! [`FrameIdentity`] is the part of the state the core can compare before and after a frame (the
//! primitive setters and the two identity counters it already keeps); [`side_state_moves`] is the
//! part only the frame can answer, because a fresh object is a change by definition.

use ghostex_gx_protocol::{ChatSideState, Tri, TurnLifecycle};
use serde_json::Value;

use crate::state::ChatState;

/// The setters whose values the core holds as primitives or counters, read before and after.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameIdentity {
    status: String,
    server_working: bool,
    session_activity_working: bool,
    screen_probed: bool,
    async_questions_since: Option<i64>,
    agent_session_id: Option<String>,
    error: Option<String>,
    has_more: bool,
    /// `setSelectedOptions(next)` with a freshly built object.
    selected_options_generation: u64,
    /// `setSyncedDraft(merge(...))`, which always answers a new object.
    synced_draft_revision: u64,
}

impl FrameIdentity {
    pub fn capture(state: &ChatState) -> Self {
        Self {
            status: format!("{:?}", state.session.server_status),
            server_working: state.session.server_working,
            session_activity_working: state.session.session_activity_working,
            screen_probed: state.session.screen_probed,
            async_questions_since: state.session.async_questions_since,
            agent_session_id: state.session.agent_session_id.clone(),
            error: state.session.error.clone(),
            has_more: state.messages.has_more,
            selected_options_generation: state.session.selected_options_generation,
            synced_draft_revision: state.session.synced_draft_revision,
        }
    }
}

/// Whether the frame's side state is handed to a setter as a new object, measured BEFORE it is
/// applied (an omitted object field clears to `null`, which is a change only when one was set).
///
/// `lifecycle_clears` is the snapshot's `setLifecycle(result.lifecycle ?? null)`; a state or
/// appended frame only sets a lifecycle it carries.
pub fn side_state_moves(
    state: &ChatState,
    lifecycle: Option<&TurnLifecycle>,
    lifecycle_clears: bool,
    side: &ChatSideState,
) -> bool {
    let session = &state.session;
    let cleared =
        |carried: &Option<Value>, current: &Option<Value>| carried.is_some() || current.is_some();
    let nullable = |carried: &Tri<Value>, current: &Tri<Value>| match carried {
        Tri::Absent => false,
        Tri::Value(Value::Null) | Tri::Null => {
            matches!(current, Tri::Value(value) if !value.is_null())
        }
        Tri::Value(_) => true,
    };
    lifecycle.is_some()
        || (lifecycle_clears && session.lifecycle.is_some())
        || cleared(&side.prompt, &session.prompt)
        || cleared(&side.terminal_notice, &session.terminal_notice)
        || cleared(&side.terminal_activity, &session.terminal_activity)
        || cleared(&side.agent_fleet, &session.agent_fleet)
        || cleared(&side.agent_tasks, &session.agent_tasks)
        || side.app_commands.is_some()
        || side.retired_async_question_ids.is_some()
        || nullable(&side.account_switch, &session.account_switch)
        || nullable(
            &side.pending_model_selection,
            &session.pending_model_selection,
        )
}
