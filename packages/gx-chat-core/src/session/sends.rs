//! Sending, interrupting, and the keystroke dispatch: the half of those actions that belongs to
//! family a.
//!
//! Ported from `send`, `sendKey` and `interrupt` in
//! `packages/shared/session-chat-controller/controller.ts`. The composer (family d) owns the draft,
//! the classification of what the user typed, and the gxserver call; what it must NOT do is push
//! into `state.pending` itself, because the echo's boundary, its occurrence and the marker's
//! retirement snapshot are the pending matcher's rules. These are the functions it calls instead.
//!
//! Every one of them is pure state: no effect is emitted here, so the caller keeps its own
//! `Effect::SendRpc` and decides what to do when the call fails.

use crate::session::app_commands::local_command_identities;
use crate::session::composition::compaction_records;
use crate::session::constants::{
    INTERRUPT_MARKER_COMMAND, INTERRUPT_MARKER_LABEL, PENDING_SEND_LIMIT,
};
use crate::session::markers::append_marker;
use crate::session::pending::assign_occurrence;
use crate::session::working::is_working;
use crate::state::{ChatContext, ChatState, CommandMarker, PendingSend};

/// `nextSessionChatPendingSendId(now)`: `${now}-${counter}`.
///
/// One counter for echoes and markers alike, because the TypeScript's was a module variable both
/// called. Here it is state, so the ids are deterministic and the core needs no random source.
pub fn next_pending_send_id(state: &mut ChatState, now_ms: i64) -> String {
    state.pending.send_counter += 1;
    format!("{now_ms}-{}", state.pending.send_counter)
}

/// Records the optimistic echo of a chat send and answers its id.
///
/// The boundary fields are frozen here rather than at match time: `sentWhileWorking` decides
/// whether the echo reads as a new turn or as a row the agent's own queue is holding, and the
/// `after…` pair is what the matcher measures a later transcript row against.
pub fn begin_send(
    state: &mut ChatState,
    context: &ChatContext,
    text: &str,
    image_paths: &[String],
) -> String {
    let working = is_working(state);
    let now = context.now_millis();
    let after_message_id = state.messages.list.last().map(|message| message.id.clone());
    let after_message_timestamp = state
        .messages
        .list
        .last()
        .and_then(|message| message.timestamp);
    let id = next_pending_send_id(state, now);
    let entry = PendingSend {
        id: id.clone(),
        queued_prompt_id: None,
        startup_delivery: None,
        text: text.to_string(),
        image_paths: image_paths.to_vec(),
        sent_at_ms: now,
        after_message_id,
        after_message_timestamp,
        matching_occurrence: None,
        matching_after_timestamp: None,
        sent_while_working: working,
    };
    let entry = assign_occurrence(&state.pending.sends, entry);
    state.pending.sends.push(entry);
    if state.pending.sends.len() > PENDING_SEND_LIMIT {
        state.pending.sends = state
            .pending
            .sends
            .split_off(state.pending.sends.len() - PENDING_SEND_LIMIT);
    }
    id
}

/// The receipt named a queue row: the echo becomes that row's optimistic twin.
pub fn adopt_queued_prompt_id(state: &mut ChatState, pending_id: &str, queued_prompt_id: &str) {
    for entry in &mut state.pending.sends {
        if entry.id == pending_id {
            entry.queued_prompt_id = Some(queued_prompt_id.to_string());
        }
    }
    // `setPending((current) => current.map(...))`: a new array whether or not a row matched.
    state.messages.new_composition_identity();
}

/// The send failed: the echo goes, because nothing will ever replace it.
pub fn drop_send(state: &mut ChatState, pending_id: &str) {
    state.pending.sends.retain(|entry| entry.id != pending_id);
    // `setPending((current) => current.filter(...))`: a new array whether or not a row went.
    state.messages.new_composition_identity();
}

