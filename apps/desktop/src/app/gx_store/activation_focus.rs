//! Clicks outside the sidebar that land on a session or a group: the status item and the pet, a
//! menu bar session row, a Quick Access or command palette session row, a palette Action run, and
//! a Back or Forward stop.
//!
//! CDXC:StatusPet 2026-09-25 WHY:
//! Each of these used to reach the app runtime through a callback of its own
//! (`onStatusPetActivation`, `onMenuBarSessionActivation`, `onCommandPaletteSessionFocus`,
//! `onCommandPaletteRunSidebarCommand`), and every one of those only re-entered the runtime's
//! `focusSession` or `runSidebarCommand`. The ids are shaped here and handed to the store's one
//! focus and Action run (`focus_perform.rs`, `sidebar_command_run.rs`), the same ones a sidebar
//! row click and a project row's Actions button end in. The bounded-id rules the callbacks
//! enforced (StatusPet 2026-06-26) are enforced before this point by `status_pet.rs` and here.
//!
//! SEE-ALSO: apps/desktop/src/app/status_pet.rs (the dispatchers),
//! apps/desktop/src/app/gx_store/focus_perform.rs.

use ghostex_gx_core::{ProjectKey, SessionKey};
use serde_json::json;

use super::focus_perform::RowFocusOptions;
use crate::GhostexGpuiApp;

impl GhostexGpuiApp {
    /// Focuses one session the way a sidebar row click does. Returns whether the id named
    /// something a focus can take.
    pub(crate) fn gx_store_focus_activated_session(
        &mut self,
        session_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if session_id.is_empty() {
            return false;
        }
        // A focus starts from the store's newest selection (gx_store/burst.rs).
        self.gx_store_flush_local_selection(cx);
        self.gx_store_focus_session_row(session_id, RowFocusOptions::default(), cx)
    }

    /// Focuses one sidebar group (a project, or a user-made group in one) the way a header click
    /// does. Returns whether the id named a group a focus can take.
    pub(crate) fn gx_store_focus_activated_group(
        &mut self,
        group_id: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if group_id.is_empty() {
            return false;
        }
        self.gx_store_flush_local_selection(cx);
        self.gx_store_focus_group_row(group_id, cx)
    }

    /// The Back/Forward stop the sidebar is on (navigation_history/controller.rs).
    pub(crate) fn gx_store_navigation_entry(
        &self,
    ) -> Option<ghostex_gx_core::navigation_history::NavigationHistoryEntry> {
        ghostex_gx_core::navigation_history::sidebar_navigation_entry(
            self.gx_store.sidebar_list.model(),
            self.gx_store.core.focus(),
        )
    }

    /// Runs one saved Action by id, with the scope that picks its list. Returns `true`: the run is
    /// answered here whatever it finds.
    pub(crate) fn gx_store_run_activated_sidebar_command(
        &mut self,
        command_id: &str,
        run_mode: Option<&str>,
        scope: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let mut message = json!({ "type": "runSidebarCommand", "commandId": command_id.trim() });
        if let Some(run_mode) = run_mode {
            message["runMode"] = json!(run_mode);
        }
        if let Some(scope) = scope {
            message["scope"] = json!(scope);
        }
        // The run can move the project, which starts from the store's newest selection.
        self.gx_store_flush_local_selection(cx);
        self.gx_store_run_sidebar_command(&message, cx);
        true
    }
}

/// The sidebar id a menu bar session row focuses. A row carries its project id and either a
/// sidebar id already or the daemon's raw session id, which is scoped to its project here.
pub(crate) fn menu_bar_session_focus_id(project_id: &str, session_id: &str) -> String {
    if SessionKey::parse_sidebar_session_id(session_id).is_some() {
        return session_id.to_string();
    }
    match ProjectKey::parse_workspace_project_id(project_id) {
        Some(ProjectKey {
            machine: ghostex_gx_core::MachineId::Remote(machine_id),
            project_id,
        }) => SessionKey::remote(machine_id, project_id, session_id).to_sidebar_session_id(),
        _ => SessionKey::local(project_id, session_id).to_sidebar_session_id(),
    }
}

/// A palette session row names a projected sidebar id: a local `combined-session:` id or a remote
/// one. A raw daemon id is not routable from the palette and is refused.
pub(crate) fn palette_session_focus_id(session_id: &str) -> Option<&str> {
    let session_id = session_id.trim();
    SessionKey::parse_sidebar_session_id(session_id).map(|_| session_id)
}
