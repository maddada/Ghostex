//! `/api/agentAccounts` operations behind `ghostex accounts` (`roster`) and
//! `ghostex agents switch-account` (`switchSession`).

use super::{endpoint, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User: "we need to have the ability for the orchestrator to switch the account of another thread and to full reload it in case it gets stuck for any reason (should see all available accounts and usages etc) through ghostex cli". `roster` is the Settings > Accounts `list` with each account's sessions, its default flag and whether a usage window is at its limit; `switchSession` finds an account by number, email or id and runs the same `select` as the chat's Switch Account (an in-place restart that resumes the same conversation); `ghostex agents reload` is `reload` in session_chat_agent_restart.rs. All of them work on any session, like `ghostex agents send`, not only an orchestrator's threads. A session in the middle of a turn is refused unless `--force` is given (user, 2026-10-14: "also add --force but force needs to send \".\" after the switch"): the restart in place ends the turn, and "." goes out through the normal send path once the restarted agent's input box is ready, like after an automatic switch (continuation.rs). `ghostex accounts` lists only an account's live sessions and counts the stopped ones unless `--all` (orchestrator, 2026-10-14: 201 mostly stopped rows buried the useful part).
/// SEE-ALSO: server/src/ghostex_cli/accounts.rs, server/src/ghostex_cli/agents/lifecycle.rs, server/src/coordinators/playbook.md ("While threads run", healing step 5).
/// Stopped sessions (closed ones, which stay in the table) are only counted unless `all_sessions`
/// asks for them: an account's live sessions (running, including one whose start failed, and
/// sleeping) are the ones a caller can act on, and hundreds of stopped rows buried them.
pub(super) fn roster(
    repository: &DomainRepository<'_>,
    snapshot: &Snapshot,
    value: &mut Value,
    all_sessions: bool,
) -> Result<(), DomainStateError> {
    let mut sessions: HashMap<String, Vec<Value>> = HashMap::new();
    let mut stopped: HashMap<String, u64> = HashMap::new();
    for session in repository.list_sessions(None)? {
        let Some(id) = session
            .pointer("/runtimeSettings/accountId")
            .and_then(Value::as_str)
        else {
            continue;
        };
        if session["lifecycleState"].as_str() == Some("stopped") {
            *stopped.entry(id.to_string()).or_default() += 1;
            if !all_sessions {
                continue;
            }
        }
        sessions.entry(id.to_string()).or_default().push(json!({
            "globalRef": session["globalRef"],
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "title": session["title"],
            "lifecycleState": session["lifecycleState"],
        }));
    }
    let defaults = value["defaultAccounts"].clone();
    let now = Utc::now();
    let Some(rows) = value["accounts"].as_array_mut() else {
        return Ok(());
    };
    for row in rows.iter_mut() {
        let id = row["id"].as_str().unwrap_or_default().to_string();
        let found = snapshot
            .accounts
            .iter()
            .find(|account| endpoint::account_id(account) == id);
        let limited = found
            .map(|account| limited_windows(account, now))
            .unwrap_or_default();
        let status = row["status"].as_str().unwrap_or_default().to_string();
        row["number"] = row["selector"].clone();
        row["default"] = json!(defaults[row["provider"].as_str().unwrap_or_default()] == json!(id));
        row["state"] = json!(match status.as_str() {
            "ready" if !limited.is_empty() => "limitReached",
            "ready" => "ready",
            "loginRequired" | "identityChanged" | "unavailable" => "needsSignIn",
            other => other,
        });
        row["limitReached"] = json!(limited);
        // A ready account with no usage windows (Codex #3 on 2026-10-14) can be at its limit
        // unseen, so a caller must not take its room on trust.
        row["noUsageReading"] =
            json!(status == "ready" && found.is_none_or(|account| account.usage.is_empty()));
        if let (Some(found), Some(windows)) = (found, row["usage"].as_array_mut()) {
            for (window, reading) in windows.iter_mut().zip(&found.usage) {
                window["label"] = json!(short_label(reading));
            }
        }
        row["sessions"] = json!(sessions.remove(&id).unwrap_or_default());
        row["stoppedSessionCount"] = json!(stopped.remove(&id).unwrap_or(0));
    }
    rows.sort_by_key(|row| {
        (
            row["provider"].as_str().unwrap_or_default().to_string(),
            row["number"]
                .as_str()
                .and_then(|number| number.parse::<u64>().ok())
                .unwrap_or(u64::MAX),
        )
    });
    Ok(())
}

/// The labels of the account-wide windows (five-hour, weekly, Codex's) at 100% with their reset
/// still ahead. Model-scoped windows (Fable weekly) and Extra usage only limit part of the
/// account, so they stay in `usage` without marking the whole account as limited.
fn limited_windows(account: &DiscoveredAccount, now: DateTime<Utc>) -> Vec<String> {
    account
        .usage
        .iter()
        .filter(|window| window.model.is_none() && window.id != "spend")
        .filter(|window| {
            window.used_percent >= 100.
                && window
                    .resets_at
                    .as_deref()
                    .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
                    .is_none_or(|at| at > now)
        })
        .map(short_label)
        .collect()
}

/// CDXC:AgentProviders 2026-10-14 DECISION:
/// User: "please use 7d and 5h everywhere never use weekly or five hours etc." A window is named by its length (`5h`, `7d`, a Codex window of another length by its own hours or days), after its model when it is model-scoped (`Fable 7d`), and Extra usage stays as it is. Derived from the window's id and length, not its label, so it holds whatever label the reading carries.
fn short_label(window: &UsageWindow) -> String {
    if window.id == "spend" {
        return window.label.clone();
    }
    let span = match (window.id.as_str(), window.limit_window_seconds) {
        ("fiveHour", _) => "5h".to_string(),
        ("sevenDay", _) => "7d".to_string(),
        (_, Some(seconds)) if seconds >= 604_800 => "7d".to_string(),
        (_, Some(seconds)) if seconds > 0 && seconds % 86_400 == 0 => {
            format!("{}d", seconds / 86_400)
        }
        (_, Some(seconds)) if seconds > 0 => format!("{}h", (seconds as f64 / 3600.0).round()),
        // Claude's model-scoped windows (`Fable weekly`) carry no length and are weekly.
        _ => "7d".to_string(),
    };
    let model = window.model.clone().or_else(|| {
        window
            .id
            .split_once(':')
            .map(|(model, _)| model.to_string())
            .filter(|model| !model.is_empty())
    });
    match model {
        Some(model) => format!("{model} {span}"),
        None => span,
    }
}

/// How long `switchSession` waits for the in-place restart (and with `force`, the "." after it) to report success or failure.
const SWITCH_WAIT: Duration = Duration::from_secs(90);

/// CDXC:AgentProviders 2026-11-04 WHY:
/// `switch-account` and `coordinator start-thread --account` name an account the same way (its number, email or id) and refuse the same ones (turned off, or not signed in on this computer), listing the accounts that would work, so the two never disagree about which account a session may use.
pub(crate) fn resolve_ready_account<'a>(
    registry: &'a Registry,
    snapshot: &Snapshot,
    provider: Provider,
    wanted: &str,
) -> Result<(&'a SavedAccount, String), DomainStateError> {
    let discovered = |account: &SavedAccount| {
        snapshot
            .accounts
            .iter()
            .find(|found| endpoint::account_id(found) == account.id)
    };
    let email = |account: &SavedAccount| {
        discovered(account)
            .map(|found| found.email.clone())
            .filter(|email| !email.is_empty())
            .unwrap_or_else(|| {
                account
                    .identity
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .to_string()
            })
    };
    let ready = |account: &SavedAccount| {
        snapshot.accounts.iter().any(|found| {
            found.provider == provider
                && found.selector == account.selector
                && found.identity == account.identity
                && found.status == "ready"
        })
    };
    let describe = |account: &SavedAccount| {
        let email = email(account);
        let mut text = format!("{} ({})", account.selector, email);
        if let Some(found) = discovered(account) {
            let limited = limited_windows(found, Utc::now());
            if !limited.is_empty() {
                text.push_str(&format!(", {} reached", limited.join(" and ")));
            } else if found.status == "ready" && found.usage.is_empty() {
                text.push_str(", no usage reading");
            }
        }
        text
    };
    let accounts: Vec<&SavedAccount> = registry
        .accounts
        .iter()
        .filter(|account| account.provider == provider)
        .collect();
    let working = || {
        let rows: Vec<String> = accounts
            .iter()
            .filter(|account| ready(account))
            .map(|account| describe(account))
            .collect();
        if rows.is_empty() {
            format!(
                "No saved {} account is signed in on this computer; the user has to sign in to one (Settings > Accounts).",
                provider.id()
            )
        } else {
            format!("Accounts that would work: {}.", rows.join("; "))
        }
    };
    let target = accounts
        .iter()
        .find(|account| account.id == wanted)
        .or_else(|| accounts.iter().find(|account| account.selector == wanted))
        .or_else(|| {
            accounts
                .iter()
                .find(|account| email(account).eq_ignore_ascii_case(wanted))
        })
        .copied()
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "No saved {} account matches \"{wanted}\". {}",
                provider.id(),
                working()
            ))
        })?;
    if target.user_disabled {
        return Err(DomainStateError::bad_request(format!(
            "Account {} ({}) is turned off in Settings > Accounts; turn it on there to use it. {}",
            target.selector,
            email(target),
            working()
        )));
    }
    if !ready(target) {
        return Err(DomainStateError::bad_request(format!(
            "Account {} needs to be signed in again on this computer before a session can use it. {}",
            describe(target),
            working()
        )));
    }
    Ok((target, email(target)))
}

