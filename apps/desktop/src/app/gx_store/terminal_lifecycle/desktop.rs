//! The desktop half of the terminal lifecycle executors: reads of the store the executors need.

use ghostex_gx_core::MachineId;
use ghostex_gx_core::protocol::PresentationSession;

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The local daemon's own row for a session, without the store's local patches: the copy the
    /// old runtime read from its `presentation`.
    pub(crate) fn gx_store_local_server_session(
        &self,
        project_id: &str,
        session_id: &str,
    ) -> Option<PresentationSession> {
        self.gx_store
            .core
            .presentation()
            .loaded(&MachineId::Local)?
            .server_session(project_id, session_id)
            .cloned()
    }
}
