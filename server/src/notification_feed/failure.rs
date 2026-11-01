//! Failed rows: one notification when an agent's run ends on an error Ghostex can positively
//! identify, written from the three places that already know it (an accepted agent hook, the
//! account recovery pass, a program's OSC 7501 report).
//!
//! CDXC:Notifications 2026-10-31 WHY:
//! A failed turn used to settle the session to idle with no notification, so a phone or a
//! glance at the bell could not tell "finished" from "stopped on an error". Only signals that
//! name an error count: a hook's failure event, recovery giving up, a program's `error` state.
//! An Esc, a Stop or an interrupt never does, and a missing or unknown signal is no failure.
//! The body is one fixed sentence, never the agent's error text or transcript, so nothing the
//! agent printed travels into the feed or onto a phone. An error Ghostex's own account recovery
//! retries is announced by recovery instead (once per episode, `failureNotifiedAt` in
//! server/src/accounts/api_errors.rs), so a short outage that the quick retries get past stays
//! quiet.
//! SEE-ALSO: `NOTIFICATION_FEED_KIND_FAILED` in store.rs, `KINDS` in
//! packages/gx-core/src/notification_feed.rs, `GpuiNotificationFeedKind::Failed` in
//! apps/desktop/src/notification_feed/mod.rs, packages/shared/notification-feed/notification-feed-contract.ts.

use chrono::Utc;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::logging::{GxserverLogInput, LogLevel};
use crate::server::AppState;
use crate::storage::open_gxserver_database_with_busy_timeout;

use super::api::broadcast_notification_feed_changed;
use super::producer::{session_row_heading, OBSERVER_BUSY_TIMEOUT};
use super::store::{
    insert_notification_feed_row, notification_agent_icon, NewNotificationFeedRow,
    NOTIFICATION_FEED_KIND_FAILED,
};

/// The body of every failed row.
pub(crate) const NOTIFICATION_FEED_FAILED_BODY: &str = "The agent run ended with an error.";

/// What an accepted hook event says about how the agent's turn ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HookFailure {
    /// An error Ghostex's account recovery takes over when it retries the session: Claude's server
    /// error, or a `StopFailure` from a Claude Code that names no error.
    Recoverable,
    /// An error nothing in Ghostex retries: the run is over.
    Final,
}

fn compact_event_name(params: &Map<String, Value>) -> Option<String> {
    let name = ["eventName", "rawEventName"]
        .iter()
        .find_map(|key| params.get(*key).and_then(Value::as_str))?;
    Some(
        name.trim()
            .to_ascii_lowercase()
            .replace(['_', '-', '.'], ""),
    )
}

/// The failure an accepted hook event reports, or `None` for every other event, an abort
/// included.
///
/// CDXC:Notifications 2026-10-31 WHY:
/// The supported sources, each a signal the agent itself gives only for an error:
/// - Claude, OpenClaude and Empryo `StopFailure` (Claude Code sends it only when a turn ends on an API error; its `error` arrives as `stopError`). `server_error` (or no `error`) is left to account recovery when that will retry the session; `rate_limit` is a usage limit, never a failure; any other named error is final.
/// - Grok and Kimi `StopFailure`, the failure hook of their Claude-style hook sets.
/// - Pi, OMP and Amp `StopFailure`, which Ghostex's own extensions send only for a run that ended with `error` after the agent's own retries (an Esc is `Interrupt`).
/// - Cursor `stop` with status `error` (`aborted` is a Stop).
/// - Copilot `ErrorOccurred` unless it says `recoverable` (the same rule that ends the turn).
/// - Mastra `AgentEnd` with stop reason `error` (`aborted` is a Stop).
/// ZCode's observer sends `StopFailure` for both a failed and a cancelled turn, so it is not a source; Codex sends no error hook (account recovery reads its screen instead).
fn hook_failure(params: &Map<String, Value>) -> Option<HookFailure> {
    let agent = crate::agents::normalize_agent_id(params.get("agentName").and_then(Value::as_str))?;
    let event = compact_event_name(params)?;
    let text = |key: &str| params.get(key).and_then(Value::as_str);
    match (agent.as_str(), event.as_str()) {
        ("claude" | "openclaude" | "empryo", "stopfailure") => match text("stopError") {
            None | Some(crate::accounts::api_errors::CLAUDE_TRANSIENT_ERROR_CODE) => {
                Some(HookFailure::Recoverable)
            }
            Some("rate_limit") => None,
            Some(_) => Some(HookFailure::Final),
        },
        ("grok" | "kimi" | "pi" | "omp" | "amp", "stopfailure") => Some(HookFailure::Final),
        ("cursor", "stop") => (text("stopStatus") == Some("error")).then_some(HookFailure::Final),
        ("copilot", "erroroccurred") => (params.get("recoverable").and_then(Value::as_bool)
            != Some(true))
        .then_some(HookFailure::Final),
        ("mastra", "agentend") => {
            (text("stop_reason") == Some("error")).then_some(HookFailure::Final)
        }
        _ => None,
    }
}

