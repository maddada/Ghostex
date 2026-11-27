//! Whether a session's agent still has work running that its transcript shows, for Native idle
//! release. Native chats have no terminal footer, so the `backgroundWorkDetectedAt` marker (read
//! from a Terminal-based agent's footer) never covers them; the release sweep asks here instead.
//! BLOCKING.
//!
//! CDXC:SessionSleep 2026-11-27 DECISION: Coordinator follow-up: "a Native chat must never be released while Claude subagents started with run_in_background (or background Bash/Monitor) are still running, or while any agent reports background work we can read ... for OpenCode, Grok, Cursor, Pi and generic agents, add whatever signal each exposes, and where none exists, keep them alive while their last turn has open tool calls". So each agent answers with what its transcript records:
//! - Claude: background shells, monitors and `run_in_background` subagents without their `<task-notification>` (scan.rs).
//! - Codex: commands an `exec` answer left running (codex.rs).
//! - Pi and OMP: background jobs (`details.async.state: running`) without their `async-result` delivery.
//! - Every agent, these included: a tool call in the newest turn that has no result yet, unless that turn was interrupted. OpenCode, Grok, Cursor and the other agents record no background work Ghostex can read, so this is all they have.
//!
//! The 4-hour cap on all of these is the sweep's (`session_native_release::busy_pin_expired`).

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::Path;

use serde_json::Value;

use crate::server::AppState;
use crate::session_chat::{
    resolve_session_chat_transcript_agent, SessionChatBlock, SessionChatMessage, SessionChatRole,
    SessionChatTranscriptAgent, SessionChatTurnLifecycleState,
};
use crate::session_chat_tail::{read_session_chat_tail_page, SessionChatTailPage};

/// How many of the newest messages are read for the newest turn's tool calls.
const NEWEST_TURN_TAIL_LIMIT: usize = 120;

/// A turn the transcript shows running, background work the agent records as still running, or a
/// tool call of the newest turn still waiting for its result.
pub(crate) fn session_has_running_background_work(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
) -> bool {
    let Ok(resolution) = crate::session_chat_read::resolve_session_chat_read_state(
        state, project_id, session_id, None,
    ) else {
        return false;
    };
    // The transcript shows a turn still running, whatever the stored activity says.
    if resolution.working() {
        return true;
    }
    let (transcript_agent, transcript) = resolution.transcript();
    let Some(transcript) = transcript.map(Path::to_path_buf) else {
        return false;
    };
    let agent = crate::session_chat_composer::session_chat_composer_agent_id(session);
    let background = match agent.as_deref() {
        Some("claude") => super::scan::scan_claude_transcript(&transcript)
            .is_ok_and(|facts| facts.has_running_background()),
        Some("codex") => super::codex::scan_rollout(&transcript)
            .is_ok_and(|facts| facts.has_running_background()),
        Some("pi" | "omp") => pi_has_running_job(&transcript),
        _ => false,
    };
    background
        || resolve_session_chat_transcript_agent(transcript_agent.or(agent.as_deref()))
            .is_some_and(|agent| newest_turn_has_open_tool_call(agent, &transcript))
}

/// Whether the newest turn holds a tool call with no result yet. A turn the agent marked
/// interrupted is over, whatever it left unanswered.
fn newest_turn_has_open_tool_call(agent: SessionChatTranscriptAgent, transcript: &Path) -> bool {
    let Ok(SessionChatTailPage::Page {
        messages,
        lifecycle,
        ..
    }) = read_session_chat_tail_page(agent, transcript, NEWEST_TURN_TAIL_LIMIT, None)
    else {
        return false;
    };
    if lifecycle
        .is_some_and(|lifecycle| lifecycle.state == SessionChatTurnLifecycleState::Interrupted)
    {
        return false;
    }
    has_open_tool_call(newest_turn(&messages))
}

/// The messages from the newest prompt on: the last user message with text and no tool result
/// (Claude records tool results as user messages).
fn newest_turn(messages: &[SessionChatMessage]) -> &[SessionChatMessage] {
    let prompt = messages.iter().rposition(|message| {
        message.role == SessionChatRole::User
            && message
                .blocks
                .iter()
                .any(|block| matches!(block, SessionChatBlock::Text { .. }))
            && !message
                .blocks
                .iter()
                .any(|block| matches!(block, SessionChatBlock::ToolResult { .. }))
    });
    prompt.map_or(messages, |at| &messages[at..])
}

/// Calls are matched to results by the agent's call id; calls an agent records without ids are
/// matched by count.
fn has_open_tool_call(messages: &[SessionChatMessage]) -> bool {
    let mut open: HashSet<&str> = HashSet::new();
    let mut answered: HashSet<&str> = HashSet::new();
    let (mut calls_without_id, mut results_without_id) = (0usize, 0usize);
    for block in messages.iter().flat_map(|message| &message.blocks) {
        match block {
            SessionChatBlock::ToolCall { call_id, .. } => match call_id.as_deref() {
                Some(id) => {
                    open.insert(id);
                }
                None => calls_without_id += 1,
            },
            SessionChatBlock::ToolResult { call_id, .. } => match call_id.as_deref() {
                Some(id) => {
                    answered.insert(id);
                }
                None => results_without_id += 1,
            },
            _ => {}
        }
    }
    open.iter().any(|id| !answered.contains(id)) || calls_without_id > results_without_id
}

/// Whether a Pi or OMP session has a background job running: a tool result whose
/// `details.async.state` is `running` names the job, and the `async-result` message that delivers
/// it ends it. A process start does not end them, so a crashed agent's jobs are released by the
/// sweep's 4-hour cap.
fn pi_has_running_job(transcript: &Path) -> bool {
    let Ok(file) = std::fs::File::open(transcript) else {
        return false;
    };
    let mut running: HashSet<String> = HashSet::new();
    for line in BufReader::new(file).split(b'\n').map_while(Result::ok) {
        let text = String::from_utf8_lossy(&line);
        if !text.contains("\"jobId\"") {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        if let Some(job) = record
            .pointer("/message/details/async")
            .filter(|job| job.get("state").and_then(Value::as_str) == Some("running"))
            .and_then(|job| job.get("jobId"))
            .and_then(Value::as_str)
        {
            running.insert(job.to_string());
        }
        // OMP's job delivery (`OMP_ASYNC_RESULT` in gx-chat-core transcript/background_rows.rs).
        if record.get("customType").and_then(Value::as_str) == Some("async-result") {
            for job in record
                .pointer("/details/jobs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if let Some(id) = job.get("jobId").and_then(Value::as_str) {
                    running.remove(id);
                }
            }
        }
    }
    !running.is_empty()
}
