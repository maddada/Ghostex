//! A blocking MCP client over Streamable HTTP, the transport `@modelcontextprotocol/sdk`'s
//! `StreamableHTTPClientTransport` speaks: every JSON-RPC message is a POST, the answer is JSON or
//! a `text/event-stream`, and the `Mcp-Session-Id` from `initialize` rides on every later request.
//! Composio's hosted server and a workspace's Executor (`/mcp`) both use it; `server` is the name
//! its messages give the other side.

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{json, Value};
use ureq::http::Response;
use ureq::Body;

use super::http::{agent, send, HttpError};
use super::oauth::{parse_bearer_challenge, BearerChallenge, MCP_PROTOCOL_VERSION};

/// Versions the SDK accepts back from `initialize` (`SUPPORTED_PROTOCOL_VERSIONS`).
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &[
    "2025-11-25",
    "2025-06-18",
    "2025-03-26",
    "2024-11-05",
    "2024-10-07",
];

/// A live MCP session.
#[derive(Clone, Debug)]
pub(crate) struct McpSession {
    pub(crate) session_id: Option<String>,
    pub(crate) protocol_version: String,
}

#[derive(Debug)]
pub(crate) enum McpError {
    /// 401: sign in or renew the token. Carries the `WWW-Authenticate` hints.
    Unauthorized(BearerChallenge),
    /// The server no longer knows the session (404, or a 400 that names the session): start a new one.
    SessionGone,
    /// No connection was made (DNS, connect, TLS handshake), so nothing ran. The message names the
    /// server.
    Unreached(String),
    /// The server answered with an error status, or the answer broke off.
    Failed(String),
    /// A JSON-RPC error object.
    Rpc(String),
}

impl McpError {
    pub(crate) fn message(&self) -> String {
        match self {
            McpError::Unauthorized(_) => "The server asked to sign in again.".to_string(),
            McpError::SessionGone => "The server ended the session.".to_string(),
            McpError::Unreached(message) | McpError::Failed(message) | McpError::Rpc(message) => {
                message.clone()
            }
        }
    }
}

fn next_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn post(
    server: &str,
    url: &str,
    access_token: Option<&str>,
    session: Option<&McpSession>,
    message: &Value,
    timeout: Duration,
) -> Result<Response<Body>, McpError> {
    let mut request = agent()
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .config()
        .timeout_global(Some(timeout))
        .build();
    if let Some(token) = access_token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(session) = session {
        if let Some(id) = &session.session_id {
            request = request.header("mcp-session-id", id);
        }
        request = request.header("mcp-protocol-version", &session.protocol_version);
    }
    match send(request.send(message.to_string())) {
        Ok(response) => Ok(response),
        Err(HttpError::Status {
            status: 401,
            www_authenticate,
            ..
        }) => Err(McpError::Unauthorized(parse_bearer_challenge(
            www_authenticate.as_deref(),
        ))),
        Err(HttpError::Status { status: 404, .. })
            if session.is_some_and(|s| s.session_id.is_some()) =>
        {
            Err(McpError::SessionGone)
        }
        Err(HttpError::Status {
            status: 400, body, ..
        }) if session.is_some_and(|s| s.session_id.is_some())
            && body.to_ascii_lowercase().contains("session") =>
        {
            Err(McpError::SessionGone)
        }
        Err(error @ HttpError::Status { .. }) => Err(McpError::Failed(format!(
            "{server} answered: {}",
            error.message()
        ))),
        Err(HttpError::Unreached(message)) => Err(McpError::Unreached(format!(
            "Couldn't reach {server}: {message}"
        ))),
        // The request may have reached the server before the connection broke: it may have run.
        Err(HttpError::Transport(message)) => Err(McpError::Failed(format!(
            "The connection to {server} broke: {message}"
        ))),
    }
}

