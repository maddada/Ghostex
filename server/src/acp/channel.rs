//! `gxserver`'s side of the ACP host channel over `/api/agentChatChannel`.
//!
//! Every ACP host running inside a `zmx` session registers as `agent = "acp:<family>"`
//! on the shared long-poll channel (`pi_chat_channel.rs`). Across `gxserver` reloads,
//! `acp-host` stays alive inside `zmx`, reconnects automatically, and resumes answering
//! commands without dropping the agent turn or pending approval requests.

use std::sync::OnceLock;
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::{
    domain::{DomainRepository, DomainStateError},
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    pi_chat_channel::{ChannelError, ChannelInfo},
    server::AppState,
    session_chat_send::SessionChatSendTarget,
    storage::open_gxserver_database,
};

static LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

pub(crate) fn log(level: LogLevel, event: &str, details: Value) {
    let logger = LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let entry = GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(details),
    };
    let _ = match level {
        LogLevel::Info => logger.log_routine(DiagnosticLogScenario::AgentActivity, entry),
        _ => logger.log(entry),
    };
}

/// True when this session has an active ACP host channel open with `capability`.
pub(crate) fn live(project_id: &str, session_id: &str, capability: &str) -> Option<ChannelInfo> {
    crate::pi_chat_channel::live(project_id, session_id)
        .filter(|channel| channel.agent.starts_with("acp:") && channel.supports(capability))
}

/// Run one ACP host command off the async runtime.
pub(crate) async fn request(
    project_id: &str,
    session_id: &str,
    kind: &'static str,
    params: Value,
    timeout: Duration,
) -> Result<Value, ChannelError> {
    let (project, session) = (project_id.to_string(), session_id.to_string());
    tokio::task::spawn_blocking(move || {
        crate::pi_chat_channel::request(&project, &session, kind, params, timeout)
    })
    .await
    .unwrap_or(Err(ChannelError::NoAnswer))
}

/// Blocking state probe used by `session_chat_options::detection`.
pub(crate) fn channel_state(project_id: &str, session_id: &str) -> Option<Value> {
    live(project_id, session_id, "state")?;
    crate::pi_chat_channel::request(
        project_id,
        session_id,
        "state",
        json!({}),
        Duration::from_secs(3),
    )
    .ok()
}

/// Send a user prompt (text + optional image file paths) to an ACP session.
pub(crate) async fn send(
    paths: &crate::paths::GxserverPaths,
    target: &SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
    interrupt: bool,
) -> Result<Value, DomainStateError> {
    let validated_images = super::validate_acp_image_paths(paths, image_paths)?;
    if live(&target.project_id, &target.session_id, "send").is_none() {
        return Err(DomainStateError {
            code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
            message: "The ACP agent is still starting up. Your message is queued and will send as soon as it is ready.".to_string(),
        });
    }
    let params = json!({
        "text": text,
        "images": validated_images,
        "interrupt": interrupt,
    });
    match request(
        &target.project_id,
        &target.session_id,
        "send",
        params,
        Duration::from_secs(20),
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(ChannelError::Unavailable(_)) => Err(DomainStateError {
            code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
            message: "The ACP host is reconnecting. Your message will be retried automatically."
                .to_string(),
        }),
        Err(ChannelError::Failed(message)) => Err(DomainStateError {
            code: "invalidState",
            message,
        }),
        Err(ChannelError::Unsupported(message)) => Err(DomainStateError {
            code: "invalidParams",
            message,
        }),
        Err(ChannelError::NoAnswer) => Err(DomainStateError {
            code: "deliveryUnconfirmed",
            message: "The ACP agent took the message but did not confirm delivery. Check the chat before sending again.".to_string(),
        }),
    }
}

/// Cancel an active turn on an ACP session (`session/cancel`).
pub(crate) async fn abort(project_id: &str, session_id: &str) -> Result<(), DomainStateError> {
    if live(project_id, session_id, "abort").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The ACP host is not connected.".to_string(),
        });
    }
    request(
        project_id,
        session_id,
        "abort",
        json!({}),
        Duration::from_secs(10),
    )
    .await
    .map(|_| ())
    .map_err(|error| DomainStateError {
        code: "sessionInputFailed",
        message: format!("Could not stop the ACP turn: {error}"),
    })
}

