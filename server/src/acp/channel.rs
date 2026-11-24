//! `gxserver`'s side of the ACP host channel over `/api/agentChatChannel`.
//!
//! Every ACP host running inside a `zmx` session registers as `agent = "acp:<family>"`
//! on the shared long-poll channel (`pi_chat_channel.rs`). Across `gxserver` reloads,
//! `acp-host` stays alive inside `zmx`, reconnects automatically, and resumes answering
//! commands without dropping the agent turn or pending approval requests.

use std::collections::{HashMap, HashSet};
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

pub(crate) const ACP_CHANNEL_SECRET_ENV: &str = "GHOSTEX_ACP_CHANNEL_SECRET";

/// Derive the per-session ACP host channel secret from gxserver's persistent auth token.
/// Exported to the `zmx` PTY shell that launches `ghostex acp-host` and removed from the child
/// ACP agent subprocess env so the agent cannot poll or ack its own `/api/agentChatChannel` using
/// its inherited `GHOSTEX_SESSION_TOKEN`. Deterministic across `gxserver` reloads.
pub(crate) fn channel_secret_for_session(
    auth_token: &str,
    project_id: &str,
    session_id: &str,
) -> Option<String> {
    let key = auth_token.trim();
    if key.is_empty() || project_id.is_empty() || session_id.is_empty() {
        return None;
    }
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"ghostex-acp-channel-v1\0");
    hasher.update(key.as_bytes());
    hasher.update(b"\0");
    hasher.update(project_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(session_id.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    Some(format!("gxacp_{hex}"))
}

/// Constant-time verification of the ACP host channel secret for `(project_id, session_id)`.
pub(crate) fn verify_channel_secret(
    auth_token: &str,
    project_id: &str,
    session_id: &str,
    provided: Option<&str>,
) -> bool {
    use subtle::ConstantTimeEq as _;
    let Some(expected) = channel_secret_for_session(auth_token, project_id, session_id) else {
        return false;
    };
    let Some(provided) = provided.map(str::trim).filter(|s| !s.is_empty()) else {
        return false;
    };
    expected.len() == provided.len() && bool::from(expected.as_bytes().ct_eq(provided.as_bytes()))
}

const ACP_SECRETS_DIR_NAME: &str = "acp-secrets";
const ACCOUNT_SWITCH_DIR_NAME: &str = "account-switch";

/// Atomically create a private (`0600` on Unix, under a `0700` directory inside `paths.auth_dir`,
/// which is already in `gxserver_private_paths()`) one-shot file holding the session's ACP host
/// channel secret, with a random UUID filename so concurrent switches never collide and no TOCTOU
/// world-readable window ever exists. `ghostex acp-host --channel-secret-file` reads and unlinks
/// it immediately on startup, and `sweep_stale_channel_secret_files` clears any crash leftovers at
/// daemon startup.
pub(crate) fn write_one_shot_channel_secret_file(
    auth_dir: &std::path::Path,
    auth_token: &str,
    project_id: &str,
    session_id: &str,
) -> Result<Option<std::path::PathBuf>, std::io::Error> {
    let Some(secret) = channel_secret_for_session(auth_token, project_id, session_id) else {
        return Ok(None);
    };
    let dir = auth_dir.join(ACP_SECRETS_DIR_NAME);
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }
    let path = dir.join(format!(
        "{project_id}-{session_id}-{}.secret",
        uuid::Uuid::new_v4()
    ));
    let mut opts = std::fs::OpenOptions::new();
    opts.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = opts.open(&path)?;
    file.write_all(secret.as_bytes())?;
    file.flush()?;
    Ok(Some(path))
}

/// Read a one-shot channel secret file and unlink it immediately before any child process spawns.
pub(crate) fn read_and_remove_channel_secret_file(path: &std::path::Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok();
    let _ = std::fs::remove_file(path);
    content
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Remove any leftover one-shot secret files or account-switch scripts after a crash or restart.
pub(crate) fn sweep_stale_channel_secret_files(paths: &crate::paths::GxserverPaths) {
    let dirs = [
        paths.auth_dir.join(ACP_SECRETS_DIR_NAME),
        paths.root_dir.join(ACP_SECRETS_DIR_NAME),
        paths.root_dir.join(ACCOUNT_SWITCH_DIR_NAME),
    ];
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

/// True when the stored session in SQLite is an ACP (Non-CLI) session; fails closed (`true`)
/// if SQLite cannot be read so an ACP session cannot bypass the channel-secret gate on DB error.
pub(crate) fn session_is_acp_in_db(state: &AppState, project_id: &str, session_id: &str) -> bool {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return true;
    };
    let repo = DomainRepository::new(&db, state.metadata.server_id.as_str());
    match repo.get_session(project_id, session_id) {
        Ok(Some(session)) => super::session_is_acp(&session),
        Ok(None) => false,
        Err(_) => true,
    }
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
            message: "The Native agent is still starting up. Your message is queued and will send as soon as it is ready.".to_string(),
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
            message: "The Native host is reconnecting. Your message will be retried automatically."
                .to_string(),
        }),
        Err(ChannelError::Failed(message)) if message == "ACP_AGENT_BUSY" => Err(DomainStateError {
            code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
            message: "The Native agent is currently working on a turn. Your message is queued and will send as soon as it finishes.".to_string(),
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
            message: "The Native agent took the message but did not confirm delivery. Check the chat before sending again.".to_string(),
        }),
    }
}

