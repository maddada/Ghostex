use super::{helpers, launch, model::*, store};
use crate::{
    domain::{DomainRepository, DomainStateError},
    server::AppState,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
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
    let snapshot = state.accounts.refresh(
        &state.paths.home_dir,
        params.get("refresh").and_then(Value::as_bool) == Some(true)
            || matches!(operation, "register" | "select"),
    );
    let _gate = state.accounts.mutations.lock().map_err(store::error)?;
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let mut registry = store::read(&db)?;
    let mut changed_sessions = Vec::new();
    match operation {
        "list" | "session" => {}
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
                    launch::assign(&mut runtime, account, command.clone());
                    let mut settings = session["launchSettings"]
                        .as_object()
                        .cloned()
                        .unwrap_or_default();
                    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
                        settings.remove(key);
                    }
                    if session
                        .pointer("/runtimeSettings/agentSessionId")
                        .and_then(Value::as_str)
                        .is_none()
                    {
                        let project = repository
                            .get_project(session["projectId"].as_str().unwrap_or(""))?
                            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
                        let params = json!({"agentId":session["agentId"],"runtimeSettings":runtime,"launchSettings":settings});
                        let fresh = crate::agents::create_agent_session_params_for_project(
                            &db,
                            &project,
                            params.as_object().unwrap(),
                        )?;
                        runtime = fresh["runtimeSettings"]
                            .as_object()
                            .cloned()
                            .unwrap_or_default();
                        settings = fresh["launchSettings"]
                            .as_object()
                            .cloned()
                            .unwrap_or_default();
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
            registry.default_accounts.retain(|_, v| v != id);
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
        "defaultAccount" => {
            let provider = provider_param(params)?;
            if let Some(id) = params.get("accountId").and_then(Value::as_str) {
                if !registry
                    .accounts
                    .iter()
                    .any(|a| a.id == id && a.provider == provider)
                {
                    return Err(DomainStateError::bad_request(
                        "Choose a registered account for this provider.",
                    ));
                }
                registry.default_accounts.insert(provider, id.into());
            } else {
                registry.default_accounts.remove(&provider);
            }
            store::write(&db, &registry)?;
        }
        "sessionPolicy" | "select" | "stopRecovery" => {
            let session = get_session(&repository, params)?;
            let project = repository
                .get_project(required(params, "projectId")?)?
                .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
            launch::provider(&project, &session).ok_or_else(|| {
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
                    params.get("accountId").and_then(Value::as_str),
                )?;
            }
            changed_sessions.push(session);
        }
        _ => return Err(DomainStateError::bad_request("Unknown account operation.")),
    }
    for session in changed_sessions {
        publish(state, &repository, &session)?;
    }
    state_value(
        &repository,
        &registry,
        &snapshot,
        &state.paths.home_dir,
        params,
    )
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
        launch::assign(&mut runtime, account, command);
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
        runtime.insert("accountCommand".into(), json!(provider.id()));
    }
    runtime.remove("accountRecovery");
    let prompted = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty());
    let was_running = session["lifecycleState"].as_str() == Some("running");
    let mut launch_settings = session["launchSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    for key in ["agentLaunchPlan", "agentResumePlan", "agentCommand"] {
        launch_settings.remove(key);
    }
    if !prompted {
        let params = json!({"agentId":session["agentId"],"runtimeSettings":runtime,"launchSettings":launch_settings});
        let fresh = crate::agents::create_agent_session_params_for_project(
            repository.db,
            project,
            params.as_object().unwrap(),
        )?;
        runtime = fresh["runtimeSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
        launch_settings = fresh["launchSettings"]
            .as_object()
            .cloned()
            .unwrap_or_default();
    }
    if was_running {
        cycle(state, repository, session, "/api/sleepSession")?;
    }
    let updated = update_session(repository, session, runtime)?;
    repository.update_session(&json!({"projectId":session["projectId"],"sessionId":session["sessionId"],"launchSettings":launch_settings}).as_object().unwrap().clone())?;
    if was_running {
        cycle(state, repository, &updated, "/api/wakeSession")?;
    }
    crate::session_chat_options::forget_session_chat_options(
        state,
        session["projectId"].as_str().unwrap_or(""),
        session["sessionId"].as_str().unwrap_or(""),
    );
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
fn state_value(
    repository: &DomainRepository<'_>,
    registry: &Registry,
    snapshot: &Snapshot,
    home: &std::path::Path,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let sessions = repository.list_sessions(None)?;
    let mut rows = Vec::new();
    for found in &snapshot.accounts {
        let id = account_id(found);
        let saved = registry.accounts.iter().find(|a| a.id == id);
        rows.push(json!({"id":id,"provider":found.provider,"selector":found.selector,"name":saved.map(|a|a.name.as_str()).unwrap_or(&found.name),"email":found.email,"color":saved.map(|a|a.color.as_str()).unwrap_or("neutral"),"eligible":saved.is_some_and(|a|a.eligible),"registered":saved.is_some(),"sharedHistory":saved.is_some_and(|a|a.shared_history)||found.shared_history,"status":found.status,"usage":found.usage,"usageUpdatedAt":found.usage_updated_at,"usageError":found.usage_error,"sessionCount":sessions.iter().filter(|s|s.pointer("/runtimeSettings/accountId").and_then(Value::as_str)==Some(&id)).count()}));
    }
    for saved in &registry.accounts {
        if !rows.iter().any(|r| r["id"].as_str() == Some(&saved.id)) {
            rows.push(json!({"id":saved.id,"provider":saved.provider,"selector":saved.selector,"name":saved.name,"email":"","color":saved.color,"eligible":saved.eligible,"registered":true,"sharedHistory":saved.shared_history,"status":"unavailable","usage":[],"usageError":"The helper could not find this saved login. Refresh or reconnect it.","sessionCount":sessions.iter().filter(|s|s.pointer("/runtimeSettings/accountId").and_then(Value::as_str)==Some(&saved.id)).count()}));
        }
    }
    let helper_rows:Vec<_>=[Provider::Claude,Provider::Codex].into_iter().map(|p|json!({"provider":p,"installed":helpers::executable(home,p.helper()).is_some(),"cliInstalled":helpers::executable(home,p.id()).is_some(),"error":snapshot.errors.get(&p),"installCommand":if p==Provider::Claude{"uv tool install claude-swap"}else{"cargo install --git https://github.com/maddada/codex-swap --locked"},"loginCommand":if p==Provider::Claude{"claude auth login && cswap add"}else{"xswap add --login --share-history"}})).collect();
    let mut value = json!({"accounts":rows,"helpers":helper_rows,"defaults":{"claude":registry.defaults.get(&Provider::Claude).cloned().unwrap_or_default(),"codex":registry.defaults.get(&Provider::Codex).cloned().unwrap_or_default()},"defaultAccounts":registry.default_accounts});
    if params.contains_key("sessionId") {
        let session = get_session(repository, params)?;
        let project = repository
            .get_project(required(params, "projectId")?)?
            .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
        if let Some(p) = launch::provider(&project, &session) {
            value["session"] = json!({"provider":p,"accountId":session.pointer("/runtimeSettings/accountId"),"override":session.pointer("/runtimeSettings/accountPolicyOverride"),"policy":launch::effective_policy(registry,p,&session),"recovery":session.pointer("/runtimeSettings/accountRecovery")});
        }
    }
    Ok(value)
}
