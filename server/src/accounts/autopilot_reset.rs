//! Autopilot's "Use a limit reset when an account hits its limit" (autopilot/config.rs
//! `use_limit_resets`, off by default): a Claude account that a session Autopilot covers is stuck
//! on, with no other account that has room, uses one of its banked limit resets.
//!
//! CDXC:Security 2026-11-21 DECISION:
//! Fable's security review of the first build (commit 9920661b5) found it could spend resets nobody needed and spend a second one on a stale reading. The coordinator's answers, for the user: a reset is spent only when a session Autopilot covers is waiting on this account's usage limit and no eligible account has room (account switching comes first, finding 3); Anthropic's own reset status is read fresh right before the claim and must say the account is at a limit and the grant is usable now, and that answer's count is the truth (finding 2, `claude_resets::fresh_grant`); the Fable reset clears exactly Anthropic's `seven_day_overage_included` and any other model's weekly limit counts as not cleared (finding 6); a used-up limit with no reset time means don't spend (finding 7); and the security log names what was attempted, "Use a Claude limit reset for …", whatever came of it (finding 9).
//! SEE-ALSO: server/src/accounts/reset_watch.rs (the 5-minute pass that calls `check`), server/src/server/autopilot_http.rs (who may turn the switch on).

use super::{
    claude_resets, launch,
    model::{
        DiscoveredAccount, Provider, Registry, ResetCredit, SavedAccount, Snapshot, UsageWindow,
    },
    recovery, reset_claim,
};
use crate::{
    domain::DomainRepository,
    logging::{GxserverLogInput, LogLevel},
    notification_feed::store::{
        account_notification_session_id, insert_notification_feed_row, NewNotificationFeedRow,
        ACCOUNT_NOTIFICATION_PROJECT_ID, NOTIFICATION_FEED_KIND_RESET_EXPIRING,
        NOTIFICATION_FEED_KIND_RESET_REDEEMED,
    },
    server::AppState,
};
use chrono::{DateTime, Duration as Span, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// After a claim reached Anthropic, the account is not tried again for this long.
const AFTER_CLAIM: Duration = Duration::from_secs(10 * 60);
/// After Anthropic's fresh status said no (or could not be read), the next look waits this long,
/// so the usage endpoint, which rate-limits per account, is not asked on every pass; the answer
/// itself stays in claude_resets.rs's reading for 15 minutes.
const AFTER_NO: Duration = Duration::from_secs(12 * 60);

/// What one look at an account came to.
pub(crate) enum Check {
    /// The switch is off, Autopilot is off, or this is not a Claude account.
    Off,
    /// A reset was used.
    Used,
    /// Nothing was spent; `posted` when a row went to the bell.
    NotNow { reason: String, posted: bool },
}

fn not_now(reason: impl Into<String>) -> Check {
    Check::NotNow {
        reason: reason.into(),
        posted: false,
    }
}

/// When each account was last tried, and how long until the next try.
static TRIED: Mutex<Option<HashMap<String, (Instant, Duration)>>> = Mutex::new(None);

/// Takes the account's turn under one lock: `false` while an earlier try still holds it. The 5-minute
/// pass and the at-limit check can look at the same account at once, and only one of them may go on
/// to ask Anthropic (Fable's re-check, 2026-11-21). The turn is held as if a claim ran until
/// `remember_try` sets how long the outcome keeps it.
fn take_turn(account_id: &str) -> bool {
    let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
    let tried = tried.get_or_insert_with(HashMap::new);
    if tried
        .get(account_id)
        .is_some_and(|(at, wait)| at.elapsed() < *wait)
    {
        return false;
    }
    tried.insert(account_id.to_string(), (Instant::now(), AFTER_CLAIM));
    true
}

fn remember_try(account_id: &str, wait: Duration) {
    TRIED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get_or_insert_with(HashMap::new)
        .insert(account_id.to_string(), (Instant::now(), wait));
}

/// Anthropic's name for a used-up limit Ghostex reads: `None` for one it cannot name, which no
/// reset is taken to clear.
fn anthropic_limit(window: &UsageWindow) -> Option<&'static str> {
    match window.model.as_deref() {
        Some(model) if model.eq_ignore_ascii_case("fable") => Some("seven_day_overage_included"),
        Some(_) => None,
        None => match window.id.as_str() {
            "fiveHour" => Some("five_hour"),
            "sevenDay" => Some("seven_day"),
            _ => None,
        },
    }
}

