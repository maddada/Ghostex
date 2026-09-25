//! The transcript pipeline: normalize, fold, and project into render items.
//!
//! Ported from `packages/shared/session-chat-presentation/transcript.ts`.

use ghostex_gx_protocol::ChatMessage;

use crate::transcript::foreign::{merge_messages_with, order_messages};
use crate::transcript::image_markers::normalize_image_transcript_messages;
use crate::transcript::local_command::normalize_local_command_messages;
use crate::transcript::noise::{drop_hidden_messages, suppressed_turn_label};
use crate::transcript::tool_fold::fold_tool_messages;
use crate::transcript::turns::{
    completed_work_render_items, final_assistant_message_ids, summary_mode_turns,
    CompletedWorkTurn, RenderItem, SummaryModeTurn,
};

pub fn normalize_chat_transcript(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let ordered = order_messages(messages);
    let with_commands = normalize_local_command_messages(&ordered);
    let with_images = normalize_image_transcript_messages(&with_commands);
    drop_hidden_messages(&with_images)
}

pub fn fold_chat_transcript(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    fold_tool_messages(messages, &|message| {
        suppressed_turn_label(message).is_some()
    })
}

/// A completed turn's work, with the rows a deferred read brought back merged in.
pub fn completed_chat_work(
    turn: &CompletedWorkTurn,
    deferred: Option<&[ChatMessage]>,
) -> Vec<ChatMessage> {
    let rows = match deferred {
        Some(deferred) => {
            let final_id = turn
                .final_message
                .as_ref()
                .map(|message| message.id.as_str());
            let filtered: Vec<ChatMessage> = deferred
                .iter()
                .filter(|message| Some(message.id.as_str()) != final_id)
                .cloned()
                .collect();
            merge_messages_with(&filtered, &turn.work)
        }
        None => turn.work.clone(),
    };
    fold_chat_transcript(&normalize_chat_transcript(&rows))
}

/// Everything the projection reads off one message list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TranscriptProjection {
    pub normalized: Vec<ChatMessage>,
    pub rendered: Vec<ChatMessage>,
    pub items: Vec<RenderItem>,
    pub final_ids: Vec<String>,
    pub summary_turns: Vec<SummaryModeTurn>,
}

/// The one pass every renderer starts from.
///
/// `interacted_message_ids` is the inline-diff opt-out React passed; the native projection always
/// hands it an empty list, exactly as the TypeScript default did.
pub fn project_chat_transcript(
    messages: &[ChatMessage],
    working: bool,
    interacted_message_ids: &[String],
) -> TranscriptProjection {
    let normalized = normalize_chat_transcript(messages);
    let rendered = fold_chat_transcript(&normalized);
    let final_ids = final_assistant_message_ids(&rendered, working);
    TranscriptProjection {
        items: completed_work_render_items(&rendered, working, interacted_message_ids, &normalized),
        summary_turns: summary_mode_turns(&rendered, &final_ids, working),
        final_ids,
        normalized,
        rendered,
    }
}
