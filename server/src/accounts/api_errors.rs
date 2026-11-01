//! Automatic retry after an agent's API call failed on a connection or server error: how the
//! failure is found, the quick tries before the slow schedule, the wait for the network, and the
//! `apiRetry` state the chat draws its error card from.
//!
//! CDXC:AgentProviders 2026-10-15 WHY:
//! Recovery used to start only when the screen showed a known error line, and Claude's current wordings ("Can't reach the API server … (ENOTFOUND)", "Connection refused … (ECONNREFUSED)", "Connection lost mid-response") were not in that list, so three threads that died together when the network dropped were never retried. Claude records every such failure as a synthetic `isApiErrorMessage` row with `error: "server_error"` (logins, limits, refusals and bad models carry other codes), so for Claude the transcript is the trigger and the screen list is only the fallback; Codex keeps no record of its errors, so it still relies on the screen.
//! SEE-ALSO: recovery.rs (plan_session, the 2026-10-15 schedule DECISION), session_chat_decode_claude.rs (`apiError` on the row), packages/gx-chat-core/src/transcript/api_errors.rs (the card), coordinators/state.rs (a thread stays working while a quick try is pending).

use super::model::Provider;
use crate::{
    domain::{DomainRepository, DomainStateError},
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

/// The failed-episode marker on `accountRecovery`: the time the episode's failed row was stored,
/// `null` while a row that came due could not be stored (the episode still owes it), absent
/// otherwise. A marker alone, with no `status`, is bookkeeping and never a recovery
/// (`shown_recovery`).
pub(crate) const FAILURE_NOTIFIED_AT: &str = "failureNotifiedAt";

/// The recovery triggers that are errors: a connection or server error, an agent error line, and a
/// login the provider refused. A usage limit waits for its reset and an exit names no error, so
/// neither ever ends in a failed row.
pub(crate) fn is_error_trigger(recovery: &Value) -> bool {
    matches!(
        recovery["trigger"].as_str(),
        Some(SESSION_CHAT_NOTICE_STREAM_ERROR | "agentError" | "accountDisabled")
    )
}

fn announced(recovery: &Value) -> bool {
    recovery[FAILURE_NOTIFIED_AT].is_string()
}

fn owed(recovery: &Value) -> bool {
    recovery
        .get(FAILURE_NOTIFIED_AT)
        .is_some_and(Value::is_null)
}

/// CDXC:Notifications 2026-10-31 WHY:
/// A connection or server error the quick tries get past is not worth a notification, so an error episode gets one failed row, at the first of: the fifth quick try failed (the slow schedule takes over, the moment the orchestrator's report changes too), recovery gave up on it (`needsAttention`: retries turned off mid-way, a disabled account with no replacement, no saved conversation, a queued message to review, a send that failed, a restart during a try), recovery will not retry an error that waits or stands (`announce_episode`: switched off, retries after temporary errors off, never started), or an agent's hook reported the failed run (server/src/notification_feed/failure.rs). Only error triggers count (`is_error_trigger`); sending the fifth try is not a failure, a try the screen turned away gives its count back, and Stop retrying never notifies.
/// The row and the marker are stored in one transaction (`store_failure_notice`), so a dismissed row, a pruned feed or a gxserver restart cannot announce the episode again, and neither can a row stored without its marker. A row that could not be stored leaves the marker `null` with the episode's trigger and status kept, and the episode's next save or the next recovery pass (`settle_failure_notice`, ahead of everything that skips a session) stores it.
/// The marker stays across everything that is the same episode: wait metadata, an attempt-zero wait rebuilt for an error a hook already announced, Retry now, an automatic continuation, an account switch Ghostex makes by itself and the continuation after it (`switch_recovery`), settings saves, recovery switched off. It goes where the episode ends: a new failure after a try the API answered, a minute of progress, a new run the user starts from the composer (`recovery::user_action`) or in the agent's terminal (`end_failure_episode`), an account the user picks, Stop retrying. A network that stays down and a screen that keeps holding the tries back are waits, not ends: recovery reports neither, so neither notifies.
/// SEE-ALSO: `save`, `collect` and `plan_session` in recovery.rs, `sessionPolicy`, `select` and `state_value` in endpoint.rs, restart.rs, continuation.rs, wake_account.rs, server/src/notification_feed/failure.rs.
fn episode_ends(previous: &Value, next: &Value) -> bool {
    let attempt = |recovery: &Value| recovery["attempt"].as_u64().unwrap_or(0);
    // The API answered the last try, so this failure is a new one and the count starts over.
    let starts_over = previous["status"] == "resumed"
        && attempt(previous) > 0
        && next["status"] == "waiting"
        && attempt(next) == 0;
    // The pass's healthy reset after a minute of progress (`plan_session`).
    let recovered = next["status"] == "resumed"
        && attempt(next) == 0
        && next["healthySince"]
            .as_str()
            .and_then(|since| DateTime::parse_from_rfc3339(since).ok())
            .is_some_and(|since| {
                Utc::now() - since.with_timezone(&Utc) >= chrono::Duration::minutes(1)
            });
    starts_over || recovered
}

/// Carries the episode's marker onto a recovery state rebuilt for the same episode, and drops it
/// where the episode ends.
pub(crate) fn reconcile_failure_notice(previous: &Value, next: &mut Value) {
    if episode_ends(previous, next) {
        if let Some(fields) = next.as_object_mut() {
            fields.remove(FAILURE_NOTIFIED_AT);
        }
    } else if next.get(FAILURE_NOTIFIED_AT).is_none() {
        if let Some(marker) = previous.get(FAILURE_NOTIFIED_AT) {
            next[FAILURE_NOTIFIED_AT] = marker.clone();
        }
    }
}

/// The digest an agent's prompt-submit hook forwards as `promptDigest` instead of the prompt
/// (agent_hooks/notify_runtime.rs), and Ghostex keeps of the continuation it types: SHA-256 of the
/// prompt with its whitespace collapsed.
pub(crate) fn prompt_digest(prompt: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(
        crate::agent_hooks::probing::normalize_prompt_text(prompt).as_bytes(),
    ))
}

