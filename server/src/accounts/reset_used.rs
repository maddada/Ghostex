//! "The usage limit is reset": one entry point that continues every session waiting on a login's
//! limit once the user used a reset, the coordinator first.
//!
//! CDXC:AgentProviders 2026-11-17 DECISION:
//! The user ran out of Codex usage, used a reset, and their coordinator stayed stuck: Codex slept inside its turn on the 429's Retry-After of several days, nothing told Ghostex about the reset, and Ghostex kept nudging and retrying into the same refusal. User, on the fix plan ("execute the entire plan"): when the user signals the reset, Ghostex interrupts each session still waiting on that login's limit and sends it one "after the usage limit reset" Continue, the coordinator before its threads. The signals are Ghostex's own Redeem succeeding (`reset_claim::claim_for_account`), a usage reading of a saved account dropping below 100% before its reset time (`runtime.rs` → `RecoverySignals::note_reset`), and the limit card's "I reset it, continue" for a login Ghostex cannot read. Ghostex still never interrupts on its own (CDXC:Coordinators 2026-11-05 in coordinators/stall.rs): a long sleep alone interrupts nothing.
//! SEE-ALSO: server/src/session_chat_notice/rules.rs (the card's button), server/src/session_chat_agent_restart.rs (routes it here), server/src/accounts/recovery.rs (`claim_after_reset`, `deliver`).

use super::{launch, model::Provider, session_identity, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    storage::open_gxserver_database,
};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

/// The usage-limit card's "I reset it, continue" (a `restartAgent` action routed by this id).
pub(crate) const ACTION_RESET_USED: &str = "usageLimitReset";

/// Two signals for one reset (a Redeem, then the reading that follows it) continue once.
const SAME_RESET_WITHIN: Duration = Duration::from_secs(120);

/// The login a reset applies to.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Login {
    /// A saved account.
    Account(String),
    /// A login Ghostex keeps no account for, by the provider folder its sessions run in (`None` is
    /// the CLI's default folder).
    Untracked(Option<PathBuf>),
}

/// What told Ghostex about the reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Source {
    Redeem,
    Reading,
    Card,
}

impl Source {
    fn id(self) -> &'static str {
        match self {
            Self::Redeem => "redeem",
            Self::Reading => "reading",
            Self::Card => "card",
        }
    }
}

type SessionKey = (String, String);

type Recent = BTreeMap<(Provider, Login), (Instant, Source)>;

fn recent() -> &'static Mutex<Recent> {
    static RECENT: OnceLock<Mutex<Recent>> = OnceLock::new();
    RECENT.get_or_init(Default::default)
}

/// The login a session runs on: its saved account, else the folder its agent runs in.
fn login_of(
    registry: &super::model::Registry,
    provider: Provider,
    session: &Value,
    home: &std::path::Path,
) -> Login {
    match session_identity::display_account_id(registry, provider, session, home) {
        Some(id) => Login::Account(id.to_string()),
        None => Login::Untracked(session_identity::session_home(session, provider)),
    }
}

/// Resolves the saved-account provider (`claude` or `codex`) directly from a session row.
fn session_provider(session: &Value) -> Option<Provider> {
    if let Some(provider) = session
        .pointer("/runtimeSettings/accountProvider")
        .and_then(Value::as_str)
    {
        return match provider {
            "claude" => Some(Provider::Claude),
            "codex" => Some(Provider::Codex),
            _ => None,
        };
    }
    let agent = crate::session_chat_follower::session_chat_agent_for_session(session)?;
    match crate::agents::identity::normalize_agent_id(Some(&agent)).as_deref() {
        Some("claude" | "openclaude") => Some(Provider::Claude),
        Some("codex") => Some(Provider::Codex),
        _ => None,
    }
}

