//! `gxserver`'s side of the ACP host channel over `/api/agentChatChannel`.
//!
//! Every ACP host running inside a `zmx` session registers as `agent = "acp:<family>"`
//! on the shared long-poll channel (`pi_chat_channel.rs`). Across `gxserver` reloads,
//! `acp-host` stays alive inside `zmx`, reconnects automatically, and resumes answering
//! commands without dropping the agent turn or pending approval requests.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;

use serde_json::{json, Map, Value};

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
            message: "The Non-CLI agent is still starting up. Your message is queued and will send as soon as it is ready.".to_string(),
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
            message: "The Non-CLI host is reconnecting. Your message will be retried automatically."
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
            message: "The Non-CLI agent took the message but did not confirm delivery. Check the chat before sending again.".to_string(),
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
    tool_use_id: Option<&str>,
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
            "toolUseId": tool_use_id,
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

/// Persist the remembered ACP approval level on the session's `runtimeSettings` in `state.db`.
pub(crate) async fn persist_approval_level(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    level: super::AcpApprovalLevel,
) -> Result<(), DomainStateError> {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let value = level.as_str().to_string();
    tokio::task::spawn_blocking(move || {
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
        runtime_settings.insert(
            super::SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(),
            json!(value),
        );
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(pid));
        update.insert("sessionId".to_string(), json!(sid));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update).map(|_| ())
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save ACP approval level: {error}"),
    })?
}

/// Read the session's current ACP approval level (live channel first, then persisted SQLite value,
/// failing closed to `Ask` when unknown).
pub(crate) async fn current_approval_level(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> super::AcpApprovalLevel {
    if live(project_id, session_id, "state").is_some() {
        if let Some(level) = request(
            project_id,
            session_id,
            "state",
            json!({}),
            Duration::from_secs(5),
        )
        .await
        .ok()
        .and_then(|snap| {
            snap.get("approvalLevel")
                .or_else(|| snap.get("mode"))
                .and_then(Value::as_str)
                .and_then(super::AcpApprovalLevel::parse)
        }) {
            return level;
        }
    }
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).ok()?;
        let repo = DomainRepository::new(&db, server_id.as_str());
        let session = repo.get_session(&pid, &sid).ok().flatten()?;
        super::approval::stored_session_approval_level(&session)
    })
    .await
    .ok()
    .flatten()
    .unwrap_or_default()
}

/// Refuse any non-owner attempt to raise an ACP session's approval level.
pub(crate) async fn check_approval_level_change_allowed(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    target: super::AcpApprovalLevel,
    is_user_device: bool,
) -> Result<(), DomainStateError> {
    if is_user_device {
        return Ok(());
    }
    let current = current_approval_level(state, project_id, session_id).await;
    if super::AcpApprovalLevel::may_transition_without_owner(current, target) {
        return Ok(());
    }
    Err(DomainStateError {
        code: "forbidden",
        message: "Only your own devices can raise a session's approval level.".to_string(),
    })
}

