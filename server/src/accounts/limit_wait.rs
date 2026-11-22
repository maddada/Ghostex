//! CDXC:AgentProviders 2026-10-11 DECISION:
//! User, about a session that sat quietly at its usage limit while every account was full: "dont know what u mean but fix this best way i trust u". While recovery waits with no account that has room (`NO_ROOM`), the session's usage-limit card says so in plain words (which limit, every account is full, and when work continues: the soonest reset among the accounts, or as soon as one has room), and the bell gets one row per wait, never one per pass. Both go away when the session switches, resumes or is stopped. The card's wording is gx-chat-core's (`questions/account_wait.rs`); gxserver only carries the facts on the notice as `accountWait`.
//! SEE-ALSO: server/src/session_chat_notice/watchdog_store.rs (`resolve_session_chat_terminal_notice` attaches the wait), server/src/session_chat_read.rs (the long-poll fingerprint), packages/gx-chat-core/src/questions/account_wait.rs.
use super::{endpoint, launch, model::*};
use crate::{
    domain::{DomainRepository, DomainStateError},
    notification_feed::{
        broadcast_notification_feed_changed,
        store::{
            account_notification_session_id, dismiss_notification, insert_notification_feed_row,
            mark_notification_read, notification_agent_icon, NewNotificationFeedRow,
            ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_CUSTOM,
            NOTIFICATION_FEED_KIND_FAILED,
        },
    },
    server::AppState,
};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, OnceLock},
};

/// The recovery reason of a usage-limit wait with no account that has room.
pub(crate) const NO_ROOM: &str = "Waiting for an eligible account to have capacity.";

/// `blockedAlertedBy` stamped on sessions blocked by an exhausted account pool: the pool's own
/// single notification alerts once, so clients draw each session's red dot without firing a
/// separate sound and banner per chat (Q4472).
pub(crate) const POOL_BLOCKED_ALERTED_BY: &str = "accountPool";

const POOL_EPISODES_KEY: &str = "accounts.poolLimitEpisodes.v1";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PoolEpisode {
    #[serde(default)]
    resets_at: Option<String>,
    #[serde(default)]
    account_name: Option<String>,
    #[serde(default)]
    notification_id: Option<String>,
    since: String,
    #[serde(default)]
    acknowledged_at: Option<String>,
}

fn read_pool_episodes(db: &Connection) -> BTreeMap<String, PoolEpisode> {
    db.query_row(
        "SELECT value FROM metadata WHERE key = ?1",
        [POOL_EPISODES_KEY],
        |row| row.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|raw| serde_json::from_str::<BTreeMap<String, PoolEpisode>>(&raw).ok())
    .unwrap_or_default()
}

fn write_pool_episodes(db: &Connection, episodes: &BTreeMap<String, PoolEpisode>) {
    let Ok(raw) = serde_json::to_string(episodes) else {
        return;
    };
    let _ = db.execute(
        "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
        rusqlite::params![POOL_EPISODES_KEY, raw, Utc::now().to_rfc3339()],
    );
}

fn provider_title_name(provider: Provider) -> &'static str {
    match provider {
        Provider::Claude => "Claude",
        Provider::Codex => "Codex",
    }
}

fn pool_notification_session_id(provider: Provider) -> String {
    account_notification_session_id(&format!("pool:{}", provider.id()))
}

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
    // Everything but the bell row's bookkeeping: `model`, `accountLimit`, `blocked` and
    // `alternative` joined the first three on 2026-11-13 (model_fallback.rs); a client that
    // predates them reads the first three as before.
    let mut facts = wait.clone();
    if let Some(fields) = facts.as_object_mut() {
        fields.remove("notificationId");
        fields.remove("since");
    }
    Some(facts)
}

