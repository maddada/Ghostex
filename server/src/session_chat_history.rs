use std::collections::BTreeSet;
use std::path::Path;

use ghostex_gx_chat_core::protocol::{ChatMessage, ChatRole};
use ghostex_gx_chat_core::transcript::noise::{is_hidden_message, may_suppress_turn};
use ghostex_gx_chat_core::transcript::tool_fold::{
    is_tool_only_blocks, takes_tool_burst, takes_tool_burst_row, tool_burst_step_for, ToolBurstStep,
};
use ghostex_gx_chat_core::transcript::transcript::passes_over_tool_burst;
use ghostex_gx_chat_core::transcript::turns::{is_settled_reply, passed_over_before_wake};
use serde_json::{json, Value};

use crate::session_chat::{
    SessionChatBlock, SessionChatMessage, SessionChatRole, SessionChatTranscriptAgent,
};
use crate::session_chat_fork_stitch::{
    decode_stitched_cursor, encode_stitched_cursor, read_session_chat_tail_page_stitched,
    FORK_BOUNDARY_MESSAGE_ID_PREFIX,
};
use crate::session_chat_tail::SessionChatTailPage;

/// CDXC:SessionChat 2026-09-15 DECISION:
/// User: older history should arrive collapsed on desktop, web and mobile; fetch the middle work only when expanded, instead of displaying pages of tools before their user prompt arrives.
/// Scan in bounded chunks and retain summaries while crossing a long turn, so omitted tool payloads do not accumulate in the history response.
pub(crate) fn read_history(
    agent: SessionChatTranscriptAgent,
    path: &Path,
    before: u64,
    limit: usize,
    detail: bool,
    preserve_newest: bool,
    project_id: &str,
    session_id: &str,
) -> std::io::Result<Value> {
    let archived =
        crate::session_chat_local_command::load_session_chat_local_commands(project_id, session_id);
    if detail {
        let page =
            read_session_chat_tail_page_stitched(agent, path, limit.clamp(1, 200), Some(before))?;
        return Ok(match page.page {
            SessionChatTailPage::NotFound => page_value(Vec::new(), false, before),
            SessionChatTailPage::Page {
                messages,
                has_more,
                before_offset,
                ..
            } => {
                let commands =
                    crate::session_chat_local_command::select_session_chat_local_commands(
                        archived, &messages, false, has_more,
                    );
                let messages = crate::session_chat_local_command::merge_session_chat_local_commands(
                    messages, &commands,
                );
                page_value(
                    messages.iter().map(|message| json!(message)).collect(),
                    has_more,
                    before_offset,
                )
            }
        });
    }
    let mut cursor = before;
    let mut examined = 0;
    let mut output = Vec::new();
    let mut turn = Turn::default();
    let mut preserve = preserve_newest;
    loop {
        let page = read_session_chat_tail_page_stitched(agent, path, 200, Some(cursor))?;
        let SessionChatTailPage::Page {
            messages,
            has_more,
            before_offset,
            ..
        } = page.page
        else {
            output.extend(unanchored_work(agent, path, before, turn, &archived)?);
            output.reverse();
            return Ok(page_value(output, false, cursor));
        };
        // Stitched pages can cross files. Each boundary separates an older hop
        // from the next newer one; keep the existing opaque cursor encoding.
        let commands = crate::session_chat_local_command::select_session_chat_local_commands(
            archived.clone(),
            &messages,
            false,
            has_more,
        );
        let messages = crate::session_chat_local_command::merge_session_chat_local_commands(
            messages, &commands,
        );
        let (mut hop, _) = decode_stitched_cursor(before_offset);
        let mut cursors = Vec::with_capacity(messages.len());
        for message in &messages {
            if message.id.starts_with("fork-boundary:") {
                hop = hop.saturating_sub(1);
            }
            cursors.push(encode_stitched_cursor(
                hop,
                message.byte_offset.unwrap_or(0),
            ));
        }
        for (row_index, (message, row_cursor)) in
            messages.into_iter().zip(cursors).enumerate().rev()
        {
            examined += 1;
            if message.id.starts_with("fork-boundary:") {
                output.extend(unanchored_work(agent, path, before, turn, &archived)?);
                output.push(json!(message));
                turn = Turn::default();
                continue;
            }
            if message.role == SessionChatRole::User
                && message.byte_offset.is_some()
                && !hidden_in_chat(&message)
            {
                output.extend(turn.finish(Some(message), before));
                turn = Turn::default();
                preserve = false;
                if examined >= limit.max(1) {
                    output.reverse();
                    return Ok(page_value(output, row_index > 0 || has_more, row_cursor));
                }
            } else {
                turn.push(message, preserve);
            }
        }
        if !has_more || before_offset == cursor {
            output.extend(unanchored_work(agent, path, before, turn, &archived)?);
            output.reverse();
            return Ok(page_value(output, false, before_offset));
        }
        cursor = before_offset;
    }
}

