//! The voice agent's tools call gxserver's own HTTP routes on the loopback listener, the same
//! requests `ghostex send-session-chat-message`, `answer-session-chat-prompt` and `create-agent`
//! make.
//!
//! CDXC:VoiceMode 2026-10-20 WHY:
//! Going through the router rather than calling handlers directly keeps every route-level rule the CLI and the chat get (the send-once ledger on sends and answers, endpoint gates, request logging), and keeps the tools working when those handlers change shape.

//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (#495 follow-up, 2A): the voice agent's tools act with the token of the client that opened
//! the call (the desktop's owner session, a phone's device token, a session's own token, or the
//! token file), not always with the token file, so they can do neither more nor less than that
//! client. The owner's devices answer prompts and type into sessions without asking (as their own
//! clicks do), which the token file no longer may. When gxserver asks the owner to approve a call,
//! the tool says it is waiting and `approvals.rs` repeats it once the user approved.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghostex_gx_protocol::approval::{APPROVAL_ID_PARAM, APPROVAL_REQUIRED_ERROR};
use serde_json::{json, Value};

use crate::constants::GXSERVER_PROTOCOL_VERSION;
use crate::server::AppState;

/// How an error starts when the request got no answer at all.
pub(super) const NO_ANSWER: &str = "gxserver did not answer";

tokio::task_local! {
    static CALLER: ToolCaller;
}

/// One tool call of a voice conversation: the token its requests send, the approvals the user
/// gave it, and the calls that already went through, so repeating the tool after an approval
/// does each of those only once.
#[derive(Clone, Default)]
pub(super) struct ToolCaller {
    token: Option<String>,
    calls: Arc<Mutex<Calls>>,
}

#[derive(Default)]
struct Calls {
    approved: HashMap<String, String>,
    done: HashMap<String, Value>,
    pending: Option<PendingApproval>,
}

/// A call gxserver answered with `approvalRequired`.
#[derive(Clone, Debug)]
pub(super) struct PendingApproval {
    pub(super) id: String,
    pub(super) summary: String,
    pub(super) expires_at_ms: u64,
    /// Where the request is read: this gxserver's, or the relay's to the computer that asked.
    pub(super) read_path: String,
    pub(super) project_id: String,
    pub(super) session_id: String,
    key: String,
}

impl ToolCaller {
    /// `token`: the client's; `None` keeps the token file (gxserver's own reads).
    pub(super) fn new(token: Option<String>) -> Self {
        Self {
            token,
            calls: Arc::default(),
        }
    }

    /// The same token with nothing remembered: for reading an approval while waiting.
    pub(super) fn fresh(&self) -> Self {
        Self::new(self.token.clone())
    }

    fn calls(&self) -> std::sync::MutexGuard<'_, Calls> {
        self.calls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The approval that came up in this run, if one did.
    pub(super) fn take_pending(&self) -> Option<PendingApproval> {
        self.calls().pending.take()
    }

    /// The user approved `pending`: the next run sends its call with the approval.
    pub(super) fn approve(&self, pending: &PendingApproval) {
        self.calls()
            .approved
            .insert(pending.key.clone(), pending.id.clone());
    }
}

/// Runs `run` with `caller` as the token and memory of every request it makes.
pub(super) async fn with_caller<F: std::future::Future>(caller: ToolCaller, run: F) -> F::Output {
    CALLER.scope(caller, run).await
}

/// POSTs `params` to `path` and answers the response's `result`, or the server's error message.
/// Inside a tool call ([`with_caller`]) it sends that call's token, and an `approvalRequired`
/// answer is kept for `approvals.rs`.
pub(super) async fn call(
    state: &Arc<AppState>,
    path: &str,
    params: Value,
    timeout: Duration,
) -> Result<Value, String> {
    let caller = CALLER.try_with(Clone::clone).ok();
    let key = format!("{path}\n{params}");
    let mut params = params;
    if let Some(caller) = &caller {
        let calls = caller.calls();
        if let Some(result) = calls.done.get(&key) {
            return Ok(result.clone());
        }
        if let (Some(id), Some(object)) = (calls.approved.get(&key), params.as_object_mut()) {
            object.insert(APPROVAL_ID_PARAM.to_string(), Value::String(id.clone()));
        }
    }
    let token = caller
        .as_ref()
        .and_then(|caller| caller.token.clone())
        .unwrap_or_else(|| state.auth_token.clone());
    let answer = post(state, path, &params, token, timeout).await;
    let Some(caller) = caller else {
        return answer.map_err(|(message, _)| message);
    };
    match answer {
        Ok(result) => {
            caller.calls().done.insert(key, result.clone());
            Ok(result)
        }
        Err((message, Some(body)))
            if body.get("error").and_then(Value::as_str) == Some(APPROVAL_REQUIRED_ERROR) =>
        {
            let approval = &body["approval"];
            let Some(id) = approval.get("id").and_then(Value::as_str) else {
                return Err(message);
            };
            let summary = approval
                .get("summary")
                .and_then(Value::as_str)
                .unwrap_or("this change")
                .to_string();
            let prefix = path.rfind("/api/").map_or("", |at| &path[..at]);
            let text = |name: &str| {
                params
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            caller.calls().pending = Some(PendingApproval {
                id: id.to_string(),
                summary: summary.clone(),
                expires_at_ms: approval
                    .get("expiresAtMs")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
                read_path: format!("{prefix}{}", crate::security::APPROVALS_READ_ENDPOINT),
                project_id: text("projectId"),
                session_id: text("sessionId"),
                key,
            });
            Err(format!(
                "This needs the user's approval on their computer or phone: {summary}."
            ))
        }
        Err((message, _)) => Err(message),
    }
}

/// The request itself: the result, or the error message with the answer's body when there is one.
async fn post(
    state: &Arc<AppState>,
    path: &str,
    params: &Value,
    token: String,
    timeout: Duration,
) -> Result<Value, (String, Option<Value>)> {
    let host = match state.config.listeners.local.host.as_str() {
        "" | "0.0.0.0" | "::" => "127.0.0.1".to_string(),
        host => host.to_string(),
    };
    let url = format!("http://{host}:{}{path}", state.config.listeners.local.port);
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
            Err(error) => return Err((format!("{NO_ANSWER}: {error}"), None)),
        };
        let body: Value = crate::http_agent::read_json(response)
            .map_err(|error| (format!("gxserver sent an unreadable answer: {error}"), None))?;
        if body.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(body.get("result").cloned().unwrap_or(Value::Null))
        } else {
            let message = body
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| body.get("error").and_then(Value::as_str))
                .unwrap_or("The request failed.")
                .to_string();
            Err((message, Some(body)))
        }
    })
    .await
    .map_err(|error| (error.to_string(), None))?
}
