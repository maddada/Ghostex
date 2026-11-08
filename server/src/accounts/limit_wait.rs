//! CDXC:AgentProviders 2026-10-11 DECISION:
//! User, about a session that sat quietly at its usage limit while every account was full: "dont know what u mean but fix this best way i trust u". While recovery waits with no account that has room (`NO_ROOM`), the session's usage-limit card says so in plain words (which limit, every account is full, and when work continues: the soonest reset among the accounts, or as soon as one has room), and the bell gets one row per wait, never one per pass. Both go away when the session switches, resumes or is stopped. The card's wording is gx-chat-core's (`questions/account_wait.rs`); gxserver only carries the facts on the notice as `accountWait`.
//! SEE-ALSO: server/src/session_chat_notice/watchdog_store.rs (`resolve_session_chat_terminal_notice` attaches the wait), server/src/session_chat_read.rs (the long-poll fingerprint), packages/gx-chat-core/src/questions/account_wait.rs.
use super::{endpoint, model::*};
use crate::{
    domain::DomainRepository,
    notification_feed::{
        broadcast_notification_feed_changed,
        store::{
            dismiss_notification, insert_notification_feed_row, notification_agent_icon,
            NewNotificationFeedRow, NOTIFICATION_FEED_KIND_CUSTOM,
        },
    },
    server::AppState,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

/// The recovery reason of a usage-limit wait with no account that has room.
pub(super) const NO_ROOM: &str = "Waiting for an eligible account to have capacity.";

/// The waits in progress, by session, as saved in `accountRecovery.accountWait`: the card's facts plus the bell row's id.
fn waits() -> &'static Mutex<HashMap<(String, String), Value>> {
    static WAITS: OnceLock<Mutex<HashMap<(String, String), Value>>> = OnceLock::new();
    WAITS.get_or_init(|| Mutex::new(HashMap::new()))
}
fn key(project: &str, session: &str) -> (String, String) {
    (project.to_string(), session.to_string())
}

/// The sessions with a wait in progress, for the recovery pass to settle the ones it does not plan.
pub(super) fn waiting_sessions() -> Vec<(String, String)> {
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .cloned()
        .collect()
}

/// What the session's usage-limit notice carries while it waits: `limit`, `resetsAt` and `account`, each optional.
pub(crate) fn notice_wait(project: &str, session: &str) -> Option<Value> {
    let waits = waits().lock().unwrap_or_else(|e| e.into_inner());
    let wait = waits.get(&key(project, session))?;
    Some(json!({
        "limit": wait["limit"],
        "resetsAt": wait["resetsAt"],
        "account": wait["account"],
    }))
}

/// The facts of a wait: the limit the current account hit, and the eligible account (the current one included) whose full windows reset soonest.
pub(super) fn describe(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: Option<&str>,
    model: &str,
    now: DateTime<Utc>,
) -> Value {
    let reading = |id: &str| {
        snapshot
            .accounts
            .iter()
            .find(|d| endpoint::account_id(d) == id)
            .filter(|d| d.status == "ready" && d.usage_error.is_none())
    };
    let resets = |w: &UsageWindow| {
        w.resets_at
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
    };
    let full = |d: &'_ DiscoveredAccount| -> Vec<UsageWindow> {
        d.usage
            .iter()
            .filter(|w| super::recovery::relevant(w, model) && w.used_percent >= 100.)
            .cloned()
            .collect()
    };
    let limit = current.and_then(reading).and_then(|d| {
        full(d)
            .into_iter()
            .max_by_key(|w| resets(w))
            .map(|w| format!("{} limit", w.label))
    });
    // An account frees up when every window it has used up has reset.
    let soonest = registry
        .accounts
        .iter()
        .filter(|a| {
            a.provider == provider && (a.auto_switchable() || Some(a.id.as_str()) == current)
        })
        .filter_map(|a| {
            let windows = full(reading(&a.id)?);
            let free_at = windows
                .iter()
                .map(&resets)
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .max()?;
            (free_at > now).then_some((free_at, a.name.clone()))
        })
        .min_by_key(|(at, _)| *at);
    json!({
        "limit": limit,
        "resetsAt": soonest.as_ref().map(|(at, _)| at.to_rfc3339()),
        "account": soonest.map(|(_, name)| name),
    })
}

/// Enters or refreshes the wait on `recovery` (saved by the caller). The first entry of a wait posts its bell row; later passes keep that row.
pub(super) fn enter(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    recovery: &mut Value,
    mut wait: Value,
) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    let posted = recovery
        .pointer("/accountWait/notificationId")
        .and_then(Value::as_str)
        .map(str::to_string);
    // When the wait began, kept across passes: the blocked red dot's `blockedSince`.
    let since = recovery
        .pointer("/accountWait/since")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let first = posted.is_none();
    let notification = posted.or_else(|| {
        let subtitle = repo
            .get_project(pid)
            .ok()
            .flatten()
            .and_then(|p| p["name"].as_str().map(str::to_string))
            .unwrap_or_default();
        let item = insert_notification_feed_row(
            repo.db,
            NewNotificationFeedRow {
                project_id: pid.to_string(),
                session_id: sid.to_string(),
                kind: NOTIFICATION_FEED_KIND_CUSTOM,
                title: session["title"]
                    .as_str()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .unwrap_or("Session")
                    .to_string(),
                subtitle,
                body: bell_body(&wait),
                agent_name: notification_agent_icon(session),
                attention_event_id: None,
            },
        )
        .ok()?;
        broadcast_notification_feed_changed(state);
        item["id"].as_str().map(str::to_string)
    });
    if first {
        if let Some(id) = notification.as_deref() {
            crate::push::announce_blocked(state, repo, session, id, crate::push::BlockedWhy::Usage);
        }
    }
    wait["notificationId"] = json!(notification);
    wait["since"] = json!(since);
    recovery["accountWait"] = wait.clone();
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key(pid, sid), wait);
}

