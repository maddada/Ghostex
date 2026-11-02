//! Puts a Claude session back in the user's mode when Claude switches itself into plan mode.
//!
//! CDXC:SessionChat 2026-10-21 DECISION:
//! User: "we shouldn't switch to plan mode ever like that", and when asked how, picked "Switch back automatically". Claude Code's model can call its own `EnterPlanMode` tool, which needs no approval in auto or bypass mode (a customer's "Let's plan this" left him in plan mode); Ghostex lets Claude keep the tool and switches the session back to the mode it was in just before the call, through the same queue and mode walk as the chat's Mode pill. A plan mode the user chose (the pill, `/plan`, Shift+Tab, a launch in plan mode) is never a tool call and stays.
//!
//! CDXC:SessionChat 2026-10-21 WHY:
//! The trigger is Claude's own `PostToolUse` hook, not the transcript: the hook reaches gxserver for every Claude session whether or not a client is watching, names the Ghostex session, and fires only once the call succeeded, whereas gxserver tails a Claude transcript only while a chat is subscribed. The mode to go back to is read from the transcript, the newest `permissionMode` Claude recorded before the call's `tool_use` row.
//! SEE-ALSO: server/src/session_chat_selection_options.rs (`drive_mode_option`), server/src/session_chat_model_selection.rs (`enqueue_unless_pending`).

use std::{
    collections::HashSet,
    io::{Read, Seek, SeekFrom},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use serde_json::{json, Map, Value};

use crate::{
    logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel},
    server::AppState,
};

const ENTER_PLAN_MODE_TOOL: &str = "EnterPlanMode";
/// The newest part of the transcript searched for the call and the mode before it.
const TRANSCRIPT_TAIL_BYTES: u64 = 4 * 1024 * 1024;
/// Claude writes the `tool_use` row before it runs the tool; these cover a write not yet flushed.
const TRANSCRIPT_READ_DELAYS: [Duration; 3] = [
    Duration::from_millis(0),
    Duration::from_millis(500),
    Duration::from_millis(1500),
];

fn handled_calls() -> &'static Mutex<HashSet<String>> {
    static HANDLED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    HANDLED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Answers whether this call is new, remembering it so a repeated hook acts once.
fn first_sighting(tool_use_id: &str) -> bool {
    let Ok(mut handled) = handled_calls().lock() else {
        return false;
    };
    if handled.len() > 512 {
        handled.clear();
    }
    handled.insert(tool_use_id.to_string())
}

/// A change made is routine (the Agent activity diagnostic log); one that could not be made is a
/// warning and always written.
fn log(state: &AppState, level: LogLevel, event: &str, details: Value) {
    let entry = GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    };
    let _ = state
        .logger
        .log_routine(DiagnosticLogScenario::AgentActivity, entry);
}

/// Called with every agent hook gxserver ingests.
pub(crate) fn observe_hook(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    let text = |key: &str| params.get(key).and_then(Value::as_str);
    if endpoint_path != "/api/ingestAgentHookEvent"
        || !text("eventName").is_some_and(|event| event.eq_ignore_ascii_case("PostToolUse"))
        || text("toolName") != Some(ENTER_PLAN_MODE_TOOL)
        || text("agentName") != Some("claude")
        || matches!(
            result.get("reason").and_then(Value::as_str),
            Some("agent-hook-agent-mismatch") | Some("passive-session-identity-conflict")
        )
    {
        return;
    }
    let id = |key: &str| {
        result
            .pointer(&format!("/session/{key}"))
            .and_then(Value::as_str)
            .or_else(|| text(key))
            .map(str::to_string)
    };
    let (Some(project_id), Some(session_id), Some(tool_use_id)) =
        (id("projectId"), id("sessionId"), text("toolUseId"))
    else {
        return;
    };
    let transcript = text("agentSessionPath").map(str::to_string).or_else(|| {
        result.get("session").and_then(|session| {
            crate::server::session_state_sync::read_runtime_text(session, "agentSessionPath")
        })
    });
    observe_finished_call(state, project_id, session_id, tool_use_id, transcript);
}

