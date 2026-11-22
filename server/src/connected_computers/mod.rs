//! Connected computers: the computers this one's agents can start, message and read sessions on
//! (and that can do the same here), reached over the desktop app's connection to them.

mod held_sends;
mod http;
mod peer_listener;
mod registry;
mod relay;
mod scope;
mod thread_scope;

pub(crate) use held_sends::{
    HELD_SEND_ENDPOINTS, handle_held_sends_http, read_known_computers, start_held_send_delivery,
};
pub(crate) use http::{
    CONNECTED_COMPUTER_ENDPOINTS, handle_connected_computers_http, this_computer_name,
};
pub(crate) use peer_listener::{peer_listener_port, set_peer_listener_port};
pub(crate) use registry::{
    ConnectedComputer, DEFAULT_REGISTRATION_TTL, Direction, PeerAuthorization, Registration,
    authorize_peer_token, find, has_grant, is_server_id, issue_grant, list, owner_issued, register,
    revoke_grant, unregister,
};
pub(crate) use relay::{
    RELAY_ROUTE_PREFIX, access_off_message, parse_relay_path, peer_refusal, relay, relay_base_url,
};
pub(crate) use scope::{PEER_ALLOWED_ENDPOINTS, is_peer_allowed};

#[cfg(test)]
pub(crate) mod tests;
