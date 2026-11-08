//! Messages held on this computer for a chat whose computer is offline, sent by this gxserver on
//! its own once that computer is connected again (`holdSessionChatSend`,
//! `listHeldSessionChatSends`, `cancelHeldSessionChatSend`).
//!
//! CDXC:RemoteMachines 2026-11-08 DECISION:
//! The user, on the red "Couldn't reach Ghostex" line a chat showed while its computer (a Windows PC) was switched off, picked from the offline-notice page: a message sent while the computer is offline is held ("hold"), the hold lives on THIS computer's gxserver ("gxserver") so it goes out even with the app closed or after a restart, and it waits at most 24 hours ("24h"), after which it goes back to the chat box with a note. Held messages of the session's own computer could not do it: that computer is the one that is off.
//! SEE-ALSO: packages/gx-chat-core/src/session/computer.rs (the chat's half: when a send is held,
//! the held rows and Cancel), registry.rs (`register` wakes the delivery when a computer is back).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::Notify;

use crate::{
    domain::read_domain_rpc_params,
    protocol::{rpc_error, rpc_success},
    server::{routed_json, AppState, RoutedResponse},
};

use super::registry::{self, ConnectedComputer};

pub(crate) const HELD_SEND_ENDPOINTS: &[&str] = &[
    "/api/holdSessionChatSend",
    "/api/listHeldSessionChatSends",
    "/api/cancelHeldSessionChatSend",
];

/// How long a message waits for its computer: the same day a message waiting for an agent to
/// start gets, and the time the other computer remembers a send's id for (so a held copy of a
/// send that did land there is answered from the first attempt, not typed twice).
const HOLD_LIMIT_MS: i64 = 24 * 60 * 60 * 1000;
/// How often the waiting messages are looked at besides a computer registering again.
const SWEEP_EVERY: Duration = Duration::from_secs(15);
/// A computer that does not answer its health check this fast is not back yet; the send itself
/// may then wait as long as any chat send does.
const HEALTH_TIMEOUT: Duration = Duration::from_secs(5);
const SEND_TIMEOUT: Duration = Duration::from_secs(16 * 60);
const MAX_HELD: usize = 200;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const FILE_NAME: &str = "held-computer-sends.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HeldSend {
    id: String,
    /// The other computer's gxserver, when the client knew it.
    #[serde(default)]
    server_id: String,
    /// The desktop app's own id for the computer, for a client that knew only that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    machine_id: Option<String>,
    computer_name: String,
    project_id: String,
    session_id: String,
    text: String,
    #[serde(default)]
    image_paths: Vec<String>,
    /// The chat send's own id, so a send that did reach the computer is never typed twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    send_request_id: Option<String>,
    held_at_ms: i64,
    /// Why it came back unsent; the chat puts it back in the box with this note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    returned: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct HeldFile {
    #[serde(default)]
    sends: Vec<HeldSend>,
    /// Every computer this one has been connected to, by server id, so a phone can tell which of
    /// its computers can hold a message for one that is offline (`knowsComputer`).
    #[serde(default)]
    known: BTreeMap<String, KnownComputer>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownComputer {
    name: String,
    last_seen_ms: i64,
}

/// How stale the stored "last seen" of a connected computer may get before it is written again.
const KNOWN_WRITE_EVERY_MS: i64 = 5 * 60 * 1000;

#[derive(Default)]
struct Store {
    path: Option<PathBuf>,
    file: HeldFile,
    /// Messages whose delivery is running now; Cancel waits for it.
    sending: HashSet<String>,
}

fn store() -> MutexGuard<'static, Store> {
    static STORE: OnceLock<Mutex<Store>> = OnceLock::new();
    STORE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// A computer registered: remember it (for `knowsComputer`), and look at its waiting messages.
pub(crate) fn note_registered(server_id: &str, name: &str) {
    let mut store = store();
    if store.path.is_some() {
        let now = now_ms();
        let stale = store.file.known.get(server_id).is_none_or(|known| {
            known.name != name || now - known.last_seen_ms > KNOWN_WRITE_EVERY_MS
        });
        if stale {
            store.file.known.insert(
                server_id.to_string(),
                KnownComputer {
                    name: name.to_string(),
                    last_seen_ms: now,
                },
            );
            save(&store);
        }
    }
    drop(store);
    nudge_delivery();
}

