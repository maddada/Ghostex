//! Close Project's successor: the session the store names for the close to focus before it parks
//! the active project.
//!
//! CDXC:Projects 2026-09-21 WHY:
//! The store posted `closeWorkspaceProjectForGroup` bare and the sidebar page filled in
//! `successorSessionId` on its way past (the deleted sidebar page's `post`, which was the only
//! thing that ever set the field on the desktop). With that page going, a direct route to the
//! runtime would have carried no successor at all, and closing the active project would have left
//! the user with no focused session, which is exactly what `CDXC:Projects 2026-09-16 DECISION`
//! exists to prevent. The store fills the field in here instead, from the list it draws
//! (gx-core `sidebar_view/close_successor.rs`), and the page is told not to compute a second one:
//! `controller.ts` now forwards a message that already carries the field.
//!
//! **Only while the list is ready.** In the launch window there is no drawn list to take a
//! successor from, and `dispatch_native_sidebar_ui` drops the command before it gets here.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_view/close_successor.rs,
//! the deleted React sidebar's `close-project-successor.ts`,
//! packages/gx-core/examples/close_project_successor.rs.

use ghostex_gx_core::close_project_group_is_active;
use serde_json::Value;

use crate::GhostexGpuiApp;

/// What this app run did with Close Project. Rides `gxStore.sidebarActions.summary` as
/// `closeProject`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CloseProjectCounters {
    /// Closes of the ACTIVE project, which are the only ones that name a successor.
    pub(crate) active_closes: u64,
    /// Of those, the ones a successor was found for.
    pub(crate) successors: u64,
    /// Of those, the ones with no awake row anywhere else in the drawn list.
    pub(crate) no_successor: u64,
    /// Closes the store did not answer because the renderer is not drawing its list.
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Adds `successorSessionId` to a `closeWorkspaceProjectForGroup` for the ACTIVE project, and
    /// returns the command to send on. Every other command is returned untouched.
    pub(crate) fn gx_store_add_close_project_successor(&mut self, command: Value) -> Value {
        let Some(group_id) = close_project_group_id(&command) else {
            return command;
        };
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.close_project.declined_source += 1;
            return command;
        }
        // `sidebarStore.getState().groupsById[message.groupId]?.isActive`: a close of a project the
        // user is not in moves no focus, so the page named no successor either.
        let active_group = self.gx_store.core.focus().active_group.clone();
        if !close_project_group_is_active(active_group.as_ref(), &group_id) {
            return command;
        }
        self.gx_store.close_project.active_closes += 1;
        let successor = self
            .gx_store
            .sidebar_list
            .close_project_successor_session_id(&group_id);
        let Some(successor) = successor else {
            self.gx_store.close_project.no_successor += 1;
            return command;
        };
        self.gx_store.close_project.successors += 1;
        let mut command = command;
        command["message"]["successorSessionId"] = Value::from(successor);
        command
    }

    /// The counters, for the periodic summary in `sidebar_remote.rs`.
    pub(super) fn gx_store_close_project_counters(&self) -> CloseProjectCounters {
        self.gx_store.close_project
    }
}

/// The group a wrapped `closeWorkspaceProjectForGroup` names, and only when the field it would be
/// given is not already there: the command is a gxserver message and arrives WRAPPED.
fn close_project_group_id(command: &Value) -> Option<String> {
    if command.get("type").and_then(Value::as_str) != Some("command") {
        return None;
    }
    let message = command.get("message")?;
    if message.get("type").and_then(Value::as_str) != Some("closeWorkspaceProjectForGroup") {
        return None;
    }
    if message.get("successorSessionId").is_some() {
        return None;
    }
    Some(message.get("groupId")?.as_str()?.to_string())
}

/// The counters as the summary carries them: 4 keys at depth 2.
pub(super) fn close_project_counters_json(counters: &CloseProjectCounters) -> Value {
    serde_json::json!({
        "activeCloses": counters.active_closes,
        "successors": counters.successors,
        "noSuccessor": counters.no_successor,
        "declinedSource": counters.declined_source,
    })
}