/// CDXC:SessionChat 2026-09-16 DECISION:
/// User: the initial chat snapshot carries only the latest turn in detail; every older completed turn in the tail window arrives collapsed like a history page and loads its work on expansion.
/// The detailed region starts at the newest genuine prompt (not queued, not a harness-injected row), the same boundary the message list uses for a streaming response, so a task notification or local command output landing mid-turn never folds live work.
/// Rows no prompt owns (a window that starts mid-turn, a turn cut by a fork boundary) stay verbatim, and each collapsed turn's `beforeOffset` is the cursor of the row after it so expansion finds the turn on its first detail page.
pub(crate) fn collapse_snapshot_messages(
    messages: &[SessionChatMessage],
    before_offset: u64,
) -> Vec<Value> {
    // A row the chat never shows opens no turn there, so it owns no work here either.
    let prompt: Vec<bool> = messages
        .iter()
        .map(|message| {
            message.role == SessionChatRole::User
                && message.byte_offset.is_some()
                && !hidden_in_chat(message)
        })
        .collect();
    let Some(detail_start) = messages.iter().enumerate().rposition(|(index, message)| {
        prompt[index]
            && !message.queued
            && !crate::session_chat_decode_claude::is_noise_message(message)
    }) else {
        return messages.iter().map(|message| json!(message)).collect();
    };
    let (mut hop, _) = decode_stitched_cursor(before_offset);
    let mut cursors = Vec::with_capacity(messages.len());
    for message in messages {
        if message.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX) {
            hop = hop.saturating_sub(1);
        }
        cursors.push(encode_stitched_cursor(
            hop,
            message.byte_offset.unwrap_or(0),
        ));
    }
    let mut segments: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for (index, message) in messages.iter().enumerate().take(detail_start) {
        let boundary = message.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX);
        if boundary || prompt[index] {
            if index > start {
                segments.push((start, index));
            }
            if boundary {
                segments.push((index, index + 1));
                start = index + 1;
            } else {
                start = index;
            }
        }
    }
    if detail_start > start {
        segments.push((start, detail_start));
    }
    let mut output = Vec::with_capacity(messages.len());
    for (segment_start, segment_end) in segments {
        let first = &messages[segment_start];
        if first.id.starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX) || !prompt[segment_start] {
            output.extend(
                messages[segment_start..segment_end]
                    .iter()
                    .map(|message| json!(message)),
            );
            continue;
        }
        let before = (segment_end..messages.len())
            .find(|index| {
                !messages[*index]
                    .id
                    .starts_with(FORK_BOUNDARY_MESSAGE_ID_PREFIX)
            })
            .map(|index| cursors[index])
            .unwrap_or(before_offset);
        let mut turn = Turn::default();
        for message in messages[segment_start + 1..segment_end].iter().rev() {
            turn.push(message.clone(), false);
        }
        let mut rows = turn.finish(Some(first.clone()), before);
        rows.reverse();
        output.extend(rows);
    }
    output.extend(
        messages[detail_start..]
            .iter()
            .map(|message| json!(message)),
    );
    output
}

// A transcript can begin with work without a user prompt, or a fork can cut a
// turn in half. Such rows have no valid disclosure owner: return their original
// content instead of losing it behind a summary that cannot be expanded.
fn unanchored_work(
    agent: SessionChatTranscriptAgent,
    path: &Path,
    before: u64,
    turn: Turn,
    archived: &[crate::session_chat_local_command::SessionChatLocalCommand],
) -> std::io::Result<Vec<Value>> {
    if turn.deferred == 0 {
        return Ok(turn.finish(None, before));
    }
    let mut cursor = before;
    let mut found = false;
    let mut messages = Vec::new();
    loop {
        let page = read_session_chat_tail_page_stitched(agent, path, 200, Some(cursor))?;
        let SessionChatTailPage::Page {
            messages: chunk,
            before_offset,
            has_more,
            ..
        } = page.page
        else {
            break;
        };
        let commands = crate::session_chat_local_command::select_session_chat_local_commands(
            archived.to_vec(),
            &chunk,
            false,
            has_more,
        );
        let chunk =
            crate::session_chat_local_command::merge_session_chat_local_commands(chunk, &commands);
        for message in chunk.into_iter().rev() {
            if Some(&message.id) == turn.end_id.as_ref() {
                found = true;
            }
            if found {
                messages.push(json!(message));
            }
            if messages.len() == turn.count {
                return Ok(messages);
            }
        }
        if !has_more || cursor == before_offset {
            break;
        }
        cursor = before_offset;
    }
    Ok(messages)
}

