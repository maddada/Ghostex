//! The New/Edit Space dialog's result inside the app.
//!
//! CDXC:Spaces 2026-09-21 DECISION:
//! User, 2026-09-19: the desktop app stops running product logic in QuickJS. Creating, renaming,
//! recolouring and deleting a Space were the last edits of the Spaces document computed in the
//! sidebar page (`metadata.ts`, `applySidebarSpaceEditorResult`) and handed over, so they move
//! here and this app is the only desktop writer of that document for this computer. Supersedes the
//! placement, not the intent, of `CDXC:Spaces 2026-08-27`: gxserver still owns the whole document
//! and there is still no local key, and only bounded metadata still crosses from the dialog.
//!
//! **A REMOTE machine's document goes down that machine's tunnel** since 2026-09-21: the dialog's
//! result is applied to the machine's held copy and sent to the runtime as `updateSidebarSpaces`
//! with a `remoteMachineId` (`gx_store/remote_project_docs.rs`). The page used to compute it in
//! `metadata.ts`, and that leg is gone with the local one, so there is still exactly one writer.
//!
//! SEE-ALSO: packages/gx-core/src/project_docs/space_editor.rs,
//! apps/desktop/src/app/gx_store/project_docs.rs.

use ghostex_gx_core::{
    SpaceEditorMode, SpaceEditorResult, SpacesDocument, plan_space_editor_result,
};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with the Space editor's results. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpaceEditorCounters {
    pub(crate) creates: u64,
    pub(crate) edits: u64,
    pub(crate) deletes: u64,
    /// Results that wrote nothing: a delete or an edit naming a Space that is gone, and a create
    /// with no name left after the trim.
    pub(crate) refusals: u64,
    /// Results that named a REMOTE machine's document and went down that machine's tunnel.
    pub(crate) remotes: u64,
    /// Payloads that are not a dialog result at all. Its own counter because a silent drop here is
    /// a Space the user made and never got.
    pub(crate) unparsable: u64,
}

impl GhostexGpuiApp {
    /// Applies the dialog's result to this computer's Spaces document.
    ///
    /// The result still reaches the sidebar page afterwards, because the page keeps its own Space
    /// selection and its own projection; what the page no longer does is EDIT the document for this
    /// computer, so there is one writer and not two.
    pub(crate) fn gx_store_run_space_editor_result(
        &mut self,
        message: &serde_json::Map<String, Value>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(result) = SpaceEditorResult::from_json(&Value::Object(message.clone())) else {
            self.gx_store.space_editor.unparsable += 1;
            return;
        };
        // The dialog names the machine whose document it edits; the selected tab is not asked,
        // because the dialog can outlive a tab switch.
        let remote_machine_id = result.remote_machine_id.clone();
        if remote_machine_id.is_some() {
            self.gx_store.space_editor.remotes += 1;
        }
        // `this.spaces[id] ?? EMPTY_SIDEBAR_SPACES_STATE`: before the daemon's first document there
        // is nothing to hold, and the page edited the empty state too. The document has no stored
        // key at all, so there is no read to wait for and no cold-start window where an edit could
        // land on a document this app has not seen.
        let held = match &remote_machine_id {
            Some(machine_id) => self.gx_store_remote_spaces(machine_id).unwrap_or_default(),
            None => match self.gx_store.spaces.sync.has_document() {
                true => self.gx_store.spaces.sync.document().clone(),
                false => SpacesDocument::default(),
            },
        };
        let mode = result.mode;
        let plan = plan_space_editor_result(&held, &result, super::host::now_ms() as i64);
        let Some(document) = plan else {
            self.gx_store.space_editor.refusals += 1;
            self.gx_store
                .diagnostics
                .space_editor_ran(mode, false, self.gx_store.space_editor);
            return;
        };
        match mode {
            SpaceEditorMode::Create => self.gx_store.space_editor.creates += 1,
            SpaceEditorMode::Edit => self.gx_store.space_editor.edits += 1,
            SpaceEditorMode::Delete => self.gx_store.space_editor.deletes += 1,
        }
        self.gx_store
            .diagnostics
            .space_editor_ran(mode, true, self.gx_store.space_editor);
        match remote_machine_id {
            Some(machine_id) => self.gx_store_send_remote_spaces(&machine_id, &document, cx),
            None => self.gx_document_edit::<SpacesDocument>(document, cx),
        }
    }
}