/// The facts of a wait: the limit the current account hit, and the eligible account (the current one included) whose full windows reset soonest.
pub(super) fn describe(
    registry: &Registry,
    snapshot: &Snapshot,
    provider: Provider,
    current: Option<&str>,
    model: &str,
    model_label: Option<&str>,
    alternative: Option<&super::model_fallback::Alternative>,
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
            let last = windows.iter().max_by_key(|w| resets(w))?;
            (free_at > now).then_some((free_at, a.name.clone(), window_name(last)))
        })
        .min_by_key(|(at, _, _)| *at);
    // Why each other account of this provider cannot take the session: turned off here, set to
    // manual only, signed out (or never signed in on this computer), or a window this model counts
    // against used up. The card names them in one sentence, with a fix for the ones it can fix.
    //
    // CDXC:AgentProviders 2026-11-22 DECISION:
    // User, of a Windows session whose card said only "m•••m's 7d limit is full" while another of
    // their accounts had room: "i was confused why this session says it can't switch to another
    // account even tho i do have an account that has credits. then i remembered it's because i
    // didn't login to xyzt70 claude account or i didn't enable it there. pls make this clear in
    // the notice". So an account turned off here is named too (`off`, with Turn on), one synced
    // from another computer and never signed in here says so (`signIn` with `here: false`, with
    // Sign in), and a manual-only account says whether it has room (`room`, with Use it for this
    // session); a manual-only account whose window is used up is named by that window instead.
    // Each carries `accountId` for its fix. An account turned off after its subscription was
    // refused (`disabled`) stays out: turning it on would not help. Room is only claimed from this
    // computer's own reading: a turned-off or signed-out account has none here.
    let blocked: Vec<Value> = registry
        .accounts
        .iter()
        .filter(|a| a.provider == provider && Some(a.id.as_str()) != current)
        .filter(|a| !(a.user_disabled && a.disabled.is_some()))
        .filter_map(|a| {
            if a.user_disabled {
                return Some(json!({ "account": a.name, "accountId": a.id, "why": "off" }));
            }
            if a.login_expired_at.is_some() || a.selector.is_empty() {
                return Some(json!({
                    "account": a.name,
                    "accountId": a.id,
                    "why": "signIn",
                    "here": !a.selector.is_empty(),
                }));
            }
            let read = reading(&a.id);
            let windows = read.map(full).unwrap_or_default();
            if let Some(window) = windows
                .iter()
                .find(|w| w.model.is_some())
                .or_else(|| windows.first())
            {
                return Some(json!({
                    "account": a.name,
                    "accountId": a.id,
                    "why": "full",
                    "limit": window_name(window),
                }));
            }
            (!a.eligible).then(|| {
                json!({
                    "account": a.name,
                    "accountId": a.id,
                    "why": "manual",
                    "room": a.disabled.is_none()
                        && read.is_some_and(|d| super::recovery::trusted_room(d, model)),
                })
            })
        })
        .collect();
    json!({
        "provider": provider.id(),
        "limit": limit,
        "resetsAt": soonest.as_ref().map(|(at, _, _)| at.to_rfc3339()),
        "account": soonest.as_ref().map(|(_, name, _)| name.clone()),
        "accountLimit": soonest.map(|(_, _, limit)| limit),
        "model": model_label,
        "blocked": blocked,
        "alternative": alternative.map(super::model_fallback::Alternative::to_json),
    })
}

/// A window as the card names it: a model's own window by the model ("Fable"), others by their
/// label ("7d").
fn window_name(window: &UsageWindow) -> String {
    window
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| window.label.clone())
}

