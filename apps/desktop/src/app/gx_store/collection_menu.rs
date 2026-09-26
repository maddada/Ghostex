//! A Project Group's Rename, colour and Ungroup inside the app.
//!
//! CDXC:Projects 2026-09-21 DECISION:
//! User, 2026-09-19: the desktop app stops running product logic in QuickJS. These three menu items
//! were the last `collectionAction` arms the sidebar page still wrote the collections document for
//! (`runNativeCollectionAction`), so they join the project moves on the same host and this app is
//! the only desktop writer of that document for this computer. The other four arms of the same
//! payload (`toggle`, `select`, `toggleProjects`, `hide`) write no document and stay where they
//! are, in `sidebar_ui_commands.rs`.
//!
//! **A REMOTE machine's tab goes down that machine's tunnel**, exactly as the project moves do
//! since 2026-09-21: the document is that machine's held copy, the edit is the same one, and the
//! result is sent down that machine's tunnel as `/api/updateSidebarProjectCollections`
//! (`gx_store/remote_project_docs.rs`; until 2026-09-25 it went to the runtime as
//! `updateSidebarProjectCollections` with a `remoteMachineId`). It is not refused any more, because the payload reached
//! `runNativeCollectionAction` in a page that is being deleted and nothing forwards it in its place.
//!
//! SEE-ALSO: packages/gx-core/src/project_docs/collection_menu.rs,
//! the deleted sidebar page's `collections.ts`,
//! apps/desktop/src/app/gx_store/project_docs.rs.

use ghostex_gx_core::{
    CollectionsDocument, owns_collection_menu_command, plan_collection_menu_edit,
};
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with the three document-writing Project Group menu items. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CollectionMenuCounters {
    pub(crate) renames: u64,
    pub(crate) colors: u64,
    pub(crate) ungroups: u64,
    /// Payloads whose collection id names nothing the document holds, which write nothing at all.
    pub(crate) refusals: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
    /// Payloads left to the old runtime because this computer's stored key had not been read yet.
    /// A remote machine's tab is not one: it has no stored key to wait for.
    pub(crate) hand_offs: u64,
}

impl GhostexGpuiApp {
    /// Answers a Project Group's Rename, colour or Ungroup. Returns whether it did, in which case
    /// the payload must NOT also reach the old runtime, which would write the document twice.
    pub(crate) fn gx_store_run_collection_menu_edit(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_collection_menu_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.collection_menu.declined_source += 1;
            return false;
        }
        // Every arm of `runNativeCollectionAction` reads `ui.selectedMachineId`: this computer's
        // document is one this app OWNS, a remote machine's one it holds.
        let remote_machine_id = self.gx_store_selected_remote_machine_id();
        // The stored key has to be in hand before an edit lands on top of it, for the same reason
        // a project drop refuses until it is: a rename computed against a document this app has not
        // read would store one folder and drop every other. A remote machine has no stored key.
        if remote_machine_id.is_none() && !self.gx_document_restored::<CollectionsDocument>(cx) {
            self.gx_store.collection_menu.hand_offs += 1;
            return false;
        }
        let (held, _) = self.gx_store_project_documents(remote_machine_id.as_deref());
        let plan = plan_collection_menu_edit(&held, command);
        let action = command
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(document) = plan else {
            self.gx_store.collection_menu.refusals += 1;
            self.gx_store.diagnostics.collection_menu_ran(
                &action,
                false,
                self.gx_store.collection_menu,
            );
            // Answered: the TypeScript's `if (!collection) return` wrote nothing and sent the
            // payload nowhere else either, so handing it on would be a second, different answer.
            return true;
        };
        match action.as_str() {
            "rename" => self.gx_store.collection_menu.renames += 1,
            "ungroup" => self.gx_store.collection_menu.ungroups += 1,
            _ => self.gx_store.collection_menu.colors += 1,
        }
        self.gx_store
            .diagnostics
            .collection_menu_ran(&action, true, self.gx_store.collection_menu);
        match remote_machine_id {
            Some(machine_id) => self.gx_store_send_remote_collections(&machine_id, &document, cx),
            None => self.gx_document_edit::<CollectionsDocument>(document, cx),
        }
        true
    }
}
