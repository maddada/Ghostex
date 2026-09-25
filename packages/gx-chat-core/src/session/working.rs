//! Whether the agent is working, which drives Stop versus Send, the typing indicator, and whether
//! the transcript may fold a turn into "Worked for Xs".
//!
//! Ported from `packages/core-ui/chat/session-chat-working-status.ts` and the derivation block of
//! `packages/shared/session-chat-controller/controller.ts`. The server already derives a base
//! status from the transcript lifecycle; the client re-checks the lifecycle so a terminal boundary
//! can settle a stale "working" signal without waiting for the next frame.

use ghostex_gx_protocol::{ChatMessage, ChatRole, ChatStatus, TurnLifecycle, TurnLifecycleState};

use crate::session::constants::{LIFECYCLE_CLOCK_SKEW_SLACK_MS, REAL_EPOCH_FLOOR_MS};

/// Whether this lifecycle ends the turn that started at `working_started_at`.
pub fn lifecycle_terminates_current_turn(
    lifecycle: Option<&TurnLifecycle>,
    working_started_at: Option<i64>,
) -> bool {
    let Some(lifecycle) = lifecycle else {
        return false;
    };
    if !matches!(
        lifecycle.state,
        TurnLifecycleState::Completed | TurnLifecycleState::Interrupted
    ) {
        return false;
    }
    let (Some(started), Some(stamp)) = (working_started_at, lifecycle.timestamp) else {
        // Omitted and null timestamps are valid on the wire; prefer the terminal marker over a
        // stuck spinner, because lifecycle is last-wins and a newer user generation would have
        // replaced it with 'working'.
        return true;
    };
    if stamp >= started {
        return true;
    }
    if stamp > REAL_EPOCH_FLOOR_MS && started > REAL_EPOCH_FLOOR_MS {
        // Clock skew slack, REAL epochs only; the floor keeps small logical clocks strictly
        // ordered.
        return stamp + LIFECYCLE_CLOCK_SKEW_SLACK_MS >= started;
    }
    false
}

/// Whether the transcript's last row is assistant prose written after the turn began, which is the
/// recovery path for a turn whose lifecycle never settled.
pub fn trailing_assistant_post_dates(
    transcript: &[ChatMessage],
    working_started_at: Option<i64>,
) -> bool {
    let Some(started) = working_started_at else {
        return false;
    };
    transcript.last().is_some_and(|message| {
        matches!(message.role, ChatRole::Assistant)
            && message.timestamp.is_some_and(|stamp| stamp >= started)
    })
}

/// What `deriveSessionChatWorkingOverride` decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkingOverride {
    /// The live signal stands: the turn is working.
    Working,
    /// No override; the server's status is used as is.
    None,
}

/// The live-work override.
pub fn derive_working_override(
    working: bool,
    lifecycle: Option<&TurnLifecycle>,
    working_started_at: Option<i64>,
    has_working_subagents: bool,
    transcript: &[ChatMessage],
) -> WorkingOverride {
    if !working {
        return WorkingOverride::None;
    }
    let terminates = lifecycle_terminates_current_turn(lifecycle, working_started_at);
    if terminates
        && lifecycle
            .is_some_and(|lifecycle| matches!(lifecycle.state, TurnLifecycleState::Interrupted))
    {
        // An explicit interruption ends the WHOLE turn, children included.
        return WorkingOverride::None;
    }
    if has_working_subagents {
        return WorkingOverride::Working;
    }
    if terminates {
        return WorkingOverride::None;
    }
    let mid_turn =
        lifecycle.is_some_and(|lifecycle| matches!(lifecycle.state, TurnLifecycleState::Working));
    if !mid_turn && trailing_assistant_post_dates(transcript, working_started_at) {
        // Prose recovery: available whenever the latest lifecycle is NOT an explicit in-progress
        // generation. Mid-turn keeps prose off so partial assistant rows do not settle early.
        return WorkingOverride::None;
    }
    WorkingOverride::Working
}