/// Drops every echo that became a queue row, for a queue row the user deleted.
pub fn drop_queued_send(state: &mut ChatState, queued_prompt_id: &str) {
    state
        .pending
        .sends
        .retain(|entry| entry.queued_prompt_id.as_deref() != Some(queued_prompt_id));
    state.messages.new_composition_identity();
}

/// Records the "Ran /x" marker for a catalog slash command and answers its stamp.
///
/// The two snapshots are the marker's whole retirement rule: the compactions already on record, so
/// a `/compact` marker retires against ITS OWN compaction rather than an earlier one, and the
/// server identities already visible, which is skew-proof where a timestamp is not.
pub fn begin_command_marker(state: &mut ChatState, context: &ChatContext, command: &str) -> i64 {
    let sent_at_ms = context.now_millis();
    let compaction_records_before = compaction_records(&state.messages.list);
    let identities: Vec<String> =
        local_command_identities(&state.session.app_commands, &state.messages.list)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
    let id = next_pending_send_id(state, sent_at_ms);
    let marker = CommandMarker {
        id,
        command: command
            .trim_matches(crate::session::text::is_js_space)
            .to_string(),
        sent_at_ms,
        label: None,
        compaction_records_before: Some(compaction_records_before),
        local_command_ids_before: identities,
    };
    state.pending.markers = append_marker(&state.pending.markers, marker);
    sent_at_ms
}

/// The command never reached the agent: its marker goes with it.
pub fn drop_command_marker(state: &mut ChatState, command: &str, sent_at_ms: i64) {
    let command = command
        .trim_matches(crate::session::text::is_js_space)
        .to_string();
    state
        .pending
        .markers
        .retain(|marker| marker.sent_at_ms != sent_at_ms || marker.command != command);
    // `setMarkers((current) => current.filter(...))`: a new array either way.
    state.messages.new_composition_identity();
}

/// Keystroke dispatch: a non-empty marker is recorded only after the write is accepted.
///
/// Multi-key setting adjustments pass an empty marker so their implementation keystrokes do not
/// become chat rows.
pub fn record_key_marker(state: &mut ChatState, context: &ChatContext, key: &str, label: &str) {
    if label
        .trim_matches(crate::session::text::is_js_space)
        .is_empty()
    {
        return;
    }
    let sent_at_ms = context.now_millis();
    let id = next_pending_send_id(state, sent_at_ms);
    let marker = CommandMarker {
        id,
        command: key.to_string(),
        sent_at_ms,
        label: Some(label.to_string()),
        compaction_records_before: None,
        local_command_ids_before: Vec::new(),
    };
    state.pending.markers = append_marker(&state.pending.markers, marker);
}

/// The local half of Escape, for the stop lane alone.
///
/// Answers whether a turn was actually interrupted, which is what the caller uses to decide between
/// the stop lane and a dialog's own cancel lane.
///
/// CDXC:SessionChat 2026-09-04 DECISION:
/// User: pressing Escape in the chat box must show an "Interrupted the agent" status row, because
/// the Escape goes to the terminal and nothing else tells the user it happened. The agent writes
/// its own interrupt row only for a turn cut off mid-response; a prompt it hands back leaves none.
pub fn begin_interrupt(state: &mut ChatState, context: &ChatContext) -> bool {
    if !is_working(state) {
        return false;
    }
    // Stop: suppress the spinner and drop optimistic echoes. The delayed server-side Enter may
    // never fire, so the echo would be a ghost bubble.
    state.session.interrupted = true;
    // `setPending([])`: a new array even when there was nothing to drop.
    state.pending.sends.clear();
    state.messages.new_composition_identity();
    let sent_at_ms = context.now_millis();
    let id = next_pending_send_id(state, sent_at_ms);
    let marker = CommandMarker {
        id,
        command: INTERRUPT_MARKER_COMMAND.to_string(),
        sent_at_ms,
        label: Some(INTERRUPT_MARKER_LABEL.to_string()),
        compaction_records_before: None,
        local_command_ids_before: Vec::new(),
    };
    state.pending.markers = append_marker(&state.pending.markers, marker);
    true
}