/// How long a typed continuation waits for its own prompt hook, which normally follows the send
/// within seconds. A hook that arrives later counts as a user prompt.
pub(crate) const CONTINUATION_HOOK_WINDOW: Duration = Duration::from_secs(120);
/// At most this many sessions with a typed continuation waiting for its hook; the oldest goes first.
const TYPED_CONTINUATION_CAP: usize = 256;

type SessionKey = (String, String);

/// A continuation Ghostex typed: its recovery `claim`, the digest of its text, when it was typed.
struct TypedContinuation {
    claim: String,
    digest: String,
    at: Instant,
}

/// The continuation Ghostex is typing into each session: recorded when the bytes are about to be
/// sent (not when the claim is prepared, which can be long before the composer is ready). It goes
/// when its own prompt hook arrives, when the session's recovery takes another claim or is removed,
/// when the session is stopped or deleted (the recovery pass), after `CONTINUATION_HOOK_WINDOW`, and
/// past `TYPED_CONTINUATION_CAP`. In memory only; no prompt text.
static TYPED_CONTINUATIONS: std::sync::LazyLock<Mutex<HashMap<SessionKey, TypedContinuation>>> =
    std::sync::LazyLock::new(Default::default);

fn session_key(project: &str, session: &str) -> SessionKey {
    (project.to_string(), session.to_string())
}

/// Drops the continuations whose hook window has passed, then the oldest past `cap`.
fn bound_continuations(
    typed: &mut HashMap<SessionKey, TypedContinuation>,
    now: Instant,
    cap: usize,
) {
    typed.retain(|_, continuation| now.duration_since(continuation.at) < CONTINUATION_HOOK_WINDOW);
    while typed.len() > cap {
        let Some(oldest) = typed
            .iter()
            .min_by_key(|(_, continuation)| continuation.at)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        typed.remove(&oldest);
    }
}

/// Records the continuation about to be typed for `claim` (`recovery::deliver`, the continuation
/// after an account switch), as the send expands it (review-comment pills become their text).
pub(crate) fn note_continuation(project: &str, session: &str, claim: &str, text: &str) {
    let typed_text = ghostex_gx_chat_core::composer::review_comments::expand_review_comments(text);
    if let Ok(mut typed) = TYPED_CONTINUATIONS.lock() {
        typed.insert(
            session_key(project, session),
            TypedContinuation {
                claim: claim.to_string(),
                digest: prompt_digest(&typed_text),
                at: Instant::now(),
            },
        );
        bound_continuations(&mut typed, Instant::now(), TYPED_CONTINUATION_CAP);
    }
}

