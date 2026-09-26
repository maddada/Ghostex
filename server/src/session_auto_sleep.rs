//! Auto Sleep ("Sleep idle agent sessions"): which sessions this daemon's sweep puts to sleep.
//! Pure: the runner (`server/session_auto_sleep_sweep.rs`) reads the settings file, the
//! presentation snapshot and the keep-awake leases, and performs the sleeps.
//!
//! CDXC:SessionSleep 2026-09-25 DECISION:
//! User (question 1, answer 1A): auto-sleep and Close After Done run in gxserver, always, with the same settings. One policy for every client: sessions started from the phone or the CLI are covered, and it keeps working while the app window is closed. This replaces the desktop app runtime's sweep (`auto-sleep.ts`, `helpers/auto-sleep.ts`), whose rules are ported here one for one.
//!
//! CDXC:SessionSleep 2026-09-25 WHY:
//! gxserver does not know what a client shows, and the runtime's sweep skipped every session the user was looking at (CDXC:SessionSleep 2026-08-20). Clients now report the sessions they show as keep-awake leases (`/api/holdSessionsAwake`, the same presence input the phone already used), so a session is shown while any client holds it. The runtime's "a group with no focused or visible row keeps its first row awake" rule reads the leases as its focused and visible rows and the daemon's group order as the row order.
//!
//! SEE-ALSO: server/src/session_keep_awake.rs, server/src/server/session_auto_sleep_sweep.rs,
//! apps/desktop/src/app/gx_store/terminal_lifecycle/shown_sessions.rs (the desktop's report),
//! server/src/zmx/endpoint.rs (the daemon's own automatic-sleep declines).

use std::collections::HashSet;

use chrono::DateTime;
use serde_json::Value;

/// One minute, the runtime's `GPUI_AUTO_SLEEP_MINUTE_MS`.
const MINUTE_MS: i64 = 60_000;

/// The idle minutes the Settings page offers; anything else falls back.
const IDLE_MINUTE_OPTIONS: [i64; 8] = [0, 5, 10, 15, 30, 60, 120, 300];

/// The three settings the sweep reads, normalized the way `normalizeAutoSleepIdleMinutes` and
/// `readBoolean` normalize them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutoSleepSettings {
    pub idle_minutes: i64,
    pub favorite_sessions: bool,
    pub require_resume_reference: bool,
}

impl AutoSleepSettings {
    /// From the desktop's `native-sidebar-settings.json` (absent file: the shipped defaults, which
    /// leave Auto Sleep off).
    pub fn from_settings(settings: Option<&Value>) -> Self {
        let legacy = settings.and_then(|settings| settings.get("autoSleepAgentSessionsEnabled"));
        let stored = settings
            .and_then(|settings| settings.get("autoSleepAgentIdleMinutes"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite());
        let idle_minutes = if legacy == Some(&Value::Bool(false)) {
            0
        } else {
            let fallback = if legacy == Some(&Value::Bool(true)) {
                15
            } else {
                0
            };
            let value = stored.unwrap_or(fallback as f64);
            IDLE_MINUTE_OPTIONS
                .iter()
                .copied()
                .find(|option| *option as f64 == value)
                .unwrap_or(fallback)
        };
        let flag = |key: &str, fallback: bool| {
            settings
                .and_then(|settings| settings.get(key))
                .and_then(Value::as_bool)
                .unwrap_or(fallback)
        };
        Self {
            idle_minutes,
            favorite_sessions: flag("autoSleepFavoriteAgentSessions", false),
            require_resume_reference: flag("autoSleepRequireAgentResumeCommand", true),
        }
    }
}

/// `(projectId, sessionId)` of a presentation session.
pub type SessionRef = (String, String);

/// The sessions to sleep, in the snapshot's session order, from a presentation snapshot
/// (`{ sessions: { rows }, groups: { rows } }` or the flat arrays), the sessions some client
/// holds awake, and the settings.
pub fn sessions_to_sleep(
    snapshot: &Value,
    held: &dyn Fn(&str, &str) -> bool,
    settings: AutoSleepSettings,
    now_ms: i64,
) -> Vec<SessionRef> {
    if settings.idle_minutes == 0 {
        return Vec::new();
    }
    let sessions = rows(snapshot, "sessions");
    let protected = protected_sessions(snapshot, held);
    sessions
        .iter()
        .filter_map(|session| {
            let key = session_ref(session)?;
            (!protected.contains(&key) && !held(&key.0, &key.1))
                .then_some(())
                .filter(|_| should_sleep(session, settings, now_ms))
                .map(|_| key)
        })
        .collect()
}

/// `collectGpuiAutoSleepProtectedProjectSessionKeys`, less what the leases already say: for each
/// group, a group whose rows nobody shows keeps its FIRST row awake.
fn protected_sessions(snapshot: &Value, held: &dyn Fn(&str, &str) -> bool) -> HashSet<SessionRef> {
    let mut protected = HashSet::new();
    for group in rows(snapshot, "groups") {
        let Some(project_id) = group.get("projectId").and_then(Value::as_str) else {
            continue;
        };
        let session_ids: Vec<&str> = group
            .get("sessionIds")
            .and_then(Value::as_array)
            .map(|ids| ids.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let has_owner = session_ids
            .iter()
            .any(|session_id| held(project_id, session_id));
        if !has_owner {
            if let Some(first) = session_ids.first() {
                protected.insert((project_id.to_string(), first.to_string()));
            }
        }
    }
    protected
}

/// `shouldAutoSleepGpuiPresentationAgentSession`, less the protected-set test.
pub fn should_sleep(session: &Value, settings: AutoSleepSettings, now_ms: i64) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    let non_empty = |key: &str| text(key).is_some_and(|value| !value.trim().is_empty());
    if text("lifecycleState") != Some("running") || text("activity") != Some("idle") {
        return false;
    }
    let can_sleep = session
        .get("actions")
        .and_then(|actions| actions.get("sleep"))
        .and_then(Value::as_bool)
        == Some(true);
    if !can_sleep || !is_agent_terminal(session) {
        return false;
    }
    // CDXC:SessionSleep 2026-08-22: a session nobody has prompted has no idle clock; gxserver declines it too (`neverActive`).
    if session.get("hasEverBeenActive").and_then(Value::as_bool) != Some(true) {
        return false;
    }
    // CDXC:SessionSleep 2026-09-24 DECISION: User: a session whose agent still has a background shell or monitor running (the grey dot) is not inactive.
    if non_empty("backgroundWorkDetectedAt") {
        return false;
    }
    // CDXC:DelayedSend 2026-08-20: an armed daemon-owned Delayed Send means a prompt is queued for this terminal.
    if non_empty("delayedSendDeadlineAt")
        || session
            .get("delayedSendRemainingMs")
            .is_some_and(|value| !value.is_null())
        || session
            .get("sendWhenAgentStopsActive")
            .and_then(Value::as_bool)
            == Some(true)
        || session
            .get("sendWhenAllProjectSessionsStopActive")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return false;
    }
    // CDXC:SessionChat 2026-08-21: queued chat prompts (not counting failed rows) mean the scheduler is about to hand the agent more work.
    let count = |key: &str| session.get(key).and_then(Value::as_i64).unwrap_or(0);
    if count("queuedPromptCount") - count("queuedPromptFailedCount") > 0 {
        return false;
    }
    if session.get("isFavorite").and_then(Value::as_bool) == Some(true)
        && !settings.favorite_sessions
    {
        return false;
    }
    if settings.require_resume_reference
        && !(non_empty("agentSessionId")
            || non_empty("agentSessionPath")
            || non_empty("trustedResumeTitle"))
    {
        return false;
    }
    let Some(last_activity_ms) = text("lastActiveAt")
        .or_else(|| text("updatedAt"))
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.timestamp_millis())
    else {
        return false;
    };
    now_ms - last_activity_ms >= settings.idle_minutes * MINUTE_MS
}

