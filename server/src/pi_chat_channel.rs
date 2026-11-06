//! The chat channel Ghostex's Pi and OMP extensions open (`CHAT_CHANNEL_SOURCE` in
//! agent_hooks/plugin_sources.rs).
//!
//! CDXC:PiChatChannel 2026-11-05 WHY:
//! Session Chat drove Pi by typing `/model`, `/thinking` and the message into its TUI and reading
//! the screen to confirm, which broke whenever Pi's screen changed (#423). The extension runs
//! inside the Pi process, where Pi's own API sets the model and thinking level and sends user
//! messages, so the chat hands it those commands instead and the TUI stays a normal Pi TUI on the
//! same conversation. The extension long-polls gxserver over the same local API, bearer token and
//! session ref its hook reports use, so nothing new listens and no new secret exists. Each command
//! gets one acknowledgement carrying Pi's own state afterwards. A Pi still running an older
//! extension never polls, so `live` is false and callers keep the screen path for it.

use std::{
    collections::{HashMap, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, LazyLock, Mutex,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};
use tokio::sync::Notify;

use crate::{
    domain::DomainStateError,
    logging::{DiagnosticLogScenario, GxserverLogInput, GxserverLogger, LogLevel},
};

/// How long one poll waits for a command before answering with none.
const POLL_HOLD: Duration = Duration::from_secs(20);
/// A channel counts as open this long after its last poll ended, which covers the gap while the
/// extension runs a command or opens its next poll.
const LIVE_GRACE: Duration = Duration::from_secs(8);
/// A command nobody picked up within this is withdrawn, so the caller can still use the screen.
const PICKUP_TIMEOUT: Duration = Duration::from_secs(3);

struct Channel {
    agent: String,
    pid: Option<u64>,
    instance: Option<String>,
    version: u64,
    capabilities: Vec<String>,
    last_seen: Instant,
    polling: u32,
    queue: VecDeque<Value>,
    wake: Arc<Notify>,
}

enum Outcome {
    Done(Value),
    Refused(String),
    Unsupported(String),
}

struct Waiter {
    reply: mpsc::Sender<Outcome>,
    picked: bool,
}

type ChannelKey = (String, String);

static CHANNELS: LazyLock<Mutex<HashMap<ChannelKey, Channel>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static WAITERS: LazyLock<Mutex<HashMap<String, Waiter>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_COMMAND: AtomicU64 = AtomicU64::new(1);

/// What an open channel said about itself on its last poll.
#[derive(Clone, Debug)]
pub(crate) struct ChannelInfo {
    pub agent: String,
    pub capabilities: Vec<String>,
}

impl ChannelInfo {
    pub(crate) fn supports(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|value| value == capability)
    }
}

