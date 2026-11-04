use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::ghostex_cli::rpc::{CliError, CliResult};

use super::*;

// ---------------------------------------------------------------------------
// CDP client (GhostexCdpClient over tungstenite instead of SimpleWebSocket)
// ---------------------------------------------------------------------------

pub(super) struct CdpClient {
    socket: tungstenite::WebSocket<TcpStream>,
    next_id: u64,
    pub(super) capture_enabled: bool,
    pub(super) closed: bool,
    timeout_ms: u64,
    /// CDP events (messages without an id) collected while waiting on calls;
    /// drained by the caller into per-page capture buffers.
    pub(super) events: Vec<Value>,
}

impl CdpClient {
    pub(super) fn connect(
        ws_url: &str,
        timeout_ms: u64,
        bearer: Option<&str>,
    ) -> CliResult<CdpClient> {
        let (host, port) = parse_ws_host_port(ws_url)
            .ok_or_else(|| CliError::Other("Invalid WebSocket handshake".to_string()))?;
        let address = std::net::ToSocketAddrs::to_socket_addrs(&(host.as_str(), port))
            .map_err(|error| CliError::Other(error.to_string()))?
            .next()
            .ok_or_else(|| CliError::Other("Invalid WebSocket handshake".to_string()))?;
        let stream =
            TcpStream::connect_timeout(&address, Duration::from_secs(5)).map_err(|error| {
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) {
                    CliError::Other("Timed out opening CDP WebSocket".to_string())
                } else {
                    CliError::Other(error.to_string())
                }
            })?;
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut request = tungstenite::client::IntoClientRequest::into_client_request(ws_url)
            .map_err(|error| CliError::Other(error.to_string()))?;
        if let Some(token) = bearer {
            let value = tungstenite::http::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|error| CliError::Other(error.to_string()))?;
            request.headers_mut().insert("authorization", value);
        }
        let (socket, _response) = tungstenite::client::client(request, stream)
            .map_err(|error| CliError::Other(error.to_string()))?;
        Ok(CdpClient {
            socket,
            next_id: 1,
            capture_enabled: false,
            closed: false,
            timeout_ms,
            events: Vec::new(),
        })
    }

    pub(super) fn call(&mut self, method: &str, params: Value) -> CliResult<Value> {
        if self.closed {
            return Err(CliError::Other("CDP connection is closed".to_string()));
        }
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({ "id": id, "method": method, "params": params }).to_string();
        if let Err(error) = self.socket.send(tungstenite::Message::Text(payload.into())) {
            self.closed = true;
            return Err(CliError::Other(error.to_string()));
        }
        let timeout_ms = if self.timeout_ms > 0 {
            self.timeout_ms
        } else {
            10_000
        };
        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(CliError::Other(format!(
                    "Timed out waiting for CDP method {method}"
                )));
            }
            let remaining = deadline - now;
            let _ = self
                .socket
                .get_mut()
                .set_read_timeout(Some(remaining.max(Duration::from_millis(1))));
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    let Ok(parsed) = serde_json::from_str::<Value>(&text) else {
                        continue;
                    };
                    match defined(parsed.get("id")).and_then(Value::as_u64) {
                        Some(message_id) if message_id == id => {
                            if let Some(error) =
                                parsed.get("error").filter(|error| js_truthy(error))
                            {
                                let message = error
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                                    .unwrap_or_else(|| {
                                        serde_json::to_string(error)
                                            .unwrap_or_else(|_| "null".to_string())
                                    });
                                return Err(CliError::Other(message));
                            }
                            return Ok(defined(parsed.get("result"))
                                .cloned()
                                .unwrap_or_else(|| json!({})));
                        }
                        Some(_) => {}
                        None => self.events.push(parsed),
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    self.closed = true;
                    return Err(CliError::Other("CDP connection closed".to_string()));
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                    ) =>
                {
                    return Err(CliError::Other(format!(
                        "Timed out waiting for CDP method {method}"
                    )));
                }
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    self.closed = true;
                    return Err(CliError::Other("CDP connection closed".to_string()));
                }
                Err(error) => {
                    self.closed = true;
                    return Err(CliError::Other(error.to_string()));
                }
            }
        }
    }

    /// Drain already-buffered CDP events without blocking.
    pub(super) fn pump_events(&mut self) {
        if self.closed {
            return;
        }
        if self.socket.get_mut().set_nonblocking(true).is_err() {
            return;
        }
        loop {
            match self.socket.read() {
                Ok(tungstenite::Message::Text(text)) => {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&text) {
                        if defined(parsed.get("id")).is_none() {
                            self.events.push(parsed);
                        }
                    }
                }
                Ok(tungstenite::Message::Close(_)) => {
                    self.closed = true;
                    break;
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(error))
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(_) => {
                    self.closed = true;
                    break;
                }
            }
        }
        let _ = self.socket.get_mut().set_nonblocking(false);
    }
}

/// Parse host/port from a `ws://host:port/path` DevTools URL (JS used
/// `new URL(...)` with a port-80 default).
pub(super) fn parse_ws_host_port(ws_url: &str) -> Option<(String, u16)> {
    let rest = ws_url.strip_prefix("ws://")?;
    let host_port = rest.split(['/', '?']).next().unwrap_or(rest);
    match host_port.split_once(':') {
        Some((host, port)) => Some((host.to_string(), port.parse::<u16>().ok()?)),
        None => Some((host_port.to_string(), 80)),
    }
}
