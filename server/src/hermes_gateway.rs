//! Session Chat's controls for a Hermes Agent session through Hermes's own gateway API.
//!
//! CDXC:HermesGateway 2026-11-05 DECISION:
//! User (via coordinator, Q1346): Hermes's chat moves "stop, model, rename, questions and approvals, and ready/busy to the gateway API, the send keeps typing but gated by API state", so the chat stops reading Hermes's screen to know whether it can send. Since Hermes 0.22 every `hermes` terminal is a viewer of one shared gateway process that owns the conversation, and that gateway is a documented multi-viewer authority (Hermes `website/docs/developer-guide/gateway-local-sessions.md`): a second authenticated viewer attaches with `session.resume`, stops a turn with `session.interrupt`, changes the model, effort and title with `session.mutate`, answers an approval with `approval.respond` and a clarify question with a response frame to its `srq-` request, and every terminal viewer follows (its clarify panel closes, its status bar takes the new model). Ghostex attaches the way Hermes's own clients do: the `identify` and `session-ticket` verbs on the gateway's control socket (`<HERMES_HOME>/gateway.sock`, same-user only) mint a single-use ticket, which opens `<api_origin>/api/ws` under the `hermes-gateway-v1` subprotocol. `runtime.describe` must answer before anything is sent.
//! WHY: the message itself is still typed. Measured 2026-11-05 on Hermes 0.22.5: a `prompt.submit` from another viewer runs and is stored, but the default classic terminal never paints it (its idle loop only paints its own queued turns and the gateway's background cards, `hermes_cli/gateway_tools_notify.py`), while `hermes --tui` paints it like a typed message. Typing during a turn redirects it ("type to redirect this turn"), so a send waits until the gateway says the turn ended and nothing waits on the user.
//! WHY: Hermes 0.21 and older run the agent inside the terminal with no gateway; a session there, a gateway that is down, restarting or refuses the session, all answer `None`, which keeps that action on the screen path.
//! SEE-ALSO: server/src/agent_hooks/hermes_gateway.rs (routing the gateway's hooks to their terminal), server/src/session_chat_hermes.rs (the session store the transcript is read from).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde_json::{json, Value};

const PROTOCOL: &str = "hermes-gateway-v1";
const TICKET_PREFIX: &str = "hermes-gateway-ticket.";
const IO_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a gateway action did not happen.
#[derive(Debug)]
pub(crate) enum GatewayError {
    /// No gateway answers for this session (Hermes 0.21 or older, gateway down, unknown session):
    /// nothing was sent, so the screen path may do it.
    Unavailable(String),
    /// The gateway refused it with this reason code.
    Refused(String),
}

impl std::fmt::Display for GatewayError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(formatter, "Hermes gateway unavailable: {reason}"),
            Self::Refused(reason) => write!(formatter, "Hermes refused it: {reason}"),
        }
    }
}

fn unavailable(reason: impl std::fmt::Display) -> GatewayError {
    GatewayError::Unavailable(reason.to_string())
}

/// The Hermes conversation a Ghostex session shows, and the Hermes home (profile) that owns it.
fn session_target(session: &Value) -> Result<(PathBuf, String), GatewayError> {
    let id = crate::server::read_runtime_text(session, "agentSessionId")
        .filter(|id| crate::session_chat_hermes::is_safe_hermes_session_id(id))
        .ok_or_else(|| unavailable("the session has no Hermes session id yet"))?;
    let root = crate::session_chat_hermes::hermes_home();
    let home = crate::session_chat_hermes::hermes_session_store(&root, &id)
        .and_then(|store| store.parent().map(Path::to_path_buf))
        .unwrap_or(root);
    Ok((home, id))
}

struct Endpoint {
    control: PathBuf,
    api_origin: String,
    profile_id: String,
    instance_id: String,
}

