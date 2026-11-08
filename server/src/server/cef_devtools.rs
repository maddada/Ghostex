//! The Chrome DevTools protocol for Ghostex's own pages (the Code view, Browser tabs, Files,
//! extension views), carried through gxserver instead of CEF's open remote-debugging port.
//!
//! CDXC:Browser 2026-10-25 DECISION:
//! User (security audit, Q11 "also move the skill off the open port"): CEF's DevTools port was always open on 127.0.0.1 with `remote-allow-origins=*`, so any local program, or a web page in another browser, could drive every Ghostex page. The desktop now joins gxserver as the DevTools host (`/api/cefDevtools/host`, main token only) and serves each page through CEF's in-process DevTools API. `ghostex browser mcp` lists pages at `/api/cefDevtools/json` and talks to one at `/api/cefDevtools/page/<id>`, both with the gxserver token, and CEF opens its port only for development (an explicit port variable or Show debug UI controls).
//! Wire between gxserver and the host, one JSON text frame each:
//! gxserver → host: `{type:"list",requestId}`, `{type:"attach",session,pageId}`, `{type:"send",session,message}`, `{type:"detach",session}`.
//! host → gxserver: `{type:"pages",requestId,pages:[{id,title,url}]}`, `{type:"message",session,message}`, `{type:"closed",session,reason}`.
//! SEE-ALSO: apps/desktop/src/cef/shell/devtools_relay.rs (the host), server/src/ghostex_cli/browser_mcp/cdp_pages.rs (the client).
use super::*;
use axum::extract::Path;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{mpsc, oneshot};

/// Screenshots of long pages arrive as one base64 message.
const DEVTOOLS_MESSAGE_BYTES: usize = 128 * 1024 * 1024;
const PAGE_LIST_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
pub(crate) struct CefDevtoolsHub {
    inner: Arc<Mutex<HubState>>,
}

#[derive(Default)]
struct HubState {
    /// The connected desktop: its connection number and the sender to its socket.
    host: Option<(u64, mpsc::UnboundedSender<String>)>,
    next_id: u64,
    lists: HashMap<u64, oneshot::Sender<Vec<Value>>>,
    /// Open page sessions: the host connection they belong to, and their socket's sender.
    sessions: HashMap<u64, (u64, mpsc::UnboundedSender<SessionEvent>)>,
}

enum SessionEvent {
    Message(String),
    Closed(String),
}