#[derive(Debug)]
pub(crate) enum ChannelError {
    /// No extension took the command: safe to do the same thing through the screen.
    Unavailable(&'static str),
    /// The extension cannot do this one (a TUI-only slash command, an older Pi without the API);
    /// nothing ran, so the screen path is still safe.
    Unsupported(String),
    /// The extension ran the command and Pi refused it.
    Failed(String),
    /// The extension took the command but never answered; it may have run.
    NoAnswer,
}

impl std::fmt::Display for ChannelError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(formatter, "chat channel unavailable: {reason}"),
            Self::Unsupported(message) => write!(formatter, "chat channel unsupported: {message}"),
            Self::Failed(message) => formatter.write_str(message),
            Self::NoAnswer => formatter.write_str("the agent took the command but did not answer"),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn key(project_id: &str, session_id: &str) -> ChannelKey {
    (project_id.to_string(), session_id.to_string())
}

/// The channel the session's agent has open now, if any.
pub(crate) fn live(project_id: &str, session_id: &str) -> Option<ChannelInfo> {
    let channels = lock(&CHANNELS);
    let channel = channels.get(&key(project_id, session_id))?;
    (channel.polling > 0 || channel.last_seen.elapsed() < LIVE_GRACE).then(|| ChannelInfo {
        agent: channel.agent.clone(),
        capabilities: channel.capabilities.clone(),
    })
}

/// Hands `kind` to the session's extension and waits for its acknowledgement. Blocking: call it
/// from a blocking thread.
pub(crate) fn request(
    project_id: &str,
    session_id: &str,
    kind: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, ChannelError> {
    if live(project_id, session_id).is_none() {
        return Err(ChannelError::Unavailable("no open channel"));
    }
    let id = format!("gx{}", NEXT_COMMAND.fetch_add(1, Ordering::Relaxed));
    let (reply, answer) = mpsc::channel();
    lock(&WAITERS).insert(
        id.clone(),
        Waiter {
            reply,
            picked: false,
        },
    );
    {
        let mut channels = lock(&CHANNELS);
        let Some(channel) = channels.get_mut(&key(project_id, session_id)) else {
            lock(&WAITERS).remove(&id);
            return Err(ChannelError::Unavailable("channel closed"));
        };
        channel
            .queue
            .push_back(json!({ "id": id, "kind": kind, "params": params }));
        channel.wake.notify_one();
    }
    let started = Instant::now();
    let pickup_deadline = started + PICKUP_TIMEOUT;
    let deadline = started + timeout.max(PICKUP_TIMEOUT);
    loop {
        let now = Instant::now();
        let picked = lock(&WAITERS).get(&id).is_some_and(|waiter| waiter.picked);
        let until = if picked { deadline } else { pickup_deadline };
        if now >= until {
            lock(&WAITERS).remove(&id);
            if picked {
                return Err(ChannelError::NoAnswer);
            }
            withdraw(project_id, session_id, &id);
            return Err(ChannelError::Unavailable(
                "the extension did not pick it up",
            ));
        }
        match answer.recv_timeout((until - now).min(Duration::from_millis(200))) {
            Ok(Outcome::Done(result)) => return Ok(result),
            Ok(Outcome::Refused(message)) => return Err(ChannelError::Failed(message)),
            Ok(Outcome::Unsupported(message)) => return Err(ChannelError::Unsupported(message)),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(ChannelError::NoAnswer),
        }
    }
}

fn withdraw(project_id: &str, session_id: &str, id: &str) {
    if let Some(channel) = lock(&CHANNELS).get_mut(&key(project_id, session_id)) {
        channel
            .queue
            .retain(|command| command.get("id").and_then(Value::as_str) != Some(id));
    }
}

/// Counts a poll as waiting until its request ends, including when the extension hangs up.
struct PollGuard(ChannelKey);

impl Drop for PollGuard {
    fn drop(&mut self) {
        if let Some(channel) = lock(&CHANNELS).get_mut(&self.0) {
            channel.polling = channel.polling.saturating_sub(1);
            channel.last_seen = Instant::now();
        }
    }
}

fn read_string(params: &Map<String, Value>, name: &str) -> Option<String> {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn channel_key(params: &Map<String, Value>) -> Result<ChannelKey, DomainStateError> {
    let reference = read_string(params, "sessionRef").unwrap_or_default();
    match crate::agent_hooks::probing::parse_global_session_ref(&reference) {
        (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
        _ => Err(DomainStateError::bad_request(
            "The chat channel needs the session's GHOSTEX_GLOBAL_SESSION_REF.",
        )),
    }
}

fn drain(queue: &mut VecDeque<Value>) -> Vec<Value> {
    let commands = queue.drain(..).collect::<Vec<_>>();
    let mut waiters = lock(&WAITERS);
    for command in &commands {
        if let Some(waiter) = command
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| waiters.get_mut(id))
        {
            waiter.picked = true;
        }
    }
    commands
}

async fn poll(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let channel_key = channel_key(params)?;
    let agent = read_string(params, "agent").unwrap_or_else(|| "pi".to_string());
    let pid = params.get("pid").and_then(Value::as_u64);
    let instance = read_string(params, "instance");
    let version = params.get("version").and_then(Value::as_u64).unwrap_or(0);
    let capabilities = params
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let wake = {
        let mut channels = lock(&CHANNELS);
        let channel = channels.entry(channel_key.clone()).or_insert_with(|| {
            log_channel("piChatChannel.opened", &channel_key, &agent, version, pid);
            Channel {
                agent: agent.clone(),
                pid,
                instance: instance.clone(),
                version,
                capabilities: Vec::new(),
                last_seen: Instant::now(),
                polling: 0,
                queue: VecDeque::new(),
                wake: Arc::new(Notify::new()),
            }
        });
        if channel.instance != instance {
            // The session's agent was restarted, or Pi replaced its runtime (/new, /resume, reload).
            log_channel("piChatChannel.reopened", &channel_key, &agent, version, pid);
            channel.pid = pid;
            channel.instance = instance;
        }
        channel.agent = agent;
        channel.version = version;
        channel.capabilities = capabilities;
        channel.last_seen = Instant::now();
        if !channel.queue.is_empty() {
            return Ok(json!({ "commands": drain(&mut channel.queue) }));
        }
        channel.polling += 1;
        channel.wake.clone()
    };
    let guard = PollGuard(channel_key);
    let _ = tokio::time::timeout(POLL_HOLD, wake.notified()).await;
    let commands = lock(&CHANNELS)
        .get_mut(&guard.0)
        .map(|channel| drain(&mut channel.queue))
        .unwrap_or_default();
    drop(guard);
    Ok(json!({ "commands": commands, "holdMs": POLL_HOLD.as_millis() as u64 }))
}

fn ack(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let id = read_string(params, "id")
        .ok_or_else(|| DomainStateError::bad_request("An acknowledgement names its command."))?;
    let Some(waiter) = lock(&WAITERS).remove(&id) else {
        return Ok(json!({ "accepted": false }));
    };
    let message = || read_string(params, "error").unwrap_or_else(|| "The agent refused it.".to_string());
    let outcome = if params.get("ok").and_then(Value::as_bool) == Some(true) {
        Outcome::Done(params.get("result").cloned().unwrap_or(Value::Null))
    } else if params.get("unsupported").and_then(Value::as_bool) == Some(true) {
        Outcome::Unsupported(message())
    } else {
        Outcome::Refused(message())
    };
    let _ = waiter.reply.send(outcome);
    Ok(json!({ "accepted": true }))
}

fn close(params: &Map<String, Value>) -> Result<Value, DomainStateError> {
    let channel_key = channel_key(params)?;
    let instance = read_string(params, "instance");
    let mut channels = lock(&CHANNELS);
    if channels
        .get(&channel_key)
        .is_some_and(|channel| channel.instance == instance)
    {
        if let Some(channel) = channels.remove(&channel_key) {
            log_channel(
                "piChatChannel.closed",
                &channel_key,
                &channel.agent,
                channel.version,
                channel.pid,
            );
        }
    }
    Ok(json!({ "closed": true }))
}

/// `/api/agentChatChannel`: `poll`, `ack` and `close` from the extension.
pub(crate) async fn dispatch(
    state: &crate::server::AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    match read_string(params, "action").as_deref() {
        Some("poll") => poll(params).await,
        Some("ack") => ack(params),
        Some("close") => close(params),
        Some("trust") => trust(state, params),
        _ => Err(DomainStateError::bad_request(
            "The chat channel action is poll, ack, close or trust.",
        )),
    }
}

/// Pi's `project_trust` check, asked by the extension before Pi draws its trust dialog: a folder
/// the user chose Trust and Remember for (session_chat_trust_memory.rs) is trusted at once, so the
/// dialog never appears and nothing is typed to answer it. Any other folder stays `undecided` and
/// Pi shows its own dialog, which the chat's trust card answers on screen as before.
fn trust(
    state: &crate::server::AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let (project_id, session_id) = channel_key(params)?;
    let db = crate::storage::open_gxserver_database(&state.paths).map_err(|error| {
        DomainStateError {
            code: "internalError",
            message: error.to_string(),
        }
    })?;
    let repository =
        crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
    let remembered = crate::session_chat_trust_memory::session_folders_remembered(
        &repository,
        &project_id,
        &session_id,
    ) || read_string(params, "cwd")
        .is_some_and(|cwd| crate::session_chat_trust_memory::folders_remembered(&[cwd]));
    log(
        LogLevel::Info,
        "piChatChannel.trust",
        json!({ "projectId": project_id, "sessionId": session_id, "remembered": remembered }),
    );
    Ok(json!({ "trusted": if remembered { "yes" } else { "undecided" } }))
}

static LOGGER: std::sync::OnceLock<GxserverLogger> = std::sync::OnceLock::new();

/// Opening and closing are routine (Agent Activity diagnostics); a fallback to the screen is a
/// warning, so every chat action still driving a Pi screen shows in the log.
fn log(level: LogLevel, event: &str, details: Value) {
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

fn log_channel(event: &str, channel_key: &ChannelKey, agent: &str, version: u64, pid: Option<u64>) {
    log(
        LogLevel::Info,
        event,
        json!({
            "projectId": channel_key.0,
            "sessionId": channel_key.1,
            "agent": agent,
            "extensionVersion": version,
            "pid": pid,
        }),
    );
}

/// Names a Pi session through Pi's own `setSessionName`, so its title is not typed into the TUI as
/// `/name`. False when the caller has to type it: no channel, or the extension could not.
pub(crate) async fn set_session_name(project_id: &str, session_id: &str, title: &str) -> bool {
    let (project, session, title) = (
        project_id.to_string(),
        session_id.to_string(),
        title.to_string(),
    );
    tokio::task::spawn_blocking(move || set_session_name_blocking(&project, &session, &title))
        .await
        .unwrap_or(false)
}

/// `set_session_name` from a blocking thread.
pub(crate) fn set_session_name_blocking(project_id: &str, session_id: &str, title: &str) -> bool {
    let supported = live(project_id, session_id).map(|channel| channel.supports("name"));
    if supported != Some(true) {
        let reason = if supported.is_none() {
            "no chat channel (older extension or agent not running)"
        } else {
            "extension cannot name the session"
        };
        log_screen_fallback(project_id, session_id, "rename", reason);
        return false;
    }
    let named = request(
        project_id,
        session_id,
        "name",
        json!({ "name": title }),
        Duration::from_secs(5),
    );
    match named {
        Ok(_) => true,
        // Naming twice is harmless, so any failure leaves it to the typed command.
        Err(reason) => {
            log_screen_fallback(project_id, session_id, "rename", &reason.to_string());
            false
        }
    }
}

/// Records that a chat action used the terminal because the session's agent has no channel, so
/// the remaining screen-driven Pi sessions can be found in the log.
pub(crate) fn log_screen_fallback(project_id: &str, session_id: &str, action: &str, reason: &str) {
    log(
        LogLevel::Warn,
        "piChatChannel.screenFallback",
        json!({
            "projectId": project_id,
            "sessionId": session_id,
            "action": action,
            "reason": reason,
        }),
    );
}
