//! Applying an authoritative read or frame to the state.
//!
//! Ported from `applyAuthoritative`, `applyAgentIdentity`, `applyQueueCarriage` and
//! `applyDraftAgentCarriage` in `packages/shared/session-chat-controller/controller.ts`.

use ghostex_gx_protocol::{ChatStatus, ReadSessionChatResult, Tri};
use serde_json::Value;

use crate::session::fold::{merge_draft_state, merge_options_detail};
use crate::session::pagination::{page_has_more, PageBoundary};
use crate::session::text::normalize_pending_text;
use crate::state::ChatState;

/// The identity fields a read or a frame can carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentIdentity {
    pub agent: Option<String>,
    pub agent_session_id: Option<String>,
    /// Three-state, unlike the two above: a read spells the draft's own agent as
    /// `result.sessionAgentId ?? null`, so an omission on a read is the promotion signal and CLEARS
    /// it, while a frame carries no field at all and must leave it alone.
    pub session_agent_id: Tri<String>,
}

/// The `sessionChatState` arm of the controller's `onEvent` (`controller.ts:1010`), field by field.
///
/// A state frame carries no transcript, and the controller never runs it through
/// `applyAuthoritative`: it sets the status, the working flags, the lifecycle, the prompt, the
/// identity and the carriage keys the frame names, and touches nothing else. The core used to fold
/// the frame onto the retained snapshot and apply THAT as if a read had landed, which replaced the
/// list with the fold's window, re-derived `hasMore` and the cursor from it, and reset the history
/// prefix on every frame (a page read per frame on a real chat, none of which the TypeScript
/// brain made), and read `working` from the fold's previous value where the controller reads the
/// frame alone.
pub fn apply_state_frame(
    state: &mut ChatState,
    frame: &ghostex_gx_protocol::ChatStateFrame,
    context: &crate::state::ChatContext,
) {
    state.session.server_status = crate::session::view_state::transcript_status_after_state(
        state.session.server_status.clone(),
        frame.status.clone(),
    );
    state.session.server_working =
        frame.working == Some(true) || matches!(frame.status, ChatStatus::Working);
    if let Some(working) = frame.working {
        state.session.session_activity_working = working;
    }
    if let Some(lifecycle) = frame.lifecycle.clone() {
        state.session.lifecycle = Some(lifecycle);
    }
    state.session.prompt = frame.state.prompt.clone();
    apply_agent_identity(
        state,
        &AgentIdentity {
            agent: None,
            agent_session_id: frame.state.agent_session_id.clone(),
            session_agent_id: Tri::Absent,
        },
    );
    if let Tri::Value(since) = frame.state.async_questions_since {
        state.session.async_questions_since = Some(since);
    } else if matches!(frame.state.async_questions_since, Tri::Null) {
        state.session.async_questions_since = None;
    }
    if let Some(ids) = frame.state.retired_async_question_ids.clone() {
        state.session.retired_async_question_ids = ids;
    }
    apply_selected_options(state, frame.state.selected_options.as_ref());
    state.session.terminal_notice = frame.state.terminal_notice.clone();
    crate::session::terminal::apply_terminal_activity(
        state,
        frame.state.terminal_activity.as_ref(),
        context,
    );
    state.session.agent_fleet = frame.state.agent_fleet.clone();
    state.session.agent_tasks = frame.state.agent_tasks.clone();
    if let Some(commands) = frame.state.app_commands.clone() {
        // `if (event.appCommands) setAppCommands(event.appCommands)`: a fresh array each time.
        state.session.app_commands = commands;
        state.messages.new_composition_identity();
    }
    if let Some(prompt) = frame.state.returned_prompt.clone() {
        apply_returned_prompt(state, &prompt);
    }
    if frame.state.screen_probed == Some(true) {
        state.session.screen_probed = true;
    }
    apply_queue_carriage(
        state,
        &frame.state.account_switch,
        &frame.state.pending_model_selection,
        frame.state.queue.as_ref(),
        frame.state.draft.as_ref(),
    );
}

