//! Slash-command markers, the `/clear` boundary, and the interrupt row.
//!
//! Ported from the marker half of `packages/core-ui/chat/session-chat-pending.ts` and from
//! `session-chat-returned-prompt.ts`. A marker is the row that says "a command went to the
//! terminal"; it retires against the agent's own record of the same command.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::session::constants::{
    COMMAND_MARKER_ID_PREFIX, COMMAND_MARKER_LIMIT, INTERRUPT_MARKER_COMMAND,
    INTERRUPT_MARKER_MATCH_SLACK_MS, TRANSCRIPT_INTERRUPTED_TEXT,
};
use crate::session::text::{command_marker_name, is_clear_command, is_js_space};
use crate::state::CommandMarker;

/// `appendSessionChatCommandMarker`, bounded to the newest few.
pub fn append_marker(markers: &[CommandMarker], marker: CommandMarker) -> Vec<CommandMarker> {
    let mut next = markers.to_vec();
    next.push(marker);
    if next.len() > COMMAND_MARKER_LIMIT {
        next.split_off(next.len() - COMMAND_MARKER_LIMIT)
    } else {
        next
    }
}

/// `sessionChatCommandMarkersAsMessages`.
///
/// `compaction_records` is what the authoritative transcript records NOW, so a `/compact` marker
/// can retire the moment its own compaction lands.
pub fn markers_as_messages(
    markers: &[CommandMarker],
    compaction_records: usize,
) -> Vec<ChatMessage> {
    markers
        .iter()
        .filter_map(|marker| {
            let name = command_marker_name(&marker.command);
            if name == "/model" || name == "/effort" || (name == "/fast" && marker.label.is_none())
            {
                // Not typed: these are what the model, effort and Fast mode controls dispatch.
                // Their configuration gets one authoritative status row; a redundant "Ran ..." row
                // would narrate the implementation of a click.
                return None;
            }
            // `/compact` IS typed, and until it finishes there is nothing else to see: Claude
            // hides its command record and Codex writes none at all, so dropping this row left a
            // minutes-long compaction looking like the chat had ignored the send. It retires
            // against the agent's own completion row rather than living forever, because markers
            // render after the transcript and an un-retired one would end up below the row it
            // precedes.
            if name == "/compact"
                && compaction_records > marker.compaction_records_before.unwrap_or(0)
            {
                return None;
            }
            let user_typed = marker.label.is_none();
            Some(ChatMessage {
                id: format!("{COMMAND_MARKER_ID_PREFIX}{}", marker.id),
                // Composer commands are user turns, just like the line the terminal displays.
                // Host-dispatched key markers keep their explicit system label because the user
                // did not type those implementation keys.
                role: if user_typed {
                    ChatRole::User
                } else {
                    ChatRole::System
                },
                blocks: vec![ChatBlock::Text {
                    text: marker
                        .label
                        .clone()
                        .filter(|_| !user_typed)
                        .unwrap_or_else(|| marker.command.clone()),
                }],
                async_questions: None,
                timestamp: Some(marker.sent_at_ms),
                source: ChatSource::Client,
                turn_id: None,
                byte_offset: None,
                queued: false,
                deferred_work: None,
                startup_delivery: None,
                api_error: None,
                mention_paths: None,
            })
        })
        .collect()
}

/// `applySessionChatCommandMarkerBoundaries`.
///
/// `/clear` mutates the transcript ASYNCHRONOUSLY: hide the current transcript immediately so the
/// chat reflects the command before the agent writes a replacement session.
pub fn apply_marker_boundaries(
    messages: &[ChatMessage],
    markers: &[CommandMarker],
) -> Vec<ChatMessage> {
    apply_marker_boundaries_in(messages.to_vec(), markers)
}

/// [`apply_marker_boundaries`] on a list the caller owns, filtered in place.
pub fn apply_marker_boundaries_in(
    mut messages: Vec<ChatMessage>,
    markers: &[CommandMarker],
) -> Vec<ChatMessage> {
    let mut clear_sent_at: Option<i64> = None;
    for marker in markers {
        if is_clear_command(&marker.command) {
            clear_sent_at = Some(match clear_sent_at {
                None => marker.sent_at_ms,
                Some(current) => current.max(marker.sent_at_ms),
            });
        }
    }
    let Some(boundary) = clear_sent_at else {
        return messages;
    };
    messages.retain(|message| message.timestamp.is_some_and(|stamp| stamp > boundary));
    messages
}

fn message_text_lower(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .map(|block| match block {
            ChatBlock::Text { text } => text.as_str(),
            _ => "",
        })
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(is_js_space)
        .to_lowercase()
}

/// Drops an interrupt marker once the transcript carries the agent's own interrupt row from the
/// same moment, so a mid-response Stop shows one row, not two.
pub fn retire_interrupt_markers(
    markers: &[CommandMarker],
    transcript: &[ChatMessage],
) -> Vec<CommandMarker> {
    if !markers
        .iter()
        .any(|marker| marker.command == INTERRUPT_MARKER_COMMAND)
    {
        return markers.to_vec();
    }
    let interrupted_at: Vec<i64> = transcript
        .iter()
        .filter(|message| {
            matches!(message.role, ChatRole::System)
                && matches!(message.source, ChatSource::Transcript)
                && message_text_lower(message) == TRANSCRIPT_INTERRUPTED_TEXT
                && message.timestamp.is_some()
        })
        .filter_map(|message| message.timestamp)
        .collect();
    if interrupted_at.is_empty() {
        return markers.to_vec();
    }
    let next: Vec<CommandMarker> = markers
        .iter()
        .filter(|marker| {
            marker.command != INTERRUPT_MARKER_COMMAND
                || !interrupted_at
                    .iter()
                    .any(|at| *at >= marker.sent_at_ms - INTERRUPT_MARKER_MATCH_SLACK_MS)
        })
        .cloned()
        .collect();
    if next.len() == markers.len() {
        markers.to_vec()
    } else {
        next
    }
}