/// Whether a reset clears every used-up limit.
fn frees(credit: &ResetCredit, limited: &[&UsageWindow]) -> bool {
    let Some(clears) = &credit.clears else {
        return false;
    };
    limited.iter().all(|window| {
        anthropic_limit(window).is_some_and(|name| clears.iter().any(|limit| limit == name))
    })
}

/// The reset Autopilot would use from the account's last reading: the soonest-expiring one that
/// clears every used-up limit. Refused while the reading is older than 10 minutes, while no limit
/// is used up, when a used-up limit has no reset time, and when the limit lifts on its own within
/// 30 minutes.
fn pick(account: &DiscoveredAccount, now: DateTime<Utc>) -> Result<ResetCredit, &'static str> {
    let fresh = account.status == "ready"
        && account.usage_error.is_none()
        && !account.usage_stale
        && account
            .usage_updated_at
            .as_deref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .is_some_and(|t| now - t.with_timezone(&Utc) < Span::minutes(10));
    if !fresh {
        return Err("Its usage reading is not current.");
    }
    let limited: Vec<&UsageWindow> = account
        .usage
        .iter()
        .filter(|w| w.id != "spend" && w.used_percent >= 100.)
        .collect();
    if limited.is_empty() {
        return Err("It is not at a usage limit.");
    }
    let Some(lifts_at) = limited
        .iter()
        .map(|w| {
            w.resets_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc))
        })
        .collect::<Option<Vec<_>>>()
        .and_then(|times| times.into_iter().max())
    else {
        return Err("A used-up limit has no reset time.");
    };
    if lifts_at <= now + Span::minutes(30) {
        return Err("The limit lifts on its own within 30 minutes.");
    }
    account
        .reset_credit_details
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter(|credit| !credit.paused)
        .filter(|credit| {
            credit
                .expires_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .is_some_and(|t| t.with_timezone(&Utc) > now)
        })
        .find(|credit| frees(credit, &limited))
        .cloned()
        .ok_or("No reset it has clears the used-up limit.")
}

/// A session Autopilot covers that waits at this account's usage limit with no eligible account
/// that has room, as `<projectId>:<sessionId>`.
fn waiting_session(
    state: &AppState,
    registry: &Registry,
    snapshot: &Snapshot,
    saved: &SavedAccount,
) -> Option<String> {
    let config = crate::autopilot::read_config(&state.paths);
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let sessions = repository
        .list_sessions_with_lifecycle_state("running")
        .ok()?;
    sessions.into_iter().find_map(|session| {
        let project_id = session["projectId"].as_str()?;
        let session_id = session["sessionId"].as_str()?;
        if session["kind"].as_str() != Some("agent") {
            return None;
        }
        let project = repository.get_project(project_id).ok()??;
        if launch::provider(&project, &session) != Some(saved.provider)
            || launch::session_uses_own_login(saved.provider, &session)
        {
            return None;
        }
        let on_account = super::session_identity::display_account_id(
            registry,
            saved.provider,
            &session,
            &state.paths.home_dir,
        ) == Some(saved.id.as_str());
        let at_limit = crate::session_chat_options::cached_session_chat_terminal_notice(
            state, project_id, session_id,
        )
        .is_some_and(|notice| notice.kind == "usageLimit");
        let covered = config.covers(
            project_id,
            crate::autopilot::main_project_of(&project).as_deref(),
            session_id,
        );
        if !(on_account && at_limit && covered) {
            return None;
        }
        let policy = launch::effective_policy(registry, saved.provider, &session);
        let room = !recovery::ranked(
            registry,
            snapshot,
            saved.provider,
            Some(saved.id.as_str()),
            "",
            policy.priority,
        )
        .is_empty();
        (!room).then(|| format!("{project_id}:{session_id}"))
    })
}

