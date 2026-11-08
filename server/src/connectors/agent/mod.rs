//! Connectors as agents use them: the `ghostex connectors mcp` server every Claude and Codex
//! session loads asks gxserver for its workspace's connector tools and runs each call here, where
//! a call that would change something waits for Ghostex's card. Split by concern; the route
//! functions are re-exported.

mod api;
mod calls;
mod cards;
mod launch;
mod revision;
mod rules;
mod session;

pub(crate) use api::*;
pub(crate) use cards::answer_from_chat as answer_connector_card_from_chat;
pub(crate) use cards::waiting_card as waiting_connector_card;
// A coordinator's chat relays a thread's card (coordinators/relay.rs).
pub(crate) use cards::{
    connector_card_outcome, is_connector_card, Answer as ConnectorCardAnswer,
    CARD_LABELS as CONNECTOR_CARD_LABELS,
};
#[cfg(test)]
pub(crate) use cards::{
    hold_waiting_card as hold_waiting_connector_card,
    release_waiting_card as release_waiting_connector_card,
};
pub(crate) use launch::{
    connector_tool_args, with_connector_tools, CONNECTORS_MCP_SERVER_NAME,
    CONNECTORS_SESSION_REF_ENV,
};
pub(crate) use revision::bump_tools_revision;
// The Session details panel lists the apps of the workspace a session's agent gets.
pub(crate) use session::resolve_cached as resolve_connector_session;
