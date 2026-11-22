//! The GBot API: an app or script messages a GBot with that GBot's key and gets its reply, or
//! starts one of its routines (a webhook). Off by default for every GBot.
//!
//! - [`keys`]: `gbots_bot_keys`, the key's SHA-256 only, one live key per GBot.
//! - [`ops`]: `/api/gbots` `api`, `apiOn`, `apiNewKey`, `apiOff` (only the user; on and New key
//!   ask the user's approval first).
//! - [`inbound`]: the `/hooks/gbots/…` routes the key opens.
//! - [`chat`]: which chat an app's message goes to, and the line that marks it as outside text.
//! - [`reply`]: the message's row and its reply, read from the chat's transcript.

mod chat;
mod inbound;
mod keys;
mod ops;
mod reply;

pub(crate) use chat::{is_api_chat, is_api_chat_session, write_api_files};
pub(crate) use inbound::{ROUTE_PREFIX as BOT_API_ROUTE_PREFIX, serve as serve_bot_api};
pub(crate) use keys::{BotKey, bots_with_api, key_by_id};
pub(crate) use ops::{handles, run};