/// Forgets the session's typed continuation (its recovery took another claim or was removed).
pub(crate) fn forget_continuation(project: &str, session: &str) {
    if let Ok(mut typed) = TYPED_CONTINUATIONS.lock() {
        typed.remove(&session_key(project, session));
    }
}

/// Keeps the typed continuations of the sessions the recovery pass still reads (not stopped, not
/// deleted).
pub(crate) fn retain_continuations(sessions: &[Value]) {
    let live: std::collections::HashSet<SessionKey> = sessions
        .iter()
        .filter_map(|session| {
            Some(session_key(
                session["projectId"].as_str()?,
                session["sessionId"].as_str()?,
            ))
        })
        .collect();
    if let Ok(mut typed) = TYPED_CONTINUATIONS.lock() {
        typed.retain(|session, _| live.contains(session));
    }
}

/// Whether a prompt hook is the continuation Ghostex typed for `claim`: its text matches what was
/// typed, or, for a hook that forwards no text, a continuation for that claim was typed and its own
/// hook has not come yet. A match is consumed; a record of an older claim is dropped.
fn take_continuation(project: &str, session: &str, claim: &str, digest: Option<&str>) -> bool {
    let Ok(mut typed) = TYPED_CONTINUATIONS.lock() else {
        return false;
    };
    bound_continuations(&mut typed, Instant::now(), TYPED_CONTINUATION_CAP);
    let key = session_key(project, session);
    let Some(continuation) = typed.get(&key) else {
        return false;
    };
    if continuation.claim != claim {
        typed.remove(&key);
        return false;
    }
    let matched = digest.is_none_or(|digest| digest == continuation.digest);
    if matched {
        typed.remove(&key);
    }
    matched
}

/// Whether the session has a typed continuation waiting for its hook (tests).
#[cfg(test)]
pub(crate) fn continuation_pending(project: &str, session: &str) -> bool {
    TYPED_CONTINUATIONS
        .lock()
        .is_ok_and(|typed| typed.contains_key(&session_key(project, session)))
}

/// Moves the session's typed continuation back in time by `by` (tests of the hook window).
#[cfg(test)]
pub(crate) fn age_continuation(project: &str, session: &str, by: Duration) {
    if let Ok(mut typed) = TYPED_CONTINUATIONS.lock() {
        if let Some(continuation) = typed.get_mut(&session_key(project, session)) {
            continuation.at -= by;
        }
    }
}

/// Whether the recovery state just built owes its episode's failed row. The fifth failed quick try
/// counts when the wait it starts is new (a failed try, not a wait updated in place), so a wait
/// whose episode a new user turn ended does not announce itself again on its next update.
fn notice_due(previous: &Value, next: &Value) -> bool {
    if announced(next) {
        return false;
    }
    let attempt = |recovery: &Value| recovery["attempt"].as_u64().unwrap_or(0);
    owed(next)
        || (is_error_trigger(next)
            && match next["status"].as_str() {
                Some("needsAttention") => previous["status"] != "needsAttention",
                Some("waiting") => {
                    is_connection_trigger(next)
                        && attempt(next) >= QUICK_TRIES
                        && !(previous["status"] == "waiting" && attempt(previous) == attempt(next))
                }
                _ => false,
            })
}

