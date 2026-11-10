//! K5 and K6 inside the app: the project collections document and the Spaces document.
//!
//! CDXC:Projects 2026-09-21 DECISION:
//! User, 2026-09-19: the desktop app stops running product logic in QuickJS and one Rust store owns
//! projects. For these two documents that decision lands as OWNERSHIP: this app is the only desktop
//! writer of `ghostex.sidebar.projectCollections.v1` and the only caller of
//! `/api/updateSidebarProjectCollections` and `/api/updateSidebarSpaces` for this computer. The
//! Supersedes the placement, not
//! the intent, of `CDXC:Projects 2026-07-18-00:00` and `CDXC:Spaces 2026-08-27`: localStorage is
//! still the instant-edit overlay and the write-through is still debounced with an indefinite retry
//! and an echo guard, all of it now in Rust.
//!
//! **The hand-OFF is gone (2026-09-21).** The page kept an in-memory copy, edited it for the paths
//! the store did not own, and handed the result over; this app handed the held document back after
//! every change so that copy was never a stale base. The page is deleted, so nothing posts
//! `persistProjectCollections` or `persistSidebarSpaces` any more and nothing reads a hand-back.
//!
//! **A REMOTE machine's copies are held, not owned.** `updateRemoteSidebarProjectCollections` and
//! `updateRemoteSidebarSpaces` are direct calls down that machine's tunnel, with no debounce and no
//! guard at all. Since 2026-09-21 this app computes those edits too and sends them straight to the
//! runtime that owns the tunnel (`gx_store/remote_project_docs.rs`), because the page that used to
//! compute them is being deleted and no message it sent could be forwarded in its place.
//!
//! SEE-ALSO: packages/gx-core/src/project_docs/,
//! apps/desktop/src/app/gx_store/client_document.rs.

use ghostex_gx_core::{
    CollectionsDocument, MachineId, ProjectDropLanding, ProjectWrite, SideStateUpdate,
    SpacesDocument, owns_project_move_command, plan_project_move, project_body_drop_command,
    project_drop_command, project_drop_landing,
};
use serde_json::{Value, json};

use super::client_document::{ClientDocument, ClientDocumentHost};
use super::sidebar_drop_queue::DropQueueNeed;
use crate::GhostexGpuiApp;

/// `ghostex.sidebar.projectCollections.v1`.
pub(crate) const COLLECTIONS_STORAGE_KEY: &str = "ghostex.sidebar.projectCollections.v1";

/// What this app run did with the project moves. Memory only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ProjectMoveCounters {
    /// Project-move payloads the store answered.
    pub(crate) moves: u64,
    /// Moves the rules refused (a group that is not there, a Space that is not there, a drop onto
    /// a target that moved out with the source).
    pub(crate) refusals: u64,
    pub(crate) collection_edits: u64,
    pub(crate) space_edits: u64,
    pub(crate) group_orders: u64,
    pub(crate) rename_requests: u64,
    pub(crate) space_editors: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
    /// Payloads handed to the old runtime: a shape this store cannot see the whole of, or a
    /// machine tab whose presentation the store holds nothing for.
    pub(crate) hand_offs: u64,
}

impl ClientDocument for CollectionsDocument {
    const NAME: &'static str = "collections";
    const STORAGE_KEY: Option<&'static str> = Some(COLLECTIONS_STORAGE_KEY);
    const RPC_PATH: &'static str = "/api/updateSidebarProjectCollections";

    fn parse_storage(value: &Value) -> Self {
        Self::from_storage_json(value)
    }

    fn host(app: &mut GhostexGpuiApp) -> &mut ClientDocumentHost<Self> {
        &mut app.gx_store.collections
    }

    fn apply_to_store(
        app: &mut GhostexGpuiApp,
        document: &Self,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) {
        let state = serde_json::from_value(document.to_wire_json()).unwrap_or_default();
        app.gx_store_apply_side_state(SideStateUpdate::ProjectCollections(state), cx);
    }

    fn server_state(app: &GhostexGpuiApp) -> Option<Value> {
        app.gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .and_then(|machine| machine.side_state().project_collections.as_ref())
            .and_then(|state| serde_json::to_value(state).ok())
    }
}