impl CefDevtoolsHub {
    fn state(&self) -> std::sync::MutexGuard<'_, HubState> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn next_id(state: &mut HubState) -> u64 {
        state.next_id += 1;
        state.next_id
    }

    /// Makes `sender` the host; a previous host's sessions end.
    fn connect_host(&self, sender: mpsc::UnboundedSender<String>) -> u64 {
        let mut state = self.state();
        let connection = Self::next_id(&mut state);
        state.host = Some((connection, sender));
        Self::close_sessions_except(&mut state, connection);
        connection
    }

    fn disconnect_host(&self, connection: u64) {
        let mut state = self.state();
        if state.host.as_ref().map(|(current, _)| *current) == Some(connection) {
            state.host = None;
            state.lists.clear();
        }
        Self::close_sessions_of(&mut state, connection);
    }

    fn close_sessions_except(state: &mut HubState, connection: u64) {
        state.sessions.retain(|_, (owner, sender)| {
            let keep = *owner == connection;
            if !keep {
                let _ = sender.send(SessionEvent::Closed(
                    "The Ghostex app reconnected.".to_string(),
                ));
            }
            keep
        });
    }

    fn close_sessions_of(state: &mut HubState, connection: u64) {
        state.sessions.retain(|_, (owner, sender)| {
            let keep = *owner != connection;
            if !keep {
                let _ = sender.send(SessionEvent::Closed(
                    "The Ghostex app disconnected.".to_string(),
                ));
            }
            keep
        });
    }

    fn send_to_host(&self, frame: Value) -> bool {
        let state = self.state();
        state
            .host
            .as_ref()
            .is_some_and(|(_, sender)| sender.send(frame.to_string()).is_ok())
    }

    async fn list_pages(&self) -> Option<Vec<Value>> {
        let (sender, receiver) = oneshot::channel();
        let request_id = {
            let mut state = self.state();
            state.host.as_ref()?;
            let request_id = Self::next_id(&mut state);
            state.lists.insert(request_id, sender);
            request_id
        };
        if !self.send_to_host(json!({ "type": "list", "requestId": request_id })) {
            self.state().lists.remove(&request_id);
            return None;
        }
        let pages = tokio::time::timeout(PAGE_LIST_TIMEOUT, receiver)
            .await
            .ok()
            .and_then(Result::ok);
        self.state().lists.remove(&request_id);
        pages
    }

    fn open_session(&self) -> Option<(u64, mpsc::UnboundedReceiver<SessionEvent>)> {
        let mut state = self.state();
        let connection = state.host.as_ref()?.0;
        let session = Self::next_id(&mut state);
        let (sender, receiver) = mpsc::unbounded_channel();
        state.sessions.insert(session, (connection, sender));
        Some((session, receiver))
    }

    fn close_session(&self, session: u64) {
        if self.state().sessions.remove(&session).is_some() {
            self.send_to_host(json!({ "type": "detach", "session": session }));
        }
    }

    /// Routes one frame from the host.
    fn receive_from_host(&self, connection: u64, text: &str) {
        let Ok(frame) = serde_json::from_str::<Value>(text) else {
            return;
        };
        let mut state = self.state();
        match frame.get("type").and_then(Value::as_str) {
            Some("pages") => {
                let Some(request_id) = frame.get("requestId").and_then(Value::as_u64) else {
                    return;
                };
                let pages = frame
                    .get("pages")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if let Some(sender) = state.lists.remove(&request_id) {
                    let _ = sender.send(pages);
                }
            }
            Some("message") => {
                let (Some(session), Some(message)) = (
                    frame.get("session").and_then(Value::as_u64),
                    frame.get("message").and_then(Value::as_str),
                ) else {
                    return;
                };
                if let Some((owner, sender)) = state.sessions.get(&session) {
                    if *owner == connection {
                        let _ = sender.send(SessionEvent::Message(message.to_string()));
                    }
                }
            }
            Some("closed") => {
                let Some(session) = frame.get("session").and_then(Value::as_u64) else {
                    return;
                };
                if state
                    .sessions
                    .get(&session)
                    .is_some_and(|(owner, _)| *owner == connection)
                {
                    if let Some((_, sender)) = state.sessions.remove(&session) {
                        let reason = frame
                            .get("reason")
                            .and_then(Value::as_str)
                            .unwrap_or("The page closed.")
                            .to_string();
                        let _ = sender.send(SessionEvent::Closed(reason));
                    }
                }
            }
            _ => {}
        }
    }
}

fn devtools_failure(
    headers: &HeaderMap,
    status: StatusCode,
    code: &str,
    message: &str,
) -> Response<Body> {
    json_response(status, rpc_error(code, message, Some(request_id(headers))))
}