/// Enters or refreshes the wait on `recovery` (saved by the caller). Emits ONE per-pool bell row
/// and push notification per exhaustion episode (Q4472), never one per chat and never repeated on
/// later passes or restarts.
pub(super) fn enter(
    state: &AppState,
    repo: &DomainRepository<'_>,
    session: &Value,
    recovery: &mut Value,
    mut wait: Value,
    provider: Provider,
) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    let provider_key = provider.id().to_string();
    let mut episodes = read_pool_episodes(repo.db);
    let session_ack = recovery
        .pointer("/accountWait/acknowledgedAt")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut episodes_dirty = false;
    let episode = match episodes.get_mut(&provider_key) {
        Some(existing) => {
            if let Some(next_resets) = wait["resetsAt"].as_str() {
                let now = Utc::now();
                let next_at = DateTime::parse_from_rfc3339(next_resets)
                    .ok()
                    .map(|at| at.with_timezone(&Utc));
                let existing_at = existing
                    .resets_at
                    .as_deref()
                    .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                    .map(|at| at.with_timezone(&Utc));
                let replace_reset = match (existing_at, next_at) {
                    (None, Some(_)) => true,
                    (Some(cur), Some(next)) if cur <= now && next > now => true,
                    (Some(cur), Some(next)) if next > now && next < cur => true,
                    (None, None) if existing.resets_at.is_none() => true,
                    _ => false,
                };
                if replace_reset {
                    existing.resets_at = Some(next_resets.to_string());
                    if let Some(name) = wait["account"].as_str() {
                        existing.account_name = Some(name.to_string());
                    }
                    episodes_dirty = true;
                }
            }
            if existing.account_name.is_none() {
                if let Some(name) = wait["account"].as_str() {
                    existing.account_name = Some(name.to_string());
                    episodes_dirty = true;
                }
            }
            if existing.acknowledged_at.is_none() {
                if let Some(ack) = session_ack.clone() {
                    existing.acknowledged_at = Some(ack);
                    episodes_dirty = true;
                }
            }
            existing.clone()
        }
        None => {
            let since = recovery
                .pointer("/accountWait/since")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| Utc::now().to_rfc3339());
            let pool_session_id = pool_notification_session_id(provider);
            let notification_id = insert_notification_feed_row(
                repo.db,
                NewNotificationFeedRow {
                    project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.to_string(),
                    session_id: pool_session_id.clone(),
                    kind: NOTIFICATION_FEED_KIND_FAILED,
                    title: format!("{} accounts at limit", provider_title_name(provider)),
                    subtitle: pool_subtitle(&wait),
                    body: pool_body(&wait),
                    agent_name: Some(provider.id().to_string()),
                    attention_event_id: None,
                },
            )
            .ok()
            .and_then(|item| item["id"].as_str().map(str::to_string));
            if let Some(id) = notification_id.as_deref() {
                broadcast_notification_feed_changed(state);
                crate::push::announce_pool_blocked(
                    state,
                    provider_title_name(provider),
                    &pool_session_id,
                    id,
                );
            }
            let created = PoolEpisode {
                resets_at: wait["resetsAt"].as_str().map(str::to_string),
                account_name: wait["account"].as_str().map(str::to_string),
                notification_id,
                since,
                acknowledged_at: session_ack.clone(),
            };
            episodes.insert(provider_key, created.clone());
            episodes_dirty = true;
            created
        }
    };
    if episodes_dirty {
        write_pool_episodes(repo.db, &episodes);
    }
    wait["provider"] = json!(provider.id());
    wait["notificationId"] = json!(episode.notification_id);
    wait["since"] = json!(episode.since);
    if let Some(ack) = episode.acknowledged_at.or(session_ack) {
        wait["acknowledgedAt"] = json!(ack);
    }
    recovery["accountWait"] = wait.clone();
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key(pid, sid), wait);
}

/// Ends the wait once the saved recovery no longer waits for room: a switch, a resume, Stop, or
/// another state. Legacy per-session bell rows (without `provider`) are dismissed here; shared
/// per-pool rows are settled in `settle_pools`.
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
    if let Some(saved) = session.as_ref().and_then(shown) {
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
    if wait["provider"].is_null() {
        if let Some(id) = wait["notificationId"].as_str() {
            if dismiss_notification(repo.db, id).is_ok_and(|removed| removed > 0) {
                broadcast_notification_feed_changed(state);
            }
        }
    }
}

