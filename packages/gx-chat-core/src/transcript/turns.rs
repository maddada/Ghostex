//! Turn boundaries, completed work, final replies, and summaries.
//!
//! Ported from `packages/shared/session-chat-presentation/turns.ts`.
//!
//! CDXC:SessionChat 2026-09-25 DECISION:
//! User: delete the TypeScript chat rules and the React chat, so the Rust core is the only chat
//! brain. Turn boundaries, completed work, final replies, and summaries have this one
//! implementation, drawn by the GPUI chat and the phone's native chat. Supersedes the 2026-09-17
//! decision that kept React as the desktop default with GPUI opt-in.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, DeferredWork};

use crate::transcript::background_rows::{is_background_task_row, OMP_ASYNC_RESULT};
use crate::transcript::foreign::STREAMING_ID;
use crate::transcript::jsstr::js_trim;
use crate::transcript::noise::{
    is_command_output_turn, is_command_turn, is_hidden_message, is_setting_status, is_turn_seam,
    suppressed_turn_label, INTERRUPTED_LABEL,
};
use crate::transcript::side_question::SIDE_QUESTION_LABEL;
use crate::transcript::system_cards::{OMP_ADVISOR, OMP_IRC_INCOMING};

/// One finished user turn: the prompt, the work under it, and the reply that settled it.
#[derive(Clone, Debug, PartialEq)]
pub struct CompletedWorkTurn {
    pub user: ChatMessage,
    pub final_message: Option<ChatMessage>,
    pub work: Vec<ChatMessage>,
    /// The `deferredWork` that holds this turn's rows still on the server: the prompt's own, or,
    /// for a turn an OMP delivery opened, the prompt's before it.
    pub section: Option<DeferredWork>,
    /// The row that opens the next turn, where this turn's part of the section ends.
    pub next_opener: Option<String>,
}

/// One compact row in summary mode.
#[derive(Clone, Debug, PartialEq)]
pub struct SummaryModeTurn {
    pub active: bool,
    pub active_work: Vec<ChatMessage>,
    /// The replies the agent gave before `final_message` in the same turn, oldest first.
    pub earlier_replies: Vec<ChatMessage>,
    pub final_message: Option<ChatMessage>,
    pub user: ChatMessage,
    /// The rows that end a turn the agent wrote no reply to (a command's output, an interruption).
    pub outcome: Vec<ChatMessage>,
    /// While the turn runs: the id of the agent's newest row, whose tool calls start open so the
    /// running call shows. `None` once the turn has settled.
    pub running_row: Option<String>,
    /// The agent's messages in the turn that hold generated content (a page card, a question form
    /// or a chart), drawn in full under the prompt and never inside a fold (`content_rows`).
    pub cards: Vec<ChatMessage>,
}

/// The agent's rows of `work` that hold generated content: a published page's card, a question
/// form that asks something, or a chart.
///
/// CDXC:SessionChat 2026-10-27 DECISION:
/// User: "in summary mode view please make the inline htmls appear shown not hidden under folds so i can scroll to older inline htmls", then "when i pick "Summary" please always show these: "Html" "Q&A" "Charts", messages that contain them shouoldn't be hidden under toggles". In summary mode every message of a turn that holds a page card (```visual page from `ghostex show`), a question form or a chart is drawn in full under the prompt, and the turn's "Agent reply" and "Active work" folds leave it out, so it is never folded and never drawn twice. Supersedes the same day's rule that hoisted only the cards and only while their fold was closed. Normal mode is unchanged.
/// SEE-ALSO: apps/desktop/src/app/native_chat/transcript.rs and apps/mobile/app/src/chat/native/transcript/TranscriptItemRow.tsx draw them.
pub fn content_rows(work: &[ChatMessage]) -> Vec<ChatMessage> {
    work.iter()
        .filter(|row| row.role == ChatRole::Assistant && row.id != STREAMING_ID)
        .filter(|row| {
            !crate::transcript::question_forms::content_fences(
                &crate::transcript::prose::prose_markdown(&row.blocks),
            )
            .is_empty()
        })
        .cloned()
        .collect()
}

/// A message between agents: another agent's message delivered into the chat (an orchestrator's
/// brief, a thread's report, `ghostex agents send`) rather than the user's own words.
///
/// CDXC:SessionChat 2026-10-27 DECISION:
/// User: "when i use summary we should hide messages to and from agents under the "Agent reply" collapsed areas there". In summary mode a message from another agent opens no turn of its own: it and the agent's answer to it fold into the "Agent reply" of the turn the user's own prompt opened, beside the agent's replies, so the summary is the user's own conversation. A chat that starts with one (a thread's brief) still opens its first turn with it. The agent's own sends to other agents are tool calls, already inside the folds.
pub fn is_agent_conversation(message: &ChatMessage) -> bool {
    if message.role != ChatRole::User {
        return false;
    }
    let text = crate::transcript::prose::prose_markdown(&message.blocks);
    let body = js_trim(&text);
    body.starts_with("Ghostex thread report:")
        || crate::transcript::agent_message::parse_inter_agent_message(body).is_some()
        || crate::transcript::agent_message::parse_agent_message(body).is_some()
}

