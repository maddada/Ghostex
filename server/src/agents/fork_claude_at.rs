//! Fork from a message, for Claude Code: a copy of the conversation cut at that message, under a
//! new id, resumed with `claude --resume <new id>`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::fork_empryo::write_private_file;
use super::*;
use crate::domain::DomainStateError;
use crate::session_chat::{SessionChatRole, SessionChatTranscriptAgent};
use crate::session_chat_decode_claude::{decode_claude_transcript_line, is_noise_message};

/// CDXC:SessionFork 2026-11-12 WHY:
/// Fork from a message (#362). Claude Code's own `--resume-session-at <message uuid>` would do this, but it only takes effect with `--print` (2.1.322: the interactive start never reads it), and `--fork-session` alone copies the whole conversation. A conversation is one `<id>.jsonl` file in its project folder, and `claude --resume <id>` loads any file there, continuing from its last row (tested 2026-11-12: a copy cut after the first of two turns, every mention of the old id rewritten, resumed under its own id and remembered only the first turn). So the fork writes the source's rows up to the chosen message, plus the attachment and system rows right after it (the hook context of a prompt, the stop hook summary of a reply), under a new id, and leaves the source untouched.
pub(crate) fn fork_claude_conversation_at(
    source_session: &Value,
    message_id: &str,
    fork_id: &str,
) -> Result<ClaudeForkCopy, DomainStateError> {
    let runtime = object_field(source_session, "runtimeSettings");
    let source_path = crate::session_chat_paths::resolve_session_chat_transcript_path(
        SessionChatTranscriptAgent::Claude,
        read_text_from_map(&runtime, "agentSessionId").as_deref(),
        read_text_from_map(&runtime, "agentSessionPath").as_deref(),
    )
    .ok_or_else(|| {
        DomainStateError::bad_request(
            "This Claude Code session has no saved conversation to fork yet.",
        )
    })?;
    let source_id = source_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| {
            DomainStateError::bad_request("Claude Code's saved conversation has no id.")
        })?
        .to_string();
    let log = fs::read_to_string(&source_path).map_err(io_error)?;
    // Claude may be appending right now; only whole lines are copied.
    let log = &log[..log.rfind('\n').map_or(0, |end| end + 1)];
    let kept = conversation_through_message(log, message_id, &source_path)?;
    let dir = source_path.parent().ok_or_else(|| {
        DomainStateError::bad_request("Claude Code's saved conversation has no folder.")
    })?;
    let copy = ClaudeForkCopy {
        path: dir.join(format!("{fork_id}.jsonl")),
    };
    write_private_file(&copy.path, kept.replace(&source_id, fork_id).as_bytes())
        .map_err(io_error)?;
    Ok(copy)
}

/// The lines of `log` through the row whose `uuid` is `message_id`, and the attachment and system
/// rows written right after it. Refused for a row that is not a prompt or a reply.
fn conversation_through_message<'a>(
    log: &'a str,
    message_id: &str,
    source_path: &Path,
) -> Result<&'a str, DomainStateError> {
    let mut end = None;
    let mut offset = 0usize;
    for line in log.split_inclusive('\n') {
        let line_offset = offset;
        offset += line.len();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if end.is_none() {
            if row_uuid(trimmed).as_deref() != Some(message_id) {
                continue;
            }
            let fallback_id = format!("{}:{line_offset:012}", source_path.to_string_lossy());
            let forkable =
                decode_claude_transcript_line(trimmed, &fallback_id).is_some_and(|message| {
                    matches!(
                        message.role,
                        SessionChatRole::User | SessionChatRole::Assistant
                    ) && !message.queued
                        && !is_noise_message(&message)
                });
            if !forkable {
                return Err(DomainStateError::bad_request(
                    "Fork from here works on your messages and the agent's replies.",
                ));
            }
            end = Some(offset);
            continue;
        }
        if matches!(row_type(trimmed).as_deref(), Some("attachment" | "system")) {
            end = Some(offset);
            continue;
        }
        break;
    }
    end.map(|end| &log[..end]).ok_or_else(|| {
        DomainStateError::bad_request(
            "That message is not in this conversation's saved history, so it cannot be forked from.",
        )
    })
}

fn row_uuid(line: &str) -> Option<String> {
    if !line.contains("\"uuid\"") {
        return None;
    }
    let record = serde_json::from_str::<Value>(line).ok()?;
    record.get("uuid")?.as_str().map(str::to_string)
}

fn row_type(line: &str) -> Option<String> {
    let record = serde_json::from_str::<Value>(line).ok()?;
    record.get("type")?.as_str().map(str::to_string)
}

pub(crate) struct ClaudeForkCopy {
    path: PathBuf,
}

impl ClaudeForkCopy {
    /// The copy, recorded on the fork's row as its `agentSessionPath`.
    pub(crate) fn session_log(&self) -> &Path {
        &self.path
    }

    /// Removes the copy, for a fork whose session was never created. The fork wrote the file
    /// itself (`create_new`), so nothing else is in it.
    pub(crate) fn discard(self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Claude Code fork failed: {error}"))
}