/// Whether the ingest accepted the event for the lead session: it was applied as an activity
/// event and was not a subagent's, a stale one, or another agent's.
fn accepted_for_lead_session(result: &Value) -> bool {
    result.get("activity").is_some_and(Value::is_object)
        && !matches!(
            result.get("reason").and_then(Value::as_str),
            Some(
                "subagent-hook"
                    | "agent-hook-agent-mismatch"
                    | "passive-session-identity-conflict"
                    | "stale-activity-event"
            )
        )
}

/// Ends or announces an error episode for an accepted hook event, after the ingest committed: a
/// prompt the user submitted starts a new run, an event that reports an error is the run's
/// failure. Never fails the request: every error is logged and swallowed.
pub(crate) fn observe_agent_turn(
    state: &AppState,
    endpoint_path: &str,
    params: &Map<String, Value>,
    result: &Value,
) {
    if endpoint_path != "/api/ingestAgentHookEvent" {
        return;
    }
    let user_turn = crate::session_parking::is_user_prompt_submit_hook_event(params);
    let failure = hook_failure(params);
    if (!user_turn && failure.is_none()) || !accepted_for_lead_session(result) {
        return;
    }
    let Some(session) = result.get("session") else {
        return;
    };
    let (Some(project), Some(session_id)) = (
        session.get("projectId").and_then(Value::as_str),
        session.get("sessionId").and_then(Value::as_str),
    ) else {
        return;
    };
    // The hook and account recovery share one marker per error episode, so whichever sees the
    // error first announces it (`announce_failed_turn` in server/src/accounts/api_errors.rs).
    let outcome = open_repository(state).and_then(|(db, server_id)| {
        let repository = DomainRepository::new(&db, server_id);
        if user_turn {
            return crate::accounts::api_errors::end_failure_episode(
                state,
                &repository,
                project,
                session_id,
                params.get("promptDigest").and_then(Value::as_str),
            );
        }
        if failure == Some(HookFailure::Recoverable)
            && crate::accounts::recovery::recovery_retries(&repository, session, false)
        {
            return Ok(());
        }
        crate::accounts::api_errors::announce_failed_turn(state, &repository, project, session_id)
    });
    if let Err(error) = outcome {
        log_failed_notice(state, session, error);
    }
}

/// Writes a failed row for a program's effective OSC 7501 `error` report, which already put the
/// session in finished attention under `attentionEventId`. Called once per new report: the
/// caller skips a report it applies again.
pub(crate) fn observe_program_error(state: &AppState, result: &Value) {
    let Some(session) = result.get("session") else {
        return;
    };
    let attention_event_id = result
        .pointer("/activity/attentionEventId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let outcome = open_repository(state).and_then(|(db, server_id)| {
        record_failed_row(
            &DomainRepository::new(&db, server_id),
            session,
            attention_event_id,
        )
        .map(|_| true)
    });
    finish(state, session, outcome);
}