/// Steer a running Non-CLI turn (for example delivering an async question answer to Codex via
/// `turn/steer`), falling back to `send(..., interrupt: true)` if the host does not advertise
/// `"steer"`.
pub(crate) async fn steer(
    paths: &crate::paths::GxserverPaths,
    target: &SessionChatSendTarget,
    text: &str,
) -> Result<Value, DomainStateError> {
    if live(&target.project_id, &target.session_id, "steer").is_none() {
        return send(paths, target, text, &[], true).await;
    }
    match request(
        &target.project_id,
        &target.session_id,
        "steer",
        json!({ "text": text }),
        Duration::from_secs(20),
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(ChannelError::Unsupported(_)) => send(paths, target, text, &[], true).await,
        Err(ChannelError::Unavailable(_)) => Err(DomainStateError {
            code: crate::session_chat_send_wake::SESSION_CHAT_SESSION_STARTING,
            message: "The Native host is reconnecting. Your answer will be retried automatically."
                .to_string(),
        }),
        Err(ChannelError::Failed(message)) => Err(DomainStateError {
            code: "invalidState",
            message,
        }),
        Err(ChannelError::NoAnswer) => Err(DomainStateError {
            code: "deliveryUnconfirmed",
            message: "The Native agent took the answer but did not confirm delivery.".to_string(),
        }),
    }
}

/// Cancel an active turn on an ACP session (`session/cancel`).
pub(crate) async fn abort(project_id: &str, session_id: &str) -> Result<(), DomainStateError> {
    if live(project_id, session_id, "abort").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The Native host is not connected.".to_string(),
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
        message: format!("Could not stop the Native turn: {error}"),
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
            message: "The Native host is not connected to receive this approval.".to_string(),
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
        message: format!("Could not answer the Native approval: {error}"),
    })
}

/// Answer a pending interactive question prompt on an ACP/Non-CLI session.
pub(crate) async fn answer_question(
    project_id: &str,
    session_id: &str,
    selections: &[Value],
    tool_use_id: Option<&str>,
) -> Result<Value, DomainStateError> {
    if live(project_id, session_id, "answer_question").is_none() {
        return Err(DomainStateError {
            code: "dependencyUnavailable",
            message: "The Native host is not connected to receive this answer.".to_string(),
        });
    }
    request(
        project_id,
        session_id,
        "answer_question",
        json!({
            "selections": selections,
            "toolUseId": tool_use_id,
        }),
        Duration::from_secs(15),
    )
    .await
    .map_err(|error| match error {
        ChannelError::Failed(msg) if msg == "NO_PENDING_QUESTION" => DomainStateError {
            code: "noPendingQuestion",
            message: "No blocking Native question prompt is pending.".to_string(),
        },
        other => DomainStateError {
            code: "invalidState",
            message: format!("Could not answer the Native question: {other}"),
        },
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
            message: "The Native host is not connected yet.".to_string(),
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
        message: format!("Native model change failed: {error}"),
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
        message: format!("Could not save Native approval level: {error}"),
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

/// Per-session lock serializing Terminal <-> Native switches (`switch_to_cli`,
/// `execute_switch_to_acp`) with chat message sends (`session_chat_queue_runtime::send`) so a
/// switch never cuts a send mid-delivery and a send arriving during a switch sees the updated
/// transport once the cycle completes.
static SESSION_SWITCH_LOCKS: LazyLock<
    Mutex<HashMap<(String, String), Arc<tokio::sync::Mutex<()>>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Sessions currently executing a Terminal <-> Native transition (`execute_switch_to_acp` or
/// `switch_to_cli`). Read by the chat queue scheduler so it holds queued sends (such as coordinator
/// thread reports) without racing a duplicate wake against the switch cycle.
static ACTIVE_SWITCHES: LazyLock<Mutex<HashSet<(String, String)>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub(crate) fn is_switching(project_id: &str, session_id: &str) -> bool {
    ACTIVE_SWITCHES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains(&(project_id.to_string(), session_id.to_string()))
}

/// Reconcile a session's stored `runtimeSettings` (`nonCliMode`/`acpMode`/`acpFamily`) with an
/// authenticated live `acp-host` channel when they diverge (e.g. an older session that woke into
/// `acp-host` before wake persisted `nonCliMode: true`). Skips when a Terminal <-> Native switch
/// is in flight so it never races `switch_to_cli` or `execute_switch_to_acp`.
pub(crate) async fn reconcile_live_host_mode(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    channel_agent: &str,
    source: &'static str,
) -> bool {
    if is_switching(project_id, session_id) {
        return false;
    }
    let switch_lock = session_switch_lock(project_id, session_id);
    let Ok(_guard) = switch_lock.try_lock() else {
        return false;
    };
    reconcile_live_host_mode_inner(state, project_id, session_id, channel_agent, source).await
}

/// Variant of [`reconcile_live_host_mode`] for callers that already hold `session_switch_lock`.
pub(crate) async fn reconcile_live_host_mode_locked(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    channel_agent: &str,
    source: &'static str,
) -> bool {
    if is_switching(project_id, session_id) {
        return false;
    }
    reconcile_live_host_mode_inner(state, project_id, session_id, channel_agent, source).await
}

async fn reconcile_live_host_mode_inner(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    channel_agent: &str,
    source: &'static str,
) -> bool {
    let state_for_db = state.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let channel_agent = channel_agent.to_string();
    let updated = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&state_for_db.paths).ok()?;
        let repository = DomainRepository::new(&db, state_for_db.metadata.server_id.as_str());
        let session = repository.get_session(&pid, &sid).ok().flatten()?;
        if super::session_is_acp(&session) {
            return None;
        }
        let family = super::acp_family_from_channel_agent(&channel_agent)
            .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(&session))
            .filter(|fam| super::supports_acp(fam))?;
        let mut runtime_settings = session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        if !super::apply_acp_runtime_settings(&mut runtime_settings, &family, None) {
            return None;
        }
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(pid));
        update.insert("sessionId".to_string(), json!(sid));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        let updated_session = repository.update_session(&update).ok()?;
        let _ = crate::server::presentation_delta::schedule_presentation_session_delta(
            &state_for_db,
            &db,
            &repository,
            &pid,
            &sid,
        );
        Some((updated_session, family))
    })
    .await
    .ok()
    .flatten();

    let Some((updated_session, family)) = updated else {
        return false;
    };
    super::pending_switch::cancel(project_id, session_id);
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
        "acp.reconciledLiveHostMode",
        json!({
            "projectId": project_id,
            "sessionId": session_id,
            "family": family,
            "source": source,
        }),
    );
    true
}

