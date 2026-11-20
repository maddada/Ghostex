//! What a window had open in a workspace when it left it (the Space, the project and the
//! session), and putting it back when the window returns there. The desktop keeps it with the
//! window's slot (`workspace_windows/window_workspace.rs`), the web build for the page's life
//! (`web_app.rs`); both read and restore it through these.
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User (picked on the 11.12.0 smoke improvements page): switching workspace and back returns to the same Space and the same chat that were open; Personal to Work and back opened the all-projects Space on another project instead of the ★ Space and its chat. The Space is per computer tab, not per workspace, so the other workspace's Space (or a reveal there) replaced it, and the remembered chat was then looked for in a Space that does not draw it. A window now remembers the Space and project per workspace beside the sessions, and re-selects the Space before it picks the session to open.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/space_switch.rs (`plan_workspace_switch_restore`).

use ghostex_gx_core::{SidebarUiIntent, SpaceSwitchFocus, plan_workspace_switch_restore};

use crate::GhostexGpuiApp;

/// Where a window was in a workspace when it left it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WorkspacePlace {
    /// The Space selected on this computer's tab (`other` and `active` included).
    pub(crate) space_id: Option<String>,
    /// The sidebar group id of the project that was active.
    pub(crate) group_id: Option<String>,
    /// The session that was open, as a sidebar session id.
    pub(crate) sidebar_session_id: Option<String>,
}

impl GhostexGpuiApp {
    /// What this window has open now, read before it leaves its workspace.
    pub(crate) fn gx_store_window_workspace_place(&self) -> WorkspacePlace {
        let view = self.gx_store.sidebar_list.view();
        let local_tab = view.selected_machine_id == ghostex_gx_core::LOCAL_MACHINE_ID;
        let focus = self.gx_store.core.focus();
        WorkspacePlace {
            space_id: view
                .spaces
                .iter()
                .find(|space| space.selected)
                .filter(|_| local_tab && view.spaces_enabled)
                .map(|space| space.id.clone()),
            group_id: focus
                .active_project
                .as_ref()
                .filter(|project| project.machine.is_local())
                .map(ghostex_gx_core::ProjectKey::to_sidebar_group_id),
            sidebar_session_id: focus
                .focused_session
                .as_ref()
                .map(ghostex_gx_core::SessionKey::to_sidebar_session_id),
        }
    }

    /// Selects the Space the window had in the workspace it now shows, when that workspace still
    /// has it, so the list the landing picks from is that Space's. Returns whether it moved.
    pub(crate) fn gx_store_return_to_workspace_space(
        &mut self,
        space_id: Option<&str>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(space_id) = space_id else {
            return false;
        };
        let view = self.gx_store.sidebar_list.view();
        if view.selected_machine_id != ghostex_gx_core::LOCAL_MACHINE_ID
            || !view.spaces_enabled
            || !view.spaces.iter().any(|space| space.id == space_id)
            || view
                .spaces
                .iter()
                .any(|space| space.selected && space.id == space_id)
        {
            return false;
        }
        self.gx_store_apply_sidebar_ui_intent(
            SidebarUiIntent::SelectSpace {
                space_id: space_id.to_string(),
            },
            cx,
        )
    }

    /// What the landing opens: the newest remembered session the list draws, else the project the
    /// window had active there, else the workspace's first project; `None` for an empty workspace.
    pub(crate) fn gx_store_workspace_return_focus(
        &self,
        recent_sidebar_session_ids: &[String],
        group_id: Option<&str>,
    ) -> Option<SpaceSwitchFocus> {
        let view = self.gx_store.sidebar_list.view();
        plan_workspace_switch_restore(view, recent_sidebar_session_ids).or_else(|| {
            let remembered = group_id.and_then(|group_id| {
                view.groups
                    .iter()
                    .find(|group| group.core.group_id == group_id)
            });
            remembered
                .or_else(|| view.groups.first())
                .map(|group| SpaceSwitchFocus::Group {
                    group_id: group.core.group_id.clone(),
                })
        })
    }
}