/// The control sockets that may answer for `home`: its own, then the default home's, which a
/// multiplexing gateway serves every profile from. A home whose path is too long for a socket
/// names the real one in `gateway.sock.path`.
fn control_sockets(home: &Path) -> Vec<PathBuf> {
    let mut homes = vec![home.to_path_buf()];
    let root = crate::session_chat_hermes::hermes_home();
    if root != home {
        homes.push(root);
    }
    homes
        .into_iter()
        .flat_map(|home| {
            let pointer = std::fs::read_to_string(home.join("gateway.sock.path"))
                .ok()
                .map(|path| PathBuf::from(path.trim()));
            [pointer, Some(home.join("gateway.sock"))]
        })
        .flatten()
        .filter(|path| path.exists())
        .collect()
}

#[cfg(unix)]
fn control_request(socket: &Path, request: &Value) -> Result<Value, GatewayError> {
    use std::io::{BufRead, BufReader, Write};
    let mut stream = std::os::unix::net::UnixStream::connect(socket).map_err(unavailable)?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(unavailable)?;
    stream
        .write_all(format!("{request}\n").as_bytes())
        .map_err(unavailable)?;
    let mut line = String::new();
    BufReader::new(std::io::Read::take(stream, 512 * 1024))
        .read_line(&mut line)
        .map_err(unavailable)?;
    let answer: Value = serde_json::from_str(&line).map_err(unavailable)?;
    if answer["ok"] != json!(true) {
        return Err(unavailable(
            answer["error"]
                .as_str()
                .unwrap_or("the control socket refused the request"),
        ));
    }
    Ok(answer["result"].clone())
}

#[cfg(not(unix))]
fn control_request(_socket: &Path, _request: &Value) -> Result<Value, GatewayError> {
    Err(unavailable(
        "the gateway's control pipe is not wired up on Windows yet",
    ))
}

fn same_path(left: &str, right: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical(Path::new(left)) == canonical(right)
}

fn discover(home: &Path) -> Result<Endpoint, GatewayError> {
    let mut last =
        unavailable("no Hermes gateway is running (Hermes 0.21 or older, or none started yet)");
    for control in control_sockets(home) {
        let identity = match control_request(
            &control,
            &json!({ "protocol": 1, "id": 1, "verb": "identify" }),
        ) {
            Ok(identity) => identity,
            Err(error) => {
                last = error;
                continue;
            }
        };
        if identity["runtime_protocol"] != json!(1) || identity["state"] != json!("ready") {
            last = unavailable(format!(
                "the gateway is {}",
                identity["state"].as_str().unwrap_or("not ready")
            ));
            continue;
        }
        let capable = identity["capabilities"]
            .as_array()
            .is_some_and(|capabilities| {
                capabilities
                    .iter()
                    .any(|value| value == "session-authority-v1")
            });
        let profile_id = identity["served_profiles"].as_array().and_then(|served| {
            served
                .iter()
                .find(|profile| {
                    profile["home"]
                        .as_str()
                        .is_some_and(|served| same_path(served, home))
                })
                .and_then(|profile| profile["profile_id"].as_str())
                .map(str::to_string)
        });
        let api_origin = identity["api_origin"].as_str().unwrap_or_default();
        let loopback = ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"]
            .iter()
            .any(|prefix| api_origin.starts_with(prefix));
        match (
            capable,
            profile_id,
            identity["instance_id"].as_str(),
            loopback,
        ) {
            (true, Some(profile_id), Some(instance_id), true) => {
                return Ok(Endpoint {
                    control,
                    api_origin: api_origin.to_string(),
                    profile_id,
                    instance_id: instance_id.to_string(),
                })
            }
            _ => last = unavailable("this gateway does not serve the session's Hermes profile"),
        }
    }
    Err(last)
}

type Socket = tungstenite::WebSocket<std::net::TcpStream>;

/// One attached viewer: a websocket to the gateway with `runtime.describe` already answered.
pub(crate) struct Gateway {
    socket: Socket,
    next_id: u64,
}

