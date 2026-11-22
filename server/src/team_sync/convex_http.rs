//! One-shot calls to the team's Convex functions over Convex's HTTP API (`/api/query`,
//! `/api/mutation`, `/api/action`). Blocking: call from `spawn_blocking` or the CLI.
//!
//! CDXC:TeamSync 2026-10-09 WHY:
//! Request/response calls (join, invite, status, a ticket's Slack threads) go over plain HTTP from
//! the blocking route workers, like the Linear calls; only the always-on command subscription
//! holds a WebSocket (crate::team_sync::runtime).

use std::time::Duration;

use serde_json::{json, Value};

const CONVEX_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug)]
pub(crate) enum ConvexCallKind {
    Query,
    Mutation,
    Action,
}

impl ConvexCallKind {
    fn route(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Action => "action",
        }
    }
}

/// Runs `path` (`module:function`) with `args` and returns its value, or the function's error
/// message (a `ConvexError`'s text when the function threw one).
pub(crate) fn call_convex(
    deployment_url: &str,
    kind: ConvexCallKind,
    path: &str,
    args: Value,
) -> Result<Value, String> {
    let url = format!("{deployment_url}/api/{}", kind.route());
    let body = json!({ "path": path, "args": args, "format": "json" });
    let response = match crate::http_agent::answering_agent()
        .post(&url)
        .config()
        .timeout_global(Some(CONVEX_TIMEOUT))
        .build()
        .send_json(body)
    {
        Ok(response) if response.status().as_u16() < 400 => response,
        Ok(mut response) => {
            let code = response.status().as_u16();
            let text = response.body_mut().read_to_string().unwrap_or_default();
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|value| {
                    value
                        .get("errorMessage")
                        .or_else(|| value.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or(text);
            return Err(format!("Convex answered {code}: {}", message.trim()));
        }
        Err(error) => {
            return Err(format!(
                "Could not reach the team's Convex project: {error}"
            ))
        }
    };
    let value: Value = crate::http_agent::read_json(response)
        .map_err(|error| format!("Convex sent an unreadable answer: {error}"))?;
    match value.get("status").and_then(Value::as_str) {
        Some("success") => Ok(value.get("value").cloned().unwrap_or(Value::Null)),
        _ => Err(convex_error_text(&value)),
    }
}

/// The user-facing text of a failed call: the `ConvexError` data when it is a string, else the
/// first line of the message without Convex's request-id prefix.
fn convex_error_text(value: &Value) -> String {
    if let Some(data) = value.get("errorData").and_then(Value::as_str) {
        return data.to_string();
    }
    let message = value
        .get("errorMessage")
        .and_then(Value::as_str)
        .unwrap_or("The team's Convex project returned an error.");
    let message = message
        .split_once("] ")
        .map(|(_, rest)| rest)
        .unwrap_or(message);
    message.lines().next().unwrap_or(message).trim().to_string()
}
