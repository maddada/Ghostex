//! Which relayed calls are in flight or just finished, so a CLI that stopped waiting can ask
//! which side was slow (`/relay/status`), and a thread start that outlives the CLI's wait is
//! still linked, started and briefed once the other computer answers (`/relay/watchStart`).
//!
//! CDXC:RemoteMachines 2026-11-28 WHY:
//! GitHub #789: a relayed call the CLI gave up on after 15 s said only "did not answer", whether
//! this gxserver, the connection or the other computer was slow, and a thread start that
//! finished later was never linked to its coordinator or sent its brief.
//!
//! CDXC:Security 2026-11-28 WHY (Fable's review of the #789 fix):
//! - A call belongs to the caller that made it: only that same caller (the same session token,
//!   the token file, the same phone) may read its state or attach a watch to it, and another
//!   caller's request id reads as unknown. The id comes from the caller (`x-request-id`), so an
//!   id that is in flight, or that another caller holds, is never replaced.
//! - A watch does nothing its caller could not do itself: what it sends later (start the agent,
//!   set the model, queue the brief) goes through `relay` again as that caller, so the Agent
//!   access switch, the thread rules and the signing of an agent's message apply as they do to a
//!   call the CLI makes (connected_computers/relay.rs, session_chat_send/sender_mark.rs). A
//!   session may name only itself as the coordinator.
//! - Memory is bounded: at most [`MAX_TRACKED_CALLS`] calls, [`MAX_PENDING_WATCHES`] waiting
//!   watches of at most [`WATCH_BODY_LIMIT_BYTES`] each, a kept error of [`MAX_ERROR_CHARS`], and
//!   every entry is dropped after [`COMPLETED_CALL_TTL`] or [`IN_FLIGHT_CALL_TTL`].

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    body::{to_bytes, Body},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
};
use serde_json::{json, Value};

use crate::{
    constants::{GXSERVER_PROTOCOL_HEADER, GXSERVER_PROTOCOL_VERSION},
    domain::DomainRepository,
    protocol::rpc_success,
    security::Principal,
    server::{routed_json, schedule_presentation_session_delta, AppState, RoutedResponse},
    storage::open_gxserver_database,
};

use super::{registry, ConnectedComputer, RELAY_ROUTE_PREFIX};

const MAX_TRACKED_CALLS: usize = 256;
const MAX_PENDING_WATCHES: usize = 32;
const MAX_REQUEST_ID_CHARS: usize = 128;
const MAX_ERROR_CHARS: usize = 2_000;
const MAX_TITLE_CHARS: usize = 200;
const COMPLETED_CALL_TTL: Duration = Duration::from_secs(15 * 60);
/// Longer than the relay's own limit for a call (15 minutes, relay.rs `forward`).
const IN_FLIGHT_CALL_TTL: Duration = Duration::from_secs(20 * 60);
const STATUS_PROBE_TIMEOUT: Duration = Duration::from_millis(1_500);
const WATCH_BODY_LIMIT_BYTES: usize = 1024 * 1024;
const CREATED_RESULT_LIMIT_BYTES: usize = 16 * 1024;
/// Answers larger than this are not searched for an error message.
const ERROR_SCAN_LIMIT_BYTES: usize = 64 * 1024;

/// The two calls that create a thread's session on another computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StartEndpoint {
    /// `/api/createAgentSession`: the agent still has to be started (`startSessionProvider`).
    AgentSession,
    /// `/api/createWorktreeSession`: creates the worktree and starts the agent.
    WorktreeSession,
}