impl Gateway {
    fn connect(home: &Path) -> Result<Self, GatewayError> {
        let endpoint = discover(home)?;
        let ticket = control_request(
            &endpoint.control,
            &json!({ "protocol": 1, "id": 1, "verb": "session-ticket", "params": {
                "profile_id": endpoint.profile_id,
                "instance_id": endpoint.instance_id,
                "purpose": "interactive",
            }}),
        )?;
        let ticket = ticket["ticket"]
            .as_str()
            .ok_or_else(|| unavailable("the gateway minted no ticket"))?;
        let address = endpoint
            .api_origin
            .trim_start_matches("http://")
            .to_string();
        let stream = std::net::TcpStream::connect(&address).map_err(unavailable)?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(unavailable)?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(unavailable)?;
        use tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://{address}/api/ws")
            .into_client_request()
            .map_err(unavailable)?;
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            format!("{PROTOCOL}, {TICKET_PREFIX}{ticket}")
                .parse()
                .map_err(unavailable)?,
        );
        let (socket, response) =
            tungstenite::client(request, stream).map_err(|error| unavailable(error.to_string()))?;
        let chosen = response
            .headers()
            .get("Sec-WebSocket-Protocol")
            .and_then(|value| value.to_str().ok());
        if chosen != Some(PROTOCOL) {
            return Err(unavailable(
                "the gateway speaks another protocol; Hermes needs a restart or an update",
            ));
        }
        let mut gateway = Self { socket, next_id: 0 };
        gateway.call("runtime.describe", json!({}))?;
        Ok(gateway)
    }

    fn send(&mut self, frame: Value) -> Result<(), GatewayError> {
        self.socket
            .send(tungstenite::Message::text(frame.to_string()))
            .map_err(unavailable)
    }

    /// One JSON-RPC call; event frames that arrive before its answer are skipped.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, GatewayError> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let message = match self.socket.read() {
                Ok(message) => message,
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(error) => return Err(unavailable(error)),
            };
            let Ok(text) = message.to_text() else {
                continue;
            };
            let Ok(frame) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            if frame["id"] != json!(id) {
                continue;
            }
            if let Some(error) = frame.get("error") {
                let reason = error["data"]["reason"]
                    .as_str()
                    .or_else(|| error["message"].as_str())
                    .unwrap_or("request_failed");
                return Err(GatewayError::Refused(reason.to_string()));
            }
            return Ok(frame["result"].clone());
        }
        Err(unavailable(format!("{method} got no answer")))
    }
}

impl Drop for Gateway {
    fn drop(&mut self) {
        let _ = self.socket.close(None);
        let _ = self.socket.flush();
    }
}

/// What the gateway says about one conversation (`session.resume`).
pub(crate) struct Snapshot {
    pub running: bool,
    revision: Value,
    generation: Value,
    /// Approvals waiting on the user (`approval.request`): `prompt_id`, `choices`, `command`, …
    pub prompts: Vec<Value>,
    /// The gateway's own requests waiting on a viewer (`clarify`, `sudo`, `secret`, …).
    pub open_requests: Vec<Value>,
    pub info: Value,
}

fn attach(session: &Value) -> Result<(Gateway, String, Snapshot), GatewayError> {
    let (home, id) = session_target(session)?;
    let mut gateway = Gateway::connect(&home)?;
    let snapshot = match gateway.call("session.resume", json!({ "session_id": id })) {
        Ok(snapshot) => snapshot,
        // An unknown or foreign session is one this gateway cannot drive: the screen still can.
        Err(GatewayError::Refused(reason)) => {
            return Err(unavailable(format!("session.resume: {reason}")))
        }
        Err(error) => return Err(error),
    };
    let list = |key: &str| snapshot[key].as_array().cloned().unwrap_or_default();
    let parsed = Snapshot {
        running: snapshot["running"] == json!(true),
        revision: snapshot["revision"].clone(),
        generation: snapshot["execution_generation"].clone(),
        prompts: list("prompts"),
        open_requests: list("open_requests"),
        info: snapshot["info"].clone(),
    };
    Ok((gateway, id, parsed))
}

/// The conversation's state. Blocking.
pub(crate) fn snapshot(session: &Value) -> Result<Snapshot, GatewayError> {
    attach(session).map(|(_, _, snapshot)| snapshot)
}

/// Stops the running turn; `Ok(false)` when none runs. Blocking.
pub(crate) fn interrupt(session: &Value) -> Result<bool, GatewayError> {
    let (mut gateway, id, snapshot) = attach(session)?;
    if !snapshot.running {
        return Ok(false);
    }
    gateway.call(
        "session.interrupt",
        json!({ "session_id": id, "execution_generation": snapshot.generation }),
    )?;
    Ok(true)
}

