//! The terminal status machine: the transient status rows, the painted-line streaming bubble, and
//! the pending tool row with its five-second hold.
//!
//! Ported from `applyTerminalActivity` in `packages/shared/session-chat-controller/controller.ts`
//! plus `packages/core-ui/chat/session-chat-terminal-status.ts` and
//! `session-chat-terminal-stream.ts`.
//!
//! CDXC:AgentScreenDetection 2026-09-03:
//! The agent's `⏺` status rows are transient chat history, and the transcript is what retires them.
//! They exist because the same text can take a while to reach the JSONL transcript, so the terminal
//! is the earliest place the chat can read it from. They are only ever a stand-in: the moment the
//! transcript carries the same turn, the transient row must go, or the reader sees the sentence
//! twice. Two structural facts make retirement deterministic without guessing at the agent's
//! wording: a status is published as the message's FIRST PARAGRAPH, so the label is always a prefix
//! of the transcript's text once decoration is normalized away; and the transcript is appended in
//! order, so once it holds a row newer than the sample and the status still has no match, its own
//! row can never arrive.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};
use serde_json::Value;

use crate::session::constants::{TERMINAL_TOOL_HOLD_MS, TIMER_TERMINAL_TOOL_HOLD};
use crate::session::startup_sends::parse_iso_ms;
use crate::session::terminal_text::{
    ends_with_ellipsis, is_completed_tool_summary, is_shell_command_tool_label, joined_text,
    message_text, normalized_tool_text, terminal_status_text,
};
use crate::state::{ChatContext, ChatState, TerminalStream};

const CLAUDE_TERMINAL_STATUS_KIND: &str = "claude-status";
const CLAUDE_TERMINAL_TOOL_KIND: &str = "claude-tool";
const AGENT_TERMINAL_STREAM_KIND: &str = "agent-stream";
const TERMINAL_STATUS_ID_PREFIX: &str = "terminal-status:";
const TERMINAL_TOOL_ID_PREFIX: &str = "terminal-tool:";

/// Newest transcript assistant and system rows a stream is matched against. Wide enough that a
/// tool-heavy turn (one row per call) cannot push the text a stale on-screen block belongs to out
/// of range, which would show it twice.
const STREAM_MATCH_MESSAGE_ROWS: usize = 25;

fn field<'a>(activity: &'a Value, key: &str) -> Option<&'a str> {
    activity.get(key).and_then(Value::as_str)
}

fn kind(activity: &Value) -> &str {
    field(activity, "kind").unwrap_or_default()
}

fn label(activity: &Value) -> &str {
    field(activity, "label").unwrap_or_default()
}

fn detected_at(activity: &Value) -> &str {
    field(activity, "detectedAt").unwrap_or_default()
}

/// `Date.parse(detectedAt)`, falling back to now the way `Number.isNaN(...) ? Date.now() : …` does.
fn stamp(activity: &Value, context: &ChatContext) -> i64 {
    parse_iso_ms(field(activity, "detectedAt")).unwrap_or_else(|| context.now_millis())
}

fn hook_message(id: String, role: ChatRole, blocks: Vec<ChatBlock>, timestamp: i64) -> ChatMessage {
    ChatMessage {
        id,
        role,
        blocks,
        async_questions: None,
        timestamp: Some(timestamp),
        source: ChatSource::Hook,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
    }
}

/// `sessionChatTerminalStatusMessage`: one `claude-status` activity as a transient reasoning row.
pub fn terminal_status_message(activity: &Value, context: &ChatContext) -> Option<ChatMessage> {
    let text = label(activity).trim_matches(crate::session::text::is_js_space);
    if kind(activity) != CLAUDE_TERMINAL_STATUS_KIND || text.is_empty() {
        return None;
    }
    Some(hook_message(
        format!("{TERMINAL_STATUS_ID_PREFIX}{}", detected_at(activity)),
        ChatRole::Reasoning,
        vec![ChatBlock::Text {
            text: text.to_string(),
        }],
        stamp(activity, context),
    ))
}

