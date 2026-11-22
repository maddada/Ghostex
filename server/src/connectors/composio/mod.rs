//! A workspace's apps through Composio, ported from Open Dot's `composio.ts` (Composio For You: an
//! OAuth sign-in held by gxserver and Composio's hosted MCP server, whose tools reach the
//! workspace's agent sessions).
//!
//! Everything here is blocking (call from `spawn_blocking`); the OAuth redirect route is the one
//! async entry.

mod account;
mod connections;
mod http;
pub(super) mod mcp_client;
mod names;
mod oauth;
mod oauth_route;
mod read_rule;
mod signin;
mod tools;

pub(crate) use account::{set_changed_listener, NOT_CONNECTED, UNREADABLE};
pub(crate) use connections::start_connect;
pub(crate) use oauth_route::{serve_oauth_redirect, OAUTH_REDIRECT_PATH};
pub(crate) use read_rule::{call_decision, call_label, rule_tools, ComposioDecision};
pub(crate) use signin::{sign_in, sign_in_unreadable, sign_out, signed_in};
pub(crate) use tools::{apps, call_tool, mcp_tools, refresh};

/// Apps suggested before the workspace connected any (Open Dot's SUGGESTED).
pub(crate) const SUGGESTED_APPS: &[&str] = &[
    "gmail",
    "googlecalendar",
    "slack",
    "notion",
    "github",
    "googledrive",
    "linear",
    "outlook",
];

/// The suggestions with their names and logos, for Settings.
pub(crate) fn suggested_apps() -> Vec<serde_json::Value> {
    SUGGESTED_APPS
        .iter()
        .map(|slug| {
            serde_json::json!({
                "slug": slug,
                "name": names::pretty_name(slug),
                "logo": names::logo_url(slug),
            })
        })
        .collect()
}

/// A started app connection: the sign-in link to open.
#[derive(Clone, Debug)]
pub(crate) struct ComposioConnectStart {
    pub(crate) already_connected: bool,
    pub(crate) name: String,
    pub(crate) url: Option<String>,
}