impl StartEndpoint {
    pub(crate) fn of(forwarded: &str) -> Option<Self> {
        match forwarded {
            "/api/createAgentSession" => Some(Self::AgentSession),
            "/api/createWorktreeSession" => Some(Self::WorktreeSession),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct CreatedSessionInfo {
    project_id: String,
    session_id: String,
    global_ref: String,
    read_only: bool,
    /// The create call's own `result`, which the CLI goes on with when the call ended just as
    /// it stopped waiting; left out when larger than [`CREATED_RESULT_LIMIT_BYTES`].
    result: Option<Value>,
}

#[derive(Clone, Debug)]
pub(crate) struct CompletedRelayCall {
    pub(crate) status: u16,
    /// `ok`, or what went wrong (relay.rs `RelayCall::finish`).
    pub(crate) outcome: &'static str,
    pub(crate) error: Option<String>,
    pub(crate) created_session: Option<CreatedSessionInfo>,
}

#[derive(Clone, Debug)]
enum RelayCallStage {
    /// This gxserver is still checking and preparing the call.
    LocalPrep,
    /// Sent; waiting for the other computer's answer.
    Forwarding,
    Completed {
        call: CompletedRelayCall,
        finished_at: Instant,
    },
}

/// What is left to do for a thread whose create call the CLI stopped waiting for.
#[derive(Clone, Debug)]
struct PendingStartWatch {
    /// The coordinator told when the start fails, and linked to the thread when `link` is set.
    coordinator: Option<(String, String)>,
    link: bool,
    title: String,
    task: String,
    message: Option<String>,
    model_line: Option<String>,
    send_request_id: String,
    read_only: bool,
}

struct RelayCallEntry {
    server_id: String,
    computer_name: String,
    caller: Principal,
    start: Option<StartEndpoint>,
    started_at: Instant,
    stage: RelayCallStage,
    watch: Option<PendingStartWatch>,
}

fn calls() -> std::sync::MutexGuard<'static, HashMap<String, RelayCallEntry>> {
    static CALLS: OnceLock<Mutex<HashMap<String, RelayCallEntry>>> = OnceLock::new();
    CALLS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Drops entries past their time, then the oldest finished ones until there is room for one more.
fn prune(map: &mut HashMap<String, RelayCallEntry>, now: Instant) {
    map.retain(|_, entry| match &entry.stage {
        RelayCallStage::Completed { finished_at, .. } => {
            now.duration_since(*finished_at) < COMPLETED_CALL_TTL
        }
        _ => now.duration_since(entry.started_at) < IN_FLIGHT_CALL_TTL,
    });
    while map.len() >= MAX_TRACKED_CALLS {
        let oldest = map
            .iter()
            .filter_map(|(id, entry)| match &entry.stage {
                RelayCallStage::Completed { finished_at, .. } => Some((id.clone(), *finished_at)),
                _ => None,
            })
            .min_by_key(|(_, finished_at)| *finished_at)
            .map(|(id, _)| id);
        match oldest {
            Some(id) => map.remove(&id),
            None => break,
        };
    }
}

/// Starts tracking a call `caller` makes to `server_id`. `false` when it is not tracked (the
/// table is full of calls in flight, or the id is in use): the call itself goes on unchanged, and
/// nothing later may be recorded under that id for it.
pub(crate) fn track(
    request_id: &str,
    caller: &Principal,
    server_id: &str,
    computer_name: &str,
    forwarded: &str,
) -> bool {
    if request_id.len() > MAX_REQUEST_ID_CHARS {
        return false;
    }
    let now = Instant::now();
    let mut map = calls();
    prune(&mut map, now);
    let replaceable = map.get(request_id).is_none_or(|existing| {
        existing.caller == *caller && matches!(existing.stage, RelayCallStage::Completed { .. })
    });
    if !replaceable || map.len() >= MAX_TRACKED_CALLS {
        return false;
    }
    map.insert(
        request_id.to_string(),
        RelayCallEntry {
            server_id: server_id.to_string(),
            computer_name: computer_name.to_string(),
            caller: caller.clone(),
            start: StartEndpoint::of(forwarded),
            started_at: now,
            stage: RelayCallStage::LocalPrep,
            watch: None,
        },
    );
    true
}

pub(crate) fn mark_forwarding(request_id: &str) {
    if let Some(entry) = calls().get_mut(request_id) {
        entry.stage = RelayCallStage::Forwarding;
    }
}

/// Records how a tracked call ended and runs the watch attached to it, if any.
pub(crate) fn complete(state: &Arc<AppState>, request_id: &str, call: CompletedRelayCall) {
    let pending = {
        let mut map = calls();
        let Some(entry) = map.get_mut(request_id) else {
            return;
        };
        entry.stage = RelayCallStage::Completed {
            call: call.clone(),
            finished_at: Instant::now(),
        };
        entry.watch.take().map(|watch| StartWatchRun {
            state: state.clone(),
            server_id: entry.server_id.clone(),
            computer_name: entry.computer_name.clone(),
            caller: entry.caller.clone(),
            start: entry.start,
            request_id: request_id.to_string(),
            watch,
            call,
        })
    };
    if let Some(run) = pending {
        tokio::spawn(run.run());
    }
}

fn clipped(text: &str, max_chars: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    clipped.push('…');
    clipped
}

/// The session a create call's answer names, or `None` for any other call or a failed one.
pub(crate) fn created_session(
    forwarded: &str,
    server_id: &str,
    status: u16,
    body: &[u8],
) -> Option<CreatedSessionInfo> {
    let start = StartEndpoint::of(forwarded)?;
    if !(200..300).contains(&status) {
        return None;
    }
    let parsed: Value = serde_json::from_slice(body).ok()?;
    if parsed.get("ok") == Some(&Value::Bool(false)) {
        return None;
    }
    let result = parsed.get("result")?;
    let (session, read_only) = match start {
        StartEndpoint::AgentSession => {
            let session = result.get("session")?;
            let read_only = session
                .pointer("/runtimeSettings")
                .and_then(|settings| settings.get(crate::coordinators::READ_ONLY_THREAD_KEY));
            (session, read_only)
        }
        StartEndpoint::WorktreeSession => (result, result.get("readOnly")),
    };
    let text = |value: &Value, key: &str| {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let project_id = text(session, "projectId")?;
    let session_id = text(session, "sessionId")?;
    Some(CreatedSessionInfo {
        global_ref: crate::ids::create_global_session_ref(server_id, &project_id, &session_id),
        project_id,
        session_id,
        read_only: read_only.and_then(Value::as_bool) == Some(true),
        result: (body.len() <= CREATED_RESULT_LIMIT_BYTES).then(|| result.clone()),
    })
}

/// Why the other computer refused or failed a call, from its answer; `None` when it went through.
/// A large successful answer (a chat read) is not parsed for it.
pub(crate) fn remote_error_message(status: u16, body: &[u8]) -> Option<String> {
    let success = (200..300).contains(&status);
    if success && body.len() > ERROR_SCAN_LIMIT_BYTES {
        return None;
    }
    let parsed = serde_json::from_slice::<Value>(body).ok();
    let refused = parsed
        .as_ref()
        .is_some_and(|value| value.get("ok") == Some(&Value::Bool(false)));
    if success && !refused {
        return None;
    }
    Some(
        parsed
            .as_ref()
            .and_then(|value| value.get("message").or_else(|| value.get("error")))
            .and_then(Value::as_str)
            .map(|message| clipped(message, MAX_ERROR_CHARS))
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| format!("HTTP {status}")),
    )
}

fn answer(request_id: String, result: Value) -> RoutedResponse {
    routed_json(
        Some(RELAY_ROUTE_PREFIX.to_string()),
        StatusCode::OK,
        rpc_success(request_id, result),
    )
}

/// `GET /api/computers/<serverId>/relay/status?requestId=<id>`: where the caller's own call is.
pub(crate) async fn handle_status(
    method: &Method,
    caller: &Principal,
    server_id: &str,
    query: Option<&str>,
    request_id: String,
) -> RoutedResponse {
    if *method != Method::GET {
        return super::relay::refusal(
            StatusCode::METHOD_NOT_ALLOWED,
            "methodNotAllowed",
            "/relay/status requires GET.".to_string(),
            request_id,
        );
    }
    let wanted = query
        .and_then(|query| {
            url::form_urlencoded::parse(query.as_bytes())
                .find(|(key, _)| key == "requestId")
                .map(|(_, value)| value.trim().to_string())
        })
        .unwrap_or_default();
    let computer = registry::find(server_id);
    let found = calls()
        .get(&wanted)
        .filter(|entry| entry.caller == *caller && entry.server_id == server_id)
        .map(|entry| {
            (
                entry.computer_name.clone(),
                entry.started_at.elapsed().as_millis() as u64,
                entry.stage.clone(),
            )
        });
    let Some((computer_name, elapsed_ms, stage)) = found else {
        // Not this caller's, never arrived, or no longer kept: all read the same.
        let name = computer.map_or_else(|| server_id.to_string(), |computer| computer.name);
        return answer(
            request_id,
            json!({ "state": "unknown", "serverId": server_id, "computerName": name }),
        );
    };
    let mut result = json!({
        "serverId": server_id,
        "computerName": computer_name,
        "elapsedMs": elapsed_ms,
    });
    match stage {
        RelayCallStage::LocalPrep => result["state"] = json!("localPrep"),
        RelayCallStage::Forwarding => {
            // The other computer answers a health check while the call is out: it has the call
            // and is slow with it. No answer: the connection, or its gxserver, is stalled.
            let reachable = match computer
                .filter(|computer| computer.agent_access && computer.waiting_message().is_none())
            {
                Some(computer) => tokio::task::spawn_blocking(move || answers_health(&computer))
                    .await
                    .unwrap_or(false),
                None => false,
            };
            result["state"] = json!(if reachable {
                "otherComputerSlow"
            } else {
                "connectionStalled"
            });
        }
        RelayCallStage::Completed { call, .. } => {
            result["state"] = json!("completed");
            result["status"] = json!(call.status);
            result["outcome"] = json!(call.outcome);
            if let Some(error) = call.error {
                result["error"] = json!(error);
            }
            if let Some(created) = call.created_session.and_then(|created| created.result) {
                result["created"] = created;
            }
        }
    }
    answer(request_id, result)
}

fn answers_health(computer: &ConnectedComputer) -> bool {
    let url = format!("{}/api/health/server", computer.base_url);
    if computer.check_listener(&url).is_err() {
        return false;
    }
    crate::http_agent::answering_agent()
        .get(&url)
        .header("authorization", format!("Bearer {}", computer.token()))
        .header(
            GXSERVER_PROTOCOL_HEADER,
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(STATUS_PROBE_TIMEOUT))
        .build()
        .call()
        .is_ok_and(|response| response.status().is_success())
}

/// `POST /api/computers/<serverId>/relay/watchStart`: the CLI stopped waiting for its own create
/// call `requestId`; when that call ends, finish the thread start for it.
pub(crate) async fn handle_watch_start(
    state: Arc<AppState>,
    method: &Method,
    caller: &Principal,
    server_id: &str,
    body: Body,
    request_id: String,
) -> RoutedResponse {
    let refuse = |status: StatusCode, code: &str, message: &str, request_id: String| {
        super::relay::refusal(status, code, message.to_string(), request_id)
    };
    if *method != Method::POST {
        return refuse(
            StatusCode::METHOD_NOT_ALLOWED,
            "methodNotAllowed",
            "/relay/watchStart requires POST.",
            request_id,
        );
    }
    let Ok(bytes) = to_bytes(body, WATCH_BODY_LIMIT_BYTES).await else {
        return refuse(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payloadTooLarge",
            "That brief is too large to hand over for a late start.",
            request_id,
        );
    };
    let Ok(parsed) = serde_json::from_slice::<Value>(&bytes) else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "watchStart requires a JSON body.",
            request_id,
        );
    };
    let params = parsed.get("params").unwrap_or(&parsed);
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    };
    let flag = |key: &str| params.get(key).and_then(Value::as_bool) == Some(true);
    let Some(watched) = text("requestId") else {
        return refuse(
            StatusCode::BAD_REQUEST,
            "badRequest",
            "watchStart requires requestId.",
            request_id,
        );
    };
    let coordinator = match (text("coordinatorProjectId"), text("coordinatorSessionId")) {
        (Some(project_id), Some(session_id)) => Some((project_id, session_id)),
        (None, None) => None,
        _ => {
            return refuse(
                StatusCode::BAD_REQUEST,
                "badRequest",
                "coordinatorProjectId and coordinatorSessionId must both be set.",
                request_id,
            );
        }
    };
    // The same rule as for a thread it links itself (server/security_gates.rs
    // `coordinator_refusal`): a session's thread is its own.
    if let (
        Principal::Session {
            project_id,
            session_id,
            ..
        },
        Some(coordinator),
    ) = (caller, &coordinator)
    {
        if (project_id, session_id) != (&coordinator.0, &coordinator.1) {
            return refuse(
                StatusCode::FORBIDDEN,
                "forbidden",
                "A session can only manage its own threads.",
                request_id,
            );
        }
    }
    let watch = PendingStartWatch {
        coordinator,
        link: flag("link"),
        title: clipped(
            &text("title").unwrap_or_default().replace(['\n', '\r'], " "),
            MAX_TITLE_CHARS,
        ),
        task: text("task").unwrap_or_default(),
        message: text("message"),
        model_line: text("modelLine"),
        send_request_id: text("sendRequestId")
            .filter(|id| id.len() <= MAX_REQUEST_ID_CHARS)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        read_only: flag("readOnly"),
    };
    enum Attach {
        Waiting,
        Finished(Box<StartWatchRun>),
        Unknown,
        Full,
    }
    let attach = {
        let mut map = calls();
        let waiting = map.values().filter(|entry| entry.watch.is_some()).count();
        match map.get_mut(&watched).filter(|entry| {
            entry.caller == *caller && entry.server_id == server_id && entry.start.is_some()
        }) {
            None => Attach::Unknown,
            Some(entry) => match &entry.stage {
                RelayCallStage::Completed { call, .. } => {
                    Attach::Finished(Box::new(StartWatchRun {
                        state,
                        server_id: entry.server_id.clone(),
                        computer_name: entry.computer_name.clone(),
                        caller: entry.caller.clone(),
                        start: entry.start,
                        request_id: watched.clone(),
                        watch,
                        call: call.clone(),
                    }))
                }
                _ if entry.watch.is_none() && waiting >= MAX_PENDING_WATCHES => Attach::Full,
                _ => {
                    entry.watch = Some(watch);
                    Attach::Waiting
                }
            },
        }
    };
    match attach {
        Attach::Unknown => refuse(
            StatusCode::NOT_FOUND,
            "notFound",
            "No thread start of yours with that request id is in flight here.",
            request_id,
        ),
        Attach::Full => refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "tooManyRequests",
            "Too many late thread starts are already waiting here.",
            request_id,
        ),
        Attach::Waiting => answer(request_id, json!({ "watching": true })),
        Attach::Finished(run) => {
            // It ended between the CLI's look and this request.
            tokio::spawn(run.run());
            answer(request_id, json!({ "watching": true }))
        }
    }
}

