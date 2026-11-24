//! The row that marks a turn cut off when gxserver restarted the agent mid-turn.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole, ChatSource};

use crate::state::ChatRow;

/// The row's text; `transcript/noise.rs` draws it like the interrupted-turn marker.
pub const AGENT_RESTARTED_TEXT: &str = "Stopped when the agent restarted";

/// The id prefix of an [`with_agent_restart_rows`] row.
const AGENT_RESTART_ID_PREFIX: &str = "agent-restart:";

/// `transcript` with a "Stopped when the agent restarted" row after the last row written at or
/// before each of `restarted_at` (epoch ms, oldest first).
///
/// CDXC:SessionChat 2026-11-24 WHY: a Native host that dies mid-turn leaves nothing in the agent's own transcript (Claude records the killed tool's "Exit code 137" and stops), so the turn just ended at its tool call. gxserver records each restart it made after such a death (server/src/session_chat_agent_restarts.rs, sent as `agentRestartedAt`) and the row is placed here by time. A restart older than the first row shown belongs to history that is not loaded or was cleared, so it is left out rather than drawn at the top.
pub fn with_agent_restart_rows(transcript: Vec<ChatRow>, restarted_at: &[i64]) -> Vec<ChatRow> {
    if restarted_at.is_empty() {
        return transcript;
    }
    let Some(first_at) = transcript.iter().find_map(|row| row.timestamp) else {
        return transcript;
    };
    let mut marks: Vec<i64> = restarted_at
        .iter()
        .copied()
        .filter(|at| *at >= first_at)
        .collect();
    if marks.is_empty() {
        return transcript;
    }
    marks.sort_unstable();
    marks.dedup();
    let mut out = Vec::with_capacity(transcript.len() + marks.len());
    let mut next = marks.iter().peekable();
    for row in transcript {
        if let Some(time) = row.timestamp {
            while let Some(at) = next.next_if(|at| **at < time) {
                out.push(agent_restart_row(*at));
            }
        }
        out.push(row);
    }
    out.extend(next.map(|at| agent_restart_row(*at)));
    out
}

fn agent_restart_row(at: i64) -> ChatRow {
    std::sync::Arc::new(ChatMessage {
        id: format!("{AGENT_RESTART_ID_PREFIX}{at}"),
        role: ChatRole::System,
        blocks: vec![ChatBlock::Text {
            text: AGENT_RESTARTED_TEXT.to_string(),
        }],
        async_questions: None,
        timestamp: Some(at),
        source: ChatSource::Transcript,
        turn_id: None,
        byte_offset: None,
        queued: false,
        deferred_work: None,
        startup_delivery: None,
        api_error: None,
        mention_paths: None,
        custom_message: None,
        message_number: None,
        message_ref: None,
        held_notes: None,
    })
}
