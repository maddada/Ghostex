//! The transcript pipeline: normalize, fold, and project into render items.
//!
//! Ported from `packages/shared/session-chat-presentation/transcript.ts`.

use ghostex_gx_protocol::ChatMessage;

use crate::transcript::foreign::{merge_messages_with, order_messages};
use crate::transcript::image_markers::normalize_image_transcript_messages;
use crate::transcript::local_command::{fold_shell_commands, normalize_local_command_messages};
use crate::transcript::noise::{
    drop_hidden_messages, fold_model_effort_pairs, is_command_turn, is_turn_seam,
    suppressed_turn_label,
};
use crate::transcript::side_question::fold_side_questions;
use crate::transcript::tool_fold::fold_tool_messages;
use crate::transcript::turns::{
    completed_work_render_items, final_assistant_message_ids, summary_mode_turns,
    wakes_settled_agent, CompletedWorkTurn, RenderItem, SummaryModeTurn,
};

pub fn normalize_chat_transcript(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    // A failed command inside a Codex script marks the script's result before its hidden exit row
    // goes with the other harness rows (`command_exits.rs`).
    let ordered = crate::transcript::command_exits::link_command_exits(order_messages(messages));
    let with_commands = fold_side_questions(&fold_shell_commands(
        &normalize_local_command_messages(&ordered),
    ));
    let with_images = normalize_image_transcript_messages(&with_commands);
    fold_model_effort_pairs(&drop_hidden_messages(&with_images))
}

/// A row that woke the agent after its reply opens a new turn, so it is not transparent: the tool
/// calls the agent then makes belong to that turn, not to the reply before it.
pub fn fold_chat_transcript(messages: &[ChatMessage]) -> Vec<ChatMessage> {
    let wakes: Vec<&str> = (0..messages.len())
        .filter(|at| wakes_settled_agent(messages, *at))
        .map(|at| messages[at].id.as_str())
        .collect();
    fold_tool_messages(messages, &|message| {
        suppressed_turn_label(message).is_some() && !wakes.contains(&message.id.as_str())
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
            let final_at = deferred
                .iter()
                .position(|message| Some(message.id.as_str()) == final_id);
            /* CDXC:SessionChat 2026-10-07 WHY:
            gxserver folds a turn up to its last row, so a seam after the reply (a compaction, or a model or effort change made between turns) comes back with the turn's work although the turn already draws it after itself; kept, it showed twice. A slash command gxserver archived after the reply comes back the same way and opens its own turn (`completed_work_render_items`). */
            let filtered: Vec<ChatMessage> = deferred
                .iter()
                .enumerate()
                .filter(|(at, message)| {
                    Some(message.id.as_str()) != final_id
                        && !(final_at.is_some_and(|final_at| *at > final_at)
                            && (is_turn_seam(message) || is_command_turn(message)))
                })
                .map(|(_, message)| message.clone())
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
    /// Rows that keep their place in the turns but draw nothing (`hide_redone_forms`).
    pub hidden_rows: Vec<String>,
}

/// The one pass every renderer starts from.
///
/// `interacted_message_ids` is the inline-diff opt-out React passed; the native projection always
/// hands it an empty list, exactly as the TypeScript default did.
pub fn project_chat_transcript(
    messages: &[ChatMessage],
    working: bool,
    interacted_message_ids: &[String],
    live_side_question: Option<&str>,
) -> TranscriptProjection {
    let mut normalized = normalize_chat_transcript(messages);
    let hidden_rows = crate::transcript::question_forms::hide_redone_forms(&mut normalized);
    if let Some(question) = live_side_question {
        crate::transcript::side_question::hide_live_side_question(&mut normalized, question);
    }
    project_normalized(normalized, hidden_rows, working, interacted_message_ids)
}

/// [`project_chat_transcript`] for one stretch of the list after a settled prefix
/// (`transcript/settled.rs`), which the prefix's redone forms are stripped from as well. Hands back
/// the forms this stretch itself redoes beside its projection: those strip the prefix too, so the
/// caller can only use the stretch alone when there are none.
pub(crate) fn project_chat_segment(
    messages: &[ChatMessage],
    working: bool,
    prior_redone: &[(String, String)],
) -> (TranscriptProjection, Vec<(String, String)>) {
    let (projection, own, _) = project_chat_segment_with(messages, working, prior_redone, None);
    (projection, own)
}

/// [`project_chat_segment`] with the side-question card open above the composer: the newest row of
/// the stretch asking its question is left out, and the flag says whether there was one.
pub(crate) fn project_chat_segment_with(
    messages: &[ChatMessage],
    working: bool,
    prior_redone: &[(String, String)],
    live_side_question: Option<&str>,
) -> (TranscriptProjection, Vec<(String, String)>, bool) {
    use crate::transcript::question_forms::{redone_forms, strip_redone_forms};
    let mut normalized = normalize_chat_transcript(messages);
    let own = redone_forms(&normalized);
    let mut redone = prior_redone.to_vec();
    redone.extend(own.iter().cloned());
    strip_redone_forms(&mut normalized, &redone);
    let hidden_rows = own.iter().map(|(_, resend)| resend.clone()).collect();
    let hid = live_side_question.is_some_and(|question| {
        crate::transcript::side_question::hide_newest_side_question(&mut normalized, question)
    });
    (
        project_normalized(normalized, hidden_rows, working, &[]),
        own,
        hid,
    )
}

fn project_normalized(
    normalized: Vec<ChatMessage>,
    hidden_rows: Vec<String>,
    working: bool,
    interacted_message_ids: &[String],
) -> TranscriptProjection {
    let rendered = fold_chat_transcript(&normalized);
    let final_ids = final_assistant_message_ids(&rendered, working);
    TranscriptProjection {
        items: completed_work_render_items(&rendered, working, interacted_message_ids, &normalized),
        summary_turns: summary_mode_turns(&rendered, &final_ids, working),
        final_ids,
        normalized,
        rendered,
        hidden_rows,
    }
}
