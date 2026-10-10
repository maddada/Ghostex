//! The workspace tile's menu (left of the Spaces row) and a project's "Move to workspace" submenu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User (mockups 01 and 07): the workspace tile's menu lists the workspaces (the current one
//! checked), "Workspace settings…" and "New workspace…"; right-click a project → "Move to
//! workspace ▸" lists the other workspaces and "New workspace…", next to the Work mode switch.
//! User, later the same day: "remove this window item, instead add a child button on the right side
//! of workspaces that aren't currently the selected one AND don't already have a window open. When
//! that one is clicked then we open that in a new window". This supersedes the "Open <name> in a
//! new window" row.

use ghostex_gx_protocol::{SidebarWorkspace, SidebarWorkspacesState};
use serde_json::json;

use super::commands::MenuCommand;
use super::item::{MenuItem, MenuSecondary};

/// The workspace tile's menu for a window showing `current`; `in_windows` are the workspaces some
/// window already shows, which get no new-window button.
pub fn workspace_menu(
    state: &SidebarWorkspacesState,
    current: &str,
    in_windows: &[String],
) -> Vec<MenuItem> {
    let mut menu = vec![MenuItem::heading("Workspaces")];
    for workspace in state.ordered() {
        let mut row = MenuItem::row(
            &workspace.name,
            if workspace.is_work() {
                "briefcase"
            } else {
                "user"
            },
            MenuCommand::host(json!({
                "type": "selectWorkspace",
                "workspaceId": workspace.workspace_id,
            })),
        )
        .with_checked(workspace.workspace_id == current);
        row.icon_color = Some(workspace.color.clone());
        if workspace.workspace_id != current && !in_windows.contains(&workspace.workspace_id) {
            row.secondary = Some(MenuSecondary {
                icon: "external-link".to_string(),
                label: String::new(),
                command: MenuCommand::host(json!({
                    "type": "openWorkspaceWindow",
                    "workspaceId": workspace.workspace_id,
                })),
            });
        }
        menu.push(row);
    }
    menu.push(MenuItem::separator());
    menu.push(MenuItem::row(
        "Workspace settings…",
        "settings",
        MenuCommand::host(json!({ "type": "openWorkspaceSettings", "workspaceId": current })),
    ));
    menu.push(MenuItem::row(
        "New workspace…",
        "plus",
        MenuCommand::host(json!({ "type": "newWorkspace" })),
    ));
    menu
}

/// Where the workspace tile's one click goes from `current`: with two workspaces the other one;
/// with more, the one this window showed most recently before (`recent`, newest first), else the
/// next one in order. `None` with a single workspace.
///
/// CDXC:Workspaces 2026-10-10 DECISION:
/// User: "i want 1 click to switch workspaces between work and personal one in my case (but still have access to the dropdown we have now basically) ... when i click the left side of the split button then it toggles betwen the available workspaces right side shows the dropdown we have now". With three or more workspaces the click goes back to the one the window used last. The split button's look is in the desktop's `native_sidebar/workspace_tile.rs`.
pub fn workspace_switch_target<'a>(
    state: &'a SidebarWorkspacesState,
    current: &str,
    recent: &[String],
) -> Option<&'a SidebarWorkspace> {
    if let Some(workspace) = recent
        .iter()
        .filter(|id| id.as_str() != current)
        .find_map(|id| state.workspaces.get(id))
    {
        return Some(workspace);
    }
    let ordered: Vec<&SidebarWorkspace> = state.ordered().collect();
    let position = ordered
        .iter()
        .position(|workspace| workspace.workspace_id == current);
    let start = position.map_or(0, |index| index + 1);
    (0..ordered.len())
        .map(|offset| ordered[(start + offset) % ordered.len()])
        .find(|workspace| workspace.workspace_id != current)
}

/// "Move to workspace ▸" for a project in `project_workspace_id`: every other workspace, then
/// "New workspace…", which creates one and moves the project into it.
pub(crate) fn move_to_workspace_menu(
    state: &SidebarWorkspacesState,
    project_id: &str,
    project_workspace_id: &str,
) -> MenuItem {
    let mut children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != project_workspace_id)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                if workspace.is_work() {
                    "briefcase"
                } else {
                    "user"
                },
                MenuCommand::command(json!({
                    "type": "moveProjectToWorkspace",
                    "projectId": project_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.icon_color = Some(workspace.color.clone());
            row
        })
        .collect();
    if !children.is_empty() {
        children.push(MenuItem::separator());
    }
    children.push(MenuItem::row(
        "New workspace…",
        "plus",
        MenuCommand::host(json!({ "type": "newWorkspace", "moveProjectId": project_id })),
    ));
    MenuItem::submenu("Move to workspace", "switch-horizontal", children)
}

/// "Move to workspace ▸" on a remote machine's tab: every workspace of this computer other than
/// the one the tab shows in now (`SidebarWorkspacesState::machine_workspace`).
pub fn machine_workspace_menu(state: &SidebarWorkspacesState, machine_id: &str) -> MenuItem {
    let current = state.machine_workspace(machine_id);
    let children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != current)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                if workspace.is_work() {
                    "briefcase"
                } else {
                    "user"
                },
                MenuCommand::host(json!({
                    "type": "moveMachineToWorkspace",
                    "machineId": machine_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.icon_color = Some(workspace.color.clone());
            row
        })
        .collect();
    MenuItem::submenu("Move to workspace", "switch-horizontal", children)
}
