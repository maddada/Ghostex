//! The session chat's connection to gxserver, shared by the desktop app and the GPUI web build.
//!
//! [`ChatStreams`] keeps one `/api/events?stream=sessionChat` socket per machine, opened only
//! while that machine has a followed conversation and an endpoint. A host calls
//! [`ChatStreams::follow`] when a chat asks to subscribe, [`ChatStreams::refresh`] when it asks for
//! a fresh authoritative snapshot, and [`ChatStreams::unfollow`] when it lets the chat go; every
//! frame for a followed conversation, and every published model catalog, comes back through the
//! sink the host passed to [`ChatStreams::new`] as an [`Inbound`].
//!
//! What stays out on purpose:
//!
//! - Folding, sequencing and resync. `packages/gx-chat-core` owns them (`session/stream.rs`,
//!   `session/fold.rs`), so this crate forwards every well-formed frame of a followed conversation
//!   and lets the core drop, apply or re-read.
//! - Chat RPCs. They are one `POST /api/<method>` each and the chat view already performs them
//!   (`apps/desktop/src/app/native_chat/rpc.rs`, and the web build's `fetch` twin).
//! - Chunking. The 96 KiB pieces the QuickJS broker cut a message into were a cost of crossing
//!   into QuickJS; a socket message arrives whole.
//!
//! The rules (routing, the messages, the ladder) are in `wire.rs` for both targets. The socket is
//! `native.rs` (one tungstenite thread per machine) or `web.rs` (a browser `WebSocket` on the
//! page's one thread).

mod wire;

#[cfg(not(target_family = "wasm"))]
mod native;
#[cfg(target_family = "wasm")]
mod web;

#[cfg(not(target_family = "wasm"))]
pub use native::{ChatStreams, InboundSink};
#[cfg(target_family = "wasm")]
pub use web::{ChatStreams, InboundSink};
pub use wire::{Endpoint, Inbound, RECONNECT_DELAYS_MS};
