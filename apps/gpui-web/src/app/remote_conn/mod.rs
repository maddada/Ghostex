//! Remote machines in the browser build. The desktop reaches another computer's gxserver through an SSH tunnel it opens and owns (`apps/desktop/src/app/remote_conn/`); a page cannot open one, and the web store connects only to the daemon that served it, so no remote machine tab is ever drawn here. These are the names the shared files call, with the browser's answer: a request to a remote machine is refused with a sentence, never sent to the local daemon.
mod project_docs;
pub(crate) mod sidebar_rpc;

/// A live tunnel to a remote machine. The page never opens one, so the map that holds them (`remote_gxserver_connections`) stays empty; the type exists for the shared files that look a machine up in it.
pub(crate) struct GpuiRemoteGxserverConnection {
    target: crate::app::model::GpuiRemoteGxserverRequestTarget,
}

impl GpuiRemoteGxserverConnection {
    pub(crate) fn request_target(&self) -> crate::app::model::GpuiRemoteGxserverRequestTarget {
        self.target.clone()
    }
}

impl crate::GhostexGpuiApp {
    /// Another computer's model catalog (the desktop reads it through that computer's tunnel, remote_conn/coordinator_catalog.rs). The page has no tunnels, so there is none to reach.
    pub(crate) fn gpui_remote_model_catalog(
        &self,
        _machine_id: &str,
        _cx: &mut gpui::Context<Self>,
    ) -> gpui::Task<crate::app::window::RemoteModelCatalog> {
        gpui::Task::ready(crate::app::window::RemoteModelCatalog::Unreachable)
    }
}
