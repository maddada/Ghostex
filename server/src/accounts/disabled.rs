//! CDXC:AgentProviders 2026-10-08 DECISION:
//! User: "why does sending a session to a session using this account not switch the thread to the sharptabs.com@gmail.com account which has usage (even tho there's error about subscription ending)". A Claude login whose organization disabled subscription access ("Your organization has disabled Claude subscription access for Claude Code", API 403 `oauth_not_allowed_for_organization`) can never answer again, so it is a reason to switch, not to wait or retry. The account is marked disabled and set to Manual, so no automatic rule picks it until the user sets it back to Automatic, selects it for a session or reconnects it. A message sent to a session on it switches to the best account set to Automatic first (resuming the same conversation), then sends. With Continue automatically on, the recovery pass switches by itself and sends the usual continuation dot, which makes the agent answer the message that failed. The wait-at-limit choice does not apply: waiting never revives the account.
//! WHY: The screen notice alone cannot say which login failed, because the resumed CLI repaints the previous login's error. Only a transcript error recorded after the session moved to its current account (`accountSuppressedUsageNoticeAt`, the switch time) and not followed by a real reply condemns the current account; otherwise one dead account would mark every account the session is moved to.
//! SEE-ALSO: session_chat_notice/rules.rs (the notice), session_chat_notice/watchdog_store.rs (hidden after a switch), session_chat_notice_progress.rs (lifted when the new login is refused too), recovery.rs (plan_session), session_chat_queue_runtime/send.rs (switch before send).

use super::{continuation::SwitchSource, endpoint, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{GxserverLogInput, LogLevel},
    server::AppState,
    storage::open_gxserver_database,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::time::Duration;

const DISABLED_REASON: &str = "Its organization disabled Claude subscription access.";

/// Whether a Claude transcript record is the API refusing this login for good.
pub(crate) fn transcript_record_disabled(record: &Value) -> bool {
    record["isApiErrorMessage"] == true
        && (record["apiErrorCode"] == "oauth_not_allowed_for_organization"
            || record["error"] == "oauth_org_not_allowed"
            || record["message"]["content"]
                .as_array()
                .is_some_and(|blocks| {
                    blocks.iter().any(|block| {
                        block["text"].as_str().is_some_and(|text| {
                            text.contains("disabled Claude subscription access")
                        })
                    })
                }))
}

/// CDXC:AgentProviders 2026-10-11 WHY:
/// A CLI started by hand in the session's terminal (a bare `claude` typed after the account's launcher exited) runs on the computer's default login, not the session's account, and writes its transcript under that login's home. On 2026-10-11 a test agent did this, the default login was a disabled one, and its refusal marked the session's working account disabled, which refused every send to every session on that account. The transcript's home names the login that really answered, so a refusal from another login says nothing about the account.
pub(super) fn written_by_other_login(
    state: &AppState,
    registry: &Registry,
    session: &Value,
    account: &str,
) -> bool {
    let Some(saved) = registry.accounts.iter().find(|a| a.id == account) else {
        return false;
    };
    super::session_identity::transcript_login_identity(
        Provider::Claude,
        session,
        &state.paths.home_dir,
    )
    .is_some_and(|identity| !saved.identity.is_empty() && identity != saved.identity)
}

/// Whether the session's transcript shows its current login refused: a refusal recorded after the session moved to this account, under this account's own login, with no real reply after it.
fn current_login_refused(
    state: &AppState,
    registry: &Registry,
    session: &Value,
    account: Option<&str>,
) -> bool {
    if account.is_some_and(|id| written_by_other_login(state, registry, session, id)) {
        return false;
    }
    let Some(agent) = crate::session_chat::resolve_session_chat_transcript_agent(Some("claude"))
    else {
        return false;
    };
    let Some(path) = crate::session_chat::resolve_session_chat_transcript_path(
        agent,
        session
            .pointer("/runtimeSettings/agentSessionId")
            .and_then(Value::as_str),
        session
            .pointer("/runtimeSettings/agentSessionPath")
            .and_then(Value::as_str),
    ) else {
        return false;
    };
    let Ok(text) = crate::session_chat_options::transcript_tail_text(&path) else {
        return false;
    };
    let since = session
        .pointer("/runtimeSettings/accountSuppressedUsageNoticeAt")
        .and_then(Value::as_str)
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .map(|t| t.timestamp_millis())
        .unwrap_or(i64::MIN);
    let mut refused_at = None;
    let mut replied_at = None;
    for line in text.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record["isSidechain"] == true {
            continue;
        }
        let Some(timestamp) = crate::session_chat::parse_timestamp(record.get("timestamp")) else {
            continue;
        };
        if transcript_record_disabled(&record) {
            refused_at = Some(timestamp);
        } else if record["type"] == "assistant"
            && record["isApiErrorMessage"] != true
            && record["message"]["model"]
                .as_str()
                .is_some_and(|model| model != "<synthetic>")
        {
            replied_at = Some(timestamp);
        }
    }
    refused_at.is_some_and(|refused| {
        refused > since && replied_at.is_none_or(|replied| replied < refused)
    })
}

