//! Long tool outputs cut for the phone (`toolOutputCut`): in its chat socket's frames, in its
//! `readSessionChat` answers, and read back whole by `readSessionChatToolOutput`.
//!
//! CDXC:SessionChat 2026-11-12 DECISION:
//! The user chose "Head and tail plus counts" (Q2729): a client that asks (`toolOutputs=cut` on its chat socket, `cutToolOutputs` on a read; the phone does, behind the `toolOutputCut` capability) gets every long tool output as the chat core cuts it (`ghostex_gx_chat_core::transcript::output_cut`: the head an open row shows, the facts its labels read, the tail), and every other client gets them whole. Each full text cut here is kept in [`KEPT_BYTES_MAX`] of memory so the phone's Copy reads it back in one round trip; one no longer kept is read from the transcript again.
//! SEE-ALSO: packages/gx-chat-core/src/transcript/output_cut.rs, apps/mobile/app/src/chat/rust/machine-link.ts.

use std::collections::VecDeque;
use std::sync::Mutex;

use axum::body::Body;
use axum::http::{header, HeaderValue};
use ghostex_gx_chat_core::transcript::output_cut;
use serde_json::{json, Value};

use crate::server::{AppState, RoutedResponse};

/// The full texts kept for `readSessionChatToolOutput`.
const KEPT_BYTES_MAX: usize = 32 * 1024 * 1024;

struct Kept {
    entries: VecDeque<(String, String)>,
    bytes: usize,
}

static KEPT: Mutex<Kept> = Mutex::new(Kept {
    entries: VecDeque::new(),
    bytes: 0,
});

fn key(project_id: &str, session_id: &str, call_id: &str) -> String {
    format!("{project_id}\u{1f}{session_id}\u{1f}{call_id}")
}

fn keep(project_id: &str, session_id: &str, call_id: &str, output: &str) {
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = key(project_id, session_id, call_id);
    // The latest text wins: a running call's result is sent again as it grows.
    if let Some(at) = kept.entries.iter().position(|(known, _)| known == &key) {
        if kept.entries[at].1 == output {
            return;
        }
        if let Some((_, old)) = kept.entries.remove(at) {
            kept.bytes -= old.len();
        }
    }
    kept.bytes += output.len();
    kept.entries.push_back((key, output.to_string()));
    while kept.bytes > KEPT_BYTES_MAX {
        let Some((_, dropped)) = kept.entries.pop_front() else {
            break;
        };
        kept.bytes -= dropped.len();
    }
}

fn kept(project_id: &str, session_id: &str, call_id: &str) -> Option<String> {
    let kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let key = key(project_id, session_id, call_id);
    kept.entries
        .iter()
        .rev()
        .find(|(known, _)| known == &key)
        .map(|(_, output)| output.clone())
}

/// Cuts the long tool outputs of a chat frame or read result for `project_id`/`session_id`, keeping
/// each full text. Bytes saved.
pub(crate) fn cut_value(value: &mut Value, project_id: &str, session_id: &str) -> usize {
    output_cut::cut_messages(value, &mut |call_id, output| {
        keep(project_id, session_id, call_id, output);
    })
}

/// A chat frame for a socket that asked for cut outputs: the same frame when nothing in it is long.
pub(crate) fn cut_frame(event: crate::events::EventPayload) -> crate::events::EventPayload {
    let kind = event.get("type").and_then(Value::as_str);
    if !matches!(
        kind,
        Some("sessionChatSnapshot" | "sessionChatReplaced" | "sessionChatAppended")
    ) {
        return event;
    }
    let (Some(project_id), Some(session_id)) = (
        event.get("projectId").and_then(Value::as_str),
        event.get("sessionId").and_then(Value::as_str),
    ) else {
        return event;
    };
    let has_long = event
        .get("messages")
        .and_then(Value::as_array)
        .is_some_and(|messages| {
            messages.iter().any(|message| {
                message
                    .get("blocks")
                    .and_then(Value::as_array)
                    .is_some_and(|blocks| {
                        blocks.iter().any(|block| {
                            block
                                .get("output")
                                .and_then(Value::as_str)
                                .is_some_and(output_cut::may_cut)
                        })
                    })
            })
        });
    if !has_long {
        return event;
    }
    let (project_id, session_id) = (project_id.to_string(), session_id.to_string());
    let mut value = (*event).clone();
    cut_value(&mut value, &project_id, &session_id);
    value.into()
}

