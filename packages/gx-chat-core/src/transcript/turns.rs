//! Turn boundaries, completed work, final replies, and summaries.
//!
//! Ported from `packages/shared/session-chat-presentation/turns.ts`.
//!
//! CDXC:SessionChat 2026-09-25 DECISION:
//! User: delete the TypeScript chat rules and the React chat, so the Rust core is the only chat
//! brain. Turn boundaries, completed work, final replies, and summaries have this one
//! implementation, drawn by the GPUI chat and the phone's native chat. Supersedes the 2026-09-17
//! decision that kept React as the desktop default with GPUI opt-in.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

use crate::transcript::foreign::STREAMING_ID;
use crate::transcript::jsstr::js_trim;
use crate::transcript::noise::{is_command_turn, suppressed_turn_label};

/// One finished user turn: the prompt, the work under it, and the reply that settled it.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletedWorkTurn {
    pub user: ChatMessage,
    pub final_message: Option<ChatMessage>,
    pub work: Vec<ChatMessage>,
}

/// One compact row in summary mode.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryModeTurn {
    pub active: bool,
    pub active_work: Vec<ChatMessage>,
    pub final_message: Option<ChatMessage>,
    pub user: ChatMessage,
}

/// One entry of the verbose transcript.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderItem {
    Message(Box<ChatMessage>),
    CompletedWork(Box<CompletedWorkTurn>),
}

fn has_text_content(message: &ChatMessage) -> bool {
    message.blocks.iter().any(|block| match block {
        ChatBlock::Text { text } => !js_trim(text).is_empty(),
        _ => false,
    })
}

fn has_agent_response_content(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant
        && message.id != STREAMING_ID
        && message.blocks.iter().any(|block| match block {
            ChatBlock::ImageRef { .. } => true,
            ChatBlock::Text { text } => !js_trim(text).is_empty(),
            _ => false,
        })
}

/// A prompt the agent has actually taken, as every turn boundary here tests it.
fn is_accepted_user_prompt(message: &ChatMessage) -> bool {
    message.role == ChatRole::User && !message.queued && suppressed_turn_label(message).is_none()
}

/// One compact row per genuine user prompt, paired with its settled final reply.
pub fn summary_mode_turns(
    messages: &[ChatMessage],
    final_assistant_message_ids: &[String],
    is_working: bool,
) -> Vec<SummaryModeTurn> {
    let mut turns: Vec<SummaryModeTurn> = Vec::new();
    for message in messages {
        // A held prompt (agent-CLI queue row, mid-turn send echo) has not started its own response
        // yet: it stays inside the working turn's activeWork instead of opening a turn whose reply
        // would never come.
        if is_accepted_user_prompt(message) || is_command_turn(message) {
            turns.push(SummaryModeTurn {
                active: false,
                active_work: Vec::new(),
                final_message: None,
                user: message.clone(),
            });
        } else if let Some(current) = turns.last_mut() {
            current.active_work.push(message.clone());
            if final_assistant_message_ids.contains(&message.id) {
                current.final_message = Some(message.clone());
            }
        }
    }
    if is_working {
        if let Some(newest) = turns.last_mut() {
            newest.active = true;
        }
    }
    turns
}

pub fn is_visible_assistant_artifact(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant
        && message
            .blocks
            .iter()
            .any(|block| matches!(block, ChatBlock::ImageRef { .. }))
}

/// Splits a completed turn's work into the rows that stay visible and the rows that collapse.
pub fn partition_completed_work(messages: &[ChatMessage]) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
    let mut visible_artifacts = Vec::new();
    let mut collapsed_work = Vec::new();
    for message in messages {
        if is_visible_assistant_artifact(message) || message.role == ChatRole::User {
            visible_artifacts.push(message.clone());
        } else {
            collapsed_work.push(message.clone());
        }
    }
    (visible_artifacts, collapsed_work)
}

/// Where the response the agent is CURRENTLY producing begins: the last user row that is a genuine
/// prompt the agent has accepted for delivery.
///
/// A harness-injected turn (task notification, local command output) and a prompt the agent CLI is
/// still holding in its queue both land as user rows WHILE the agent is mid-response; none of them
/// starts a new response, so none of them may settle the one in flight. Falls back to 0 when no
/// such prompt exists (stitched scroll-back that opens mid-conversation).
fn active_response_start_index(messages: &[ChatMessage]) -> usize {
    messages
        .iter()
        .rposition(is_accepted_user_prompt)
        .unwrap_or(0)
}

