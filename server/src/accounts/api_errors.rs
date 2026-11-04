//! Automatic retry after an agent's API call failed on a connection or server error: how the
//! failure is found, the quick tries before the slow schedule, the wait for the network, and the
//! `apiRetry` state the chat draws its error card from.
//!
//! CDXC:AgentProviders 2026-10-15 WHY:
//! Recovery used to start only when the screen showed a known error line, and Claude's current wordings ("Can't reach the API server … (ENOTFOUND)", "Connection refused … (ECONNREFUSED)", "Connection lost mid-response") were not in that list, so three threads that died together when the network dropped were never retried. Claude records every such failure as a synthetic `isApiErrorMessage` row with `error: "server_error"` (logins, limits, refusals and bad models carry other codes), so for Claude the transcript is the trigger and the screen list is only the fallback; Codex keeps no record of its errors, so it still relies on the screen.
//! SEE-ALSO: recovery.rs (plan_session, the 2026-10-15 schedule DECISION), session_chat_decode_claude.rs (`apiError` on the row), packages/gx-chat-core/src/transcript/api_errors.rs (the card), coordinators/state.rs (a thread stays working while a quick try is pending).

use super::model::Provider;
use crate::{
    server::AppState,
    session_chat_notice::{
        SessionChatTerminalNotice, SessionChatTerminalNoticeSeverity,
        SessionChatTerminalNoticeSource, SESSION_CHAT_NOTICE_STREAM_ERROR,
    },
};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    net::{TcpStream, ToSocketAddrs},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

/// The code Claude records on a failed call another try can get past.
pub(crate) const CLAUDE_TRANSIENT_ERROR_CODE: &str = "server_error";
/// Quick tries before the slow schedule takes over.
pub(crate) const QUICK_TRIES: u64 = 5;
/// The wait before each quick try, in seconds.
const QUICK_DELAYS_SECS: [i64; QUICK_TRIES as usize] = [5, 15, 60, 120, 300];
/// How often a session waiting for the network looks again.
pub(crate) const NETWORK_RECHECK: Duration = Duration::from_secs(5);
/// How often a quick try held back by the agent's screen looks again.
pub(crate) const BLOCKED_RECHECK: Duration = Duration::from_secs(5);
/// How long the screen may hold a quick try back before its orchestrator hears about the thread.
const BLOCKED_GRACE_SECS: i64 = 60;
/// How long one reachability answer stands, so a pass over many sessions probes once.
const PROBE_TTL: Duration = Duration::from_secs(3);
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether a recovery is about a connection or server error (the only trigger with quick tries).
pub(crate) fn is_connection_trigger(recovery: &Value) -> bool {
    recovery["trigger"].as_str() == Some(SESSION_CHAT_NOTICE_STREAM_ERROR)
}

/// The wait before the next try after `attempt` tries, for a connection or server error.
pub(crate) fn error_backoff(attempt: u64) -> chrono::Duration {
    match QUICK_DELAYS_SECS.get(attempt as usize) {
        Some(seconds) => chrono::Duration::seconds(*seconds),
        None => super::recovery::backoff(attempt - QUICK_TRIES),
    }
}

/// A failed call the agent recorded that nothing has answered yet.
#[derive(Clone)]
pub(crate) struct OpenFailure {
    pub text: String,
    /// When the API last really answered (a reply that is not an error row), epoch ms.
    pub replied_at_ms: Option<i64>,
}

impl OpenFailure {
    /// The failure as the notice the recovery pass already understands.
    pub(crate) fn notice(&self) -> SessionChatTerminalNotice {
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_STREAM_ERROR,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            "Claude Code hit a temporary service error",
        )
        .with_detail(self.text.clone())
    }
}