/// `isGpuiAutoSleepAgentTerminalSession`.
fn is_agent_terminal(session: &Value) -> bool {
    let text = |key: &str| session.get(key).and_then(Value::as_str);
    if !matches!(text("surface"), Some("workspace" | "commands")) {
        return false;
    }
    if text("kind") == Some("agent") {
        return true;
    }
    ["agentId", "agentName", "agentSessionId", "agentSessionPath"]
        .iter()
        .any(|key| text(key).is_some_and(|value| !value.trim().is_empty()))
}

fn session_ref(session: &Value) -> Option<SessionRef> {
    let project_id = session.get("projectId").and_then(Value::as_str)?;
    let session_id = session.get("sessionId").and_then(Value::as_str)?;
    Some((project_id.to_string(), session_id.to_string()))
}

/// A snapshot collection, whether the snapshot wraps it as `{ rows }` or holds the array itself.
fn rows<'a>(snapshot: &'a Value, key: &str) -> &'a [Value] {
    let collection = snapshot.get(key);
    collection
        .and_then(|value| value.get("rows"))
        .or(collection)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

/// Holder id prefixes of the clients that report every session they show (the desktop app today;
/// the GPUI web build when it reports too). The phone's `mobile-` holds are attach leases, not a
/// full report, and do not count.
const SHOWN_SESSIONS_REPORTER_PREFIXES: [&str; 2] = ["desktop-", "web-"];

/// How recent a report must be for its client to count as reporting (one lease).
const SHOWN_SESSIONS_REPORT_FRESH_MS: i64 = 180_000;

fn last_shown_sessions_report_ms() -> &'static std::sync::atomic::AtomicI64 {
    static LAST: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    &LAST
}

/// A `/api/holdSessionsAwake` call arrived from this holder.
pub fn note_shown_sessions_report(holder_id: &str) {
    if SHOWN_SESSIONS_REPORTER_PREFIXES
        .iter()
        .any(|prefix| holder_id.starts_with(prefix))
    {
        last_shown_sessions_report_ms().store(
            chrono::Utc::now().timestamp_millis(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// Whether the sweep may run now.
///
/// CDXC:SessionSleep 2026-09-25 WHY:
/// A connected client that does not report what it shows (an app built before this sweep moved here, which still runs its own) could be looking at any idle session, so the sweep waits while one is connected and no reporting client has spoken within a lease. With no client connected at all (the app is closed), nothing is on screen and the sweep runs.
pub fn sweep_allowed(open_event_streams: usize, now_ms: i64) -> bool {
    if open_event_streams == 0 {
        return true;
    }
    let last = last_shown_sessions_report_ms().load(std::sync::atomic::Ordering::Relaxed);
    last > 0 && now_ms - last <= SHOWN_SESSIONS_REPORT_FRESH_MS
}
