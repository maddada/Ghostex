//! The session moves, performed by the store.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! A drop posts a message and the message writes an order, and both halves live here because the
//! app is where the two meet: gx-core decides what a drag posts (`plan_session_move`) and what a
//! post writes (`plan_order_write`), and this file performs the writes, which are a client-storage
//! document behind the pending-push guard, a daemon call, an activation and a toast.
//!
//! **Two envelopes, and this piece uses both.** `moveSession` is a RENDERER command and arrives at
//! the TOP level, because `controller.ts:160` answers it itself; `createGroupFromSession` is a
//! gxserver message and arrives wrapped as `{ type: 'command', message }`. Unwrapping the wrong one
//! is how the whole dialog port of piece 3d shipped dead while its gate read clean, so each entry
//! point below reads the envelope it really gets.
//!
//! **The counters that prove this path fires in the app** are `moves`, `movePosts`, `moveRefusals`
//! and `orderWrites` on `gxStore.sidebarDrag`, plus `edits` on `gxStore.workspaceGroups`. A run in
//! which the user dragged a row and `moves` is zero means the drop never reached here; a run where
//! `moves` climbs and `movePosts` does not means every drop is being refused.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_drag/,
//! apps/desktop/src/app/gx_store/workspace_groups.rs.

use std::time::Duration;

use ghostex_gx_core::{
    Event, Intent, OrderWrite, ProjectKey, owns_order_write_message, owns_session_move_command,
    plan_order_write, plan_project_order_write, plan_session_move,
};
use serde_json::{Value, json};

use super::rpc::gxserver_rpc_result_task;
use super::sidebar_drop_queue::DropQueueNeed;
use crate::GhostexGpuiApp;