struct ActiveSwitchGuard {
    key: (String, String),
}

impl ActiveSwitchGuard {
    fn enter(project_id: &str, session_id: &str) -> Self {
        let key = (project_id.to_string(), session_id.to_string());
        ACTIVE_SWITCHES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.clone());
        Self { key }
    }
}

impl Drop for ActiveSwitchGuard {
    fn drop(&mut self) {
        ACTIVE_SWITCHES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&self.key);
        crate::session_chat_queue_runtime::kick_session_chat_queue();
    }
}

pub(crate) fn session_switch_lock(
    project_id: &str,
    session_id: &str,
) -> Arc<tokio::sync::Mutex<()>> {
    let mut map = SESSION_SWITCH_LOCKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Drop locks nobody holds or waits on so the map stays bounded across the daemon's lifetime.
    if map.len() > 256 {
        map.retain(|_, lock| Arc::strong_count(lock) > 1);
    }
    map.entry((project_id.to_string(), session_id.to_string()))
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

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
    is_user_device: bool,
) -> Result<Value, DomainStateError> {
    let level = super::AcpApprovalLevel::parse(raw_mode).ok_or_else(|| DomainStateError {
        code: "invalidParams",
        message: format!("Unsupported approval level: {raw_mode}"),
    })?;
    let lock = approval_lock(project_id, session_id);
    let _guard = lock.lock().await;
    set_approval_level_locked(state, project_id, session_id, level, is_user_device).await
}

async fn set_approval_level_locked(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    level: super::AcpApprovalLevel,
    is_user_device: bool,
) -> Result<Value, DomainStateError> {
    check_approval_level_change_allowed(state, project_id, session_id, level, is_user_device)
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
                    message: "The Native host is busy or reconnecting. Your approval level was saved and will apply on its next poll.".to_string(),
                });
            }
            Err(error) => {
                // Only roll SQLite back on an upgrade failure. Never roll a failed downgrade back
                // up to a stronger previous level, so a subsequent wake/restart cannot resurrect
                // the stronger approval policy.
                if !super::AcpApprovalLevel::may_transition_without_owner(previous_level, level) {
                    let _ =
                        persist_approval_level(state, project_id, session_id, previous_level).await;
                }
                return Err(DomainStateError {
                    code: crate::session_chat_codex_picker::UNSUPPORTED_SELECTION_CODE,
                    message: format!("Native approval level change failed: {error}"),
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
        message: "The Native host is starting or reconnecting. Your approval level was saved and will apply as soon as it reconnects.".to_string(),
    })
}

/// Cycle to the next ACP approval level (`ask` -> `auto-edits` -> `auto` -> `full-access`).
pub(crate) async fn cycle_mode(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    is_user_device: bool,
) -> Result<Value, DomainStateError> {
    let lock = approval_lock(project_id, session_id);
    let _guard = lock.lock().await;
    let current = current_approval_level(state, project_id, session_id).await;
    let next = current.next_in_cycle();
    set_approval_level_locked(state, project_id, session_id, next, is_user_device).await
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
    super::pending_switch::cancel(project_id, session_id);
    let _switch_guard = session_switch_lock(project_id, session_id)
        .lock_owned()
        .await;
    let _active_switch = ActiveSwitchGuard::enter(project_id, session_id);
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
            message: format!("Could not switch Native session to Terminal: {error}"),
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
                message: "The Native host is starting or reconnecting. Wait a moment and try switching to Terminal again.".to_string(),
            })
        })
        .await
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not inspect Native session state: {error}"),
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
        message: format!("Could not persist Native-to-Terminal transition: {error}"),
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

/// Origin of a Terminal -> Native switch attempt so `execute_switch_to_acp` enforces the exact
/// eligibility rules under the per-session lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwitchOrigin {
    /// User explicitly clicked "Switch to Native" on this one session.
    Single,
    /// Queued switch firing once a busy session becomes safe (`pending_switch.rs`).
    Queued,
    /// First-launch bulk switch (`switch_eligible_sessions_to_acp`): live running non-draft
    /// sessions without an explicit Terminal override or coordinator-thread role.
    Bulk,
}