/// CDXC:AgentProviders 2026-09-14 WHY:
/// Invalidate retained usage before folding a replacement agent's options, so an options-only
/// reply cannot inherit the previous provider's usage.
/// Learning an identity for the first time does not invalidate evidence already received from that
/// same conversation.
pub fn apply_agent_identity(state: &mut ChatState, patch: &AgentIdentity) {
    let session = &mut state.session;
    let account_changed = differs(&session.agent, &patch.agent)
        || differs(
            &session.session_agent_id,
            &patch.session_agent_id.value().cloned(),
        );
    let changed = account_changed || differs(&session.agent_session_id, &patch.agent_session_id);
    // `accountChanged ? {accounts: undefined} : {}` on the presentation write below: the cached
    // account belongs to the account that is going away.
    session.presentation.account_changed |= account_changed;
    if changed {
        // `setSelectedOptions(null)` bails out when it was already null, so only a real clear is
        // a new identity for family e1's detection dep.
        if session.selected_options.is_some() {
            session.selected_options_generation =
                session.selected_options_generation.wrapping_add(1);
        }
        session.selected_options = None;
        session.screen_probed = false;
        session.async_questions_since = None;
        session.retired_async_question_ids = Vec::new();
    }
    if patch.agent.is_some() {
        session.agent = patch.agent.clone();
    }
    if patch.agent_session_id.is_some() {
        session.agent_session_id = patch.agent_session_id.clone();
    }
    if !patch.session_agent_id.is_absent() {
        session.session_agent_id = patch.session_agent_id.value().cloned();
    }
}

/// Both sides known and different. A field learned for the first time never counts as a change.
fn differs(current: &Option<String>, next: &Option<String>) -> bool {
    matches!((current, next), (Some(current), Some(next)) if current != next)
}

/// CDXC:AgentScreenDetection 2026-09-08 WHY:
/// Option captures have their own timestamps; a transcript snapshot winning the seed-read race
/// must not discard terminal evidence.
/// The option store orders each field by source, then capture time; an older reply carrying
/// stronger evidence must reach that store too.
pub fn apply_selected_options(state: &mut ChatState, detected: Option<&Value>) {
    if detected.is_none() {
        return;
    }
    let (merged, fresh_identity) =
        merge_options_detail(state.session.selected_options.as_ref(), detected);
    state.session.selected_options = merged;
    // `setSelectedOptions(next)` re-renders only on a new object, and that render is what re-fires
    // family e1's detection effect. The counter is that identity, carried where a Rust `Option`
    // cannot.
    if fresh_identity {
        state.session.selected_options_generation =
            state.session.selected_options_generation.wrapping_add(1);
    }
}

/// CDXC:SessionChat 2026-09-04 DECISION:
/// User: the optimistic echo of a prompt the agent handed back must leave the transcript with it.
/// The echo never had a transcript twin to prune it (the message was never recorded), and an
/// Escape typed in the terminal never reached this client's own interrupt, so the returned prompt
/// is what retires it.
pub fn apply_returned_prompt(state: &mut ChatState, prompt: &Value) {
    state.session.returned_prompt = Some(prompt.clone());
    let returned = normalize_pending_text(prompt.get("text").and_then(Value::as_str).unwrap_or(""));
    state
        .pending
        .sends
        .retain(|entry| normalize_pending_text(&entry.text) != returned);
}

