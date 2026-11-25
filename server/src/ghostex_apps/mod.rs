//! Ghostex Apps: HTML apps an agent builds with `ghostex app show`, kept under the gxserver state
//! directory with a stable id and every version, served at `/app/<id>` and opened like a Live page.
//! Unlike a Live page an app is never deleted on its own, keeps its own data and may use powers
//! (sending to its chat, and in later phases notifications, always on top, a menu bar item, a
//! global shortcut and connectors), each one only after the user allowed it.
//!
//! `store` keeps the apps, `manifest` reads what an app's HTML declares, `icon` checks the icon the
//! agent drew, `publish` and `api` are the authenticated routes, `serve` answers the app itself,
//! `bridge` is the app's own calls (`window.ghostex.app`) and `tokens` the launch tokens they need.

mod api;
mod bridge;
mod feature;
pub(crate) mod icon;
pub(crate) mod manifest;
mod publish;
mod serve;
mod store;
mod tokens;

pub(crate) use api::{
    handle_ghostex_apps_http, is_ghostex_apps_endpoint, DELETE_GHOSTEX_APP_ENDPOINT,
    LIST_GHOSTEX_APPS_ENDPOINT, OPEN_GHOSTEX_APP_ENDPOINT, READ_GHOSTEX_APP_ENDPOINT,
    SET_GHOSTEX_APP_GRANT_ENDPOINT,
};
pub(crate) use bridge::{handle as bridge_call, is_bridge_route};
pub(crate) use feature::agent_note;
pub(crate) use publish::{PUBLISH_GHOSTEX_APP_BODY_LIMIT_BYTES, PUBLISH_GHOSTEX_APP_ENDPOINT};
pub(crate) use serve::{is_app_route, serve};