static APPROVAL_LOCKS: LazyLock<Mutex<HashMap<(String, String), Arc<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn approval_lock(project_id: &str, session_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = APPROVAL_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    map.entry((project_id.to_string(), session_id.to_string()))
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// Change the approval level on an ACP session, persisting it BEFORE pushing to `acp-host` (so no
/// channel-open read can see a stale level), serialized per session, and rolling back on a hard
/// failure from `acp-host`.
///
/// - When `acp-host` is starting or reconnecting, keeps the desired level persisted so channel
///   registration pushes it once and returns retryable `dependencyUnavailable`.
/// - When `acp-host` refuses the transition (e.g. leaving native `Auto` failed), rolls back SQLite
///   and returns `UNSUPPORTED_SELECTION_CODE` so the durable selection queue fails visibly once.
pub(crate) async fn set_approval_level(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    raw_mode: &str,
    is_owner_device: bool,
) -> Result<Value, DomainStateError> {
    let level = super::AcpApprovalLevel::parse(raw_mode).ok_or_else(|| DomainStateError {
        code: "invalidParams",
        message: format!("Unsupported approval level: {raw_mode}"),
    })?;
    let lock = approval_lock(project_id, session_id);
    let _guard = lock.lock().await;
    set_approval_level_locked(state, project_id, session_id, level, is_owner_device).await
}

async fn set_approval_level_locked(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    level: super::AcpApprovalLevel,
    is_owner_device: bool,
) -> Result<Value, DomainStateError> {
    check_approval_level_change_allowed(state, project_id, session_id, level, is_owner_device)
        .await?;

    let previous_level = {
        let paths = state.paths.clone();
        let server_id = state.metadata.server_id.clone();
        let pid = project_id.to_string();
        let sid = session_id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&paths).ok()?;
            let repo = DomainRepository::new(&db, server_id.as_str());
            let session = repo.get_session(&pid, &sid).ok().flatten()?;
            super::approval::stored_session_approval_level(&session)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
    };

    // Persist BEFORE pushing to `acp-host` so channel open/reconnect can never read a stale level.
    persist_approval_level(state, project_id, session_id, level).await?;

    if live(project_id, session_id, "mode").is_some() {
        match request(
            project_id,
            session_id,
            "mode",
            json!({ "mode": level.as_str() }),
            // Strictly above `ACP_RPC_TIMEOUT` (30s) in `acp/host.rs` so slow `session/set_mode`
            // answers are not prematurely dropped as `NoAnswer`.
            Duration::from_secs(35),
        )
        .await
        {
            Ok(result) => return Ok(result),
            Err(ChannelError::Unavailable(_) | ChannelError::NoAnswer) => {
                // Mark the live channel dirty so `acp-host`'s very next poll receives
                // `desiredApprovalLevel` once even if its `instance` did not change.
                crate::pi_chat_channel::mark_approval_sync_needed(project_id, session_id);
                return Err(DomainStateError {
                    code: "dependencyUnavailable",
                    message: "The ACP host is busy or reconnecting. Your approval level was saved and will apply on its next poll.".to_string(),
                });
            }
            Err(error) => {
                let _ = persist_approval_level(state, project_id, session_id, previous_level).await;
                return Err(DomainStateError {
                    code: crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE,
                    message: format!("ACP approval level change failed: {error}"),
                });
            }
        }
    }

    // No open live channel: desired level is already persisted; mark any channel entry dirty just
    // in case it just finished a poll, and only return Ok if confirmed inactive.
    crate::pi_chat_channel::mark_approval_sync_needed(project_id, session_id);
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let confirmed_inactive = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).ok()?;
        let repo = DomainRepository::new(&db, server_id.as_str());
        let session = repo.get_session(&pid, &sid).ok().flatten()?;
        let lifecycle = crate::presentation::effective_lifecycle_state(&session);
        let inactive_state = matches!(lifecycle.as_str(), "stopped" | "sleeping")
            || crate::agents::session_is_draft(&session);
        Some(inactive_state && zmx_session_confirmed_missing(&session))
    })
    .await
    .ok()
    .flatten()
    .unwrap_or(false);

    if confirmed_inactive {
        return Ok(json!({
            "mode": level.as_str(),
            "modeLabel": level.label(),
            "approvalLevel": level.as_str(),
        }));
    }

    Err(DomainStateError {
        code: "dependencyUnavailable",
        message: "The ACP host is starting or reconnecting. Your approval level was saved and will apply as soon as it reconnects.".to_string(),
    })
}

/// Cycle to the next ACP approval level (`ask` -> `auto-edits` -> `auto` -> `full-access`).
pub(crate) async fn cycle_mode(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    is_owner_device: bool,
) -> Result<Value, DomainStateError> {
    let lock = approval_lock(project_id, session_id);
    let _guard = lock.lock().await;
    let current = current_approval_level(state, project_id, session_id).await;
    let next = current.next_in_cycle();
    set_approval_level_locked(state, project_id, session_id, next, is_owner_device).await
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
            message: format!("Could not switch Non-CLI session to Agent CLI: {error}"),
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
        runtime_settings.insert(super::SESSION_NON_CLI_ACTIVE_KEY.to_string(), json!(false));
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

    // Broadcast updated session and queue state so desktop/web/phone drop the ACP flag right away.
    state.event_hub.broadcast(json!({
        "type": "sessionUpdated",
        "projectId": project_id,
        "session": updated_session,
    }));
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state, project_id, session_id,
    );

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