/// `sessionChatTerminalToolMessage`: the pending tool row.
pub fn terminal_tool_message(activity: &Value, context: &ChatContext) -> Option<ChatMessage> {
    let text = label(activity).trim_matches(crate::session::text::is_js_space);
    if kind(activity) != CLAUDE_TERMINAL_TOOL_KIND
        || text.is_empty()
        || is_completed_tool_summary(text)
    {
        return None;
    }
    let detail = field(activity, "detail")
        .unwrap_or_default()
        .trim_matches(crate::session::text::is_js_space);
    // The painted tool block rides along as a tool-result block, so the text blocks stay the label
    // alone for every comparison below.
    let blocks = if detail.is_empty() {
        vec![ChatBlock::Text {
            text: text.to_string(),
        }]
    } else {
        vec![
            ChatBlock::Text {
                text: text.to_string(),
            },
            ChatBlock::ToolResult {
                output: detail.to_string(),
                is_error: None,
            },
        ]
    };
    Some(hook_message(
        format!("{TERMINAL_TOOL_ID_PREFIX}{}", detected_at(activity)),
        ChatRole::System,
        blocks,
        stamp(activity, context),
    ))
}

/// `sessionChatTerminalStreamFromActivity`.
pub fn terminal_stream_from_activity(
    activity: &Value,
    context: &ChatContext,
) -> Option<TerminalStream> {
    let text = field(activity, "text")
        .unwrap_or_default()
        .trim_matches(crate::session::text::is_js_space);
    if kind(activity) != AGENT_TERMINAL_STREAM_KIND || text.is_empty() {
        return None;
    }
    Some(TerminalStream {
        id: detected_at(activity).to_string(),
        started_at_ms: stamp(activity, context),
        text: text.to_string(),
        key: terminal_status_text(label(activity)),
        live: true,
    })
}

/// `mergeSessionChatTerminalStatus`.
///
/// One row per status, however many probes it took to paint. A later sample that extends an earlier
/// one is the same sentence with more of it visible, so it replaces that row's text in place and
/// keeps the row's id and position; a sample that is a prefix of a row already held brings nothing
/// new.
pub fn merge_terminal_status(current: &[ChatMessage], transient: &ChatMessage) -> Vec<ChatMessage> {
    let text = message_text(transient);
    if text.is_empty() {
        return current.to_vec();
    }
    for index in (0..current.len()).rev() {
        let existing_text = message_text(&current[index]);
        if existing_text.starts_with(&text) {
            return current.to_vec();
        }
        if text.starts_with(&existing_text) {
            let mut next = current.to_vec();
            next[index].blocks = transient.blocks.clone();
            return next;
        }
    }
    let mut next = current.to_vec();
    next.push(transient.clone());
    next
}

/// `withoutSessionChatTerminalStatus`.
///
/// A tool's bullet can be read before its `⎿` gutter has been painted, in which case it arrived as
/// a status. The moment the same row is recognised as a tool, that status row is its twin and goes.
pub fn without_terminal_status(current: &[ChatMessage], tool: &ChatMessage) -> Vec<ChatMessage> {
    let text = message_text(tool);
    current
        .iter()
        .filter(|status| {
            let status_text = message_text(status);
            status_text != text && !text.starts_with(&status_text)
        })
        .cloned()
        .collect()
}

