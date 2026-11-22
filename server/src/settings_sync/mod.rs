//! Settings sync between the user's own computers (rules: gx-core `settings_sync`): this computer's
//! synced entries, the exchange another computer starts over the peer link, the rounds this one
//! starts, the bridges to the Spaces documents, the Session Tags and to Accounts, Agents and Skills, and Settings >
//! Sync's calls.

mod categories;
mod engine;
mod exchange;
mod http;
mod lists_bridge;
mod runner;
mod sidebar_tags;
mod spaces_bridge;
mod store;

pub(crate) use exchange::{handle_settings_sync_exchange, EXCHANGE_ENDPOINT};
pub(crate) use http::{handle_settings_sync_http, SETTINGS_SYNC_ENDPOINTS};
pub(crate) use lists_bridge::agents_synced_from_elsewhere;
pub(crate) use runner::{nudge_settings_sync, start_settings_sync};
