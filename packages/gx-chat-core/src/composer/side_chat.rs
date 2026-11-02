//! Side chat: a message sent with the Side Chat pill (`/btw ` at its start) starts a side chat, a
//! fork of this session that answers it beside the main conversation (server/src/side_chats/,
//! extras/side_chats.rs), instead of reaching the agent CLI as its own `/btw`.
//!
//! CDXC:SessionChat 2026-09-30 DECISION:
//! User: a "Side chat" button in the chat composer's More actions puts a "Side Chat" pill in the chat box, and the user can delete the pill. The pill is the `/btw ` prefix itself, drawn as a reference pill, so drafts and other devices need nothing new. Sending a side chat switches Side chat off: the pill leaves with the message (supersedes the 2026-09-27 choice to keep it after a send).
//!
//! CDXC:SideChats 2026-11-02 DECISION:
//! User: replace the agent CLI's `/btw` everywhere with t3code-style side chats. What is sent with the Side Chat pill, or typed as `/btw <question>`, forks the session into a side chat that starts with that question and opens beside the chat; it never reaches the main agent. Supersedes Side chat sending `/btw` to Claude (its answer card above the chat box) and switching Codex chats to the terminal (2026-09-30). Offered for every agent Ghostex can fork, and not in a side chat itself.
//! SEE-ALSO: apps/desktop/src/app/native_chat/actions.rs, apps/mobile/app/src/chat/native/composer/menus.ts, server/src/side_chats/mod.rs.

use crate::menus::option_menus::DraftAgent;
use crate::state::ChatState;

/// What Side chat puts at the start of the chat box.
pub const SIDE_CHAT_PREFIX: &str = "/btw ";

/// The pill's words in the chat box.
pub const SIDE_CHAT_LABEL: &str = "Side Chat";

/// The CLI behind this session: a custom agent reports its base CLI, the way the account panel
/// resolves it (menus/controls.rs).
pub(crate) fn agent_family(state: &ChatState) -> Option<String> {
    DraftAgent::list(state.session.available_agents.as_ref())
        .and_then(|agents| {
            agents
                .iter()
                .find(|row| {
                    Some(row.agent_id.as_str()) == state.session.session_agent_id.as_deref()
                })
                .and_then(|row| row.base_agent_id.clone())
        })
        .or_else(|| state.session.agent.clone())
}

/// Whether this chat can start a side chat: its agent is one gxserver can fork, and it is not a
/// side chat itself.
pub fn side_chat_available(state: &ChatState) -> bool {
    crate::composer::host_actions::session_supports_fork(state)
        && !crate::extras::side_chats::is_side_chat(state.session.side_chat_of.as_ref())
}

/// The question of a Side chat send: the text after the pill, when there is one.
pub fn side_chat_question(text: &str) -> Option<&str> {
    text.strip_prefix(SIDE_CHAT_PREFIX)
        .map(str::trim)
        .filter(|question| !question.is_empty())
}

/// Whether a send starts a side chat instead of reaching the agent.
pub fn starts_side_chat(state: &ChatState, text: &str) -> bool {
    side_chat_question(text).is_some() && side_chat_available(state)
}

/// The Side chat prefix, when Side chat is available.
pub fn side_chat_prefix(state: &ChatState) -> Option<String> {
    side_chat_available(state).then(|| SIDE_CHAT_PREFIX.to_string())
}

/// The chat box with Side chat switched on (prefix added) or off (prefix removed).
pub fn toggle_side_chat(text: &str) -> String {
    match text.strip_prefix(SIDE_CHAT_PREFIX) {
        Some(rest) => rest.to_string(),
        None => format!("{SIDE_CHAT_PREFIX}{}", text.trim_start()),
    }
}

/// The tag gxserver wraps around a side chat's note to its agent, at the start of its first
/// message. The chat shows that message without the note.
pub const SIDE_CHAT_NOTE_OPEN: &str = "<ghostex-side-chat>";
pub const SIDE_CHAT_NOTE_CLOSE: &str = "</ghostex-side-chat>";

/// What a side chat's agent is told before the user's first question.
const SIDE_CHAT_NOTE: &str = "You are in a side chat beside the main session you were forked from. The main session may still be working in this same folder, and its last turn may be unfinished in what you can see. Answer the user's questions here; do not continue or redo the main session's task, and ask before you change files.";

/// A side chat's first message: the note in its tag, then the user's question.
pub fn side_chat_first_message(question: &str) -> String {
    format!(
        "{SIDE_CHAT_NOTE_OPEN}{SIDE_CHAT_NOTE}{SIDE_CHAT_NOTE_CLOSE}\n\n{}",
        question.trim()
    )
}

/// A message without a side chat's note: what the user wrote.
pub fn without_side_chat_note(text: &str) -> &str {
    let trimmed = text.trim_start();
    trimmed
        .strip_prefix(SIDE_CHAT_NOTE_OPEN)
        .and_then(|rest| rest.split_once(SIDE_CHAT_NOTE_CLOSE))
        .map(|(_, question)| question.trim_start())
        .unwrap_or(text)
}