/// One copy affordance per response: the last assistant text before the next user turn.
/// The ids are returned in transcript order, not sorted: the document's `finalIds` is the
/// JavaScript `Set`'s insertion order and the renderer keys rows off it.
pub fn final_assistant_message_ids(messages: &[ChatMessage], is_working: bool) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut final_assistant_id: Option<String> = None;
    let active_start = if is_working {
        active_response_start_index(messages)
    } else {
        messages.len()
    };

    for (index, message) in messages.iter().enumerate() {
        // A harness-injected turn (a background-task notification, local command output, a message
        // from another session) is authored by the terminal, not by the reader: the agent is still
        // mid-response on both sides of it. Ending the turn there put a copy affordance under
        // commentary that the agent then kept building on.
        if suppressed_turn_label(message).is_some() && !is_command_turn(message) {
            continue;
        }
        if message.role == ChatRole::User {
            // A user row past the active response's start is a held prompt: the text before it is
            // still commentary, so it must not mint a final reply.
            if index > active_start {
                final_assistant_id = None;
            } else if let Some(id) = final_assistant_id.take() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            continue;
        }
        if message.role == ChatRole::Assistant && has_text_content(message) {
            final_assistant_id = Some(message.id.clone());
        }
    }
    // The newest assistant text is only a final reply once the turn has finished. While the agent
    // is still working it is commentary, even when it happens to be the most recent text block for
    // a moment.
    if !is_working {
        if let Some(id) = final_assistant_id {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    ids
}

/// A completed interaction keeps the user's message and the agent's final response in the normal
/// transcript flow. Everything the agent emitted in between becomes one collapsed work section.
///
/// While the agent is still working, everything from the active response's start onward stays
/// expanded. "Newest turn" is NOT enough for that guard: a harness-injected user row or a held
/// prompt lands mid-response and would close the streaming turn the moment it appears, folding live
/// work into a "Worked for" row and yanking the bottom-pinned viewport onto it, only for the fold to
/// vanish again when the injected row settles.
pub fn completed_work_render_items(
    messages: &[ChatMessage],
    is_working: bool,
    interacted_message_ids: &[String],
    raw_messages: &[ChatMessage],
) -> Vec<RenderItem> {
    let mut items = Vec::new();
    let raw_index = |id: &str| raw_messages.iter().position(|message| message.id == id);
    let active_start = if is_working {
        active_response_start_index(messages)
    } else {
        messages.len()
    };
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        if message.role != ChatRole::User {
            items.push(RenderItem::Message(Box::new(message.clone())));
            index += 1;
            continue;
        }

        let mut next_user_index = index + 1;
        while next_user_index < messages.len()
            && (messages[next_user_index].role != ChatRole::User
                || (message.deferred_work.is_some()
                    && messages[next_user_index].byte_offset.is_none()))
        {
            next_user_index += 1;
        }
        let turn_messages = &messages[index + 1..next_user_index];
        let final_index = turn_messages.iter().rposition(has_agent_response_content);
        let interacted_inline_diff = turn_messages
            .iter()
            .any(|row| interacted_message_ids.contains(&row.id));
        let interacted_grouped_diff = interacted_message_ids.contains(&message.id);
        if (message.deferred_work.is_none() && final_index.is_none())
            || (index >= active_start && !interacted_grouped_diff)
            || interacted_inline_diff
        {
            items.push(RenderItem::Message(Box::new(message.clone())));
            for turn_message in turn_messages {
                items.push(RenderItem::Message(Box::new(turn_message.clone())));
            }
            index = next_user_index;
            continue;
        }

        let final_message = final_index.map(|at| turn_messages[at].clone());
        let raw_start = raw_index(&message.id).expect("a rendered row comes from the raw list");
        let raw_end = messages
            .get(next_user_index)
            .and_then(|next| raw_index(&next.id))
            .unwrap_or(raw_messages.len());
        let final_id = final_message.as_ref().map(|row| row.id.clone());
        items.push(RenderItem::Message(Box::new(message.clone())));
        items.push(RenderItem::CompletedWork(Box::new(CompletedWorkTurn {
            user: message.clone(),
            work: raw_messages[(raw_start + 1).min(raw_end)..raw_end]
                .iter()
                .filter(|row| Some(&row.id) != final_id.as_ref())
                .cloned()
                .collect(),
            final_message,
        })));
        index = next_user_index;
    }
    items
}

/// `stickySessionChatTranscriptWorking` in `packages/shared/session-chat-presentation/turns.ts`:
/// a transcript that settled keeps treating the session as settled until its newest row changes,
/// so a working blip alone never reopens a landed fold.
///
/// `settled_at` is the fold memory, the newest message id at the moment the transcript settled.
/// Returns the working flag the projection uses and records the settle.
pub fn sticky_transcript_working(
    messages: &[ChatMessage],
    working: bool,
    settled_at: &mut Option<String>,
) -> bool {
    let effective = working && !sticky_fold_holds(messages, settled_at.as_deref());
    *settled_at = if effective {
        None
    } else {
        messages.last().map(|message| message.id.clone())
    };
    effective
}

/// Whether a recorded settle still stands: the newest row is the one the transcript settled on.
pub fn sticky_fold_holds(messages: &[ChatMessage], settled_at: Option<&str>) -> bool {
    settled_at.is_some() && messages.last().map(|message| message.id.as_str()) == settled_at
}

/// "Worked for 12s", the label on a finished turn's fold.
pub fn worked_duration_label(started_at: Option<i64>, completed_at: Option<i64>) -> String {
    let (Some(started_at), Some(completed_at)) = (started_at, completed_at) else {
        return "Worked".to_string();
    };
    if completed_at < started_at {
        return "Worked".to_string();
    }
    let elapsed = (completed_at - started_at) as f64 / 1000.0;
    let seconds = (elapsed.round() as i64).max(1);
    if seconds < 60 {
        return format!("Worked for {seconds}s");
    }
    let minutes = seconds / 60;
    let remainder = seconds % 60;
    if remainder > 0 {
        format!("Worked for {minutes}m {remainder}s")
    } else {
        format!("Worked for {minutes}m")
    }
}