/// Answer a pending ACP `session/request_permission` request.
pub(crate) async fn answer_permission(
    project_id: &str,
    session_id: &str,
    decision: &str,
    option_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    if live(project_id, session_id, "approve").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The ACP host is not connected to receive this approval.".to_string(),
        });
    }
    request(
        project_id,
        session_id,
        "approve",
        json!({
            "decision": decision,
            "optionId": option_id,
        }),
        Duration::from_secs(15),
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: format!("Could not answer the ACP approval: {error}"),
    })
}

/// Change model/effort on a live ACP session.
pub(crate) async fn set_model(
    project_id: &str,
    session_id: &str,
    model: &str,
    effort: &str,
) -> Result<Value, DomainStateError> {
    if live(project_id, session_id, "model").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The ACP host is not connected yet.".to_string(),
        });
    }
    request(
        project_id,
        session_id,
        "model",
        json!({
            "model": model,
            "effort": effort,
        }),
        Duration::from_secs(15),
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidParams",
        message: format!("ACP model change failed: {error}"),
    })
}

/// Change permission/interaction mode on a live ACP session.
pub(crate) async fn set_mode(
    project_id: &str,
    session_id: &str,
    mode: &str,
) -> Result<Value, DomainStateError> {
    if live(project_id, session_id, "mode").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The ACP host is not connected yet.".to_string(),
        });
    }
    request(
        project_id,
        session_id,
        "mode",
        json!({ "mode": mode }),
        Duration::from_secs(15),
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidParams",
        message: format!("ACP mode change failed: {error}"),
    })
}

/// Cycle to the next available ACP mode (equivalent of Shift+Tab in a CLI session).
pub(crate) async fn cycle_mode(
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    if live(project_id, session_id, "mode").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The ACP host is not connected yet.".to_string(),
        });
    }
    let snap = request(
        project_id,
        session_id,
        "state",
        json!({}),
        Duration::from_secs(5),
    )
    .await
    .map_err(|error| DomainStateError {
        code: "dependencyUnavailable",
        message: format!("Could not read ACP modes: {error}"),
    })?;
    let current = snap.get("mode").and_then(Value::as_str).unwrap_or_default();
    let mode_ids: Vec<String> = snap
        .get("availableModes")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    item.get("id")
                        .or_else(|| item.get("modeId"))
                        .and_then(Value::as_str)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    if mode_ids.len() < 2 {
        return Err(DomainStateError {
            code: "invalidState",
            message: "This ACP session does not expose multiple modes to cycle.".to_string(),
        });
    }
    let next_index = mode_ids
        .iter()
        .position(|id| id == current)
        .map(|idx| (idx + 1) % mode_ids.len())
        .unwrap_or(0);
    set_mode(project_id, session_id, &mode_ids[next_index]).await
}

/// Probe whether the session's `zmx`/`wmx` provider is confirmed missing on disk.
/// Returns `true` only when the probe explicitly confirms the session does not exist (`exit_code == 1`).
fn zmx_session_confirmed_missing(session: &Value) -> bool {
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(session) else {
        return false;
    };
    let Ok(zmx) = crate::zmx::require_zmx() else {
        return false;
    };
    crate::zmx::run_zmx_probe_script(
        crate::zmx::build_zmx_exists_command(&zmx_name, &zmx.executable_path),
        crate::zmx::ZmxCommandOptions {
            timeout_ms: Some(1_500),
            ..crate::zmx::ZmxCommandOptions::default()
        },
    )
    .map(|result| result.exit_code == 1)
    .unwrap_or(false)
}