/// CDXC:SessionChat 2026-11-02 DECISION:
/// User, asked how to close the gaps that keep Claude's hooks installed: "Everything, including Windows". While Track Claude without its hooks is on, the `PostToolUse` hook of a pane the observer follows drives nothing, so the hook-free observer reads the finished `EnterPlanMode` call off the transcript of a busy session (hookless_activity/plan_mode.rs) and calls this instead, with or without a chat open. A call both paths report acts once.
pub(crate) fn observe_finished_call(
    state: &AppState,
    project_id: String,
    session_id: String,
    tool_use_id: &str,
    transcript: Option<String>,
) {
    if !first_sighting(tool_use_id) {
        return;
    }
    let state = state.clone();
    let tool_use_id = tool_use_id.to_string();
    std::thread::spawn(move || {
        restore_mode(
            &state,
            &project_id,
            &session_id,
            &tool_use_id,
            transcript.as_deref(),
        );
    });
}

fn restore_mode(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    tool_use_id: &str,
    transcript: Option<&str>,
) {
    let details = |extra: Value| {
        let mut details = json!({
            "projectId": project_id,
            "sessionId": session_id,
            "toolUseId": tool_use_id,
        });
        if let (Some(details), Some(extra)) = (details.as_object_mut(), extra.as_object()) {
            details.extend(extra.clone());
        }
        details
    };
    let mut before = None;
    if let Some(transcript) = transcript {
        for delay in TRANSCRIPT_READ_DELAYS {
            std::thread::sleep(delay);
            before = mode_before_call(transcript, tool_use_id);
            if before.is_some() {
                break;
            }
        }
    }
    let Some(before) = before else {
        log(
            state,
            LogLevel::Warn,
            "sessionChatPlanModeRestoreSkipped",
            details(
                json!({ "reason": "The transcript did not show the mode before Claude's EnterPlanMode call." }),
            ),
        );
        return;
    };
    if before == "plan" {
        return;
    }
    let mut params = Map::new();
    params.insert("projectId".into(), json!(project_id));
    params.insert("sessionId".into(), json!(session_id));
    params.insert("options".into(), json!({ "mode": before }));
    match crate::session_chat_model_selection::enqueue_unless_pending(state, &params) {
        Ok(answer) if answer.get("queued").and_then(Value::as_bool) == Some(true) => log(
            state,
            LogLevel::Info,
            "sessionChatPlanModeRestoreQueued",
            details(json!({ "mode": before })),
        ),
        Ok(answer) => log(
            state,
            LogLevel::Info,
            "sessionChatPlanModeRestoreSkipped",
            details(json!({ "mode": before, "reason": answer.get("skipped") })),
        ),
        Err(error) => log(
            state,
            LogLevel::Warn,
            "sessionChatPlanModeRestoreSkipped",
            details(json!({ "mode": before, "reason": error.message })),
        ),
    }
}

/// The chat's value (`auto`, `manual`, …) of the newest mode Claude recorded before the call.
fn mode_before_call(transcript: &str, tool_use_id: &str) -> Option<String> {
    let mut file = std::fs::File::open(transcript).ok()?;
    let length = file.metadata().ok()?.len();
    let start = length.saturating_sub(TRANSCRIPT_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut call_seen = false;
    for line in text.lines().rev() {
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        if !call_seen {
            call_seen = calls_tool(&record, tool_use_id);
            continue;
        }
        if let Some(mode) = record.get("permissionMode").and_then(Value::as_str) {
            return crate::session_chat_options::claude_transcript_mode_choice(mode)
                .map(|choice| choice.value);
        }
    }
    None
}

fn calls_tool(record: &Map<String, Value>, tool_use_id: &str) -> bool {
    record.get("type").and_then(Value::as_str) == Some("assistant")
        && record
            .get("message")
            .and_then(|message| message.get("content"))
            .and_then(Value::as_array)
            .is_some_and(|blocks| {
                blocks.iter().any(|block| {
                    block.get("type").and_then(Value::as_str) == Some("tool_use")
                        && block.get("id").and_then(Value::as_str) == Some(tool_use_id)
                })
            })
}
