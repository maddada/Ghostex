//! Family a: the wire fold, the merge, pagination, the pending echoes, persistence, and the
//! controller core.
//!
//! This directory is family a's alone. It owns `ChatState::identity`, `ChatState::session`,
//! `ChatState::messages`, `ChatState::pending` and `ChatState::core`, and it publishes the
//! document keys listed for family a in `docs/2026-09-21/rust-chat/FAMILIES.md`.

pub mod actions;
pub mod answer_echoes;
pub mod app_commands;
pub mod appended_notes;
pub mod apply;
pub mod assembler;
pub mod cloud_box;
pub mod composition;
pub mod computer;
pub mod constants;
pub mod document;
pub mod error_card;
pub mod events;
pub mod fold;
pub mod frame_publish;
pub mod markers;
pub mod merge;
pub mod pagination;
pub mod pending;
pub mod persistence;
pub mod presentation;
pub mod reads;
pub mod sends;
pub mod settle;
pub mod startup_sends;
pub mod stream;
pub mod streaming;
pub mod terminal;
pub mod terminal_text;
pub mod text;
pub mod timers;
pub mod view_state;
pub mod working;

pub use crate::session::actions::handle;
pub use crate::session::document::document;
pub use crate::session::events::handle as handle_event;
pub use crate::session::events::{boot_read, settle};
pub use crate::session::settle::before_compose;