/// One watch whose create call has ended.
struct StartWatchRun {
    state: Arc<AppState>,
    server_id: String,
    computer_name: String,
    caller: Principal,
    start: Option<StartEndpoint>,
    request_id: String,
    watch: PendingStartWatch,
    call: CompletedRelayCall,
}

impl StartWatchRun {
    /// Does what `ghostex coordinator start-thread` and `ghostex agents create` do after the
    /// create call (ghostex_cli/coordinator/threads.rs, ghostex_cli/agents/lifecycle.rs), and
    /// tells the coordinator when a step fails.
    async fn run(self) {
        let computer = self.computer_name.clone();
        let thread = if self.watch.title.is_empty() {
            "The thread".to_string()
        } else {
            format!("Thread \"{}\"", self.watch.title)
        };
        if self.call.outcome != "ok" {
            let why = self.call.error.clone().unwrap_or_else(|| {
                format!(
                    "{computer} answered HTTP {status}",
                    status = self.call.status
                )
            });
            return self
                .tell_coordinator(format!("{thread} was not started on {computer}: {why}"))
                .await;
        }
        let Some(created) = self.call.created_session.clone() else {
            return self
                .tell_coordinator(format!(
                    "{thread} may have started on {computer}, but its answer named no session. Check `ghostex sessions --computer {computer}` before starting another."
                ))
                .await;
        };
        let reference = created.global_ref.clone();
        let session = json!({
            "globalRef": reference,
            "projectId": created.project_id,
            "sessionId": created.session_id,
        });
        if self.watch.read_only && !created.read_only {
            // As `undo_unrestricted_thread` does for a start the CLI waited for.
            let _ = self.relayed("/api/killSession", session.clone()).await;
            let _ = self.relayed("/api/removeSession", session.clone()).await;
            return self
                .tell_coordinator(format!(
                    "{thread} was not started on {computer}: its Ghostex is older than read-only threads, so its agent would have had full rights. Update Ghostex there, or start the thread without --read-only."
                ))
                .await;
        }
        if self.watch.link {
            self.link(&created).await;
        }
        if self.start == Some(StartEndpoint::AgentSession) {
            if let Err(error) = self
                .relayed("/api/startSessionProvider", session.clone())
                .await
            {
                return self
                    .tell_coordinator(format!(
                        "{thread} ({reference}) was created on {computer}, but its agent did not start: {error} Check ghostex coordinator status; do not start another."
                    ))
                    .await;
            }
        }
        let mut prompts = Vec::new();
        if let Some(line) = &self.watch.model_line {
            prompts.push((line.clone(), None));
        }
        if let Some(message) = &self.watch.message {
            prompts.push((message.clone(), Some(self.watch.send_request_id.clone())));
        }
        for (text, send_request_id) in prompts {
            let mut params = session.clone();
            params["text"] = json!(text);
            params["startupSend"] = json!(true);
            if let Some(send_request_id) = send_request_id {
                params["sendRequestId"] = json!(send_request_id);
            }
            if let Err(error) = self.relayed("/api/queueSessionChatPrompt", params).await {
                return self
                    .tell_coordinator(format!(
                        "{thread} ({reference}) started on {computer}, but its task was not delivered: {error} Send it with ghostex agents send {reference} --body-file <file>; do not start another."
                    ))
                    .await;
            }
        }
    }