fn page_value(messages: Vec<Value>, has_more: bool, before_offset: u64) -> Value {
    json!({"messages": messages, "hasMore": has_more, "hasMoreExact": true,
        "beforeOffset": before_offset, "historyMode": "turns", "status": "ready", "epoch": 0, "seq": 0})
}

/// The OMP custom message types that start an agent turn when the agent is idle, which the chat
/// draws as the start of a turn of their own when the agent's work follows them.
/// SEE-ALSO: `is_omp_wake_kind` in packages/gx-chat-core/src/transcript/turns.rs.
const OMP_WAKE_TYPES: [&str; 8] = [
    "async-result",
    "irc:incoming",
    "advisor",
    "launch-completion",
    "lsp-late-diagnostic",
    "live-delegation",
    "collab-prompt",
    "skill-prompt",
];

/// An OMP delivery the agent worked after, waiting for the row before it to say whether it woke
/// the agent after a reply (rows arrive newest first).
struct PendingWake {
    /// Where the delivery sits in `Turn::kept`.
    at: usize,
    /// Whether one of the agent's rows after it is in view, and otherwise its oldest run of work
    /// out of view (`Turn::agent_folded`).
    agent_kept: bool,
    agent_folded: Vec<SessionChatMessage>,
}

/// CDXC:SessionChat 2026-10-30 WHY: an OMP delivery that woke the agent after its reply opens a turn of its own in the chat (`wakes_settled_agent` in gx-chat-core). A collapsed turn still folds the prompt's whole stretch, wake-ups included, under the prompt's `deferredWork`: that is the shape every chat core reads, so an older chat that does not open turns at OMP rows still reaches all of the work through the prompt. The chat core cuts the loaded stretch at the wake-ups (`completed_chat_work`). So that it sees the wake-up, the reply before it is kept the way the last one is. When none of the agent's rows after it is in view, the oldest group of its work after it is kept as well. A delivery that did not wake the agent but has the agent's rows after it in view keeps the row before it in view when the next row in view before it would otherwise be a finished reply, so the chat does not read it as a wake-up either. Whether a delivery woke the agent is decided with the chat core's own test of the row before it (`settles_before_delivery`), past the rows the chat never shows; one row decides for several deliveries with only such rows between them. Rows are grouped with the chat's own tool fold rules (`tool_burst_step_for`, `takes_tool_burst`, `passes_over_tool_burst`): a row and the tool-only rows that fold into it are one message in the chat, which pairs its calls with its results. A turn's work leaves its final reply out, so there the rows after that reply fold into the group before it. A group is kept whole when one of its rows with a tool call or result is in view (a reply with a call, a question, a message to another agent), so every call in view pairs with the results the chat pairs it with, before and after the fold loads. A kept reply without calls keeps only itself, as before; the calls the chat folds into it come back with the loaded work. Copies of that pairing rule here kept missing one of the chat's boundaries, and a call kept without its result read as stopped. Giving each wake-up a `deferredWork` of its own was tried and removed: an older chat never opens it and loses that work.
#[derive(Default)]
struct Turn {
    kept: Vec<SessionChatMessage>,
    end_id: Option<String>,
    completed_at: Option<i64>,
    count: usize,
    deferred: usize,
    final_found: bool,
    /// The agent worked or replied after the rows pushed so far, before any user row.
    agent_after: bool,
    /// Since the newest delivery pushed: one of the agent's rows is in view, and otherwise the
    /// oldest group of its work out of view, newest first.
    agent_kept: bool,
    agent_folded: Vec<SessionChatMessage>,
    /// The tool-only rows out of view since the last row that ends a run in the chat's tool fold,
    /// newest first, and in `run_at` the place in `kept` each takes if the run is kept.
    run: Vec<SessionChatMessage>,
    run_at: Vec<usize>,
    /// A row of the open run is in view, so the whole run is.
    run_kept: bool,
    /// The deliveries waiting for the row before them, newest first: every row between them is
    /// one the chat looks past, so the same row decides for all of them.
    pending: Vec<PendingWake>,
    /// A delivery the agent did not wake from has the agent's rows after it in view while the row
    /// before it is out of view (`check_guard`). `guard_run`: that row is in the open run; `held`
    /// and `held_at`: the groups it closed out of view, newest first, with their places in `kept`.
    guard: bool,
    guard_run: bool,
    held: Vec<SessionChatMessage>,
    held_at: Vec<usize>,
    /// The row being pushed is the row before such a delivery.
    guard_next: bool,
    files: BTreeSet<String>,
    /// The picture files the turn's calls opened, oldest first, and the calls whose result
    /// failed (rows arrive newest first, so a result is seen before its call).
    images: Vec<String>,
    failed_calls: BTreeSet<String>,
}

