//! Family d: the composer. Drafts, the queue, submission, suggestions, references, attachments,
//! the session note and the keys.
//!
//! This directory is family d's alone. It owns `ChatState::composer` and the document keys listed
//! for family d in `docs/2026-09-21/rust-chat/FAMILIES.md`.

pub mod actions;
pub mod document;
pub mod draft_sync;
pub mod history;
pub mod host_actions;
pub mod json;
pub mod keys;
pub mod layout;
pub mod links;
pub mod note;
pub mod policy;
pub mod queries;
pub mod queue;
pub mod queue_edit;
pub mod reference_menu;
pub mod reference_pills;
pub mod references;
pub mod slash_catalogs;
pub mod slash_commands;
pub mod storage;
pub mod submission;
pub mod suggestions;
pub mod text;
pub mod transcript_menu;
pub mod trigger;
pub mod view;

pub mod send;
pub mod settle;

pub use crate::composer::actions::handle;
pub use crate::composer::document::document;
pub use crate::composer::settle::settle;
pub use crate::composer::queries::{
    composer_references, composer_key_intent, reference_menu, send_blocked_toast, transcript_menu,
};
