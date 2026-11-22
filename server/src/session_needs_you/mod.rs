//! Needs you to-dos for any session (`ghostex needs-you`): a step only the user can do (a sudo
//! command, a tap on the phone, a sign-in, a check by eye) as a lasting row above the chat box with
//! a Done button, numbered with the session's own Q tags.
//!
//! CDXC:SessionChat 2026-11-16 DECISION:
//! User, after asking an ordinary session to "always, in this kind of case, put a needs you work
//! item for me so I can do it", on the options page (docs/2026-11-15/needs-you-sessions), Q59 "B.
//! A to-do any session can add": `ghostex needs-you add "<what>" [--detail] [--command] [--answer
//! …]`, plus `list`, `done Q<n>` (the agent saw it happen) and `drop Q<n>`, numbered with the
//! session's own Q tags so a to-do and a question never share a number. The to-do shows as a row
//! in that session's dock above the chat box, stays until the user presses Done or the agent
//! clears it, and survives a cleared chat (it lives in the session's record, not its transcript).
//! A command shows as a chip the user clicks to copy. Q65: no list across sessions for now.
//! SEE-ALSO: packages/gx-chat-core/src/extras/needs_you.rs (the dock rows),
//! apps/desktop/src/app/native_chat/work_cards.rs and apps/mobile/app/src/chat/native/cards/WorkCards.tsx
//! (the rows' command chip), server/src/ghostex_cli/needs_you.rs (the verbs),
//! server/src/agents/ghostex_note.rs (`NEEDS_YOU_NOTE`).

mod api;
mod store;
mod thread;

pub(crate) use api::{
    SESSION_NEEDS_YOU_ENDPOINT, handle_needs_you_request, highest_tag, insert_needs_you,
    needs_you_identity, route_board_answer, turn_ended,
};
pub(crate) use store::open_count;
pub(crate) use thread::{forward_answer, thread_ask_of};