impl Turn {
    fn push(&mut self, message: SessionChatMessage, preserve: bool) {
        let before_delivery = !self.pending.is_empty()
            && match settles_before_delivery(&message) {
                None => false,
                Some(true) => {
                    self.woke();
                    false
                }
                Some(false) => self.not_woken(),
            };
        if self.end_id.is_none() {
            self.completed_at = message.timestamp;
            self.end_id = Some(message.id.clone());
        }
        self.count += 1;
        for block in &message.blocks {
            if let SessionChatBlock::ToolResult {
                is_error: Some(true),
                call_id: Some(id),
                ..
            } = block
            {
                self.failed_calls.insert(id.clone());
            }
            if let SessionChatBlock::ToolCall {
                name,
                input,
                call_id,
            } = block
            {
                // The thumbnails the folded turn shows before it is opened (CDXC:SessionChat
                // 2026-11-02 in packages/gx-chat-core/src/transcript/read_images.rs).
                let failed = call_id
                    .as_ref()
                    .is_some_and(|id| self.failed_calls.contains(id));
                if !failed {
                    for path in ghostex_gx_chat_core::transcript::read_images::image_paths_in_call(
                        name, input, None,
                    )
                    .into_iter()
                    .rev()
                    {
                        self.images.insert(0, path);
                    }
                }
            }
            if let SessionChatBlock::ToolCall { name, input, .. } = block {
                collect_file_paths(name, input, &mut self.files);
            }
        }
        let mut row = None;
        // The chat drops a row it never shows before it looks for deliveries.
        let wake = message.role == SessionChatRole::System
            && self.agent_after
            && message
                .custom_message
                .as_ref()
                .is_some_and(|custom| OMP_WAKE_TYPES.contains(&custom.custom_type.as_str()))
            && !read_once(&mut row, &message).is_some_and(is_hidden_message);
        let reply = message.role == SessionChatRole::Assistant
            && message.blocks.iter().any(|block| {
                matches!(block, SessionChatBlock::Text { text } if !text.trim().is_empty())
                    || matches!(block, SessionChatBlock::ImageRef { .. })
            });
        let final_reply = reply && !self.final_found;
        self.final_found |= reply;
        let agent = matches!(
            message.role,
            SessionChatRole::Assistant | SessionChatRole::Reasoning | SessionChatRole::Tool
        );
        if agent {
            self.agent_after = true;
        } else if message.role == SessionChatRole::User
            && !read_once(&mut row, &message).is_some_and(is_hidden_message)
        {
            self.agent_after = false;
            self.agent_kept = false;
            self.agent_folded.clear();
        }
        let question = message.blocks.iter().any(|block|
            matches!(block, SessionChatBlock::ToolCall { name, .. } if crate::session_chat_interactive::is_ask_user_question_tool(name)));
        let sent_message = message.blocks.iter().any(|block| {
            matches!(block, SessionChatBlock::ToolCall { name, input, .. } if sends_agent_message(name, input))
        });
        let visible = message
            .blocks
            .iter()
            .any(|block| matches!(block, SessionChatBlock::ImageRef { .. }));
        // A command's card line (`ghostex gbots create`'s "GBot added", gx-protocol chat_card.rs)
        // is drawn out of the fold like a sent message, so its row stays in view like one.
        let chat_card = message.blocks.iter().any(|block| {
            matches!(block, SessionChatBlock::ToolResult { output, .. }
                if output.contains(ghostex_gx_protocol::chat_card::CHAT_CARD_MARKER))
        });
        let keep = preserve
            || final_reply
            || question
            || sent_message
            || chat_card
            || visible
            || (reply && asks_questions(&message))
            || message
                .async_questions
                .as_ref()
                .is_some_and(|questions| !questions.is_empty())
            || matches!(
                message.role,
                SessionChatRole::System | SessionChatRole::User
            );
        if self.guard && keep {
            self.check_guard(&message, &mut row);
        }
        self.guard_next = before_delivery;
        match self.tool_burst_step(&message, &mut row) {
            ToolBurstStep::PassesOver if keep => self.keep_row(message, agent),
            ToolBurstStep::PassesOver => self.deferred += 1,
            ToolBurstStep::Joins if keep || self.run_kept => {
                self.keep_run();
                self.keep_row(message, agent);
            }
            ToolBurstStep::Joins => {
                self.deferred += 1;
                if std::mem::take(&mut self.guard_next) {
                    self.guard = true;
                    self.guard_run = true;
                }
                self.run_at.push(self.kept.len());
                self.run.push(message);
            }
            ToolBurstStep::Ends if final_reply => self.keep_final(message),
            ToolBurstStep::Ends => {
                // A background task's end is drawn as tool calls and takes the run after it like a
                // reply (`takes_tool_burst_row`); only a user or system row can be one.
                let takes = takes_tool_burst(&chat_role(message.role))
                    || (matches!(
                        message.role,
                        SessionChatRole::User | SessionChatRole::System
                    ) && read_once(&mut row, &message).is_some_and(takes_tool_burst_row));
                self.end_run(message, keep, agent, takes)
            }
        }
        self.guard_next = false;
        if wake {
            self.pending.push(PendingWake {
                at: self.kept.len() - 1,
                agent_kept: std::mem::take(&mut self.agent_kept),
                agent_folded: std::mem::take(&mut self.agent_folded),
            });
        }
    }

