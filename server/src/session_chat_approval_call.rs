//! Ties a stored approval card to the tool call that asked, so a call the agent's CLI refused on
//! its own retires the card.
//!
//! CDXC:SessionChat 2026-11-08 WHY:
//! Claude Code's `PermissionRequest` hook names no `tool_use_id`, and a call it refuses by itself (a
//! permission prompt that timed out, a built-in safety check) fires no hook at all: no PostToolUse,
//! no PostToolUseFailure. Its refusal only lands in the transcript, as that call's `tool_result`. The
//! stored card therefore stood until the thread's next Stop while the thread worked on, and a
//! coordinator's Approvals pill kept saying "1 thread waiting for you" for a prompt that was long
//! gone (2026-11-08, a cleanup `rm` timed out and denied; the thread re-ran it and carried on). The
//! PreToolUse that precedes the PermissionRequest carries the call's id, so the card takes it, and
//! the next tool hook of another call retires the card once the transcript holds that call's
//! result.
//! SEE-ALSO: server/src/agents/activity.rs (`next_session_chat_prompt_setting`), server/src/coordinators/relay.rs.

use std::collections::HashMap;
use std::io::{Read as _, Seek as _, SeekFrom};
use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::{Map, Value};

use crate::session_chat_interactive::{
    normalize_session_chat_tool_name, parse_stored_session_chat_prompt, summarize_approval_input,
    SessionChatInteractivePrompt,
};

/// How much of the transcript's end is searched for the call's result.
const TRANSCRIPT_TAIL_BYTES: u64 = 512 * 1024;
/// Sessions whose last tool call is remembered; the oldest go first.
const REMEMBERED_SESSIONS: usize = 512;

#[derive(Clone, Debug, PartialEq)]
struct ToolCall {
    tool: String,
    summary: String,
    id: String,
}

fn last_calls() -> &'static Mutex<HashMap<(String, String), (u64, ToolCall)>> {
    static CALLS: OnceLock<Mutex<HashMap<(String, String), (u64, ToolCall)>>> = OnceLock::new();
    CALLS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn param<'a>(params: &'a Map<String, Value>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| params.get(*key).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn event_kind(params: &Map<String, Value>) -> String {
    param(params, &["eventName", "rawEventName"])
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Remembers the call a PreToolUse event names, for the PermissionRequest that may follow it.
fn remember_call(project_id: &str, session_id: &str, params: &Map<String, Value>) {
    if event_kind(params) != "pretooluse" {
        return;
    }
    let (Some(tool), Some(id)) = (
        param(params, &["toolName", "tool_name"]),
        param(params, &["toolUseId", "tool_use_id"]),
    ) else {
        return;
    };
    let input = params.get("toolInput").or_else(|| params.get("tool_input"));
    let call = ToolCall {
        tool: normalize_session_chat_tool_name(tool),
        summary: summarize_approval_input(input),
        id: id.to_string(),
    };
    let mut calls = last_calls().lock().unwrap_or_else(PoisonError::into_inner);
    let stamp = calls.values().map(|(stamp, _)| *stamp).max().unwrap_or(0) + 1;
    calls.insert(
        (project_id.to_string(), session_id.to_string()),
        (stamp, call),
    );
    if calls.len() > REMEMBERED_SESSIONS {
        if let Some(oldest) = calls
            .iter()
            .min_by_key(|(_, (stamp, _))| *stamp)
            .map(|(key, _)| key.clone())
        {
            calls.remove(&oldest);
        }
    }
}

/// The remembered call's id when it is the one `prompt` asks about.
fn asking_call_id(
    project_id: &str,
    session_id: &str,
    prompt: &SessionChatInteractivePrompt,
) -> Option<String> {
    let SessionChatInteractivePrompt::Approval { tool, summary, .. } = prompt else {
        return None;
    };
    let calls = last_calls().lock().unwrap_or_else(PoisonError::into_inner);
    let (_, call) = calls.get(&(project_id.to_string(), session_id.to_string()))?;
    (call.tool == normalize_session_chat_tool_name(tool)
        && call.summary == summary.clone().unwrap_or_default())
    .then(|| call.id.clone())
}

/// True when `tail` (the end of a JSONL transcript) holds a tool result for the call `id`.
pub fn transcript_has_result(tail: &str, id: &str) -> bool {
    let needle = format!("\"tool_use_id\":\"{id}\"");
    tail.lines()
        .any(|line| line.contains(&needle) && line.contains("\"tool_result\""))
}

fn read_tail(path: &str) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(
        length.saturating_sub(TRANSCRIPT_TAIL_BYTES),
    ))
    .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The stored card after a hook event, given what the usual clear rule left (`after`):
