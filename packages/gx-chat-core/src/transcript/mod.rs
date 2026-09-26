//! Family b: the transcript rows. Messages, tool grouping, diffs, file changes, markdown, row
//! details and the rewind sheet.
//!
//! This directory is family b's alone. It owns `ChatState::transcript_view` and the document keys
//! listed for family b in `docs/2026-09-21/rust-chat/FAMILIES.md`.

pub mod actions;
pub mod agent_message;
pub mod deferred_work;
pub mod diff;
pub mod document;
pub mod file_change_rows;
pub mod file_changes;
pub mod file_paths;
pub mod file_position;
pub mod foreign;
pub mod image_markers;
pub mod images;
pub mod json_text;
pub mod jsstr;
pub mod line_breaks;
pub mod links;
pub mod local_command;
pub mod markdown_links;
pub mod message_text;
pub mod message_time;
pub mod native_markdown;
pub mod noise;
pub mod presentation;
pub mod prose;
pub mod question_exchange;
pub mod rows;
pub mod simple;
pub mod subagent;
pub mod system_cards;
pub mod tool_fold;
pub mod tool_rows;
pub mod tool_summary;
#[allow(clippy::module_inception)]
pub mod transcript;
pub mod transcript_rows;
pub mod turns;

pub mod settle;

pub use crate::transcript::actions::handle;
pub use crate::transcript::document::document;
pub use crate::transcript::rows::{advance, refresh, row_details, rows};
pub use crate::transcript::settle::settle;