/// Moves a session to the account named by `account` (cswap/xswap number, email or id) through
/// `select`, then waits for the restart to finish so the caller learns whether it worked.
pub(super) fn switch_session(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let wanted = params
        .get("account")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty() && text.len() < 512)
        .ok_or_else(|| DomainStateError::bad_request("account is required."))?;
    let snapshot = state.accounts.refresh(&state.paths.home_dir, false);
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let registry = store::read(&db)?;
    let session = endpoint::get_session(&repository, params)?;
    let project = repository
        .get_project(session["projectId"].as_str().unwrap_or_default())?
        .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
    let provider = launch::provider(&project, &session).ok_or_else(|| {
        DomainStateError::bad_request(
            "This session is not a Claude or Codex session, so it has no account to switch.",
        )
    })?;
    let (target, target_email) = resolve_ready_account(&registry, &snapshot, provider, wanted)?;
    let previous = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str)
        .map(str::to_string);
    let account = json!({
        "id": target.id,
        "provider": provider,
        "number": target.selector,
        "email": target_email,
        "name": target.name,
    });
    let failed_switch_to_target = session
        .pointer("/runtimeSettings/accountSwitch/phase")
        .and_then(Value::as_str)
        == Some("failed")
        && session
            .pointer("/runtimeSettings/accountSwitch/toAccountId")
            .and_then(Value::as_str)
            == Some(target.id.as_str());
    if previous.as_deref() == Some(target.id.as_str()) && !failed_switch_to_target {
        return Ok(
            json!({ "status": "unchanged", "account": account, "session": summary(&session) }),
        );
    }
    // `force` restarts a running session even mid-turn (the restart's Escape and Ctrl+C end the
    // turn) and then sends "." once the agent is ready (continuation.rs); a sleeping session has
    // no turn to end, so it switches without one.
    let running = session["lifecycleState"].as_str() == Some("running");
    let force = params.get("force").and_then(Value::as_bool) == Some(true) && running;
    let interrupted = force
        && crate::presentation::presentation_activity(&session, &Utc::now().to_rfc3339())
            == "working";
    let mut select = params.clone();
    select.insert("operation".into(), json!("select"));
    select.insert("accountId".into(), json!(target.id));
    select.insert("force".into(), json!(force));
    endpoint::dispatch(state, &select)?;
    // `select` restarts a live agent in place on its own thread; a sleeping session only changes
    // its row and starts on the new account at its next wake.
    let started = Instant::now();
    let row = loop {
        let row = endpoint::get_session(&repository, params)?;
        let phase = row
            .pointer("/runtimeSettings/accountSwitch/phase")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let restarting = matches!(phase, "switching" | "resuming")
            && row
                .pointer("/runtimeSettings/accountSwitchAttempt")
                .is_some();
        if !(restarting || phase == "continuing") || started.elapsed() >= SWITCH_WAIT {
            break row;
        }
        std::thread::sleep(Duration::from_millis(500));
    };
    let phase = row
        .pointer("/runtimeSettings/accountSwitch/phase")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let still_restarting = row
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .is_some()
        || phase == "continuing";
    let status = if phase == "failed" {
        "failed"
    } else if still_restarting {
        "switching"
    } else if row["lifecycleState"].as_str() == Some("running") {
        "switched"
    } else {
        "switchedWhileAsleep"
    };
    let mut result = json!({
        "status": status,
        "account": account,
        "previousAccountId": previous,
        "session": summary(&row),
        "forced": force,
        "interruptedTurn": interrupted,
        "continueSent": force && status == "switched",
        // The agent kept the terminal, so the switch restarted the session (accounts/restart.rs).
        "reloaded": row.pointer("/runtimeSettings/accountSwitch/reloaded") == Some(&json!(true)),
    });
    if phase == "failed" {
        result["error"] = row
            .pointer("/runtimeSettings/accountSwitch/reason")
            .cloned()
            .unwrap_or(Value::Null);
    }
    Ok(result)
}

fn summary(session: &Value) -> Value {
    json!({
        "globalRef": session["globalRef"],
        "title": session["title"],
        "lifecycleState": session["lifecycleState"],
        "accountId": session.pointer("/runtimeSettings/accountId"),
        "agentSessionId": session.pointer("/runtimeSettings/agentSessionId"),
    })
}
