//! New Group, Rename and Close Group, performed on the one writer of the workspace session groups
//! document instead of in the old runtime's copy of it.
//!
//! The planner is gx-core's (`workspace_groups/group_commands.rs`); this file performs it: the
//! document through `gx_store_edit_workspace_groups` (which writes the stored key, books the push
//! and puts it in the store so the list redraws in this frame), the members of a closed group
//! through the store's own `closeSession` route (a remote member goes down its machine's tunnel
//! there), the limit toast, and the group activation through the runtime's `focusGroup`, which is
//! the focus authority until F3 moves it.
//!
//! **A command made before the stored key has been read is held** in the same queue a drag uses
//! (`sidebar_drop_queue.rs`) and replayed once it has been, because a document edit computed against
//! nothing would overwrite what the user has.
//!
//! SEE-ALSO: packages/gx-core/src/workspace_groups/group_commands.rs.

use ghostex_gx_core::{GroupCommandPlan, MachineId, owns_group_command, plan_group_command};
use serde_json::{Value, json};

use super::super::sidebar_drop_queue::DropQueueNeed;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// A wrapped `createGroup`, `renameGroup` (with a title) or `closeGroup`. Returns whether it was
    /// answered here.
    pub(crate) fn gx_store_run_group_command(
        &mut self,
        command: &Value,
        message: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !owns_group_command(message) {
            return false;
        }
        if !self.gx_store_restore_workspace_groups(cx) {
            // Held until the read lands, then replayed through the same dispatch; a queue that is
            // draining right now cannot take it, and the command is then dropped rather than
            // computed against a document this app has not read.
            if !self.gx_store_queue_sidebar_drop(command, DropQueueNeed::WorkspaceGroups, cx) {
                self.gx_store
                    .workspace_groups
                    .counters
                    .group_commands_dropped += 1;
            }
            return true;
        }
        let plan = {
            let focus = self.gx_store.core.focus();
            plan_group_command(
                self.gx_store.workspace_groups.sync.document(),
                focus.active_project.as_ref(),
                focus.active_group.as_ref(),
                message,
            )
        };
        let Some(plan) = plan else {
            return false;
        };
        self.gx_store.workspace_groups.counters.group_commands += 1;
        match plan {
            GroupCommandPlan::Nothing => {}
            GroupCommandPlan::LimitReached => {
                self.receive_gpui_app_toast_bridge_message(
                    &json!({
                        "level": "info",
                        "title": "Group limit reached for this project.",
                        "type": "toast",
                    }),
                    cx,
                );
            }
            GroupCommandPlan::Create {
                document,
                activate_group_id,
            } => {
                self.gx_store_edit_workspace_groups(document, cx);
                self.dispatch_native_sidebar_command(
                    json!({ "type": "focusGroup", "groupId": activate_group_id }),
                    cx,
                );
            }
            GroupCommandPlan::Rename { document } => {
                self.gx_store_edit_workspace_groups(document, cx);
            }
            GroupCommandPlan::Close {
                member_session_ids,
                document,
                activate_project_group,
            } => {
                // The TypeScript awaited every close before it removed the group. The store's close
                // takes each row away at once (and puts it back if the daemon refuses), so the
                // members are already off the list when the group goes.
                for session_id in member_session_ids {
                    self.dispatch_native_sidebar_command(
                        json!({ "type": "closeSession", "sessionId": session_id }),
                        cx,
                    );
                }
                self.gx_store_edit_workspace_groups(document, cx);
                if let Some(project) = activate_project_group {
                    match project.machine {
                        // `focusGroup` with the project's own group is the runtime's
                        // `activeGroupId = project group` plus the publish.
                        MachineId::Local => self.dispatch_native_sidebar_command(
                            json!({ "type": "focusGroup", "groupId": project.to_sidebar_group_id() }),
                            cx,
                        ),
                        // A remote project's `focusGroup` attaches a session, which closing a
                        // group never did; the store's own active group moves instead.
                        MachineId::Remote(_) => {
                            let output = self.gx_store.core.handle(
                                ghostex_gx_core::Event::Intent(ghostex_gx_core::Intent::FocusProject {
                                    project,
                                }),
                                super::super::host::now_ms(),
                            );
                            if !output.changes.is_empty() {
                                self.gx_store.sidebar_list.note_changes(&output.changes);
                                self.gx_store_update_sidebar_list(cx);
                            }
                        }
                    }
                }
            }
        }
        true
    }
}
