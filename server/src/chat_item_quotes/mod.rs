//! The quote under a message that points at a question, a work item or a Live page by its link
//! (`[…](question:Q4933)`, `[…](board:B1071)`, `[…](page:<id>)`), and `ghostex question show`,
//! which prints one question whole. gxserver looks each one up where it lives (a work item's ask,
//! a Needs you to-do, a form or a Live page in the chat, the coordinator's work items) and adds a
//! short quote of each in one `<ghostex-system>` block at the bottom of the prompt, after the
//! quotes of the messages it points at (chat_message_numbers.rs).
//!
//! CDXC:SessionChat 2026-11-25 SEE-ALSO:
//! The user's decision ("the agent gets the question, the work item and the page quoted") is
//! written at packages/gx-chat-core/src/transcript/item_references.rs, which also holds the links
//! and the block's wording. server/src/session_chat_queue_runtime/send.rs adds the quote,
//! server/src/ghostex_cli/question.rs is the CLI verb.

mod endpoint;
mod page;
mod question;
mod quote;
mod work_item;

pub(crate) use endpoint::{handle_chat_questions_http, CHAT_QUESTIONS_ENDPOINT};
pub(crate) use quote::with_item_quotes;