    /// The step `message` takes in the chat's tool fold, read off the row as gxserver holds it.
    /// Whether the fold passes over the row needs it read as the chat's row, and only a user or
    /// system row can be passed over (`may_suppress_turn`); that is read only when it changes the
    /// step, as a row that would end a run ends nothing while none is open. A delivery is read as
    /// one that did not wake the agent until the row before it says it did (`woke`).
    fn tool_burst_step(
        &self,
        message: &SessionChatMessage,
        row: &mut Option<Option<ChatMessage>>,
    ) -> ToolBurstStep {
        let role = chat_role(message.role);
        let tool_only = is_tool_only_blocks(&message.blocks, is_tool_block);
        let step = tool_burst_step_for(false, tool_only, &role);
        let run_open = !self.run.is_empty() || self.run_kept;
        if !may_suppress_turn(&role) || (step == ToolBurstStep::Ends && !run_open) {
            return step;
        }
        let passes_over =
            read_once(row, message).is_some_and(|row| passes_over_tool_burst(row, false));
        tool_burst_step_for(passes_over, tool_only, &role)
    }

    /// The turn's final reply. In the chat it takes the run after it, and its turn's work, which
    /// leaves it out (`completed_chat_work`), folds that run into the group before it instead: the
    /// run stays open past it. A reply with calls keeps that run with it.
    fn keep_final(&mut self, message: SessionChatMessage) {
        if !self.run.is_empty() && message.blocks.iter().any(is_tool_block) {
            self.keep_run();
        }
        self.keep_row(message, true);
    }

    /// `message` ends the open run in the chat's tool fold: a reply, reasoning, tool or background
    /// task's end row takes the run into itself (`takes`, the chat core's `takes_tool_burst_row`),
    /// and after any other row the run is a message of its own.
    fn end_run(&mut self, message: SessionChatMessage, keep: bool, agent: bool, takes: bool) {
        if takes && (self.run_kept || (keep && message.blocks.iter().any(is_tool_block))) {
            self.keep_run();
            self.run_kept = false;
            return self.keep_row(message, agent);
        }
        // A run in view starts after the same row as in the chat.
        let keep = keep || self.run_kept;
        let before = !keep && std::mem::take(&mut self.guard_next);
        if before {
            self.guard = true;
            self.guard_run |= takes;
        }
        let held = self.guard_run;
        let run = self.close_run();
        if keep {
            self.keep_row(message, agent);
            if !self.agent_kept && !run.is_empty() {
                self.agent_folded = run;
            }
        } else {
            self.deferred += 1;
            if before || (held && takes) {
                self.held_at.push(self.kept.len());
                self.held.push(message);
            } else if !self.agent_kept {
                let mut rows = if takes { run } else { Vec::new() };
                rows.push(message);
                self.agent_folded = rows;
            }
        }
    }

    fn keep_row(&mut self, message: SessionChatMessage, agent: bool) {
        if agent {
            self.agent_kept = true;
            self.agent_folded.clear();
        }
        self.kept.push(message);
    }

    /// A row of the open run is in view: the run's rows out of view go into `kept` in their places.
    /// They are tool rows the chat reads before a guarded delivery, none a finished reply.
    fn keep_run(&mut self) {
        self.run_kept = true;
        if self.run.is_empty() {
            return;
        }
        self.drop_guard();
        self.deferred -= self.run.len();
        self.agent_kept = true;
        self.agent_folded.clear();
        for (row, at) in self.run.drain(..).zip(self.run_at.drain(..)).rev() {
            self.kept.insert(at, row);
        }
    }

    /// Ends the open run and hands back its rows out of view (none when it is in view). A run that
    /// holds the row before a guarded delivery goes to `held` instead.
    fn close_run(&mut self) -> Vec<SessionChatMessage> {
        self.run_kept = false;
        if std::mem::take(&mut self.guard_run) {
            self.held.append(&mut self.run);
            self.held_at.append(&mut self.run_at);
        }
        self.run_at.clear();
        std::mem::take(&mut self.run)
    }