/// Folds the two queue-carriage fields with their DIFFERENT omission rules: an absent `queue`
/// leaves the capability, and the list, exactly as it was; an absent `draft` means unchanged.
/// Neither ever clears anything, because clearing a draft is an explicit empty `content` from the
/// server.
pub fn apply_queue_carriage(
    state: &mut ChatState,
    account_switch: &Tri<Value>,
    pending_model_selection: &Tri<Value>,
    queue: Option<&Vec<Value>>,
    draft: Option<&Value>,
) {
    if !matches!(account_switch, Tri::Absent) {
        state.session.account_switch = account_switch.clone();
    }
    if !matches!(pending_model_selection, Tri::Absent) {
        state.session.pending_model_selection = pending_model_selection.clone();
    }
    if let Some(queue) = queue {
        // Every state frame repeats the (usually empty) queue; only a real change replaces it, so
        // a new identity for the same rows cannot dirty every consumer down to the projection.
        if state.session.queue_prompts.as_ref() != Some(queue) {
            state.session.queue_prompts = Some(queue.clone());
        }
    }
    if let Some(draft) = draft {
        state.session.synced_draft = Some(merge_draft_state(
            state.session.synced_draft.as_ref(),
            draft,
        ));
        state.session.synced_draft_revision += 1;
    }
}

/// CDXC:Drafts 2026-08-28:
/// Folded from READ RESULTS ONLY. Snapshot, replaced and state frames have no field for either
/// value, so folding them in the authoritative path (which frames also go through) would clear the
/// switcher on the next frame. An omission on a read is the promotion signal and clears both.
pub fn apply_draft_agent_carriage(state: &mut ChatState, result: &ReadSessionChatResult) {
    apply_agent_identity(
        state,
        &AgentIdentity {
            agent: result.agent.clone(),
            agent_session_id: result.state.agent_session_id.clone(),
            session_agent_id: match result.session_agent_id.clone() {
                Some(id) => Tri::Value(id),
                None => Tri::Null,
            },
        },
    );
    state.session.available_agents = result.available_agents.clone();
    state.session.switchable_agents = result
        .switchable_agents
        .clone()
        .filter(|value| value.as_array().is_none_or(|rows| !rows.is_empty()));
}