/// A `readSessionChat` answer with its long outputs cut, for a read that asked (`cutToolOutputs`).
pub(crate) async fn cut_read_answer(
    routed: RoutedResponse,
    project_id: &str,
    session_id: &str,
) -> RoutedResponse {
    let (mut parts, body) = routed.response.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, usize::MAX).await else {
        return RoutedResponse {
            endpoint_path: routed.endpoint_path,
            response: axum::http::Response::from_parts(parts, Body::empty()),
        };
    };
    let Ok(mut answer) = serde_json::from_slice::<Value>(&bytes) else {
        return RoutedResponse {
            endpoint_path: routed.endpoint_path,
            response: axum::http::Response::from_parts(parts, Body::from(bytes)),
        };
    };
    let saved = answer
        .get_mut("result")
        .map_or(0, |result| cut_value(result, project_id, session_id));
    if saved == 0 {
        return RoutedResponse {
            endpoint_path: routed.endpoint_path,
            response: axum::http::Response::from_parts(parts, Body::from(bytes)),
        };
    }
    let text = answer.to_string();
    parts
        .headers
        .insert(header::CONTENT_LENGTH, HeaderValue::from(text.len()));
    RoutedResponse {
        endpoint_path: routed.endpoint_path,
        response: axum::http::Response::from_parts(parts, Body::from(text)),
    }
}

fn param<'a>(body: &'a Value, name: &str) -> &'a str {
    body.get("params")
        .and_then(|params| params.get(name))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// `readSessionChatToolOutput {projectId, sessionId, callId}`: `{output}`, the whole text of a tool
/// output cut for this client. Kept texts answer at once; one no longer kept is looked for in the
/// session's transcript.
pub(crate) async fn handle_read_tool_output_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let (project_id, session_id, call_id) = (
        param(body, "projectId"),
        param(body, "sessionId"),
        param(body, "callId"),
    );
    if project_id.is_empty() || session_id.is_empty() || call_id.is_empty() {
        return crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "invalidParams",
                message: "readSessionChatToolOutput requires projectId, sessionId and callId."
                    .to_string(),
            },
        );
    }
    let output = match kept(project_id, session_id, call_id) {
        Some(output) => Some(output),
        None => read_from_transcript(state, project_id, session_id, call_id).await,
    };
    match output {
        Some(output) => crate::server::routed_json(
            Some(endpoint_path),
            axum::http::StatusCode::OK,
            crate::protocol::rpc_success(request_id, json!({ "output": output })),
        ),
        None => crate::server::domain_error_response(
            endpoint_path,
            request_id,
            crate::domain::DomainStateError {
                code: "notFound",
                message: "That tool output is no longer in the transcript.".to_string(),
            },
        ),
    }
}

async fn read_from_transcript(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    call_id: &str,
) -> Option<String> {
    let body = json!({
        "protocolVersion": 1,
        "params": {
            "projectId": project_id,
            "sessionId": session_id,
            "limit": crate::session_chat::SESSION_CHAT_MAX_LIMIT,
        },
    });
    let routed = crate::session_chat_read::handle_read_session_chat_http(
        state,
        "/api/readSessionChat".to_string(),
        String::new(),
        &body,
    )
    .await;
    let bytes = axum::body::to_bytes(routed.response.into_body(), usize::MAX)
        .await
        .ok()?;
    let answer: Value = serde_json::from_slice(&bytes).ok()?;
    answer
        .get("result")?
        .get("messages")?
        .as_array()?
        .iter()
        .filter_map(|message| message.get("blocks")?.as_array())
        .flatten()
        .find(|block| block.get("callId").and_then(Value::as_str) == Some(call_id))
        .and_then(|block| block.get("output")?.as_str().map(str::to_string))
}