fn mutate(
    gateway: &mut Gateway,
    id: &str,
    snapshot: &Snapshot,
    operation: &str,
    payload: Value,
    fenced: bool,
) -> Result<Value, GatewayError> {
    let mut params = json!({
        "session_id": id,
        "operation": operation,
        "payload": payload,
        "request_id": uuid::Uuid::new_v4().simple().to_string(),
        "expected_revision": snapshot.revision,
    });
    if fenced {
        params["expected_generation"] = snapshot.generation.clone();
    }
    gateway.call("session.mutate", params)
}

/// Switches the session's model (and reasoning effort when one is named). The pick in Ghostex is
/// the user's confirmation, so a guarded model the gateway asks to confirm is confirmed. Blocking.
pub(crate) fn switch_model(
    session: &Value,
    model: &str,
    provider: Option<&str>,
    effort: &str,
) -> Result<(), GatewayError> {
    let (mut gateway, id, mut snapshot) = attach(session)?;
    let current = snapshot.info["model"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let same_provider =
        provider.is_none_or(|provider| snapshot.info["provider"].as_str() == Some(provider));
    if !(current == model && same_provider) {
        let mut payload = json!({ "model": model });
        if let Some(provider) = provider {
            payload["provider"] = json!(provider);
        }
        let mut result = mutate(&mut gateway, &id, &snapshot, "model", payload.clone(), true)?;
        if result["status"] == json!("confirmation_required") {
            payload["confirm"] = result["confirm"].clone();
            result = mutate(&mut gateway, &id, &snapshot, "model", payload, true)?;
        }
        if result["status"] == json!("confirmation_required") {
            return Err(GatewayError::Refused(
                "Hermes asked to confirm the model again".to_string(),
            ));
        }
        snapshot = snapshot_of(&mut gateway, session)?;
    }
    if !effort.is_empty() && snapshot.info["reasoning_effort"].as_str() != Some(effort) {
        mutate(
            &mut gateway,
            &id,
            &snapshot,
            "reasoning",
            json!({ "effort": effort }),
            true,
        )?;
    }
    Ok(())
}

/// Renames the conversation (Hermes's `/title`). Blocking.
pub(crate) fn rename(session: &Value, title: &str) -> Result<(), GatewayError> {
    let (mut gateway, id, snapshot) = attach(session)?;
    mutate(
        &mut gateway,
        &id,
        &snapshot,
        "rename",
        json!({ "title": title }),
        false,
    )
    .map(|_| ())
}

/// Answers the one clarify question the gateway is waiting on. `Unavailable` when it waits on
/// none, or on a batch of several (the screen keys answer those). Blocking.
pub(crate) fn answer_clarify(session: &Value, answer: &str) -> Result<(), GatewayError> {
    let (mut gateway, _, snapshot) = attach(session)?;
    let clarify: Vec<&Value> = snapshot
        .open_requests
        .iter()
        .filter(|request| request["method"] == json!("clarify"))
        .collect();
    let [request] = clarify.as_slice() else {
        return Err(unavailable(
            "Hermes is not waiting on exactly one clarify request",
        ));
    };
    let questions = request["params"]["questions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let [question] = questions.as_slice() else {
        return Err(unavailable("a clarify batch of several questions"));
    };
    let qid = question["qid"].as_str().unwrap_or("q0");
    let request_id = request["id"].as_str().unwrap_or_default();
    gateway.send(
        json!({ "jsonrpc": "2.0", "id": request_id, "result": { "answers": { qid: answer } } }),
    )?;
    // The answer is a response frame with no reply of its own; the request leaving the session's
    // open requests is the proof.
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(300));
        let still_open = snapshot_of(&mut gateway, session)?
            .open_requests
            .iter()
            .any(|open| open["id"].as_str() == Some(request_id));
        if !still_open {
            return Ok(());
        }
    }
    Err(GatewayError::Refused(
        "Hermes did not take the answer".to_string(),
    ))
}

