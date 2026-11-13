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
    read_tail_bytes(path, TRANSCRIPT_TAIL_BYTES)
}

fn read_tail_bytes(path: &str, bytes_wanted: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(bytes_wanted)))
        .ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// How much of a Codex rollout's end is searched for the turn's `turn_context`.
const CODEX_TURN_CONTEXT_TAIL_BYTES: u64 = 4 * 1024 * 1024;

/// CDXC:SessionChat 2026-11-13 WHY:
/// GitHub #597: a coordinator's Codex thread running with Codex's automatic approval reviewer
/// (`approvals_reviewer = "auto_review"`, the "Approve for me" mode) showed "Reviewing approval
/// request" for a minute or two while Codex's reviewer decided, and Ghostex reported it as
/// "waiting for an answer" with an approval card relayed to the user, five times in one review
/// round; the reviewer then approved each one by itself. Codex runs PermissionRequest hooks before
/// its reviewer (codex-rs core/src/tools/approvals.rs `request_approval_with_user_prompt`): the
/// hook fires for every approval, whoever decides it, and its payload does not say who will. The
/// turn's reviewer is in the rollout the hook names, on that turn's `turn_context` record. When it
/// is Codex's reviewer, the hook is no user approval: it stores no card and moves no activity
/// (`ingest_agent_hook_event`). A request the reviewer hands to the user, or one a managed
/// checkpoint keeps for a person, is drawn as Codex's own approval prompt, which the screen reader
/// already turns into a permission notice and relays like any other (session_chat_codex_blocking.rs).
/// Seen live on Codex 0.167.2 (throwaway test server): the card stood the whole review and left
/// when the reviewer's approval ran the command.
pub(crate) fn codex_reviewer_decides(params: &Map<String, Value>, session: &Value) -> bool {
    if event_kind(params) != "permissionrequest"
        || crate::agents::normalize_agent_id(param(params, &["agentName"])).as_deref()
            != Some("codex")
    {
        return false;
    }
    let Some(path) = param(params, &["agentSessionPath"]).or_else(|| {
        session
            .pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str)
    }) else {
        return false;
    };
    read_tail_bytes(path, CODEX_TURN_CONTEXT_TAIL_BYTES)
        .and_then(|tail| codex_turn_reviewer(&tail))
        .is_some_and(|reviewer| matches!(reviewer.as_str(), "auto_review" | "guardian_subagent"))
}

/// The `approvals_reviewer` of the newest `turn_context` record in a Codex rollout's `tail`.
fn codex_turn_reviewer(tail: &str) -> Option<String> {
    tail.lines()
        .rev()
        .filter(|line| line.contains("\"turn_context\""))
        .find_map(|line| {
            let record = serde_json::from_str::<Value>(line).ok()?;
            (record.get("type").and_then(Value::as_str) == Some("turn_context")).then(|| {
                record
                    .pointer("/payload/approvals_reviewer")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_string()
            })
        })
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
