use super::{continuation::SwitchSource, helpers, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
fn required<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty() && s.len() < 512)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
}
fn provider_param(params: &Map<String, Value>) -> Result<Provider, DomainStateError> {
    serde_json::from_value(params.get("provider").cloned().unwrap_or(Value::Null))
        .map_err(|_| DomainStateError::bad_request("Choose Claude or Codex."))
}
/// A saved account no login on this computer answers to any more.
const SAVED_LOGIN_MISSING: &str = "This account isn't signed in on this computer. Reconnect it.";

pub(crate) fn account_id(account: &DiscoveredAccount) -> String {
    format!(
        "{}-{}",
        account.provider.id(),
        hex::encode(Sha256::digest(account.identity.as_bytes()))
    )
}
/// A login added to the saved accounts for the first time, with the settings a new account starts with.
///
/// CDXC:AgentProviders 2026-10-07 DECISION:
/// User: "please also make 'available for automatic switching' enabled by default for accounts when added in settings in ghostex". A newly registered account starts with `eligible: true`; Settings, `setup.rs` and the phone all add accounts through `register`, and accounts already saved keep their own value.
///
/// CDXC:AgentProviders 2026-11-03 DECISION:
/// User: "when i add an acconut in ghostex make it appear in sidebar enabled by default". A new account starts with `show_in_titlebar: true`, so its usage shows in the sidebar's account strip at once; accounts already saved keep the choice they have, including one the user hid.
pub(crate) fn new_saved_account(found: &DiscoveredAccount) -> SavedAccount {
    SavedAccount {
        id: account_id(found),
        provider: found.provider,
        selector: found.selector.clone(),
        identity: found.identity.clone(),
        name: found.name.clone(),
        color: "neutral".into(),
        indicator: String::new(),
        show_in_titlebar: true,
        eligible: true,
        shared_history: true,
        disabled: None,
        user_disabled: false,
        login_expired_at: None,
        login_confirmed_at: None,
    }
}
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let operation = required(params, "operation")?;
    if operation.starts_with("setup") {
        return super::setup::dispatch(state, params);
    }
    if operation.starts_with("helper") {
        return super::helper_tools::dispatch(state, params);
    }
    if operation == "redeemReset" {
        return super::reset_claim::redeem(state, params);
    }
    if operation == "resetWatch" {
        return super::reset_watch::endpoint(state, params);
    }
    if operation == "switchSession" {
        return super::cli::switch_session(state, params);
    }
    match super::usage_only::handle(&state.paths, params)? {
        super::usage_only::Handled::Answer(answer) => return Ok(answer),
        super::usage_only::Handled::Changed => {
            let mut list = Map::new();
            list.insert("operation".into(), json!("list"));
            return dispatch(state, &list);
        }
        super::usage_only::Handled::NotMine => {}
    }
    let titlebar_has_accounts = if operation == "titlebar" {
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
        store::read(&db)?
            .accounts
            .iter()
            .any(|a| a.show_in_titlebar)
    } else {
        true
    };
    let usage_history = (operation == "titlebar" && titlebar_has_accounts)
        .then(|| state.accounts.history.snapshot());
    // CDXC:AgentProviders 2026-09-07 WHY:
    // A manual switch already has a selected saved login and validates its identity locally in launch::command. Polling both providers' usage first could delay the restart by an unrelated network timeout.
    // CDXC:AgentProviders 2026-09-09 WHY:
    // The first titlebar response renders saved identities without waiting for helper discovery or usage network requests. The desktop follows it with a normal usage refresh.
    let cached_titlebar =
        operation == "titlebar" && params.get("cachedOnly").and_then(Value::as_bool) == Some(true);
    // CDXC:AgentProviders 2026-10-01 WHY:
    // Settings > Accounts draws the last reading at once and follows it with the forced refresh it must run on open, instead of showing no accounts until every helper and usage request answers. Before the first reading exists it refreshes as usual, so saved accounts never flash as unavailable.
    let cached_list = operation == "list"
        && params.get("cachedOnly").and_then(Value::as_bool) == Some(true)
        && state.accounts.snapshot().fetched_at.is_some();
    // Usage discovery skips the logins the user turned off, so a fresh gxserver reads the
    // registry before its first poll.
    if !store::loaded() {
        if let Ok(db) = crate::storage::open_gxserver_database(&state.paths) {
            let _ = store::read(&db);
        }
    }
    let mut snapshot = if matches!(
        operation,
        "select" | "setTitlebar" | "dismissSwitch" | "retryNow"
    ) || !titlebar_has_accounts
        || cached_titlebar
        || cached_list
    {
        state.accounts.snapshot()
    } else {
        state.accounts.refresh(
            &state.paths.home_dir,
            params.get("refresh").and_then(Value::as_bool) == Some(true) || operation == "register",
        )
    };
    // CDXC:AgentProviders 2026-10-23 WHY:
    // The four read operations change nothing, so they no longer queue behind `mutations`: a switch restarting a session holds it for seconds, and every open chat's 30-second `session` poll and the titlebar's read used to wait that out.
    let read_only = matches!(operation, "list" | "roster" | "session" | "titlebar");
    let _gate = if read_only {
        None
    } else {
        Some(state.accounts.mutation_gate())
    };
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let mut registry = store::read(&db)?;
    let mut changed_sessions = Vec::new();
    match operation {
        "list" | "roster" | "session" | "titlebar" => {}
        // CDXC:AgentProviders 2026-09-08 DECISION: Starring a saved account pins its own usage button before extensions in the titlebar; multiple Claude and Codex accounts can be pinned independently.
        "setTitlebar" => {
            let id = required(params, "id")?;
            let shown = params
                .get("shown")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    DomainStateError::bad_request(
                        "Choose whether to show this account in the titlebar.",
                    )
                })?;
            let account = registry
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| DomainStateError::not_found("Account not found."))?;
            account.show_in_titlebar = shown;
            store::write(&db, &registry)?;
        }
        // The Enabled switch in Settings > Accounts (CDXC:AgentProviders 2026-10-14 on `SavedAccount::user_disabled`). Turning an account back on reads its usage and sign-in state again at once.
        "setEnabled" => {
            let id = required(params, "id")?;
            let enabled = params
                .get("enabled")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Choose whether this account is enabled.")
                })?;
            let account = registry
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| DomainStateError::not_found("Account not found."))?;
            account.user_disabled = !enabled;
            store::write(&db, &registry)?;
            state.accounts.invalidate();
            snapshot = state.accounts.refresh(&state.paths.home_dir, true);
        }
        // CDXC:AgentProviders 2026-09-08 DECISION: Settings can exchange two account slots through cswap or xswap. Defaults and session bindings follow account identity, and cached launch plans must be regenerated for the new slot.
        "swapSlots" => {
            let first = registry
                .accounts
                .iter()
                .find(|a| a.id == required(params, "firstId").unwrap_or(""))
                .cloned()
                .ok_or_else(|| DomainStateError::bad_request("Choose the first account."))?;
            let second = registry
                .accounts
                .iter()
                .find(|a| a.id == required(params, "secondId").unwrap_or(""))
                .cloned()
                .ok_or_else(|| DomainStateError::bad_request("Choose the second account."))?;
            if first.provider != second.provider || first.id == second.id {
                return Err(DomainStateError::bad_request(
                    "Choose two different accounts of the same provider.",
                ));
            }
            launch::command(&state.paths.home_dir, &first)?;
            launch::command(&state.paths.home_dir, &second)?;
            let binary = helpers::executable(&state.paths.home_dir, first.provider.helper())
                .ok_or_else(|| {
                    DomainStateError::bad_request(format!(
                        "Install {} in Settings > Accounts first.",
                        helpers::tool_label(first.provider.helper())
                    ))
                })?;
            let output = crate::platform::process::background_command(binary)
                .args(["swap", &first.selector, &second.selector])
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(store::error)?;
            if !output.status.success() {
                return Err(DomainStateError::bad_request("Couldn't swap these two accounts. For Claude, stop sessions using either account first, then try again."));
            }
            state.accounts.invalidate();
            snapshot = state.accounts.refresh(&state.paths.home_dir, true);
            for saved in &mut registry.accounts {
                if saved.id == first.id {
                    saved.selector = second.selector.clone();
                } else if saved.id == second.id {
                    saved.selector = first.selector.clone();
                }
            }
            store::write(&db, &registry)?;
            for session in repository.list_sessions(None)? {
                if let Some(saved) = registry.accounts.iter().find(|a| {
                    session
                        .pointer("/runtimeSettings/accountId")
                        .and_then(Value::as_str)
                        == Some(a.id.as_str())
                        && (a.id == first.id || a.id == second.id)
                }) {
                    let mut runtime = session["runtimeSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    launch::assign(
                        &mut runtime,
                        saved,
                        launch::command(&state.paths.home_dir, saved)?,
                    )?;
                    let mut settings = session["launchSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
                        settings.remove(key);
                    }
                    if super::drafts::needs_fresh_launch(&session) {
                        let project = repository
                            .get_project(session["projectId"].as_str().unwrap_or(""))?
                            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
                        super::drafts::rebuild_launch(
                            &repository,
                            &project,
                            &session,
                            &mut runtime,
                            &mut settings,
                        )?;
                    }
                    repository.update_session(json!({"projectId":session["projectId"],"sessionId":session["sessionId"],"runtimeSettings":runtime,"launchSettings":settings}).as_object().unwrap())?;
                    changed_sessions.push(session);
                }
            }
        }
        "register" => {
            let provider = provider_param(params)?;
            let selector = required(params, "selector")?;
            if params.get("shareHistory").and_then(Value::as_bool) != Some(true) {
                return Err(DomainStateError::bad_request(
                    "Enable shared conversations before adding this account.",
                ));
            }
            let found = snapshot
                .accounts
                .iter()
                .find(|a| a.provider == provider && a.selector == selector)
                .ok_or_else(|| {
                    DomainStateError::bad_request("Refresh the saved accounts list first.")
                })?;
            if found.status != "ready" || found.identity.is_empty() {
                return Err(DomainStateError::bad_request(
                    "Sign in to this account before adding it.",
                ));
            }
            let id = account_id(found);
            if params
                .get("id")
                .and_then(Value::as_str)
                .is_some_and(|expected| expected != id)
            {
                return Err(DomainStateError::bad_request("This slot now contains a different login. Sign in to the original account or add the new login separately."));
            }
            let reconnecting = registry.accounts.iter().any(|a| a.id == id);
            if let Some(saved) = registry.accounts.iter_mut().find(|a| a.id == id) {
                saved.selector = found.selector.clone();
                // Signing in again is the user saying the login works now.
                if saved.disabled.take().is_some() {
                    saved.eligible = true;
                }
            }
            if !reconnecting {
                if registry.accounts.len() >= 50 {
                    return Err(DomainStateError::bad_request(
                        "At most 50 accounts can be registered.",
                    ));
                }
                registry.accounts.push(new_saved_account(found));
            }
            store::write(&db, &registry)?;
            if reconnecting {
                let account = registry.accounts.iter().find(|a| a.id == id).unwrap();
                let command = launch::command(&state.paths.home_dir, account)?;
                for session in repository.list_sessions(None)? {
                    if session
                        .pointer("/runtimeSettings/accountId")
                        .and_then(Value::as_str)
                        != Some(id.as_str())
                    {
                        continue;
                    }
                    let mut runtime = session["runtimeSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    launch::assign(&mut runtime, account, command.clone())?;
                    let mut settings = session["launchSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
                        settings.remove(key);
                    }
                    if super::drafts::needs_fresh_launch(&session) {
                        let project = repository
                            .get_project(session["projectId"].as_str().unwrap_or(""))?
                            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
                        super::drafts::rebuild_launch(
                            &repository,
                            &project,
                            &session,
                            &mut runtime,
                            &mut settings,
                        )?;
                    }
                    repository.update_session(json!({"projectId":session["projectId"],"sessionId":session["sessionId"],"runtimeSettings":runtime,"launchSettings":settings}).as_object().unwrap())?;
                    changed_sessions.push(session);
                }
            }
        }
        "update" => {
            let id = required(params, "id")?;
            let name = required(params, "name")?.trim();
            let color = required(params, "color")?;
            if name.chars().count() > 80 || color_hex(color).is_none() {
                return Err(DomainStateError::bad_request(
                    "Choose a name up to 80 characters and one of the account colors.",
                ));
            }
            let eligible = params
                .get("eligible")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("Account eligibility is required."))?;
            let account = registry
                .accounts
                .iter_mut()
                .find(|a| a.id == id)
                .ok_or_else(|| DomainStateError::not_found("Account not found."))?;
            if let Some(indicator) = params.get("indicator") {
                let indicator = indicator.as_str().ok_or_else(|| DomainStateError::bad_request("Enter up to two letters or numbers, or - to hide the account indicator."))?.trim();
                if !indicator.is_empty()
                    && indicator != "-"
                    && (indicator.chars().count() > 2
                        || !indicator.chars().all(char::is_alphanumeric))
                {
                    return Err(DomainStateError::bad_request(
                        "Enter up to two letters or numbers, or - to hide the account indicator.",
                    ));
                }
                account.indicator = indicator.into();
            }
            let renamed = account.name != name || account.color != color;
            account.name = name.into();
            account.color = color.into();
            account.eligible = eligible;
            if eligible {
                account.disabled = None;
            }
            store::write(&db, &registry)?;
            // Sessions carry only the name and color; an indicator or switching edit leaves them alone.
            let sessions = if renamed {
                repository.list_sessions(None)?
            } else {
                Vec::new()
            };
            for session in sessions {
                if session
                    .pointer("/runtimeSettings/accountId")
                    .and_then(Value::as_str)
                    == Some(id)
                {
                    let mut runtime = session["runtimeSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    runtime.insert("accountName".into(), json!(name));
                    runtime.insert("accountColor".into(), json!(color));
                    update_session(&repository, &session, runtime)?;
                    changed_sessions.push(session);
                }
            }
        }
        "remove" => {
            let id = required(params, "id")?;
            registry.accounts.retain(|a| a.id != id);
            registry.new_session_accounts.retain(
                |_, c| !matches!(c, NewSessionAccount::Pinned { id: pinned } if pinned == id),
            );
            registry.last_used_accounts.retain(|_, v| v != id);
            store::write(&db, &registry)?;
        }
        "defaultAccount" => {
            let provider = provider_param(params)?;
            let choice: NewSessionAccount =
                serde_json::from_value(params.get("choice").cloned().unwrap_or(Value::Null))
                    .map_err(|_| {
                        DomainStateError::bad_request("Choose how new sessions pick their account.")
                    })?;
            if let NewSessionAccount::Pinned { id } = &choice {
                if !registry
                    .accounts
                    .iter()
                    .any(|a| a.id == *id && a.provider == provider)
                {
                    return Err(DomainStateError::bad_request(
                        "Choose a registered account for this provider.",
                    ));
                }
            }
            if choice == NewSessionAccount::Auto {
                registry.new_session_accounts.remove(&provider);
            } else {
                registry.new_session_accounts.insert(provider, choice);
            }
            store::write(&db, &registry)?;
        }
        "defaults" => {
            let provider = provider_param(params)?;
            let policy: Policy =
                serde_json::from_value(params.get("policy").cloned().unwrap_or(Value::Null))
                    .map_err(|_| DomainStateError::bad_request("Invalid continuation settings."))?;
            registry.defaults.insert(provider, policy);
            store::write(&db, &registry)?;
        }
        // CDXC:AgentProviders 2026-10-04 DECISION:
        // User: the "Couldn't complete the switch" card stuck on screen needs "a close button top right of the modal". Closing a failed switch marks it `cancelled` in the session itself, so it stays closed on reload, on every client and after a restart; Retry and later switches are untouched.
        "dismissSwitch" => {
            let session = get_session(&repository, params)?;
            if session
                .pointer("/runtimeSettings/accountSwitch/phase")
                .and_then(Value::as_str)
                == Some("failed")
            {
                let mut runtime = session["runtimeSettings"]
                    .as_object()
                    .cloned()
                    .unwrap_or_default();
                super::switch_progress::set_phase(&mut runtime, "cancelled", None);
                update_session(&repository, &session, runtime)?;
                changed_sessions.push(session);
            }
        }
        "retryNow" => {
            let session = get_session(&repository, params)?;
            super::recovery::retry_now(state, &repository, &db, &session)?;
            changed_sessions.push(session);
        }
        // A Claude cloud session keeps its account in its cloud record and runs no local agent
        // (claude_cloud/account_pick.rs).
        "select"
            if get_session(&repository, params)?
                .pointer("/runtimeSettings/claudeCloud")
                .is_some_and(Value::is_object) =>
        {
            let session = get_session(&repository, params)?;
            if let Some(selected) = crate::claude_cloud::select_cloud_account(
                state,
                &db,
                &repository,
                &session,
                required(params, "accountId")?,
            ) {
                selected?;
            }
        }
        "sessionPolicy" | "select" | "stopRecovery" => {
            let session = get_session(&repository, params)?;
            let project = repository
                .get_project(required(params, "projectId")?)?
                .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
            let provider = launch::provider(&project, &session).ok_or_else(|| {
                DomainStateError::bad_request(
                    "Accounts are available for Claude and Codex sessions.",
                )
            })?;
            let mut runtime = session["runtimeSettings"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            if operation == "sessionPolicy" {
                let policy = params.get("policy").cloned().unwrap_or(Value::Null);
                if !policy.is_null() {
                    serde_json::from_value::<Policy>(policy.clone()).map_err(|_| {
                        DomainStateError::bad_request("Invalid continuation settings.")
                    })?;
                }
                runtime.insert("accountPolicyOverride".into(), policy);
                runtime.remove("accountRecoverySuppressed");
                // Starts recovery over; a failed episode keeps its marker, and an error the new
                // settings stop retrying is announced once (CDXC:Notifications 2026-10-31 in
                // api_errors.rs).
                super::api_errors::save_settings(state, &repository, &session, runtime)?;
            } else if operation == "stopRecovery" {
                runtime.remove("accountRecovery");
                runtime.insert("accountRecoverySuppressed".into(), json!(true));
                super::switch_progress::set_phase(&mut runtime, "cancelled", None);
                update_session(&repository, &session, runtime)?;
                crate::session_chat_send::cancel_session_chat_sends(
                    required(params, "projectId")?,
                    required(params, "sessionId")?,
                );
            } else {
                // `force` (accounts/cli.rs) lets the in-place restart interrupt the turn and continues it with ".".
                let force = params.get("force").and_then(Value::as_bool) == Some(true);
                if !force
                    && crate::presentation::presentation_activity(
                        &session,
                        &chrono::Utc::now().to_rfc3339(),
                    ) == "working"
                {
                    return Err(DomainStateError::bad_request(
                        "Stop the current turn before switching accounts.",
                    ));
                }
                select(
                    state,
                    &repository,
                    &registry,
                    &snapshot,
                    &project,
                    &session,
                    Some(required(params, "accountId")?),
                    if force && session["lifecycleState"].as_str() == Some("running") {
                        SwitchSource::Forced
                    } else {
                        SwitchSource::Manual
                    },
                )?;
                // Choosing a disabled account by hand is the user saying it works again (disabled.rs); otherwise the next send would move the session straight off it.
                let chosen = required(params, "accountId")?;
                if let Some(account) = registry
                    .accounts
                    .iter_mut()
                    .find(|a| a.id == chosen && a.disabled.is_some())
                {
                    account.disabled = None;
                    account.eligible = true;
                    store::write(&db, &registry)?;
                }
                let selected = get_session(&repository, params)?;
                if selected
                    .pointer("/runtimeSettings/accountSwitchAttempt")
                    .is_none()
                {
                    registry
                        .last_used_accounts
                        .insert(provider, required(params, "accountId")?.to_string());
                    store::write(&db, &registry)?;
                }
            }
            changed_sessions.push(session);
        }
        _ => return Err(DomainStateError::bad_request("Unknown account operation.")),
    }
    for session in changed_sessions {
        publish(state, &repository, &session)?;
    }
    let mut value = state_value(
        &repository,
        &registry,
        &snapshot,
        &state.paths.home_dir,
        params,
    )?;
    if operation == "roster" {
        super::cli::roster(
            &repository,
            &snapshot,
            &mut value,
            params.get("allSessions").and_then(Value::as_bool) == Some(true),
        )?;
    }
    // The Cursor, Gemini and Copilot accounts, which only show usage (accounts/usage_only/).
    if matches!(operation, "list" | "titlebar" | "session") {
        let wait = match (operation, params.get("refresh").and_then(Value::as_bool)) {
            ("list", Some(true)) => super::usage_only::Wait::Always,
            ("list", _) if !cached_list => super::usage_only::Wait::FirstRead,
            _ => super::usage_only::Wait::Never,
        };
        value["usageAccounts"] = json!(super::usage_only::rows(
            &state.paths,
            operation == "titlebar",
            wait
        ));
    }
    if usage_history.is_some() {
        value["usageHistory"] = state.accounts.history.snapshot();
        value["accountCounts"] = json!({
            "claude": registry.accounts.iter().filter(|a| a.provider == Provider::Claude).count(),
            "codex": registry.accounts.iter().filter(|a| a.provider == Provider::Codex).count(),
        });
    }
    Ok(value)
}
pub(crate) fn get_session(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(
            required(params, "projectId")?,
            required(params, "sessionId")?,
        )?
        .ok_or_else(|| DomainStateError::not_found("Session not found."))
}
pub(crate) fn update_session(
    repository: &DomainRepository<'_>,
    session: &Value,
    runtime: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    repository.update_session(&json!({"projectId":session["projectId"],"sessionId":session["sessionId"],"runtimeSettings":runtime}).as_object().unwrap().clone())
}
/// Writes only `members` of the session's runtime settings, as `runtime` has them (removed where it
/// has none), onto the row as stored now. The row is read after SQLite's writer is reserved, so the
/// members other writers stored since the caller read the session (a hook's activity and question
/// card, a published question) stay; inside a caller's transaction the caller holds that reservation.
pub(crate) fn update_session_members(
    repository: &DomainRepository<'_>,
    session: &Value,
    runtime: &Map<String, Value>,
    members: &[&str],
) -> Result<Value, DomainStateError> {
    write_session_members(repository, session, runtime, members, Map::new())
}
/// `update_session_members` for the members `runtime` changed relative to the session it was built
/// from (added, changed or removed), with the other session fields in `other` (an account switch's
/// `launchSettings`) written in the same update.
pub(crate) fn update_session_changes(
    repository: &DomainRepository<'_>,
    session: &Value,
    runtime: &Map<String, Value>,
    other: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let before = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut members: Vec<&str> = before
        .keys()
        .chain(runtime.keys())
        .map(String::as_str)
        .filter(|member| before.get(*member) != runtime.get(*member))
        .collect();
    members.sort_unstable();
    members.dedup();
    write_session_members(repository, session, runtime, &members, other)
}
fn write_session_members(
    repository: &DomainRepository<'_>,
    session: &Value,
    runtime: &Map<String, Value>,
    members: &[&str],
    other: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let own = repository.db.is_autocommit();
    if own {
        repository
            .db
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(store::error)?;
    }
    let written = repository
        .get_session(
            session["projectId"].as_str().unwrap_or_default(),
            session["sessionId"].as_str().unwrap_or_default(),
        )
        .and_then(|row| row.ok_or_else(|| DomainStateError::not_found("Session not found.")))
        .and_then(|row| {
            let mut current = row["runtimeSettings"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for member in members {
                match runtime.get(*member) {
                    Some(value) => current.insert((*member).to_string(), value.clone()),
                    None => current.remove(*member),
                };
            }
            let mut params = other;
            params.insert("projectId".into(), row["projectId"].clone());
            params.insert("sessionId".into(), row["sessionId"].clone());
            params.insert("runtimeSettings".into(), Value::Object(current));
            repository.update_session(&params)
        });
    if own {
        let end = match written {
            Ok(_) => repository.db.execute_batch("COMMIT").map_err(store::error),
            Err(_) => Ok(()),
        };
        if written.is_err() || end.is_err() {
            let _ = repository.db.execute_batch("ROLLBACK");
        }
        end?;
    }
    written
}
pub(crate) fn publish(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    super::switch_progress::publish(state, session);
    crate::server::schedule_presentation_session_delta(
        state,
        repository.db,
        repository,
        session["projectId"].as_str().unwrap_or(""),
        session["sessionId"].as_str().unwrap_or(""),
    )
}
/// Why a session on its own login cannot be switched (see `launch::session_uses_own_login`).
pub(crate) const OWN_LOGIN_SWITCH_REFUSED: &str =
    "This agent uses its own login, set in its command, so Ghostex doesn't switch its account.";
pub(crate) fn select(
    state: &AppState,
    repository: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    project: &Value,
    session: &Value,
    id: Option<&str>,
    source: SwitchSource,
) -> Result<(), DomainStateError> {
    crate::agentbox::refuse_for_agentbox_session(session, "Switching the account")?;
    let provider = launch::provider(project, session)
        .ok_or_else(|| DomainStateError::bad_request("Unsupported account provider."))?;
    if launch::session_uses_own_login(provider, session) {
        return Err(DomainStateError::bad_request(OWN_LOGIN_SWITCH_REFUSED));
    }
    let current = session
        .pointer("/runtimeSettings/accountId")
        .and_then(Value::as_str);
    if session
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .is_some()
        && session
            .pointer("/runtimeSettings/accountRecovery/status")
            .and_then(Value::as_str)
            == Some("retrying")
    {
        return Err(DomainStateError::bad_request(
            "Wait for the current account switch to finish before choosing another account.",
        ));
    }
    let retrying_failed_switch = session
        .pointer("/runtimeSettings/accountSwitch/phase")
        .and_then(Value::as_str)
        == Some("failed")
        && session
            .pointer("/runtimeSettings/accountSwitch/toAccountId")
            .and_then(Value::as_str)
            == id;
    if current == id && !retrying_failed_switch {
        return Ok(());
    }
    let mut runtime = session["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    if let Some(id) = id {
        let account = registry
            .accounts
            .iter()
            .find(|a| a.id == id && a.provider == provider)
            .ok_or_else(|| {
                DomainStateError::bad_request("Choose a registered account for this provider.")
            })?;
        if account.user_disabled {
            return Err(DomainStateError::bad_request(
                "This account is turned off in Settings > Accounts. Turn it on there to switch to it.",
            ));
        }
        if !snapshot.accounts.iter().any(|a| {
            a.provider == provider
                && a.selector == account.selector
                && a.identity == account.identity
                && a.status == "ready"
        }) {
            return Err(DomainStateError::bad_request(
                "This account needs to be reconnected first.",
            ));
        }
        // The resumed conversation has to be where the new account's Claude reads it; see conversation_link.rs.
        if provider == Provider::Claude {
            super::conversation_link::bring_claude_conversation(
                &state.paths.home_dir,
                session,
                account,
            )?;
        }
        let command = launch::launch_command(&state.paths.home_dir, account)?;
        launch::assign(&mut runtime, account, command)?;
    } else {
        for k in [
            "accountId",
            "accountName",
            "accountColor",
            "accountProvider",
            "accountCommand",
            "agentCommand",
        ] {
            runtime.remove(k);
        }
        runtime.insert("accountId".into(), Value::Null);
        let base = runtime
            .get("accountBaseCommand")
            .and_then(Value::as_str)
            .unwrap_or(provider.id());
        let command = launch::with_account_command(base, provider, provider.id())?;
        runtime.insert("accountCommand".into(), json!(command));
    }
    // An automatic switch keeps the error episode; one the user picks ends it (api_errors.rs).
    super::api_errors::switch_recovery(&mut runtime, None, source == SwitchSource::Automatic);
    // Every switch hides usage-limit notices from the switch time on, whether or not the cache held one: the resumed CLI repaints the previous login's limit from the transcript, and that replay must never read as the new login's limit. See `suppress_account_usage_notice`.
    let switched_at = chrono::Utc::now();
    runtime.remove("accountSuppressedUsageNotice");
    runtime.insert(
        "accountSuppressedUsageNoticeAt".into(),
        json!(switched_at.to_rfc3339()),
    );
    let was_running = session["lifecycleState"].as_str() == Some("running");
    let mut launch_settings = session["launchSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
        launch_settings.remove(key);
    }
    if super::drafts::needs_fresh_launch(session) {
        super::drafts::rebuild_launch(
            repository,
            project,
            session,
            &mut runtime,
            &mut launch_settings,
        )?;
    }
    let is_draft = crate::agents::session_is_draft(session);
    let reuse_command = if is_draft {
        super::drafts::prepare_live_switch(repository, session, &mut launch_settings)?
    } else {
        None
    };
    // A running conversation whose daemon is alive restarts in place (see accounts/restart.rs). The plan is resolved from the row as it will be saved, so a session that cannot be resumed is refused before anything changes.
    let restart =
        if was_running && !is_draft && super::restart::provider_is_live(repository, session) {
            let mut planned = session.clone();
            planned["runtimeSettings"] = Value::Object(runtime.clone());
            planned["launchSettings"] = Value::Object(launch_settings.clone());
            Some(super::restart::plan(state, repository, project, &planned)?)
        } else {
            None
        };
    if was_running && !is_draft && restart.is_none() {
        cycle(state, repository, session, "/api/sleepSession")?;
    }
    let restarting_in_place = restart.is_some();
    if restarting_in_place {
        super::restart::retain_current_account(&mut runtime, session);
    }
    runtime.insert("accountSwitch".into(), json!({
        "id": uuid::Uuid::new_v4().to_string(),
        "provider": provider,
        "source": if source == SwitchSource::Automatic { "automatic" } else { "manual" },
        "phase": "switching",
        "fromAccountId": super::session_identity::display_account_id(registry, provider, session, &state.paths.home_dir),
        "toAccountId": id,
        "updatedAt": chrono::Utc::now().to_rfc3339(),
    }));
    // Only what the switch changed, onto the row as stored now (CDXC:Notifications 2026-10-31 in
    // api_errors.rs): a question card or published question stored meanwhile stays.
    let launch_settings = json!({ "launchSettings": launch_settings });
    let updated = update_session_changes(
        repository,
        session,
        &runtime,
        launch_settings.as_object().cloned().unwrap_or_default(),
    )?;
    super::switch_progress::publish(state, &updated);
    if !restarting_in_place {
        crate::session_chat_notice::suppress_account_usage_notice(
            session["projectId"].as_str().unwrap_or_default(),
            session["sessionId"].as_str().unwrap_or_default(),
            switched_at,
        );
    }
    let restart_result = if let Some(command) = reuse_command {
        super::drafts::switch_in_live_provider(repository, &updated, &command)
    } else if let Some(plan) = restart {
        super::restart::start(state, repository, &updated, plan, source)
    } else if was_running {
        cycle(state, repository, &updated, "/api/wakeSession")
    } else {
        Ok(())
    };
    if let Err(error) = restart_result {
        super::switch_progress::fail(state, repository, &updated, &error.message);
        return Err(error);
    }
    crate::session_chat_options::forget_session_chat_options(
        state,
        session["projectId"].as_str().unwrap_or(""),
        session["sessionId"].as_str().unwrap_or(""),
    );
    // The in-place restart starts an automatic continuation only after verifying the new login; starting it here would let the dot reach the CLI that is about to exit.
    if !restarting_in_place {
        super::continuation::start(state, repository, session, source)?;
    }
    crate::session_chat_options::session_chat_terminal_notice_publisher(
        state,
        session["projectId"].as_str().unwrap_or_default(),
        session["sessionId"].as_str().unwrap_or_default(),
    )();
    Ok(())
}
pub(crate) fn cycle(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
    path: &str,
) -> Result<(), DomainStateError> {
    let context = crate::zmx::ZmxServerContext {
        auth_token_file: state.paths.auth_token_file.to_string_lossy().into_owned(),
        base_url: format!(
            "http://{}:{}",
            state.config.listeners.local.host, state.config.listeners.local.port
        ),
    };
    let params = json!({"projectId":session["projectId"],"sessionId":session["sessionId"]});
    crate::zmx::dispatch_zmx_lifecycle_endpoint(
        repository,
        path,
        params.as_object().unwrap(),
        &context,
        &crate::agents::read_agent_settings(repository.db)?,
    )
    .map_err(|e| match e {
        crate::zmx::ZmxEndpointError::Domain(e) => e,
        crate::zmx::ZmxEndpointError::DependencyUnavailable(s) => store::error(s),
    })?;
    Ok(())
}
/// CDXC:AgentProviders 2026-09-11 WHY:
/// Account refreshes need binding counts, not hydrated session payloads. Count every lifecycle and project exactly as the full session list did, and exclude non-string account IDs.
fn session_account_counts(
    repository: &DomainRepository<'_>,
) -> Result<HashMap<String, u64>, DomainStateError> {
    let mut statement = repository
        .db
        .prepare_cached(
            "SELECT json_extract(runtimeSettingsJson, '$.accountId'), COUNT(*)
         FROM sessions
         WHERE json_type(runtimeSettingsJson, '$.accountId') = 'text'
         GROUP BY json_extract(runtimeSettingsJson, '$.accountId')",
        )
        .map_err(store::error)?;
    let counts = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
        })
        .map_err(store::error)?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(store::error)?;
    Ok(counts)
}
fn state_value(
    repository: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    home: &std::path::Path,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let operation = params.get("operation").and_then(Value::as_str);
    let titlebar = operation == Some("titlebar");
    // CDXC:AgentProviders 2026-10-23 WHY:
    // `sessionCount` is drawn only by Settings > Accounts (`list` and the mutations it sends) and `ghostex accounts` (`roster`). The `session` answer feeds the chat's and the sidebar's account menus, which never read it, and every open chat asks for it every 30 seconds; counting walks every session row's JSON, so `session` skips it as `titlebar` already did.
    let session_counts = if titlebar || operation == Some("session") {
        HashMap::new()
    } else {
        session_account_counts(repository)?
    };
    let mut rows = Vec::new();
    for found in &snapshot.accounts {
        let id = account_id(found);
        let saved = registry.accounts.iter().find(|a| a.id == id);
        rows.push(json!({"id":id,"provider":found.provider,"selector":found.selector,"name":saved.map(|a|a.name.as_str()).unwrap_or(&found.name),"email":found.email,"indicator":saved.map(|a|a.indicator.as_str()).unwrap_or(""),"color":saved.map(|a|a.color.as_str()).unwrap_or("neutral"),"eligible":saved.is_some_and(|a|a.eligible),"registered":saved.is_some(),"showInTitlebar":saved.is_some_and(|a|a.show_in_titlebar&&!a.user_disabled),"userDisabled":saved.is_some_and(|a|a.user_disabled),"sharedHistory":saved.is_some_and(|a|a.shared_history)||found.shared_history,"disabledReason":saved.and_then(|a|a.disabled.as_ref()).map(|d|d.reason.as_str()),"status":found.status,"usage":found.usage,"resetCredits":found.reset_credits,"resetCreditDetails":found.reset_credit_details,"resetCreditsError":found.reset_credits_error,"usageUpdatedAt":found.usage_updated_at,"usageError":found.usage_error,"usageStale":found.usage_stale,"sessionCount":session_counts.get(&id).copied().unwrap_or(0)}));
    }
    let loading = titlebar && snapshot.fetched_at.is_none();
    for saved in &registry.accounts {
        if !rows.iter().any(|r| r["id"].as_str() == Some(&saved.id)) {
            rows.push(json!({"id":saved.id,"provider":saved.provider,"selector":saved.selector,"name":saved.name,"email":"","indicator":saved.indicator,"color":saved.color,"eligible":saved.eligible,"registered":true,"showInTitlebar":saved.show_in_titlebar&&!saved.user_disabled,"userDisabled":saved.user_disabled,"sharedHistory":saved.shared_history,"status":if saved.user_disabled {"disabled"} else if loading {"loading"} else {"unavailable"},"usage":[],"usageError":if loading || saved.user_disabled {None} else {Some(SAVED_LOGIN_MISSING)},"sessionCount":session_counts.get(&saved.id).copied().unwrap_or(0)}));
        }
    }
    if titlebar {
        rows.retain(|row| row["registered"] == true && row["showInTitlebar"] == true);
    }
    // CDXC:AgentProviders 2026-09-18 DECISION:
    // User: Claude Swap and Codex Swap must work on Windows, under PowerShell and WSL.
    // Supersedes the 2026-09-06 note that Codex Swap installs through Homebrew on macOS and Linux without requiring Cargo: Homebrew stays the macOS and Linux route, but a computer without brew (the usual case inside WSL) was being handed a command it cannot run, so it now gets the source build. Windows uses codex-swap's own installer and needs neither. See Provider::install_command.
    let helper_rows:Vec<_>=[Provider::Claude,Provider::Codex].into_iter().map(|p|json!({"provider":p,"installed":helpers::executable(home,p.helper()).is_some(),"cliInstalled":helpers::executable(home,p.id()).is_some(),"error":snapshot.errors.get(&p),"installCommand":p.install_command(home),"loginCommand":p.login_command(),"codexSwapNeedsUpdate":p==Provider::Codex&&super::helper_tools::codex_swap_needs_update_to_add(home)})).collect();
    let mut value = json!({"accounts":rows,"helpers":helper_rows,"defaults":{"claude":registry.defaults.get(&Provider::Claude).cloned().unwrap_or_default(),"codex":registry.defaults.get(&Provider::Codex).cloned().unwrap_or_default()},"defaultAccounts":super::default_account::quick_launch_accounts(registry,snapshot),"newSessionAccounts":registry.new_session_accounts});
    // CDXC:Workspaces 2026-10-09 DECISION:
    // User: the sidebar launcher's highlighted default account shows the workspace's Claude
    // account when the workspace sets one, so what it shows is what launches. A launcher list for a
    // project (`projectId`, no `sessionId`) reports the account `launch::apply_new_session` would
    // pick for a new session there: the workspace's account while it is still registered, else
    // the provider's rule.
    if !params.contains_key("sessionId") {
        if let Some(project) = params
            .get("projectId")
            .and_then(Value::as_str)
            .map(|project_id| repository.get_project(project_id))
            .transpose()?
            .flatten()
        {
            if let Some(account_id) =
                crate::workspaces::project_claude_account_id(repository.db, &project)?.filter(
                    |id| {
                        registry.accounts.iter().any(|account| {
                            account.id == *id && account.provider == Provider::Claude
                        })
                    },
                )
            {
                value["defaultAccounts"]["claude"] = json!(account_id);
            }
        }
    }
    if params.contains_key("sessionId") {
        let session = get_session(repository, params)?;
        let project = repository
            .get_project(required(params, "projectId")?)?
            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
        if let Some(cloud) = crate::claude_cloud::cloud_session_accounts(&session) {
            value["session"] = cloud;
        } else if let Some(p) = launch::provider(&project, &session) {
            let account_id =
                super::session_identity::display_account_id(registry, p, &session, home);
            value["session"] = json!({"provider":p,"accountId":account_id,"override":session.pointer("/runtimeSettings/accountPolicyOverride"),"policy":launch::effective_policy(registry,p,&session),"recovery":super::api_errors::shown_recovery(&session),"accountSwitch":session.pointer("/runtimeSettings/accountSwitch")});
            if launch::session_uses_own_login(p, &session) {
                value["session"]["usesOwnLogin"] = json!(true);
            }
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new account shows its usage in the sidebar and may be switched to automatically.
    #[test]
    fn a_new_account_starts_shown_in_the_sidebar_and_eligible() {
        let found = DiscoveredAccount {
            provider: Provider::Claude,
            selector: "3".into(),
            identity: "someone@example.com".into(),
            name: "Work".into(),
            email: "someone@example.com".into(),
            status: "ready".into(),
            shared_history: true,
            usage: vec![],
            reset_credits: None,
            reset_credit_details: None,
            reset_credits_error: None,
            usage_updated_at: None,
            usage_error: None,
            usage_stale: false,
        };
        let saved = new_saved_account(&found);
        assert_eq!(saved.id, account_id(&found));
        assert_eq!(
            (saved.selector.as_str(), saved.name.as_str()),
            ("3", "Work")
        );
        assert!(saved.show_in_titlebar && saved.eligible && saved.shared_history);
        assert!(!saved.user_disabled && saved.disabled.is_none());
    }

    /// An account saved before this default existed keeps the sidebar choice it has.
    #[test]
    fn a_saved_account_keeps_its_sidebar_choice() {
        let hidden: SavedAccount = serde_json::from_value(json!({
            "id": "claude-1", "provider": "claude", "selector": "1", "identity": "a",
            "name": "A", "color": "neutral", "showInTitlebar": false,
            "eligible": true, "sharedHistory": true
        }))
        .unwrap();
        assert!(!hidden.show_in_titlebar);
    }
}