/// A message was just held: look at the waiting messages now.
pub(crate) fn nudge_delivery() {
    wake().notify_one();
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or_default()
}

/// Loads the file the first time this gxserver touches it.
fn opened(state_dir: &Path) -> MutexGuard<'static, Store> {
    let mut store = store();
    if store.path.is_none() {
        let path = state_dir.join(FILE_NAME);
        store.file = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        store.path = Some(path);
    }
    store
}

fn save(store: &Store) {
    let Some(path) = store.path.as_ref() else {
        return;
    };
    let Ok(text) = serde_json::to_string_pretty(&store.file) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    if std::fs::write(&temporary, text).is_ok() && std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

/// What a client sees of one held message.
fn view(send: &HeldSend, sending: bool) -> Value {
    json!({
        "id": send.id,
        "text": send.text,
        "imagePaths": send.image_paths,
        "heldAtMs": send.held_at_ms,
        "computerName": send.computer_name,
        "sendRequestId": send.send_request_id,
        "returned": send.returned,
        "sending": sending,
    })
}

pub(crate) fn handle_held_sends_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let refuse = |status: StatusCode, code: &str, message: String, endpoint: String, id: String| {
        routed_json(Some(endpoint), status, rpc_error(code, message, Some(id)))
    };
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return refuse(
                StatusCode::BAD_REQUEST,
                "badRequest",
                error.message,
                endpoint_path,
                request_id,
            );
        }
    };
    let result = match endpoint_path.as_str() {
        "/api/holdSessionChatSend" => hold(&state.paths.app_state_dir, &params),
        "/api/cancelHeldSessionChatSend" => cancel(&state.paths.app_state_dir, &params),
        _ => Ok(list(&state.paths.app_state_dir, &params)),
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err((code, message)) => refuse(
            StatusCode::BAD_REQUEST,
            code,
            message,
            endpoint_path,
            request_id,
        ),
    }
}

