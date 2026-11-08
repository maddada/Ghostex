//! Typed client-side description of the gxserver wire protocol.
//!
//! Rules every type in this crate follows:
//!
//! - Unknown fields never fail a parse (`deny_unknown_fields` is never used), because older and
//!   newer remote daemons are merged into one client.
//! - Every optional wire key is an `Option<T>` (or [`Tri<T>`] where absent and `null` mean
//!   different things) with `#[serde(default)]`.
//! - String enums are open: known variants plus `Other(String)`, so an unknown value round-trips
//!   instead of failing the frame that carries it.
//! - Fields whose shape the protocol survey flagged as loose stay `serde_json::Value`.
//!
//! The crate has no transport code: sockets, HTTP, and reconnect live in the client crate.

#[macro_use]
mod open_enum;

pub mod agent_message_delivery;
pub mod agentbox;
/// Signed approvals for sensitive actions (GitHub #495); used module-qualified.
pub mod approval;
pub mod chat;
pub mod client_message;
/// Cloud Boxes; used module-qualified (`cloud_box::CloudBox`), not glob re-exported, because its
/// param names (`ListParams`, `BoxParams`) are generic.
pub mod cloud_box;
pub mod collab_component;
pub mod coordinator;
pub mod de;
pub mod delta;
pub mod event;
pub mod peek;
pub mod presentation;
pub mod rpc;
pub mod session_chat_agents;
pub mod session_ref;
pub mod side_state;
pub mod starting;
pub mod tri;
pub mod worktree_name;

pub use chat::*;
pub use client_message::*;
pub use coordinator::*;
pub use de::{Rows, SkippedRow};
pub use delta::*;
pub use event::*;
pub use peek::peek_event_type;
pub use presentation::*;
pub use rpc::*;
pub use session_chat_agents::*;
pub use session_ref::*;
pub use side_state::*;
pub use starting::*;
pub use tri::Tri;

/// The `product` string of every RPC envelope.
pub const GXSERVER_PRODUCT: &str = "gxserver";
/// The only protocol version gxserver accepts; the gate is an exact match, not a negotiation.
pub const GXSERVER_PROTOCOL_VERSION: u64 = 1;
/// Request header that carries the protocol version.
pub const GXSERVER_PROTOCOL_VERSION_HEADER: &str = "x-gxserver-protocol-version";
/// Loopback listener of the local daemon.
pub const GXSERVER_LOCAL_HOST: &str = "127.0.0.1";
pub const GXSERVER_LOCAL_PORT: u16 = 58744;
/// Remote listener (disabled by default, bearer token required).
pub const GXSERVER_REMOTE_PORT: u16 = 58745;
/// WebSocket path of the event stream.
pub const GXSERVER_EVENTS_PATH: &str = "/api/events";
/// `stream` query value that restricts an event socket to subscribed session chat frames.
pub const GXSERVER_EVENTS_STREAM_SESSION_CHAT: &str = "sessionChat";
/// `stream` query value that restricts an event socket to notification feed changes and the
/// daemon's start and stop. A client opens it only when `ghostex server endpoint` lists
/// [`GXSERVER_ENDPOINT_CAPABILITY_NOTIFICATION_STREAM`]; an older daemon would send its whole hub.
pub const GXSERVER_EVENTS_STREAM_NOTIFICATIONS: &str = "notifications";
/// The `ghostex server endpoint` capability that promises [`GXSERVER_EVENTS_STREAM_NOTIFICATIONS`].
pub const GXSERVER_ENDPOINT_CAPABILITY_NOTIFICATION_STREAM: &str = "notificationStream";
/// The `ghostex server endpoint` capability that promises `/api/registerPushToken` and push
/// notifications to the phones registered there (finished, needs input, failed, agent starts).
pub const GXSERVER_ENDPOINT_CAPABILITY_PUSH_NOTIFICATIONS: &str = "pushNotifications";
