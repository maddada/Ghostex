//! Session chat host actions and the app modal bridge, chat drafts and attachments, and opening links and files from a chat, one sibling module per concern (each with its own `impl GhostexGpuiApp` block).

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: sidebar/app-modal/session-chat CEF bridge handlers and chat host actions

mod drafts_and_attachments;
mod file_open;
mod host_actions;
mod run_in_terminal;
mod runnable_links;
pub(crate) mod session_details;

pub(crate) use drafts_and_attachments::*;
pub(crate) use runnable_links::OtherAppLinkSource;