/// `unreconciledSessionChatTerminalStatuses`: the transient rows the transcript has not caught up
/// with yet.
pub fn unreconciled_terminal_statuses(
    statuses: &[ChatMessage],
    transcript: &[ChatMessage],
) -> Vec<ChatMessage> {
    if statuses.is_empty() {
        return Vec::new();
    }
    let mut texts: Vec<String> = Vec::new();
    let mut latest: Option<i64> = None;
    for message in transcript {
        if !matches!(message.source, ChatSource::Transcript) {
            continue;
        }
        let text = message_text(message);
        if !text.is_empty() {
            texts.push(text);
        }
        if let Some(timestamp) = message.timestamp {
            if latest.is_none_or(|current| timestamp > current) {
                latest = Some(timestamp);
            }
        }
    }
    statuses
        .iter()
        .filter(|status| {
            if let (Some(latest), Some(timestamp)) = (latest, status.timestamp) {
                if timestamp < latest {
                    return false;
                }
            }
            let text = message_text(status);
            !texts
                .iter()
                .any(|candidate| *candidate == text || candidate.starts_with(&text))
        })
        .cloned()
        .collect()
}

/// `sameSessionChatTerminalTool`: two samples of the same painted tool row.
pub fn same_terminal_tool(current: &ChatMessage, next: &ChatMessage) -> bool {
    message_text(current) == message_text(next)
}

