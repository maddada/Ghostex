//! What travels on the chat socket, whichever target opens it.

use std::collections::BTreeMap;

use ghostex_gx_protocol::{
    ClientMessage, GXSERVER_EVENTS_PATH, GXSERVER_EVENTS_STREAM_SESSION_CHAT,
    GXSERVER_PROTOCOL_VERSION,
};
use serde_json::Value;

/// Delay before reconnect attempt `n`; the last entry repeats. `socket.ts`'s ladder, unchanged.
pub const RECONNECT_DELAYS_MS: [u64; 5] = [500, 1_000, 2_000, 5_000, 10_000];

/// The four chat frame types a chat-only stream carries.
const FRAME_TYPES: [&str; 4] = [
    "sessionChatSnapshot",
    "sessionChatReplaced",
    "sessionChatAppended",
    "sessionChatState",
];

/// The frame gxserver pushes when the published model lineup changes, and once as a socket opens.
const MODEL_CATALOG_CHANGED: &str = "agentModelCatalogChanged";

/// Where one machine's gxserver is: `http://<host>:<port>` and its bearer token.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Endpoint {
    pub base_url: String,
    pub auth_token: String,
}

impl Endpoint {
    pub fn new(base_url: impl Into<String>, auth_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            auth_token: auth_token.into(),
        }
    }

    /// Both halves present. A socket is never opened without them.
    pub fn usable(&self) -> bool {
        !self.base_url.is_empty() && !self.auth_token.is_empty()
    }
}

/// Written by hand so the token can never reach a log through `{:?}`.
impl std::fmt::Debug for Endpoint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Endpoint")
            .field("base_url", &self.base_url)
            .field("auth_token", &"[redacted]")
            .finish()
    }
}

/// What the socket hands its host.
#[derive(Clone, Debug)]
pub enum Inbound {
    /// A chat frame for a conversation this machine's socket follows, already parsed.
    Frame {
        machine_id: String,
        project_id: String,
        session_id: String,
        frame: Value,
    },
    /// gxserver's published model catalog, as it sent it.
    ModelCatalog { machine_id: String, catalog: Value },
}

/// A followed conversation: `(projectId, sessionId)`, the pair gxserver routes by.
pub(crate) type StreamKey = (String, String);

/// Every conversation one machine's socket follows, with the window each asked for.
pub(crate) type Followers = BTreeMap<StreamKey, u32>;

/// `subscribeSessionChat`. The reply is a fresh `sessionChatSnapshot`; a repeat on the same socket
/// is not a new subscriber, so it only re-sends the snapshot and can raise the follower's window.
pub(crate) fn subscribe_message(key: &StreamKey, limit: u32) -> String {
    serde_json::to_string(&ClientMessage::SubscribeSessionChat {
        project_id: key.0.clone(),
        session_id: key.1.clone(),
        limit: Some(limit),
    })
    .unwrap_or_default()
}

pub(crate) fn unsubscribe_message(key: &StreamKey) -> String {
    serde_json::to_string(&ClientMessage::UnsubscribeSessionChat {
        project_id: key.0.clone(),
        session_id: key.1.clone(),
    })
    .unwrap_or_default()
}

/// The ladder step for the `attempts`-th reconnect in a row.
pub(crate) fn reconnect_delay_ms(attempts: usize) -> u64 {
    RECONNECT_DELAYS_MS[attempts.min(RECONNECT_DELAYS_MS.len() - 1)]
}

/// `ws://<host>:<port>/api/events?protocolVersion=1&stream=sessionChat`, from `http://<host>:<port>`
/// (`https` becomes `wss`). The token is added by the caller only where a header cannot carry it.
pub(crate) fn events_url(base_url: &str) -> Option<String> {
    let base = base_url.trim().trim_end_matches('/');
    let rest = base
        .strip_prefix("http://")
        .map(|rest| ("ws://", rest))
        .or_else(|| base.strip_prefix("https://").map(|rest| ("wss://", rest)))?;
    Some(format!(
        "{}{}{GXSERVER_EVENTS_PATH}?protocolVersion={GXSERVER_PROTOCOL_VERSION}&stream={GXSERVER_EVENTS_STREAM_SESSION_CHAT}",
        rest.0, rest.1
    ))
}

/// Routes one socket message, or `None` when it is not for anyone this socket follows.
///
/// A frame passes the eight checks `socket.ts` ran before it reached a follower: one of the four
/// chat types, string `projectId` / `sessionId` / `serverId`, numeric `epoch` / `seq`, and this
/// build's protocol version. Anything else (the `eventStreamReady` header, a frame for a
/// conversation unfollowed a moment ago) is dropped here.
pub(crate) fn route(machine_id: &str, text: &str, followers: &Followers) -> Option<Inbound> {
    let frame: Value = serde_json::from_str(text).ok()?;
    let object = frame.as_object()?;
    let kind = object.get("type")?.as_str()?;
    if kind == MODEL_CATALOG_CHANGED {
        let catalog = object.get("catalog")?.clone();
        return Some(Inbound::ModelCatalog {
            machine_id: machine_id.to_string(),
            catalog,
        });
    }
    if !FRAME_TYPES.contains(&kind)
        || !object.get("epoch").is_some_and(Value::is_number)
        || !object.get("seq").is_some_and(Value::is_number)
        || !object.get("serverId").is_some_and(Value::is_string)
        || object.get("protocolVersion").and_then(Value::as_u64) != Some(GXSERVER_PROTOCOL_VERSION)
    {
        return None;
    }
    let project_id = object.get("projectId")?.as_str()?.to_string();
    let session_id = object.get("sessionId")?.as_str()?.to_string();
    if !followers.contains_key(&(project_id.clone(), session_id.clone())) {
        return None;
    }
    Some(Inbound::Frame {
        machine_id: machine_id.to_string(),
        project_id,
        session_id,
        frame,
    })
}