/// Resolves the session's configured model name if known (for filtering model-specific windows).
fn session_model(session: &Value) -> Option<&str> {
    session
        .pointer("/runtimeSettings/agentModel")
        .or_else(|| session.pointer("/runtimeSettings/model"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Whether `discovered` has a live reading showing at least one applicable usage window at 100%
/// whose reset time has not passed yet. Model-specific windows (e.g. `seven_day_sonnet`) are
/// skipped unless `model` matches them so they never false-block healthy sessions on other models.
fn account_is_at_limit(
    discovered: &super::model::DiscoveredAccount,
    model: Option<&str>,
    now: chrono::DateTime<chrono::Utc>,
) -> bool {
    discovered.status == "ready"
        && discovered.usage_error.is_none()
        && discovered.usage.iter().any(|window| {
            super::autopilot::window_applies_to_model(window, model)
                && window.used_percent >= 100.0
                && window
                    .resets_at
                    .as_deref()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .is_none_or(|at| at.with_timezone(&chrono::Utc) > now)
        })
}

/// Account-pool safety net: true when the session runs on Claude or Codex (not its own login), its
/// bound account is at 100% usage for its applicable windows, AND zero auto-switchable accounts in
/// that provider's pool have room. When `allow_unbound_fallback` is false, sessions without a
/// resolved `display_account_id` return false so healthy unbound sessions are never false-blocked.
fn session_pool_exhausted_inner(
    state: &AppState,
    session: &Value,
    allow_unbound_fallback: bool,
) -> bool {
    let Some(provider) = session_provider(session) else {
        return false;
    };
    if launch::session_uses_own_login(provider, session) {
        return false;
    }
    let snapshot = super::runtime::current_snapshot();
    if snapshot.fetched_at.is_none() {
        return false;
    }
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return false;
    };
    let Ok(registry) = store::read(&db) else {
        return false;
    };
    if !registry
        .accounts
        .iter()
        .any(|account| account.provider == provider)
    {
        return false;
    }
    let workspace_pool = session["projectId"]
        .as_str()
        .and_then(|pid| {
            DomainRepository::new(&db, &state.metadata.server_id)
                .get_project(pid)
                .ok()
                .flatten()
        })
        .and_then(|project| super::workspace_pool::session_pool(&db, &project, session, provider));
    let model = session_model(session);
    if super::autopilot::provider_has_eligible_room_for_model_in_pool(
        &registry,
        &snapshot,
        provider,
        model,
        workspace_pool.as_deref(),
    ) {
        return false;
    }
    let now = chrono::Utc::now();
    if let Some(current_id) =
        session_identity::display_account_id(&registry, provider, session, &state.paths.home_dir)
    {
        return snapshot.accounts.iter().any(|discovered| {
            super::endpoint::account_id(discovered) == current_id
                && account_is_at_limit(discovered, model, now)
        });
    }
    if !allow_unbound_fallback {
        return false;
    }
    // Only when the session's screen is already input-blocked: treat pool as exhausted if at least
    // one auto-switchable account of this provider is explicitly confirmed at 100%.
    registry
        .accounts
        .iter()
        .filter(|account| {
            account.provider == provider
                && account.auto_switchable()
                && super::workspace_pool::in_pool(&account.id, workspace_pool.as_deref())
        })
        .any(|saved| {
            snapshot.accounts.iter().any(|discovered| {
                super::endpoint::account_id(discovered) == saved.id
                    && account_is_at_limit(discovered, model, now)
            })
        })
}

/// Account-pool safety net for sessions with a resolved bound account at 100% and zero eligible
/// room in the pool.
pub(crate) fn session_pool_exhausted(state: &AppState, session: &Value) -> bool {
    session_pool_exhausted_inner(state, session, false)
}

/// Whether the session is stopped on a usage limit: its screen shows one, its recovery waits on
/// one, or (account-pool safety net) it shows an input-blocked notice (`claudeInputBlocked` or
/// `codexInputBlocked`, never permission/trust/resume prompts) while its account pool is exhausted.
pub(crate) fn waits_at_usage_limit(state: &AppState, session: &Value) -> bool {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return false;
    };
    let cached = crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid);
    let on_screen = cached.as_ref().is_some_and(|notice| {
        notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT
    });
    let recovery = session
        .pointer("/runtimeSettings/accountRecovery")
        .unwrap_or(&Value::Null);
    let in_recovery = recovery["trigger"]
        == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT
        && matches!(
            recovery["status"].as_str(),
            Some("waiting" | "paused" | "needsAttention")
        );
    if on_screen || super::quota_wait::is_quota_paused(session) || in_recovery {
        return true;
    }
    let input_blocked = cached.as_ref().is_some_and(|notice| {
        matches!(
            notice.kind.as_str(),
            crate::session_chat_notice::SESSION_CHAT_NOTICE_CLAUDE_INPUT_BLOCKED
                | crate::session_chat_notice::SESSION_CHAT_NOTICE_CODEX_INPUT_BLOCKED
        )
    });
    input_blocked && session_pool_exhausted_inner(state, session, true)
}

/// Continues the sessions waiting on `login` in the background (the Redeem and the reading).
pub(crate) fn spawn(state: &AppState, provider: Provider, login: Login, source: Source) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    let state = Arc::new(state.clone());
    handle.spawn(async move {
        continue_after_reset(state, provider, login, None, source).await;
    });
}