/// Applies one authoritative result: a seed read, a resync read, a snapshot, or the cached
/// snapshot the host seeded the core with.
///
/// `restore_presentation` is false only when the identity the host had cached disagrees with the
/// one the snapshot carries, which is the case a stale presentation cache must not win.
pub fn apply_authoritative(
    state: &mut ChatState,
    result: &ReadSessionChatResult,
    restore_presentation: bool,
    context: &crate::state::ChatContext,
) {
    // A detail-mode history prefix survives a re-read of the same epoch when the window still
    // overlaps it; otherwise the prefix is dropped and the cursor restarts.
    let overlap = result
        .messages
        .first()
        .and_then(|first| state.messages.index_by_id.get(&first.id).copied());
    let keep_history = state
        .messages
        .history_epoch
        .is_some_and(|epoch| epoch == result.epoch)
        && overlap.is_some();

    let next_messages = if let Some(overlap) = overlap.filter(|_| keep_history) {
        let mut rows = state.messages.list[..overlap].to_vec();
        rows.extend(result.messages.iter().map(|message| {
            let carried = state
                .messages
                .index_by_id
                .get(&message.id)
                .and_then(|at| state.messages.list.get(*at))
                .and_then(|old| old.deferred_work.clone());
            match carried {
                Some(deferred) => {
                    let mut next = message.clone();
                    next.deferred_work = Some(deferred);
                    next
                }
                None => message.clone(),
            }
        }));
        rows
    } else {
        result.messages.clone()
    };
    // `setTranscript(mergerRef.current.list)` is always a new array, but the assembler memo
    // behind it hands back the same `messages` when the new list is a suffix extension of the
    // applied one BY IDENTITY, which a replaced list only is when both are empty.
    if !(state.messages.list.is_empty() && next_messages.is_empty()) {
        state.messages.new_composition_identity();
    }
    if !keep_history {
        // `invalidateDeferredSessionChatWork(transport.readHistory)`: a transcript that was
        // replaced rather than extended invalidates every walked work section, because the byte
        // offsets they were walked from belong to the conversation that is gone.
        state.transcript_view.deferred_cache.clear();
        state.messages.history_epoch = None;
        state.messages.history_prefix_count = 0;
        state.messages.boundary_attempt = None;
    }
    // Every result row is a new object, and so is its `deferredWork` unless it was carried over.
    for message in &result.messages {
        let carried = keep_history
            && state
                .messages
                .index_by_id
                .get(&message.id)
                .and_then(|at| state.messages.list.get(*at))
                .is_some_and(|old| old.deferred_work.is_some());
        state.messages.note_arrival(message, carried);
    }
    state.messages.replace_list(&next_messages);
    state.session.lifecycle = result.lifecycle.clone();
    if !keep_history {
        state.messages.has_more = page_has_more(
            PageBoundary {
                message_count: result.messages.len(),
                has_more: result.has_more,
                has_more_exact: result.has_more_exact,
                before_offset: result.before_offset,
            },
            None,
        );
        state.messages.before_offset = result.before_offset;
    }
    state.session.server_status = result.status.clone();
    state.session.server_working =
        result.working == Some(true) || matches!(result.status, ChatStatus::Working);
    if let Some(working) = result.working {
        state.session.session_activity_working = working;
    }
    state.session.prompt = result.state.prompt.clone();
    if restore_presentation {
        apply_agent_identity(
            state,
            &AgentIdentity {
                agent: result.agent.clone(),
                agent_session_id: result.state.agent_session_id.clone(),
                session_agent_id: Tri::Absent,
            },
        );
        apply_selected_options(state, result.state.selected_options.as_ref());
    }
    if let Tri::Value(since) = result.state.async_questions_since {
        state.session.async_questions_since = Some(since);
    } else if matches!(result.state.async_questions_since, Tri::Null) {
        state.session.async_questions_since = None;
    }
    if let Some(ids) = result.state.retired_async_question_ids.clone() {
        state.session.retired_async_question_ids = ids;
    }
    state.session.terminal_notice = result.state.terminal_notice.clone();
    // The status machine decides whether this becomes the working strip's activity, a transient
    // status row, the streaming bubble or the pending tool row.
    crate::session::terminal::apply_terminal_activity(
        state,
        result.state.terminal_activity.as_ref(),
        context,
    );
    state.session.agent_fleet = result.state.agent_fleet.clone();
    state.session.agent_tasks = result.state.agent_tasks.clone();
    if let Some(commands) = result.state.app_commands.clone() {
        // `setAppCommands(result.appCommands)`: a fresh array every time the field is present.
        state.session.app_commands = commands;
        state.messages.new_composition_identity();
    }
    if let Some(prompt) = result.state.returned_prompt.clone() {
        apply_returned_prompt(state, &prompt);
    }
    if result.state.screen_probed == Some(true) {
        state.session.screen_probed = true;
    }
    apply_queue_carriage(
        state,
        &result.state.account_switch,
        &result.state.pending_model_selection,
        result.state.queue.as_ref(),
        result.state.draft.as_ref(),
    );
    state.session.error = if matches!(result.status, ChatStatus::Error) {
        Some(
            result
                .error
                .clone()
                .unwrap_or_else(|| "Conversation could not be loaded.".to_string()),
        )
    } else {
        None
    };

    // CDXC:SessionChat 2026-09-15 WHY:
    // A seed read and its socket snapshot can overlap the request for a long turn's prompt.
    // Cancelling that request on every snapshot left the partial turn hidden behind "Load earlier
    // turns". Preserve requests for the same epoch and history boundary; request identity prevents
    // a cancelled response from completing a newer request in the same epoch.
    let cursor = state.messages.before_offset;
    if let Some(request) = state.messages.load_earlier_request.clone() {
        if request.epoch != Some(result.epoch) || request.before_offset != cursor {
            state.messages.load_earlier_request = None;
            state.messages.boundary_attempt = None;
            state.messages.loading_earlier = false;
        }
    }
}