/// Reads the JSON-RPC response with `id` from a JSON body or an SSE stream.
fn read_response(server: &str, mut response: Response<Body>, id: u64) -> Result<Value, McpError> {
    let content_type = response
        .body()
        .mime_type()
        .unwrap_or("text/plain")
        .to_ascii_lowercase();
    if content_type == "text/event-stream" {
        return read_sse_response(server, response, id);
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| McpError::Failed(format!("{server}'s answer broke off: {error}")))?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| McpError::Failed(format!("{server} sent an unreadable answer.")))?;
    let messages = match value {
        Value::Array(messages) => messages,
        other => vec![other],
    };
    messages
        .into_iter()
        .find_map(|message| take_matching_response(server, message, id))
        .unwrap_or_else(|| {
            Err(McpError::Failed(format!(
                "{server} didn't answer the request."
            )))
        })
}

fn take_matching_response(
    server: &str,
    message: Value,
    id: u64,
) -> Option<Result<Value, McpError>> {
    if message.get("id").and_then(Value::as_u64) != Some(id) || message.get("method").is_some() {
        return None;
    }
    if let Some(error) = message.get("error") {
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{server} refused the request."));
        return Some(Err(McpError::Rpc(text)));
    }
    Some(Ok(message.get("result").cloned().unwrap_or(Value::Null)))
}

/// SSE framing: `data:` lines join with newlines, a blank line ends the event, `:` lines are
/// comments. Only `message` events (or unnamed ones) carry JSON-RPC.
fn read_sse_response(server: &str, response: Response<Body>, id: u64) -> Result<Value, McpError> {
    let mut reader = BufReader::new(response.into_body().into_reader());
    let mut data = String::new();
    let mut event = String::new();
    let mut line = String::new();
    loop {
        line.clear();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| McpError::Failed(format!("{server}'s answer broke off: {error}")))?;
        let ended = read == 0;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if ended || trimmed.is_empty() {
            if !data.is_empty() && (event.is_empty() || event == "message") {
                if let Ok(message) = serde_json::from_str::<Value>(&data) {
                    let messages = match message {
                        Value::Array(messages) => messages,
                        other => vec![other],
                    };
                    for message in messages {
                        if let Some(result) = take_matching_response(server, message, id) {
                            return result;
                        }
                    }
                }
            }
            data.clear();
            event.clear();
            if ended {
                return Err(McpError::Failed(format!(
                    "{server} closed the connection before answering."
                )));
            }
            continue;
        }
        if trimmed.starts_with(':') {
            continue;
        }
        let (field, value) = match trimmed.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (trimmed, ""),
        };
        match field {
            "data" => {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(value);
            }
            "event" => event = value.to_string(),
            _ => {}
        }
    }
}

/// One JSON-RPC request in an existing session.
pub(crate) fn request(
    server: &str,
    url: &str,
    access_token: Option<&str>,
    session: &McpSession,
    method: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, McpError> {
    let id = next_id();
    let message = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    let response = post(server, url, access_token, Some(session), &message, timeout)?;
    read_response(server, response, id)
}

/// `initialize` then `notifications/initialized` (the SDK's `Client.connect`).
pub(crate) fn initialize(
    server: &str,
    url: &str,
    access_token: Option<&str>,
    client_name: &str,
) -> Result<McpSession, McpError> {
    let id = next_id();
    let message = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": client_name, "version": "1.0.0" },
        },
    });
    let response = post(
        server,
        url,
        access_token,
        None,
        &message,
        Duration::from_secs(60),
    )?;
    let session_id = crate::http_agent::header(&response, "mcp-session-id").map(str::to_string);
    let result = read_response(server, response, id)?;
    let protocol_version = result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if !SUPPORTED_PROTOCOL_VERSIONS.contains(&protocol_version.as_str()) {
        return Err(McpError::Failed(format!(
            "{server} speaks a newer version of MCP ({protocol_version}). Update Ghostex."
        )));
    }
    let session = McpSession {
        session_id,
        protocol_version,
    };
    let notification = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    post(
        server,
        url,
        access_token,
        Some(&session),
        &notification,
        Duration::from_secs(30),
    )?;
    Ok(session)
}