/// Result of checking whether a running Terminal session can safely sleep and wake in Native mode
/// right now without cutting a turn, tool call, approval wait, queued prompt, or background work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SwitchSafety {
    Safe,
    Busy(&'static str),
    CannotResume(&'static str),
}

/// Whether `session` is an AgentBox, detected external, Claude Cloud, or Teleport session that
/// cannot run inside Ghostex's local `acp-host` wrapper (`resume_plan.rs` bypasses `acp-host` for
/// these, so flipping `nonCliMode` would leave them unusable).
fn session_ineligible_reason(session: &Value) -> Option<&'static str> {
    if crate::agentbox::is_agentbox_session(session) {
        return Some("Native mode is not available for chats running in a box.");
    }
    if session
        .pointer("/runtimeSettings/externalSession")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Some("Native mode is not available for externally attached chats.");
    }
    if session
        .pointer("/runtimeSettings/claudeCloud")
        .is_some_and(Value::is_object)
        || session
            .pointer(&format!(
                "/runtimeSettings/{}",
                crate::claude_cloud::TELEPORT_RUNTIME_KEY
            ))
            .and_then(Value::as_str)
            .is_some_and(crate::claude_cloud::is_cloud_session_id)
    {
        return Some("Native mode is not available for cloud or teleported chats.");
    }
    None
}

/// Whether a session is structurally eligible to switch from Terminal to Native mode (advertised
/// via `nativeModeSwitchCapable` in `readSessionChat`).
pub(crate) fn session_is_switch_eligible(session: &Value) -> bool {
    session_ineligible_reason(session).is_none()
}

/// Resolve the Native-capable agent family for `session`, rejecting unsupported agents as well as
/// AgentBox, detected external, cloud, and teleported sessions.
pub(crate) fn session_native_switch_family(session: &Value) -> Result<String, DomainStateError> {
    if let Some(reason) = session_ineligible_reason(session) {
        return Err(DomainStateError::bad_request(reason));
    }
    let family = session
        .pointer("/runtimeSettings/acpFamily")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(session))
        .ok_or_else(|| DomainStateError {
            code: "invalidParams",
            message: "This agent does not support Native mode.".to_string(),
        })?;
    if !super::supports_acp(&family) {
        return Err(DomainStateError {
            code: "invalidParams",
            message: format!("Native mode is not supported for {family}."),
        });
    }
    Ok(family)
}

/// True when the user explicitly placed this session in Terminal mode (`switch_to_cli` writes
/// `nonCliMode: false, acpMode: false`). Bulk switch always respects this choice.
pub(crate) fn session_explicitly_terminal(session: &Value) -> bool {
    session
        .pointer(&format!(
            "/runtimeSettings/{}",
            super::SESSION_NON_CLI_ACTIVE_KEY
        ))
        .and_then(Value::as_bool)
        == Some(false)
        || session
            .pointer(&format!(
                "/runtimeSettings/{}",
                super::SESSION_ACP_ACTIVE_KEY
            ))
            .and_then(Value::as_bool)
            == Some(false)
}

/// Check whether a running Terminal session is safe to sleep and wake in Native mode right now.
///
/// Busy = working OR attention mid-turn (connector card, approval, question, or any non-turnComplete
/// attention) OR `agent_is_busy` (hooks, compacting, pending question, transcript gate) OR pending
/// chat queue rows OR live in-process background shells/monitors (`background_work_a_restart_would_end`).
/// A coordinator's active threads run in their own sessions and their reports queue, so they never
/// block switching a coordinator between turns. Also requires `can_start_on_its_conversation` so
/// cycling never discards an unwritten or uncaptured history.
pub(crate) async fn check_session_safe_to_switch_now(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    session: &Value,
) -> SwitchSafety {
    if crate::presentation::effective_lifecycle_state(session) != "running" {
        return SwitchSafety::Safe;
    }
    let now = crate::presentation::now_iso();
    let activity = crate::presentation::presentation_activity(session, &now);
    if activity == "working" {
        return SwitchSafety::Busy("the agent is currently replying");
    }
    if activity == "attention" {
        let waiting_connector = crate::connectors::waiting_connector_card(session).is_some();
        let turn_complete = crate::session_status::is_turn_complete_attention(
            session.pointer("/runtimeSettings/agentActivity"),
        );
        if waiting_connector || !turn_complete {
            return SwitchSafety::Busy("the agent is waiting for your approval or answer");
        }
    }
    if crate::session_chat_queue_runtime::agent_is_busy(session).await {
        return SwitchSafety::Busy("the agent is currently replying");
    }
    let state_for_db = state.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let (has_pending_queue, send_target) = tokio::task::spawn_blocking(move || {
        let has_pending = open_gxserver_database(&state_for_db.paths).is_ok_and(|db| {
            crate::session_chat_queue::session_has_pending_session_chat_queue(&db, &pid, &sid)
        });
        let target = if has_pending {
            None
        } else {
            let mut params = Map::new();
            params.insert("projectId".to_string(), json!(pid));
            params.insert("sessionId".to_string(), json!(sid));
            crate::session_chat_send::resolve_session_chat_send_target(
                &state_for_db,
                &params,
                "switchSessionCliToAcp",
            )
            .ok()
        };
        (has_pending, target)
    })
    .await
    .unwrap_or((true, None));
    if has_pending_queue {
        return SwitchSafety::Busy("queued messages are waiting to send");
    }
    if let Some(target) = send_target {
        if let Some(reason) =
            crate::session_chat_queue_runtime::background_work_a_restart_would_end(state, &target)
                .await
        {
            return SwitchSafety::Busy(reason);
        }
    }
    if !crate::session_chat_queue_runtime::can_start_on_its_conversation(state, session).await {
        return SwitchSafety::CannotResume(
            "Send a message first so this chat has history to continue in Native mode.",
        );
    }
    SwitchSafety::Safe
}