impl ClientDocument for SpacesDocument {
    const NAME: &'static str = "spaces";
    /// None at all: `CDXC:Spaces 2026-08-27` says gxserver owns the whole document, so there has
    /// never been a local copy and creating one here would be a second source of truth.
    const STORAGE_KEY: Option<&'static str> = None;
    const RPC_PATH: &'static str = "/api/updateSidebarSpaces";

    fn parse_storage(_value: &Value) -> Self {
        // Unreachable: `STORAGE_KEY` is `None`, so nothing reads a stored copy.
        Self::default()
    }

    fn host(app: &mut GhostexGpuiApp) -> &mut ClientDocumentHost<Self> {
        &mut app.gx_store.spaces
    }

    fn apply_to_store(
        app: &mut GhostexGpuiApp,
        document: &Self,
        cx: &mut gpui::Context<GhostexGpuiApp>,
    ) {
        let state = serde_json::from_value(document.to_wire_json()).unwrap_or_default();
        app.gx_store_apply_side_state(SideStateUpdate::Spaces(state), cx);
    }

    fn server_state(app: &GhostexGpuiApp) -> Option<Value> {
        app.gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .and_then(|machine| machine.side_state().spaces.as_ref())
            .and_then(|state| serde_json::to_value(state).ok())
    }

    /// CDXC:Spaces 2026-09-28 WHY:
    /// gxserver checks a Spaces document against the Project Groups it holds: a grouped project
    /// loses its direct membership and an unknown group is dropped. The two pushes run on separate
    /// connections, so a Spaces edit made with a Project Groups edit (a new group filed in the
    /// project's Space, a project that left a group keeping its Space, a project dropped on a
    /// Space) could land first, be checked against the old groups, and lose the member it filed.
    ///
    /// SEE-ALSO: server/src/sidebar_spaces.rs (`carry_sidebar_spaces_across_collections`).
    fn push_waits(app: &GhostexGpuiApp) -> bool {
        app.gx_store.collections.sync.is_pending()
    }
}

