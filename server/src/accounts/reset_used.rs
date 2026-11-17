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

fn recent() -> &'static Mutex<BTreeMap<(Provider, Login), Instant>> {
    static RECENT: OnceLock<Mutex<BTreeMap<(Provider, Login), Instant>>> = OnceLock::new();
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

/// Whether the session is stopped on a usage limit: its screen shows one, or its recovery waits
/// on one.
pub(crate) fn waits_at_usage_limit(state: &AppState, session: &Value) -> bool {
    let (Some(pid), Some(sid)) = (session["projectId"].as_str(), session["sessionId"].as_str())
    else {
        return false;
    };
    let on_screen =
        crate::session_chat_options::cached_session_chat_terminal_notice(state, pid, sid)
            .is_some_and(|notice| {
                notice.kind == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT
            });
    let recovery = session
        .pointer("/runtimeSettings/accountRecovery")
        .unwrap_or(&Value::Null);
    on_screen
        || (recovery["trigger"] == crate::session_chat_notice::SESSION_CHAT_NOTICE_USAGE_LIMIT
            && matches!(
                recovery["status"].as_str(),
                Some("waiting" | "paused" | "needsAttention")
            ))
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

/// One session to continue, read under the accounts gate.
struct Target {
    key: SessionKey,
    coordinator: bool,
    working: bool,
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
    let now = chrono::Utc::now().to_rfc3339();
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
            working: crate::presentation::presentation_activity(&session, &now) == "working",
        });
    }
    targets.sort_by_key(|target| !target.coordinator);
    Ok(targets)
}

/// Interrupts the session's turn when it is still running (a Codex asleep on the limit's
/// Retry-After), then claims and sends its usage-limit Continue. False when nothing was claimed.
async fn continue_one(state: Arc<AppState>, target: &Target) -> bool {
    if target.working {
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
        recent.retain(|_, at| now.duration_since(*at) < SAME_RESET_WITHIN);
        if recent.insert((provider, login.clone()), now).is_some() {
            return 0;
        }
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
            if continue_one(state.clone(), &target).await {
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
            continue_one(state, &target).await;
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
