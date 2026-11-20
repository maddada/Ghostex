//! The workspace tile's menu (the first tile of the Spaces row) and a project's "Move to Workspace"
//! submenu.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User (mockups 01 and 07): the workspace tile's menu lists the workspaces (the current one
//! checked), "Workspace settings…" and "New workspace…"; right-click a project → "Move to
//! workspace ▸" lists the other workspaces and "New workspace…", next to the Work mode switch.
//! User, later the same day: "remove this window item, instead add a child button on the right side
//! of workspaces that aren't currently the selected one AND don't already have a window open. When
//! that one is clicked then we open that in a new window". This supersedes the "Open <name> in a
//! new window" row.
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User picked "A. The name, quietly" (Q4254): the menu's workspace rows show the workspace's
//! colour as a small dot instead of the person and briefcase icons, and each row names its Go to
//! Workspace hotkey (⌃⌥1, ⌃⌥2…); the new-window button stays.
//!
//! CDXC:Workspaces 2026-11-20 DECISION:
//! User picked "F. The first tile of the Spaces row" (Q4296), with an icon per workspace: every
//! workspace row here (the tile's menu and each "Move to Workspace" list) shows the workspace's
//! icon on a disc in its colour (`badge`), the same mark as the tile. Supersedes the dot above.

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
    for (index, workspace) in state.ordered().enumerate() {
        let mut row = MenuItem {
            label: Some(workspace.name.clone()),
            icon: Some(workspace.icon_id().to_string()),
            color: Some(workspace.color.clone()),
            badge: true,
            command: Some(MenuCommand::host(json!({
                "type": "selectWorkspace",
                "workspaceId": workspace.workspace_id,
            }))),
            ..MenuItem::default()
        }
        .with_checked(workspace.workspace_id == current);
        // Go to Workspace 1 to 9 (packages/settings-catalog hotkey_definitions.rs); the desktop
        // writes the binding as the row's hint when the menu opens.
        if index < 9 {
            row = row.with_hotkey(&format!("goToWorkspace{}", index + 1));
        }
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

/// "Move to Workspace ▸" for a project in `project_workspace_id`: every other workspace, then
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
                workspace.icon_id(),
                MenuCommand::command(json!({
                    "type": "moveProjectToWorkspace",
                    "projectId": project_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.color = Some(workspace.color.clone());
            row.badge = true;
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
    MenuItem::submenu("Move to Workspace", "switch-horizontal", children)
}

/// "Move to Workspace ▸" for a remote machine's project in `project_workspace_id` (this
/// computer's workspace it shows in): every other workspace of this computer.
pub(crate) fn move_remote_project_to_workspace_menu(
    state: &SidebarWorkspacesState,
    machine_id: &str,
    project_id: &str,
    project_workspace_id: &str,
) -> MenuItem {
    let children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != project_workspace_id)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                workspace.icon_id(),
                MenuCommand::host(json!({
                    "type": "moveRemoteProjectToWorkspace",
                    "machineId": machine_id,
                    "projectId": project_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.color = Some(workspace.color.clone());
            row.badge = true;
            row
        })
        .collect();
    MenuItem::submenu("Move to Workspace", "switch-horizontal", children)
}

/// "Move to Workspace ▸" on a remote machine's tab: every workspace of this computer other than
/// the one the tab shows in now (`SidebarWorkspacesState::machine_workspace`).
pub fn machine_workspace_menu(state: &SidebarWorkspacesState, machine_id: &str) -> MenuItem {
    let current = state.machine_workspace(machine_id);
    let children: Vec<MenuItem> = state
        .ordered()
        .filter(|workspace| workspace.workspace_id != current)
        .map(|workspace| {
            let mut row = MenuItem::row(
                &workspace.name,
                workspace.icon_id(),
                MenuCommand::host(json!({
                    "type": "moveMachineToWorkspace",
                    "machineId": machine_id,
                    "workspaceId": workspace.workspace_id,
                })),
            );
            row.color = Some(workspace.color.clone());
            row.badge = true;
            row
        })
        .collect();
    MenuItem::submenu("Move to Workspace", "switch-horizontal", children)
}
