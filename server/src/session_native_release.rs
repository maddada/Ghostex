//! Native idle release: which idle Native chats this daemon stops so their agent programs stop
//! holding memory. Pure: the runner (`server/session_native_release_sweep.rs`) reads the inputs
//! and stops them through the ordinary sleep path, which keeps the conversation and its resume id.
//!
//! CDXC:SessionSleep 2026-11-27 DECISION:
//! User: "I really want users to not worry about "sleeping" we should manage this fully intelligently for the user to minimize bg running stuff", and of the Auto Sleep setting: "this auto sleep setting is for terminal-based chats. native chats shouldn't care about what's configured here. it should sleep based on our intelligent rules to minimize bg running processes that aren't needed basically". So a Native chat is released on these rules whatever Auto Sleep says, Auto Sleep skips Native chats (`session_auto_sleep.rs`), and a released chat starts its agent again when a message is sent to it ("wake on send").
//!
//! CDXC:SessionSleep 2026-11-27 WHY:
//! The idle times follow how dear a restart is. Another agent GUI releases an idle agent after 2 minutes, because its resume reconnects in-process; here a release ends the chat's daemon, host and agent, and the next message starts all three and reloads the conversation (Native Claude allows 90 s), so a quick back-and-forth must not pay that. 10 minutes covers the pauses of a conversation while still freeing the 200 to 500 MB an idle Native Claude holds. A chat some client shows waits 30 minutes, since the user is probably reading it and about to answer. Under memory pressure (the kernel's warning or critical level) both shorten to 2 and 10 minutes. Being stopped this way is not shown anywhere: a released Native chat looks like any idle chat.
//! SEE-ALSO: server/src/session_auto_sleep.rs (the Terminal-based chats' Auto Sleep), server/src/zmx/endpoint.rs (the daemon's automatic-sleep declines, which `sleepTrigger: nativeIdle` also passes), server/src/session_chat_send_wake.rs (the wake on send).

use chrono::DateTime;
use serde_json::Value;

const MINUTE_MS: i64 = 60_000;

/// Idle minutes before a Native chat nobody shows is released.
const HIDDEN_IDLE_MINUTES: i64 = 10;
/// Idle minutes before a Native chat a client shows is released.
const SHOWN_IDLE_MINUTES: i64 = 30;
/// The same two under memory pressure.
const HIDDEN_IDLE_MINUTES_UNDER_PRESSURE: i64 = 2;
const SHOWN_IDLE_MINUTES_UNDER_PRESSURE: i64 = 10;

/// Hours an idle Native chat's background work (a background command, subagent or job, or a tool
/// call still waiting for its result) keeps it running before it is released anyway.
///
/// CDXC:SessionSleep 2026-11-27 DECISION: Coordinator follow-up: "keep the 4-hour cap like the other GUI so stuck "busy" never holds forever". Counted from the chat's last activity, so a turn or a thread report restarts it. It covers only the work read from the transcript: a turn the agent reports running, a question card or a queued message still keeps the chat running.
const MAX_BUSY_PIN_HOURS: i64 = 4;

/// How long a Native chat must have been idle before it is released.
pub fn idle_minutes(shown: bool, memory_pressure: bool) -> i64 {
    match (shown, memory_pressure) {
        (false, false) => HIDDEN_IDLE_MINUTES,
        (true, false) => SHOWN_IDLE_MINUTES,
        (false, true) => HIDDEN_IDLE_MINUTES_UNDER_PRESSURE,
        (true, true) => SHOWN_IDLE_MINUTES_UNDER_PRESSURE,
    }
}

/// Whether a presentation row is a Native chat (`nonCliMode`, or the older `acpMode`).
pub fn is_native_chat(session: &Value) -> bool {
    ["nonCliMode", "acpMode"]
        .iter()
        .any(|key| session.get(*key).and_then(Value::as_bool) == Some(true))
}

/// What the presentation row says about releasing a Native chat: awake, idle, nothing pending,
/// with a conversation to resume. The runner adds what the row cannot say (background work read
/// from the transcript, a coordinator's threads, a switch between Native and Terminal-based).
pub fn row_allows_release(session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    let non_empty = |key: &str| text(key).is_some_and(|value| !value.trim().is_empty());
    let flag = |key: &str| session.get(key).and_then(Value::as_bool) == Some(true);
    // `attention` is a finished turn whose reply nobody has opened yet: its agent is as idle as an
    // `idle` one, and releasing it keeps the unread mark. (A connector card waiting in a turn also
    // reads `attention`; the sweep leaves those running.)
    if !is_native_chat(session)
        || text("lifecycleState") != Some("running")
        || !matches!(text("activity"), Some("idle" | "attention"))
        || text("kind") != Some("agent")
    {
        return false;
    }
    // Chats that do not run on this computer have their own lifecycles.
    if ["agentbox", "claudeCloud", "cloudBox"]
        .iter()
        .any(|key| session.get(*key).is_some_and(|value| !value.is_null()))
    {
        return false;
    }
    if flag("switchingHostMode") || flag("switchToNativeQueued") {
        return false;
    }
    // A chat nobody has prompted has no conversation to resume yet.
    if !flag("hasEverBeenActive") {
        return false;
    }
    if flag("awaitingApproval")
        || ghostex_gx_protocol::session_has_question(
            session
                .get("pendingQuestionCount")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            flag("awaitingAnswer"),
        )
    {
        return false;
    }
    if non_empty("backgroundWorkDetectedAt") {
        return false;
    }
    if non_empty("delayedSendDeadlineAt")
        || session
            .get("delayedSendRemainingMs")
            .is_some_and(|value| !value.is_null())
        || flag("sendWhenAgentStopsActive")
        || flag("sendWhenAllProjectSessionsStopActive")
    {
        return false;
    }
    let count = |key: &str| session.get(key).and_then(Value::as_i64).unwrap_or(0);
    if count("queuedPromptCount") - count("queuedPromptFailedCount") > 0 {
        return false;
    }
    if text("coordinatorThreadState") == Some("working") {
        return false;
    }
    non_empty("agentSessionId") || non_empty("agentSessionPath")
}

/// When the chat last did anything: the later of the last turn's start (`lastActiveAt`), the
/// last change of its agent's activity (a turn's end, stored on the session row) and its
/// threads' latest activity when it is a coordinator.
pub fn idle_since_ms(
    session: &Value,
    activity_changed_at: Option<&str>,
    last_thread_activity_ms: Option<i64>,
) -> Option<i64> {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    [
        timestamp_ms(text("lastActiveAt").or_else(|| text("updatedAt"))),
        timestamp_ms(activity_changed_at),
        last_thread_activity_ms,
    ]
    .into_iter()
    .flatten()
    .max()
}

/// Whether a chat idle since `idle_since_ms` is due for release now.
pub fn release_due(idle_since_ms: i64, shown: bool, memory_pressure: bool, now_ms: i64) -> bool {
    now_ms - idle_since_ms >= idle_minutes(shown, memory_pressure) * MINUTE_MS
}

/// Whether a chat idle since `idle_since_ms` has been held by background work long enough that it
/// is released anyway.
pub fn busy_pin_expired(idle_since_ms: i64, now_ms: i64) -> bool {
    now_ms - idle_since_ms >= MAX_BUSY_PIN_HOURS * 60 * MINUTE_MS
}

fn timestamp_ms(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.timestamp_millis())
}