/// Stores the recovery state the pass built from `previous` (`runtime`'s `accountRecovery`), with
/// its episode's failed row when one is due. The caller publishes the session.
pub(crate) fn store_recovery(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    previous: &Value,
    mut runtime: Map<String, Value>,
) -> Result<(), DomainStateError> {
    if runtime
        .get("accountRecovery")
        .map_or(&Value::Null, |next| &next["claim"])
        != &previous["claim"]
    {
        forget_continuation(
            session["projectId"].as_str().unwrap_or_default(),
            session["sessionId"].as_str().unwrap_or_default(),
        );
    }
    let due = runtime.get_mut("accountRecovery").is_some_and(|recovery| {
        reconcile_failure_notice(previous, recovery);
        notice_due(previous, recovery)
    });
    if !due {
        return super::endpoint::update_session_members(repo, session, &runtime, SAVE_MEMBERS)
            .map(|_| ());
    }
    let mut unannounced = runtime.clone();
    unannounced["accountRecovery"][FAILURE_NOTIFIED_AT] = Value::Null;
    if crate::notification_feed::store_failure_notice(
        state,
        repo,
        session,
        runtime,
        Some(unannounced),
        SAVE_MEMBERS,
    )? {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
    Ok(())
}

/// The runtime members a recovery save writes.
pub(crate) const SAVE_MEMBERS: &[&str] = &["accountRecovery", "accountSwitch"];

/// The session's episode gets its failed row now unless it has one: an error recovery will not
/// retry, a failed run a hook reported, or a row still owed. The recovery state stays as it is
/// apart from the marker, or is only the marker when there is none. The caller holds
/// `accounts.mutations`.
pub(crate) fn announce_episode(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let recovery = runtime
        .get("accountRecovery")
        .filter(|recovery| recovery.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    if announced(&recovery) {
        return Ok(());
    }
    let unannounced = (!owed(&recovery)).then(|| {
        let mut runtime = runtime.clone();
        let mut recovery = recovery.clone();
        recovery[FAILURE_NOTIFIED_AT] = Value::Null;
        runtime.insert("accountRecovery".into(), recovery);
        runtime
    });
    runtime.insert("accountRecovery".into(), recovery);
    if crate::notification_feed::store_failure_notice(
        state,
        repo,
        session,
        runtime,
        unannounced,
        &["accountRecovery"],
    )? {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
    Ok(())
}

/// Whether the session's episode owes its row without a new state from the pass: a row that came
/// due and could not be stored, or an error recovery waits on but will not retry (`retries` false).
/// Never after Stop retrying.
fn close_due(session: &Value, retries: bool) -> bool {
    let runtime = &session["runtimeSettings"];
    let recovery = &runtime["accountRecovery"];
    runtime["accountRecoverySuppressed"] != true
        && !announced(recovery)
        && (owed(recovery)
            || (!retries && recovery["status"] == "waiting" && is_error_trigger(recovery)))
}

/// The recovery pass's first look at each session it reads, ahead of everything that may skip the
/// session (the collection's fast path, a policy that is off, a screen not captured): a row the
/// episode owes is stored here. Errors are logged.
pub(crate) fn settle_failure_notice(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
) {
    if !close_due(session, false) {
        return;
    }
    let (Some(project), Some(id)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    let _gate = state.accounts.mutation_gate();
    let outcome = repo.get_session(project, id).and_then(|row| match row {
        Some(row) if close_due(&row, super::recovery::recovery_retries(repo, &row, false)) => {
            announce_episode(state, repo, &row)
        }
        _ => Ok(()),
    });
    if let Err(error) = outcome {
        crate::notification_feed::log_failed_notice(state, session, error);
    }
}

/// A failed run an agent's hook reported (server/src/notification_feed/failure.rs): the episode's
/// row unless recovery or an earlier hook already stored it, whatever the current policy says.
pub(crate) fn announce_failed_turn(
    state: &AppState,
    repo: &DomainRepository<'_>,
    project: &str,
    session: &str,
) -> Result<(), DomainStateError> {
    let _gate = state.accounts.mutation_gate();
    match repo.get_session(project, session)? {
        Some(row) => announce_episode(state, repo, &row),
        None => Ok(()),
    }
}

/// A prompt the user submitted in the agent's terminal starts a new run, so the episode before it
/// ends, unless the prompt is the continuation Ghostex typed itself for the recovery's current
/// `claim` (a recovery try, Retry now, the continuation after an automatic account switch):
/// `digest` is the hook's `promptDigest`, matched against what was typed (`take_continuation`).
/// Ending drops the marker; a recovery state that is only the marker, or an error wait recovery
/// will no longer retry (marked or not), goes with it as a chat message would retire it, so the old
/// error is not announced again.
pub(crate) fn end_failure_episode(
    state: &AppState,
    repo: &DomainRepository<'_>,
    project: &str,
    session: &str,
    digest: Option<&str>,
) -> Result<(), DomainStateError> {
    let _gate = state.accounts.mutation_gate();
    let Some(row) = repo.get_session(project, session)? else {
        return Ok(());
    };
    let recovery = &row["runtimeSettings"]["accountRecovery"];
    let status = recovery["status"].as_str();
    if let Some(claim) = recovery["claim"]
        .as_str()
        .filter(|_| matches!(status, Some("retrying" | "resumed")))
    {
        if take_continuation(project, session, claim, digest) {
            return Ok(());
        }
    }
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let stale_wait = status == Some("waiting")
        && is_error_trigger(recovery)
        && !super::recovery::recovery_retries(repo, &row, false);
    if stale_wait || (status.is_none() && recovery.get(FAILURE_NOTIFIED_AT).is_some()) {
        runtime.remove("accountRecovery");
    } else if recovery.get(FAILURE_NOTIFIED_AT).is_some() {
        let mut kept = recovery.clone();
        if let Some(fields) = kept.as_object_mut() {
            fields.remove(FAILURE_NOTIFIED_AT);
        }
        runtime.insert("accountRecovery".into(), kept);
    } else {
        return Ok(());
    }
    super::endpoint::update_session_members(repo, &row, &runtime, &["accountRecovery"]).map(|_| ())
}

/// A settings save (`sessionPolicy`) starts recovery over, but neither ends nor repeats a failed
/// episode: the marker stays as a recovery state of its own, and an error the new settings will
/// not retry is announced once, keeping its recovery state until its row is stored. `runtime`
/// holds the new settings.
pub(crate) fn save_settings(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    mut runtime: Map<String, Value>,
) -> Result<(), DomainStateError> {
    let previous = runtime.remove("accountRecovery").unwrap_or(Value::Null);
    if let Some(marker) = previous.get(FAILURE_NOTIFIED_AT) {
        runtime.insert(
            "accountRecovery".into(),
            json!({ FAILURE_NOTIFIED_AT: marker }),
        );
    }
    let mut next = session.clone();
    next["runtimeSettings"] = Value::Object(runtime.clone());
    if !close_due(
        session,
        super::recovery::recovery_retries(repo, &next, false),
    ) {
        return super::endpoint::update_session_members(repo, session, &runtime, SETTINGS_MEMBERS)
            .map(|_| ());
    }
    let mut unannounced = runtime.clone();
    let mut kept = previous;
    kept[FAILURE_NOTIFIED_AT] = Value::Null;
    unannounced.insert("accountRecovery".into(), kept);
    runtime.insert("accountRecovery".into(), json!({}));
    if crate::notification_feed::store_failure_notice(
        state,
        repo,
        session,
        runtime,
        Some(unannounced),
        SETTINGS_MEMBERS,
    )? {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
    Ok(())
}

/// The runtime members a settings save writes.
const SETTINGS_MEMBERS: &[&str] = &[
    "accountPolicyOverride",
    "accountRecoverySuppressed",
    "accountRecovery",
];

/// The session's recovery as the account answer shows it (the account menus' recovery block and
/// its Stop automatic recovery button): only a recovery with a status, never a marker alone.
pub(crate) fn shown_recovery(session: &Value) -> Option<&Value> {
    session
        .pointer("/runtimeSettings/accountRecovery")
        .filter(|recovery| recovery["status"].is_string())
}

/// An account switch puts `next` (or nothing) in place of the session's recovery state. A switch
/// Ghostex makes by itself (`automatic`: a limit, a disabled login, a wake onto an account with
/// room) and the continuation after it stay the same error episode, so the marker stays, on `next`
/// or as a recovery state of its own. An account the user picks, and the switch that goes with the
/// user's own message (`disabled::switch_before_send`), end the episode like a new turn.
pub(crate) fn switch_recovery(
    runtime: &mut Map<String, Value>,
    next: Option<Value>,
    automatic: bool,
) {
    let marker = runtime
        .remove("accountRecovery")
        .and_then(|previous| previous.get(FAILURE_NOTIFIED_AT).cloned())
        .filter(|_| automatic);
    let next = match (next, marker) {
        (Some(mut next), Some(marker)) => {
            if next.get(FAILURE_NOTIFIED_AT).is_none() {
                next[FAILURE_NOTIFIED_AT] = marker;
            }
            Some(next)
        }
        (None, Some(marker)) => Some(json!({ FAILURE_NOTIFIED_AT: marker })),
        (next, None) => next,
    };
    if let Some(next) = next {
        runtime.insert("accountRecovery".into(), next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_continuations_expire_and_stay_bounded() {
        let now = Instant::now();
        let typed_ago = |seconds: u64| TypedContinuation {
            claim: "claim".to_string(),
            digest: String::new(),
            at: now - Duration::from_secs(seconds),
        };
        let mut typed: HashMap<SessionKey, TypedContinuation> = HashMap::new();
        let past_window = CONTINUATION_HOOK_WINDOW.as_secs() + 1;
        for (session, age) in [
            ("expired", past_window),
            ("oldest", 30),
            ("older", 20),
            ("newest", 10),
        ] {
            typed.insert(session_key("project", session), typed_ago(age));
        }
        let kept = |typed: &HashMap<SessionKey, TypedContinuation>| {
            let mut kept: Vec<String> = typed.keys().map(|(_, session)| session.clone()).collect();
            kept.sort_unstable();
            kept
        };
        // Past the hook window, whatever the room.
        bound_continuations(&mut typed, now, 10);
        assert_eq!(kept(&typed), ["newest", "older", "oldest"]);
        // Past the cap, the oldest first.
        bound_continuations(&mut typed, now, 2);
        assert_eq!(kept(&typed), ["newest", "older"]);
    }

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

/// The failed notification at the recovery boundary: every state goes through `recovery::save`,
/// the one place the pass persists recovery, on a temporary state database, in the order the
/// pass writes them (a new failure, a claimed try, its send, the next failure).
#[cfg(test)]
mod failure_notice_tests {
    use super::*;
    use crate::domain::DomainRepository;
    use crate::storage::open_gxserver_database;
    use std::sync::Arc;

    struct Session {
        state: Arc<AppState>,
        project: String,
        session: String,
        _temp: tempfile::TempDir,
    }

    impl Session {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("tempdir");
            let state = crate::server::tests::helpers::test_app_state(
                crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf())),
            );
            let db = open_gxserver_database(&state.paths).expect("db");
            let repo = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let params = |value: Value| value.as_object().cloned().expect("params");
            let project = repo
                .create_project(&params(json!({ "name": "Recovery", "path": temp.path() })))
                .expect("project")["projectId"]
                .as_str()
                .expect("projectId")
                .to_string();
            let session = repo
                .create_session(
                    &params(json!({
                        "projectId": project, "kind": "agent", "agentId": "claude",
                        "title": "Retrying thread", "lifecycleState": "running",
                    })),
                    false,
                )
                .expect("session")["sessionId"]
                .as_str()
                .expect("sessionId")
                .to_string();
            Self {
                state,
                project,
                session,
                _temp: temp,
            }
        }

        fn stored(&self) -> Value {
            let db = open_gxserver_database(&self.state.paths).expect("db");
            DomainRepository::new(&db, self.state.metadata.server_id.as_str())
                .get_session(&self.project, &self.session)
                .expect("read")
                .expect("session")
        }

        fn save(&self, recovery: Value) {
            let db = open_gxserver_database(&self.state.paths).expect("db");
            let repo = DomainRepository::new(&db, self.state.metadata.server_id.as_str());
            let session = self.stored();
            super::super::recovery::save(&self.state, &repo, &session, recovery).expect("save");
        }

        /// The pass's writes around one quick try: claimed, sent, and the failure seen after it.
        fn try_that_fails(&self, attempt: u64) {
            self.save(state("retrying", attempt));
            self.save(state("resumed", attempt));
            self.save(state("waiting", attempt));
        }

        fn failed_rows(&self) -> usize {
            let db = open_gxserver_database(&self.state.paths).expect("db");
            crate::notification_feed::store::read_notification_feed_state(&db).expect("feed")
                ["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|row| row["kind"] == "failed" && row["sessionId"] == self.session.as_str())
                .count()
        }
    }

    /// A connection or server error's recovery state as the pass builds it (a fresh object each
    /// time it starts a wait, as `plan_session` does).
    fn state(status: &str, attempt: u64) -> Value {
        json!({
            "status": status, "trigger": SESSION_CHAT_NOTICE_STREAM_ERROR, "attempt": attempt,
            "reason": "Claude Code hit a temporary service error",
            "errorText": "API Error: Connection refused (ECONNREFUSED)",
        })
    }

    #[tokio::test]
    async fn transient_failure_notifies_only_after_the_fifth_failed_retry() {
        let session = Session::new();
        session.save(state("waiting", 0));
        for attempt in 1..QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        assert_eq!(
            session.failed_rows(),
            0,
            "the first four failed tries stay quiet"
        );
        // Sending the fifth try is not a failure.
        session.save(state("retrying", QUICK_TRIES));
        session.save(state("resumed", QUICK_TRIES));
        assert_eq!(session.failed_rows(), 0);
        session.save(state("waiting", QUICK_TRIES));
        assert_eq!(session.failed_rows(), 1, "the fifth try failed");
        // The slow schedule and finally giving up belong to the same episode.
        session.try_that_fails(QUICK_TRIES + 1);
        session.try_that_fails(QUICK_TRIES + 2);
        let mut gave_up = state("needsAttention", QUICK_TRIES + 2);
        gave_up["reason"] = json!("A queued message needs review before recovery can continue.");
        session.save(gave_up);
        assert_eq!(session.failed_rows(), 1);
        // The row says nothing of the error text recovery keeps.
        let db = open_gxserver_database(&session.state.paths).expect("db");
        let feed = crate::notification_feed::store::read_notification_feed_state(&db).unwrap();
        assert!(!feed.to_string().contains("ECONNREFUSED"));
    }

    #[tokio::test]
    async fn a_blocked_fifth_try_and_a_fifth_try_that_works_do_not_notify() {
        let session = Session::new();
        session.save(state("waiting", 0));
        for attempt in 1..QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        // The screen turned the fifth try away: it gives its count back.
        session.save(state("retrying", QUICK_TRIES));
        let mut blocked = session.stored()["runtimeSettings"]["accountRecovery"].clone();
        give_back_blocked_try(
            &mut blocked,
            "Claude Code is waiting for folder trust",
            Utc::now(),
        );
        session.save(blocked);
        assert_eq!(session.failed_rows(), 0);
        // Sent again, and this time the agent makes progress for a minute: the episode ends.
        session.save(state("retrying", QUICK_TRIES));
        session.save(state("resumed", QUICK_TRIES));
        let mut healthy = session.stored()["runtimeSettings"]["accountRecovery"].clone();
        healthy["attempt"] = json!(0);
        healthy["reason"] = json!("The agent is making progress again.");
        session.save(healthy);
        assert_eq!(session.failed_rows(), 0);
    }

    #[tokio::test]
    async fn one_episode_notifies_once_across_a_cleared_feed_and_a_restart() {
        let session = Session::new();
        session.save(state("waiting", 0));
        for attempt in 1..=QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        assert_eq!(session.failed_rows(), 1);
        let stored = session.stored()["runtimeSettings"]["accountRecovery"].clone();
        assert!(stored[FAILURE_NOTIFIED_AT].is_string());
        // The chat's retry card reads the same state as before.
        assert_eq!(
            chat_state(&session.stored()["runtimeSettings"]).unwrap()["attempt"],
            QUICK_TRIES
        );
        {
            let db = open_gxserver_database(&session.state.paths).expect("db");
            crate::notification_feed::store::clear_notification_feed(&db).expect("clear");
        }
        session.try_that_fails(QUICK_TRIES + 1);
        // A restart during the next try turns it into needsAttention from the stored state.
        session.save(state("retrying", QUICK_TRIES + 2));
        let mut restarted = session.stored()["runtimeSettings"]["accountRecovery"].clone();
        restarted["status"] = json!("needsAttention");
        restarted["reason"] =
            json!("Ghostex restarted during a recovery attempt. Check the conversation before continuing.");
        session.save(restarted);
        assert_eq!(
            session.failed_rows(),
            0,
            "the episode was already announced"
        );

        // The user sends again: the run that follows is a new episode, so its failure notifies.
        let app = session.state.clone();
        let (project, id) = (session.project.clone(), session.session.clone());
        tokio::task::spawn_blocking(move || {
            super::super::recovery::user_action(&app, &project, &id, false)
        })
        .await
        .expect("join")
        .expect("send");
        session.save(state("waiting", 0));
        for attempt in 1..=QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        assert_eq!(session.failed_rows(), 1);
    }

    #[tokio::test]
    async fn recovery_that_gives_up_notifies_once_and_a_limit_wait_never_does() {
        let session = Session::new();
        session.save(state("waiting", 0));
        session.try_that_fails(1);
        // Retries were turned off mid-way: recovery ends before the fifth try.
        let mut off = session.stored()["runtimeSettings"]["accountRecovery"].clone();
        off["status"] = json!("needsAttention");
        session.save(off.clone());
        session.save(off);
        assert_eq!(session.failed_rows(), 1);

        let waiting = Session::new();
        let mut limit = state("waiting", 7);
        limit["trigger"] = json!("usageLimit");
        waiting.save(limit);
        assert_eq!(waiting.failed_rows(), 0);
    }

    #[tokio::test]
    async fn stop_retrying_never_notifies() {
        let session = Session::new();
        session.save(state("waiting", 0));
        for attempt in 1..QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        let state = session.state.clone();
        let (project, id) = (session.project.clone(), session.session.clone());
        tokio::task::spawn_blocking(move || {
            super::super::recovery::user_action(&state, &project, &id, true)
        })
        .await
        .expect("join")
        .expect("stop");
        assert!(session.stored()["runtimeSettings"]["accountRecovery"].is_null());
        assert_eq!(session.failed_rows(), 0);
    }

    impl Session {
        /// One hook event through the real agent endpoint dispatcher and its observers.
        fn hook(&self, params: Value) {
            let mut params = params.as_object().cloned().expect("params");
            params.insert("projectId".to_string(), json!(self.project));
            params.insert("sessionId".to_string(), json!(self.session));
            let _ = crate::server::dispatch_agent_http_blocking(
                &self.state,
                "/api/ingestAgentHookEvent".to_string(),
                "test-hook".to_string(),
                params,
            );
        }
    }

    #[tokio::test]
    async fn limit_waits_and_unidentified_exits_never_notify() {
        for trigger in ["usageLimit", "agentExited"] {
            let session = Session::new();
            let mut waiting = state("waiting", 1);
            waiting["trigger"] = json!(trigger);
            session.save(waiting.clone());
            waiting["status"] = json!("needsAttention");
            waiting["reason"] = json!("No saved conversation is available to resume.");
            session.save(waiting);
            assert_eq!(session.failed_rows(), 0, "{trigger}");
        }
    }

    #[tokio::test]
    async fn the_marker_is_stored_only_once_the_row_exists() {
        let session = Session::new();
        let db = open_gxserver_database(&session.state.paths).expect("db");
        db.execute_batch(
            "CREATE TRIGGER refuse_failed BEFORE INSERT ON notification_feed WHEN NEW.kind = 'failed' BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .expect("trigger");
        session.save(state("waiting", 0));
        for attempt in 1..=QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        assert_eq!(session.failed_rows(), 0);
        assert!(
            session.stored()["runtimeSettings"]["accountRecovery"][FAILURE_NOTIFIED_AT].is_null()
        );
        db.execute_batch("DROP TRIGGER refuse_failed;")
            .expect("drop");
        // The episode's next state change writes the row it still owes.
        session.try_that_fails(QUICK_TRIES + 1);
        assert_eq!(session.failed_rows(), 1);
        assert!(
            session.stored()["runtimeSettings"]["accountRecovery"][FAILURE_NOTIFIED_AT].is_string()
        );
    }

    #[tokio::test]
    async fn a_hook_and_recovery_announce_one_error_once_whoever_sees_it_first() {
        // The hook first: a refused login, then recovery gives up on the same refusal.
        let session = Session::new();
        session.hook(json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "oauth_org_not_allowed" }));
        assert_eq!(session.failed_rows(), 1);
        {
            let db = open_gxserver_database(&session.state.paths).expect("db");
            crate::notification_feed::store::mark_all_notifications_read(&db).expect("read");
        }
        session.save(json!({
            "status": "needsAttention", "trigger": "accountDisabled", "attempt": 0,
            "reason": "This account's organization disabled Claude subscription access.",
        }));
        assert_eq!(session.failed_rows(), 1);

        // Recovery first: the fifth try failed, then the hook reports a final error.
        let session = Session::new();
        session.save(state("waiting", 0));
        for attempt in 1..=QUICK_TRIES {
            session.try_that_fails(attempt);
        }
        assert_eq!(session.failed_rows(), 1);
        session.hook(
            json!({ "agentName": "claude", "eventName": "StopFailure", "stopError": "unknown" }),
        );
        assert_eq!(session.failed_rows(), 1);
    }
}
