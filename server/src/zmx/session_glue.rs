use std::path::Path;

use serde_json::{json, Map, Value};

use crate::{
    agents::get_agent_startup_text_for_session,
    domain::{read_project_id, read_session_id, DomainRepository, DomainStateError},
    toolchain::{require_bundled_zmx, GxserverResolvedTool},
};

use super::*;

pub(crate) fn read_lifecycle_params(
    params: &Map<String, Value>,
) -> Result<LifecycleParams, DomainStateError> {
    Ok(LifecycleParams {
        project_id: read_project_id(params)?,
        session_id: read_session_id(params)?,
    })
}

pub(crate) fn prompt_editor_mode_from_params(
    params: &Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    match params.get("promptEditor") {
        None => Ok(None),
        Some(Value::String(value)) if matches!(value.as_str(), "monaco" | "code-server") => {
            Ok(Some(value.clone()))
        }
        Some(_) => Err(DomainStateError::bad_request(
            "promptEditor must be monaco or code-server when provided.",
        )),
    }
}

pub(crate) fn js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| js_string(Some(value)))
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".to_string(),
        Some(value) => value.to_string(),
    }
}

pub(crate) fn require_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {}/{} does not exist.",
                lifecycle.project_id, lifecycle.session_id
            ))
        })
}

/// Whether this session has ever entered working or attention.
///
/// CDXC:AutoSleepNeverActive 2026-08-22: the durable `lastActiveAt` column is
/// written only by an activity transition, so its absence is the daemon's own
/// record that nobody has prompted this terminal yet. Presentation publishes the
/// same fact as `hasEverBeenActive` because the projected `lastActiveAt` there
/// falls back to `createdAt`.
pub(crate) fn session_has_ever_been_active(session: &Value) -> bool {
    session
        .get("lastActiveAt")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

pub(crate) fn require_zmx() -> ZmxEndpointResult<GxserverResolvedTool> {
    require_bundled_zmx().map_err(ZmxEndpointError::DependencyUnavailable)
}

pub(crate) fn provider_zmx_session_name(session: &Value) -> Result<String, DomainStateError> {
    /*
    CDXC:GxserverUnsupportedSessionParity 2026-06-22-07:30:
    TypeScript gxserver does not have an unsupported-session branch for zmx lifecycle or session-I/O endpoints. Route every persisted session through the canonical top-level zmxName, ignoring providerState.provider, providerState.zmxName, and runtimeSettings.sessionPersistenceProvider so provider-off, missing-provider, and migrated rows keep the same error/order behavior.
    */
    string_field(session, "zmxName").ok_or_else(|| {
        DomainStateError::corrupt_state("zmxName missing from session domain state.")
    })
}

pub(crate) fn provider_state_patch(
    session: &Value,
    probe: &ProviderProbe,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.remove("killError");
    provider_state.insert(
        "lifecycleState".to_string(),
        Value::String(probe.lifecycle_state.clone()),
    );
    if let Some(error) = &probe.error {
        provider_state.insert("probeError".to_string(), Value::String(error.clone()));
    } else {
        provider_state.remove("probeError");
    }
    provider_state.insert(
        "probedAt".to_string(),
        Value::String(probe.probed_at.clone()),
    );
    provider_state.insert("zmxName".to_string(), Value::String(probe.zmx_name.clone()));
    Ok(provider_state)
}

pub(crate) fn missing_provider_state_patch(
    session: &Value,
    timestamp: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.remove("killError");
    provider_state.remove("probeError");
    // No daemon, so nothing was spawned by any zmx binary. See CDXC:ZmxWireCycle.
    provider_state.remove(ZMX_WIRE_GENERATION_KEY);
    provider_state.remove(LEGACY_ZMX_BINARY_STAMP_KEY);
    provider_state.insert("lifecycleState".to_string(), json!("missing"));
    provider_state.insert("probedAt".to_string(), json!(timestamp));
    provider_state.insert(
        "zmxName".to_string(),
        json!(provider_zmx_session_name(session)?),
    );
    Ok(provider_state)
}

pub(crate) fn failed_kill_provider_state_patch(
    session: &Value,
    kill: &ProviderKill,
    timestamp: &str,
) -> Result<Map<String, Value>, DomainStateError> {
    let error = kill
        .error
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| (!kill.stderr.trim().is_empty()).then(|| kill.stderr.clone()))
        .unwrap_or_else(|| format!("zmx kill command exited {}", kill.exit_code));
    let mut provider_state = session
        .get("providerState")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    provider_state.insert("killError".to_string(), Value::String(error.clone()));
    provider_state.insert("lifecycleState".to_string(), json!("unknown"));
    provider_state.insert("probeError".to_string(), Value::String(error));
    provider_state.insert("probedAt".to_string(), json!(timestamp));
    provider_state.insert(
        "zmxName".to_string(),
        json!(provider_zmx_session_name(session)?),
    );
    Ok(provider_state)
}

