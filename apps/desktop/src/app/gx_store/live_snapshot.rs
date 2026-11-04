//! This machine's presentation snapshot as the app's live store holds it.
//!
//! CDXC:StateSync 2026-11-05 DECISION:
//! User (Q1244): the app's own readers take what its live store already holds instead of fetching
//! the whole snapshot over HTTP (about 0.9 MB on a Mac with 45 sessions, rebuilt by gxserver for
//! each read). The store keeps this machine current over the event socket, and its typed snapshot
//! carries the sessions, projects and groups whole (server test
//! `a_snapshot_survives_the_typed_store_whole`). Readers take it on the main thread before their
//! background work starts; a store that has not loaded this machine yet gives `None` and the
//! reader asks gxserver as before.

use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// The local machine's snapshot from the live store, shaped as `/api/readPresentationSnapshot`'s
    /// `snapshot`, or `None` while the stream has not delivered it.
    pub(crate) fn gx_store_local_snapshot_value(&self) -> Option<serde_json::Value> {
        let presentation = self.gx_store.core.presentation();
        let local = ghostex_gx_core::MachineId::Local;
        presentation.loaded_live(&local)?;
        serde_json::to_value(presentation.machine(&local)?.to_snapshot()?).ok()
    }
}