/// The limit card's "I reset it, continue": every session on this session's login that waits on
/// its limit, this one included whatever its screen shows now.
pub(crate) async fn answer(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<(), DomainStateError> {
    let field = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
    };
    let key = (field("projectId")?, field("sessionId")?);
    let lookup = {
        let state = state.clone();
        let key = key.clone();
        tokio::task::spawn_blocking(move || -> Result<(Provider, Login), DomainStateError> {
            let db = open_gxserver_database(&state.paths).map_err(store::error)?;
            let repo = DomainRepository::new(&db, &state.metadata.server_id);
            let session = repo
                .get_session(&key.0, &key.1)?
                .ok_or_else(|| DomainStateError::not_found("This session no longer exists."))?;
            let project = repo
                .get_project(&key.0)?
                .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
            let provider = launch::provider(&project, &session).ok_or_else(|| {
                DomainStateError::bad_request("Only Claude and Codex sessions have usage limits.")
            })?;
            let registry = store::read(&db)?;
            Ok((
                provider,
                login_of(&registry, provider, &session, &state.paths.home_dir),
            ))
        })
        .await
        .map_err(|_| DomainStateError::bad_request("Could not read the session."))??
    };
    let (provider, login) = lookup;
    // The card answers at once; a coordinator's Continue can take a minute to land.
    let state = Arc::new(state.clone());
    tokio::spawn(async move {
        continue_after_reset(state, provider, login, Some(key), Source::Card).await;
    });
    Ok(())
}

/// One session to continue.
struct Target {
    key: SessionKey,
    coordinator: bool,
}

/// BLOCKING. The running sessions of `provider` on `login` that wait on its limit (`clicked`
/// always counts), coordinators first.
fn targets(
    state: &AppState,
    provider: Provider,
    login: &Login,
    clicked: Option<&SessionKey>,
    source: Source,
) -> Result<Vec<Target>, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let mut targets = Vec::new();
    for session in repo.list_sessions_excluding_stopped(None)? {
        let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
        else {
            continue;
        };
        let key = (pid.to_string(), sid.to_string());
        if session["lifecycleState"].as_str() != Some("running") {
            continue;
        }
        let Some(project) = repo.get_project(pid)? else {
            continue;
        };
        if launch::provider(&project, &session) != Some(provider)
            || login_of(&registry, provider, &session, &state.paths.home_dir) != *login
        {
            continue;
        }
        let picked = clicked == Some(&key);
        if !picked && !waits_at_usage_limit(state, &session) {
            continue;
        }
        // A reading is no user action: a session the user stopped, or whose Continue
        // automatically is off, keeps waiting for them.
        if source == Source::Reading && !super::recovery::recovery_retries(&repo, &session, true) {
            continue;
        }
        let runtime = session["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        targets.push(Target {
            key,
            coordinator: crate::agents::session_is_coordinator(&runtime),
        });
    }
    targets.sort_by_key(|target| !target.coordinator);
    Ok(targets)
}

/// BLOCKING. Whether the session, read now, still takes this Continue, and whether its turn is
/// running: `None` when it is gone, already being continued (`retrying`), or, for a reading, the
/// user stopped it or turned Continue automatically off since it was picked.
fn still_due(state: &AppState, key: &SessionKey, source: Source) -> Option<bool> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let session = repo.get_session(&key.0, &key.1).ok()??;
    if session.pointer("/runtimeSettings/accountRecovery/status") == Some(&json!("retrying")) {
        return None;
    }
    if source == Source::Reading && !super::recovery::recovery_retries(&repo, &session, true) {
        return None;
    }
    let now = chrono::Utc::now().to_rfc3339();
    Some(crate::presentation::presentation_activity(&session, &now) == "working")
}

