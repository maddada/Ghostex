//! Whether the agent is working, which drives Stop versus Send, the typing indicator, and whether
//! the transcript may fold a turn into "Worked for Xs".
//!
//! Ported from `packages/core-ui/chat/session-chat-working-status.ts` and the derivation block of
//! `packages/shared/session-chat-controller/controller.ts`. The server already derives a base
//! status from the transcript lifecycle; the client re-checks the lifecycle so a terminal boundary
//! can settle a stale "working" signal without waiting for the next frame.

use ghostex_gx_protocol::{
    ChatBlock, ChatMessage, ChatRole, ChatSource, ChatStatus, TurnLifecycle, TurnLifecycleState,
};

use crate::session::constants::{LIFECYCLE_CLOCK_SKEW_SLACK_MS, REAL_EPOCH_FLOOR_MS};
use crate::state::QuietCard;

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

/// Whether a Completed or Interrupted lifecycle is about the newest row: its time is absent, or it
/// is no older than the newest row the transcript recorded. A later prompt or row means a newer
/// generation has started.
pub fn newest_turn_ended(state: &crate::state::ChatState) -> bool {
    let Some(lifecycle) = state.session.lifecycle.as_ref() else {
        return false;
    };
    if !matches!(
        lifecycle.state,
        TurnLifecycleState::Completed | TurnLifecycleState::Interrupted
    ) {
        return false;
    }
    let Some(stamp) = lifecycle.timestamp else {
        return true;
    };
    state
        .messages
        .composed
        .iter()
        // An informational warning can land after the row that ended the turn.
        .filter(|row| row.source == ChatSource::Transcript && row.role != ChatRole::System)
        .filter_map(|row| row.timestamp)
        .max()
        .is_none_or(|newest| stamp >= newest)
}

/// Whether Agent work Quiet hides the response still running (`transcript/grouped_work.rs`
/// `hide_work`).
///
/// CDXC:SessionChat 2026-11-08 WHY: a hidden reply is far worse than extra words, so this hides only when every signal agrees the agent is mid-response, and the words show whenever one doubts it. [`lifecycle_terminates_current_turn`] compares a Completed with when this client began watching, so a chat opened after the turn ended (subagents still running, or an explicit working whose Stop was lost) reads as working for as long as that lasts, with the reply out of sight; [`newest_turn_ended`] compares it with the newest row instead. [`is_working`] settling on trailing prose while [`transcript_working`] does not left a chat with neither words nor a status line, so a mid-turn sentence on an agent that reports no Working (Cursor) shows until its next tool row.
pub fn hides_running_response(state: &crate::state::ChatState) -> bool {
    state.transcript_view.hidden_work()
        && !agent_exited(state)
        && ((transcript_working(state) && is_working(state) && !newest_turn_ended(state))
            || quiet_holds_turn_open(state))
}

/// While Agent work Quiet hides the running response: the time up to which its rows are from runs
/// that already finished, whose replies stay in view. The lifecycle's own stamp separates them (a
/// Working stamps when this run began, a stale Completed when the one before it ended), or when
/// this client began watching for an agent that has reported neither.
///
/// CDXC:SessionChat 2026-11-08 WHY: a run with no prompt or harness row of its own (a goal's next step) leaves the finished replies inside the running response (`active_response_start_index`), where they would vanish. It is a time rather than remembered rows: the follower drains once a second and keeps only the last lifecycle, so a Completed and the next Working in one drain reach the client as Working alone, and a chat opened mid-run has seen nothing. It is every reply up to that time rather than the newest, which vanished when a chain of steps moved on to the next one; that also keeps some words from those earlier steps.
pub fn quiet_finished_before(state: &crate::state::ChatState) -> Option<i64> {
    if !hides_running_response(state) {
        return None;
    }
    state
        .session
        .lifecycle
        .as_ref()
        .and_then(|lifecycle| lifecycle.timestamp)
        .or(state
            .session
            .working_started_at_ms
            .map(|at| at.round() as i64))
}

/// Whether a card is up that the agent waits on: an approval, a question, or a dialog in its
/// terminal (a permission prompt, a plan to approve).
pub fn waits_on_user(state: &crate::state::ChatState) -> bool {
    use crate::questions::gates::{notice_visible, question_card_visible};
    use crate::questions::model::{InteractivePrompt, TerminalNotice};
    let session = &state.session;
    if session.interrupted || (session.prompt.is_none() && session.terminal_notice.is_none()) {
        return false;
    }
    let prompt = InteractivePrompt::parse(session.prompt.as_ref());
    let notice = TerminalNotice::parse(session.terminal_notice.as_ref());
    question_card_visible(state, prompt.as_ref(), notice.as_ref())
        || (notice_visible(state)
            && notice
                .as_ref()
                .is_some_and(|notice| notice.passthrough || notice.holds_input()))
}

/// Whether gxserver reports that the agent left its terminal (`agentExited`, read off the screen):
/// nothing runs any more, whatever the working signal or the rows say.
pub fn agent_exited(state: &crate::state::ChatState) -> bool {
    crate::questions::model::TerminalNotice::parse(state.session.terminal_notice.as_ref())
        .is_some_and(|notice| notice.kind == "agentExited")
}

