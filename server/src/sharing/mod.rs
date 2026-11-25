//! Built-in tunnel sharing for Live pages and localhost ports (`server/src/sharing/`).

mod blocklist;
pub(crate) mod config;
pub(crate) mod http;
pub(crate) mod manager;
mod provider;
mod proxy;

pub(crate) use http::{handle_sharing_http, SHARING_ENDPOINTS};
pub(crate) use manager::ShareManager;