/// Switch a Terminal session whose agent supports Native mode into Native (`acp-host`).
///
/// CDXC:AgentProviders 2026-11-24 DECISION:
/// User (Q4771): never cycle a running session mid-reply. When the session is busy (working,
/// attention mid-turn, pending queue, or live background work), queue the switch and run it
/// automatically once safe, reporting `{ queued: true }` so clients show a cancelable pending
/// state in the pill strip.
pub(crate) async fn switch_to_acp(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    caller_is_user_device: bool,
) -> Result<Value, DomainStateError> {
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let (session, family, already_acp) = tokio::task::spawn_blocking(move || {
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
        let family = session_native_switch_family(&session)?;
        let already_acp = super::session_is_acp(&session);
        Ok::<_, DomainStateError>((session, family, already_acp))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not inspect session for Native mode: {error}"),
    })??;

    if already_acp {
        super::pending_switch::cancel(project_id, session_id);
        return Ok(json!({
            "switched": true,
            "queued": false,
            "restarted": false,
            "family": family,
        }));
    }

    if super::pending_switch::is_in_flight(project_id, session_id) {
        return Ok(json!({
            "switched": false,
            "queued": true,
            "inFlight": true,
            "restarted": false,
            "family": family,
        }));
    }

    match check_session_safe_to_switch_now(state, project_id, session_id, &session).await {
        SwitchSafety::Busy(reason) => {
            super::pending_switch::queue(project_id, session_id, caller_is_user_device);
            super::pending_switch::broadcast_state(state, project_id, session_id).await;
            log(
                LogLevel::Info,
                "acp.switchToAcpQueued",
                json!({
                    "projectId": project_id,
                    "sessionId": session_id,
                    "family": family,
                    "reason": reason,
                }),
            );
            return Ok(json!({
                "switched": false,
                "queued": true,
                "restarted": false,
                "family": family,
                "reason": reason,
            }));
        }
        SwitchSafety::CannotResume(reason) => {
            return Err(DomainStateError::bad_request(reason));
        }
        SwitchSafety::Safe => {}
    }

    super::pending_switch::cancel(project_id, session_id);
    match execute_switch_to_acp(
        state,
        project_id,
        session_id,
        caller_is_user_device,
        SwitchOrigin::Single,
    )
    .await
    {
        Ok(result) => Ok(result),
        Err(error) if error.code == "sessionBusy" => {
            // Session became busy while waiting for the per-session lock: queue safely instead of failing.
            super::pending_switch::queue(project_id, session_id, caller_is_user_device);
            super::pending_switch::broadcast_state(state, project_id, session_id).await;
            Ok(json!({
                "switched": false,
                "queued": true,
                "restarted": false,
                "family": family,
                "reason": error.message,
            }))
        }
        Err(error) => Err(error),
    }
}