/// Settles per-pool exhaustion episodes once the provider has an eligible account with room again
/// or no session is waiting on that pool anymore. Dismisses the shared pool bell row so a future
/// exhaustion starts a fresh red episode.
pub(super) fn settle_pools(
    state: &AppState,
    repo: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
) {
    // Never settle persisted episodes before the first live usage discovery completes: an
    // unfetched snapshot treats every provider as having room and would wipe the episode and
    // re-fire bell/sound/banner/push on the next pass.
    if snapshot.fetched_at.is_none() {
        return;
    }
    let mut episodes = read_pool_episodes(repo.db);
    if episodes.is_empty() {
        return;
    }
    let mut active_providers: std::collections::HashSet<String> = waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .values()
        .filter_map(|wait| wait["provider"].as_str().map(str::to_string))
        .collect();
    // Also check persisted non-stopped sessions waiting or paused on a usage limit (including
    // `afterRestart` waits) so a restart never prematurely clears an active pool episode.
    if let Ok(sessions) = repo.list_sessions_excluding_stopped(None) {
        for session in sessions {
            let Some(recovery) = session.pointer("/runtimeSettings/accountRecovery") else {
                continue;
            };
            if recovery["trigger"] != crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT {
                continue;
            }
            if !matches!(recovery["status"].as_str(), Some("waiting" | "paused")) {
                continue;
            }
            if let Some(provider_id) = recovery
                .pointer("/accountWait/provider")
                .and_then(Value::as_str)
            {
                active_providers.insert(provider_id.to_string());
                continue;
            }
            if let Some(pid) = session["projectId"].as_str() {
                if let Some(provider) = repo
                    .get_project(pid)
                    .ok()
                    .flatten()
                    .and_then(|project| launch::provider(&project, &session))
                {
                    active_providers.insert(provider.id().to_string());
                }
            }
        }
    }
    let mut removed_any = false;
    let mut dismissed_any = false;
    episodes.retain(|provider_id, episode| {
        let provider = match provider_id.as_str() {
            "claude" => Some(Provider::Claude),
            "codex" => Some(Provider::Codex),
            _ => None,
        };
        let has_room = provider
            .is_some_and(|p| super::autopilot::provider_has_eligible_room(registry, snapshot, p));
        let still_waiting = active_providers.contains(provider_id);
        if !has_room && still_waiting {
            return true;
        }
        removed_any = true;
        if let Some(id) = episode.notification_id.as_deref() {
            if dismiss_notification(repo.db, id).is_ok_and(|removed| removed > 0) {
                dismissed_any = true;
            }
        }
        false
    });
    if removed_any {
        write_pool_episodes(repo.db, &episodes);
    }
    if dismissed_any {
        broadcast_notification_feed_changed(state);
    }
}

/// User clicked Wait or Dismiss on an every-account-full card (Q4472): clears the red blocked dot
/// into the calm amber waiting clock across the pool's waiting chats and marks the pool bell row
/// read, while keeping automatic resume at reset active.
pub(crate) fn acknowledge(
    state: &AppState,
    repo: &DomainRepository<'_>,
    provider: Provider,
    clicked: &Value,
) -> Result<(), DomainStateError> {
    let now = Utc::now().to_rfc3339();
    let provider_key = provider.id().to_string();
    let mut episodes = read_pool_episodes(repo.db);
    let mut notification_to_read: Option<String> = None;
    let mut episodes_dirty = false;
    if let Some(episode) = episodes.get_mut(&provider_key) {
        if episode.acknowledged_at.is_none() {
            episode.acknowledged_at = Some(now.clone());
            episodes_dirty = true;
        }
        notification_to_read = episode.notification_id.clone();
    }
    if episodes_dirty {
        write_pool_episodes(repo.db, &episodes);
    }
    if let Some(id) = notification_to_read.as_deref() {
        if mark_notification_read(repo.db, id).is_ok_and(|changed| changed > 0) {
            broadcast_notification_feed_changed(state);
        }
    }
    let mut to_update: Vec<Value> = Vec::new();
    let clicked_key = (
        clicked["projectId"].as_str().unwrap_or_default().to_string(),
        clicked["sessionId"].as_str().unwrap_or_default().to_string(),
    );
    for session in repo.list_sessions_excluding_stopped(None)? {
        let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        if waiting(&session).is_none() {
            continue;
        }
        let same_clicked = pid == clicked_key.0 && sid == clicked_key.1;
        let same_pool = session
            .pointer("/runtimeSettings/accountRecovery/accountWait/provider")
            .and_then(Value::as_str)
            == Some(provider.id())
            || repo
                .get_project(pid)
                .ok()
                .flatten()
                .and_then(|project| launch::provider(&project, &session))
                == Some(provider);
        if same_clicked || same_pool {
            to_update.push(session);
        }
    }
    for session in to_update {
        let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        let mut runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        let Some(recovery) = runtime
            .get_mut("accountRecovery")
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let Some(wait) = recovery.get_mut("accountWait").and_then(Value::as_object_mut) else {
            continue;
        };
        if wait.get("acknowledgedAt").and_then(Value::as_str).is_some() {
            continue;
        }
        wait.insert("acknowledgedAt".to_string(), json!(now));
        let wait_value = Value::Object(wait.clone());
        waits()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key(pid, sid), wait_value);
        let updated = endpoint::update_session_members(repo, &session, &runtime, &["accountRecovery"])?;
        endpoint::publish(state, repo, &updated)?;
        crate::session_chat_options::session_chat_terminal_notice_publisher(state, pid, sid)();
    }
    Ok(())
}