fn record_text(record: &Value) -> String {
    match &record["message"]["content"] {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|block| block["type"] == "text")
            .filter_map(|block| block["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// Whether a Claude transcript row is a turn of the conversation: the user's prompt or anything
/// the agent wrote. Tool results, meta rows and bookkeeping rows are not.
fn conversation_row(record: &Value) -> bool {
    match record["type"].as_str() {
        Some("assistant") => true,
        Some("user") => {
            record["isMeta"] != true
                && match &record["message"]["content"] {
                    Value::String(text) => !text.trim().is_empty(),
                    Value::Array(blocks) => blocks.iter().any(|block| block["type"] == "text"),
                    _ => false,
                }
        }
        _ => false,
    }
}

/// Whether a Claude transcript row is the agent recording a connection or server failure.
pub(crate) fn claude_transient_failure(record: &Value) -> bool {
    record["type"] == "assistant"
        && record["isApiErrorMessage"] == true
        && record["error"].as_str() == Some(CLAUDE_TRANSIENT_ERROR_CODE)
}

/// The newest failure in a Claude transcript tail, when it is still the newest turn.
pub(crate) fn claude_open_failure_in(text: &str) -> Option<OpenFailure> {
    let rows: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| record["isSidechain"] != true && conversation_row(record))
        .collect();
    let last = rows.last()?;
    let replied_at_ms = rows
        .iter()
        .filter(|record| record["type"] == "assistant" && record["isApiErrorMessage"] != true)
        .filter_map(|record| crate::session_chat::parse_timestamp(record.get("timestamp")))
        .max();
    claude_transient_failure(last).then(|| OpenFailure {
        text: record_text(last).trim().to_string(),
        replied_at_ms,
    })
}

/// BLOCKING (a bounded transcript tail read). The Claude session's open failure, if any.
pub(crate) fn claude_open_failure(session: &Value) -> Option<OpenFailure> {
    let agent = crate::session_chat::resolve_session_chat_transcript_agent(Some("claude"))?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str),
        session
            .pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str),
    )?;
    // Every 30 seconds per running Claude session (recovery.rs); kept while the transcript is unchanged.
    static MEMO: std::sync::LazyLock<crate::file_memo::FileMemo<Option<OpenFailure>>> =
        std::sync::LazyLock::new(crate::file_memo::FileMemo::new);
    MEMO.get_or_read(&path, || {
        let text = crate::session_chat_options::transcript_tail_text(&path).ok()?;
        claude_open_failure_in(&text)
    })
}

/// What on the agent's screen keeps a try from reaching it, in the words the chat already shows
/// for it (the notice's title, the input box's reason), or `None` when the input is free. A screen
/// nothing can be concluded about counts as free, the way the send path fails open; a Claude panel
/// the send closes with Escape is not a block either.
pub(crate) fn screen_block(
    detection: &crate::session_chat_options::SessionChatTerminalDetection,
    session: &Value,
) -> Option<String> {
    if detection.prompt.is_some()
        || crate::session_chat_send::transcript_pending_question_prompt(session).is_some()
    {
        return Some("The agent is waiting for an answer to its question".to_string());
    }
    let dismissible = detection.composer.should_dismiss();
    if let Some(notice) = detection.notice.as_ref().filter(|notice| {
        (notice.is_answerable() || notice.blocks_input())
            && !(dismissible
                && notice.kind
                    == crate::session_chat_notice::SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED)
    }) {
        return Some(notice.title.clone());
    }
    (detection.composer.state == crate::session_chat_composer::SessionChatComposerState::NotReady
        && !dismissible)
        .then(|| {
            detection
                .composer
                .reason
                .clone()
                .filter(|reason| !reason.trim().is_empty())
                .unwrap_or_else(|| "The agent's input box is not showing".to_string())
        })
}

/// A try that never reached the agent (the screen blocked it) gives its quick try back: the count,
/// the time of the last real try, and what blocked it, so the card stays true and the next look
/// comes a few seconds later.
pub(crate) fn give_back_blocked_try(recovery: &mut Value, blocked_by: &str, now: DateTime<Utc>) {
    let attempt = recovery["attempt"].as_u64().unwrap_or(0);
    recovery["attempt"] = json!(attempt.saturating_sub(1));
    recovery["lastAttemptAt"] = recovery["previousAttemptAt"].clone();
    mark_blocked(recovery, blocked_by, now);
    recovery["status"] = json!("waiting");
    recovery["nextAttemptAt"] =
        json!((now + chrono::Duration::from_std(BLOCKED_RECHECK).unwrap_or_default()).to_rfc3339());
}

/// Records what holds the next quick try back, keeping when the hold began.
pub(crate) fn mark_blocked(recovery: &mut Value, blocked_by: &str, now: DateTime<Utc>) {
    if recovery["blockedSince"].is_null() {
        recovery["blockedSince"] = json!(now.to_rfc3339());
    }
    recovery["blockedBy"] = json!(blocked_by);
    recovery["updatedAt"] = json!(now.to_rfc3339());
}

/// Clears the hold once the screen lets a try through.
pub(crate) fn clear_blocked(recovery: &mut Value) {
    if let Some(fields) = recovery.as_object_mut() {
        fields.remove("blockedBy");
        fields.remove("blockedSince");
    }
}

fn api_host(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "api.anthropic.com",
        Provider::Codex => "chatgpt.com",
    }
}