/// Bulk-switch every safe, live Terminal session (Antigravity, Claude Code, Codex) across projects
/// on this computer to Native mode (`Q4773`/`Q4774`).
///
/// Only switches live running non-draft sessions without an explicit Terminal override
/// (`nonCliMode: false`) and not running as a coordinator worker thread. Re-checks busy/background
/// work/resumability under each session's lock right before sleeping, runs bounded in parallel,
/// and never blocks past the RPC soft wait window.
pub(crate) async fn switch_eligible_sessions_to_acp(
    state: &AppState,
    caller_is_user_device: bool,
) -> Result<Value, DomainStateError> {
    const MAX_CONCURRENT_SWITCHES: usize = 6;
    const SOFT_WAIT: Duration = Duration::from_secs(25);

    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let scan = tokio::task::spawn_blocking(move || {
        let db = open_gxserver_database(&paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("SQLite gxserver state error: {error}"),
        })?;
        let repository = DomainRepository::new(&db, server_id.as_str());
        let sessions = repository.list_sessions_excluding_stopped(None)?;
        let mut candidates: Vec<(String, String)> = Vec::new();
        let mut skipped_threads: usize = 0;
        let mut skipped_explicit_terminal: usize = 0;
        let mut skipped_sleeping: usize = 0;
        let mut skipped_unsupported: usize = 0;

        for session in sessions {
            let Some(project_id) = session.get("projectId").and_then(Value::as_str) else {
                continue;
            };
            let Some(session_id) = session.get("sessionId").and_then(Value::as_str) else {
                continue;
            };
            if super::session_is_acp(&session) {
                continue;
            }
            if session_native_switch_family(&session).is_err() {
                skipped_unsupported += 1;
                continue;
            }
            if session_explicitly_terminal(&session) {
                skipped_explicit_terminal += 1;
                continue;
            }
            let running = crate::presentation::effective_lifecycle_state(&session) == "running";
            let is_draft = crate::agents::session_is_draft(&session);
            if !running || is_draft {
                skipped_sleeping += 1;
                continue;
            }
            if crate::coordinators::read_thread(&db, project_id, session_id)
                .ok()
                .flatten()
                .is_some()
            {
                skipped_threads += 1;
                continue;
            }
            candidates.push((project_id.to_string(), session_id.to_string()));
        }

        Ok::<_, DomainStateError>((
            candidates,
            skipped_threads,
            skipped_explicit_terminal,
            skipped_sleeping,
            skipped_unsupported,
        ))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not scan sessions for Native mode: {error}"),
    })??;

    let (
        candidates,
        skipped_threads_initial,
        skipped_explicit_terminal_initial,
        skipped_sleeping_initial,
        skipped_unsupported_initial,
    ) = scan;
    let total_eligible = candidates.len();

    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<Value>();
    let state_bg = state.clone();
    tokio::spawn(async move {
        let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_SWITCHES));
        let mut join_set = tokio::task::JoinSet::new();
        for (project_id, session_id) in candidates {
            let sem = semaphore.clone();
            let state_task = state_bg.clone();
            join_set.spawn(async move {
                let Ok(_permit) = sem.acquire_owned().await else {
                    return ("failed", project_id, session_id, None::<String>);
                };
                if super::pending_switch::is_in_flight(&project_id, &session_id) {
                    return ("busy", project_id, session_id, None);
                }
                super::pending_switch::cancel(&project_id, &session_id);
                match execute_switch_to_acp(
                    &state_task,
                    &project_id,
                    &session_id,
                    caller_is_user_device,
                    SwitchOrigin::Bulk,
                )
                .await
                {
                    Ok(_) => ("switched", project_id, session_id, None),
                    Err(error) => {
                        let bucket = match error.code {
                            "sessionBusy" => "busy",
                            "coordinatorThread" => "thread",
                            "explicitTerminal" => "explicitTerminal",
                            "sessionNotRunning" => "sleeping",
                            "cannotResume" => "cannotResume",
                            "invalidParams" | "badRequest" => "unsupported",
                            _ => "failed",
                        };
                        (bucket, project_id, session_id, Some(error.message))
                    }
                }
            });
        }

        let mut switched: usize = 0;
        let mut skipped_busy: usize = 0;
        let mut skipped_threads: usize = skipped_threads_initial;
        let mut skipped_explicit_terminal: usize = skipped_explicit_terminal_initial;
        let mut skipped_sleeping: usize = skipped_sleeping_initial;
        let mut skipped_unsupported: usize = skipped_unsupported_initial;
        let mut skipped_cannot_resume: usize = 0;
        let mut failed: usize = 0;

        while let Some(joined) = join_set.join_next().await {
            match joined {
                Ok(("switched", _, _, _)) => switched += 1,
                Ok(("busy", _, _, _)) => skipped_busy += 1,
                Ok(("thread", _, _, _)) => skipped_threads += 1,
                Ok(("explicitTerminal", _, _, _)) => skipped_explicit_terminal += 1,
                Ok(("sleeping", _, _, _)) => skipped_sleeping += 1,
                Ok(("unsupported", _, _, _)) => skipped_unsupported += 1,
                Ok(("cannotResume", pid, sid, msg)) => {
                    skipped_cannot_resume += 1;
                    log(
                        LogLevel::Info,
                        "acp.bulkSwitchToAcpSkippedCannotResume",
                        json!({
                            "projectId": pid,
                            "sessionId": sid,
                            "message": msg,
                        }),
                    );
                }
                Ok((_, pid, sid, msg)) => {
                    failed += 1;
                    log(
                        LogLevel::Warn,
                        "acp.bulkSwitchToAcpFailed",
                        json!({
                            "projectId": pid,
                            "sessionId": sid,
                            "message": msg,
                        }),
                    );
                }
                Err(_) => failed += 1,
            }
        }

        let deferred = skipped_busy + skipped_threads + skipped_cannot_resume + failed;
        let summary = json!({
            "completed": true,
            "inProgress": false,
            "totalEligible": total_eligible,
            "switched": switched,
            "skippedBusy": skipped_busy,
            "skippedWorking": skipped_busy,
            "skippedThreads": skipped_threads,
            "skippedCannotResume": skipped_cannot_resume,
            "skippedExplicitTerminal": skipped_explicit_terminal,
            "skippedSleeping": skipped_sleeping,
            "skippedUnsupported": skipped_unsupported,
            "failed": failed,
            "deferred": deferred,
        });
        log(
            LogLevel::Info,
            "acp.switchEligibleSessionsToAcp",
            summary.clone(),
        );
        let _ = done_tx.send(summary);
    });

    match tokio::time::timeout(SOFT_WAIT, done_rx).await {
        Ok(Ok(summary)) => Ok(summary),
        _ => Ok(json!({
            "completed": false,
            "inProgress": true,
            "totalEligible": total_eligible,
            "switched": 0,
            "skippedBusy": 0,
            "skippedWorking": 0,
            "skippedThreads": skipped_threads_initial,
            "skippedCannotResume": 0,
            "skippedExplicitTerminal": skipped_explicit_terminal_initial,
            "skippedSleeping": skipped_sleeping_initial,
            "skippedUnsupported": skipped_unsupported_initial,
            "failed": 0,
            "deferred": skipped_threads_initial,
        })),
    }
}

/// Cancel a queued Terminal-to-Native switch before the turn finishes (`Q4771`).
pub(crate) async fn cancel_switch_to_acp(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let cancelled = super::pending_switch::cancel(project_id, session_id);
    super::pending_switch::broadcast_state(state, project_id, session_id).await;
    log(
        LogLevel::Info,
        "acp.switchToAcpCancelled",
        json!({
            "projectId": project_id,
            "sessionId": session_id,
            "cancelled": cancelled,
        }),
    );
    Ok(json!({
        "cancelled": cancelled,
        "queued": false,
    }))
}

#[derive(Clone, Debug)]
struct PriorSwitchFields {
    non_cli: Option<Value>,
    acp_mode: Option<Value>,
    acp_family: Option<Value>,
    approval_level: Option<Value>,
}