/// Ends the wait once the saved recovery no longer waits for room: a switch, a resume, Stop, or another state. Removes its bell row if still there.
pub(super) fn settle(state: &AppState, repo: &DomainRepository<'_>, pid: &str, sid: &str) {
    let Some(wait) = waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key(pid, sid))
        .cloned()
    else {
        return;
    };
    let session = match repo.get_session(pid, sid) {
        Ok(session) => session,
        Err(_) => return,
    };
    if let Some(saved) = session.as_ref().and_then(waiting) {
        waits()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key(pid, sid), saved);
        return;
    }
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&key(pid, sid));
    if let Some(id) = wait["notificationId"].as_str() {
        if dismiss_notification(repo.db, id).is_ok_and(|removed| removed > 0) {
            broadcast_notification_feed_changed(state);
        }
    }
}

/// After a restart: a wait saved before it shows again at once, and one that ended while gxserver was down loses its bell row on the next `settle`.
pub(super) fn restore(session: &Value) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    if let Some(wait) = session
        .pointer("/runtimeSettings/accountRecovery/accountWait")
        .filter(|w| w.is_object())
    {
        waits()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key(pid, sid), wait.clone());
    }
}

/// Whether a recovery state is a usage limit recovery gave up on: no account it could move the
/// session to (`needsAttention` with the `usageLimit` trigger). Nothing retries it, and no failed
/// row covers it, since a usage limit is no error.
pub(crate) fn limit_gave_up(recovery: &Value) -> bool {
    recovery["status"] == "needsAttention" && recovery["trigger"] == "usageLimit"
}

/// The bell row and the phone push for a usage limit recovery just gave up on: blocked, which
/// always alerts (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs).
pub(super) fn announce_limit_gave_up(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    let subtitle = repo
        .get_project(pid)
        .ok()
        .flatten()
        .and_then(|p| p["name"].as_str().map(str::to_string))
        .unwrap_or_default();
    let Ok(item) = insert_notification_feed_row(
        repo.db,
        NewNotificationFeedRow {
            project_id: pid.to_string(),
            session_id: sid.to_string(),
            kind: NOTIFICATION_FEED_KIND_CUSTOM,
            title: session["title"]
                .as_str()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .unwrap_or("Session")
                .to_string(),
            subtitle,
            body: LIMIT_GAVE_UP_REASON.to_string(),
            agent_name: notification_agent_icon(session),
            attention_event_id: None,
        },
    ) else {
        return;
    };
    broadcast_notification_feed_changed(state);
    if let Some(id) = item["id"].as_str() {
        crate::push::announce_blocked(state, repo, session, id, crate::push::BlockedWhy::Usage);
    }
}

/// What a session blocked at a usage limit recovery gave up on says, on its red dot and its row.
pub(crate) const LIMIT_GAVE_UP_REASON: &str =
    "Blocked: it hit its usage limit and Ghostex couldn't move it to an account with room.";

/// Since when the session waits with every account out of usage: the blocked red dot
/// (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs).
pub(crate) fn blocked_since(session: &Value) -> Option<String> {
    let wait = waiting(session)?;
    wait["since"]
        .as_str()
        .or_else(|| {
            session
                .pointer("/runtimeSettings/accountRecovery/updatedAt")
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

/// The saved wait, while the session's recovery is still waiting for an account with room.
fn waiting(session: &Value) -> Option<Value> {
    let recovery = session.pointer("/runtimeSettings/accountRecovery")?;
    (recovery["status"] == "waiting" && recovery["reason"] == NO_ROOM)
        .then(|| recovery["accountWait"].clone())
        .filter(Value::is_object)
}

fn bell_body(wait: &Value) -> String {
    let limit = wait["limit"]
        .as_str()
        .map(|l| l.to_lowercase())
        .unwrap_or_else(|| "usage limit".to_string());
    let when = match (
        wait["resetsAt"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok()),
        wait["account"].as_str(),
    ) {
        (Some(at), Some(account)) => format!(
            "It continues automatically {}, when {account}'s limit resets.",
            local_time(at.with_timezone(&Utc))
        ),
        _ => "It continues automatically as soon as an account has room.".to_string(),
    };
    format!("Hit its {limit}, and every account is full. {when}")
}

fn local_time(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let clock = local.format("%-I:%M %p");
    if local.date_naive() == today {
        format!("at {clock}")
    } else if Some(local.date_naive()) == today.succ_opt() {
        format!("tomorrow at {clock}")
    } else {
        format!("on {} at {clock}", local.format("%b %-d"))
    }
}
