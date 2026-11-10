//! A reply's Markdown by message id, and its speakable segments.

use ghostex_gx_protocol::ChatRole;

use crate::read_aloud::speakable::{speakable_segments, SpeechSegment};
use crate::state::ChatState;
use crate::transcript::jsstr::js_trim;
use crate::transcript::prose::prose_markdown;

/// The prose Markdown of agent message `message_id`, the same source as its `copyText`, or `None`
/// when the chat has no agent message by that id.
///
/// The projection's own copy is read first, because that is the row as the transcript drew it
/// (after its folds); the composed list answers for a row the projection has not reached yet.
pub fn reply_markdown(state: &ChatState, message_id: &str) -> Option<String> {
    let message = state
        .transcript_view
        .projected
        .get(message_id)
        .map(|projected| &*projected.source)
        .or_else(|| {
            state
                .messages
                .composed
                .iter()
                .find(|message| message.id == message_id)
                .map(|message| &**message)
        })?;
    // Only the agent's own messages are read: never yours, a tool call or the agent's thinking.
    (message.role == ChatRole::Assistant)
        .then(|| js_trim(&prose_markdown(&message.blocks)).to_string())
}

/// What Read aloud says for agent message `message_id`, or `None` when there is no such message.
pub fn message_segments(state: &ChatState, message_id: &str) -> Option<Vec<SpeechSegment>> {
    reply_markdown(state, message_id).map(|markdown| speakable_segments(&markdown))
}