/// Whether Anthropic's fresh answer lets the claim go ahead: the account is at a limit now, the
/// grant is usable now with a reset left, and it clears every limit Anthropic says is used up.
fn anthropic_allows(fresh: &claude_resets::FreshGrant) -> bool {
    fresh.at_limit
        && fresh.usable_now
        && !fresh.paused
        && fresh.resets_left > 0
        && !fresh.exhausted.is_empty()
        && fresh
            .exhausted
            .iter()
            .all(|limit| fresh.clears.contains(limit))
}

/// BLOCKING (a session scan, then at most one read of Anthropic's reset status and one claim).
/// One look at a saved account: uses a reset when the switch is on and every condition holds.
/// `dry_run` stops before Anthropic is asked anything.
pub(crate) fn check(
    state: &AppState,
    registry: &Registry,
    snapshot: &Snapshot,
    saved: &SavedAccount,
    account: &DiscoveredAccount,
    dry_run: bool,
) -> Check {
    let config = crate::autopilot::read_config(&state.paths);
    if !(config.enabled && config.use_limit_resets) || saved.provider != Provider::Claude {
        return Check::Off;
    }
    let credit = match pick(account, Utc::now()) {
        Ok(credit) => credit,
        Err(reason) => return not_now(reason),
    };
    let Some(waiting) = waiting_session(state, registry, snapshot, saved) else {
        return not_now(
            "No session Autopilot covers is waiting on its limit, or another account has room.",
        );
    };
    if dry_run {
        return not_now(format!(
            "Would ask Anthropic about reset {} for session {waiting}.",
            credit.id
        ));
    }
    if !take_turn(&saved.id) {
        return not_now("Tried a few minutes ago, or another check is asking now.");
    }
    let home = &state.paths.home_dir;
    let slot = match claude_resets::ClaudeSlot::read(home, &saved.selector) {
        Ok(slot) if slot.identity() == saved.identity => slot,
        _ => {
            remember_try(&saved.id, AFTER_NO);
            return not_now("The saved account changed. Refresh Accounts.");
        }
    };
    let fresh = match claude_resets::fresh_grant(home, &slot, &credit.id) {
        Ok(Some(fresh)) => fresh,
        Ok(None) => {
            remember_try(&saved.id, AFTER_NO);
            return not_now("Anthropic no longer offers this reset.");
        }
        Err(error) => {
            remember_try(&saved.id, AFTER_NO);
            return not_now(error);
        }
    };
    if !anthropic_allows(&fresh) {
        remember_try(&saved.id, AFTER_NO);
        return not_now("Anthropic says the account is not at a limit this reset clears, or the reset can't be used now.");
    }
    remember_try(&saved.id, AFTER_CLAIM);
    // Anthropic's count makes the key: a retry after a lost reply replays the same claim, and a
    // claim that went through leaves a fresh status that is no longer at the limit.
    let request_id = format!(
        "autopilot-{}",
        &hex::encode(Sha256::digest(
            format!("{}|{}|{}", saved.id, credit.id, fresh.resets_left).as_bytes()
        ))[..32]
    );
    let (kind, message) =
        match reset_claim::claim_for_account(state, saved, &credit.id, &request_id) {
            Ok(outcome) => (outcome.kind(), outcome.message().to_string()),
            Err(error) => ("failed", error.message),
        };
    crate::audit::record(
        crate::audit::Action::new(
            "autopilot.useLimitReset",
            format!(
                "Use a Claude limit reset for {}: session {waiting} waits on its usage limit and no other account has room",
                saved.name
            ),
        )
        .target(saved.id.clone()),
        crate::audit::Actor::Autopilot,
        if kind == "success" {
            crate::audit::Outcome::Done
        } else {
            crate::audit::Outcome::Failed(message.clone())
        },
        None,
    );
    let _ = state.logger.log(GxserverLogInput {
        level: if kind == "failed" {
            LogLevel::Warn
        } else {
            LogLevel::Info
        },
        event: "autopilotLimitReset".to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: (kind == "failed").then(|| message.clone()),
        details: Some(json!({
            "accountId": saved.id,
            "creditId": credit.id,
            "session": waiting,
            "outcome": kind,
        })),
    });
    // A claim Anthropic answered without spending anything (not at a limit, no longer offered)
    // stays in the logs; a used reset and a failed claim go to the bell.
    let posted = match kind {
        "success" => bell(
            state,
            saved,
            NOTIFICATION_FEED_KIND_RESET_REDEEMED,
            "Autopilot used a Claude limit reset".to_string(),
            format!(
                "A session waited on {}'s usage limit and no other account had room, so Autopilot used one of its limit resets ({} left). Its limits are fresh again.",
                saved.name,
                fresh.resets_left.saturating_sub(1)
            ),
        ),
        "failed" => bell(
            state,
            saved,
            NOTIFICATION_FEED_KIND_RESET_EXPIRING,
            "Autopilot couldn't use a Claude limit reset".to_string(),
            message.clone(),
        ),
        _ => false,
    };
    if kind == "success" {
        Check::Used
    } else {
        Check::NotNow {
            reason: message,
            posted,
        }
    }
}