impl GhostexGpuiApp {
    /// Answers a project move. Returns whether it did, in which case the command must NOT also
    /// reach the old runtime, which would write both documents a second time.
    ///
    /// `moveGroup`, `moveSpace`, `moveToSpace`, `moveToCollection` and `moveCollection` are
    /// RENDERER commands and arrive at the TOP level; `projectMembership` and `spaceMembership`
    /// arrive the same way, because `controller.ts` answers the whole drag family itself. Getting
    /// that backwards is how piece 3d shipped dead with a clean gate, so this entry point reads the
    /// envelope it really gets and the caller passes the top-level payload.
    pub(crate) fn gx_store_run_project_move(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_project_move_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.project_moves.declined_source += 1;
            return false;
        }
        // Neither stored key is this app's to wait for when the gesture is on a REMOTE machine's
        // tab: that machine's documents are held copies of what it published, and the drop queue
        // must never hold a remote drop for a key it will never need.
        // In the one list a project only another machine has is filed in THAT machine's documents,
        // as its own tab would; settings sync then gives every other computer the same
        // (server/src/settings_sync/).
        let merged_remote = self
            .gx_store
            .sidebar_list
            .last_inputs
            .host
            .merge_machines
            .then(|| merged_move_machine(command))
            .flatten();
        let remote_machine_id = self
            .gx_store_selected_remote_machine_id()
            .or_else(|| merged_remote.clone());
        if remote_machine_id.is_none() {
            // Both documents have to be in hand before an edit lands on top of one: a move computed
            // against a document this app has not read yet would write an order over what it cannot
            // see. The Spaces document has no stored key, so it is ready at once; the collections
            // document books its read here and HOLDS the drop until it lands
            // (gx_store/sidebar_drop_queue.rs), because the page that used to perform the refused
            // drop is going and refusing it now would lose the gesture.
            if !self.gx_document_restored::<CollectionsDocument>(cx) {
                if self.gx_store_queue_sidebar_drop(command, DropQueueNeed::ProjectDocuments, cx) {
                    return true;
                }
                self.gx_store.project_moves.declined_source += 1;
                return false;
            }
            // The project order also edits the workspace session groups document, so its stored key
            // has to be in hand too, for the same reason.
            if !self.gx_store_restore_workspace_groups(cx) {
                if self.gx_store_queue_sidebar_drop(command, DropQueueNeed::ProjectDocuments, cx) {
                    return true;
                }
                self.gx_store.project_moves.declined_source += 1;
                return false;
            }
        }
        let (collections, spaces) = self.gx_store_project_documents(remote_machine_id.as_deref());
        let plan = {
            let store = &self.gx_store;
            let mut machine_inputs;
            let inputs = match &merged_remote {
                Some(machine_id) => {
                    machine_inputs = store.sidebar_list.last_inputs.clone();
                    machine_inputs.ui.selected_machine_id = machine_id.clone();
                    machine_inputs.host.merge_machines = false;
                    &machine_inputs
                }
                None => &store.sidebar_list.last_inputs,
            };
            plan_project_move(
                &store.core,
                inputs,
                &collections,
                spaces.as_ref(),
                command,
                super::host::now_ms() as i64,
            )
        };
        let Some(plan) = plan else {
            self.gx_store.project_moves.hand_offs += 1;
            return false;
        };
        self.gx_store.project_moves.moves += 1;
        if plan.writes.is_empty() {
            self.gx_store.project_moves.refusals += 1;
        }
        self.gx_store
            .diagnostics
            .project_move_ran(&plan, self.gx_store.project_moves);
        // A remote gesture's order write was taken out of the plan by `without_group_order`, so it
        // is counted here rather than where it would have been performed.
        if plan.dropped_group_order {
            self.gx_store_note_remote_order_dropped();
        }
        for write in plan.writes {
            self.gx_store_run_project_write(write, remote_machine_id.as_deref(), cx);
        }
        true
    }

    /// The project drop of a project or collection held over a row under project `group_id`'s
    /// header (gx-core `project_body_drop_command`).
    pub(crate) fn gx_store_project_body_drop_command(
        &self,
        moved_kind: &str,
        moved_id: &str,
        group_id: &str,
    ) -> Option<Value> {
        if !self.gx_store_sidebar_list_ready() {
            return None;
        }
        project_body_drop_command(
            self.gx_store.sidebar_list.view(),
            moved_kind,
            moved_id,
            group_id,
        )
    }

    /// Where the row a `moveGroup`, `moveCollection` or `moveToCollection` drag moves would land,
    /// from the same plan the drop performs, for its drop line, with the command as planned (a drop
    /// on a worktree family aims at its parent; gx-core `project_drop_command`), which is the one
    /// to perform. `None` is a drop that does nothing.
    pub(crate) fn gx_store_preview_project_drop(
        &mut self,
        command: &Value,
    ) -> Option<(Value, ProjectDropLanding)> {
        if !self.gx_store_sidebar_list_ready() {
            return None;
        }
        let command = &project_drop_command(self.gx_store.sidebar_list.view(), command);
        let text = |key: &str| command.get(key).and_then(Value::as_str);
        let (kind, id) = match text("type")? {
            "moveGroup" => ("group", text("groupId")?),
            "moveCollection" => ("collection", text("sourceId")?),
            "moveToCollection" => match text("sourceKind")? {
                "group" => ("group", text("sourceId")?),
                _ => ("collection", text("sourceId")?),
            },
            _ => return None,
        };
        let remote_machine_id = self.gx_store_selected_remote_machine_id();
        let (collections, spaces) = self.gx_store_project_documents(remote_machine_id.as_deref());
        let store = &self.gx_store;
        let plan = plan_project_move(
            &store.core,
            &store.sidebar_list.last_inputs,
            &collections,
            spaces.as_ref(),
            command,
            super::host::now_ms() as i64,
        )?;
        project_drop_landing(store.sidebar_list.view(), &collections, &plan, kind, id)
            .map(|landing| (command.clone(), landing))
    }

    /// Whether a `moveToSpace` drop would change which Space holds the project or folder, from the
    /// same plan the drop performs. A drop on the Space that already holds it only moves it to the
    /// front of the order, so that Space is no target worth lighting up.
    pub(crate) fn gx_store_move_to_space_changes_space(&mut self, command: &Value) -> bool {
        if !self.gx_store_sidebar_list_ready() {
            return false;
        }
        let remote_machine_id = self.gx_store_selected_remote_machine_id();
        let (collections, spaces) = self.gx_store_project_documents(remote_machine_id.as_deref());
        let store = &self.gx_store;
        let Some(plan) = plan_project_move(
            &store.core,
            &store.sidebar_list.last_inputs,
            &collections,
            spaces.as_ref(),
            command,
            super::host::now_ms() as i64,
        ) else {
            return false;
        };
        plan.writes.iter().any(|write| {
            matches!(write, ProjectWrite::EditSpaces { document } if Some(document) != spaces.as_ref())
        })
    }

    /// The two documents a gesture is computed against: this app's own for this computer, and the
    /// machine's held copies for a remote tab.
    pub(super) fn gx_store_project_documents(
        &mut self,
        remote_machine_id: Option<&str>,
    ) -> (CollectionsDocument, Option<SpacesDocument>) {
        let Some(machine_id) = remote_machine_id else {
            let store = &self.gx_store;
            return (
                store.collections.sync.document().clone(),
                store
                    .spaces
                    .sync
                    .has_document()
                    .then(|| store.spaces.sync.document().clone()),
            );
        };
        (
            self.gx_store_remote_collections(machine_id),
            self.gx_store_remote_spaces(machine_id),
        )
    }

    fn gx_store_run_project_write(
        &mut self,
        write: ProjectWrite,
        remote_machine_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) {
        match write {
            ProjectWrite::EditCollections { document } => {
                self.gx_store.project_moves.collection_edits += 1;
                match remote_machine_id {
                    Some(machine_id) => {
                        self.gx_store_send_remote_collections(machine_id, &document, cx)
                    }
                    None => self.gx_document_edit::<CollectionsDocument>(document, cx),
                }
            }
            ProjectWrite::EditSpaces { document } => {
                self.gx_store.project_moves.space_edits += 1;
                match remote_machine_id {
                    Some(machine_id) => self.gx_store_send_remote_spaces(machine_id, &document, cx),
                    None => self.gx_document_edit::<SpacesDocument>(document, cx),
                }
            }
            ProjectWrite::GroupOrder { group_ids } => {
                self.gx_store.project_moves.group_orders += 1;
                // The same entry point the renderer's own posts reach, so the project order write
                // has one implementation and not two.
                let message = json!({ "type": "syncGroupOrder", "groupIds": group_ids });
                if !self.gx_store_run_project_order_message(&message, cx) {
                    self.dispatch_native_sidebar_command(message, cx);
                }
            }
            ProjectWrite::RequestCollectionRename { collection_id } => {
                self.gx_store.project_moves.rename_requests += 1;
                // `ui.renameRequest = { collectionId, requestId: Date.now() }`, which the RENDERER
                // consumes: it opens its inline rename on the collection the drop just created. It
                // is held here rather than posted anywhere, because the snapshot carries it and the
                // old projection, which used to carry it, no longer knows the collection exists.
                let request_id = super::host::now_ms();
                self.gx_store.pending_collection_rename = Some((collection_id, request_id));
                self.gx_store_update_sidebar_list(cx);
            }
            ProjectWrite::FollowToSpace { space_id, group_id } => {
                // The Space's own intent rather than a `selectSpace` through the dispatch, which
                // would also restore the Space's last session and focus another project.
                self.gx_store_apply_sidebar_ui_intent(
                    ghostex_gx_core::SidebarUiIntent::SelectSpace { space_id },
                    cx,
                );
                // A remote project's focus attaches one of its sessions, which filing it never did.
                let project = ghostex_gx_core::ProjectKey::parse_sidebar_group_id(&group_id)
                    .filter(|project| project.machine.is_local());
                let already_active = project.as_ref().is_some_and(|project| {
                    self.gx_store.core.focus().active_project.as_ref() == Some(project)
                });
                if project.is_some() && !already_active {
                    self.dispatch_native_sidebar_command(
                        json!({ "type": "focusGroup", "groupId": group_id }),
                        cx,
                    );
                }
            }
            ProjectWrite::OpenSpaceEditor {
                section_key,
                remote_machine_id,
                member_collection_id,
                member_project_id,
            } => {
                self.gx_store.project_moves.space_editors += 1;
                // `openAppModal` from the sidebar page, through the same entry the store's Rename
                // and Note dialogs use. The keys the payload does NOT carry matter: an absent
                // `memberProjectId` is a field `JSON.stringify` drops, where a `null` would be one
                // the dialog has to interpret, which is the absent-versus-null class that has cost
                // this port two findings already.
                let mut open = json!({
                    "type": "open",
                    "modal": "sidebarSpaceEditor",
                    "mode": "create",
                    "sectionKey": section_key,
                });
                if let Some(machine_id) = remote_machine_id {
                    open["remoteMachineId"] = Value::from(machine_id);
                }
                if let Some(collection_id) = member_collection_id {
                    open["memberCollectionId"] = Value::from(collection_id);
                }
                if let Some(project_id) = member_project_id {
                    open["memberProjectId"] = Value::from(project_id);
                }
                self.open_app_modal_from_bridge(open, cx);
            }
        }
    }

    /// The collection a project move just created, for the renderer's inline Rename. Read once:
    /// the renderer remembers the request id it handled, exactly as it does for the publish's.
    pub(crate) fn gx_store_pending_collection_rename(
        &self,
    ) -> Option<crate::app::native_sidebar::model::NativeSidebarRenameRequest> {
        self.gx_store
            .pending_collection_rename
            .as_ref()
            .map(|(collection_id, request_id)| {
                crate::app::native_sidebar::model::NativeSidebarRenameRequest {
                    collection_id: collection_id.clone(),
                    request_id: *request_id,
                }
            })
    }

    /// The daemon's copy of either document just landed in the store. Called from the one place a
    /// change summary reports it, so there is no second copy of this decision.
    pub(crate) fn gx_store_reconcile_project_docs(
        &mut self,
        collections: bool,
        spaces: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if collections {
            let state = CollectionsDocument::server_state(self);
            self.gx_document_reconcile::<CollectionsDocument>(state, cx);
        }
        if spaces {
            let state = SpacesDocument::server_state(self);
            self.gx_document_reconcile::<SpacesDocument>(state, cx);
        }
    }

    /// Books the read of the collections key, whatever else is happening.
    ///
    /// CDXC:Projects 2026-09-21 WHY:
    /// Asked on every pump, the way the prune books the workspace groups key, because nothing else
    /// books it before the daemon's first snapshot: the first call used to come from the reconcile
    /// that snapshot triggers, so the FIRST echo of every run was deferred, and a deferral judged
    /// against a side state the restore had just overwritten spent the collections document's
    /// first-echo push-back on this app's own document. It is one boolean after the read lands.
    pub(crate) fn gx_store_book_project_docs_read(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_document_restored::<CollectionsDocument>(cx);
    }

    /// Both documents on the quit path: the owed storage write, and then the push the daemon has
    /// not got yet.
    ///
    /// The push matters most for the Spaces document, which has no stored key at all, so a 400 ms
    /// debounce is the only thing between the gesture and the daemon; for the collections document
    /// it is the difference between the next launch reading the user's own key and adopting the
    /// daemon's older copy over it.
    pub(crate) fn gx_store_flush_project_docs(&mut self) {
        self.gx_document_flush_write::<CollectionsDocument>();
        self.gx_document_flush_write::<SpacesDocument>();
        self.gx_document_flush_push::<CollectionsDocument>();
        self.gx_document_flush_push::<SpacesDocument>();
    }
}

/// The remote machine a one-list project move belongs to: the machine of the project it moves (or
/// moves onto), when that is not this computer.
fn merged_move_machine(command: &Value) -> Option<String> {
    let project_source = command.get("sourceKind").and_then(Value::as_str) != Some("collection");
    ["groupId", "targetGroupId", "sourceId"]
        .into_iter()
        .filter(|key| *key != "sourceId" || project_source)
        .filter_map(|key| command.get(key).and_then(Value::as_str))
        .filter_map(ghostex_gx_core::ProjectKey::parse_sidebar_group_id)
        .find_map(|project| project.machine.remote_id().map(str::to_string))
}