/// Switch a running ACP session in-place into its interactive CLI terminal inside the same `zmx` PTY.
/// Cancels any active turn if `interrupt` is true, shuts down the ACP stdio server, execs the CLI
/// with `--conversation <id>`, and marks `runtimeSettings.acpMode = false` in `state.db`.
pub(crate) async fn switch_to_cli(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    interrupt: bool,
) -> Result<Value, DomainStateError> {
    let (native_session_id, switched_live) = if live(project_id, session_id, "switch_to_cli")
        .is_some()
    {
        let reply = request(
            project_id,
            session_id,
            "switch_to_cli",
            json!({ "interrupt": interrupt }),
            Duration::from_secs(15),
        )
        .await
        .map_err(|error| DomainStateError {
            code: "sessionInputFailed",
            message: format!("Could not switch ACP session to Terminal: {error}"),
        })?;
        let native_id = reply
            .get("nativeSessionId")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        (native_id, true)
    } else {
        // Refuse an offline flip while `acp-host` may still be starting or reconnecting inside the
        // live PTY; only allow flipping `acpMode = false` when the session is already non-ACP or
        // confirmed stopped/sleeping/draft with no live zmx provider.
        let paths = state.paths.clone();
        let server_id = state.metadata.server_id.clone();
        let pid = project_id.to_string();
        let sid = session_id.to_string();
        let precheck = tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
                code: "internalError",
                message: format!("SQLite gxserver state error: {error}"),
            })?;
            let repository = DomainRepository::new(&db, server_id.as_str());
            let session = repository
                .get_session(&pid, &sid)?
                .ok_or_else(|| DomainStateError {
                    code: "notFound",
                    message: "The session no longer exists.".to_string(),
                })?;
            if !super::session_is_acp(&session) {
                let existing_native = session
                    .pointer("/runtimeSettings/agentSessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Ok((true, existing_native));
            }
            let lifecycle = crate::presentation::effective_lifecycle_state(&session);
            let inactive_state = matches!(lifecycle.as_str(), "stopped" | "sleeping")
                || crate::agents::session_is_draft(&session);
            if inactive_state && zmx_session_confirmed_missing(&session) {
                let existing_native = session
                    .pointer("/runtimeSettings/agentSessionId")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                return Ok((false, existing_native));
            }
            Err(DomainStateError {
                code: "dependencyUnavailable",
                message: "The ACP host is starting or reconnecting. Wait a moment and try switching to Terminal again.".to_string(),
            })
        })
        .await
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not inspect ACP session state: {error}"),
        })??;
        let (already_switched, existing_native) = precheck;
        if already_switched {
            return Ok(json!({
                "switched": true,
                "switchedLive": false,
                "nativeSessionId": existing_native,
            }));
        }
        (existing_native, false)
    };

    // Persist `acpMode = false` so subsequent sends, model picks, and resumes use the CLI path.
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let native_for_db = native_session_id.clone();
    let updated_session = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        let session = repository
            .get_session(&pid, &sid)?
            .ok_or_else(|| DomainStateError {
                code: "notFound",
                message: "The session no longer exists.".to_string(),
            })?;
        let mut runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        runtime_settings.insert(super::SESSION_ACP_ACTIVE_KEY.to_string(), json!(false));
        if let Some(native_id) = native_for_db.as_deref() {
            runtime_settings.insert("agentSessionId".to_string(), json!(native_id));
            if let Some(family) = runtime_settings
                .get(super::SESSION_ACP_FAMILY_KEY)
                .and_then(Value::as_str)
            {
                if let Some(transcript) =
                    super::spec::transcript_path_for_session(family, native_id)
                {
                    runtime_settings.insert(
                        "agentSessionPath".to_string(),
                        json!(transcript.to_string_lossy().to_string()),
                    );
                }
            }
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(pid));
        update.insert("sessionId".to_string(), json!(sid));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update)
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not persist ACP-to-CLI transition: {error}"),
    })??;

    // Broadcast updated session so desktop/web/phone drop the ACP flag right away.
    state.event_hub.broadcast(json!({
        "type": "sessionUpdated",
        "projectId": project_id,
        "session": updated_session,
    }));

    log(
        LogLevel::Info,
        "acp.switchedToCli",
        json!({
            "projectId": project_id,
            "sessionId": session_id,
            "nativeSessionId": native_session_id,
            "switchedLive": switched_live,
        }),
    );

    Ok(json!({
        "switched": true,
        "switchedLive": switched_live,
        "nativeSessionId": native_session_id,
    }))
}