/// `/api/updateSessionOrder` is a plain write; it gets the same timeout the other sidebar calls do.
const ORDER_RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// What this app run did with the moves. Memory only; the record lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarDragCounters {
    /// `moveSession` payloads the store answered.
    pub(crate) moves: u64,
    /// Messages those payloads posted. A move the rules refuse posts none.
    pub(crate) move_posts: u64,
    /// Moves the rules refused (a pinned row onto an unpinned one, a remote group, a row the
    /// projection does not list, the sort mode not being Manual).
    pub(crate) move_refusals: u64,
    /// Order messages the store answered, its own and the menus'.
    pub(crate) order_writes: u64,
    pub(crate) document_edits: u64,
    pub(crate) session_order_calls: u64,
    pub(crate) session_order_failures: u64,
    pub(crate) activations: u64,
    pub(crate) toasts: u64,
    /// Payloads the store owns but did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers a `moveSession` drop. Returns whether it did, in which case the command must NOT
    /// also reach the old runtime, which would write the order a second time.
    pub(crate) fn gx_store_run_sidebar_session_move(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_session_move_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_drag.declined_source += 1;
            return false;
        }
        // A move edits the document, so the stored key has to be in hand first. A read that has
        // not landed HOLDS the drop until it does (gx_store/sidebar_drop_queue.rs) instead of
        // writing an order over a document this app cannot see: the page that used to perform the
        // refused drop is going, so refusing it now would lose the gesture. A queue that cannot
        // take it declines as before and the old runtime performs it.
        if !self.gx_store_restore_workspace_groups(cx) {
            if self.gx_store_queue_sidebar_drop(command, DropQueueNeed::WorkspaceGroups, cx) {
                return true;
            }
            self.gx_store.sidebar_drag.declined_source += 1;
            return false;
        }
        // A browser row on either end and a malformed payload are refused inside the planner, each
        // with the reason written down there.
        let plan = {
            let store = &self.gx_store;
            plan_session_move(&store.core, &store.sidebar_list.last_inputs, command)
        };
        let Some(plan) = plan else {
            return false;
        };
        self.gx_store.sidebar_drag.moves += 1;
        if plan.messages.is_empty() {
            self.gx_store.sidebar_drag.move_refusals += 1;
        }
        self.gx_store
            .diagnostics
            .sidebar_move_ran(&plan, self.gx_store.sidebar_drag);
        for message in plan.messages.clone() {
            self.gx_store.sidebar_drag.move_posts += 1;
            // The messages go through the same entry point the renderer's own posts reach, so the
            // order write has one implementation and not two.
            if !self.gx_store_run_sidebar_order_write_message(&message, cx) {
                self.dispatch_native_sidebar_command(message, cx);
            }
        }
        true
    }

    /// Answers `createGroupFromSession`, which arrives in the gxserver command envelope.
    pub(crate) fn gx_store_run_sidebar_order_write(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if command.get("type").and_then(Value::as_str) != Some("command") {
            return false;
        }
        let Some(message) = command.get("message").cloned() else {
            return false;
        };
        if !owns_order_write_message(&message) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_drag.declined_source += 1;
            return false;
        }
        // Held until the stored key lands, for the reason above.
        if !self.gx_store_restore_workspace_groups(cx) {
            if self.gx_store_queue_sidebar_drop(command, DropQueueNeed::WorkspaceGroups, cx) {
                return true;
            }
            self.gx_store.sidebar_drag.declined_source += 1;
            return false;
        }
        self.gx_store_run_sidebar_order_write_message(&message, cx)
    }

    /// One order message. Returns whether the store answered it; `false` means the old runtime
    /// must, which is what a remote row needs.
    fn gx_store_run_sidebar_order_write_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let plan = {
            let document = self.gx_store.workspace_groups.sync.document();
            plan_order_write(document, message)
        };
        let Some(plan) = plan else {
            return false;
        };
        self.gx_store.sidebar_drag.order_writes += 1;
        self.gx_store.diagnostics.sidebar_order_write_ran(
            message,
            &plan,
            self.gx_store.sidebar_drag,
        );
        for write in plan.writes {
            match write {
                OrderWrite::EditDocument { document } => {
                    self.gx_store.sidebar_drag.document_edits += 1;
                    self.gx_store_edit_workspace_groups(document, cx);
                }
                OrderWrite::SessionOrderCall {
                    project,
                    session_ids,
                } => self.gx_store_call_session_order(project, session_ids, cx),
                OrderWrite::ActivateSubgroup { project, group_id } => {
                    self.gx_store.sidebar_drag.activations += 1;
                    // A remote project's group is also the runtime's `activeGroupId`, which its
                    // remote tab list is read from (gx-core order_write.rs, CDXC:Sessions 2026-09-25).
                    let remote_group = (!project.machine.is_local())
                        .then(|| ghostex_gx_core::encode_workspace_subgroup_id(&project, &group_id));
                    let output = self.gx_store.core.handle(
                        Event::Intent(Intent::FocusSubgroup { project, group_id }),
                        super::host::now_ms(),
                    );
                    if !output.changes.is_empty() {
                        self.gx_store.sidebar_list.note_changes(&output.changes);
                        self.gx_store_update_sidebar_list(cx);
                    }
                    if let Some(group_id) = remote_group {
                        self.dispatch_native_sidebar_command(
                            json!({ "type": "focusGroup", "groupId": group_id }),
                            cx,
                        );
                    }
                }
                OrderWrite::RemoteProjectOrder { machine_id, state } => {
                    // `requestRemoteGxserver`'s default timeout; the machine's own stream brings
                    // the order back into the store, as it brought it to the runtime's copy.
                    self.start_gpui_remote_sidebar_rpc(
                        &machine_id,
                        "/api/updateWorkspaceSessionGroups",
                        Some(json!({ "state": state })),
                        std::time::Duration::from_secs(20),
                        crate::app::remote_conn::sidebar_rpc::GpuiRemoteSidebarRpcMode::Awaited,
                        cx,
                    )
                    .detach();
                }
                OrderWrite::Toast { level, title } => {
                    self.gx_store.sidebar_drag.toasts += 1;
                    self.receive_gpui_app_toast_bridge_message(
                        &json!({ "level": level.as_str(), "title": title, "type": "toast" }),
                        cx,
                    );
                }
            }
        }
        true
    }

    /// Answers the `syncGroupOrder` a project move posts, which writes the project order and each
    /// project's user-made group order into the workspace session groups document.
    ///
    /// A DIFFERENT writer from `syncSessionOrder` above, and kept apart for that reason: this one
    /// needs the projection's worktree metadata, and it is the one place in the project moves where
    /// an unchanged order really does write nothing, because `syncWorkspaceGroupOrder` returns on
    /// `this.workspaceGroups === before`.
    pub(crate) fn gx_store_run_project_order_message(
        &mut self,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let plan = {
            let store = &self.gx_store;
            plan_project_order_write(
                &store.core,
                &store.sidebar_list.last_inputs,
                store.workspace_groups.sync.document(),
                message,
            )
        };
        let Some(plan) = plan else {
            return false;
        };
        self.gx_store.sidebar_drag.order_writes += 1;
        self.gx_store.diagnostics.sidebar_order_write_ran(
            message,
            &plan,
            self.gx_store.sidebar_drag,
        );
        for write in plan.writes {
            if let OrderWrite::EditDocument { document } = write {
                self.gx_store.sidebar_drag.document_edits += 1;
                self.gx_store_edit_workspace_groups(document, cx);
            }
        }
        true
    }

    /// The manual order of a PROJECT's sessions, which the daemon owns. The local rows are written
    /// first so the list moves under the user's finger, exactly as
    /// `reorderPresentationProjectSessions` does before the call is awaited.
    fn gx_store_call_session_order(
        &mut self,
        project: ProjectKey,
        session_ids: Vec<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.gx_store.sidebar_drag.session_order_calls += 1;
        let output = self.gx_store.core.handle(
            Event::Intent(Intent::ReorderProjectSessions {
                project: project.clone(),
                session_ids: session_ids.clone(),
            }),
            super::host::now_ms(),
        );
        if !output.changes.is_empty() {
            self.gx_store.sidebar_list.note_changes(&output.changes);
            self.gx_store_update_sidebar_list(cx);
        }
        let params = json!({ "projectId": project.project_id, "sessionIds": session_ids });
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = gxserver_rpc_result_task(
                &background,
                "/api/updateSessionOrder",
                params,
                ORDER_RPC_TIMEOUT,
            )
            .await;
            let _ = this.update(cx, |this, _| {
                if result.is_err() {
                    this.gx_store.sidebar_drag.session_order_failures += 1;
                }
            });
        })
        .detach();
    }
}
