//! Family c: questions, approvals and notices. The blocking question card, the async question
//! strip a working agent collects answers in, and the dismissible notices.
//!
//! This directory is family c's alone. It owns `ChatState::questions` and the document keys listed
//! for family c in `docs/2026-09-21/rust-chat/FAMILIES.md`.

pub mod account_wait;
pub mod actions;
pub mod async_answers;
pub mod async_controller;
pub mod content;
pub mod dialog_rows;
pub mod document;
pub mod drafts;
pub mod events;
pub mod exchange;
pub mod exchange_answers;
pub mod forms;
pub mod gates;
pub mod hoisting;
pub mod interactive;
pub mod later;
pub mod model;
pub mod notice_choices;
pub mod notice_state;
pub mod notice_tail;
pub mod notice_tone;
pub mod open_questions;
pub mod resource_index;
pub mod resource_modal;
pub mod permission_card;
pub mod sync;
pub mod terminal_dialog_copy;
pub mod terminal_picker;
pub mod terminal_prompts;
pub mod unreachable_retry;

pub use crate::questions::actions::handle;
pub use crate::questions::document::document;
pub use crate::questions::events::{rpc_settled, storage_loaded, storage_written};
pub use crate::questions::exchange_answers::{answered_question_exchange, QuestionExchange};
pub use crate::questions::hoisting::{hoisted_question_exchanges, message_question_exchanges};
pub use crate::questions::sync::{settle, sync};