/// The desktop joins as the DevTools host. Only the main token or the desktop's owner session
/// (which the app sends once it is open, #495): a paired phone's token cannot pose as the app.
pub(crate) async fn handle_cef_devtools_host(
    State(state): State<Arc<AppState>>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
) -> Response<Body> {
    let app = crate::auth::is_authorized_headers(&headers, &state.auth_token)
        || crate::auth::bearer_token(&headers)
            .is_some_and(|token| state.security.session_tokens.is_owner_session(token));
    if !app {
        return devtools_failure(
            &headers,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "gxserver authorization is required.",
        );
    }
    let Ok(ws) = ws else {
        return devtools_failure(
            &headers,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "A DevTools host WebSocket is required.",
        );
    };
    let hub = state.cef_devtools.clone();
    let mut shutdown = state.shutdown_tx.subscribe();
    ws.max_frame_size(DEVTOOLS_MESSAGE_BYTES)
        .max_message_size(DEVTOOLS_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            let (mut socket_tx, mut socket_rx) = socket.split();
            let (sender, mut outgoing) = mpsc::unbounded_channel::<String>();
            let connection = hub.connect_host(sender);
            let write = async {
                while let Some(frame) = outgoing.recv().await {
                    if socket_tx.send(Message::Text(frame.into())).await.is_err() {
                        break;
                    }
                }
            };
            let read = async {
                while let Some(Ok(message)) = socket_rx.next().await {
                    match message {
                        Message::Text(text) => hub.receive_from_host(connection, text.as_str()),
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            };
            tokio::select! {
                _ = shutdown.recv() => {},
                _ = write => {},
                _ = read => {},
            }
            hub.disconnect_host(connection);
        })
}

/// `GET /api/cefDevtools/json`: the pages, shaped like CEF's own `/json` list.
pub(crate) async fn handle_cef_devtools_pages(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Response<Body> {
    if !is_authorized_client_headers(
        &headers,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    ) {
        return devtools_failure(
            &headers,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "gxserver authorization is required.",
        );
    }
    let Some(pages) = state.cef_devtools.list_pages().await else {
        return devtools_failure(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "No Ghostex app with browser pages is connected to this gxserver.",
        );
    };
    let port = state.config.listeners.local.port;
    let targets: Vec<Value> = pages
        .into_iter()
        .filter_map(|page| {
            let id = page.get("id").and_then(Value::as_str)?.to_string();
            Some(json!({
                "id": id,
                "type": "page",
                "title": page.get("title").and_then(Value::as_str).unwrap_or_default(),
                "url": page.get("url").and_then(Value::as_str).unwrap_or_default(),
                "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/api/cefDevtools/page/{id}"),
            }))
        })
        .collect();
    json_response(StatusCode::OK, Value::Array(targets))
}

/// `GET /api/cefDevtools/page/<id>`: one DevTools session with that page.
pub(crate) async fn handle_cef_devtools_page(
    State(state): State<Arc<AppState>>,
    Path(page_id): Path<String>,
    ws: std::result::Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    headers: HeaderMap,
) -> Response<Body> {
    if !is_authorized_client_headers(
        &headers,
        &state.auth_token,
        &state.device_api_tokens,
        &state.security.session_tokens,
    ) {
        return devtools_failure(
            &headers,
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "gxserver authorization is required.",
        );
    }
    let Ok(ws) = ws else {
        return devtools_failure(
            &headers,
            StatusCode::BAD_REQUEST,
            "badRequest",
            "A DevTools page WebSocket is required.",
        );
    };
    let hub = state.cef_devtools.clone();
    let Some((session, mut events)) = hub.open_session() else {
        return devtools_failure(
            &headers,
            StatusCode::SERVICE_UNAVAILABLE,
            "unavailable",
            "No Ghostex app with browser pages is connected to this gxserver.",
        );
    };
    let mut shutdown = state.shutdown_tx.subscribe();
    ws.max_frame_size(DEVTOOLS_MESSAGE_BYTES)
        .max_message_size(DEVTOOLS_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            hub.send_to_host(json!({ "type": "attach", "session": session, "pageId": page_id }));
            let (mut socket_tx, mut socket_rx) = socket.split();
            let write = async {
                while let Some(event) = events.recv().await {
                    let message = match event {
                        SessionEvent::Message(text) => Message::Text(text.into()),
                        SessionEvent::Closed(reason) => {
                            Message::Close(Some(axum::extract::ws::CloseFrame {
                                code: 1001,
                                // A close reason is limited to 123 bytes.
                                reason: reason.chars().take(100).collect::<String>().into(),
                            }))
                        }
                    };
                    let closing = matches!(message, Message::Close(_));
                    if socket_tx.send(message).await.is_err() || closing {
                        break;
                    }
                }
            };
            let read = async {
                while let Some(Ok(message)) = socket_rx.next().await {
                    match message {
                        Message::Text(text) => {
                            hub.send_to_host(json!({
                                "type": "send",
                                "session": session,
                                "message": text.as_str(),
                            }));
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
            };
            tokio::select! {
                _ = shutdown.recv() => {},
                _ = write => {},
                _ = read => {},
            }
            hub.close_session(session);
        })
}