/// A usage-limit wait that no every-account-full card covers (Wait for reset, or a login Ghostex
/// does not track): the limit card says when Ghostex continues the session, `at`, and offers Stop
/// (CDXC:AgentProviders 2026-11-14 in recovery.rs, Q3229). No bell row or push: nothing is blocked,
/// the session waits as its settings say. Returns whether `recovery` changed and needs saving.
pub(super) fn enter_reset(
    session: &Value,
    recovery: &mut Value,
    at: Option<DateTime<Utc>>,
) -> bool {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return false;
    };
    let existing = recovery
        .get("accountWait")
        .filter(|w| w.is_object())
        .cloned();
    if existing.as_ref().is_some_and(|w| w["mode"] != "reset") {
        return false;
    }
    let resets_at = at.map(|t| t.to_rfc3339());
    let since = existing
        .as_ref()
        .and_then(|w| w["since"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let wait = json!({ "mode": "reset", "resetsAt": resets_at, "since": since });
    let changed = existing.as_ref() != Some(&wait);
    recovery["accountWait"] = wait.clone();
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key(pid, sid), wait);
    changed
}

/// Drops the session's wait (and any legacy per-session bell row), before its recovery is replaced
/// at startup. Shared per-pool rows are settled in `settle_pools`.
pub(super) fn forget(state: &AppState, repo: &DomainRepository<'_>, session: &Value) {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return;
    };
    waits()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&key(pid, sid));
    let is_pool = session
        .pointer("/runtimeSettings/accountRecovery/accountWait/provider")
        .and_then(Value::as_str)
        .is_some();
    if !is_pool {
        if let Some(id) = session
            .pointer("/runtimeSettings/accountRecovery/accountWait/notificationId")
            .and_then(Value::as_str)
        {
            if dismiss_notification(repo.db, id).is_ok_and(|removed| removed > 0) {
                broadcast_notification_feed_changed(state);
            }
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
        crate::push::announce_blocked(
            state,
            repo,
            session,
            id,
            crate::push::BlockedWhy::Usage,
            crate::push::PushRoute::default(),
        );
    }
}

/// What a session blocked at a usage limit recovery gave up on says, on its red dot and its row.
pub(crate) const LIMIT_GAVE_UP_REASON: &str =
    "Blocked: it hit its usage limit and Ghostex couldn't move it to an account with room.";

