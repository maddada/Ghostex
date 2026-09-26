//! A remote machine's collections and Spaces documents (`gx_store/remote_project_docs.rs` on the desktop), which go down that machine's tunnel. The page selects only this computer's tab, so the selected remote machine is always none and these are never asked for a document to send; a send that did arrive is refused with the toast a disconnected machine gets.
use ghostex_gx_core::{CollectionsDocument, SpacesDocument};

use super::sidebar_rpc::WEB_REMOTE_MACHINES_UNAVAILABLE;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    pub(crate) fn gx_store_selected_remote_machine_id(&self) -> Option<String> {
        None
    }

    pub(crate) fn gx_store_remote_collections(&mut self, _machine_id: &str) -> CollectionsDocument {
        CollectionsDocument::empty()
    }

    pub(crate) fn gx_store_remote_spaces(&self, _machine_id: &str) -> Option<SpacesDocument> {
        None
    }

    pub(crate) fn gx_store_send_remote_collections(
        &mut self,
        _machine_id: &str,
        _document: &CollectionsDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Remote action unavailable",
            WEB_REMOTE_MACHINES_UNAVAILABLE,
            cx,
        );
    }

    pub(crate) fn gx_store_send_remote_spaces(
        &mut self,
        _machine_id: &str,
        _document: &SpacesDocument,
        cx: &mut gpui::Context<Self>,
    ) {
        self.dispatch_gpui_app_modal_toast(
            "warning",
            "Remote action unavailable",
            WEB_REMOTE_MACHINES_UNAVAILABLE,
            cx,
        );
    }

    pub(crate) fn gx_store_note_remote_order_dropped(&mut self) {}

    /// A remote row's sleep, wake, close, fork, flags, snooze or reload: not this build's (`None` is the desktop's "not owned").
    pub(crate) fn gx_store_start_remote(
        &mut self,
        _message: &serde_json::Value,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Task<bool>> {
        None
    }
}