/// `mergeSessionChatStatus`, the status arm of the live merge.
pub fn merge_status(
    server_status: ChatStatus,
    loading: bool,
    error: bool,
    override_working: WorkingOverride,
) -> ChatStatus {
    if error {
        return ChatStatus::Error;
    }
    // Live work WINS over loading: 'working' drives Stop-vs-Send, the typing indicator, and the
    // streaming preview.
    if loading && override_working != WorkingOverride::Working {
        return ChatStatus::Loading;
    }
    match override_working {
        WorkingOverride::Working => ChatStatus::Working,
        WorkingOverride::None => server_status,
    }
}

/// The raw live signal, before the lifecycle settle.
///
/// Three independent starts: the `working` flag on reads and snapshots, the server's
/// activity-transition state frames, and the host's own signal. From `controller.ts`.
pub fn working_signal(state: &crate::state::ChatState) -> bool {
    let session = &state.session;
    let optimistic = state
        .pending
        .sends
        .iter()
        .any(|entry| entry.queued_prompt_id.is_none());
    let compacting = session
        .terminal_activity
        .as_ref()
        .and_then(|activity| activity.get("kind"))
        .and_then(|kind| kind.as_str())
        == Some("compacting");
    optimistic
        || compacting
        || session.server_working
        || matches!(session.server_status, ChatStatus::Working)
        || session.external_working
}

/// Whether the transcript keeps the newest turn open: the live signal, with the Stop suppression
/// applied, until the turn lifecycle ends the current run.
///
/// CDXC:SessionChat 2026-09-24 SEE-ALSO:
/// The user's decision (fold right away, no settle hold) was first recorded on
/// `sessionChatTranscriptWorking` in `packages/core-ui/chat/session-chat-working-status.ts`.
/// Unlike [`is_working`] there is no trailing-prose recovery, which would fold and unfold a turn at
/// every commentary line.
pub fn transcript_working(state: &crate::state::ChatState) -> bool {
    let session = &state.session;
    working_signal(state)
        && !session.interrupted
        && !lifecycle_terminates_current_turn(
            session.lifecycle.as_ref(),
            session
                .working_started_at_ms
                .map(|value| value.round() as i64),
        )
}

/// Records when the current working run began.
///
/// The TypeScript did this during render (`workingStartedAtRef.current ??= Date.now()`), and it
/// is load bearing: without a start boundary the PREVIOUS turn's completed lifecycle would settle
/// the new turn instantly, which is the dead-indicator bug. The core has no render pass, so the
/// stamp is taken once per event, before the document is assembled.
pub fn stamp_working_started(state: &mut crate::state::ChatState, now_ms: f64) {
    if working_signal(state) {
        state.session.working_started_at_ms.get_or_insert(now_ms);
    } else {
        state.session.working_started_at_ms = None;
    }
}

/// Whether the chat presents as working: the settled override, or a locally accepted send that
/// owns the presentation until the transcript advances past its user turn.
pub fn is_working(state: &crate::state::ChatState) -> bool {
    let session = &state.session;
    let optimistic = state
        .pending
        .sends
        .iter()
        .any(|entry| entry.queued_prompt_id.is_none());
    let override_working = derive_working_override(
        working_signal(state),
        session.lifecycle.as_ref(),
        session
            .working_started_at_ms
            .map(|value| value.round() as i64),
        false,
        &state.messages.list,
    );
    (optimistic || override_working == WorkingOverride::Working) && !session.interrupted
}

/// The status the document publishes.
///
/// Live work can arrive before the seed read, so unresolved transcript states stay authoritative
/// and cannot be mistaken for confirmed emptiness. From `controller.ts`.
pub fn publish_status(server_status: &ChatStatus, working: bool, has_error: bool) -> ChatStatus {
    if has_error {
        return ChatStatus::Error;
    }
    match server_status {
        ChatStatus::Loading | ChatStatus::Starting => server_status.clone(),
        _ if working => ChatStatus::Working,
        ChatStatus::Working => ChatStatus::Ready,
        other => other.clone(),
    }
}