/// A report one of this session's own background agents handed back (Claude's
/// `<agent-message from="…">`), which reaches the session as a user row. A message from another
/// session or agent (`ghostex agents send`, `<cross-session-message>`) is not one.
pub fn is_subagent_report(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && crate::transcript::agent_message::parse_cross_session_message(
            &crate::transcript::prose::prose_markdown(&message.blocks),
        )
        .is_some_and(|report| report.subagent)
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

pub(crate) fn has_agent_response_content(message: &ChatMessage) -> bool {
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

/// A row that opens a summary-mode turn: a prompt the agent took, or a slash command the reader
/// sent. A row the list never shows opens nothing.
pub fn opens_turn(message: &ChatMessage) -> bool {
    (is_accepted_user_prompt(message) || is_command_turn(message)) && !is_hidden_message(message)
}

/// One compact row per genuine user prompt, paired with its settled final reply.
///
/// CDXC:SessionChat 2026-10-13 DECISION: User: "ok if the issue was that i had summary mode on, i think we should just expand the currently running agent's work even in this status you know? (the stuff under the last user message basically needs to not be all hidden like this)" and "also yea why not, if you can show the description of the tool call would be nice". While a summary-mode turn runs, its "Active work" fold starts open (`workOpen`) and the agent's newest row opens its tool calls (`running_row`, `toolsOpen`), so the work under the last prompt shows as it does with summary mode off; the reader can close the fold for the rest of that turn, and the settled turn folds as before. (A tool row's description label, which this decision first gave Simple mode, now applies in every mode: see `tool_description` in `tool_summary.rs`.) Supersedes the same day's rule that kept the newest message and running row in view under a closed fold. Exception (the chat look's contributor, 2026-11-09, merged by the user): under Agent work Reduced and Quiet the running work starts shut and Quiet hides it while the agent runs (`presentation::turn_items`).
pub fn summary_mode_turns(
    messages: &[ChatMessage],
    final_assistant_message_ids: &[String],
    is_working: bool,
) -> Vec<SummaryModeTurn> {
    let mut turns: Vec<SummaryModeTurn> = Vec::new();
    let wakes: Vec<&str> = (0..messages.len())
        .filter(|at| opens_omp_turn(messages, *at))
        .map(|at| messages[at].id.as_str())
        .collect();
    for message in messages {
        // A held prompt (agent-CLI queue row, mid-turn send echo) has not started its own response
        // yet: it stays inside the working turn's activeWork instead of opening a turn whose reply
        // would never come.
        if opens_turn(message) && (turns.is_empty() || !is_agent_conversation(message)) {
            turns.push(SummaryModeTurn {
                active: false,
                active_work: Vec::new(),
                earlier_replies: Vec::new(),
                final_message: None,
                user: message.clone(),
                outcome: Vec::new(),
                running_row: None,
                cards: Vec::new(),
            });
        } else if let Some(current) = turns.last_mut() {
            current.active_work.push(message.clone());
            if final_assistant_message_ids.contains(&message.id) {
                current.final_message = Some(message.clone());
            }
        }
    }
    for turn in &mut turns {
        turn.cards = content_rows(&turn.active_work);
        if let Some(final_message) = &turn.final_message {
            turn.earlier_replies = earlier_replies(&turn.active_work, final_message, &wakes);
        }
        let final_at = turn.final_message.as_ref().and_then(|final_message| {
            turn.active_work
                .iter()
                .position(|row| row.id == final_message.id)
        });
        turn.outcome = turn
            .active_work
            .iter()
            .enumerate()
            .filter(|(at, row)| {
                // A compaction after the reply, or under a `/compress` that has none, is the seam
                // before the next turn, as in verbose mode.
                let seam = final_at.is_none_or(|final_at| *at > final_at);
                is_command_output_turn(row)
                    || suppressed_turn_label(row).as_deref() == Some("Interrupted")
                    || (seam && is_turn_seam(row))
                    // A side question stays in view in summary mode too.
                    || crate::transcript::side_question::is_side_question_message(row)
            })
            .map(|(_, row)| row.clone())
            .collect();
    }
    if is_working {
        if let Some(newest) = turns.last_mut() {
            newest.active = true;
            newest.running_row = newest
                .active_work
                .iter()
                .rev()
                .find(|row| matches!(row.role, ChatRole::Assistant | ChatRole::Reasoning))
                .map(|row| row.id.clone());
        }
    }
    turns
}

/// CDXC:SessionChat 2026-09-26 DECISION: User: in summary mode, expanding "Agent reply" must show all of the agent's replies to that prompt, not only the last one. A turn the harness broke up (a background task finishing, a held prompt) gets one reply per stretch, the same replies verbose mode leaves outside its "Worked for" folds; text before the final reply inside its own stretch is still commentary.
///
/// `wakes` are the OMP rows that woke the agent after a reply (`opens_omp_turn`), which end a
/// stretch as a background task's user row does.
fn earlier_replies(
    work: &[ChatMessage],
    final_message: &ChatMessage,
    wakes: &[&str],
) -> Vec<ChatMessage> {
    let Some(final_at) = work.iter().position(|row| row.id == final_message.id) else {
        return Vec::new();
    };
    let mut replies = Vec::new();
    let mut reply: Option<&ChatMessage> = None;
    for row in &work[..final_at] {
        if row.role == ChatRole::User || wakes.contains(&row.id.as_str()) {
            if let Some(settled) = reply.take() {
                replies.push(settled.clone());
            }
            // Another agent's message folds in among the replies (`is_agent_conversation`).
            if is_agent_conversation(row) && !is_hidden_message(row) {
                replies.push(row.clone());
            }
        } else if has_agent_response_content(row) {
            reply = Some(row);
        }
    }
    replies
}

pub fn is_visible_assistant_artifact(message: &ChatMessage) -> bool {
    message.role == ChatRole::Assistant
        && message
            .blocks
            .iter()
            .any(|block| matches!(block, ChatBlock::ImageRef { .. }))
}

/// Splits a completed turn's work into the rows that stay visible and the rows that collapse.
/// Under Agent work Reduced or Quiet (`grouped`) a subagent's report is work too
/// ([`completed_work_render_items`]).
pub fn partition_completed_work(
    messages: &[ChatMessage],
    grouped: bool,
) -> (Vec<ChatMessage>, Vec<ChatMessage>) {
    let mut visible_artifacts = Vec::new();
    let mut collapsed_work = Vec::new();
    for message in messages {
        // The "Interrupted" marker stays in view as it does in summary mode; Claude's used to be a user row, gxserver's own is a system row.
        // A model or effort change made while the turn ran stays in view too, as a status row.
        if is_visible_assistant_artifact(message)
            || (message.role == ChatRole::User
                && !is_background_task_row(message)
                && !(grouped && is_subagent_report(message)))
            || suppressed_turn_label(message).as_deref() == Some("Interrupted")
            || is_setting_status(message)
            || crate::transcript::open_questions::asks_questions(message)
        {
            visible_artifacts.push(message.clone());
        } else {
            collapsed_work.push(message.clone());
        }
    }
    (visible_artifacts, collapsed_work)
}

/// The other visible OMP custom message types that start a turn of their own when they reach the
/// agent while it is idle; they stay the marker line they always were.
const OMP_WAKE_TYPES: [&str; 5] = [
    "launch-completion",
    "lsp-late-diagnostic",
    "live-delegation",
    "collab-prompt",
    "skill-prompt",
];

/// An OMP message of a kind that starts a turn of its own when it reaches the agent while it is
/// idle: a background job's result, another agent's IRC message, advisor notes, and the
/// `OMP_WAKE_TYPES`.
///
/// CDXC:SessionChat 2026-10-30 WHY: in oh-my-pi 18.13.3 these visible deliveries start an agent turn when the agent is idle: `async-result`, `launch-completion` and `lsp-late-diagnostic` through the yield queue's idle flush (`session/yield-queue.ts`, `injectIdle` in `session/agent-session.ts`), an `irc:incoming` through `wakeForIrc` (`session/irc-bridge.ts`), advisor notes and `live-delegation` sent with `triggerTurn` (`session/session-advisors.ts`, `live/controller.ts`), and `collab-prompt` and `skill-prompt` through `promptCustomMessage` (`collab/host.ts`, `modes/skill-command.ts`). They are system rows, not the user rows a Claude wake-up arrives as, so without this the reply after one became the previous turn's final reply and the reply before it folded away. Where OMP adds one without a turn (plan mode, after the user stopped the agent, a preserved advisor card), the row stands between the turns with nothing folded under it. OMP types that never start a turn (`background-tan-dispatch`, `session-transfer`, the display-only IRC relay cards) keep the old placement.
pub(crate) fn is_omp_wake_kind(message: &ChatMessage) -> bool {
    message.role == ChatRole::System
        && message.custom_message.as_ref().is_some_and(|custom| {
            matches!(
                custom.custom_type.as_str(),
                OMP_ASYNC_RESULT | OMP_ADVISOR | OMP_IRC_INCOMING
            ) || OMP_WAKE_TYPES.contains(&custom.custom_type.as_str())
        })
}

/// An OMP row that woke the agent after its reply (`wakes_settled_agent`), and so opens a turn.
pub(crate) fn opens_omp_turn(messages: &[ChatMessage], index: usize) -> bool {
    is_omp_wake_kind(&messages[index]) && wakes_settled_agent(messages, index)
}

/// A harness row that woke the agent after it had finished replying: a background task or watcher
/// notification, a message from another session, and the like, landing after the agent's last row
/// was a reply rather than a tool call. The reply before it ended a turn of its own, and the row
/// starts a new one.
///
/// CDXC:SessionChat 2026-10-20 DECISION:
/// User: "we're collapsing the agent message i want to actually read (last agent message of a turn) whenever a new message from the agent due to a watcher (just says something like "Linear watcher: nothing new") [...] instead if we get multiple agent final messages in a row but without a user message or messages from another agent etc. then we need to show the multiple agent last turn replies". Every turn the agent finishes is a turn with its own final reply, whether a prompt or a wake-up started it: each reply stays in view as a reply (flush, with its actions), with only its own work folded above it, and a short watcher reply after it never turns it back into commentary. A notification that lands while the agent is still mid-response (its last row is a tool call) is not a wake-up and ends nothing. Supersedes the rule that only a prompt, never a harness row, ended a turn's reply (`final_assistant_message_ids`).
pub fn wakes_settled_agent(messages: &[ChatMessage], index: usize) -> bool {
    let message = &messages[index];
    let omp = is_omp_wake_kind(message);
    if !omp {
        if message.role != ChatRole::User
            || is_command_turn(message)
            || is_command_output_turn(message)
        {
            return false;
        }
        match suppressed_turn_label(message).as_deref() {
            None | Some(INTERRUPTED_LABEL) | Some(SIDE_QUESTION_LABEL) => return false,
            Some(_) => {}
        }
    }
    // CDXC:SessionChat 2026-10-30 WHY: OMP's persisted records do not say whether a delivery started a turn (a preserved advisor card, an IRC message in plan mode and a result after the user stopped the agent are written like any other, `recordWithoutTurn` in `session/agent-session.ts`), so an OMP row opens a turn only when the agent's own work or reply follows it before the next user row. A passive OMP notice (the `/tan` dispatch notice) is not the agent's response and does not hide the reply before it.
    if omp
        && !messages[index + 1..]
            .iter()
            .take_while(|row| row.role != ChatRole::User)
            .any(|row| {
                matches!(
                    row.role,
                    ChatRole::Assistant | ChatRole::Reasoning | ChatRole::Tool
                )
            })
    {
        return false;
    }
    messages[..index]
        .iter()
        .rev()
        .find(|row| !passed_over_before_wake(row))
        .is_some_and(is_settled_reply)
}

/// A row [`wakes_settled_agent`] looks past for the row before a delivery: a row the chat never
/// shows, a harness marker, or an OMP notice of a kind that never starts a turn (the `/tan`
/// dispatch notice). The chat drops hidden rows before it looks; gxserver, which reads the rows
/// as they were written, looks past them here.
pub fn passed_over_before_wake(row: &ChatMessage) -> bool {
    is_hidden_message(row)
        || suppressed_turn_label(row).is_some()
        || (row.role == ChatRole::System && row.custom_message.is_some() && !is_omp_wake_kind(row))
}

/// The agent's finished reply, which a delivery after it wakes the agent from: response content
/// whose last block is text or an image, not a tool call still waiting for its result.
pub fn is_settled_reply(row: &ChatMessage) -> bool {
    has_agent_response_content(row)
        && matches!(
            row.blocks.last(),
            Some(ChatBlock::Text { .. } | ChatBlock::ImageRef { .. })
        )
}

/// A row of the agent's own work in a turn: its words, thoughts and calls, and a background task's
/// end, which is drawn as a call.
fn is_agent_work_row(row: &ChatMessage) -> bool {
    matches!(
        row.role,
        ChatRole::Assistant | ChatRole::Reasoning | ChatRole::Tool
    ) || is_background_task_row(row)
}

/// Whether the row at `at` is a subagent's report that reached the agent mid-response: the agent's
/// last row before it is not a finished reply ([`is_settled_reply`], past the rows a wake-up test
/// looks past), so the words before it are commentary between calls, not a reply.
///
/// CDXC:SessionChat 2026-11-09 DECISION:
/// Agent work Reduced and Quiet treat a report from one of the session's own subagents (`<agent-message from="…">`, a user row) and the harness's `<task-notification>` as the agent's work. User, agreeing to the behaviour: under Reduced they are a step in the turn's line, and opening the line shows the report's card; under Quiet they sit behind the reply's work button, and while the agent runs the status line says it is reading the subagent's report; they never open a turn of their own, and a report that wakes an idle agent shows (under Quiet) only the reply it writes. So the report's turn folds with the report in it ([`completed_work_render_items`]), which merges it into the line of the message it answers (`grouped_work.rs`), and the words a report cut short fold as commentary. Messages from other sessions or agents (`ghostex agents send`, `<cross-session-message>`) stay cards: they are real messages. Default is unchanged: the report is its own card there (the 2026-09-30 decision on `parse_cross_session_message`).
pub(crate) fn report_lands_mid_response(messages: &[ChatMessage], at: usize) -> bool {
    messages.get(at).is_some_and(is_subagent_report) && lands_mid_response(messages, at)
}

/// Under Reduced and Quiet: whether the row at `at` opens a turn that is the agent's work landing
/// mid-response, so the words before it are commentary: a subagent's report
/// ([`report_lands_mid_response`]), or a background task's end that gxserver handed back holding
/// the rows after it (`deferredWork`), which opens a turn here so those rows still load.
///
/// CDXC:SessionChat 2026-11-09 WHY: gxserver's collapsed history cuts a turn at every user row, so a task notification after a subagent's report holds the rest of the response in its own `deferredWork`; read as one of the turn's calls, that section was never loaded and the line's steps stopped at the notification. Default keeps the notification as one of the turn's calls and reads its section with the turn's own instead (`turn_section`).
fn work_lands_mid_response(messages: &[ChatMessage], at: usize) -> bool {
    report_lands_mid_response(messages, at)
        || messages.get(at).is_some_and(|row| {
            is_background_task_row(row)
                && row.deferred_work.is_some()
                && lands_mid_response(messages, at)
        })
}

/// Whether the words in `rows[0]` introduce a question the agent asks after them (its
/// `AskUserQuestion` and the like, in the same row or a later one): they are not the turn's reply.
///
/// CDXC:SessionChat 2026-11-09 WHY: the turn's Completed can reach the chat before the reply the agent wrote after the answer, and for that moment the turn read as finished with the question's sentence as its reply. Under Reduced and Quiet the turn's tail draws its answered card, and the sentence's row drew the same card again, so a second answered card and the sentence showed for about a second until the reply landed.
fn introduces_question(rows: &[ChatMessage]) -> bool {
    let Some((first, later)) = rows.split_first() else {
        return false;
    };
    let after_words = first
        .blocks
        .iter()
        .rposition(|block| matches!(block, ChatBlock::Text { .. } | ChatBlock::ImageRef { .. }))
        .map_or(&first.blocks[..], |at| &first.blocks[at + 1..]);
    after_words
        .iter()
        .chain(later.iter().flat_map(|row| row.blocks.iter()))
        .any(|block| {
            matches!(block, ChatBlock::ToolCall { name, .. }
                if crate::questions::exchange::is_question_tool_name(name))
        })
}

/// Whether the agent's last row before `at` is not a finished reply ([`is_settled_reply`]), past
/// the rows a wake-up test looks past.
fn lands_mid_response(messages: &[ChatMessage], at: usize) -> bool {
    !messages[..at]
        .iter()
        .rev()
        .find(|row| !passed_over_before_wake(row))
        .is_some_and(is_settled_reply)
}

/// Where the response the agent is CURRENTLY producing begins: the last user row that is a genuine
/// prompt the agent has accepted for delivery, or a harness row that woke it after it had finished
/// (`wakes_settled_agent`).
///
/// A harness-injected turn (task notification, local command output) landing WHILE the agent is
/// mid-response and a prompt the agent CLI is still holding in its queue both arrive as user rows;
/// neither starts a new response, so neither may settle the one in flight. Falls back to 0 when no
/// such row exists (stitched scroll-back that opens mid-conversation).
pub(crate) fn active_response_start_index(messages: &[ChatMessage]) -> usize {
    (0..messages.len())
        .rev()
        .find(|at| is_accepted_user_prompt(&messages[*at]) || wakes_settled_agent(messages, *at))
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
        // An OMP row that woke the agent ends the reply before it, as a harness user row does.
        if is_omp_wake_kind(message) {
            if wakes_settled_agent(messages, index) {
                if let Some(id) = final_assistant_id.take() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
            continue;
        }
        // A harness-injected turn (a background-task notification, local command output, a message
        // from another session) landing mid-response is authored by the terminal, not by the
        // reader: the agent is still mid-response on both sides of it. Ending the turn there put a
        // copy affordance under commentary that the agent then kept building on. One that woke the
        // agent after it had finished does end the reply before it (`wakes_settled_agent`).
        if suppressed_turn_label(message).is_some() && !is_command_turn(message) {
            // An interrupted response has no final reply: its commentary folds into "Worked for" and the marker closes the turn, instead of the marker drawing above text the agent wrote before it was stopped.
            if suppressed_turn_label(message).as_deref() == Some("Interrupted") {
                final_assistant_id = None;
            } else if wakes_settled_agent(messages, index) {
                // A wake-up after a finished reply: that reply ended its own turn.
                if let Some(id) = final_assistant_id.take() {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
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
///
/// `grouped` is Agent work Reduced or Quiet (`transcript/chat_look.rs`), where every finished turn
/// folds and a subagent's report is the agent's work ([`report_lands_mid_response`]).
pub fn completed_work_render_items(
    messages: &[ChatMessage],
    is_working: bool,
    interacted_message_ids: &[String],
    raw_messages: &[ChatMessage],
    grouped: bool,
) -> (Vec<RenderItem>, Vec<String>) {
    let mut items = Vec::new();
    // The words a subagent's report cut short, which are commentary rather than a reply.
    let mut demoted = Vec::new();
    let raw_index = |id: &str| raw_messages.iter().position(|message| message.id == id);
    let active_start = if is_working {
        active_response_start_index(messages)
    } else {
        messages.len()
    };
    let mut index = 0;
    // The `deferredWork` of the latest prompt, which also holds the work of the OMP wake-ups after
    // it (`CompletedWorkTurn::section`).
    let mut section: Option<DeferredWork> = None;
    while index < messages.len() {
        let message = &messages[index];
        if message.role != ChatRole::User && !opens_omp_turn(messages, index) {
            items.push(RenderItem::Message(Box::new(message.clone())));
            index += 1;
            continue;
        }
        if message.role == ChatRole::User {
            section = message.deferred_work.clone();
        }

        let mut next_user_index = index + 1;
        // CDXC:SessionChat 2026-10-17 WHY: a slash command opens its own turn (the 2026-09-10
        // decision on `is_command_turn`) even after a turn gxserver folded. gxserver's archived row
        // for a command the agent does not record (Cursor's `/run-everything`) has no byte offset,
        // so it was folded into the turn before it and drawn above that turn's reply.
        while next_user_index < messages.len()
            && ((messages[next_user_index].role != ChatRole::User
                && !opens_omp_turn(messages, next_user_index))
                // A background task ending mid-response is one of the turn's calls
                // (`background_rows.rs`); one that woke the agent after its reply opens a turn.
                || (is_background_task_row(&messages[next_user_index])
                    && !wakes_settled_agent(messages, next_user_index)
                    && !(grouped && messages[next_user_index].deferred_work.is_some()))
                || (message.deferred_work.is_some()
                    && messages[next_user_index].byte_offset.is_none()
                    && !is_command_turn(&messages[next_user_index])))
        {
            next_user_index += 1;
        }
        let turn_messages = &messages[index + 1..next_user_index];
        let final_index = turn_messages.iter().rposition(has_agent_response_content);
        // Stopped after its last text: the turn folds with no final reply, and the marker (kept in view by `partition_completed_work`) ends it.
        let interrupted = final_index.is_some_and(|at| {
            turn_messages[at + 1..]
                .iter()
                .any(|row| suppressed_turn_label(row).as_deref() == Some("Interrupted"))
        });
        let final_index = final_index.filter(|_| !interrupted);
        // A turn a background task's end or an OMP wake-up opened folds that row with its work,
        // as the call or card it is; so does a subagent's report under Reduced and Quiet.
        let opener_is_work = message.role != ChatRole::User
            || is_background_task_row(message)
            || (grouped && is_subagent_report(message));
        // Reduced and Quiet: words a subagent's report cut short are not a reply, nor are the
        // words that introduce a question.
        let final_index = match final_index {
            Some(at)
                if grouped
                    && (work_lands_mid_response(messages, next_user_index)
                        || introduces_question(&turn_messages[at..])) =>
            {
                demoted.push(turn_messages[at].id.clone());
                None
            }
            final_index => final_index,
        };
        let interacted_inline_diff = turn_messages
            .iter()
            .any(|row| interacted_message_ids.contains(&row.id));
        let interacted_grouped_diff = interacted_message_ids.contains(&message.id);
        let folds_without_reply =
            grouped && (opener_is_work || turn_messages.iter().any(is_agent_work_row));
        if (section.is_none() && final_index.is_none() && !interrupted && !folds_without_reply)
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
        /*
        CDXC:SessionChat 2026-09-28 WHY:
        A compaction the reader runs between turns (Hermes `/compress`, Codex `/compact`) is
        recorded as a system row after the previous turn's reply, with no user row of its own, so
        the fold swallowed it and drew it above that reply. It is the seam before the next turn,
        not work.
        */
        // A turn folded with no reply (Reduced, Quiet) ends with its last piece of work.
        let tail_from = final_index.or_else(|| {
            grouped
                .then(|| turn_messages.iter().rposition(is_agent_work_row))
                .flatten()
        });
        let trailing_compactions: Vec<&ChatMessage> = tail_from
            .map_or(&[][..], |at| &turn_messages[at + 1..])
            .iter()
            .filter(|row| is_turn_seam(row))
            .collect();
        let raw_start = raw_index(&message.id).expect("a rendered row comes from the raw list");
        let raw_end = messages
            .get(next_user_index)
            .and_then(|next| raw_index(&next.id))
            .unwrap_or(raw_messages.len());
        let final_id = final_message.as_ref().map(|row| row.id.clone());
        if !opener_is_work {
            items.push(RenderItem::Message(Box::new(message.clone())));
        }
        let work_start = if opener_is_work {
            raw_start
        } else {
            raw_start + 1
        };
        items.push(RenderItem::CompletedWork(Box::new(CompletedWorkTurn {
            user: message.clone(),
            work: raw_messages[work_start.min(raw_end)..raw_end]
                .iter()
                .filter(|row| {
                    Some(&row.id) != final_id.as_ref()
                        && !trailing_compactions.iter().any(|seam| seam.id == row.id)
                })
                .cloned()
                .collect(),
            final_message,
            section: turn_section(section.as_ref(), turn_messages),
            // Only a wake-up's turn takes its rows from the same section.
            next_opener: (next_user_index < messages.len()
                && opens_omp_turn(messages, next_user_index))
            .then(|| messages[next_user_index].id.clone()),
        })));
        items.extend(
            trailing_compactions
                .into_iter()
                .map(|row| RenderItem::Message(Box::new(row.clone()))),
        );
        index = next_user_index;
    }
    (items, demoted)
}

/// The section a turn loads when opened: its opener's (`section`), reaching on to the end of the
/// last section a background task's end inside the turn holds, so one walk reads all of them.
///
/// CDXC:SessionChat 2026-11-09 WHY: gxserver's collapsed history cuts a turn at every user row, so a task notification that lands mid-response and stays one of the turn's calls (Agent work Default) holds the rest of the response in its own `deferredWork`; loading only the opener's section drew the turn up to the notification and lost everything after it. The rows between the sections are the turn's own, so a walk from the first section's start to the last one's end reads exactly the turn. Reduced and Quiet open a turn at such a row instead (`work_lands_mid_response`).
fn turn_section(
    section: Option<&DeferredWork>,
    turn_messages: &[ChatMessage],
) -> Option<DeferredWork> {
    turn_messages
        .iter()
        .filter(|row| is_background_task_row(row))
        .filter_map(|row| row.deferred_work.as_ref())
        .fold(section.cloned(), |merged, later| {
            Some(match merged {
                None => later.clone(),
                Some(mut merged) => {
                    merged.before_offset = later.before_offset;
                    merged.end_id = later.end_id.clone();
                    merged.completed_at = later.completed_at.or(merged.completed_at);
                    merged.message_count += later.message_count + 1;
                    for path in &later.file_paths {
                        if !merged.file_paths.contains(path) {
                            merged.file_paths.push(path.clone());
                        }
                    }
                    for path in &later.image_paths {
                        if !merged.image_paths.contains(path) {
                            merged.image_paths.push(path.clone());
                        }
                    }
                    merged
                }
            })
        })
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

/// "Worked for 12s", the label on a finished turn's fold and the working card's end line (the
/// 2026-10-11 work look, `work_summary.rs`); plain "Worked" for a turn with no length to show.
///
/// CDXC:SessionChat 2026-10-18 DECISION:
/// User: "Say Worked for 12s not Worked 12s. this is in all places we mention how long we worked please". Every work duration reads "Worked for 12s", "Worked for 2m 5s" or "Worked for 2m"; the renderers draw this label and format none of their own. Supersedes the "Worked 2m 5s" wording of the 2026-10-11 work look.
pub fn worked_duration_label(started_at: Option<i64>, completed_at: Option<i64>) -> String {
    let (Some(started_at), Some(completed_at)) = (started_at, completed_at) else {
        return "Worked".to_string();
    };
    // A turn the transcript times only to its prompt (a Cursor turn with no timed thinking) has no
    // length to show; "Worked for 1s" would be invented.
    if completed_at <= started_at {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, role: &str, text: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "role": role,
            "source": "transcript",
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    const PAGE: &str = "```visual\n{\"page\":{\"title\":\"Mockups\",\"url\":\"http://127.0.0.1:1/visual/0123456789abcdef0123456789abcdef\"}}\n```";
    const FORM: &str = "```questions\n{\"questions\":[{\"tag\":\"Q1\",\"title\":\"Ship it?\",\"choices\":[\"Yes\",\"No\"]}]}\n```";

    const CHART: &str = "```visual\n{\"title\":\"Weekly runs\",\"mark\":\"bar\",\"data\":{\"values\":[{\"a\":1}]},\"encoding\":{}}\n```";

    fn agent_message(id: &str, body: &str) -> ChatMessage {
        row(
            id,
            "user",
            &format!("{body}\n\nMessage from another agent\nAgent: Claude\nSession: Thread\nSession ID: G1\nAgent ID: claude\nAgent Session ID: x\nReply to: S1:P1:G1"),
        )
    }

    #[test]
    fn a_summary_turn_keeps_its_content_messages_in_full() {
        let messages = vec![
            row("u1", "user", "Make mockups"),
            row(
                "w1",
                "assistant",
                &format!("Here they are.\n\n{PAGE}\n\nMore words."),
            ),
            row(
                "w2",
                "assistant",
                "Working on it, no cards here. ```inline``` is not a fence.",
            ),
            row("w3", "assistant", &format!("Runs this week:\n\n{CHART}")),
            row("f1", "assistant", &format!("Done. One question:\n\n{FORM}")),
            row("u2", "user", "Thanks"),
            row("f2", "assistant", "You're welcome."),
        ];
        let turns = summary_mode_turns(&messages, &["f1".to_string(), "f2".to_string()], false);
        assert_eq!(turns.len(), 2);
        let ids: Vec<&str> = turns[0].cards.iter().map(|card| card.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["w1", "w3", "f1"],
            "whole messages: pages, charts and forms"
        );
        assert_eq!(
            turns[0].cards[0], messages[1],
            "the message itself, words and all"
        );
        assert!(turns[1].cards.is_empty());
    }

    /// A form the agent asked before it kept working stays below the finished turn's fold, so the
    /// ? pill's jump lands on a card the reader can see.
    #[test]
    fn a_finished_turn_keeps_its_question_replies_out_of_the_fold() {
        let work = vec![
            row("w1", "assistant", &format!("One question first:\n\n{FORM}")),
            row("w2", "assistant", "Running the checks now."),
        ];
        let (visible, folded) = partition_completed_work(&work, false);
        let ids = |rows: &[ChatMessage]| rows.iter().map(|m| m.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&visible), ["w1"]);
        assert_eq!(ids(&folded), ["w2"]);
        let withdraw = "```questions\n{\"withdraw\":[\"Q1\"]}\n```";
        let (visible, _) = partition_completed_work(&[row("w3", "assistant", withdraw)], false);
        assert!(visible.is_empty(), "a withdrawal asks nothing");
    }

    #[test]
    fn a_withdrawal_is_no_content() {
        let withdraw = "```questions\n{\"withdraw\":[\"Q1\"]}\n```";
        let rows = vec![row("a", "assistant", withdraw)];
        assert!(content_rows(&rows).is_empty());
    }

    #[test]
    fn messages_between_agents_fold_into_the_users_turn() {
        let messages = vec![
            row("u1", "user", "Fix the sidebar"),
            row("r1", "assistant", "Started a thread."),
            agent_message(
                "t1",
                "Ghostex thread report: finished its turn. Its final message:\n\nDone.",
            ),
            row("r2", "assistant", "The thread fixed it."),
            row("u2", "user", "Great"),
            row("r3", "assistant", "Anything else?"),
        ];
        let finals = ["r1", "r2", "r3"].map(str::to_string);
        let turns = summary_mode_turns(&messages, &finals, false);
        assert_eq!(turns.len(), 2, "the report opens no turn of its own");
        assert_eq!(turns[0].user.id, "u1");
        assert_eq!(
            turns[0].final_message.as_ref().map(|m| m.id.as_str()),
            Some("r2")
        );
        let folded: Vec<&str> = turns[0]
            .earlier_replies
            .iter()
            .map(|m| m.id.as_str())
            .collect();
        assert_eq!(
            folded,
            vec!["r1", "t1"],
            "the report folds in among the replies"
        );
    }

    #[test]
    fn a_chat_that_starts_with_an_agents_message_opens_its_turn_with_it() {
        let messages = vec![
            agent_message("b1", "Task: fix the sidebar"),
            row("r1", "assistant", "On it."),
        ];
        let turns = summary_mode_turns(&messages, &["r1".to_string()], false);
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0].user.id, "b1");
        assert!(is_agent_conversation(&messages[0]));
        assert!(!is_agent_conversation(&row(
            "u",
            "user",
            "Message from me, the user"
        )));
    }
}