    /// `message` comes into view while a delivery the agent did not wake from has the agent's rows
    /// after it in view and the row before it out of view. Unless the chat looks past `message`,
    /// it is the row the chat reads before the delivery (`wakes_settled_agent`): a finished reply
    /// would read as one the delivery woke the agent from, so the row before the delivery comes
    /// into view with its group; any other row reads as the row before does.
    fn check_guard(&mut self, message: &SessionChatMessage, row: &mut Option<Option<ChatMessage>>) {
        let chat = read_once(row, message);
        if chat.is_some_and(passed_over_before_wake) {
            return;
        }
        if !chat.is_some_and(is_settled_reply) {
            return self.drop_guard();
        }
        let held = std::mem::take(&mut self.held);
        let held_at = std::mem::take(&mut self.held_at);
        self.guard = false;
        if std::mem::take(&mut self.guard_run) {
            self.keep_run();
        }
        self.deferred -= held.len();
        // Every held row is newer than the open run's rows, so it goes in before their places.
        self.run_at.iter_mut().for_each(|at| *at += held.len());
        for (row, at) in held.into_iter().zip(held_at).rev() {
            self.kept.insert(at, row);
        }
    }

    fn drop_guard(&mut self) {
        self.guard = false;
        self.guard_run = false;
        self.held.clear();
        self.held_at.clear();
    }

    /// The deliveries woke the agent after the reply before them: that reply stays in view as its
    /// turn's final reply, and the chat must see the agent's work after the newest delivery, which
    /// follows the older ones as well (only rows the chat looks past lie between them). The chat's
    /// tool fold does not pass over such a delivery, so the run open since the newest one ends
    /// there, and a group a run closed among the rows looked past is after it too.
    fn woke(&mut self) {
        self.final_found = false;
        let run = self.close_run();
        let mut pending = std::mem::take(&mut self.pending);
        let newest = pending.remove(0);
        let closed = std::mem::take(&mut self.agent_folded);
        if newest.agent_kept {
            return;
        }
        let folded = [run, closed]
            .into_iter()
            .chain(pending.into_iter().map(|older| older.agent_folded))
            .chain([newest.agent_folded])
            .find(|rows| !rows.is_empty())
            .unwrap_or_default();
        self.deferred = self.deferred.saturating_sub(folded.len());
        self.kept.splice(newest.at..newest.at, folded);
    }

    /// The deliveries did not wake the agent: the chat's tool fold went past them as they were
    /// read, and the agent's rows after them count for an earlier one. Whether one of those rows is
    /// in view, so that the chat must read the row before them as it is (`check_guard`).
    fn not_woken(&mut self) -> bool {
        let pending = std::mem::take(&mut self.pending);
        let after_in_view = pending.iter().any(|delivery| delivery.agent_kept);
        self.agent_kept |= after_in_view;
        if self.agent_folded.is_empty() {
            self.agent_folded = pending
                .into_iter()
                .rev()
                .map(|delivery| delivery.agent_folded)
                .find(|rows| !rows.is_empty())
                .unwrap_or_default();
        }
        after_in_view
    }

    fn finish(self, user: Option<SessionChatMessage>, before: u64) -> Vec<Value> {
        let mut result: Vec<Value> = self.kept.iter().map(|message| json!(message)).collect();
        if let Some(user) = user {
            let mut value = json!(user);
            if self.deferred > 0 {
                value["deferredWork"] = json!({"beforeOffset": before, "startId": user.id,
                    "endId": self.end_id, "completedAt": self.completed_at, "messageCount": self.count, "filePaths": self.files});
                let mut image_paths: Vec<&String> = Vec::new();
                for path in &self.images {
                    if !image_paths.contains(&path) {
                        image_paths.push(path);
                    }
                }
                if !image_paths.is_empty() {
                    value["deferredWork"]["imagePaths"] = json!(image_paths);
                }
            }
            result.push(value);
        }
        result
    }
}

/// A reply holding a question form or a question page: a collapsed turn keeps it, as it keeps an
/// AskUserQuestion call (CDXC:SessionChat 2026-11-02 on gx-chat-core's
/// `transcript::open_questions::asks_questions`).
fn asks_questions(message: &SessionChatMessage) -> bool {
    let fenced = message.blocks.iter().any(|block| {
        matches!(block, SessionChatBlock::Text { text }
            if (text.contains("```") || text.contains("~~~"))
                && (text.contains("questions") || text.contains("visual")))
    });
    fenced
        && serde_json::to_value(message)
            .ok()
            .and_then(|value| {
                serde_json::from_value::<ghostex_gx_protocol::ChatMessage>(value).ok()
            })
            .is_some_and(|message| {
                ghostex_gx_chat_core::transcript::open_questions::asks_questions(&message)
            })
}