/// Execute the Terminal-to-Native switch under the per-session switch lock: re-checks eligibility,
/// busy state, background work, and conversation resumability right before persisting `nonCliMode`
/// and cycling the provider. Rolls back exact prior fields, attempts a best-effort Terminal
/// re-wake if `/api/wakeSession` failed after `/api/sleepSession`, and always broadcasts
/// `sessionUpdated` so no client is left with stale state.
pub(crate) async fn execute_switch_to_acp(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    caller_is_user_device: bool,
    origin: SwitchOrigin,
) -> Result<Value, DomainStateError> {
    let lock = session_switch_lock(project_id, session_id);
    // Bulk and queued switches must never stall behind a long-running Terminal send (e.g.
    // WaitOutBusyAgent or account switch): fail fast with `sessionBusy` so bulk moves on to
    // unrelated idle sessions and queued switches re-queue cleanly.
    let _switch_guard = match origin {
        SwitchOrigin::Bulk | SwitchOrigin::Queued => {
            lock.try_lock_owned().map_err(|_| DomainStateError {
                code: "sessionBusy",
                message: "Cannot switch right now: a message or switch is in progress.".to_string(),
            })?
        }
        SwitchOrigin::Single => lock.lock_owned().await,
    };
    let _active_switch = ActiveSwitchGuard::enter(project_id, session_id);

    // 1. Read fresh session state under the lock and validate structural eligibility.
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let (session, family, already_acp, is_thread) = tokio::task::spawn_blocking(move || {
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
        let family = session_native_switch_family(&session)?;
        let already_acp = super::session_is_acp(&session);
        let is_thread = crate::coordinators::read_thread(&db, &pid, &sid)
            .ok()
            .flatten()
            .is_some();
        Ok::<_, DomainStateError>((session, family, already_acp, is_thread))
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not read session for Native mode: {error}"),
    })??;

    if already_acp {
        return Ok(json!({
            "switched": true,
            "queued": false,
            "restarted": false,
            "family": family,
        }));
    }

    let lifecycle = crate::presentation::effective_lifecycle_state(&session);
    let was_running = lifecycle == "running";
    let is_draft = crate::agents::session_is_draft(&session);

    match origin {
        SwitchOrigin::Bulk => {
            if !was_running || is_draft {
                return Err(DomainStateError {
                    code: "sessionNotRunning",
                    message: "Only live running chats are switched in bulk.".to_string(),
                });
            }
            if session_explicitly_terminal(&session) {
                return Err(DomainStateError {
                    code: "explicitTerminal",
                    message: "Session was explicitly set to Terminal mode.".to_string(),
                });
            }
            if is_thread {
                return Err(DomainStateError {
                    code: "coordinatorThread",
                    message: "Coordinator worker threads switch when reloaded or created."
                        .to_string(),
                });
            }
        }
        SwitchOrigin::Queued => {
            if !was_running || is_draft {
                return Err(DomainStateError {
                    code: "sessionNotRunning",
                    message: "Session is no longer running.".to_string(),
                });
            }
        }
        SwitchOrigin::Single => {}
    }

    // 2. Final safety re-check right before writing `nonCliMode` and sleeping the provider.
    if was_running {
        match check_session_safe_to_switch_now(state, project_id, session_id, &session).await {
            SwitchSafety::Safe => {}
            SwitchSafety::Busy(reason) => {
                return Err(DomainStateError {
                    code: "sessionBusy",
                    message: format!("Cannot switch right now: {reason}."),
                });
            }
            SwitchSafety::CannotResume(reason) => {
                return Err(DomainStateError {
                    code: "cannotResume",
                    message: reason.to_string(),
                });
            }
        }
    }

    // 3. Persist Native runtimeSettings while capturing exact prior values for clean rollback.
    let paths = state.paths.clone();
    let server_id = state.metadata.server_id.clone();
    let pid = project_id.to_string();
    let sid = session_id.to_string();
    let family_for_db = family.clone();
    let prior = tokio::task::spawn_blocking(move || {
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
        let prior = PriorSwitchFields {
            non_cli: runtime_settings
                .get(super::SESSION_NON_CLI_ACTIVE_KEY)
                .cloned(),
            acp_mode: runtime_settings.get(super::SESSION_ACP_ACTIVE_KEY).cloned(),
            acp_family: runtime_settings.get(super::SESSION_ACP_FAMILY_KEY).cloned(),
            approval_level: runtime_settings
                .get(super::SESSION_ACP_APPROVAL_LEVEL_KEY)
                .cloned(),
        };
        let force_ask = crate::coordinators::is_read_only_thread(Some(&runtime_settings))
            || crate::side_chats::parent_of(&session).is_some()
            || crate::agents::is_gbot_api_chat(Some(&runtime_settings));
        let has_stored_level = runtime_settings
            .get(super::SESSION_ACP_APPROVAL_LEVEL_KEY)
            .and_then(Value::as_str)
            .and_then(super::AcpApprovalLevel::parse)
            .is_some();
        if force_ask || (!caller_is_user_device && !has_stored_level) {
            runtime_settings.insert(
                super::SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(),
                json!(super::AcpApprovalLevel::Ask.as_str()),
            );
        }
        runtime_settings.insert(super::SESSION_NON_CLI_ACTIVE_KEY.to_string(), json!(true));
        runtime_settings.insert(super::SESSION_ACP_ACTIVE_KEY.to_string(), json!(true));
        runtime_settings.insert(
            super::SESSION_ACP_FAMILY_KEY.to_string(),
            json!(family_for_db),
        );
        let mut update = Map::new();
        update.insert("projectId".to_string(), json!(pid));
        update.insert("sessionId".to_string(), json!(sid));
        update.insert(
            "runtimeSettings".to_string(),
            Value::Object(runtime_settings),
        );
        repository.update_session(&update)?;
        Ok::<_, DomainStateError>(prior)
    })
    .await
    .map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not update session for Native mode: {error}"),
    })??;

    // 4. Cycle running provider. If wake fails after sleep, restore exact prior fields, attempt
    //    a best-effort Terminal re-wake so the session is not left dead, and broadcast state.
    if was_running {
        if let Err(failure) = crate::session_chat_queue_runtime::cycle_session_explained(
            state, project_id, session_id,
        )
        .await
        {
            let wake_failed = failure.step == "the session could not be started again";
            let prior_after_wake = prior.clone();
            let paths = state.paths.clone();
            let server_id = state.metadata.server_id.clone();
            let pid = project_id.to_string();
            let sid = session_id.to_string();
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(db) = open_gxserver_database(&paths) {
                    let repo = DomainRepository::new(&db, server_id.as_str());
                    if let Ok(Some(session)) = repo.get_session(&pid, &sid) {
                        let mut rs = session
                            .get("runtimeSettings")
                            .and_then(Value::as_object)
                            .cloned()
                            .unwrap_or_default();
                        let restore =
                            |map: &mut Map<String, Value>, key: &str, prev: Option<Value>| {
                                if let Some(value) = prev {
                                    map.insert(key.to_string(), value);
                                } else {
                                    map.remove(key);
                                }
                            };
                        // If we are about to re-wake as Terminal while global `agentSessionMode`
                        // may already be `"non-cli"`, pin `nonCliMode = false` for the rollback
                        // wake, then restore the exact prior fields once wake returns so the
                        // session is not permanently pinned as explicit Terminal.
                        if wake_failed {
                            rs.insert(super::SESSION_NON_CLI_ACTIVE_KEY.to_string(), json!(false));
                            rs.insert(super::SESSION_ACP_ACTIVE_KEY.to_string(), json!(false));
                        } else {
                            restore(&mut rs, super::SESSION_NON_CLI_ACTIVE_KEY, prior.non_cli);
                            restore(&mut rs, super::SESSION_ACP_ACTIVE_KEY, prior.acp_mode);
                        }
                        restore(&mut rs, super::SESSION_ACP_FAMILY_KEY, prior.acp_family);
                        restore(
                            &mut rs,
                            super::SESSION_ACP_APPROVAL_LEVEL_KEY,
                            prior.approval_level,
                        );
                        let mut update = Map::new();
                        update.insert("projectId".to_string(), json!(pid));
                        update.insert("sessionId".to_string(), json!(sid));
                        update.insert("runtimeSettings".to_string(), Value::Object(rs));
                        let _ = repo.update_session(&update);
                    }
                }
            })
            .await;

            if wake_failed {
                let rewoke = crate::session_chat_queue_runtime::wake_session_best_effort(
                    state, project_id, session_id,
                )
                .await;
                let paths = state.paths.clone();
                let server_id = state.metadata.server_id.clone();
                let pid = project_id.to_string();
                let sid = session_id.to_string();
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(db) = open_gxserver_database(&paths) {
                        let repo = DomainRepository::new(&db, server_id.as_str());
                        if let Ok(Some(session)) = repo.get_session(&pid, &sid) {
                            let mut rs = session
                                .get("runtimeSettings")
                                .and_then(Value::as_object)
                                .cloned()
                                .unwrap_or_default();
                            let restore =
                                |map: &mut Map<String, Value>, key: &str, prev: Option<Value>| {
                                    if let Some(value) = prev {
                                        map.insert(key.to_string(), value);
                                    } else {
                                        map.remove(key);
                                    }
                                };
                            restore(
                                &mut rs,
                                super::SESSION_NON_CLI_ACTIVE_KEY,
                                prior_after_wake.non_cli,
                            );
                            restore(
                                &mut rs,
                                super::SESSION_ACP_ACTIVE_KEY,
                                prior_after_wake.acp_mode,
                            );
                            let mut update = Map::new();
                            update.insert("projectId".to_string(), json!(pid));
                            update.insert("sessionId".to_string(), json!(sid));
                            update.insert("runtimeSettings".to_string(), Value::Object(rs));
                            let _ = repo.update_session(&update);
                        }
                    }
                })
                .await;
                log(
                    LogLevel::Warn,
                    "acp.switchToAcpRollbackWake",
                    json!({
                        "projectId": project_id,
                        "sessionId": session_id,
                        "rewokeInTerminal": rewoke,
                        "failure": failure.to_string(),
                    }),
                );
            }
            super::pending_switch::broadcast_state(state, project_id, session_id).await;
            return Err(DomainStateError {
                code: "sessionInputFailed",
                message: format!("Could not switch to Native mode: {failure}"),
            });
        }
    }

    super::pending_switch::broadcast_state(state, project_id, session_id).await;

    log(
        LogLevel::Info,
        "acp.switchedToAcp",
        json!({
            "projectId": project_id,
            "sessionId": session_id,
            "family": family,
            "restarted": was_running,
        }),
    );

    Ok(json!({
        "switched": true,
        "queued": false,
        "restarted": was_running,
        "family": family,
    }))
}