fn snapshot_of(gateway: &mut Gateway, session: &Value) -> Result<Snapshot, GatewayError> {
    let (_, id) = session_target(session)?;
    let snapshot = gateway.call("session.resume", json!({ "session_id": id }))?;
    let list = |key: &str| snapshot[key].as_array().cloned().unwrap_or_default();
    Ok(Snapshot {
        running: snapshot["running"] == json!(true),
        revision: snapshot["revision"].clone(),
        generation: snapshot["execution_generation"].clone(),
        prompts: list("prompts"),
        open_requests: list("open_requests"),
        info: snapshot["info"].clone(),
    })
}

/// The prefix of an approval button's `approvalSend` that names a gateway choice
/// (`hermes:approval:<prompt id>:<choice>`), which the answer path sends with `approval.respond`
/// instead of writing it into the terminal.
pub(crate) const APPROVAL_SEND_PREFIX: &str = "hermes:approval:";

/// Answers the approval `prompt_id` with `choice`. Blocking.
pub(crate) fn answer_approval(
    session: &Value,
    prompt_id: &str,
    choice: &str,
) -> Result<(), GatewayError> {
    let (mut gateway, id, snapshot) = attach(session)?;
    let prompt = snapshot
        .prompts
        .iter()
        .find(|prompt| prompt["prompt_id"].as_str() == Some(prompt_id))
        .ok_or_else(|| GatewayError::Refused("that approval is no longer waiting".to_string()))?;
    let generation = prompt
        .get("execution_generation")
        .cloned()
        .unwrap_or_else(|| snapshot.generation.clone());
    gateway.call(
        "approval.respond",
        json!({ "session_id": id, "execution_generation": generation, "prompt_id": prompt_id, "choice": choice }),
    )?;
    Ok(())
}

/// The approval the gateway waits on, as the chat's notice buttons: `(prompt id, title, detail,
/// [(choice, label)])`. Blocking.
pub(crate) fn pending_approval(
    session: &Value,
) -> Option<(String, String, String, Vec<(String, String)>)> {
    let snapshot = cached_snapshot_value(session)?;
    let prompt = snapshot["prompts"].as_array()?.first()?.clone();
    let prompt_id = prompt["prompt_id"].as_str()?.to_string();
    let title = prompt["description"]
        .as_str()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or("Hermes asks to run a command")
        .to_string();
    let detail = prompt["command"].as_str().unwrap_or_default().to_string();
    let label = |choice: &str| match choice {
        "once" => "Allow once".to_string(),
        "session" => "Allow for this session".to_string(),
        "always" => "Always allow".to_string(),
        "deny" => "Deny".to_string(),
        other => other.to_string(),
    };
    let choices = prompt["choices"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .map(|choice| (choice.to_string(), label(choice)))
        .collect::<Vec<_>>();
    (!choices.is_empty()).then_some((prompt_id, title, detail, choices))
}

/// Whether the gateway would take a typed message now, per session, refreshed at most every
/// second: `Some(true)` idle with nothing waiting on the user, `Some(false)` busy or waiting, `None`
/// no gateway (the screen decides).
struct Cached {
    at: Instant,
    snapshot: Option<Value>,
}

static CACHE: Mutex<Option<HashMap<String, Cached>>> = Mutex::new(None);
const CACHE_FOR: Duration = Duration::from_millis(1000);

fn cache_key(session: &Value) -> Option<String> {
    crate::server::read_runtime_text(session, "agentSessionId")
}

/// The last snapshot, refreshed when older than a second. The JSON form keeps the cache cheap to
/// clone. Blocking (one connection when stale).
fn cached_snapshot_value(session: &Value) -> Option<Value> {
    let key = cache_key(session)?;
    if let Some(hit) = CACHE.lock().ok().and_then(|cache| {
        cache
            .as_ref()?
            .get(&key)
            .filter(|hit| hit.at.elapsed() < CACHE_FOR)
            .map(|hit| hit.snapshot.clone())
    }) {
        return hit;
    }
    let fresh = snapshot(session).ok().map(|snapshot| {
        json!({
            "running": snapshot.running,
            "prompts": snapshot.prompts,
            "openRequests": snapshot.open_requests,
            "info": snapshot.info,
        })
    });
    if let Ok(mut cache) = CACHE.lock() {
        let cache = cache.get_or_insert_with(HashMap::new);
        cache.retain(|_, hit| hit.at.elapsed() < Duration::from_secs(300));
        cache.insert(
            key,
            Cached {
                at: Instant::now(),
                snapshot: fresh.clone(),
            },
        );
    }
    fresh
}

/// Forgets the cached state after an action changed it.
pub(crate) fn invalidate(session: &Value) {
    if let (Some(key), Ok(mut cache)) = (cache_key(session), CACHE.lock()) {
        if let Some(cache) = cache.as_mut() {
            cache.remove(&key);
        }
    }
}

/// Whether a typed message would start a turn now, per the gateway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TypedMessageGate {
    /// Idle, nothing waiting on the user: the typed message starts a turn.
    Ready,
    /// A turn runs: typing now would redirect it ("type to redirect this turn").
    Busy,
    /// An approval or a question waits: typing now would answer it.
    WaitingOnUser,
}