fn notice_shows_disabled(state: &AppState, project: &str, session: &str) -> bool {
    crate::session_chat_options::cached_session_chat_terminal_notice(state, project, session)
        .is_some_and(|notice| notice.account_disabled())
}

pub(super) fn switch_in_progress(session: &Value) -> bool {
    matches!(
        session
            .pointer("/runtimeSettings/accountSwitch/phase")
            .and_then(Value::as_str),
        Some("switching" | "resuming" | "continuing")
    )
}

pub(super) fn current_account(
    state: &AppState,
    registry: &Registry,
    session: &Value,
) -> Option<String> {
    super::session_identity::display_account_id(
        registry,
        Provider::Claude,
        session,
        &state.paths.home_dir,
    )
    .map(str::to_string)
}

/// Marks the account disabled and takes it out of automatic switching; false when it already was.
fn mark(
    state: &AppState,
    db: &rusqlite::Connection,
    registry: &mut Registry,
    id: &str,
) -> Result<bool, DomainStateError> {
    let Some(account) = registry.accounts.iter_mut().find(|a| a.id == id) else {
        return Ok(false);
    };
    if account.disabled.is_some() {
        return Ok(false);
    }
    account.eligible = false;
    account.disabled = Some(DisabledAccount {
        reason: DISABLED_REASON.into(),
        at: Utc::now().to_rfc3339(),
    });
    let name = account.name.clone();
    store::write(db, registry)?;
    log(
        state,
        LogLevel::Warn,
        "accountDisabled",
        json!({ "accountId": id, "name": name }),
    );
    Ok(true)
}

fn log(state: &AppState, level: LogLevel, event: &str, details: Value) {
    let _ = state.logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: Some(state.metadata.server_id.clone()),
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    });
}

/// The account a session on a disabled login moves to: the same ranking a usage-limit switch uses, over accounts set to Automatic with usage left.
fn replacement(
    registry: &Registry,
    snapshot: &Snapshot,
    session: &Value,
    current: Option<&str>,
) -> Option<String> {
    let policy = launch::effective_policy(registry, Provider::Claude, session);
    super::recovery::ranked(
        registry,
        snapshot,
        Provider::Claude,
        current,
        "",
        policy.priority,
    )
    .first()
    .map(|account| account.id.clone())
}

/// Recovery-pass step for one running Claude session; true when it owned the session this pass.
pub(crate) fn recover(
    state: &AppState,
    db: &rusqlite::Connection,
    repo: &DomainRepository<'_>,
    snapshot: &Snapshot,
    project: &Value,
    old: &Value,
    provider: Provider,
) -> Result<bool, DomainStateError> {
    let (Some(pid), Some(sid)) = (old["projectId"].as_str(), old["sessionId"].as_str()) else {
        return Ok(false);
    };
    // A session on its own login has no Ghostex account to replace (CDXC:AgentProviders 2026-10-09 in launch.rs).
    if provider != Provider::Claude
        || launch::session_uses_own_login(provider, old)
        || !notice_shows_disabled(state, pid, sid)
    {
        return Ok(false);
    }
    let _gate = state.accounts.mutation_gate();
    let Some(session) = repo.get_session(pid, sid)? else {
        return Ok(true);
    };
    if session["lifecycleState"].as_str() != Some("running") || switch_in_progress(&session) {
        return Ok(true);
    }
    let mut registry = store::read(db)?;
    let current = current_account(state, &registry, &session);
    if !current_login_refused(state, &registry, &session, current.as_deref()) {
        return Ok(false);
    }
    if let Some(id) = current.as_deref() {
        mark(state, db, &mut registry, id)?;
    }
    let policy = launch::effective_policy(&registry, provider, &session);
    if !policy.enabled
        || session
            .pointer("/runtimeSettings/accountRecoverySuppressed")
            .and_then(Value::as_bool)
            == Some(true)
        || session
            .pointer("/runtimeSettings/accountRecovery/status")
            .and_then(Value::as_str)
            == Some("needsAttention")
    {
        return Ok(true);
    }
    let attention = |reason: &str| {
        json!({
            "status": "needsAttention", "trigger": "accountDisabled", "attempt": 0,
            "reason": reason, "updatedAt": Utc::now().to_rfc3339()
        })
    };
    let Some(target) = replacement(&registry, snapshot, &session, current.as_deref()) else {
        super::recovery::save(state, repo, &session, attention("This account's organization disabled Claude subscription access, and no other account set to Automatic has usage left. Switch this session's account to continue."))?;
        return Ok(true);
    };
    match endpoint::select(
        state,
        repo,
        &registry,
        snapshot,
        project,
        &session,
        Some(&target),
        SwitchSource::Automatic,
    ) {
        Ok(()) => {
            log(
                state,
                LogLevel::Info,
                "accountDisabledSwitch",
                json!({ "projectId": pid, "sessionId": sid, "from": current, "to": target, "source": "recovery" }),
            );
            endpoint::publish(state, repo, &session)?;
        }
        Err(error) => {
            let session = repo.get_session(pid, sid)?.unwrap_or(session);
            super::recovery::save(state, repo, &session, attention(&error.message))?;
        }
    }
    Ok(true)
}

