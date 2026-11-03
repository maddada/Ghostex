//! The voice agent's tools call gxserver's own HTTP routes on the loopback listener, the same
//! requests `ghostex send-session-chat-message`, `answer-session-chat-prompt` and `create-agent`
//! make.
//!
//! CDXC:VoiceMode 2026-10-20 WHY:
//! Going through the router rather than calling handlers directly keeps every route-level rule the CLI and the chat get (the send-once ledger on sends and answers, endpoint gates, request logging), and keeps the tools working when those handlers change shape.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::AppState;

/// How an error starts when the request got no answer at all.
pub(super) const NO_ANSWER: &str = "gxserver did not answer";

/// POSTs `params` to `path` and answers the response's `result`, or the server's error message.
pub(super) async fn call(
    state: &Arc<AppState>,
    path: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let host = match state.config.listeners.local.host.as_str() {
        "" | "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        host => host.to_string(),
    };
    let url = format!("http://{host}:{}{path}", state.config.listeners.local.port);
    let token = state.auth_token.clone();
    let body = json!({"params": params, "protocolVersion": GXSERVER_PROTOCOL_VERSION});
    tokio::task::spawn_blocking(move || {
        let outcome = crate::http_agent::answering_agent()
            .post(&url)
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .header(
                "x-gxserver-protocol-version",
                GXSERVER_PROTOCOL_VERSION.to_string(),
            )
            .config()
            .timeout_global(Some(timeout))
            .build()
            .send(body.to_string());
        let response = match outcome {
            Ok(response) => response,
            Err(error) => return Err(format!("{NO_ANSWER}: {error}")),
        };
        let body: Value = crate::http_agent::read_json(response)
            .map_err(|error| format!("gxserver sent an unreadable answer: {error}"))?;
        if body.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(body.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(body
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| body.get("error").and_then(Value::as_str))
                .unwrap_or("The request failed.")
                .to_string())
        }
    })
    .await
    .map_err(|error| error.to_string())?
}
