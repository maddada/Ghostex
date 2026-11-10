//! The transcript pipeline: normalize, fold, and project into render items.
//!
//! Ported from `packages/shared/session-chat-presentation/transcript.ts`.

use ghostex_gx_protocol::ChatMessage;

use crate::transcript::foreign::{merge_messages_with, order_messages};
use crate::transcript::image_markers::normalize_image_transcript_messages;
use crate::transcript::local_command::{fold_shell_commands, normalize_local_command_messages};
use crate::transcript::noise::{
    classify_suppressed_turn, drop_hidden_messages, fold_model_effort_pairs, is_command_turn,
    is_turn_seam, SuppressedTurn,
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
        passes_over_tool_burst(message, wakes.contains(&message.id.as_str()))
    })
}

/// Whether the chat's tool fold ([`fold_chat_transcript`]) passes over `message`, which `wakes`
/// says opened a turn: a harness marker or status that did not wake the agent. A row the chat
/// never shows is dropped before the fold (`normalize_chat_transcript`), so it is passed over as
/// well; gxserver, which reads rows as they were written, needs that. Never true for a row whose
/// role `may_suppress_turn` (`noise.rs`) rules out.
pub fn passes_over_tool_burst(message: &ChatMessage, wakes: bool) -> bool {
    match classify_suppressed_turn(message) {
        Some(SuppressedTurn::Hidden) => true,
        Some(SuppressedTurn::Collapsed { .. } | SuppressedTurn::Status { .. }) => !wakes,
        None => false,
    }
}

/// A completed turn's work, with the rows a deferred read brought back merged in. `deferred` is the
/// whole section the turn's `section` names; the turn takes its own part of it.
pub fn completed_chat_work(
    turn: &CompletedWorkTurn,
    deferred: Option<&[ChatMessage]>,
) -> Vec<ChatMessage> {
    let rows = match deferred.map(|section| section_part(turn, section)) {
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

/// The rows of a loaded section that belong to `turn`: after its opener (the prompt that owns the
/// section is not in it) or the row of its work that holds it, up to the OMP wake-up that opens
/// the next turn.
///
/// CDXC:SessionChat 2026-10-30 WHY: gxserver folds a prompt and the OMP wake-ups the agent answered after it into one `deferredWork` stretch, the shape every chat core reads, so an older chat that does not open turns at OMP rows still reaches all of the work through the prompt. This core opens a turn at each such wake-up (`wakes_settled_agent`) and cuts the loaded stretch at them, so each fold holds its own work, whichever of their folds loaded it.
pub(crate) fn section_part<'a>(
    turn: &CompletedWorkTurn,
    section: &'a [ChatMessage],
) -> &'a [ChatMessage] {
    // A section that starts at a row of the turn's own work (a background task's end,
    // `turns::turn_section`) is all this turn's.
    let own_row = turn
        .section
        .as_ref()
        .is_some_and(|section| turn.work.iter().any(|row| row.id == section.start_id));
    let start = if turn.user.deferred_work.is_some() || own_row {
        0
    } else {
        match section.iter().position(|row| row.id == turn.user.id) {
            Some(at) => at + 1,
            None => return &[],
        }
    };
    let end = turn
        .next_opener
        .as_deref()
        .and_then(|next| section[start..].iter().position(|row| row.id == next))
        .map_or(section.len(), |at| start + at);
    &section[start..end]
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
    /// The ids of the rows of the response still running, from its start
    /// (`active_response_start_index`, the boundary `completed_work_render_items` keeps open);
    /// empty when the agent is not working. Agent work Quiet leaves them out
    /// (`grouped_work::hide_work`).
    pub live_ids: Vec<String>,
}

/// The one pass every renderer starts from.
///
/// `interacted_message_ids` is the inline-diff opt-out React passed; the native projection always
/// hands it an empty list, exactly as the TypeScript default did. `grouped` is Agent work Reduced
/// or Quiet outside the Summary view (`completed_work_render_items`).
pub fn project_chat_transcript(
    messages: &[ChatMessage],
    working: bool,
    interacted_message_ids: &[String],
    live_side_question: Option<&str>,
    grouped: bool,
) -> TranscriptProjection {
    let mut normalized = normalize_chat_transcript(messages);
    let hidden_rows = crate::transcript::question_forms::hide_redone_forms(&mut normalized);
    if let Some(question) = live_side_question {
        crate::transcript::side_question::hide_live_side_question(&mut normalized, question);
    }
    project_normalized(
        normalized,
        hidden_rows,
        working,
        interacted_message_ids,
        grouped,
    )
}

/// [`project_chat_transcript`] for one stretch of the list after a settled prefix
/// (`transcript/settled.rs`), which the prefix's redone forms are stripped from as well. Hands back
/// the forms this stretch itself redoes beside its projection: those strip the prefix too, so the
/// caller can only use the stretch alone when there are none.
pub(crate) fn project_chat_segment(
    messages: &[ChatMessage],
    working: bool,
    prior_redone: &[(String, String)],
    grouped: bool,
) -> (TranscriptProjection, Vec<(String, String)>) {
    let (projection, own, _) =
        project_chat_segment_with(messages, working, prior_redone, None, grouped);
    (projection, own)
}

/// [`project_chat_segment`] with the side-question card open above the composer: the newest row of
/// the stretch asking its question is left out, and the flag says whether there was one.
pub(crate) fn project_chat_segment_with(
    messages: &[ChatMessage],
    working: bool,
    prior_redone: &[(String, String)],
    live_side_question: Option<&str>,
    grouped: bool,
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
        project_normalized(normalized, hidden_rows, working, &[], grouped),
        own,
        hid,
    )
}

fn project_normalized(
    normalized: Vec<ChatMessage>,
    hidden_rows: Vec<String>,
    working: bool,
    interacted_message_ids: &[String],
    grouped: bool,
) -> TranscriptProjection {
    let rendered = fold_chat_transcript(&normalized);
    let mid_response = crate::transcript::turns::mid_response_rows(&normalized);
    let mut final_ids = final_assistant_message_ids(&rendered, working, &mid_response);
    let (items, demoted) = completed_work_render_items(
        &rendered,
        working,
        interacted_message_ids,
        &normalized,
        grouped,
    );
    final_ids.retain(|id| !demoted.contains(id));
    let live_ids = if working {
        rendered[crate::transcript::turns::active_response_start_index(&rendered, &mid_response)..]
            .iter()
            .map(|message| message.id.clone())
            .collect()
    } else {
        Vec::new()
    };
    TranscriptProjection {
        live_ids,
        items,
        summary_turns: summary_mode_turns(&rendered, &final_ids, working),
        final_ids,
        normalized,
        rendered,
        hidden_rows,
    }
}