fn terminal_tool_detail(message: &ChatMessage) -> String {
    message
        .blocks
        .iter()
        .find_map(|block| match block {
            ChatBlock::ToolResult { output, .. } => Some(output.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// `withSessionChatTerminalToolDetail`: the held row with the newer sample's tool block, keeping
/// its identity and position.
pub fn with_terminal_tool_detail(current: &ChatMessage, next: &ChatMessage) -> ChatMessage {
    if terminal_tool_detail(current) == terminal_tool_detail(next) {
        return current.clone();
    }
    let mut merged = current.clone();
    merged.blocks = next.blocks.clone();
    merged
}

/// `sessionChatTerminalStreamIsTool`: the same painted row read as a tool once its gutter appeared.
pub fn terminal_stream_is_tool(stream: &TerminalStream, tool: &ChatMessage) -> bool {
    let tool_text = terminal_status_text(&joined_text(tool, " "));
    !tool_text.is_empty()
        && (tool_text == stream.key
            || (!stream.key.is_empty() && tool_text.starts_with(&stream.key)))
}

/// `matches(painted, saved)` from `sessionChatTerminalToolRetired`.
fn painted_matches(painted: &str, saved: &str) -> bool {
    let preview = normalized_tool_text(painted);
    let full = normalized_tool_text(saved);
    if preview.is_empty() || full.is_empty() {
        return false;
    }
    if preview == full {
        return true;
    }
    if !ends_with_ellipsis(&preview) {
        return false;
    }
    let head = preview
        .trim_end_matches('\u{2026}')
        .trim_end_matches("...")
        .trim_end_matches(crate::session::text::is_js_space);
    full.starts_with(head)
}

/// `sessionChatTerminalToolRetired`: a terminal preview is redundant once its call is recorded or
/// the transcript advances beyond it.
pub fn terminal_tool_retired(tool: &ChatMessage, transcript: &[ChatMessage]) -> bool {
    let label = joined_text(tool, " ");
    let detail = terminal_tool_detail(tool);
    let command = detail
        .split('\n')
        .map(|line| line.trim_matches(crate::session::text::is_js_space))
        .find(|line| line.starts_with("$ "))
        .map(|line| line[2..].to_string());
    for message in transcript.iter().rev() {
        if !matches!(message.source, ChatSource::Transcript) {
            continue;
        }
        if let (Some(row), Some(painted)) = (message.timestamp, tool.timestamp) {
            if row > painted {
                return true;
            }
        }
        if matches!(message.role, ChatRole::User) {
            break;
        }
        let mut has_call = false;
        for block in &message.blocks {
            let ChatBlock::ToolCall { name, input } = block else {
                continue;
            };
            has_call = true;
            let Some(input) = input.as_object() else {
                continue;
            };
            if let Some(description) = input.get("description").and_then(Value::as_str) {
                if painted_matches(&label, description) {
                    return true;
                }
            }
            let saved_command = input
                .get("command")
                .or_else(|| input.get("cmd"))
                .and_then(Value::as_str);
            if let (Some(command), Some(saved)) = (command.as_deref(), saved_command) {
                if painted_matches(command, saved) {
                    return true;
                }
            }
            let argument = input
                .get("file_path")
                .or_else(|| input.get("path"))
                .or_else(|| input.get("pattern"))
                .and_then(Value::as_str);
            if let Some(argument) = argument {
                if painted_matches(&label, &format!("{name}({argument})"))
                    || painted_matches(&label, &format!("{name} {argument}"))
                {
                    return true;
                }
            }
        }
        // Only the newest batch can describe the pending tool. Older identical commands may be
        // earlier attempts in the same turn.
        if !has_call
            && message.blocks.iter().any(|block| match block {
                ChatBlock::Text { text } => !text
                    .trim_matches(crate::session::text::is_js_space)
                    .is_empty(),
                _ => false,
            })
        {
            break;
        }
    }
    false
}

/// `sessionChatTerminalStreamRetired`: true once the transcript makes the stream redundant.
///
/// CDXC:AgentScreenDetection 2026-09-11 WHY:
/// The agent paints informational warnings with the same bullet as assistant prose but saves them
/// as system rows, so assistant-only matching left the terminal preview beside the saved warning.
pub fn terminal_stream_retired(stream: &TerminalStream, transcript: &[ChatMessage]) -> bool {
    let mut message_rows_seen = 0usize;
    let mut latest: Option<i64> = None;
    let mut newest_row = true;
    for message in transcript.iter().rev() {
        if !matches!(message.source, ChatSource::Transcript) {
            continue;
        }
        if newest_row {
            newest_row = false;
            // The agent cannot stream text while a tool executes, so whatever `⏺` row the screen
            // shows then is the tool's own row or an older message; neither is a message being
            // written.
            if matches!(message.role, ChatRole::Assistant)
                && message
                    .blocks
                    .iter()
                    .any(|block| matches!(block, ChatBlock::ToolCall { .. }))
                && !message.blocks.iter().any(|block| match block {
                    ChatBlock::Text { text } => !text
                        .trim_matches(crate::session::text::is_js_space)
                        .is_empty(),
                    _ => false,
                })
            {
                return true;
            }
        }
        if let Some(timestamp) = message.timestamp {
            if latest.is_none_or(|current| timestamp > current) {
                latest = Some(timestamp);
            }
        }
        if matches!(message.role, ChatRole::Assistant | ChatRole::System)
            && message_rows_seen < STREAM_MATCH_MESSAGE_ROWS
        {
            message_rows_seen += 1;
            if !stream.key.is_empty() && message_text(message).contains(&stream.key) {
                return true;
            }
        }
    }
    !stream.live && latest.is_some_and(|latest| latest > stream.started_at_ms)
}

/// `applyTerminalActivity`: where one activity payload lands.
///
/// Exactly one of four places: dropped as a completed-tool summary or a bare shell command, the
/// streaming bubble, the pending tool row, a transient status row, or the working strip's activity.
pub fn apply_terminal_activity(
    state: &mut ChatState,
    activity: Option<&Value>,
    context: &ChatContext,
) {
    if let Some(activity) = activity {
        let summary_text = field(activity, "text").unwrap_or_else(|| label(activity));
        if matches!(
            kind(activity),
            CLAUDE_TERMINAL_TOOL_KIND | CLAUDE_TERMINAL_STATUS_KIND | AGENT_TERMINAL_STREAM_KIND
        ) && (is_completed_tool_summary(summary_text)
            || is_shell_command_tool_label(summary_text))
        {
            clear_tool_hold(state);
            state.pending.terminal_tool = None;
            hold_stream(state);
            state.session.terminal_activity = None;
            return;
        }
    }

    let stream = activity.and_then(|activity| terminal_stream_from_activity(activity, context));
    match &stream {
        Some(stream) => {
            // `setTerminalStream(stream)`: a new object per sample, equal or not.
            state.pending.terminal_stream = Some(stream.clone());
            state.messages.new_composition_identity();
        }
        // The stream is no longer the newest thing on screen: hold it for the transcript to retire.
        None => hold_stream(state),
    }

    let tool = activity.and_then(|activity| terminal_tool_message(activity, context));
    if let Some(tool) = tool {
        clear_tool_hold(state);
        state.pending.terminal_tool = Some(match state.pending.terminal_tool.as_ref() {
            Some(current) if same_terminal_tool(current, &tool) => {
                with_terminal_tool_detail(current, &tool)
            }
            _ => tool.clone(),
        });
        state.pending.terminal_status_messages =
            without_terminal_status(&state.pending.terminal_status_messages, &tool);
        if state
            .pending
            .terminal_stream
            .as_ref()
            .is_some_and(|stream| terminal_stream_is_tool(stream, &tool))
        {
            state.pending.terminal_stream = None;
        }
        state.session.terminal_activity = None;
        return;
    }

    // A newer status replaces the tool immediately. Only a missing sample gets the short hold that
    // bridges the terminal's repaint gaps.
    if activity.is_some() {
        clear_tool_hold(state);
        // The agent can repaint the same tool without its gutter. That is still this tool, not a
        // newer prose status replacing it.
        let keep = match (&stream, state.pending.terminal_tool.as_ref()) {
            (Some(stream), Some(current)) => terminal_stream_is_tool(stream, current),
            _ => false,
        };
        if !keep {
            state.pending.terminal_tool = None;
        }
    } else if state.pending.terminal_tool_hold_until_ms.is_none() {
        state.pending.terminal_tool_hold_until_ms = Some(context.now_ms + TERMINAL_TOOL_HOLD_MS);
        state.core.timers.arm(
            TIMER_TERMINAL_TOOL_HOLD,
            context.now_ms,
            TERMINAL_TOOL_HOLD_MS,
        );
    }

    if stream.is_some() {
        // Rendered in the transcript as the streaming bubble, never in the working strip.
        state.session.terminal_activity = None;
        return;
    }
    let transient = activity.and_then(|activity| terminal_status_message(activity, context));
    let Some(transient) = transient else {
        state.session.terminal_activity = activity.cloned();
        return;
    };
    state.session.terminal_activity = None;
    state.pending.terminal_status_messages =
        merge_terminal_status(&state.pending.terminal_status_messages, &transient);
}

/// `setTerminalStream((current) => (current?.live ? { ...current, live: false } : current))`.
fn hold_stream(state: &mut ChatState) {
    if let Some(stream) = state.pending.terminal_stream.as_mut() {
        if stream.live {
            stream.live = false;
        }
    }
}

/// `clearTerminalToolHold`.
pub fn clear_tool_hold(state: &mut ChatState) {
    state.pending.terminal_tool_hold_until_ms = None;
    state.core.timers.cancel(TIMER_TERMINAL_TOOL_HOLD);
}

/// The pending tool row as the transcript shows it, or `None`.
///
/// `working && !prompt && !error && !retired` is the whole gate: a blocking prompt, a failure, or
/// the end of the turn clears the card immediately, and the five-second hold only ever bridges a
/// missing sample during active work.
pub fn visible_terminal_tool(state: &ChatState, working: bool) -> Option<&ChatMessage> {
    let tool = state.pending.terminal_tool.as_ref()?;
    if !working || state.session.prompt.is_some() || state.session.error.is_some() {
        return None;
    }
    if terminal_tool_retired(tool, &state.messages.list) {
        return None;
    }
    Some(tool)
}

/// The `useEffect` that drops a tool row the transcript has retired, run once per event.
pub fn settle(state: &mut ChatState, working: bool) {
    if state.pending.terminal_tool.is_some() && visible_terminal_tool(state, working).is_none() {
        clear_tool_hold(state);
        state.pending.terminal_tool = None;
    }
}