/// BLOCKING (a DNS lookup and a TCP connect, a few seconds at most). Whether the provider's API
/// host answers on its HTTPS port: cheap, and enough to tell "the network is back" from "still
/// down" without spending a try.
pub(crate) fn api_reachable(provider: Provider) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, (Instant, bool)>>> = OnceLock::new();
    let host = api_host(provider);
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, reachable)) = cache.lock().ok().and_then(|c| c.get(host).copied()) {
        if at.elapsed() < PROBE_TTL {
            return reachable;
        }
    }
    let reachable = (host, 443).to_socket_addrs().is_ok_and(|addresses| {
        addresses
            .take(2)
            .any(|address| TcpStream::connect_timeout(&address, PROBE_TIMEOUT).is_ok())
    });
    if let Ok(mut cache) = cache.lock() {
        cache.insert(host, (Instant::now(), reachable));
    }
    reachable
}

/// Runs the recovery pass for one session at `at`, so a quick try does not wait for the 30-second
/// sweep. Several wake-ups for one session are harmless: the pass reads the stored schedule.
pub(crate) fn wake_at(state: &AppState, project: &str, session: &str, at: DateTime<Utc>) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let delay = (at - Utc::now()).to_std().unwrap_or_default();
    let state = state.clone();
    let (project, session) = (project.to_string(), session.to_string());
    handle.spawn(async move {
        tokio::time::sleep(delay).await;
        state.accounts.recovery.wake(&project, &session);
    });
}

/// How each session's latest Claude turn ended, as its hooks said: when, and whether it finished
/// (`Stop`) rather than failing on a call (`StopFailure`). In memory only.
fn turn_ends() -> &'static Mutex<HashMap<(String, String), (DateTime<Utc>, bool)>> {
    static TURN_ENDS: OnceLock<Mutex<HashMap<(String, String), (DateTime<Utc>, bool)>>> =
        OnceLock::new();
    TURN_ENDS.get_or_init(Default::default)
}

/// A Claude turn that ended on a failed call (`StopFailure`) wakes the recovery pass at once, and
/// every Claude turn end is remembered for `last_try_answered`.
pub(crate) fn observe_agent_hook(state: &AppState, endpoint: &str, params: &Map<String, Value>) {
    if endpoint != "/api/ingestAgentHookEvent"
        || !matches!(
            params.get("agentName").and_then(Value::as_str),
            Some("claude" | "openclaude")
        )
    {
        return;
    }
    let named = |wanted: &str| {
        ["eventName", "rawEventName", "event"].iter().any(|key| {
            params
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(wanted))
        })
    };
    let (failed, finished) = (named("StopFailure"), named("Stop"));
    let (Some(project), Some(session)) = (
        params.get("projectId").and_then(Value::as_str),
        params.get("sessionId").and_then(Value::as_str),
    ) else {
        return;
    };
    if failed || finished {
        if let Ok(mut ends) = turn_ends().lock() {
            ends.insert(
                (project.to_string(), session.to_string()),
                (Utc::now(), finished),
            );
        }
    }
    if failed {
        state.accounts.recovery.wake(project, session);
    }
}

/// CDXC:AgentProviders 2026-10-16 WHY:
/// A continuation is sent once per failure. When the session's hooks say the turn after the last try finished (Claude's `Stop`, not `StopFailure`), that try worked, whatever the transcript or the screen still shows: a transcript gxserver was not following (throwaway G8yfk on 2026-10-15) or an old error line left on screen made the pass send a second continuation after the first had been answered.
pub(crate) fn last_try_answered(
    project: &str,
    session: &str,
    last_try: Option<DateTime<Utc>>,
) -> bool {
    let Some(last_try) = last_try else {
        return false;
    };
    turn_ends()
        .lock()
        .ok()
        .and_then(|ends| {
            ends.get(&(project.to_string(), session.to_string()))
                .copied()
        })
        .is_some_and(|(at, finished)| finished && at > last_try)
}

fn millis(value: &Value) -> Value {
    value
        .as_str()
        .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
        .map_or(Value::Null, |at| json!(at.timestamp_millis()))
}

/// The `apiRetry` state frames carry: the retry for a connection or server error, `stopped` after
/// Stop retrying, or `None`.
pub(crate) fn chat_state(runtime: &Value) -> Option<Value> {
    if runtime["accountRecoverySuppressed"] == true {
        return Some(json!({ "status": "stopped" }));
    }
    let recovery = &runtime["accountRecovery"];
    if !is_connection_trigger(recovery) {
        return None;
    }
    Some(json!({
        "status": recovery["status"],
        "attempt": recovery["attempt"].as_u64().unwrap_or(0),
        "quickTries": QUICK_TRIES,
        "nextAttemptAt": millis(&recovery["nextAttemptAt"]),
        "lastAttemptAt": millis(&recovery["lastAttemptAt"]),
        "waitingForNetwork": recovery["waitingForNetwork"] == true,
        "blockedBy": recovery["blockedBy"],
        "reason": recovery["reason"],
        "errorText": recovery["errorText"],
    }))
}

