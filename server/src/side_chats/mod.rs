//! Side chats: a fork of a session that runs beside it in the same folder, hidden from the session
//! lists and reached from its main session's chat.
//!
//! CDXC:SideChats 2026-11-02 DECISION:
//! User: side chats work like t3code's instead of the agent CLI's own `/btw`, for every agent Ghostex can fork. Side chat (the chat box's Side Chat pill, `/btw <question>`) forks the session and sends the question to the fork; the fork is not a session in the sidebar but one entry of the main chat's Side chats pill, opens in the side panel (its own chat screen on the phone), and has Bring back (its conversation sent to the main session) and Delete.
//! SEE-ALSO: server/src/agents/fork.rs (`fork_session_as`), server/src/server/side_chat_http.rs (routes), packages/gx-chat-core/src/composer/side_chat.rs (the note's tag), packages/gx-core/src/sidebar_view/membership.rs (hidden from lists).

mod digest;
mod launch;
mod panel;

pub(crate) use digest::bring_back_message;
pub(crate) use launch::{ask_before_edits_command, mark_fork_params, parent_of, side_chat_title};
pub use panel::insert_side_chats;
pub(crate) use panel::{cached_side_chats, refresh_side_chat_panels};
