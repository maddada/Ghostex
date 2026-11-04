//! Family e: menus and pickers. The model picker, the option pills and their menus, accounts,
//! context details and the fork branch picker.
//!
//! This directory is family e's alone. It owns `ChatState::menus` and the document keys listed for
//! family e in `docs/2026-09-21/rust-chat/FAMILIES.md`.
//!
//! Family e is split in two: e1 owns this directory (session options, option pills, option menus,
//! option state and dispatch, accounts and the account switch, native controls, chat settings) and
//! e2 owns the two subdirectories `picker/` (model picker, model menu, model selection, model
//! favorites, fork branches) and `context/` (context meter, editor and details). `document.rs` and
//! `actions.rs` here call into both, so neither half edits the other's files.

pub mod account_switch;
pub mod accounts_data;
pub mod accounts_presentation;
pub mod actions;
pub mod catalog;
pub mod cloud_box;
pub mod context;
pub mod controls;
pub mod dispatch_run;
pub mod document;
pub mod draft_worktree;
pub mod lifecycle;
pub mod native_accounts;
pub mod option_catalog;
pub mod option_dispatch;
pub mod option_menu;
pub mod option_menus;
pub mod option_pills;
pub mod option_storage;
pub mod option_store;
pub mod option_values;
pub mod options;
pub mod orchestrator_effort;
pub mod picker;
pub mod run_location;
pub mod time;

pub mod settle;

pub use crate::menus::actions::handle;
pub use crate::menus::document::document;
pub use crate::menus::lifecycle::observe;
pub use crate::menus::settle::settle;
