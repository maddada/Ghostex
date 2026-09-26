//! Native gxserver client for the Rust store in `ghostex-gx-core`.
//!
//! [`GxClient`] owns one std thread and one `/api/events` socket to the daemon on this computer.
//! The thread connects, subscribes to the presentation stream, parses every frame it keeps with
//! [`ghostex_gx_protocol::ServerEvent::parse`], and hands the host ready-made
//! [`ghostex_gx_core::Event`] values through a channel. The host drains the channel on its own
//! thread, feeds the core, and writes the revision the store then holds into the shared atom the
//! client quotes as `lastRevision` on the next subscribe.
//!
//! What this client deliberately never does:
//!
//! - It sends `rendererCommands: true` only when [`GxClientConfig::renderer_commands`] asks for it
//!   (the desktop's local store, and nothing else). Such a socket becomes the daemon's dispatch
//!   target for CLI renderer commands and must answer every one of them.
//! - It never subscribes to a session chat. A chat subscribe from a second client starts a new
//!   epoch and rebroadcasts a snapshot to every client, which would disturb a runtime that is
//!   live beside this one.
//! - It never talks TLS. It is for the loopback daemon only: `http://` and `ws://`.
//!
//! It sends `subscribePresentation` with a client id and, when the store already holds rows,
//! `lastRevision`; a renderer-command target also sends `rendererCommandResult`.

mod client;
mod config;
mod http;
mod output;
mod socket;
mod worker;

pub use crate::client::{GxClient, StartError};
pub use crate::config::{
    GxClientConfig, FORCED_RESUBSCRIBE_INTERVAL, HEALTHY_STREAM_DURATION, RECONNECT_LADDER_MS,
    SUBSCRIBE_ACK_TIMEOUT,
};
pub use crate::output::{
    redact_quoted_values, ClientDiagnostic, ClientOutput, ClientStats, ClientStatsSnapshot,
};