/// Agent work Quiet: a turn waiting on the user is still running, whatever the working signal says.
/// Claude in manual mode reads as idle while it waits on a permission, which drew the turn as
/// finished: its reply and its calls came into view beside the card, and left once it was answered.
/// So is one whose card just left, until [`track_quiet_card`] lets go.
pub fn quiet_holds_turn_open(state: &crate::state::ChatState) -> bool {
    state.transcript_view.hidden_work()
        && !newest_turn_ended(state)
        && !agent_exited(state)
        && (waits_on_user(state)
            || asks_a_question(state)
            || matches!(state.transcript_view.quiet_card, QuietCard::Left(_)))
}

/// Whether the newest row ends in a question the agent asks (`AskUserQuestion` and the like) that
/// has no answer yet: its card is on its way, so the agent waits on the user already.
///
/// CDXC:SessionChat 2026-11-09 WHY: the agent stops reporting work as soon as it asks, a moment before gxserver's question card arrives, and in that gap Quiet drew the running turn as finished, "3 tool calls" with the question's sentence as its reply, for under a second. Sebastian decided that must never show, not even for a frame.
fn asks_a_question(state: &crate::state::ChatState) -> bool {
    state
        .messages
        .composed
        .iter()
        .rev()
        .find(|row| row.source == ChatSource::Transcript && row.role != ChatRole::System)
        .and_then(|row| row.blocks.last())
        .is_some_and(|block| {
            matches!(block, ChatBlock::ToolCall { name, .. }
                if crate::questions::exchange::is_question_tool_name(name))
        })
}

/// Agent work Quiet: keeps a turn open in the gap between its card leaving and the agent reporting
/// again. Run once per pass, before [`quiet_holds_turn_open`] is read.
///
/// CDXC:SessionChat 2026-11-08 WHY: answering a card leaves the working signal false for a moment (Claude reads as idle while it waits on the user), and with nothing bridging that gap the transcript recorded its newest row as settled and drew the running turn as finished, its work button and in-between words included, until the next row. No timer: the hold rests on the row the card left on, and only when that row ends in a call or a call's result, which no finished turn ends on, so a reply is never hidden by it. It lets go when working returns, on a new row other than a call's bare result (the answer being recorded, which arrives before the agent reports and is not it reporting), on a Completed or Interrupted lifecycle, on an interrupt, or once gxserver reports that the agent left its terminal ([`agent_exited`]; 2026-11-09, Sebastian decided an agent that dies there must not keep its words hidden until the next send or reload, which this hold did before).
pub fn track_quiet_card(state: &mut crate::state::ChatState) {
    let next =
        if !state.transcript_view.hidden_work()
            || state.session.interrupted
            || agent_exited(state)
            || newest_turn_ended(state)
        {
            QuietCard::None
        } else if waits_on_user(state) || asks_a_question(state) {
            QuietCard::Up
        } else if transcript_working(state) {
            QuietCard::None
        } else {
            let newest =
                state.messages.composed.iter().rev().find(|row| {
                    row.source == ChatSource::Transcript && row.role != ChatRole::System
                });
            match (&state.transcript_view.quiet_card, newest) {
                (QuietCard::Up, Some(row)) if ends_in_tool_block(row) => {
                    QuietCard::Left(row.id.clone())
                }
                (QuietCard::Left(at), Some(row)) if *at == row.id || holds_only_results(row) => {
                    QuietCard::Left(row.id.clone())
                }
                _ => QuietCard::None,
            }
        };
    state.transcript_view.quiet_card = next;
}

/// Whether the agent owes more after `row`: it ends in a call or a call's result.
fn ends_in_tool_block(row: &ChatMessage) -> bool {
    matches!(
        row.blocks.last(),
        Some(ChatBlock::ToolCall { .. } | ChatBlock::ToolResult { .. })
    )
}

/// Whether `row` is only calls' results.
fn holds_only_results(row: &ChatMessage) -> bool {
    !row.blocks.is_empty()
        && row
            .blocks
            .iter()
            .all(|block| matches!(block, ChatBlock::ToolResult { .. }))
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

/// Whether the working card above the composer (and its working word) presents the agent as
/// working.
///
/// CDXC:SessionChat 2026-11-04 WHY:
/// The card read the raw live signal while Stop versus Send read [`is_working`], which settles that signal against the turn lifecycle, so a stale "working" from the server (an OpenCode turn whose end its TUI plugin never reported, GitHub #403) left the card on "Sprouting…" beside an enabled Send. The card now takes the same settled answer, plus the raw signal while a subagent still works, because subagents outlive the lead's turn and keep the thread working (CDXC:SessionStatus 2026-09-28 in server/src/presentation/session_attributes.rs).
pub fn presents_working(state: &crate::state::ChatState) -> bool {
    is_working(state)
        || (working_signal(state) && !state.session.interrupted && has_working_subagent(state))
}

/// Whether the subagent roster the server last sent has a child still working.
fn has_working_subagent(state: &crate::state::ChatState) -> bool {
    state
        .session
        .agent_fleet
        .as_ref()
        .and_then(|fleet| fleet.get("agents"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|agents| {
            agents.iter().any(|agent| {
                agent.get("status").and_then(serde_json::Value::as_str) == Some("working")
            })
        })
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