    /// Links the thread to its coordinator, as `/api/linkRemoteCoordinatorThread` does with
    /// `onlyIfCoordinator` (coordinators/remote_threads.rs).
    async fn link(&self, created: &CreatedSessionInfo) {
        let Some(coordinator) = self.watch.coordinator.clone() else {
            return;
        };
        let state = self.state.clone();
        let key = (
            self.server_id.clone(),
            created.project_id.clone(),
            created.session_id.clone(),
        );
        let (task, title) = (self.watch.task.clone(), self.watch.title.clone());
        let _ = tokio::task::spawn_blocking(move || {
            let db = open_gxserver_database(&state.paths).ok()?;
            crate::coordinators::read_coordinator(&db, &coordinator.0, &coordinator.1)
                .ok()
                .flatten()?;
            crate::coordinators::link_remote_thread(&db, &key, &coordinator, &task, &title).ok()?;
            let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
            let _ = schedule_presentation_session_delta(
                &state,
                &db,
                &repository,
                &coordinator.0,
                &coordinator.1,
            );
            Some(())
        })
        .await;
    }

    /// One call to the other computer through the relay, as the caller that asked for the watch.
    /// Boxed with a named type: `relay` leads to the request that spawns a watch, so as a plain
    /// `async fn` this future's type would depend on itself.
    fn relayed<'a>(
        &'a self,
        endpoint: &'a str,
        params: Value,
    ) -> Pin<Box<dyn Future<Output = Result<Value, String>> + Send + 'a>> {
        Box::pin(async move {
            let path = format!(
                "{}{endpoint}",
                super::relay::relay_base_url("", &self.server_id)
            );
            let mut headers = HeaderMap::new();
            headers.insert(
                GXSERVER_PROTOCOL_HEADER,
                HeaderValue::from(GXSERVER_PROTOCOL_VERSION),
            );
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            let body = json!({ "params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION });
            let routed = super::relay::relay(
                self.state.clone(),
                Method::POST,
                &path,
                None,
                headers,
                Body::from(body.to_string()),
                uuid::Uuid::new_v4().to_string(),
                &self.caller,
            )
            .await;
            let status = routed.response.status().as_u16();
            let bytes = to_bytes(routed.response.into_body(), WATCH_BODY_LIMIT_BYTES)
                .await
                .unwrap_or_default();
            if let Some(error) = remote_error_message(status, &bytes) {
                return Err(error);
            }
            Ok(serde_json::from_slice::<Value>(&bytes)
                .ok()
                .and_then(|answer| answer.get("result").cloned())
                .unwrap_or(Value::Null))
        })
    }

    /// One line into the coordinator's chat, delivered at most once per watched call.
    async fn tell_coordinator(&self, message: String) {
        let Some(coordinator) = &self.watch.coordinator else {
            return;
        };
        let line = clipped(&message.replace(['\n', '\r'], " "), MAX_ERROR_CHARS);
        let _ = crate::server::send_to_coordinator(
            &self.state,
            coordinator,
            &line,
            &format!("late-start:{}", self.request_id),
        )
        .await;
    }
}