fn is_tool_block(block: &SessionChatBlock) -> bool {
    matches!(
        block,
        SessionChatBlock::ToolCall { .. } | SessionChatBlock::ToolResult { .. }
    )
}

/// The chat core's role for `role`.
fn chat_role(role: SessionChatRole) -> ChatRole {
    match role {
        SessionChatRole::User => ChatRole::User,
        SessionChatRole::Assistant => ChatRole::Assistant,
        SessionChatRole::Reasoning => ChatRole::Reasoning,
        SessionChatRole::Tool => ChatRole::Tool,
        SessionChatRole::System => ChatRole::System,
    }
}

/// `message` as the chat core reads it, made the first time a rule reads it.
fn read_once<'a>(
    row: &'a mut Option<Option<ChatMessage>>,
    message: &SessionChatMessage,
) -> Option<&'a ChatMessage> {
    row.get_or_insert_with(|| chat_row(message)).as_ref()
}

/// The row as the chat core reads it.
fn chat_row(message: &SessionChatMessage) -> Option<ChatMessage> {
    serde_json::to_value(message)
        .ok()
        .and_then(|value| serde_json::from_value(value).ok())
}

/// The chat never shows the row (`is_hidden_message` in gx-chat-core): it opens no turn, so it
/// must never be the only owner of work, and it does not end the agent's work after a delivery.
fn hidden_in_chat(message: &SessionChatMessage) -> bool {
    chat_row(message).is_some_and(|row| is_hidden_message(&row))
}

/// Whether the row before an OMP delivery is the agent's finished reply, with the chat core's rule
/// (`wakes_settled_agent`), or `None` for a row that rule looks past.
fn settles_before_delivery(message: &SessionChatMessage) -> Option<bool> {
    let Some(row) = chat_row(message) else {
        return Some(false);
    };
    if passed_over_before_wake(&row) {
        return None;
    }
    Some(is_settled_reply(&row))
}

/// CDXC:SessionChat 2026-09-30 WHY:
/// A collapsed turn keeps the call that sent another agent a message, and the result after it, the way it keeps an answered question: the chat lifts that call out of the folded work as a "Message to" card, so a turn that deferred it would lose the card until opened. The test is loose on purpose; an extra row only stays inside the fold, and the chat core decides which calls are sends.
/// SEE-ALSO: packages/gx-chat-core/src/transcript/sent_message.rs
fn sends_agent_message(name: &str, input: &Value) -> bool {
    if crate::session_chat_interactive::normalize_session_chat_tool_name(name) == "sendmessage" {
        return input.get("message").is_some_and(Value::is_string);
    }
    let mentions = |text: &str| text.contains("agents send") || text.contains("agents create");
    let command = |value: Option<&Value>| match value {
        Some(Value::String(text)) => mentions(text),
        Some(Value::Array(parts)) => parts.iter().filter_map(Value::as_str).any(mentions),
        _ => false,
    };
    match input {
        Value::String(text) => mentions(text),
        _ => ["command", "cmd", "script"]
            .iter()
            .any(|key| command(input.get(*key))),
    }
}

fn collect_file_paths(name: &str, input: &Value, paths: &mut BTreeSet<String>) {
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_lowercase();
    let parsed = input
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok());
    let input = parsed.as_ref().unwrap_or(input);
    if ["write", "edit", "multiedit", "str_replace"].contains(&name.as_str()) {
        for key in [
            "file_path",
            "filePath",
            "path",
            "target_file",
            "targetFile",
            "filename",
        ] {
            if let Some(path) = input
                .get(key)
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
            {
                paths.insert(path.to_string());
                break;
            }
        }
    }
    if name == "apply_patch" {
        if let Some(patch) = input
            .as_str()
            .or_else(|| input.get("patch").and_then(Value::as_str))
        {
            collect_patch_paths(patch, paths);
        }
    }
    if name == "exec" {
        if let Some(source) = input.as_str() {
            for argument in source.split("tools.apply_patch(").skip(1) {
                // Parse one JSON string argument without evaluating JavaScript.
                let mut values =
                    serde_json::Deserializer::from_str(argument.trim_start()).into_iter::<String>();
                if let Some(Ok(patch)) = values.next() {
                    collect_patch_paths(&patch, paths);
                }
            }
        }
    }
}