/// Interrupts the session's turn when it is still running (a Codex asleep on the limit's
/// Retry-After), then claims and sends its usage-limit Continue. False when nothing was claimed.
async fn continue_one(state: Arc<AppState>, target: &Target, source: Source) -> bool {
    // Checked before the interrupt: interrupting a session already being continued would cancel
    // that Continue, and the claim below would then find it `retrying` and send none.
    let working = {
        let state = state.clone();
        let key = target.key.clone();
        tokio::task::spawn_blocking(move || still_due(&state, &key, source)).await
    };
    let Ok(Some(working)) = working else {
        return false;
    };
    if working {
        let body = json!({ "params": {
            "projectId": target.key.0,
            "sessionId": target.key.1,
        }});
        let _ = crate::session_chat_send::handle_interrupt_session_chat_http(
            &state,
            "/api/interruptSessionChat".to_string(),
            uuid::Uuid::new_v4().to_string(),
            &body,
        )
        .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let claimed = {
        let state = state.clone();
        let key = target.key.clone();
        tokio::task::spawn_blocking(move || {
            let _gate = state.accounts.mutation_gate();
            let db = open_gxserver_database(&state.paths).map_err(store::error)?;
            let repo = DomainRepository::new(&db, &state.metadata.server_id);
            let Some(session) = repo.get_session(&key.0, &key.1)? else {
                return Ok(None);
            };
            super::recovery::claim_after_reset(&state, &repo, &db, &session)
        })
        .await
    };
    match claimed {
        Ok(Ok(Some(plan))) => {
            super::recovery::deliver(state, plan).await;
            true
        }
        Ok(Ok(None)) => false,
        Ok(Err(error)) => {
            log(
                &state,
                LogLevel::Warn,
                "accountResetContinueFailed",
                Some(error.message),
                json!({ "projectId": target.key.0, "sessionId": target.key.1 }),
            );
            false
        }
        Err(_) => false,
    }
}

/// Continues every session waiting on `login`: coordinators one after another, so each is back
/// before its threads report to it, then the rest together. Returns how many were continued or
/// are being continued.
async fn continue_after_reset(
    state: Arc<AppState>,
    provider: Provider,
    login: Login,
    clicked: Option<SessionKey>,
    source: Source,
) -> usize {
    if clicked.is_none() {
        let mut recent = recent().lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        recent.retain(|_, (at, _)| now.duration_since(*at) < SAME_RESET_WITHIN);
        // A reading after any signal is the same reset; a Redeem yields only to an earlier Redeem,
        // since a reading skips the sessions whose Continue automatically is off.
        let key = (provider, login.clone());
        if let Some((_, earlier)) = recent.get(&key) {
            if source == Source::Reading || *earlier == Source::Redeem {
                return 0;
            }
        }
        recent.insert(key, (now, source));
    }
    let found = {
        let state = state.clone();
        let login = login.clone();
        let clicked = clicked.clone();
        tokio::task::spawn_blocking(move || {
            targets(&state, provider, &login, clicked.as_ref(), source)
        })
        .await
    };
    let targets = match found {
        Ok(Ok(targets)) => targets,
        Ok(Err(error)) => {
            log(
                &state,
                LogLevel::Warn,
                "accountResetContinueFailed",
                Some(error.message),
                json!({ "provider": provider.id(), "source": source.id() }),
            );
            return 0;
        }
        Err(_) => return 0,
    };
    log(
        &state,
        LogLevel::Info,
        "accountResetContinuing",
        None,
        json!({
            "provider": provider.id(),
            "source": source.id(),
            "sessions": targets.iter().map(|t| format!("{}/{}", t.key.0, t.key.1)).collect::<Vec<_>>(),
        }),
    );
    let mut continued = 0;
    let mut rest = Vec::new();
    for target in targets {
        if target.coordinator {
            if continue_one(state.clone(), &target, source).await {
                continued += 1;
            }
        } else {
            rest.push(target);
        }
    }
    continued += rest.len();
    for target in rest {
        let state = state.clone();
        tokio::spawn(async move {
            continue_one(state, &target, source).await;
        });
    }
    continued
}

/// The saved accounts a new reading shows reset before their reset time: the previous reading had
/// a window at 100% that was not due to reset yet, and the new one has every window below 100%.
pub(crate) fn reset_early(
    previous: &super::model::Snapshot,
    next: &super::model::Snapshot,
    now: chrono::DateTime<chrono::Utc>,
) -> Vec<(Provider, String)> {
    let soon = now + chrono::Duration::minutes(1);
    next.accounts
        .iter()
        .filter(|account| {
            !account.usage_stale
                && account.usage_error.is_none()
                && !account.usage.is_empty()
                && account.usage.iter().all(|w| w.used_percent < 100.)
        })
        .filter(|account| {
            let id = super::endpoint::account_id(account);
            previous
                .accounts
                .iter()
                .find(|before| super::endpoint::account_id(before) == id)
                .is_some_and(|before| {
                    before.usage.iter().any(|w| {
                        w.used_percent >= 100.
                            && w.resets_at
                                .as_deref()
                                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                                .is_some_and(|at| at > soon)
                    })
                })
        })
        .map(|account| (account.provider, super::endpoint::account_id(account)))
        .collect()
}

fn log(state: &AppState, level: LogLevel, event: &str, error: Option<String>, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(details),
    });
}
