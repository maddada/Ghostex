//! Ghostex Collab's relay connector: while Ghostex Collab is on and a chat is shared, gxserver
//! keeps one outgoing WebSocket to the share relay (share.ghostex.dev, `server/share-relay`) and
//! registers each shared chat's route there. Each guest connection the relay hands over is
//! unlocked with the share's key (`collab::tunnel`), asked for its name and code, and then carries
//! the guest page's own gxserver calls: `http` requests and the chat's `/api/events` stream, which
//! this connector replays against this gxserver with the share's token, never the main one.
//!
//! Guest frames (JSON, inside the lock):
//! - guest → host: `{t:"hello"}`, `{t:"join", name, code}`, `{t:"http", id, path, body}`,
//!   `{t:"ws-open", id, path}`, `{t:"ws", id, text}`, `{t:"ws-close", id}`.
//! - host → guest: `{t:"intro", title, owner, needsCode, mode}`, `{t:"welcome", viewerId, name,
//!   projectId, sessionId, title, owner, mode}`, `{t:"denied", reason, triesLeft?}`,
//!   `{t:"http", id, status, body}`, `{t:"ws-open", id}`, `{t:"ws", id, text}`, `{t:"ws-close", id}`.
//!
//! CDXC:Collab 2026-11-07 DECISION:
//! User (Q1611): "gxserver keeps one outgoing connection to it only while Collab is on and a chat is shared."
//! SEE-ALSO: apps/gpui-web/www/src/collab/tunnel.js (the guest's half).

use super::*;

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use subtle::ConstantTimeEq;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

use crate::collab::{self, shares, tunnel::HostCipher};

static CONNECTED: AtomicBool = AtomicBool::new(false);

/// Whether the relay connection is up (Settings > Collab and the Share popover show it).
pub(crate) fn relay_connected() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

const PING_EVERY: Duration = Duration::from_secs(25);
const RETRY_DELAYS: [u64; 6] = [1, 2, 5, 10, 20, 30];
const MAX_HTTP_BODY: usize = 32 << 20;

pub(crate) fn start_collab_connector(state: Arc<AppState>) {
    tokio::spawn(async move {
        let mut failures = 0usize;
        loop {
            let config = collab::read_config(&state.paths);
            let wanted = config.enabled && !shares::list().is_empty();
            if !wanted {
                CONNECTED.store(false, Ordering::Relaxed);
                failures = 0;
                let _ = tokio::time::timeout(Duration::from_secs(60), collab::changed()).await;
                continue;
            }
            let relay = collab::config::relay_url(&config);
            let url = format!("{}/h", relay.replacen("http", "ws", 1));
            match tokio_tungstenite::connect_async(url.as_str()).await {
                Ok((socket, _)) => {
                    failures = 0;
                    CONNECTED.store(true, Ordering::Relaxed);
                    republish_all(&state);
                    run(&state, socket).await;
                    CONNECTED.store(false, Ordering::Relaxed);
                    republish_all(&state);
                }
                Err(error) => {
                    tracing_log(&format!("Ghostex Collab couldn't reach {url}: {error}"));
                    let wait = RETRY_DELAYS[failures.min(RETRY_DELAYS.len() - 1)];
                    failures += 1;
                    let _ =
                        tokio::time::timeout(Duration::from_secs(wait), collab::changed()).await;
                }
            }
        }
    });
}

fn tracing_log(message: &str) {
    eprintln!("[collab] {message}");
}

fn republish_all(state: &AppState) {
    for share in shares::list() {
        crate::server::republish_session_chat(state, &share.project_id, &share.session_id);
    }
}

