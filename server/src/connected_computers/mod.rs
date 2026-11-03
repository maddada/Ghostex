//! Connected computers: the computers this one's agents can start, message and read sessions on
//! (and that can do the same here), reached over the desktop app's connection to them.

mod http;
mod peer_listener;
mod registry;
mod relay;
mod scope;

pub(crate) use http::{
    handle_connected_computers_http, this_computer_name, CONNECTED_COMPUTER_ENDPOINTS,
};
pub(crate) use peer_listener::{peer_listener_port, set_peer_listener_port};
pub(crate) use registry::{
    authorize_peer_token, find, is_server_id, list, ConnectedComputer, PeerAuthorization,
};
pub(crate) use relay::{
    access_off_message, parse_relay_path, peer_refusal, relay, relay_base_url, RELAY_ROUTE_PREFIX,
};
pub(crate) use scope::{is_peer_allowed, PEER_ALLOWED_ENDPOINTS};

#[cfg(test)]
pub(crate) mod tests;
