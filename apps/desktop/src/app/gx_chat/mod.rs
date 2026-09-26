//! The chat host for `packages/gx-chat-core`: the Rust chat brain's socket, storage and timers.
//!
//! `docs/2026-09-21/rust-chat/PLAN.md` step 4. The core performs no I/O and reads no clock, so this
//! directory is everything around it: one [`ChatCore`](ghostex_gx_chat_core::ChatCore) per retained
//! session, the effects it asks for, the client-storage records it does not own, the chat socket,
//! and the frame envelope `apps/desktop/src/app/native_chat/` already consumes.
//!
//! **Both apps compile it.** `apps/gpui-web/src/app/gx_chat/` links every file here except three it
//! replaces with browser twins: `worker.rs` (a page-thread runner instead of a background thread),
//! `storage_backend.rs` (the page's `packages/client-storage` instead of SQLite) and `platform.rs`
//! (the page's clock and `crypto`). Anything that is not about threads, disks or the OS belongs in
//! the shared files.
//!
//! **The renderer does not change.** `state.rs`'s `apply_output` destructures the exact object
//! `nativeChat.take(lastRevision)` returned, so [`frame::envelope`] builds that same object from
//! [`Frame`](ghostex_gx_chat_core::Frame) and turns the effects the UI thread must perform into the
//! same `requests` entries the QuickJS brain pushed. Everything else the host performs itself.
//!
//! Per-concern files:
//!
//! - `identity.rs` names a retained chat and builds its storage session key.
//! - `world.rs` is the host (every chat, its effects, its drains); `worker.rs` runs it and is the
//!   per-view handle; `store.rs` is the retained map and its retention limits.
//! - `transport.rs` connects the chats to `packages/gx-chat-client`'s socket: follows, frames, the
//!   pushed model catalog and the context preferences one chat writes for the others.
//! - `events.rs` turns the renderer's calls and gxserver's frames into `Event`s, `effects.rs`
//!   performs or forwards each `Effect`, `frame.rs` builds the drained envelope, `queries.rs`
//!   answers the five pure helpers.
//! - `storage.rs` is the chat half of `packages/client-storage/catalog.ts`, `storage_backend.rs`
//!   where its rows live; `host_records.rs` owns the four records the core left to the host;
//!   `boot.rs` answers `Effect::ReadComposerBoot`.
//! - `retained.rs` is the transcript cache a reopened chat draws before its snapshot arrives.
//! - `draft_ops.rs` performs the three draft operations the core names as stores, `outbox.rs` is
//!   the durable save outbox and its retry ladder, `saves.rs` connects that queue to a chat,
//!   `dismissals.rs` folds the recovery markers a retirement leaves into ranges, `locale.rs` the
//!   two formatted stamps the host owes the core.
//! - `platform.rs` is the clock, the timezone and the random source.
//! - `diagnostics.rs` writes the periodic counters, behind the usual two gates; `refusals.rs` names
//!   a refused request and an unrouted call with code constants.

mod boot;
mod diagnostics;
mod dismissals;
mod draft_ops;
mod effects;
mod events;
mod frame;
mod host_records;
mod identity;
mod locale;
mod outbox;
mod platform;
mod queries;
mod refusals;
mod retained;
mod saves;
pub(crate) mod saved_prompt_records;
mod storage;
mod storage_backend;
mod store;
mod transport;
mod worker;
mod world;

pub(crate) use identity::LOCAL_MACHINE_ID;
pub(crate) use worker::{ChatHostHandle, set_endpoint};
pub(crate) use world::ChatHostOutput;
