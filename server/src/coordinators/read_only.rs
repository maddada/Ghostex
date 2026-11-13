//! Read-only threads: `ghostex coordinator start-thread --read-only` starts the thread's agent in
//! its own restricted mode, and every later launch of that session (resume, a restart, a reload)
//! starts it the same way.
//!
//! CDXC:Coordinators 2026-11-13 DECISION:
//! GitHub #554 ("No read-only threads"): "the reviewer never edits" was only prompt text, and
//! start-thread could not launch an agent in a restricted mode; the user picked "Build the 2 left
//! now". `--read-only` sends `readOnly: true` with the create; gxserver refuses it for an agent
//! with no read-only mode, otherwise keeps `READ_ONLY_THREAD_KEY` on the session's runtime
//! settings, and each launch takes accept-all off the command and adds the mode: Claude's Plan
//! mode (it reads and plans, and asks before it edits or runs anything), Codex's read-only
//! sandbox with approval on request (its Read Only preset). The agent can still ask to leave the
//! mode; that is an approval card for the user, which a coordinator never answers
//! (`relayedFrom`, session_chat_send/answer_http.rs). The session's own mode pill can change it
//! later; that is the user's choice.
//! SEE-ALSO: server/src/side_chats/launch.rs (`without_auto_approve`, the same stripping a side
//! chat's ask-before-edits uses), server/src/agents/launch_plan.rs (first launch),
//! server/src/agents/resume_plan.rs (every later one).

use serde_json::{Map, Value};

use crate::domain::DomainStateError;

/// The create param `start-thread --read-only` sends.
pub const READ_ONLY_THREAD_PARAM: &str = "readOnly";
/// The runtime setting that keeps a session read-only across launches.
pub const READ_ONLY_THREAD_KEY: &str = "readOnlyThread";

/// The flags that start an agent family in its read-only mode, or none when it has none.
fn read_only_flags(family: &str) -> Option<&'static str> {
    match family {
        "claude" | "openclaude" => Some("--permission-mode plan"),
        "codex" => Some("--sandbox read-only --ask-for-approval on-request"),
        _ => None,
    }
}

/// Whether a create asks for a read-only thread.
pub fn read_only_thread_requested(params: &Map<String, Value>) -> bool {
    params.get(READ_ONLY_THREAD_PARAM).and_then(Value::as_bool) == Some(true)
}

/// Whether a session was started as a read-only thread.
pub fn is_read_only_thread(runtime_settings: Option<&Map<String, Value>>) -> bool {
    runtime_settings
        .and_then(|settings| settings.get(READ_ONLY_THREAD_KEY))
        .and_then(Value::as_bool)
        == Some(true)
}

/// Refuses a read-only create for an agent family with no read-only mode.
pub fn check_read_only_thread_family(family: Option<&str>) -> Result<(), DomainStateError> {
    if family.and_then(read_only_flags).is_some() {
        return Ok(());
    }
    Err(DomainStateError::bad_request(format!(
        "{} has no read-only mode Ghostex can start it in. A read-only thread runs on Claude (Plan mode) or Codex (read-only sandbox); pick one of those with --agent, or start it without --read-only.",
        family
            .and_then(crate::agents::default_agent_session_title_name)
            .unwrap_or("This agent")
    )))
}

/// `command` in the family's read-only mode: accept-all taken off, the mode added at the end. An
/// agent family with none keeps its command (a create for one is refused before it launches).
pub fn read_only_thread_command(command: &str, family: Option<&str>) -> String {
    let Some(family) = family else {
        return command.to_string();
    };
    let Some(flags) = read_only_flags(family) else {
        return command.to_string();
    };
    let mut output = crate::side_chats::without_auto_approve(command, family);
    if output.is_empty() {
        return output;
    }
    output.push(' ');
    output.push_str(flags);
    output
}