pub(crate) fn reconcile_domain_lifecycle_from_provider_probe(
    current_lifecycle_state: &str,
    provider_lifecycle_state: &str,
) -> String {
    if provider_lifecycle_state == "exists" && current_lifecycle_state != "stopped" {
        "running".to_string()
    } else {
        current_lifecycle_state.to_string()
    }
}

pub(crate) fn probe_to_value(probe: &ProviderProbe) -> Value {
    let mut value = Map::new();
    if let Some(error) = &probe.error {
        value.insert("error".to_string(), Value::String(error.clone()));
    }
    value.insert(
        "lifecycleState".to_string(),
        Value::String(probe.lifecycle_state.clone()),
    );
    value.insert(
        "probedAt".to_string(),
        Value::String(probe.probed_at.clone()),
    );
    value.insert("zmxName".to_string(), Value::String(probe.zmx_name.clone()));
    Value::Object(value)
}

pub(crate) fn kill_to_value(kill: &ProviderKill) -> Value {
    let mut value = Map::new();
    if let Some(error) = &kill.error {
        value.insert("error".to_string(), Value::String(error.clone()));
    }
    value.insert("exitCode".to_string(), json!(kill.exit_code));
    value.insert("killed".to_string(), Value::Bool(kill.killed));
    value.insert("stderr".to_string(), Value::String(kill.stderr.clone()));
    value.insert("stdout".to_string(), Value::String(kill.stdout.clone()));
    value.insert("zmxName".to_string(), Value::String(kill.zmx_name.clone()));
    Value::Object(value)
}

pub(crate) fn decide_startup_text_disposition(
    provider_state: &str,
    startup_text: Option<&str>,
) -> String {
    if startup_text
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none()
    {
        return "none".to_string();
    }
    if provider_state == "exists" {
        "discardExistingProvider".to_string()
    } else if provider_state == "unknown" {
        "discardUnknownProvider".to_string()
    } else {
        "queueAfterTerminalReady".to_string()
    }
}

pub(crate) fn maybe_insert_startup_text(
    output: &mut Map<String, Value>,
    disposition: &str,
    startup_text: Option<&str>,
) {
    if disposition == "queueAfterTerminalReady" {
        if let Some(startup_text) = startup_text.filter(|value| !value.trim().is_empty()) {
            output.insert(
                "startupText".to_string(),
                Value::String(startup_text.to_string()),
            );
        }
    }
}

pub(crate) fn normalize_optional_startup_text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