/// Stores `members` of `runtime` for the session together with its episode's failed row in one
/// transaction on the caller's connection, with `accountRecovery`'s `failureNotifiedAt` set to now:
/// the row and the marker are committed together or not at all. Only `members` are written, onto
/// the row as read after the writer is reserved, so newer runtime members another writer stored
/// meanwhile stay. When that fails (logged), `members` of `unannounced` are stored instead, if
/// given, so the episode still owes its row. `Ok(true)` when the row was committed; the caller
/// announces the feed change.
pub(crate) fn store_failure_notice(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    mut runtime: Map<String, Value>,
    unannounced: Option<Map<String, Value>>,
    members: &[&str],
) -> Result<bool, DomainStateError> {
    // A caller already inside a transaction keeps it; the savepoint makes this one unit inside it.
    let (begin, commit, rollback) = if repository.db.is_autocommit() {
        ("BEGIN IMMEDIATE", "COMMIT", "ROLLBACK")
    } else {
        (
            "SAVEPOINT failure_notice",
            "RELEASE failure_notice",
            "ROLLBACK TO failure_notice; RELEASE failure_notice",
        )
    };
    let sql = |error: rusqlite::Error| DomainStateError {
        code: "internalError",
        message: format!("SQLite gxserver state error: {error}"),
    };
    let stored = repository
        .db
        .execute_batch(begin)
        .map_err(sql)
        .and_then(|()| {
            let written = record_failed_row(repository, session, None)
                .and_then(|_| {
                    if let Some(recovery) = runtime.get_mut("accountRecovery") {
                        recovery[crate::accounts::api_errors::FAILURE_NOTIFIED_AT] =
                            json!(Utc::now().to_rfc3339());
                    }
                    crate::accounts::endpoint::update_session_members(
                        repository, session, &runtime, members,
                    )
                })
                .and_then(|_| repository.db.execute_batch(commit).map_err(sql));
            if written.is_err() {
                let _ = repository.db.execute_batch(rollback);
            }
            written
        });
    match stored {
        Ok(()) => Ok(true),
        Err(error) => {
            log_failed_notice(state, session, error);
            if let Some(runtime) = unannounced {
                crate::accounts::endpoint::update_session_members(
                    repository, session, &runtime, members,
                )?;
            }
            Ok(false)
        }
    }
}

/// Logs a failed row or episode marker that could not be stored.
pub(crate) fn log_failed_notice(state: &AppState, session: &Value, error: DomainStateError) {
    finish(state, session, Err(error));
}

fn open_repository(
    state: &AppState,
) -> Result<(crate::storage::GxserverDatabase, &str), DomainStateError> {
    open_gxserver_database_with_busy_timeout(&state.paths, OBSERVER_BUSY_TIMEOUT)
        .map(|db| (db, state.metadata.server_id.as_str()))
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })
}

fn record_failed_row(
    repository: &DomainRepository<'_>,
    session: &Value,
    attention_event_id: Option<String>,
) -> Result<Value, DomainStateError> {
    let text = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| DomainStateError::bad_request(format!("The session has no {key}.")))
    };
    let (project_id, session_id) = (text("projectId")?, text("sessionId")?);
    let (title, subtitle) = session_row_heading(repository, project_id, session)?;
    insert_notification_feed_row(
        repository.db,
        NewNotificationFeedRow {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            kind: NOTIFICATION_FEED_KIND_FAILED,
            title,
            subtitle,
            body: NOTIFICATION_FEED_FAILED_BODY.to_string(),
            agent_name: notification_agent_icon(session),
            attention_event_id,
        },
    )
}

