//! One line each time a session that was working only through background work its agent left
//! running after its turn ended (`backgroundOnlySince`) is taken back to plain working.
//!
//! CDXC:Coordinators 2026-11-18 WHY:
//! A Claude thread (G1ur9, 2026-11-17) ended its turn at 23:39:51Z with "1 shell" in its footer; its Stop listed the shell, so the session stayed working and was marked background only, which the stall watches skip. At 23:51:01Z, with no new row in its transcript and nothing typed into it, the mark was gone and the coordinator's stall watch nudged a finished thread 8 minutes later. Any event that asks for working (a hook such as PreToolUse, PostToolUse or UserPromptSubmit, a terminal-title spinner, a program's status report) rebuilds the stored activity without the mark (session_status/transition.rs), and gxserver keeps no record of which one came, so this line names it: the trigger, the hook event and the working source. It is written only while the Agent activity diagnostic log is on.

use serde_json::{json, Map, Value};

#[cfg(not(test))]
use crate::logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel};

#[cfg(not(test))]
fn logger() -> &'static GxserverLogger {
    static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();
    LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)))
}

pub(super) fn log_background_only_ended(
    session: &Value,
    params: &Map<String, Value>,
    background_only_since: &str,
    working_source: Option<&str>,
) {
    let text = |key: &str| params.get(key).and_then(Value::as_str);
    let details = json!({
        "projectId": session.get("projectId").and_then(Value::as_str),
        "sessionId": session.get("sessionId").and_then(Value::as_str),
        "backgroundOnlySince": background_only_since,
        "activityEvent": text("event"),
        "hookEvent": text("eventName").or_else(|| text("rawEventName")),
        "requestedActivity": text("activity"),
        "programStatusActivity": text("programStatusActivity"),
        "hasTitle": params.contains_key("title") || params.contains_key("rawTitle"),
        "workingSource": working_source,
    });
    #[cfg(not(test))]
    {
        let _ = logger().log_routine(
            DiagnosticLogScenario::AgentActivity,
            GxserverLogInput {
                level: LogLevel::Info,
                event: "agentActivity.backgroundOnlyEnded".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(details),
            },
        );
    }
    #[cfg(test)]
    let _ = details;
}
