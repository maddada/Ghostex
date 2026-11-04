//! The port the way back from a connected computer lands on.

use std::sync::atomic::{AtomicU16, Ordering};

static PEER_LISTENER_PORT: AtomicU16 = AtomicU16::new(0);

/// This gxserver's second loopback listener, which a connected computer's reverse SSH forward
/// points at; `None` until it is bound (or when it could not be).
pub(crate) fn peer_listener_port() -> Option<u16> {
    match PEER_LISTENER_PORT.load(Ordering::SeqCst) {
        0 => None,
        port => Some(port),
    }
}

pub(crate) fn set_peer_listener_port(port: u16) {
    PEER_LISTENER_PORT.store(port, Ordering::SeqCst);
}