/// The gate for a typed message; `None` without a gateway, so the screen decides. Blocking (one
/// cached gateway read a second).
pub(crate) fn typed_message_gate(session: &Value) -> Option<TypedMessageGate> {
    let snapshot = cached_snapshot_value(session)?;
    let waiting = |key: &str| {
        snapshot[key]
            .as_array()
            .is_some_and(|list| !list.is_empty())
    };
    Some(if waiting("prompts") || waiting("openRequests") {
        TypedMessageGate::WaitingOnUser
    } else if snapshot["running"] == json!(true) {
        TypedMessageGate::Busy
    } else {
        TypedMessageGate::Ready
    })
}

static LOGGER: std::sync::OnceLock<crate::logging::GxserverLogger> = std::sync::OnceLock::new();

/// Every chat action that still drives a Hermes screen because no gateway took it, so the
/// remaining screen-driven Hermes sessions can be found in the log.
pub(crate) fn log_screen_fallback(project_id: &str, session_id: &str, action: &str, reason: &str) {
    let logger = LOGGER.get_or_init(|| {
        crate::logging::GxserverLogger::new(crate::paths::get_gxserver_paths(None))
    });
    let _ = logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "hermesGateway.screenFallback".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: None,
        details: Some(json!({ "projectId": project_id, "sessionId": session_id, "action": action, "reason": reason })),
    });
}

/// Turns a Hermes approval the gateway is waiting on into the chat's approval card: its command,
/// and one button per choice the gateway offers, answered with `approval.respond`
/// (`APPROVAL_SEND_PREFIX`). Covers the screen's blocking card or none at all; any other notice is
/// left as it is. Blocking (one cached gateway read a second).
pub(crate) fn enrich_notice(
    repository: &crate::domain::DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    notice: &mut Option<crate::session_chat_notice::SessionChatTerminalNotice>,
) {
    use crate::session_chat_notice::{
        SessionChatTerminalNotice, SessionChatTerminalNoticeAction,
        SessionChatTerminalNoticeSeverity, SessionChatTerminalNoticeSource,
        SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED,
    };
    if notice
        .as_ref()
        .is_some_and(|notice| notice.kind != SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED)
    {
        return;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    if crate::session_chat_follower::session_chat_agent_for_session(&session).as_deref()
        != Some("hermes")
    {
        return;
    }
    let Some((prompt_id, title, command, choices)) = pending_approval(&session) else {
        return;
    };
    let mut actions = choices
        .iter()
        .map(|(choice, label)| {
            SessionChatTerminalNoticeAction::send_keys(
                &format!("hermesApproval:{choice}"),
                label,
                &format!("{APPROVAL_SEND_PREFIX}{prompt_id}:{choice}"),
            )
        })
        .collect::<Vec<_>>();
    actions.push(SessionChatTerminalNoticeAction::switch_to_terminal(
        "Open terminal",
    ));
    let mut card = notice.take().unwrap_or_else(|| {
        SessionChatTerminalNotice::new(
            SESSION_CHAT_NOTICE_HERMES_INPUT_BLOCKED,
            SessionChatTerminalNoticeSeverity::Warning,
            SessionChatTerminalNoticeSource::Screen,
            &title,
        )
    });
    card.title = title;
    card.approval_command = (!command.is_empty()).then_some(command);
    card = card.with_actions(actions);
    *notice = Some(card);
}