/// an approval the event's PermissionRequest just stored takes the asking call's id, and a stored
/// approval whose call already has its result in the transcript (`transcript_path`) is retired by
/// any tool hook of another call.
pub(crate) fn settle_stored_approval(
    project_id: &str,
    session_id: &str,
    params: &Map<String, Value>,
    after: Option<String>,
    transcript_path: Option<&str>,
) -> Option<String> {
    remember_call(project_id, session_id, params);
    let stored_text = after?;
    let Some(stored) = parse_stored_session_chat_prompt(&stored_text) else {
        return Some(stored_text);
    };
    if !matches!(stored, SessionChatInteractivePrompt::Approval { .. }) {
        return Some(stored_text);
    }
    let Some(id) = stored.tool_use_id().map(str::to_string) else {
        let Some(id) = asking_call_id(project_id, session_id, &stored) else {
            return Some(stored_text);
        };
        return serde_json::to_string(&stored.with_tool_use_id(Some(id)))
            .ok()
            .or(Some(stored_text));
    };
    let event = event_kind(params);
    let other_call = matches!(
        event.as_str(),
        "pretooluse" | "posttooluse" | "posttoolusefailure"
    ) && param(params, &["toolUseId", "tool_use_id"])
        .is_some_and(|event_id| event_id != id);
    if other_call
        && transcript_path
            .and_then(read_tail)
            .is_some_and(|tail| transcript_has_result(&tail, &id))
    {
        return None;
    }
    Some(stored_text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hook(event: &str, id: &str, command: &str) -> Map<String, Value> {
        json!({
            "eventName": event,
            "toolName": "Bash",
            "toolUseId": id,
            "toolInput": { "command": command },
        })
        .as_object()
        .cloned()
        .unwrap()
    }

    fn approval(command: &str) -> String {
        serde_json::to_string(&SessionChatInteractivePrompt::Approval {
            tool: "Bash".to_string(),
            summary: Some(summarize_approval_input(Some(
                &json!({ "command": command }),
            ))),
            tool_use_id: None,
        })
        .unwrap()
    }

    #[test]
    fn a_call_refused_without_a_hook_retires_its_card_at_the_next_tool_hook() {
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("t.jsonl");
        let path = transcript.to_str().unwrap();
        std::fs::write(&transcript, "").unwrap();
        let (project, session) = ("P-approval-call", "S-refused");
        // The PreToolUse names the call; the PermissionRequest that follows stores the card.
        assert!(settle_stored_approval(
            project,
            session,
            &hook("PreToolUse", "toolu_A", "rm -rf $D"),
            None,
            Some(path)
        )
        .is_none());
        let stored = settle_stored_approval(
            project,
            session,
            &hook("PermissionRequest", "", "rm -rf $D"),
            Some(approval("rm -rf $D")),
            Some(path),
        )
        .expect("the card stays");
        let parsed = parse_stored_session_chat_prompt(&stored).unwrap();
        assert_eq!(parsed.tool_use_id(), Some("toolu_A"));
        // Another call's hook while the prompt is still up keeps it.
        let kept = settle_stored_approval(
            project,
            session,
            &hook("PreToolUse", "toolu_sub", "ls"),
            Some(stored.clone()),
            Some(path),
        );
        assert_eq!(kept.as_deref(), Some(stored.as_str()));
        // Claude refused it: only the transcript says so. The next call's hook retires the card.
        std::fs::write(
            &transcript,
            "{\"type\":\"user\",\"message\":{\"content\":[{\"tool_use_id\":\"toolu_A\",\"type\":\"tool_result\",\"is_error\":true}]}}\n",
        )
        .unwrap();
        assert!(settle_stored_approval(
            project,
            session,
            &hook("PreToolUse", "toolu_B", "rm -f /x"),
            Some(stored),
            Some(path),
        )
        .is_none());
    }

    #[test]
    fn a_card_for_another_command_does_not_take_the_last_calls_id() {
        let (project, session) = ("P-approval-call", "S-other");
        settle_stored_approval(
            project,
            session,
            &hook("PreToolUse", "toolu_X", "ls"),
            None,
            None,
        );
        let stored = settle_stored_approval(
            project,
            session,
            &hook("PermissionRequest", "", "rm -rf /tmp/a"),
            Some(approval("rm -rf /tmp/a")),
            None,
        )
        .unwrap();
        assert_eq!(
            parse_stored_session_chat_prompt(&stored)
                .unwrap()
                .tool_use_id(),
            None
        );
    }
}