fn finish(state: &AppState, session: &Value, outcome: Result<bool, DomainStateError>) {
    match outcome {
        Ok(true) => broadcast_notification_feed_changed(state),
        Ok(false) => {}
        Err(error) => {
            let _ = state.logger.log(GxserverLogInput {
                level: LogLevel::Warn,
                event: "notificationFeed.failedRowFailed".to_string(),
                server_id: Some(state.metadata.server_id.clone()),
                request_id: None,
                client: None,
                duration_ms: None,
                error: Some(error.message.clone()),
                details: Some(json!({
                    "sessionId": session.get("sessionId").and_then(Value::as_str),
                })),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(value: Value) -> Map<String, Value> {
        value.as_object().cloned().expect("params")
    }

    #[test]
    fn only_events_that_name_an_error_are_failures() {
        let failure = |value: Value| hook_failure(&params(value));
        assert_eq!(
            failure(
                json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "authentication_failed" })
            ),
            Some(HookFailure::Final)
        );
        assert_eq!(
            failure(
                json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "server_error" })
            ),
            Some(HookFailure::Recoverable)
        );
        // A Claude Code that names no error is left to recovery, never called final.
        assert_eq!(
            failure(json!({ "agentName": "claude", "eventName": "StopFailure" })),
            Some(HookFailure::Recoverable)
        );
        // A usage limit waits for its reset: never a failure.
        assert_eq!(
            failure(
                json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "rate_limit" })
            ),
            None
        );
        for (agent, event) in [
            ("omp", "StopFailure"),
            ("pi", "StopFailure"),
            ("amp", "StopFailure"),
        ] {
            assert_eq!(
                failure(json!({ "agentName": agent, "eventName": event })),
                Some(HookFailure::Final),
                "{agent}"
            );
        }
        assert_eq!(
            failure(json!({ "agentName": "cursor", "eventName": "stop", "stopStatus": "error" })),
            Some(HookFailure::Final)
        );
        assert_eq!(
            failure(
                json!({ "agentName": "copilot", "eventName": "ErrorOccurred", "recoverable": false })
            ),
            Some(HookFailure::Final)
        );
        assert_eq!(
            failure(
                json!({ "agentName": "mastra", "eventName": "AgentEnd", "stop_reason": "error" })
            ),
            Some(HookFailure::Final)
        );
    }

    #[test]
    fn aborts_finished_turns_and_unproven_signals_are_not_failures() {
        let failure = |value: Value| hook_failure(&params(value));
        for event in [
            json!({ "agentName": "cursor", "eventName": "stop", "stopStatus": "aborted" }),
            json!({ "agentName": "cursor", "eventName": "stop", "stopStatus": "completed" }),
            json!({ "agentName": "codex", "eventName": "interrupt" }),
            json!({ "agentName": "omp", "eventName": "Interrupt" }),
            json!({ "agentName": "claude", "eventName": "Stop" }),
            json!({ "agentName": "mastra", "eventName": "AgentEnd", "stop_reason": "aborted" }),
            json!({ "agentName": "copilot", "eventName": "ErrorOccurred", "recoverable": true }),
            // ZCode's observer sends StopFailure for a cancelled turn too.
            json!({ "agentName": "zcode", "eventName": "StopFailure" }),
            json!({ "agentName": "claude", "eventName": "escape" }),
        ] {
            assert_eq!(failure(event.clone()), None, "{event}");
        }
    }

    #[test]
    fn a_rejected_or_subagent_event_is_not_the_lead_sessions_failure() {
        let applied = json!({ "activity": { "activity": "idle" }, "reason": "activity-updated" });
        assert!(accepted_for_lead_session(&applied));
        for reason in [
            "agent-hook-agent-mismatch",
            "passive-session-identity-conflict",
        ] {
            assert!(!accepted_for_lead_session(
                &json!({ "activity": { "activity": "idle" }, "reason": reason })
            ));
        }
        // A subagent's hook and a stale event carry no applied activity.
        assert!(!accepted_for_lead_session(
            &json!({ "reason": "subagent-hook" })
        ));
        assert!(!accepted_for_lead_session(
            &json!({ "reason": "stale-activity-event" })
        ));
    }
}