fn get_agent_launch_startup_text_for_session(session: &Value) -> Option<String> {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("agentLaunchPlan"))
        .and_then(Value::as_object)
        .and_then(|plan| plan.get("startupText"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}

pub(crate) fn get_queued_agent_launch_startup_text_for_session(session: &Value) -> Option<String> {
    if !has_queued_agent_launch_startup_text(session) {
        return None;
    }
    get_agent_launch_startup_text_for_session(session)
}

pub(crate) fn get_persisted_provider_startup_text_for_session(
    project: &Value,
    session: &Value,
    agent_settings: &Map<String, Value>,
) -> Option<String> {
    get_queued_agent_launch_startup_text_for_session(session)
        .or_else(|| get_provider_restart_startup_text_for_session(project, session, agent_settings))
}

/*
CDXC:DraftSessions 2026-08-28:
The last step of startup-text precedence — what to run when the queued
fresh-launch text has already been consumed and the provider has to come back.
For an ordinary session that is the daemon-owned resume plan: reopen the agent
on the conversation it was in.

A DRAFT has no conversation to reopen. It publishes an agent session id at
startup but writes no transcript until the first prompt, so `build_agent_resume_plan`
would either resume a conversation that was never recorded or, with no identity
at all, degrade to a bare shell — and the user would come back to a terminal with
no agent in it. Relaunch it from its stored launch plan instead, which is exactly
the command it was created with, so waking or reopening a draft is byte-identical
to opening it the first time.
*/
pub(crate) fn get_provider_restart_startup_text_for_session(
    project: &Value,
    session: &Value,
    agent_settings: &Map<String, Value>,
) -> Option<String> {
    if crate::agents::session_is_draft(session) {
        return get_agent_launch_startup_text_for_session(session);
    }
    get_agent_startup_text_for_session(project, session, agent_settings)
}

pub(crate) fn has_queued_agent_launch_startup_text(session: &Value) -> bool {
    session
        .get("launchSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get("runtimeRelevant"))
        .and_then(Value::as_object)
        .and_then(|runtime| runtime.get("queueProviderStartupText"))
        .and_then(Value::as_bool)
        == Some(true)
}

pub(crate) fn launch_settings_with_consumed_agent_launch_startup_text(
    session: &Value,
) -> Option<Map<String, Value>> {
    if !has_queued_agent_launch_startup_text(session) {
        return None;
    }
    let mut launch_settings = session
        .get("launchSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut runtime_relevant = launch_settings
        .get("runtimeRelevant")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    runtime_relevant.insert("queueProviderStartupText".to_string(), Value::Bool(false));
    launch_settings.insert(
        "runtimeRelevant".to_string(),
        Value::Object(runtime_relevant),
    );
    Some(launch_settings)
}

pub(crate) fn consume_queued_agent_launch_startup_text(
    repository: &DomainRepository<'_>,
    session: &Value,
) -> ZmxEndpointResult<Value> {
    let Some(launch_settings) = launch_settings_with_consumed_agent_launch_startup_text(session)
    else {
        return Ok(session.clone());
    };
    let mut update = Map::new();
    update.insert("projectId".to_string(), value_field(session, "projectId")?);
    update.insert("sessionId".to_string(), value_field(session, "sessionId")?);
    update.insert("launchSettings".to_string(), Value::Object(launch_settings));
    repository
        .update_session_for_lifecycle(&update)
        .map_err(ZmxEndpointError::Domain)
}

pub(crate) fn read_interaction_text(
    value: Option<&Value>,
    command_name: &str,
) -> Result<String, DomainStateError> {
    let Some(text) = value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    else {
        return Err(DomainStateError::bad_request(format!(
            "{command_name} requires non-empty text."
        )));
    };
    if text.len() > GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES {
        return Err(DomainStateError::bad_request(format!(
            "{command_name} text exceeds the {GXSERVER_ZMX_SEND_TEXT_LIMIT_BYTES}-byte zmx send limit."
        )));
    }
    Ok(text.to_string())
}

pub(crate) fn zmx_probe_exit_error_message(result: &ZmxCommandResult) -> String {
    if !result.stderr.trim().is_empty() {
        return result.stderr.trim().to_string();
    }
    if !result.stdout.trim().is_empty() {
        return result.stdout.trim().to_string();
    }
    format!("zmx probe command exited {}", result.exit_code)
}

pub(crate) fn session_target_from_lifecycle_result(result: &Value) -> Option<(String, String)> {
    let session = result.get("session").or_else(|| {
        result
            .get("attach")
            .and_then(|attach| attach.get("session"))
    })?;
    Some((
        string_field(session, "projectId")?,
        string_field(session, "sessionId")?,
    ))
}

pub(crate) fn value_field(value: &Value, key: &str) -> Result<Value, DomainStateError> {
    value.get(key).cloned().ok_or_else(|| {
        DomainStateError::corrupt_state(format!("{key} missing from gxserver response state."))
    })
}

pub(crate) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn cwd_exists(cwd: &str) -> bool {
    let trimmed = cwd.trim();
    !trimmed.is_empty() && Path::new(trimmed).is_dir()
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