/// Send-path step: a message for a session whose login is disabled moves the session to another account first, then waits for the resumed agent's input box. True when it switched.
pub(crate) async fn switch_before_send(
    state: &AppState,
    project: &str,
    session: &str,
) -> Result<bool, DomainStateError> {
    let owned = state.clone();
    let (pid, sid) = (project.to_string(), session.to_string());
    let switched = tokio::task::spawn_blocking(move || switch_if_disabled(&owned, &pid, &sid))
        .await
        .map_err(store::error)??;
    if switched {
        wait_for_switch(state, project, session).await?;
    }
    Ok(switched)
}

fn switch_if_disabled(state: &AppState, pid: &str, sid: &str) -> Result<bool, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(store::error)?;
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Some(session) = repo.get_session(pid, sid)? else {
        return Ok(false);
    };
    let Some(project) = repo.get_project(pid)? else {
        return Ok(false);
    };
    if launch::provider(&project, &session) != Some(Provider::Claude)
        || launch::session_uses_own_login(Provider::Claude, &session)
        || switch_in_progress(&session)
    {
        return Ok(false);
    }
    let registry = store::read(&db)?;
    let current = current_account(state, &registry, &session);
    let marked = current
        .as_deref()
        .and_then(|id| registry.accounts.iter().find(|a| a.id == id))
        .is_some_and(|account| account.disabled.is_some());
    if !marked
        && !(notice_shows_disabled(state, pid, sid)
            && current_login_refused(state, &registry, &session, current.as_deref()))
    {
        return Ok(false);
    }
    // Usage is read before taking the account gate: it can wait on the network.
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let _gate = state.accounts.mutation_gate();
    let Some(session) = repo.get_session(pid, sid)? else {
        return Ok(false);
    };
    if switch_in_progress(&session) {
        return Ok(false);
    }
    let mut registry = store::read(&db)?;
    if let Some(id) = current.as_deref() {
        mark(state, &db, &mut registry, id)?;
    }
    let Some(target) = replacement(&registry, &snapshot, &session, current.as_deref()) else {
        return Err(DomainStateError::bad_request(
            "This session's Claude account can no longer be used: its organization disabled Claude subscription access. No other account set to Automatic has usage left, so the message was not sent. Switch the session to another account, or set one to Automatic in Settings > Accounts.",
        ));
    };
    // A manual switch sends no continuation dot: the message being sent is the continuation.
    endpoint::select(
        state,
        &repo,
        &registry,
        &snapshot,
        &project,
        &session,
        Some(&target),
        SwitchSource::Manual,
    )?;
    log(
        state,
        LogLevel::Info,
        "accountDisabledSwitch",
        json!({ "projectId": pid, "sessionId": sid, "from": current, "to": target, "source": "send" }),
    );
    Ok(true)
}

/// Waits for the restart on the new account to finish and its input box to appear, about a minute and a half at most; the send's own checks take over after that.
async fn wait_for_switch(
    state: &AppState,
    project: &str,
    session: &str,
) -> Result<(), DomainStateError> {
    let detector = crate::session_chat_options::SessionChatOptionDetector::new(state);
    for _ in 0..45 {
        let row = {
            let db = open_gxserver_database(&state.paths).map_err(store::error)?;
            let repo = DomainRepository::new(&db, &state.metadata.server_id);
            repo.get_session(project, session)?
                .ok_or_else(|| DomainStateError::not_found("Session not found."))?
        };
        if row
            .pointer("/runtimeSettings/accountSwitch/phase")
            .and_then(Value::as_str)
            == Some("failed")
        {
            return Err(DomainStateError::bad_request(format!(
                "The message was not sent: moving the session off its disabled Claude account failed. {}",
                row.pointer("/runtimeSettings/accountSwitch/reason")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )));
        }
        if !switch_in_progress(&row) {
            if row["lifecycleState"].as_str() != Some("running") {
                return Ok(());
            }
            let agent = crate::session_chat_composer::session_chat_composer_agent_id(&row);
            let detection = detector
                .detect(project, session, agent.as_deref(), true)
                .await;
            if detection.captured
                && detection.composer.state
                    == crate::session_chat_composer::SessionChatComposerState::Ready
            {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Ok(())
}
