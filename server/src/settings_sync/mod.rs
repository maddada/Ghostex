//! Settings sync between the user's own computers (rules: gx-core `settings_sync`): this computer's
//! synced entries, the exchange another computer starts over the peer link, the rounds this one
//! starts, the bridge to the Spaces documents and Settings > Sync's calls.

mod categories;
mod engine;
mod exchange;
mod http;
mod runner;
mod spaces_bridge;
mod store;

pub(crate) use exchange::{handle_settings_sync_exchange, EXCHANGE_ENDPOINT};
pub(crate) use http::{handle_settings_sync_http, SETTINGS_SYNC_ENDPOINTS};
pub(crate) use runner::{nudge_settings_sync, start_settings_sync};