/// Whether a quick try is still ahead for this session, so its orchestrator does not hear that
/// the thread finished its turn while Ghostex is about to resume it.
pub(crate) fn quick_retry_pending(session: &Value) -> bool {
    let runtime = &session["runtimeSettings"];
    let recovery = &runtime["accountRecovery"];
    let blocked_too_long = recovery["blockedSince"]
        .as_str()
        .and_then(|since| DateTime::parse_from_rfc3339(since).ok())
        .is_some_and(|since| {
            (Utc::now() - since.with_timezone(&Utc)).num_seconds() >= BLOCKED_GRACE_SECS
        });
    runtime["accountRecoverySuppressed"] != true
        && is_connection_trigger(recovery)
        && matches!(recovery["status"].as_str(), Some("waiting" | "retrying"))
        && recovery["attempt"].as_u64().unwrap_or(0) < QUICK_TRIES
        // A screen that keeps holding the tries back needs someone, so the thread is reported
        // the way its screen says (waiting) instead of looking busy forever.
        && !blocked_too_long
}

/// What a thread report adds when the quick tries gave up and the slow schedule is running.
pub(crate) fn slow_retry_note(session: &Value) -> Option<String> {
    let runtime = &session["runtimeSettings"];
    let recovery = &runtime["accountRecovery"];
    if runtime["accountRecoverySuppressed"] == true
        || !is_connection_trigger(recovery)
        || recovery["status"].as_str() != Some("waiting")
    {
        return None;
    }
    let next = recovery["nextAttemptAt"]
        .as_str()
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| {
            format!(
                " (next try at {} UTC)",
                at.with_timezone(&Utc).format("%H:%M")
            )
        })
        .unwrap_or_default();
    Some(format!(
        "Its turn ended on an API connection or server error and {QUICK_TRIES} quick retries failed. Ghostex keeps retrying it on a slower schedule{next} and the thread resumes by itself when one works; `ghostex agents send` reaches it as usual."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three rows the G4uul, G8biy and G639m threads ended on when the network dropped on
    /// 2026-10-15, copied from their transcripts with the bookkeeping fields trimmed.
    const FAILED_ROWS: [&str; 3] = [
        r#"{"parentUuid":"9b71b792","isSidechain":false,"type":"assistant","uuid":"43be72a9","timestamp":"2026-10-15T13:45:57.944Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","type":"message","content":[{"type":"text","text":"API Error: Connection lost mid-response. The response above may be incomplete."}]},"error":"server_error","truncatedAfterOutput":true,"isApiErrorMessage":true}"#,
        r#"{"parentUuid":"c3831a22","isSidechain":false,"type":"assistant","uuid":"1690bf9b","timestamp":"2026-10-15T13:48:23.764Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","type":"message","content":[{"type":"text","text":"API Error: Connection refused — a firewall or proxy may be blocking it (ECONNREFUSED)"}]},"error":"server_error","isApiErrorMessage":true}"#,
        r#"{"parentUuid":"da772396","isSidechain":false,"type":"assistant","uuid":"5cb5932a","timestamp":"2026-10-15T13:49:05.180Z","message":{"model":"<synthetic>","role":"assistant","stop_reason":"stop_sequence","type":"message","content":[{"type":"text","text":"API Error: Can't reach the API server — check your internet or DNS (ENOTFOUND)"}]},"error":"server_error","isApiErrorMessage":true}"#,
    ];
    const PROMPT: &str =
        r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":"keep going"}}"#;
    const TOOL_RESULT: &str = r#"{"isSidechain":false,"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#;
    const REPLY: &str = r#"{"isSidechain":false,"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"Done."}]}}"#;
    const LIMIT: &str = r#"{"isSidechain":false,"type":"assistant","message":{"model":"<synthetic>","content":[{"type":"text","text":"You've hit your session limit · resets 9:30pm"}]},"error":"rate_limit","isApiErrorMessage":true}"#;
    const LOGIN: &str = r#"{"isSidechain":false,"type":"assistant","message":{"model":"<synthetic>","content":[{"type":"text","text":"Login expired · Please run /login"}]},"error":"authentication_failed","isApiErrorMessage":true}"#;
    const QUOTE: &str = r#"{"isSidechain":false,"type":"assistant","message":{"model":"claude-opus-5-5","content":[{"type":"text","text":"API Error: Connection refused (ECONNREFUSED) is what the user saw."}]}}"#;

    fn tail(rows: &[&str]) -> String {
        rows.join("\n")
    }

    #[test]
    fn the_three_failed_threads_are_open_failures() {
        for row in FAILED_ROWS {
            let open = claude_open_failure_in(&tail(&[PROMPT, TOOL_RESULT, row]));
            assert!(
                open.is_some_and(|open| open.text.starts_with("API Error:")),
                "{row}"
            );
        }
    }

    #[test]
    fn a_later_turn_closes_the_failure() {
        for later in [PROMPT, REPLY] {
            assert!(claude_open_failure_in(&tail(&[FAILED_ROWS[2], later])).is_none());
        }
        // A tool result is part of the same turn and leaves it open.
        assert!(claude_open_failure_in(&tail(&[FAILED_ROWS[2], TOOL_RESULT])).is_some());
    }

    #[test]
    fn limits_logins_and_quotes_are_not_retried_here() {
        for row in [LIMIT, LOGIN, QUOTE] {
            assert!(
                claude_open_failure_in(&tail(&[PROMPT, row])).is_none(),
                "{row}"
            );
        }
    }

    #[test]
    fn a_blocked_try_gives_its_quick_try_back() {
        let now = Utc::now();
        // As the claim leaves it: the third quick try, started now, after one at 18:00.
        let mut recovery = json!({
            "status": "retrying", "trigger": "streamError", "attempt": 3,
            "previousAttemptAt": "2026-10-16T18:00:00+00:00", "lastAttemptAt": now.to_rfc3339(),
        });
        give_back_blocked_try(
            &mut recovery,
            "Teach auto mode about your environment?",
            now,
        );
        assert_eq!(recovery["status"], "waiting");
        assert_eq!(recovery["attempt"], 2);
        assert_eq!(recovery["lastAttemptAt"], "2026-10-16T18:00:00+00:00");
        assert_eq!(
            recovery["blockedBy"],
            "Teach auto mode about your environment?"
        );
        let next =
            DateTime::parse_from_rfc3339(recovery["nextAttemptAt"].as_str().unwrap()).unwrap();
        assert_eq!((next.with_timezone(&Utc) - now).num_seconds(), 5);
        // The card hears what blocks it.
        let shown = chat_state(&json!({ "accountRecovery": recovery })).unwrap();
        assert_eq!(
            shown["blockedBy"],
            "Teach auto mode about your environment?"
        );
        assert_eq!(shown["attempt"], 2);
    }

    #[test]
    fn a_screen_that_stays_blocked_reaches_the_orchestrator() {
        let session = |since: DateTime<Utc>| {
            json!({ "runtimeSettings": { "accountRecovery": {
                "status": "waiting", "trigger": "streamError", "attempt": 1,
                "blockedBy": "Claude Code is waiting for folder trust", "blockedSince": since.to_rfc3339(),
            }}})
        };
        assert!(quick_retry_pending(&session(Utc::now())));
        assert!(!quick_retry_pending(&session(
            Utc::now() - chrono::Duration::seconds(61)
        )));
    }

    #[test]
    fn a_try_whose_turn_finished_is_not_sent_again() {
        let tried = Utc::now() - chrono::Duration::seconds(30);
        let key = ("Ptest".to_string(), "Gtest".to_string());
        let end = |finished: bool, seconds_after: i64| {
            turn_ends().lock().unwrap().insert(
                key.clone(),
                (tried + chrono::Duration::seconds(seconds_after), finished),
            );
        };
        end(true, 10);
        assert!(last_try_answered("Ptest", "Gtest", Some(tried)));
        // The continuation's own turn failed again: that is a new failure to retry.
        end(false, 12);
        assert!(!last_try_answered("Ptest", "Gtest", Some(tried)));
        // A finished turn from before the try says nothing about it.
        end(true, -5);
        assert!(!last_try_answered("Ptest", "Gtest", Some(tried)));
        assert!(!last_try_answered("Ptest", "Gtest", None));
    }

    #[test]
    fn quick_tries_come_before_the_slow_schedule() {
        let minutes = |attempt| error_backoff(attempt).num_seconds();
        assert_eq!(
            (0..5).map(minutes).collect::<Vec<_>>(),
            [5, 15, 60, 120, 300]
        );
        assert_eq!(
            (5..11).map(minutes).collect::<Vec<_>>(),
            [300, 600, 1200, 2400, 3600, 3600]
        );
    }
}
