//! gxserver's side of DeepSeek Harness's chat channel: the commands the chat hands Ghostex's
//! dsh-tui plugin (dsh_chat_plugin.rs) over the shared channel (pi_chat_channel.rs), and the
//! inject socket dsh-tui itself ships, which a send falls back to when no plugin answers.
//!
//! CDXC:DshChatChannel 2026-11-05 WHY:
//! A dsh-tui started before the plugin was mounted, or one the plugin cannot reach the channel
//! of, still has dsh-tui's own inject socket (`~/.dsh-tui/inject/<session id>.sock`, listed in
//! `servers.json`, dsh-tui 0.17 and later): "append" then "submit", the same path as typing and
//! pressing Enter. It never answers and appends to whatever draft the input box holds, so it is
//! used only for a plain message while the input box reads empty; anything else stays on the
//! screen path. Every fallback is logged as `dshChatChannel.screenFallback`.

use std::{path::PathBuf, sync::OnceLock, time::Duration};

use serde_json::{json, Value};

use crate::{
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
    pi_chat_channel::{ChannelError, ChannelInfo},
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

pub(crate) fn log_screen_fallback(project_id: &str, session_id: &str, action: &str, reason: &str) {
    log(
        LogLevel::Warn,
        "dshChatChannel.screenFallback",
        json!({ "projectId": project_id, "sessionId": session_id, "action": action, "reason": reason }),
    );
}

/// The plugin's channel for this session, when it is open and supports `capability`.
pub(crate) fn live(project_id: &str, session_id: &str, capability: &str) -> Option<ChannelInfo> {
    crate::pi_chat_channel::live(project_id, session_id)
        .filter(|channel| channel.agent == "dsh" && channel.supports(capability))
}

/// Runs one plugin command off the async runtime.
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

/// dsh-tui's model and effort for the chat's option detection, in the Pi channel's state shape
/// (`model.provider`, `model.id`, `thinkingLevel`). Blocking.
pub(crate) fn channel_state(project_id: &str, session_id: &str) -> Option<Value> {
    live(project_id, session_id, "state")?;
    let state = crate::pi_chat_channel::request(
        project_id,
        session_id,
        "state",
        json!({}),
        Duration::from_secs(3),
    )
    .ok()?;
    state["model"]["id"]
        .as_str()
        .is_some_and(|id| !id.is_empty())
        .then_some(state)
}

/// Stops the running turn through dsh-tui's own Ctrl+C. `None` when the screen path has to.
pub(crate) async fn abort(project_id: &str, session_id: &str) -> Option<()> {
    if live(project_id, session_id, "abort").is_none() {
        log_screen_fallback(
            project_id,
            session_id,
            "interrupt",
            "no chat channel (plugin not loaded or agent not running)",
        );
        return None;
    }
    match request(
        project_id,
        session_id,
        "abort",
        json!({}),
        Duration::from_secs(8),
    )
    .await
    {
        Ok(_) => Some(()),
        Err(reason) => {
            log_screen_fallback(project_id, session_id, "interrupt", &reason.to_string());
            None
        }
    }
}

/// How a message reached dsh-tui.
pub(crate) enum DshSend {
    /// The plugin took it through dsh-tui's channel and acknowledged it.
    Channel,
    /// Written to dsh-tui's inject socket; dsh-tui does not acknowledge it.
    InjectSocket,
}

/// Sends a plain message: the plugin, then the inject socket. `None` sends it down the screen path.
pub(crate) async fn send(
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
    image_paths: &[String],
    interrupt: bool,
) -> Option<Result<DshSend, ChannelError>> {
    let (project_id, session_id) = (target.project_id.as_str(), target.session_id.as_str());
    let reason = if live(project_id, session_id, "send").is_some() {
        let params = json!({ "text": text, "images": image_paths, "interrupt": interrupt });
        match request(
            project_id,
            session_id,
            "send",
            params,
            Duration::from_secs(20),
        )
        .await
        {
            Ok(_) => return Some(Ok(DshSend::Channel)),
            Err(reason @ (ChannelError::Unavailable(_) | ChannelError::Unsupported(_))) => {
                reason.to_string()
            }
            Err(error) => return Some(Err(error)),
        }
    } else {
        "no chat channel (plugin not loaded or agent not running)".to_string()
    };
    if !image_paths.is_empty() || interrupt || text.trim_start().starts_with('/') {
        log_screen_fallback(project_id, session_id, "send", &reason);
        return None;
    }
    match inject_send(target, text).await {
        Ok(()) => {
            log(
                LogLevel::Info,
                "dshChatChannel.injectSocketSend",
                json!({ "projectId": project_id, "sessionId": session_id, "channel": reason }),
            );
            Some(Ok(DshSend::InjectSocket))
        }
        Err(inject) => {
            log_screen_fallback(
                project_id,
                session_id,
                "send",
                &format!("{reason}; inject socket: {inject}"),
            );
            None
        }
    }
}

fn inject_dir() -> PathBuf {
    crate::resume_lookup::home_dir()
        .join(".dsh-tui")
        .join("inject")
}

#[cfg(unix)]
fn process_alive(pid: u64) -> bool {
    i32::try_from(pid).is_ok_and(|pid| unsafe { libc::kill(pid, 0) } == 0)
}

/// The live inject socket dsh-tui opened for this DSH session.
#[cfg(unix)]
fn inject_socket(agent_session_id: &str) -> Option<PathBuf> {
    let servers = std::fs::read_to_string(inject_dir().join("servers.json")).ok()?;
    let servers: Vec<Value> = serde_json::from_str(&servers).ok()?;
    servers
        .iter()
        .filter(|server| server["sessionId"].as_str() == Some(agent_session_id))
        .filter(|server| server["pid"].as_u64().is_some_and(process_alive))
        .max_by_key(|server| server["startedAt"].as_u64().unwrap_or(0))
        .and_then(|server| server["socketPath"].as_str().map(PathBuf::from))
}

#[cfg(unix)]
async fn inject_send(
    target: &crate::session_chat_send::SessionChatSendTarget,
    text: &str,
) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    let agent_session_id = crate::server::read_runtime_text(&target.session, "agentSessionId")
        .filter(|id| crate::session_chat_dsh::is_safe_dsh_session_id(id))
        .ok_or("the session has no DeepSeek Harness session id yet")?;
    let socket =
        inject_socket(&agent_session_id).ok_or("no live inject socket for this session")?;
    // "append" lands at the caret, so a draft in the input box would join the message.
    let zmx_name = target.zmx_name.clone();
    let screen = tokio::task::spawn_blocking(move || {
        crate::zmx::screen_capture::read_zmx_session_screen_capture(&zmx_name)
    })
    .await
    .map_err(|error| error.to_string())??;
    let input = crate::session_chat_composer::session_chat_composer_input("dsh", &screen.text)
        .ok_or("the input box is not on screen")?;
    if !input.text.trim().is_empty() || screen.truncated {
        return Err("the input box holds a draft".to_string());
    }
    let mut stream = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::UnixStream::connect(&socket),
    )
    .await
    .map_err(|_| "the inject socket did not accept".to_string())?
    .map_err(|error| error.to_string())?;
    let lines = format!(
        "{}\n{}\n",
        json!({ "type": "prompt.append", "text": text }),
        json!({ "type": "command.execute", "command": "prompt.submit" })
    );
    stream
        .write_all(lines.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stream.flush().await.map_err(|error| error.to_string())?;
    // Let dsh-tui read both lines before the socket closes.
    tokio::time::sleep(Duration::from_millis(150)).await;
    Ok(())
}

#[cfg(not(unix))]
async fn inject_send(
    _target: &crate::session_chat_send::SessionChatSendTarget,
    _text: &str,
) -> Result<(), String> {
    Err(
        "the inject socket is a named pipe on Windows, which Ghostex does not write yet"
            .to_string(),
    )
}
