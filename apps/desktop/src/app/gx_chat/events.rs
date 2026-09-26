//! The renderer's calls and gxserver's frames, turned into the core's one enum.
//!
//! `apps/desktop/src/app/native_chat/state.rs` pushes three shapes at the host: `start` (the view
//! opening), `action` (a user gesture) and `resolve` (an answer to a `rpc` request). `tick` is the
//! host's own. A gxserver frame arrives from the chat socket (`transport.rs`) already parsed.

use ghostex_gx_chat_core::{
    ChatFrame, Event, RpcOutcome, StartConfig, UserAction,
    protocol::{ChatAppendedFrame, ChatSnapshotFrame, ChatStateFrame},
};
use serde_json::Value;

/// The events one renderer call becomes. Empty for a call this build does not model, which the
/// caller counts as a refusal rather than a silent no-op.
pub(super) fn events_for(method: &str, arguments: &[Value]) -> Vec<Event> {
    match method {
        "start" => arguments
            .first()
            .and_then(|value| serde_json::from_value::<StartConfig>(value.clone()).ok())
            .map(|config| vec![Event::Start(Box::new(config))])
            .unwrap_or_default(),
        "action" => arguments
            .first()
            .and_then(|value| serde_json::from_value::<UserAction>(value.clone()).ok())
            .map(|action| vec![Event::Action(Box::new(action))])
            .unwrap_or_default(),
        "tick" => vec![Event::Tick],
        "resolve" => Vec::new(),
        _ => Vec::new(),
    }
}

/// The `resolve` the view sends after a `rpc` request settles.
///
/// `arguments` is `[id, result, error]`, and the id is the one the core allocated: the view echoes
/// `request["id"]` back verbatim, so unlike a replay there is no order matching to do here. A
/// refusal is an answer like any other: it settles the request as `RpcOutcome::Err`, with or
/// without a code, because a request the core never hears back about keeps its lane in flight for
/// ever. The only call that yields no event is one with no usable id, and the caller counts it.
pub(super) fn resolved(arguments: &[Value]) -> Option<Event> {
    let request_id = arguments.first().and_then(request_id)?;
    let result = arguments.get(1).cloned().unwrap_or(Value::Null);
    let error = arguments.get(2).filter(|value| !value.is_null());
    let outcome = match error {
        Some(error) => RpcOutcome::Err {
            code: error
                .get("code")
                .and_then(Value::as_str)
                .map(str::to_string),
            message: error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("Request failed.")
                .to_string(),
            endpoint: error
                .get("endpoint")
                .and_then(Value::as_str)
                .map(str::to_string),
        },
        None => RpcOutcome::Ok { result },
    };
    Some(Event::RpcSettled {
        request_id,
        outcome: Box::new(outcome),
    })
}

/// A request id as the view echoes it: the number the core allocated. A whole number that crossed
/// as a float or a string is still that id.
fn request_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_f64()
                .filter(|id| id.fract() == 0.0 && *id >= 0.0 && *id < 2f64.powi(53))
                .map(|id| id as u64)
        })
        .or_else(|| value.as_str().and_then(|id| id.parse::<u64>().ok()))
}

/// One of the four chat frame types, already parsed.
///
/// `packages/gx-chat-client` has run `socket.ts`'s field checks before a frame gets here; the
/// deserializer is the second gate, and a frame that does not parse as its type is dropped.
pub(super) fn frame(value: Value) -> Option<Event> {
    let frame = match value.get("type").and_then(Value::as_str) {
        Some("sessionChatSnapshot") => serde_json::from_value::<ChatSnapshotFrame>(value)
            .ok()
            .map(|frame| ChatFrame::Snapshot(Box::new(frame))),
        Some("sessionChatReplaced") => serde_json::from_value::<ChatSnapshotFrame>(value)
            .ok()
            .map(|frame| ChatFrame::Replaced(Box::new(frame))),
        Some("sessionChatAppended") => serde_json::from_value::<ChatAppendedFrame>(value)
            .ok()
            .map(|frame| ChatFrame::Appended(Box::new(frame))),
        Some("sessionChatState") => serde_json::from_value::<ChatStateFrame>(value)
            .ok()
            .map(|frame| ChatFrame::State(Box::new(frame))),
        _ => None,
    };
    frame.map(|frame| Event::Frame(Box::new(frame)))
}