/// Since when the session waits with every account out of usage: the blocked red dot
/// (CDXC:Notifications 2026-11-08 in packages/gx-core/src/alert_rule.rs). Cleared once the user
/// clicks Wait or Dismiss (Q4472), turning the red dot into the calm amber waiting clock while
/// keeping automatic resume active.
pub(crate) fn blocked_since(session: &Value) -> Option<String> {
    let wait = waiting(session)?;
    if wait["acknowledgedAt"].is_string() {
        return None;
    }
    wait["since"]
        .as_str()
        .or_else(|| {
            session
                .pointer("/runtimeSettings/accountRecovery/updatedAt")
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

/// Whether this session's red blocked dot is already announced by the per-pool notification, so
/// clients draw the red dot without firing a separate sound and banner per chat (Q4472).
pub(crate) fn is_pool_blocked(session: &Value) -> bool {
    waiting(session)
        .is_some_and(|wait| wait["provider"].is_string() && !wait["acknowledgedAt"].is_string())
}

/// Additive sidebar presentation fields `(quotaResetsAt, quotaWaitState)` for sessions waiting on
/// an exhausted account pool or a reset wait (Q4472, harmonious with Antigravity Q4460/Q4461):
/// `"blocked"` while unacknowledged (red dot + amber countdown), `"waiting"` after Wait/Dismiss
/// or for a Wait-for-reset session (calm amber clock + countdown, no red dot).
pub(crate) fn presentation_fields(session: &Value) -> Option<(Option<String>, &'static str)> {
    if let Some(wait) = waiting(session) {
        let resets_at = wait["resetsAt"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let state = if wait["acknowledgedAt"].is_string() {
            "waiting"
        } else {
            "blocked"
        };
        return Some((resets_at, state));
    }
    let recovery = session.pointer("/runtimeSettings/accountRecovery")?;
    if recovery["status"] == "waiting" && recovery["accountWait"]["mode"] == "reset" {
        let resets_at = recovery["accountWait"]["resetsAt"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Some((resets_at, "waiting"));
    }
    None
}

/// The saved wait while the limit card should still show it: every account full, a wait for the
/// reset (`enter_reset`), or a wait a restart paused until the user continues it.
fn shown(session: &Value) -> Option<Value> {
    let recovery = session.pointer("/runtimeSettings/accountRecovery")?;
    let wait = recovery["accountWait"].clone();
    let keep = match recovery["status"].as_str() {
        Some("waiting") => recovery["reason"] == NO_ROOM || wait["mode"] == "reset",
        Some("paused") => wait["mode"] == "afterRestart",
        _ => false,
    };
    (keep && wait.is_object()).then_some(wait)
}

/// The saved wait, while the session's recovery is still waiting for an account with room.
fn waiting(session: &Value) -> Option<Value> {
    let recovery = session.pointer("/runtimeSettings/accountRecovery")?;
    (recovery["status"] == "waiting" && recovery["reason"] == NO_ROOM)
        .then(|| recovery["accountWait"].clone())
        .filter(Value::is_object)
}

fn pool_subtitle(wait: &Value) -> String {
    match wait["resetsAt"]
        .as_str()
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
    {
        Some(at) => format!("Paused until {}", local_clock_short(at.with_timezone(&Utc))),
        None => "Waiting to resume".to_string(),
    }
}

fn pool_body(wait: &Value) -> String {
    match (
        wait["resetsAt"]
            .as_str()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok()),
        wait["account"]
            .as_str()
            .map(str::trim)
            .filter(|name| !name.is_empty()),
    ) {
        (Some(at), Some(account)) => format!(
            "Paused until {} when {account} resets. Waiting to resume automatically.",
            local_clock_short(at.with_timezone(&Utc))
        ),
        (Some(at), None) => format!(
            "Paused until {}. Waiting to resume automatically.",
            local_clock_short(at.with_timezone(&Utc))
        ),
        (None, _) => {
            "Every automatic account is full. Waiting to resume automatically when one resets."
                .to_string()
        }
    }
}

fn local_clock_short(at: DateTime<Utc>) -> String {
    let local = at.with_timezone(&chrono::Local);
    let today = chrono::Local::now().date_naive();
    let clock = local.format("%-I:%M %p");
    if local.date_naive() == today {
        format!("{clock}")
    } else if Some(local.date_naive()) == today.succ_opt() {
        format!("tomorrow {clock}")
    } else {
        format!("{} {clock}", local.format("%b %-d"))
    }
}