fn collect_patch_paths(patch: &str, paths: &mut BTreeSet<String>) {
    let mut current = None;
    for line in patch.lines() {
        if let Some(path) = ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        {
            if let Some(previous) = current.replace(path.to_string()) {
                paths.insert(previous);
            }
        } else if let Some(path) = line.strip_prefix("*** Move to: ") {
            current = Some(path.to_string());
        }
    }
    if let Some(path) = current {
        paths.insert(path);
    }
}

#[cfg(test)]
mod read_image_tests {
    use super::*;

    fn row(id: &str, role: &str, blocks: Value) -> SessionChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": role, "blocks": blocks, "timestamp": null, "source": "transcript",
        }))
        .expect("a chat row")
    }

    /// A turn the history keeps folded names the pictures its held-back rows read, oldest first,
    /// leaving out a read that failed (CDXC:SessionChat 2026-11-02 in
    /// packages/gx-chat-core/src/transcript/read_images.rs).
    #[test]
    fn a_folded_turn_names_the_pictures_its_rows_read() {
        let rows = [
            row(
                "c1",
                "assistant",
                json!([{"type": "tool-call", "name": "Read", "callId": "c1",
                "input": {"file_path": "/shots/a.png"}}]),
            ),
            row(
                "r1",
                "tool",
                json!([{"type": "tool-result", "callId": "c1", "output": "ok"}]),
            ),
            row(
                "c2",
                "assistant",
                json!([{"type": "tool-call", "name": "exec", "callId": "c2",
                "input": "await tools.view_image({path:\"/shots/b.jpg\"}); await tools.view_image({path:\"/shots/c.png\"});"}]),
            ),
            row(
                "r2",
                "tool",
                json!([{"type": "tool-result", "callId": "c2", "output": "ok"}]),
            ),
            row(
                "c3",
                "assistant",
                json!([{"type": "tool-call", "name": "Read", "callId": "c3",
                "input": {"file_path": "/shots/gone.png"}}]),
            ),
            row(
                "r3",
                "tool",
                json!([{"type": "tool-result", "callId": "c3", "output": "missing", "isError": true}]),
            ),
            row(
                "c4",
                "assistant",
                json!([{"type": "tool-call", "name": "Read", "callId": "c4",
                "input": {"file_path": "/shots/a.png"}}]),
            ),
            row(
                "r4",
                "tool",
                json!([{"type": "tool-result", "callId": "c4", "output": "ok"}]),
            ),
            row(
                "a1",
                "assistant",
                json!([{"type": "text", "text": "Done."}]),
            ),
        ];
        let mut turn = Turn::default();
        for message in rows.iter().rev() {
            turn.push(message.clone(), false);
        }
        let prompt = row("u1", "user", json!([{"type": "text", "text": "look"}]));
        let rows = turn.finish(Some(prompt), 0);
        let work = &rows.last().expect("the prompt row")["deferredWork"];
        assert_eq!(
            work["imagePaths"],
            json!(["/shots/a.png", "/shots/b.jpg", "/shots/c.png"])
        );
    }
}

#[cfg(test)]
mod question_reply_tests {
    use super::*;

    fn row(id: &str, role: &str, blocks: Value) -> SessionChatMessage {
        serde_json::from_value(json!({
            "id": id, "role": role, "blocks": blocks, "timestamp": null, "source": "transcript",
        }))
        .expect("a chat row")
    }

    /// A turn that asked in a form and then kept working keeps the form's reply out of its
    /// deferred work, so the chat holds the card the ? pill counts.
    #[test]
    fn a_folded_turn_keeps_the_reply_that_asks() {
        let form = "Before I go on:\n\n```questions\n{\"questions\":[{\"tag\":\"Q1\",\"title\":\"Ship it?\",\"choices\":[\"Yes\",\"No\"]}]}\n```";
        let rows = [
            row("f1", "assistant", json!([{"type": "text", "text": form}])),
            row(
                "n1",
                "assistant",
                json!([{"type": "text", "text": "Running the checks."}]),
            ),
            row(
                "c1",
                "assistant",
                json!([{"type": "tool-call", "name": "Bash", "callId": "c1", "input": {"command": "true"}}]),
            ),
            row("a1", "assistant", json!([{"type": "text", "text": "done"}])),
        ];
        let mut turn = Turn::default();
        for message in rows.iter().rev() {
            turn.push(message.clone(), false);
        }
        let prompt = row("u1", "user", json!([{"type": "text", "text": "go"}]));
        let ids: Vec<String> = turn
            .finish(Some(prompt), 0)
            .iter()
            .filter_map(|row| row["id"].as_str().map(str::to_string))
            .collect();
        assert_eq!(
            ids,
            ["a1", "f1", "u1"],
            "the final reply, the form, the prompt"
        );
    }
}

#[cfg(test)]
#[path = "session_chat_history_tests.rs"]
mod omp_wake_tests;