fn bell(
    state: &AppState,
    saved: &SavedAccount,
    kind: &'static str,
    title: String,
    body: String,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return false;
    };
    insert_notification_feed_row(
        &db,
        NewNotificationFeedRow {
            project_id: ACCOUNT_NOTIFICATION_PROJECT_ID.into(),
            session_id: account_notification_session_id(&saved.id),
            kind,
            title,
            subtitle: format!("Claude · {}", saved.name),
            body,
            agent_name: Some(saved.provider.id().into()),
            attention_event_id: None,
        },
    )
    .is_ok()
}

/// BLOCKING. The check for the account a session runs on, at the moment Autopilot finds the
/// session stuck at its usage limit with no account that has room (server/src/server/
/// autopilot_runtime.rs, `AutopilotSwitch::NoRoom`). `true` when a reset was used; the session then
/// carries on by itself (reset_used.rs).
///
/// CDXC:Autopilot 2026-11-21 DECISION:
/// Coordinator, for the user: "run the reset check right when a covered session hits its limit (not only every 5 minutes)", after the security fixes. Autopilot's sweep sees a session at its limit within its sweep (2 minutes by default, at once when a turn ends), tries to move it, and only when no account has room asks here; every condition of `check` still holds, and the 5-minute pass in reset_watch.rs stays as the retry.
pub(crate) fn check_session(state: &AppState, project_id: &str, session_id: &str) -> bool {
    let config = crate::autopilot::read_config(&state.paths);
    if !(config.enabled && config.use_limit_resets) {
        return false;
    }
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return false;
    };
    let Ok(registry) = super::store::read(&db) else {
        return false;
    };
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let (Ok(Some(session)), Ok(Some(project))) = (
        repository.get_session(project_id, session_id),
        repository.get_project(project_id),
    ) else {
        return false;
    };
    if launch::provider(&project, &session) != Some(Provider::Claude) {
        return false;
    }
    let Some(account_id) = super::session_identity::display_account_id(
        &registry,
        Provider::Claude,
        &session,
        &state.paths.home_dir,
    )
    .map(str::to_string) else {
        return false;
    };
    let Some(saved) = registry.accounts.iter().find(|a| a.id == account_id) else {
        return false;
    };
    let Some(account) = snapshot
        .accounts
        .iter()
        .find(|a| a.provider == Provider::Claude && super::endpoint::account_id(a) == account_id)
    else {
        return false;
    };
    let used = matches!(
        check(state, &registry, &snapshot, saved, account, false),
        Check::Used
    );
    if used {
        crate::notification_feed::broadcast_notification_feed_changed(state);
    }
    used
}
