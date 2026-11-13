//! Family f: the extras. The minimap, transcript search, the subagent viewer, the agent fleet and
//! task panels, the working strip, the terminal tail, the empty state and Save to Markdown.
//!
//! This directory is family f's alone. It owns `ChatState::extras` and the document keys listed
//! for family f in `docs/2026-09-21/rust-chat/FAMILIES.md`.

pub mod actions;
pub mod activity;
pub mod agent_fleet;
pub mod agent_tasks;
pub mod agents;
pub mod autopilot;
pub mod background_tasks;
pub mod chat_pills;
pub mod collab;
pub mod coordinator_board;
pub mod coordinator_intro;
pub mod coordinator_quality;
pub mod coordinator_threads;
pub mod document;
pub mod minimap;
pub mod minimap_rail;
pub mod new_chat_tips;
pub mod panels;
pub mod relayed_approvals;
pub mod report;
pub mod save_markdown;
pub mod save_markdown_paths;
pub mod search;
pub mod secret_requests;
pub mod session_details;
pub mod settle;
pub mod side_chats;
pub mod subagent;
pub mod subagent_rows;
pub mod subagent_target;
pub mod terminal_tail;
pub mod terminal_tail_format;
pub mod time;
pub mod transcript_search;
pub mod welcome;
pub mod work_cards;
pub mod work_panel;
pub mod working_card;
pub mod working_card_label;
pub mod working_strip;
pub mod working_words;

pub use crate::extras::actions::handle;
pub use crate::extras::document::{document, document_with_board};
pub use crate::extras::minimap::markers;
pub use crate::extras::settle::settle;
pub use crate::extras::subagent_rows::{row_detail as subagent_row_detail, rows as subagent_rows};