struct Conn {
    share_id: String,
    key: [u8; 32],
    cipher: Option<HostCipher>,
    viewer: Option<shares::Viewer>,
    code_tries: u32,
    sockets: HashMap<u64, mpsc::UnboundedSender<String>>,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Conn {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

type GuestOut = mpsc::UnboundedSender<(u64, Value)>;

async fn run(
    state: &Arc<AppState>,
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) {
    let (mut sink, mut stream) = socket.split();
    let (relay_tx, mut relay_rx) = mpsc::unbounded_channel::<Value>();
    let writer = tokio::spawn(async move {
        while let Some(frame) = relay_rx.recv().await {
            if sink
                .send(WsMessage::Text(frame.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
    let (guest_tx, mut guest_rx) = mpsc::unbounded_channel::<(u64, Value)>();
    // route → share id, as registered with the relay.
    let mut routes: HashMap<String, String> = HashMap::new();
    let mut conns: HashMap<u64, Conn> = HashMap::new();
    let mut ping = tokio::time::interval(PING_EVERY);
    reconcile(state, &relay_tx, &mut routes, &mut conns);
    loop {
        tokio::select! {
            frame = stream.next() => {
                let Some(Ok(frame)) = frame else { break };
                let WsMessage::Text(text) = frame else { continue };
                let Ok(frame) = serde_json::from_str::<Value>(text.as_str()) else { continue };
                relay_frame(state, &relay_tx, &guest_tx, &routes, &mut conns, frame);
            }
            out = guest_rx.recv() => {
                let Some((conn_id, plain)) = out else { break };
                if let Some(conn) = conns.get_mut(&conn_id) {
                    if let Some(cipher) = conn.cipher.as_mut() {
                        let sealed = cipher.seal(plain.to_string().as_bytes());
                        let _ = relay_tx.send(json!({"op": "data", "conn": conn_id, "data": STANDARD.encode(sealed)}));
                    }
                }
            }
            _ = collab::changed() => {
                if !reconcile(state, &relay_tx, &mut routes, &mut conns) { break; }
            }
            _ = ping.tick() => {
                let _ = relay_tx.send(json!({"op": "ping"}));
                if !reconcile(state, &relay_tx, &mut routes, &mut conns) { break; }
            }
        }
    }
    for (_, conn) in conns.drain() {
        if let Some(viewer) = &conn.viewer {
            shares::leave(&conn.share_id, &viewer.id);
        }
    }
    writer.abort();
}

/// Registers new shares, drops ended ones and guests the owner removed. `false` when nothing is
/// shared any more, which ends the relay connection.
fn reconcile(
    state: &AppState,
    relay_tx: &mpsc::UnboundedSender<Value>,
    routes: &mut HashMap<String, String>,
    conns: &mut HashMap<u64, Conn>,
) -> bool {
    let enabled = collab::read_config(&state.paths).enabled;
    let live: Vec<shares::StoredShare> = if enabled { shares::list() } else { Vec::new() };
    let wanted: HashMap<String, &shares::StoredShare> = live
        .iter()
        .map(|share| (share.route.clone(), share))
        .collect();
    for (route, share) in &wanted {
        if !routes.contains_key(route) {
            let _ = relay_tx
                .send(json!({"op": "register", "route": route, "secret": share.host_secret}));
            routes.insert(route.clone(), share.id.clone());
        }
    }
    let ended: Vec<String> = routes
        .keys()
        .filter(|route| !wanted.contains_key(*route))
        .cloned()
        .collect();
    for route in ended {
        let _ = relay_tx.send(json!({"op": "unregister", "route": route}));
        if let Some(share_id) = routes.remove(&route) {
            conns.retain(|_, conn| conn.share_id != share_id);
        }
    }
    let removed: Vec<u64> = conns
        .iter()
        .filter(|(_, conn)| {
            conn.viewer
                .as_ref()
                .is_some_and(|viewer| !shares::viewer_ids(&conn.share_id).contains(&viewer.id))
        })
        .map(|(id, _)| *id)
        .collect();
    for id in removed {
        let _ = relay_tx.send(json!({"op": "close", "conn": id}));
        conns.remove(&id);
    }
    !live.is_empty()
}

fn relay_frame(
    state: &Arc<AppState>,
    relay_tx: &mpsc::UnboundedSender<Value>,
    guest_tx: &GuestOut,
    routes: &HashMap<String, String>,
    conns: &mut HashMap<u64, Conn>,
    frame: Value,
) {
    let conn_id = frame["conn"].as_u64().unwrap_or_default();
    match frame["op"].as_str().unwrap_or_default() {
        "open" => {
            let route = frame["route"].as_str().unwrap_or_default();
            let Some(share) = routes.get(route).and_then(|id| shares::get(id)) else {
                let _ = relay_tx.send(json!({"op": "close", "conn": conn_id}));
                return;
            };
            let Some(key) = URL_SAFE_NO_PAD
                .decode(&share.key)
                .ok()
                .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            else {
                return;
            };
            conns.insert(
                conn_id,
                Conn {
                    share_id: share.id,
                    key,
                    cipher: None,
                    viewer: None,
                    code_tries: 0,
                    sockets: HashMap::new(),
                    tasks: Vec::new(),
                },
            );
        }
        "close" => {
            if let Some(conn) = conns.remove(&conn_id) {
                if let Some(viewer) = &conn.viewer {
                    if shares::leave(&conn.share_id, &viewer.id) {
                        if let Some(share) = shares::get(&conn.share_id) {
                            crate::server::republish_session_chat(
                                state,
                                &share.project_id,
                                &share.session_id,
                            );
                        }
                    }
                }
            }
        }
        "data" => {
            let Ok(bytes) = STANDARD.decode(frame["data"].as_str().unwrap_or_default()) else {
                return;
            };
            let Some(conn) = conns.get_mut(&conn_id) else {
                return;
            };
            let Some(cipher) = conn.cipher.as_mut() else {
                match HostCipher::from_hello(&conn.key, &bytes) {
                    Some(cipher) => conn.cipher = Some(cipher),
                    None => {
                        let _ = relay_tx.send(json!({"op": "close", "conn": conn_id}));
                        conns.remove(&conn_id);
                    }
                }
                return;
            };
            let Some(plain) = cipher.open(&bytes) else {
                // A frame that does not open was not sealed with this link's key, or arrived out
                // of order: the connection can't be trusted any more.
                let _ = relay_tx.send(json!({"op": "close", "conn": conn_id}));
                conns.remove(&conn_id);
                return;
            };
            let Ok(message) = serde_json::from_slice::<Value>(&plain) else {
                return;
            };
            if !guest_message(state, guest_tx, conn_id, conn, message) {
                let _ = relay_tx.send(json!({"op": "close", "conn": conn_id}));
                if let Some(conn) = conns.remove(&conn_id) {
                    if let Some(viewer) = &conn.viewer {
                        shares::leave(&conn.share_id, &viewer.id);
                    }
                }
            }
        }
        _ => {}
    }
}

/// One guest frame. `false` closes the connection.
fn guest_message(
    state: &Arc<AppState>,
    guest_tx: &GuestOut,
    conn_id: u64,
    conn: &mut Conn,
    message: Value,
) -> bool {
    let reply = |value: Value| {
        let _ = guest_tx.send((conn_id, value));
    };
    let Some(share) = shares::get(&conn.share_id) else {
        reply(json!({"t": "denied", "reason": "ended"}));
        return false;
    };
    let config = collab::read_config(&state.paths);
    let owner = collab::config::owner_display_name(&config);
    let title = super::collab_http::share_title(state, &share);
    match message["t"].as_str().unwrap_or_default() {
        "hello" => {
            reply(json!({
                "t": "intro",
                "title": title,
                "owner": owner,
                "needsCode": !share.code.is_empty(),
                "mode": share.mode.as_str(),
            }));
            true
        }
        "join" => {
            if conn.viewer.is_some() {
                return true;
            }
            let name: String = message["name"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .chars()
                .take(40)
                .collect();
            if name.is_empty() {
                reply(json!({"t": "denied", "reason": "name"}));
                return true;
            }
            if !share.code.is_empty() {
                let typed = message["code"]
                    .as_str()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if !bool::from(typed.as_bytes().ct_eq(share.code.as_bytes())) {
                    conn.code_tries += 1;
                    let left = shares::MAX_CODE_TRIES.saturating_sub(conn.code_tries);
                    reply(json!({"t": "denied", "reason": "code", "triesLeft": left}));
                    return left > 0;
                }
            }
            match shares::join(&share.id, &name) {
                Ok(viewer) => {
                    reply(json!({
                        "t": "welcome",
                        "viewerId": viewer.id,
                        "name": viewer.name,
                        "projectId": share.project_id,
                        "sessionId": share.session_id,
                        "title": title,
                        "owner": owner,
                        "mode": share.mode.as_str(),
                    }));
                    conn.viewer = Some(viewer);
                    crate::server::republish_session_chat(
                        state,
                        &share.project_id,
                        &share.session_id,
                    );
                    collab::notify_changed();
                    true
                }
                Err(shares::JoinRefusal::Full) => {
                    reply(json!({"t": "denied", "reason": "full"}));
                    false
                }
                Err(shares::JoinRefusal::Ended) => {
                    reply(json!({"t": "denied", "reason": "ended"}));
                    false
                }
            }
        }
        "http" => {
            let Some(viewer) = conn.viewer.clone() else {
                return false;
            };
            let id = message["id"].clone();
            let path = message["path"].as_str().unwrap_or_default().to_string();
            let body = message["body"].as_str().unwrap_or_default().to_string();
            let state = state.clone();
            let guest_tx = guest_tx.clone();
            conn.tasks.retain(|task| !task.is_finished());
            conn.tasks.push(tokio::spawn(async move {
                let (status, text) = guest_http(&state, &share, &viewer, &path, body).await;
                let _ = guest_tx.send((
                    conn_id,
                    json!({"t": "http", "id": id, "status": status, "body": text}),
                ));
            }));
            true
        }
        "ws-open" => {
            if conn.viewer.is_none() {
                return false;
            }
            let Some(id) = message["id"].as_u64() else {
                return true;
            };
            let path = message["path"].as_str().unwrap_or_default();
            let chat_stream = format!(
                "stream={}",
                ghostex_gx_protocol::GXSERVER_EVENTS_STREAM_SESSION_CHAT
            );
            if !path.starts_with(ghostex_gx_protocol::GXSERVER_EVENTS_PATH)
                || !path.split(['?', '&']).any(|part| part == chat_stream)
            {
                reply(json!({"t": "ws-close", "id": id}));
                return true;
            }
            let (tx, rx) = mpsc::unbounded_channel::<String>();
            conn.sockets.insert(id, tx);
            let url = format!(
                "ws://{}:{}{}?protocolVersion={}&stream={}&authToken={}",
                state.config.listeners.local.host,
                state.config.listeners.local.port,
                ghostex_gx_protocol::GXSERVER_EVENTS_PATH,
                GXSERVER_PROTOCOL_VERSION,
                ghostex_gx_protocol::GXSERVER_EVENTS_STREAM_SESSION_CHAT,
                share.token,
            );
            let guest_tx = guest_tx.clone();
            conn.tasks
                .push(tokio::spawn(guest_socket(url, id, conn_id, rx, guest_tx)));
            true
        }
        "ws" => {
            if let (Some(id), Some(text)) = (message["id"].as_u64(), message["text"].as_str()) {
                if let Some(socket) = conn.sockets.get(&id) {
                    let _ = socket.send(text.to_string());
                }
            }
            true
        }
        "ws-close" => {
            if let Some(id) = message["id"].as_u64() {
                conn.sockets.remove(&id);
            }
            true
        }
        _ => true,
    }
}

async fn guest_http(
    state: &Arc<AppState>,
    share: &shares::StoredShare,
    viewer: &shares::Viewer,
    path: &str,
    body: String,
) -> (u16, String) {
    if !path.starts_with("/api/") || path.contains("..") || path.contains('?') {
        return (
            404,
            json!({"ok": false, "error": "notFound", "message": "Not found."}).to_string(),
        );
    }
    let request = match axum::http::Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {}", share.token))
        .header(
            "x-gxserver-protocol-version",
            GXSERVER_PROTOCOL_VERSION.to_string(),
        )
        .header(super::collab_http::VIEWER_HEADER, viewer.id.as_str())
        .body(Body::from(body))
    {
        Ok(request) => request,
        Err(_) => return (400, String::new()),
    };
    let request_id = Uuid::new_v4().to_string();
    let routed = super::route_http::route_http(state.clone(), request, request_id).await;
    let status = routed.response.status().as_u16();
    let bytes = to_bytes(routed.response.into_body(), MAX_HTTP_BODY)
        .await
        .unwrap_or_default();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// One guest WebSocket (the chat's stream), opened on this gxserver's own listener with the
/// share's token, so `/api/events` itself keeps it to the shared chat.
async fn guest_socket(
    url: String,
    id: u64,
    conn_id: u64,
    mut from_guest: mpsc::UnboundedReceiver<String>,
    guest_tx: GuestOut,
) {
    let Ok((socket, _)) = tokio_tungstenite::connect_async(url.as_str()).await else {
        let _ = guest_tx.send((conn_id, json!({"t": "ws-close", "id": id})));
        return;
    };
    let _ = guest_tx.send((conn_id, json!({"t": "ws-open", "id": id})));
    let (mut sink, mut stream) = socket.split();
    loop {
        tokio::select! {
            frame = stream.next() => match frame {
                Some(Ok(WsMessage::Text(text))) => {
                    let _ = guest_tx.send((conn_id, json!({"t": "ws", "id": id, "text": text.as_str()})));
                }
                Some(Ok(WsMessage::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            text = from_guest.recv() => match text {
                Some(text) => {
                    if sink.send(WsMessage::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
        }
    }
    let _ = sink.close().await;
    let _ = guest_tx.send((conn_id, json!({"t": "ws-close", "id": id})));
}
