use std::sync::{Arc, OnceLock};

use crate::logging::GxserverLogger;

static LOGGER: OnceLock<(Arc<GxserverLogger>, String)> = OnceLock::new();

/// Tell the module where the mismatch log goes. Called once at start.
pub(crate) fn init_program_status_logger(logger: Arc<GxserverLogger>, server_id: &str) {
    let _ = LOGGER.set((logger, server_id.to_string()));
}

/// The gxserver logger and server id, once `init_program_status_logger` ran.
pub(crate) fn program_status_logger() -> Option<&'static (Arc<GxserverLogger>, String)> {
    LOGGER.get()
}
