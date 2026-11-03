//! The gxserver log line for a ticket command that failed. Only what kind of failure it was, the
//! exit status and sizes: never the command's own output or error text, which can carry a
//! credential or a customer's data and goes only to the person checking the command (Settings,
//! `ghostex issues source`).

use serde_json::{json, Map, Value};

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};

use super::*;

/// A fixed phrase for the log's `error`.
pub(crate) fn failure_log_summary(failure: &CommandFailure) -> &'static str {
    match failure {
        CommandFailure::NotFound(_) => "ticket source command not found",
        CommandFailure::NotExecutable(_) => "ticket source command isn't executable",
        CommandFailure::BatchFile(_) => "ticket source command is a refused batch file",
        CommandFailure::Start(_) => "ticket source command couldn't start",
        CommandFailure::TimedOut { .. } => "ticket source command timed out",
        CommandFailure::TooLarge(_) => "ticket source answer too large",
        CommandFailure::Reported {
            code: ErrorCode::Auth,
            ..
        } => "ticket source authentication failed",
        CommandFailure::Reported {
            code: ErrorCode::RateLimited,
            ..
        } => "ticket source rate limited",
        CommandFailure::Reported {
            code: ErrorCode::NotFound,
            ..
        } => "ticket source reported not found",
        CommandFailure::Reported { .. } => "ticket source reported an error",
        CommandFailure::Exited { .. } => "ticket source command exited",
        CommandFailure::Unreadable { .. } => "ticket source answer unreadable",
    }
}

/// The facts the log keeps: the exit status, a cap or limit, and how much stderr there was.
fn failure_log_facts(failure: &CommandFailure) -> Map<String, Value> {
    let mut facts = Map::new();
    match failure {
        CommandFailure::TimedOut { limit, .. } => {
            facts.insert("limitSeconds".to_string(), json!(limit.as_secs()));
        }
        CommandFailure::TooLarge(cap) => {
            facts.insert("capBytes".to_string(), json!(cap));
        }
        CommandFailure::Exited {
            status,
            stderr_bytes,
            ..
        } => {
            facts.insert("status".to_string(), json!(status));
            facts.insert("stderrBytes".to_string(), json!(stderr_bytes));
        }
        _ => {}
    }
    facts
}

/// One unconditional error line for a failed `info`.
pub(crate) fn log_work_source_failure(logger: &GxserverLogger, check: &CheckFailure) {
    let mut details = failure_log_facts(&check.failure);
    details.insert("verb".to_string(), json!(CommandVerb::Info.word()));
    details.insert("workspaceId".to_string(), json!(check.workspace_id));
    if let Some(source_id) = &check.source_id {
        details.insert("sourceId".to_string(), json!(source_id));
    }
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Error,
        event: "work_source_failed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: Some(check.duration.as_millis()),
        error: Some(failure_log_summary(&check.failure).to_string()),
        details: Some(Value::Object(details)),
    });
}
