use super::{helpers, launch, model::*, store};
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
pub(crate) fn account_id(account: &DiscoveredAccount) -> String {
    format!(
        "{}-{:x}",
        account.provider.id(),
        Sha256::digest(account.identity.as_bytes())
    )
}
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let operation = required(params, "operation")?;
    if operation.starts_with("setup") {
        return super::setup::dispatch(state, params);
    }
    if operation == "prepareReset" {
        return super::reset_credits::prepare(state, required(params, "id")?);
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
    let mut snapshot = if matches!(operation, "select" | "setTitlebar")
        || !titlebar_has_accounts
        || cached_titlebar
    {
        state.accounts.snapshot()
    } else {
        state.accounts.refresh(
            &state.paths.home_dir,
            params.get("refresh").and_then(Value::as_bool) == Some(true) || operation == "register",
        )
    };
    let _gate = state.accounts.mutations.lock().map_err(store::error)?;
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let mut registry = store::read(&db)?;
    let mut changed_sessions = Vec::new();
    match operation {
        "list" | "session" | "titlebar" => {}
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
                    DomainStateError::bad_request("Install the account helper first.")
                })?;
            let output = std::process::Command::new(binary)
                .args(["swap", &first.selector, &second.selector])
                .stdin(std::process::Stdio::null())
                .output()
                .map_err(store::error)?;
            if !output.status.success() {
                return Err(DomainStateError::bad_request("The account helper could not swap these slots. For Claude, stop sessions using either account first, then try again."));
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
            }
            if !reconnecting {
                if registry.accounts.len() >= 50 {
                    return Err(DomainStateError::bad_request(
                        "At most 50 accounts can be registered.",
                    ));
                }
                registry.accounts.push(SavedAccount {
                    id: id.clone(),
                    provider,
                    selector: found.selector.clone(),
                    identity: found.identity.clone(),
                    name: found.name.clone(),
                    color: "neutral".into(),
                    indicator: String::new(),
                    show_in_titlebar: false,
                    eligible: true,
                    shared_history: true,
                });
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
            account.name = name.into();
            account.color = color.into();
            account.eligible = eligible;
            store::write(&db, &registry)?;
            for session in repository.list_sessions(None)? {
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
            if choice == NewSessionAccount::MostRemaining {
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
                runtime.remove("accountRecovery");
                runtime.remove("accountRecoverySuppressed");
                update_session(&repository, &session, runtime)?;
            } else if operation == "stopRecovery" {
                runtime.remove("accountRecovery");
                runtime.insert("accountRecoverySuppressed".into(), json!(true));
                update_session(&repository, &session, runtime)?;
                crate::session_chat_send::cancel_session_chat_sends(
                    required(params, "projectId")?,
                    required(params, "sessionId")?,
                );
            } else {
                if crate::presentation::presentation_activity(
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
                )?;
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
pub(crate) fn publish(
    state: &AppState,
    repository: &DomainRepository<'_>,
    session: &Value,
) -> Result<(), DomainStateError> {
    crate::server::schedule_presentation_session_delta(
        state,
        repository.db,
        repository,
        session["projectId"].as_str().unwrap_or(""),
        session["sessionId"].as_str().unwrap_or(""),
    )
}
pub(crate) fn select(
    state: &AppState,
    repository: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    project: &Value,
    session: &Value,
    id: Option<&str>,
) -> Result<(), DomainStateError> {
    let provider = launch::provider(project, session)
        .ok_or_else(|| DomainStateError::bad_request("Unsupported account provider."))?;
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
    if current == id {
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
        let command = launch::command(&state.paths.home_dir, account)?;
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
    runtime.remove("accountRecovery");
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
    let updated = repository.update_session(
        json!({
            "projectId": session["projectId"],
            "sessionId": session["sessionId"],
            "runtimeSettings": runtime,
            "launchSettings": launch_settings,
        })
        .as_object()
        .unwrap(),
    )?;
    if !restarting_in_place {
        crate::session_chat_notice::suppress_account_usage_notice(
            session["projectId"].as_str().unwrap_or_default(),
            session["sessionId"].as_str().unwrap_or_default(),
            switched_at,
        );
    }
    if let Some(command) = reuse_command {
        super::drafts::switch_in_live_provider(repository, &updated, &command)?;
    } else if let Some(plan) = restart {
        super::restart::start(state, repository, &updated, plan)?;
    } else if was_running {
        cycle(state, repository, &updated, "/api/wakeSession")?;
    }
    crate::session_chat_options::forget_session_chat_options(
        state,
        session["projectId"].as_str().unwrap_or(""),
        session["sessionId"].as_str().unwrap_or(""),
    );
    // The in-place restart starts the continuation itself, once the resume command has been typed; starting it here would let the dot reach the CLI that is about to exit.
    if !restarting_in_place {
        super::continuation::start(state, repository, session)?;
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
    let titlebar = params.get("operation").and_then(Value::as_str) == Some("titlebar");
    let session_counts = if titlebar {
        HashMap::new()
    } else {
        session_account_counts(repository)?
    };
    let mut rows = Vec::new();
    for found in &snapshot.accounts {
        let id = account_id(found);
        let saved = registry.accounts.iter().find(|a| a.id == id);
        rows.push(json!({"id":id,"provider":found.provider,"selector":found.selector,"name":saved.map(|a|a.name.as_str()).unwrap_or(&found.name),"email":found.email,"indicator":saved.map(|a|a.indicator.as_str()).unwrap_or(""),"color":saved.map(|a|a.color.as_str()).unwrap_or("neutral"),"eligible":saved.is_some_and(|a|a.eligible),"registered":saved.is_some(),"showInTitlebar":saved.is_some_and(|a|a.show_in_titlebar),"sharedHistory":saved.is_some_and(|a|a.shared_history)||found.shared_history,"status":found.status,"usage":found.usage,"resetCredits":found.reset_credits,"resetCreditDetails":found.reset_credit_details,"resetCreditsError":found.reset_credits_error,"usageUpdatedAt":found.usage_updated_at,"usageError":found.usage_error,"sessionCount":session_counts.get(&id).copied().unwrap_or(0)}));
    }
    let loading = titlebar && snapshot.fetched_at.is_none();
    for saved in &registry.accounts {
        if !rows.iter().any(|r| r["id"].as_str() == Some(&saved.id)) {
            rows.push(json!({"id":saved.id,"provider":saved.provider,"selector":saved.selector,"name":saved.name,"email":"","indicator":saved.indicator,"color":saved.color,"eligible":saved.eligible,"registered":true,"showInTitlebar":saved.show_in_titlebar,"sharedHistory":saved.shared_history,"status":if loading {"loading"} else {"unavailable"},"usage":[],"usageError":if loading {None} else {Some("The helper could not find this saved login. Refresh or reconnect it.")},"sessionCount":session_counts.get(&saved.id).copied().unwrap_or(0)}));
        }
    }
    if titlebar {
        rows.retain(|row| row["registered"] == true && row["showInTitlebar"] == true);
    }
    // CDXC:AgentProviders 2026-09-06 DECISION:
    // User: Codex Swap should install through Homebrew on macOS and Linux without requiring Cargo.
    let helper_rows:Vec<_>=[Provider::Claude,Provider::Codex].into_iter().map(|p|json!({"provider":p,"installed":helpers::executable(home,p.helper()).is_some(),"cliInstalled":helpers::executable(home,p.id()).is_some(),"error":snapshot.errors.get(&p),"installCommand":if p==Provider::Claude{"uv tool install claude-swap"}else{"brew install maddada/tap/codex-swap"},"loginCommand":if p==Provider::Claude{"ghostex account-login claude"}else{"xswap add --login --share-history"}})).collect();
    let mut value = json!({"accounts":rows,"helpers":helper_rows,"defaults":{"claude":registry.defaults.get(&Provider::Claude).cloned().unwrap_or_default(),"codex":registry.defaults.get(&Provider::Codex).cloned().unwrap_or_default()},"defaultAccounts":super::default_account::quick_launch_accounts(registry,snapshot),"newSessionAccounts":registry.new_session_accounts});
    if params.contains_key("sessionId") {
        let session = get_session(repository, params)?;
        let project = repository
            .get_project(required(params, "projectId")?)?
            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
        if let Some(p) = launch::provider(&project, &session) {
            let account_id =
                super::session_identity::display_account_id(registry, p, &session, home);
            value["session"] = json!({"provider":p,"accountId":account_id,"override":session.pointer("/runtimeSettings/accountPolicyOverride"),"policy":launch::effective_policy(registry,p,&session),"recovery":session.pointer("/runtimeSettings/accountRecovery")});
        }
    }
    Ok(value)
}