fn text_param(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn hold(state_dir: &Path, params: &Map<String, Value>) -> Result<Value, (&'static str, String)> {
    let bad = |message: &str| ("badRequest", message.to_string());
    let project_id =
        text_param(params, "projectId").ok_or_else(|| bad("projectId is required."))?;
    let session_id =
        text_param(params, "sessionId").ok_or_else(|| bad("sessionId is required."))?;
    let server_id = text_param(params, "serverId").unwrap_or_default();
    let machine_id = text_param(params, "machineId");
    if server_id.is_empty() && machine_id.is_none() {
        return Err(bad("serverId or machineId is required."));
    }
    if !server_id.is_empty() && !registry::is_server_id(&server_id) {
        return Err(bad("serverId is not a gxserver server id."));
    }
    let text = params
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let image_paths: Vec<String> = params
        .get("imagePaths")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    if text.trim().is_empty() && image_paths.is_empty() {
        return Err(bad("There is nothing to send."));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(bad("The message is too long to hold."));
    }
    let computer_name = text_param(params, "computerName").unwrap_or_else(|| {
        if server_id.is_empty() {
            "The computer".to_string()
        } else {
            server_id.clone()
        }
    });
    let send_request_id = text_param(params, "sendRequestId");
    let mut store = opened(state_dir);
    // The chat asks again with the same send id after a retry: it is the same message.
    if let Some(existing) = send_request_id.as_deref().and_then(|wanted| {
        store.file.sends.iter().find(|send| {
            send.returned.is_none()
                && send.session_id == session_id
                && send.send_request_id.as_deref() == Some(wanted)
        })
    }) {
        let sending = store.sending.contains(&existing.id);
        return Ok(json!({ "held": view(existing, sending) }));
    }
    if store.file.sends.len() >= MAX_HELD {
        return Err((
            "tooManyHeld",
            "Too many messages are already waiting for computers that are offline.".to_string(),
        ));
    }
    let held_at_ms = now_ms();
    let send = HeldSend {
        id: format!("held-{held_at_ms}-{:08x}", rand::random::<u32>()),
        server_id,
        machine_id,
        computer_name,
        project_id,
        session_id,
        text,
        image_paths,
        send_request_id,
        held_at_ms,
        returned: None,
    };
    let answer = json!({ "held": view(&send, false) });
    store.file.sends.push(send);
    save(&store);
    drop(store);
    nudge_delivery();
    Ok(answer)
}

fn list(state_dir: &Path, params: &Map<String, Value>) -> Value {
    let project_id = text_param(params, "projectId");
    let session_id = text_param(params, "sessionId");
    let store = opened(state_dir);
    let sends: Vec<Value> = store
        .file
        .sends
        .iter()
        .filter(|send| project_id.as_deref().is_none_or(|id| id == send.project_id))
        .filter(|send| session_id.as_deref().is_none_or(|id| id == send.session_id))
        .map(|send| view(send, store.sending.contains(&send.id)))
        .collect();
    let mut answer = json!({ "sends": sends });
    // A phone asks each of its computers whether it was ever connected to the one that is off.
    if let Some(server_id) = text_param(params, "serverId") {
        answer["knowsComputer"] = json!(store.file.known.contains_key(&server_id));
    }
    answer
}

/// Takes a message off hold (Cancel), or clears one that came back unsent once the chat put it
/// back in the box. Answers its text.
fn cancel(state_dir: &Path, params: &Map<String, Value>) -> Result<Value, (&'static str, String)> {
    let id = text_param(params, "id").ok_or(("badRequest", "id is required.".to_string()))?;
    let mut store = opened(state_dir);
    if store.sending.contains(&id) {
        return Err((
            "heldSendInProgress",
            "It's being sent now, so it can't be cancelled.".to_string(),
        ));
    }
    let Some(index) = store.file.sends.iter().position(|send| send.id == id) else {
        return Ok(json!({ "cancelled": false }));
    };
    let send = store.file.sends.remove(index);
    save(&store);
    Ok(json!({
        "cancelled": true,
        "text": send.text,
        "imagePaths": send.image_paths,
    }))
}

/// Starts the delivery: every 15 s, and at once when a computer registers again.
pub(crate) fn start_held_send_delivery(state: Arc<AppState>) {
    drop(opened(&state.paths.app_state_dir));
    let mut shutdown = state.shutdown_tx.subscribe();
    tokio::spawn(async move {
        let mut tick = crate::ticks::steady_interval(SWEEP_EVERY);
        loop {
            tokio::select! {
                _ = shutdown.recv() => break,
                _ = tick.tick() => {}
                () = wake().notified() => {}
            }
            for (computer, sends) in due() {
                tokio::task::spawn_blocking(move || deliver_in_order(&computer, sends));
            }
        }
    });
}

/// The messages that can go now, grouped per computer and chat in the order they were held, and
/// marked as sending. Ends the holds that are past their day.
fn due() -> Vec<(ConnectedComputer, Vec<HeldSend>)> {
    let mut store = store();
    if store.path.is_none() {
        return Vec::new();
    }
    let now = now_ms();
    let mut changed = false;
    let mut groups: BTreeMap<(String, String, String), (ConnectedComputer, Vec<HeldSend>)> =
        BTreeMap::new();
    let busy: HashSet<(String, String)> = store
        .file
        .sends
        .iter()
        .filter(|send| store.sending.contains(&send.id))
        .map(|send| (send.project_id.clone(), send.session_id.clone()))
        .collect();
    for send in store.file.sends.iter_mut() {
        if send.returned.is_some() {
            continue;
        }
        if now - send.held_at_ms > HOLD_LIMIT_MS {
            send.returned = Some(format!(
                "Not sent: {} was offline for a day.",
                send.computer_name
            ));
            changed = true;
            continue;
        }
        // A chat whose earlier message is still going waits for it, so the order holds.
        if busy.contains(&(send.project_id.clone(), send.session_id.clone())) {
            continue;
        }
        let Some(computer) =
            registry::find_by_server_or_machine(&send.server_id, send.machine_id.as_deref())
        else {
            continue;
        };
        if !computer.agent_access || computer.base_url.is_empty() {
            send.returned = Some(format!(
                "Not sent: {}",
                super::relay::access_off_message(&computer)
            ));
            changed = true;
            continue;
        }
        if send.server_id.is_empty() {
            send.server_id = computer.server_id.clone();
            changed = true;
        }
        groups
            .entry((
                computer.server_id.clone(),
                send.project_id.clone(),
                send.session_id.clone(),
            ))
            .or_insert_with(|| (computer, Vec::new()))
            .1
            .push(send.clone());
    }
    for (_, sends) in groups.values() {
        for send in sends {
            store.sending.insert(send.id.clone());
        }
    }
    if changed {
        save(&store);
    }
    groups.into_values().collect()
}

enum Delivery {
    Sent,
    /// The computer is not answering yet; the message keeps waiting.
    Wait,
    /// The computer answered with a refusal (the chat was closed there, for one); the message
    /// goes back to the chat box with it.
    Refused(String),
}

fn deliver_in_order(computer: &ConnectedComputer, sends: Vec<HeldSend>) {
    let mut stopped = false;
    for send in sends {
        let outcome = if stopped {
            Delivery::Wait
        } else {
            deliver(computer, &send)
        };
        let mut store = store();
        store.sending.remove(&send.id);
        match outcome {
            Delivery::Sent => store.file.sends.retain(|held| held.id != send.id),
            Delivery::Refused(reason) => {
                if let Some(held) = store.file.sends.iter_mut().find(|held| held.id == send.id) {
                    held.returned = Some(format!("Not sent: {reason}"));
                }
            }
            Delivery::Wait => stopped = true,
        }
        save(&store);
    }
}

fn deliver(computer: &ConnectedComputer, send: &HeldSend) -> Delivery {
    let base = &computer.base_url;
    // The token goes only to a listener of this account (ghostex_paths::loopback_owner).
    if computer.check_listener(base).is_err() {
        return Delivery::Wait;
    }
    let agent = crate::http_agent::answering_agent();
    let authorization = format!("Bearer {}", computer.token());
    let healthy = agent
        .get(format!("{base}/api/health/server"))
        .header("authorization", &authorization)
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            &crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(HEALTH_TIMEOUT))
        .build()
        .call()
        .is_ok_and(|response| response.status().is_success());
    if !healthy {
        return Delivery::Wait;
    }
    let mut body = json!({
        "projectId": send.project_id,
        "sessionId": send.session_id,
        "text": send.text,
    });
    if !send.image_paths.is_empty() {
        body["imagePaths"] = json!(send.image_paths);
    }
    if let Some(id) = send.send_request_id.as_deref() {
        body["sendRequestId"] = json!(id);
    }
    let response = agent
        .post(format!("{base}/api/sendSessionChatMessage"))
        .header("authorization", &authorization)
        .header("content-type", "application/json")
        .header(
            crate::constants::GXSERVER_PROTOCOL_HEADER,
            &crate::constants::GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .config()
        .timeout_global(Some(SEND_TIMEOUT))
        .http_status_as_error(false)
        .build()
        .send(json!({ "params": body }).to_string().as_bytes());
    let Ok(response) = response else {
        return Delivery::Wait;
    };
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    if std::io::Read::read_to_end(&mut response.into_body().into_reader(), &mut bytes).is_err() {
        return Delivery::Wait;
    }
    let answer: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if (200..300).contains(&status) && answer.get("ok") == Some(&Value::Bool(true)) {
        return Delivery::Sent;
    }
    let code = answer.get("error").and_then(Value::as_str).unwrap_or("");
    // Still starting, a link that dropped mid-call, or a token the desktop app has not renewed
    // yet (a grant lasts three minutes between renewals): try again on the next round. Any other
    // refusal the computer named (the chat was closed there, its agent is not running) is final.
    if status == 401
        || code.is_empty()
        || matches!(
            code,
            "computerUnreachable" | "unreachable" | "serverStarting"
        )
    {
        return Delivery::Wait;
    }
    Delivery::Refused(
        answer
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("the computer refused it.")
            .to_string(),
    )
}
